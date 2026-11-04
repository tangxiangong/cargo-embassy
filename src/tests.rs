use super::*;
use clap::Parser;

#[test]
fn legacy_init_and_new_cli_parse() {
    let cli = Cli::try_parse_from([
        "cargo-embassy",
        "init",
        "demo",
        "--chip",
        "esp32c6",
        "-o",
        "wifi",
        "--logging",
        "defmt",
        "--headless",
    ])
    .unwrap();
    let Some(Command::Init(args)) = cli.command else {
        panic!("init")
    };
    let catalog = Catalog::new().unwrap();
    let p = args.project(&catalog).unwrap();
    assert!(p.has("wifi") && p.has("alloc") && p.has("embassy") && p.has("unstable-hal"));
    assert!(p.has("defmt") && !p.has("log"));
}

#[test]
fn conflicting_explicit_cli_options_are_errors() {
    let catalog = Catalog::new().unwrap();
    let cli = Cli::try_parse_from([
        "cargo-embassy",
        "demo",
        "--chip",
        "esp32c6",
        "-o",
        "defmt",
        "-o",
        "log",
    ])
    .unwrap();
    assert!(cli.args.project(&catalog).is_err());
}

#[test]
fn incompatible_chip_features_and_cascade() {
    let catalog = Catalog::new().unwrap();
    let mut p = catalog.project(catalog.chip("esp32h2").unwrap()).unwrap();
    assert!(p.toggle("wifi").is_err());
    assert!(!p.has("alloc"));
    p.enable("ble-trouble").unwrap();
    assert!(p.has("alloc"));
    p.toggle("alloc").unwrap();
    assert!(!p.has("ble-trouble"));
}

#[test]
fn esp_full_template_combination_renders() {
    let catalog = Catalog::new().unwrap();
    let mut p = catalog.project(catalog.chip("esp32c6").unwrap()).unwrap();
    p.name = "embassy-full-test".into();
    for o in [
        "wifi",
        "ble-trouble",
        "defmt",
        "probe-rs",
        "panic-rtt-target",
        "embedded-test",
        "cargo-embed",
        "wokwi",
        "ci",
        "vscode",
        "helix",
        "zed",
        "neovim-rustaceanvim",
        "agents",
    ] {
        p.enable(o).unwrap();
    }
    catalog.refresh(&mut p).unwrap();
    let files = generate::plan(&catalog, &p).unwrap();
    for path in [
        "Cargo.toml",
        "diagram.json",
        "wokwi.toml",
        "Embed.toml",
        "AGENTS.md",
        "tests/hello_test.rs",
        ".vscode/settings.json",
    ] {
        assert!(files.contains_key(path), "{path}");
    }
    assert!(files["Cargo.toml"].contains("esp-radio"));
    assert!(files["src/bin/main.rs"].contains("WifiController"));
    assert!(files["src/bin/main.rs"].contains("trouble_host::new"));
}

#[test]
fn no_logging_removes_frontend_and_init() {
    let catalog = Catalog::new().unwrap();
    for chip in ["esp32c6", "nrf52840_xxAA", "STM32F103C6"] {
        let mut p = catalog.project(catalog.chip(chip).unwrap()).unwrap();
        p.name = "embassy-no-log-test".into();
        p.config
            .selected
            .retain(|i| p.config.flat_options[*i].selection_group != "log-frontend");
        let files = generate::plan(&catalog, &p).unwrap();
        let manifest: toml::Value = toml::from_str(&files["Cargo.toml"]).unwrap();
        assert!(manifest["dependencies"].get("defmt").is_none());
        assert!(manifest["dependencies"].get("log").is_none());
        let main = files
            .iter()
            .find(|(k, _)| k.ends_with("/main.rs"))
            .unwrap()
            .1;
        assert!(!main.contains("init_logger"));
    }
}

#[test]
fn cortex_options_and_safe_output() {
    let catalog = Catalog::new().unwrap();
    let mut p = catalog
        .project(catalog.chip("nrf52840_xxAA").unwrap())
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    p.output = dir.path().to_str().unwrap().into();
    p.name = "demo".into();
    for o in [
        "log",
        "alloc",
        "ci",
        "vscode",
        "embedded-test",
        "cargo-embed",
        "agents",
    ] {
        p.enable(o).unwrap();
    }
    let files = generate::plan(&catalog, &p).unwrap();
    assert!(files["src/main.rs"].contains("rtt_init_log!"));
    assert!(!files["Cargo.toml"].contains("defmt"));
    let path = generate::write(&p, &files).unwrap();
    assert_eq!(
        std::fs::read_to_string(path.join("Cargo.toml")).unwrap(),
        files["Cargo.toml"]
    );
    assert!(generate::write(&p, &files).is_err());
    p.name = "../escape".into();
    assert!(generate::plan(&catalog, &p).is_err());
    p.name = "second".into();
    let mut unsafe_files = files;
    unsafe_files.insert("../escape".into(), "bad".into());
    assert!(generate::write(&p, &unsafe_files).is_err());
    assert!(!dir.path().join("second").exists());
}

#[test]
fn every_supported_esp_chip_and_cortex_option_renders_valid_source() {
    let catalog = Catalog::new().unwrap();
    let mut chips: Vec<String> = catalog
        .esp
        .template
        .all_options()
        .into_iter()
        .filter(|o| o.selection_group == "chip")
        .map(|o| o.name.clone())
        .collect();
    chips.extend(
        [
            "STM32F103C6",
            "nRF52840_xxAA",
            "nRF52810_xxAA",
            "nRF52820_xxAA",
            "nRF52833_xxAA",
            "nRF52832_xxAA",
        ]
        .map(String::from),
    );
    let mut renders = 0;
    for chip in chips {
        let base = catalog.project(catalog.chip(&chip).unwrap()).unwrap();
        let names: std::collections::BTreeSet<_> = base
            .config
            .flat_options
            .iter()
            .filter(|o| {
                !o.name.is_empty()
                    && o.selection_group != "chip"
                    && base.config.is_option_compatible(o)
            })
            .map(|o| o.name.clone())
            .collect();
        for name in names {
            let mut p = catalog.project(catalog.chip(&chip).unwrap()).unwrap();
            p.name = "render-matrix-test".into();
            p.enable(&name)
                .unwrap_or_else(|e| panic!("{chip}/{name}: {e:#}"));
            catalog.refresh(&mut p).unwrap();
            let files =
                generate::plan(&catalog, &p).unwrap_or_else(|e| panic!("{chip}/{name}: {e:#}"));
            for (path, source) in &files {
                if path.ends_with(".rs") {
                    syn::parse_file(source).unwrap_or_else(|e| panic!("{chip}/{name}/{path}: {e}"));
                }
            }
            renders += 1;
        }
    }
    assert!(renders > 300, "only {renders} selections checked");
}

#[test]
fn aliases_do_not_silently_lose_explicit_choices() {
    let catalog = Catalog::new().unwrap();
    for args in [
        vec![
            "cargo-embassy",
            "demo",
            "--chip",
            "esp32c6",
            "--logging",
            "log",
            "-o",
            "probe-rs",
        ],
        vec![
            "cargo-embassy",
            "demo",
            "--chip",
            "esp32c6",
            "--logging",
            "none",
            "-o",
            "defmt",
        ],
        vec![
            "cargo-embassy",
            "demo",
            "--chip",
            "nrf52840",
            "--panic-handler",
            "reset",
            "-o",
            "panic-halt",
        ],
        vec![
            "cargo-embassy",
            "demo",
            "--chip",
            "esp32c6",
            "-o",
            "esp32s3",
        ],
    ] {
        let cli = Cli::try_parse_from(args).unwrap();
        assert!(cli.args.project(&catalog).is_err());
    }
}

#[test]
fn interactive_seed_uses_the_same_configuration_as_cli() {
    let catalog = Catalog::new().unwrap();
    let cli = Cli::try_parse_from([
        "cargo-embassy",
        "--logging",
        "none",
        "--vscode",
        "--softdevice",
        "s140",
    ])
    .unwrap();
    let mut p = catalog.project(catalog.chip("nrf52840").unwrap()).unwrap();
    cli.args.apply(&mut p, &catalog).unwrap();
    assert!(!p.has("defmt") && !p.has("log"));
    assert!(p.has("vscode") && p.has("s140"));
}

#[test]
fn required_embassy_dependency_cannot_be_deselected() {
    let catalog = Catalog::new().unwrap();
    let mut p = catalog.project(catalog.chip("esp32c6").unwrap()).unwrap();
    assert!(p.toggle("unstable-hal").is_err());
    assert!(p.has("unstable-hal") && p.has("embassy"));
}

#[test]
fn softdevice_flash_origin_matches_the_selected_stack() {
    let catalog = Catalog::new().unwrap();
    for (chip, sd, origin) in [
        ("nrf52832", "s132", "0x26000"),
        ("nrf52840", "s140", "0x27000"),
        ("nrf52820", "s122", "0x1c000"),
        ("nrf52810", "s112", "0x19000"),
    ] {
        // nRF52832 has two flash capacities; request the unambiguous variant.
        let chip = if chip == "nrf52832" {
            "nrf52832_xxaa"
        } else {
            chip
        };
        let mut p = catalog.project(catalog.chip(chip).unwrap()).unwrap();
        p.enable(sd).unwrap();
        let files = generate::plan(&catalog, &p).unwrap();
        assert!(files["memory.x"].contains(&format!("FLASH : ORIGIN = {origin}")));
    }
}

#[test]
fn softdevice_without_logging_and_stack_guard_sources_are_valid() {
    let catalog = Catalog::new().unwrap();
    let mut p = catalog.project(catalog.chip("nrf52840").unwrap()).unwrap();
    p.enable("s140").unwrap();
    p.toggle("defmt").unwrap();
    p.enable("stack-smashing-protection").unwrap();
    let files = generate::plan(&catalog, &p).unwrap();
    syn::parse_file(&files["src/main.rs"]).unwrap();
    assert!(files["src/main.rs"].contains("let _ = ($(&$x),*)"));
    assert!(files["src/main.rs"].contains("static STACK_CHK_GUARD"));
    assert!(files["rust-toolchain.toml"].contains("nightly"));
}

#[test]
fn generation_options_are_never_discarded_before_a_subcommand() {
    let root = Cli::try_parse_from(["cargo-embassy", "--dry-run", "tui"]).unwrap();
    assert!(root.command.is_none());
    assert!(root.args.dry_run);
    assert!(
        Cli::try_parse_from([
            "cargo-embassy",
            "--output-path",
            "/tmp",
            "init",
            "demo",
            "--chip",
            "esp32c6"
        ])
        .is_err()
    );
    let cli = Cli::try_parse_from([
        "cargo-embassy",
        "tui",
        "--dry-run",
        "--output-path",
        "/tmp",
        "--chip",
        "esp32c6",
    ])
    .unwrap();
    let Some(Command::Tui(args)) = cli.command else {
        panic!("expected TUI");
    };
    assert!(args.dry_run);
    assert_eq!(args.output_path, "/tmp");
    assert_eq!(args.chip.as_deref(), Some("esp32c6"));
}

#[test]
fn typed_aliases_reject_unrelated_options() {
    for (flag, value) in [("--panic-handler", "wifi"), ("--softdevice", "log")] {
        assert!(
            Cli::try_parse_from([
                "cargo-embassy",
                "init",
                "demo",
                "--chip",
                "esp32c6",
                flag,
                value
            ])
            .is_err()
        );
    }
    assert!(
        Cli::try_parse_from([
            "cargo-embassy",
            "init",
            "demo",
            "--chip",
            "nrf52840",
            "--softdevice",
            "S140"
        ])
        .is_ok()
    );
}

#[test]
fn ble_templates_handle_task_allocation_before_spawning() {
    let catalog = Catalog::new().unwrap();
    for (chip, sd) in [("nrf52840", "s140"), ("nrf52820", "s122")] {
        let mut p = catalog.project(catalog.chip(chip).unwrap()).unwrap();
        p.enable(sd).unwrap();
        let files = generate::plan(&catalog, &p).unwrap();
        let main = &files["src/main.rs"];
        assert!(main.contains("spawner.spawn(softdevice_task(sd).unwrap());"));
        assert!(!main.contains("spawner.spawn(softdevice_task(sd)).unwrap()"));
    }
}

fn in_language<T>(language: i18n::Language, action: impl FnOnce() -> T) -> T {
    struct Restore(i18n::Language);
    impl Drop for Restore {
        fn drop(&mut self) {
            i18n::set(self.0);
        }
    }
    let _restore = Restore(i18n::current());
    i18n::set(language);
    action()
}

#[test]
fn locale_detection_obeys_environment_precedence() {
    use i18n::Language::{En, Zh};
    for (values, expected) in [
        (vec![], En),
        (vec![("LANG", "zh_CN.UTF-8")], Zh),
        (vec![("LANG", "zh-TW")], Zh),
        (vec![("LANG", "zh@Hans")], Zh),
        (vec![("LC_ALL", "C"), ("LANG", "zh_CN.UTF-8")], En),
        (
            vec![("LC_MESSAGES", "zh_HK.UTF-8"), ("LANG", "en_US.UTF-8")],
            Zh,
        ),
        (vec![("LC_ALL", ""), ("LANG", "zh_CN.UTF-8")], Zh),
        (
            vec![("CARGO_EMBASSY_LANG", "en"), ("LC_ALL", "zh_CN.UTF-8")],
            En,
        ),
    ] {
        assert_eq!(
            i18n::detect(|key| values
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| v.to_string()))
            .unwrap(),
            expected
        );
    }
    assert!(i18n::detect(|key| (key == "CARGO_EMBASSY_LANG").then(|| "fr".into())).is_err());
}

#[test]
fn language_flag_is_global_without_discarding_subcommand_arguments() {
    for args in [
        vec![
            "cargo-embassy",
            "--lang",
            "zh",
            "init",
            "demo",
            "--chip",
            "esp32c6",
            "--dry-run",
        ],
        vec![
            "cargo-embassy",
            "init",
            "demo",
            "--chip",
            "esp32c6",
            "--dry-run",
            "--lang=zh",
        ],
    ] {
        let (lang, args) = i18n::prepare(args.into_iter().map(Into::into).collect(), |_| {
            Some("invalid".into())
        })
        .unwrap();
        assert_eq!(lang, i18n::Language::Zh);
        let cli = Cli::try_parse_from(args).unwrap();
        let Some(Command::Init(args)) = cli.command else {
            panic!("init")
        };
        assert!(args.dry_run);
        assert_eq!(args.name.as_deref(), Some("demo"));
    }
    let (_, args) = i18n::prepare(
        ["tool", "--", "--lang", "zh"].map(Into::into).to_vec(),
        |_| None,
    )
    .unwrap();
    assert_eq!(args.len(), 4);
    for args in [vec!["tool", "--lang"], vec!["tool", "--lang=fr"]] {
        assert!(i18n::prepare(args.into_iter().map(Into::into).collect(), |_| None).is_err());
    }
}

#[test]
fn chinese_and_english_cli_help_and_errors_are_localized() {
    in_language(i18n::Language::Zh, || {
        let mut command = Cli::localized_command();
        let help = command.render_help().to_string();
        assert!(help.contains("用法：") && help.contains("命令") && help.contains("界面语言"));
        let help = command
            .find_subcommand_mut("init")
            .unwrap()
            .render_help()
            .to_string();
        assert!(help.contains("日志库") && help.contains("项目名称"));
        let help_command = Cli::localized_command()
            .try_get_matches_from(["tool", "help", "init"])
            .unwrap_err();
        assert_eq!(help_command.kind(), clap::error::ErrorKind::DisplayHelp);
        assert!(help_command.to_string().contains("日志库"));
        let err = Cli::localized_command()
            .try_get_matches_from(["tool", "init", "demo", "--logging", "bad"])
            .unwrap_err();
        let text = i18n::clap_error(&err);
        assert!(text.contains("参数值无效") && text.contains("bad"));
    });
    in_language(i18n::Language::En, || {
        let help = Cli::localized_command().render_help().to_string();
        assert!(help.contains("Usage:") && help.contains("Interface language"));
    });
}

#[test]
fn language_changes_messages_but_not_generation_results() {
    let catalog = Catalog::new().unwrap();
    for chip in ["esp32c6", "nrf52840", "stm32f103c6"] {
        let mut p = catalog.project(catalog.chip(chip).unwrap()).unwrap();
        p.name = "i18n-test-project".into();
        let en = in_language(i18n::Language::En, || generate::plan(&catalog, &p).unwrap());
        let zh = in_language(i18n::Language::Zh, || generate::plan(&catalog, &p).unwrap());
        assert_eq!(en, zh);
        p.name = "../invalid".into();
        assert!(
            in_language(i18n::Language::Zh, || p.validate().unwrap_err().to_string())
                .contains("项目名称")
        );
        assert!(
            in_language(i18n::Language::En, || p.validate().unwrap_err().to_string())
                .contains("Project name")
        );
    }
}

#[test]
fn option_translations_preserve_identifiers_and_have_english_fallback() {
    in_language(i18n::Language::Zh, || {
        assert_eq!(i18n::title("defmt", "Use defmt"), "defmt 二进制日志");
        assert!(i18n::help("wifi", "").contains("Wi-Fi"));
        assert_eq!(i18n::title("future-option", "New option"), "New option");
        assert_eq!(i18n::title("esp32c6", "ESP32-C6"), "ESP32-C6");
    });
}

#[test]
fn language_flags_cannot_supply_a_missing_option_value() {
    for option in [
        "--chip",
        "--option",
        "-o",
        "--output-path",
        "-O",
        "--toolchain",
        "--logging",
        "--panic-handler",
        "--softdevice",
    ] {
        for language in [vec!["--lang", "zh"], vec!["--lang=zh"]] {
            let mut args = vec!["tool", "init", "demo", option];
            args.extend(language);
            args.push("value");
            let error =
                i18n::prepare(args.into_iter().map(Into::into).collect(), |_| None).unwrap_err();
            assert!(error.to_string().contains(option));
        }
    }
    for args in [
        vec!["tool", "init", "demo", "--chip=esp32c6", "--lang", "zh"],
        vec!["tool", "init", "demo", "--chip", "esp32c6", "--lang=zh"],
        vec![
            "tool",
            "init",
            "demo",
            "--output-path=--lang",
            "--lang",
            "zh",
        ],
    ] {
        let (_, args) =
            i18n::prepare(args.into_iter().map(Into::into).collect(), |_| None).unwrap();
        assert!(Cli::try_parse_from(args).is_ok());
    }
}

#[test]
fn localized_parser_errors_preserve_diagnostic_context() {
    in_language(i18n::Language::Zh, || {
        let error = Cli::localized_command()
            .try_get_matches_from(["tool", "help", "nonexistent"])
            .unwrap_err();
        let message = i18n::clap_error(&error);
        assert!(message.contains("子命令：nonexistent"));
        let mut error = clap::Error::new(clap::error::ErrorKind::ArgumentConflict);
        error.insert(
            clap::error::ContextKind::InvalidArg,
            clap::error::ContextValue::String("--first".into()),
        );
        error.insert(
            clap::error::ContextKind::PriorArg,
            clap::error::ContextValue::String("--second".into()),
        );
        let message = i18n::clap_error(&error);
        assert!(message.contains("--first") && message.contains("--second"));
    });
}
