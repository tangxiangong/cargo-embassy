use crate::chip::get_target_by_name;
use crate::{chip::Family, config::Project, generate::Files};
use anyhow::{Context, Result, ensure};
use serde_json::json;

fn dep(manifest: &mut String, name: &str, version: &str, features: &[&str]) {
    manifest.push_str(&format!(
        "{name} = {{ version = {version:?}, features = {features:?} }}\n"
    ));
}

pub fn plan(p: &Project) -> Result<Files> {
    let mut files = Files::new();
    let sd = ["s112", "s113", "s122", "s132", "s140"]
        .into_iter()
        .find(|s| p.has(s));
    let hal = if p.chip.family == Family::Stm32 {
        "embassy-stm32"
    } else {
        "embassy-nrf"
    };
    let log = if p.has("defmt") {
        Some("defmt")
    } else if p.has("log") {
        Some("log")
    } else {
        None
    };
    let mut manifest = format!(
        "[package]\nname = {:?}\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[[bin]]\nname = {:?}\npath = \"src/main.rs\"\ntest = false\nbench = false\n\n[dependencies]\n",
        p.name, p.name
    );
    let mut executor_features = vec!["platform-cortex-m", "executor-thread"];
    if let Some(log) = log {
        executor_features.push(log);
    }
    dep(
        &mut manifest,
        "embassy-executor",
        "0.10",
        &executor_features,
    );
    dep(
        &mut manifest,
        "embassy-time",
        "0.5",
        &log.into_iter().collect::<Vec<_>>(),
    );
    let mut hal_features = vec![p.chip.hal_feature.as_str()];
    hal_features.extend(if p.chip.family == Family::Stm32 {
        vec!["memory-x", "time-driver-any", "exti"]
    } else {
        vec!["gpiote", "time-driver-rtc1"]
    });
    if p.has("defmt") {
        hal_features.push("defmt");
    }
    dep(
        &mut manifest,
        hal,
        if p.chip.family == Family::Stm32 {
            "0.6"
        } else {
            "0.11"
        },
        &hal_features,
    );
    dep(
        &mut manifest,
        "cortex-m",
        "0.7",
        if sd.is_some() {
            &["inline-asm"]
        } else {
            &["inline-asm", "critical-section-single-core"]
        },
    );
    dep(&mut manifest, "cortex-m-rt", "0.7", &[]);
    let mut imports = String::new();
    let mut init = String::new();
    let mut tick = String::new();
    if let Some(log) = log {
        dep(
            &mut manifest,
            log,
            if log == "defmt" { "1" } else { "0.4" },
            &[],
        );
        dep(&mut manifest, "rtt-target", "0.6.2", &[log]);
        imports.push_str(&format!("use {log}::info;\n"));
        init.push_str(if log == "defmt" {
            "    rtt_target::rtt_init_defmt!();\n"
        } else {
            "    rtt_target::rtt_init_log!(log::LevelFilter::Info);\n"
        });
        tick.push_str("        info!(\"Embassy is running\");\n");
    } else if p.has("panic-rtt-target") {
        dep(&mut manifest, "rtt-target", "0.6.2", &[]);
        init.push_str("    rtt_target::rtt_init_print!();\n");
    }
    if p.has("panic-rtt-target") {
        dep(
            &mut manifest,
            "panic-rtt-target",
            "0.2",
            if p.has("defmt") { &["defmt"] } else { &[] },
        );
        imports.push_str("use panic_rtt_target as _;\n");
    } else if p.has("panic-reset") {
        dep(&mut manifest, "panic-reset", "0.1", &[]);
        imports.push_str("use panic_reset as _;\n");
    } else {
        dep(&mut manifest, "panic-halt", "1", &[]);
        imports.push_str("use panic_halt as _;\n");
    }
    if p.has("alloc") {
        dep(&mut manifest, "embedded-alloc", "0.7", &[]);
        imports.push_str("extern crate alloc;\n#[global_allocator]\nstatic HEAP: embedded_alloc::LlffHeap = embedded_alloc::LlffHeap::empty();\n");
        let target = get_target_by_name(&p.chip.name)?;
        let ram = target
            .memory_map
            .iter()
            .filter_map(|r| r.as_ram_region())
            .map(|r| r.range.end - r.range.start)
            .max()
            .context("No RAM region for allocator")?;
        let reserve = if sd.is_some() {
            if ram <= 32 * 1024 {
                8 * 1024
            } else {
                32 * 1024
            }
        } else {
            0
        };
        let heap_size = ((ram.saturating_sub(reserve)) / 4).min(16 * 1024);
        ensure!(heap_size >= 256, "Not enough RAM for a heap");
        init.push_str(&format!("    const HEAP_SIZE: usize = {heap_size};\n"));
        init.push_str("\n    static mut HEAP_MEM: [core::mem::MaybeUninit<u8>; HEAP_SIZE] = [core::mem::MaybeUninit::uninit(); HEAP_SIZE];\n    // SAFETY: initialized once before any task can allocate; memory lives for the whole program.\n    unsafe { HEAP.init(core::ptr::addr_of_mut!(HEAP_MEM) as usize, HEAP_SIZE); }\n");
    }
    let mut main = format!(
        "#![no_std]\n#![no_main]\n\n{imports}\nuse embassy_executor::Spawner;\nuse embassy_time::Timer;\n\n#[embassy_executor::main]\nasync fn main(_spawner: Spawner) {{\n    let _peripherals = {}::init(Default::default());\n{init}    loop {{\n{tick}        Timer::after_millis(1000).await;\n    }}\n}}\n",
        hal.replace('-', "_")
    );
    if let Some(sd) = sd {
        let mut features = vec![
            p.chip.hal_feature.as_str(),
            sd,
            "ble-peripheral",
            "ble-gatt-server",
            "critical-section-impl",
        ];
        if sd == "s122" {
            features.retain(|f| *f != "ble-peripheral" && *f != "ble-gatt-server");
            features.push("ble-central");
        }
        if p.has("defmt") {
            features.push("defmt");
        }
        manifest.push_str(&format!("nrf-softdevice = {{ git = \"https://github.com/embassy-rs/nrf-softdevice\", rev = \"b0ac850c0a5a05b8a5aef4f752b48115755b8542\", features = {features:?} }}\nnrf-softdevice-{sd} = {{ git = \"https://github.com/embassy-rs/nrf-softdevice\", rev = \"b0ac850c0a5a05b8a5aef4f752b48115755b8542\" }}\n"));
        let template = if sd == "s122" {
            include_str!("../templates/cortex/scan.rs")
        } else {
            include_str!("../templates/cortex/softdevice.rs")
        };
        if log.is_none() {
            imports.push_str("macro_rules! info { ($s:literal $(, $x:expr)* $(,)?) => {{ let _ = ($(&$x),*); }}; }\n");
        }
        main = template
            .replace("{{imports}}", &imports)
            .replace("{{init}}", &init);
    }
    let toolchain = if !p.toolchain.is_empty() {
        &p.toolchain
    } else if p.has("stack-smashing-protection") {
        "nightly"
    } else {
        "stable"
    };
    let mut rustflags = vec!["-C", "link-arg=-Tlink.x"];
    if p.has("defmt") {
        rustflags.extend(["-C", "link-arg=-Tdefmt.x"]);
    }
    if p.has("stack-smashing-protection") {
        rustflags.extend(["-Z", "stack-protector=all"]);
    }
    files.insert(".cargo/config.toml".into(), format!("[build]\ntarget = {:?}\nrustflags = {rustflags:?}\n[target.{}]\nrunner = {:?}\n[env]\nDEFMT_LOG = \"info\"\n", p.chip.target, p.chip.target, format!("probe-rs run --chip {}", p.chip.name)));
    files.insert("rust-toolchain.toml".into(), format!("[toolchain]\nchannel = {toolchain:?}\ncomponents = [\"rust-src\", \"rustfmt\", \"clippy\"]\ntargets = [{:?}]\n", p.chip.target));
    let mut build = String::from("fn main() {\n");
    if p.chip.family == Family::Nrf {
        let target = get_target_by_name(&p.chip.name)?;
        let flash = target
            .memory_map
            .iter()
            .filter_map(|r| r.as_nvm_region())
            .find(|r| r.range.start == 0)
            .context("No NRF flash region")?;
        let ram = target
            .memory_map
            .iter()
            .filter_map(|r| r.as_ram_region())
            .find(|r| r.range.start == 0x20000000)
            .context("No NRF RAM region")?;
        // Pinned nrf-softdevice bindings: SD_FLASH_SIZE plus the 4 KiB MBR.
        let flash_start = match sd {
            Some("s112") => 0x19000,
            Some("s113" | "s122") => 0x1c000,
            Some("s132") => 0x26000,
            Some("s140") => 0x27000,
            _ => flash.range.start,
        };
        let ram_start = if sd.is_some() {
            if ram.range.end - ram.range.start <= 32 * 1024 {
                0x20002000
            } else {
                0x20008000
            }
        } else {
            ram.range.start
        };
        ensure!(
            flash_start < flash.range.end && ram_start < ram.range.end,
            "Insufficient memory for this Softdevice template"
        );
        files.insert("memory.x".into(), format!("/* Verify Softdevice memory reservations against your binary and runtime configuration. */\nMEMORY {{\n FLASH : ORIGIN = {flash_start:#x}, LENGTH = {}\n RAM : ORIGIN = {ram_start:#x}, LENGTH = {}\n}}\n",flash.range.end-flash_start,ram.range.end-ram_start));
        build.push_str("    let out = std::path::PathBuf::from(std::env::var_os(\"OUT_DIR\").unwrap());\n    std::fs::copy(\"memory.x\", out.join(\"memory.x\")).unwrap();\n    println!(\"cargo:rustc-link-search={}\", out.display());\n    println!(\"cargo:rerun-if-changed=memory.x\");\n");
    }
    if p.has("stack-smashing-protection") {
        main.push_str("\n#[unsafe(export_name = \"__stack_chk_guard\")]\nstatic STACK_CHK_GUARD: usize = 0xA5A5A5A5;\n#[unsafe(no_mangle)]\nextern \"C\" fn __stack_chk_fail() -> ! { panic!(\"stack corruption\") }\n");
    }
    if p.has("embedded-test") {
        manifest.push_str("\n[dev-dependencies]\nembedded-test = \"0.7\"\n\n[[test]]\nname = \"smoke\"\nharness = false\n");
        files.insert("tests/smoke.rs".into(), "#![no_std]\n#![no_main]\n#[embedded_test::tests]\nmod tests {\n    #[test]\n    fn arithmetic() { assert_eq!(2 + 2, 4); }\n}\n".into());
        build.push_str("    println!(\"cargo:rustc-link-arg-tests=-Tembedded-test.x\");\n");
    }
    build.push_str("}\n");
    manifest.push_str("\n[profile.dev]\nopt-level = \"s\"\ndebug = 2\n\n[profile.release]\nopt-level = \"s\"\ndebug = 2\nlto = true\ncodegen-units = 1\n");
    files.insert("Cargo.toml".into(), manifest);
    files.insert("src/main.rs".into(), main);
    files.insert("build.rs".into(), build);
    files.insert(".gitignore".into(), "/target\n".into());
    files.insert("README.md".into(), format!("# {}\n\nTarget: {} (`{}`).\n\nBuild: `cargo build`. Run/flash (requires hardware permission): `cargo run`.\n\nOptions: {}\n\n{}", p.name,p.chip.name,p.chip.target,p.selected().join(", "), if sd.is_some() { "Flash the matching Nordic Softdevice separately. Review memory.x FLASH and RAM reservations against its release and BLE configuration before running.\n" } else { "" }));
    integrations(p, &mut files, toolchain)?;
    Ok(files)
}

fn integrations(p: &Project, files: &mut Files, toolchain: &str) -> Result<()> {
    let target = &p.chip.target;
    if p.has("cargo-embed") {
        files.insert(
            "Embed.toml".into(),
            format!(
                "[default.general]\nchip = {:?}\n[default.rtt]\nenabled = {}\n",
                p.chip.name,
                p.has("log") || p.has("defmt") || p.has("panic-rtt-target")
            ),
        );
    }
    if p.has("vscode") {
        files.insert(".vscode/settings.json".into(), serde_json::to_string_pretty(&json!({"rust-analyzer.cargo.target":target,"rust-analyzer.check.allTargets":false}))?);
        files.insert(".vscode/extensions.json".into(), serde_json::to_string_pretty(&json!({"recommendations":["rust-lang.rust-analyzer","probe-rs.probe-rs-debugger"]}))?);
        files.insert(".vscode/launch.json".into(), serde_json::to_string_pretty(&json!({"version":"0.2.0","configurations":[{"type":"probe-rs-debug","request":"launch","name":"Debug firmware","chip":p.chip.name,"cwd":"${workspaceFolder}","flashingConfig":{"flashingEnabled":true},"coreConfigs":[{"coreIndex":0,"programBinary":format!("target/{target}/debug/{}",p.name),"rttEnabled":p.has("log")||p.has("defmt")||p.has("panic-rtt-target")}]}]}))?);
    }
    if p.has("helix") {
        files.insert(".helix/languages.toml".into(), format!("[language-server.rust-analyzer.config.cargo]\ntarget = {target:?}\n[language-server.rust-analyzer.config.check]\nallTargets = false\n"));
    }
    if p.has("zed") {
        files.insert(".zed/settings.json".into(), serde_json::to_string_pretty(&json!({"lsp":{"rust-analyzer":{"initialization_options":{"cargo":{"target":target},"check":{"allTargets":false}}}}}))?);
    }
    if p.has("neovim-classic") || p.has("neovim-rustaceanvim") {
        let settings = format!(
            "{{ ['rust-analyzer'] = {{ cargo = {{ target = '{target}' }}, check = {{ allTargets = false }} }} }}"
        );
        files.insert(
            ".nvim.lua".into(),
            if p.has("neovim-classic") {
                format!("require('lspconfig').rust_analyzer.setup({{ settings = {settings} }})\n")
            } else {
                format!("vim.g.rustaceanvim = {{ server = {{ default_settings = {settings} }} }}\n")
            },
        );
    }
    for (name, file) in [
        ("agents", "AGENTS.md"),
        ("claude", "CLAUDE.md"),
        ("codex", "CODEX.md"),
        ("gemini", "GEMINI.md"),
    ] {
        if p.has(name) {
            files.insert(file.into(), format!("# {}\n\nEmbassy firmware for {}.\n\n- Build with `cargo build`.\n- Format with `cargo fmt --check`.\n- Review with `cargo clippy -- -D warnings`.\n- Never connect to or flash hardware without explicit permission.\n- Check the board schematic before assigning GPIO.\n",p.name,p.chip.name));
        }
    }
    if p.has("ci") {
        files.insert(".github/workflows/rust.yml".into(), format!("name: Rust\non: [push, pull_request]\njobs:\n  check:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v4\n      - uses: dtolnay/rust-toolchain@master\n        with:\n          toolchain: {toolchain}\n          targets: {target}\n          components: rustfmt, clippy\n      - run: cargo fmt --check\n      - run: cargo build --release\n      - run: cargo clippy -- -D warnings\n"));
    }
    Ok(())
}
