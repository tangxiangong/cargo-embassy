use crate::chip::{self, Chip, Family};
use anyhow::{Context, Result, bail, ensure};
use esp_generate::{
    Loaded, TemplateSource,
    config::{ActiveConfiguration, flatten_options},
    plugin,
    template::{GeneratorOption, GeneratorOptionCategory, GeneratorOptionItem},
};
use std::{collections::BTreeSet, path::Path};

pub struct Catalog {
    pub esp: Loaded,
    pub chips: Vec<String>,
}

impl Catalog {
    pub fn new() -> Result<Self> {
        let esp = Loaded::open(TemplateSource::Bundled)?;
        let mut chips = chip::cortex_names()?;
        chips.extend(
            esp.template
                .all_options()
                .into_iter()
                .filter(|o| o.selection_group == "chip")
                .map(|o| o.name.clone()),
        );
        chips.sort_by_key(|s| s.to_ascii_lowercase());
        Ok(Self { esp, chips })
    }

    pub fn chip(&self, name: &str) -> Result<Chip> {
        if let Some(o) = self
            .esp
            .template
            .all_options()
            .into_iter()
            .find(|o| o.selection_group == "chip" && o.name == chip::normalize(name))
        {
            // The plugin supplies the exact target to the renderer.
            return Ok(Chip {
                name: o.name.clone(),
                family: Family::Esp,
                target: String::new(),
                hal_feature: o.name.clone(),
            });
        }
        let name = chip::normalize(name);
        let matches: Vec<_> = self
            .chips
            .iter()
            .filter(|c| chip::normalize(c).starts_with(&name))
            .collect();
        let exact = matches.iter().find(|c| chip::normalize(c) == name);
        let selected = if let Some(exact) = exact {
            *exact
        } else if matches.len() == 1 {
            matches[0]
        } else {
            bail!(crate::tr!(
                "Unknown or ambiguous chip '{name}'; choose a full name from list-chips",
                "芯片 '{name}' 不存在或有多个匹配项；请从 list-chips 中选择完整名称"
            ))
        };
        Chip::cortex(selected)
    }

    pub fn project(&self, chip: Chip) -> Result<Project> {
        let options = if chip.family == Family::Esp {
            self.esp.template.options.clone()
        } else {
            cortex_options(&chip)
        };
        let flat_options = flatten_options(&options);
        let mut p = Project {
            name: "my-project".into(),
            output: ".".into(),
            toolchain: String::new(),
            chip,
            config: ActiveConfiguration {
                selected: vec![],
                options,
                flat_options,
                facts: None,
            },
        };
        if p.chip.family == Family::Esp {
            let name = p.chip.name.clone();
            p.enable(&name)?;
            p.enable("unstable-hal")?;
            p.enable("embassy")?;
        }
        p.enable(if p.chip.family == Family::Esp {
            "log"
        } else {
            "defmt"
        })?;
        p.enable(if p.chip.family == Family::Esp {
            "esp-backtrace"
        } else {
            "panic-halt"
        })?;
        self.refresh(&mut p)?;
        Ok(p)
    }

    pub fn refresh(&self, p: &mut Project) -> Result<()> {
        if p.chip.family == Family::Esp {
            let facts = self
                .esp
                .resolved
                .facts(&plugin::selection(p.selected(), &p.config.flat_options))
                .map_err(anyhow::Error::msg)?;
            p.config.set_facts(Some(facts));
        }
        Ok(())
    }
}

pub struct Project {
    pub name: String,
    pub output: String,
    pub toolchain: String,
    pub chip: Chip,
    pub config: ActiveConfiguration,
}

impl Project {
    pub fn has(&self, name: &str) -> bool {
        self.config.is_selected(name)
    }
    pub fn selected(&self) -> Vec<String> {
        self.config
            .selected
            .iter()
            .map(|i| self.config.flat_options[*i].name.clone())
            .collect()
    }
    pub fn enable(&mut self, name: &str) -> Result<()> {
        self.enable_inner(name, &mut BTreeSet::new())
    }
    fn enable_inner(&mut self, name: &str, visiting: &mut BTreeSet<String>) -> Result<()> {
        if self.has(name) {
            return Ok(());
        }
        ensure!(
            visiting.insert(name.into()),
            crate::tr!(
                "Cyclic option requirement: {name}",
                "选项依赖存在循环：{name}"
            )
        );
        let index = self
            .config
            .flat_options
            .iter()
            .position(|o| o.name == name && self.config.is_option_compatible(o))
            .with_context(|| {
                crate::tr!(
                    "Option '{name}' is unavailable for {}",
                    "芯片 {} 不支持选项 '{name}'",
                    self.chip.name
                )
            })?;
        ensure!(
            self.config.flat_options[index].selection_group != "chip" || name == self.chip.name,
            crate::tr!(
                "Use --chip or the chip selector to change the target",
                "请使用 --chip 或芯片选择器更改目标"
            )
        );
        let requires = self.config.flat_options[index].requires.clone();
        for req in requires.iter().filter(|r| !r.starts_with('!')) {
            self.enable_inner(req, visiting)?;
        }
        self.config.select_idx(index);
        ensure!(
            self.has(name),
            crate::tr!(
                "Cannot enable '{name}': conflicts with current selection",
                "无法启用 '{name}'：与当前配置冲突"
            )
        );
        visiting.remove(name);
        Ok(())
    }
    pub fn toggle(&mut self, name: &str) -> Result<()> {
        // An invalid toggle must not leave partially selected dependencies behind.
        let previous = self.config.selected.clone();
        let result = if let Some(index) = self
            .config
            .selected
            .iter()
            .copied()
            .find(|i| self.config.flat_options[*i].name == name)
        {
            ensure!(
                name != self.chip.name && name != "embassy",
                crate::tr!("Chip and Embassy are required", "芯片和 Embassy 为必选项")
            );
            let cleared = self
                .config
                .would_force_deselect(&self.config.flat_options[index]);
            ensure!(
                !cleared.iter().any(|name| name == "embassy"),
                crate::tr!(
                    "This option is required by Embassy",
                    "Embassy 依赖此选项，不能关闭"
                )
            );
            self.config.deselect_idx(index);
            Ok(())
        } else {
            self.enable(name)
        };
        if result.is_err() {
            self.config.selected = previous;
        }
        result
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.name.is_empty()
                && self.name.starts_with(|c: char| c.is_ascii_alphabetic())
                && self
                    .name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            crate::tr!(
                "Project name must start with a letter and contain only letters, digits, - or _",
                "项目名称必须以英文字母开头，只能包含英文字母、数字、- 或 _"
            )
        );
        ensure!(
            ![
                "test", "build", "deps", "examples", "crate", "self", "super", "std", "core",
                "alloc"
            ]
            .contains(&self.name.as_str()),
            crate::tr!("Reserved project name", "该项目名称为保留名称")
        );
        ensure!(
            Path::new(&self.output).is_dir(),
            crate::tr!(
                "Output parent must already be a directory",
                "输出父目录必须是已存在的目录"
            )
        );
        ensure!(
            !Path::new(&self.output).join(&self.name).try_exists()?,
            crate::tr!("Destination already exists", "目标目录已存在")
        );
        ensure!(
            self.toolchain
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c)),
            crate::tr!("Invalid toolchain name", "工具链名称无效")
        );
        for i in &self.config.selected {
            let o = &self.config.flat_options[*i];
            ensure!(
                self.config.is_option_active(o),
                crate::tr!(
                    "Invalid selection: {} (requires {:?})",
                    "无效选项：{}（依赖 {:?}）",
                    o.name,
                    o.requires
                )
            );
        }
        if self.chip.family == Family::Esp {
            ensure!(
                self.has("embassy"),
                crate::tr!("Embassy is required", "必须启用 Embassy")
            );
        }
        if self.has("stack-smashing-protection") && !self.toolchain.is_empty() {
            ensure!(
                self.toolchain.starts_with("nightly") || self.toolchain == "esp",
                crate::tr!(
                    "Stack protection requires nightly (or esp for Xtensa)",
                    "栈保护需要 nightly 工具链；Xtensa 使用 esp"
                )
            );
        }
        Ok(())
    }
}

fn option(
    name: &str,
    title: &str,
    group: &str,
    requires: &[&str],
    help: &str,
) -> GeneratorOptionItem {
    GeneratorOptionItem::Option(GeneratorOption {
        name: name.into(),
        display_name: title.into(),
        selection_group: group.into(),
        requires: requires.iter().map(|s| (*s).into()).collect(),
        help: help.into(),
        ..Default::default()
    })
}
fn category(name: &str, title: &str, options: Vec<GeneratorOptionItem>) -> GeneratorOptionItem {
    GeneratorOptionItem::Category(GeneratorOptionCategory {
        name: name.into(),
        display_name: title.into(),
        help: String::new(),
        requires: vec![],
        options,
    })
}
fn cortex_options(chip: &Chip) -> Vec<GeneratorOptionItem> {
    let mut runtime = vec![
        option(
            "alloc",
            "Heap allocation (embedded-alloc)",
            "",
            &[],
            "Uses up to one quarter of available RAM, capped at 16 KiB.",
        ),
        option(
            "stack-smashing-protection",
            "Stack smashing protection",
            "",
            &[],
            "Uses nightly Rust stack-protector checks.",
        ),
    ];
    if chip.family == Family::Nrf {
        for (name, supported) in [
            (
                "s112",
                &["nrf52805", "nrf52810", "nrf52811", "nrf52820", "nrf52832"][..],
            ),
            (
                "s113",
                &[
                    "nrf52805", "nrf52810", "nrf52811", "nrf52820", "nrf52832", "nrf52833",
                    "nrf52840",
                ][..],
            ),
            ("s122", &["nrf52820", "nrf52833"][..]),
            ("s132", &["nrf52810", "nrf52832"][..]),
            (
                "s140",
                &["nrf52811", "nrf52820", "nrf52833", "nrf52840"][..],
            ),
        ] {
            if supported.contains(&chip.hal_feature.as_str()) {
                runtime.push(option(name, &format!("BLE / Nordic Softdevice {name}"), "softdevice", &[],
                    if name == "s122" { "Generates a BLE central scanner. Flash S122 separately and verify memory.x before running." } else { "Generates a BLE GATT peripheral. Flash the matching Softdevice separately and verify memory.x before running." }));
            }
        }
    }
    vec![
        category("runtime", "Runtime and connectivity", runtime),
        category(
            "logging",
            "Logging frontend",
            vec![
                option(
                    "defmt",
                    "defmt over RTT",
                    "log-frontend",
                    &[],
                    "Compact binary logs decoded by probe-rs.",
                ),
                option(
                    "log",
                    "log over RTT",
                    "log-frontend",
                    &[],
                    "Text logging using rtt-target. Selecting this replaces defmt.",
                ),
            ],
        ),
        category(
            "debugging",
            "Flashing, panic and tests",
            vec![
                option(
                    "panic-halt",
                    "Halt on panic",
                    "panic-handler",
                    &[],
                    "Stops at the panic site.",
                ),
                option(
                    "panic-reset",
                    "Reset on panic",
                    "panic-handler",
                    &[],
                    "Restarts the MCU.",
                ),
                option(
                    "panic-rtt-target",
                    "Report panic over RTT",
                    "panic-handler",
                    &[],
                    "Reports panic details on the selected RTT transport.",
                ),
                option(
                    "embedded-test",
                    "On-device embedded-test harness",
                    "",
                    &[],
                    "Generates a test target; running it requires a probe and hardware.",
                ),
                option(
                    "cargo-embed",
                    "Generate Embed.toml",
                    "",
                    &[],
                    "probe-rs flashing and RTT configuration.",
                ),
            ],
        ),
        category(
            "optional",
            "Project integration",
            vec![option(
                "ci",
                "GitHub Actions build / fmt / clippy",
                "",
                &[],
                "Creates a workflow; no workflow is run during generation.",
            )],
        ),
        category(
            "editor",
            "Editor integration",
            [
                "vscode",
                "helix",
                "zed",
                "neovim-classic",
                "neovim-rustaceanvim",
            ]
            .into_iter()
            .map(|n| {
                option(
                    n,
                    n,
                    if n.starts_with("neovim") {
                        "neovim"
                    } else {
                        ""
                    },
                    &[],
                    "Configures rust-analyzer for the generated embedded target.",
                )
            })
            .collect(),
        ),
        category(
            "guidance",
            "Coding-agent guidance",
            ["agents", "claude", "codex", "gemini"]
                .into_iter()
                .map(|n| {
                    option(
                        n,
                        n,
                        "coding-agent-guidance",
                        &[],
                        "Adds project build and hardware-access guidance.",
                    )
                })
                .collect(),
        ),
    ]
}
