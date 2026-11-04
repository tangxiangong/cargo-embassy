use crate::{
    chip::Family,
    config::{Catalog, Project},
};
use anyhow::{Context, Result, ensure};
use esp_generate::{
    manifest::Emit,
    plugin,
    process::{self, FactValue},
};
use std::{collections::BTreeMap, fs, io::Write, path::PathBuf};

pub type Files = BTreeMap<String, String>;

pub fn plan(catalog: &Catalog, project: &Project) -> Result<Files> {
    project.validate()?;
    let files = if project.chip.family == Family::Esp {
        esp(catalog, project)?
    } else {
        crate::cortex::plan(project)?
    };
    for path in files.keys() {
        ensure!(
            process::is_safe_relative_path(path),
            crate::tr!("Unsafe template path: {path}", "模板路径不安全：{path}")
        );
    }
    for (name, text) in &files {
        if name.ends_with(".toml") {
            toml::from_str::<toml::Value>(text).with_context(|| {
                crate::tr!("Invalid generated TOML: {name}", "生成的 TOML 无效：{name}")
            })?;
        }
        if name.ends_with(".json") {
            json5::from_str::<serde_json::Value>(text).with_context(|| {
                crate::tr!("Invalid generated JSON: {name}", "生成的 JSON 无效：{name}")
            })?;
        }
    }
    Ok(files)
}

fn esp(catalog: &Catalog, p: &Project) -> Result<Files> {
    let loaded = &catalog.esp;
    let selected = p.selected();
    let selection = plugin::selection(selected.clone(), &p.config.flat_options);
    let groups: Vec<_> = selection.groups.keys().cloned().collect();
    let mut facts = loaded
        .resolved
        .facts(&selection)
        .map_err(anyhow::Error::msg)?;
    let xtensa = matches!(facts.field("chip", "xtensa"), Some(FactValue::Bool(true)));
    let toolchain = if !p.toolchain.is_empty() {
        &p.toolchain
    } else if xtensa {
        "esp"
    } else if p.has("stack-smashing-protection") {
        "nightly"
    } else {
        "stable"
    };
    facts.set_value("project_name", p.name.clone());
    facts.set_value(
        "generate_version",
        format!(
            "cargo-embassy {} / embedded ESP templates",
            env!("CARGO_PKG_VERSION")
        ),
    );
    facts.set_value(
        "generate_parameters",
        selected
            .iter()
            .map(|s| format!("-o {s}"))
            .collect::<Vec<_>>()
            .join(" "),
    );
    facts.set_value("rust_toolchain", toolchain);
    let source_manifest = loaded
        .source
        .read("Cargo.toml")
        .map_err(anyhow::Error::msg)?;
    let source_manifest = esp_generate::cargo::CargoToml::load(&source_manifest)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let hal_version = source_manifest.dependency_version("esp-hal");
    facts.set_value(
        "esp_hal_version_full",
        hal_version.trim_start_matches(|c: char| !c.is_ascii_digit()),
    );
    for value in &p.config.flat_options {
        if p.has(&value.name) {
            for (key, value) in &value.sets {
                if let Some(value) = value.as_scalar() {
                    facts.set_value(key.clone(), value);
                }
            }
        }
    }
    let renderer = process::Renderer::new(&selected, &groups, &facts);
    let mut files = Files::new();
    for (source, text) in loaded.source.files().map_err(anyhow::Error::msg)? {
        let Emit::When { condition, output } = loaded.manifest.emit(&source) else {
            continue;
        };
        if let Some(condition) = condition
            && !renderer.evaluate(condition, &source)?
        {
            continue;
        }
        let path = if let Some(output) = output {
            renderer.output_path(output, &source)?
        } else {
            source.clone()
        };
        let text = renderer.render(&text, &mut |path| {
            loaded.source.read(path).map(|s| s.into_owned())
        })?;
        ensure!(
            files.insert(path, text).is_none(),
            crate::tr!(
                "Duplicate template output: {source}",
                "模板生成了重复的输出文件：{source}"
            )
        );
    }
    Ok(files)
}

pub fn write(p: &Project, files: &Files) -> Result<PathBuf> {
    p.validate()?;
    let parent = PathBuf::from(&p.output);
    ensure!(
        parent.is_dir(),
        crate::tr!(
            "Output parent must already exist: {}",
            "输出父目录必须已存在：{}",
            parent.display()
        )
    );
    let destination = parent.join(&p.name);
    // Stage all writes; reserve the destination exclusively so another project is never overwritten.
    let staging = tempfile::tempdir_in(&parent)?;
    for (path, text) in files {
        ensure!(
            process::is_safe_relative_path(path),
            crate::tr!("Unsafe output path: {path}", "输出路径不安全：{path}")
        );
        let path = staging.path().join(path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        file.write_all(text.as_bytes())?;
    }
    let rust_files: Vec<_> = files
        .keys()
        .filter(|path| path.ends_with(".rs"))
        .map(|path| staging.path().join(path))
        .collect();
    if !rust_files.is_empty() {
        let output = std::process::Command::new("rustfmt")
            .args(["--edition", "2024"])
            .args(&rust_files)
            .output()
            .context(crate::tr!(
                "rustfmt is required to format generated Rust sources",
                "格式化生成的 Rust 源码需要安装 rustfmt"
            ))?;
        ensure!(
            output.status.success(),
            crate::tr!(
                "Generated Rust failed formatting: {}",
                "生成的 Rust 源码格式化失败：{}",
                String::from_utf8_lossy(&output.stderr)
            )
        );
    }
    fs::create_dir(&destination).context(crate::tr!(
        "Cannot reserve destination; nothing was overwritten",
        "无法创建目标目录；未覆盖已有文件"
    ))?;
    let result = (|| -> Result<()> {
        for entry in fs::read_dir(staging.path())? {
            let entry = entry?;
            fs::rename(entry.path(), destination.join(entry.file_name()))?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&destination);
    }
    result?;
    Ok(destination)
}
