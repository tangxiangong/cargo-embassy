use crate::config::{Catalog, Project};
use anyhow::{Context, Result, ensure};
use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(
    name = "cargo embassy",
    version,
    args_conflicts_with_subcommands = true,
    about = "Interactive Embassy project generator for STM32, NRF and ESP"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
    #[command(flatten)]
    pub args: GenerateArgs,
}
#[derive(Subcommand)]
pub enum Command {
    /// Generate a project (opens the TUI if name or chip is omitted).
    Init(GenerateArgs),
    /// Open the interactive project configurator.
    Tui(GenerateArgs),
    /// List supported chips without accessing hardware.
    ListChips { filter: Option<String> },
    /// List generation options for a chip.
    ListOptions {
        #[arg(long)]
        chip: String,
    },
    /// Explain an option, its requirements and conflicts.
    Explain {
        option: String,
        #[arg(long)]
        chip: String,
    },
    /// Render every applicable option independently, without writing files.
    Check {
        #[arg(long)]
        chip: String,
    },
    /// Print Embassy documentation links.
    Docs,
}
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum Logging {
    Auto,
    Defmt,
    Log,
    None,
}
#[derive(Args, Default)]
pub struct GenerateArgs {
    pub name: Option<String>,
    #[arg(long)]
    pub chip: Option<String>,
    /// Select an option; repeat for multiple options. Dependencies are added automatically.
    #[arg(short = 'o', long = "option")]
    pub options: Vec<String>,
    /// Do not start the TUI; missing inputs are errors.
    #[arg(long)]
    pub headless: bool,
    /// Parent directory in which the new project directory will be created.
    #[arg(short = 'O', long, default_value = ".")]
    pub output_path: String,
    #[arg(long)]
    pub toolchain: Option<String>,
    #[arg(long, value_enum)]
    pub logging: Option<Logging>,
    #[arg(long, value_parser = ["halt", "reset", "panic-halt", "panic-reset", "panic-rtt-target", "esp-backtrace"])]
    pub panic_handler: Option<String>,
    #[arg(long, value_parser = ["s112", "s113", "s122", "s132", "s140"], ignore_case = true)]
    pub softdevice: Option<String>,
    #[arg(long)]
    pub vscode: bool,
    /// Render and validate without writing any files.
    #[arg(long)]
    pub dry_run: bool,
}
impl GenerateArgs {
    pub fn project(&self, catalog: &Catalog) -> Result<Project> {
        let chip = catalog.chip(self.chip.as_deref().context(crate::tr!(
            "--chip is required in headless mode",
            "非交互模式必须指定 --chip"
        ))?)?;
        let mut p = catalog.project(chip)?;
        p.name = self.name.clone().context(crate::tr!(
            "Project name is required in headless mode",
            "非交互模式必须指定项目名称"
        ))?;
        p.output = self.output_path.clone();
        p.toolchain = self.toolchain.clone().unwrap_or_default();
        self.apply(&mut p, catalog)?;
        p.validate()?;
        Ok(p)
    }

    pub fn apply(&self, p: &mut Project, catalog: &Catalog) -> Result<()> {
        if let Some(logging) = self.logging {
            p.config
                .selected
                .retain(|i| p.config.flat_options[*i].selection_group != "log-frontend");
            match logging {
                Logging::Auto => p.enable(if p.chip.family == crate::chip::Family::Esp {
                    "log"
                } else {
                    "defmt"
                })?,
                Logging::Defmt => p.enable("defmt")?,
                Logging::Log => p.enable("log")?,
                Logging::None => {}
            }
        }
        if let Some(panic) = &self.panic_handler {
            p.enable(match panic.as_str() {
                "halt" => "panic-halt",
                "reset" => "panic-reset",
                other => other,
            })?;
        }
        if let Some(sd) = &self.softdevice {
            p.enable(&sd.to_ascii_lowercase())?;
        }
        if self.vscode {
            p.enable("vscode")?;
        }
        for name in &self.options {
            p.enable(name)?;
            catalog.refresh(p)?;
        }
        for name in &self.options {
            ensure!(
                p.has(name),
                crate::tr!(
                    "Requested option '{name}' conflicts with another requested option",
                    "指定选项 '{name}' 与其他指定选项冲突"
                )
            );
        }
        match self.logging {
            Some(Logging::Defmt) => ensure!(
                p.has("defmt"),
                crate::tr!(
                    "--logging defmt conflicts with selected options",
                    "--logging defmt 与所选功能冲突"
                )
            ),
            Some(Logging::Log) => ensure!(
                p.has("log"),
                crate::tr!(
                    "--logging log conflicts with selected options",
                    "--logging log 与所选功能冲突"
                )
            ),
            Some(Logging::None) => ensure!(
                !p.has("defmt") && !p.has("log"),
                crate::tr!(
                    "--logging none conflicts with selected options",
                    "--logging none 与所选功能冲突"
                )
            ),
            _ => {}
        }
        if let Some(sd) = &self.softdevice {
            ensure!(
                p.has(&sd.to_ascii_lowercase()),
                crate::tr!(
                    "--softdevice conflicts with selected options",
                    "--softdevice 与所选功能冲突"
                )
            );
        }
        if let Some(panic) = &self.panic_handler {
            let name = match panic.as_str() {
                "halt" => "panic-halt",
                "reset" => "panic-reset",
                other => other,
            };
            ensure!(
                p.has(name),
                crate::tr!(
                    "--panic-handler conflicts with selected options",
                    "--panic-handler 与所选功能冲突"
                )
            );
        }
        catalog.refresh(p)?;
        Ok(())
    }
}

impl Cli {
    pub fn localized_command() -> clap::Command {
        use clap::CommandFactory;
        let command = Self::command().arg(
            clap::Arg::new("lang")
                .long("lang")
                .global(true)
                .value_parser(["en", "zh"])
                .help(crate::i18n::text(
                    "Interface language (overrides environment)",
                    "界面语言：en（英文）、zh（中文），优先于环境变量",
                )),
        );
        if crate::i18n::current() == crate::i18n::Language::En {
            return command;
        }
        localize_help(command)
    }
}

fn localize_help(command: clap::Command) -> clap::Command {
    let about = match command.get_name() {
        "init" => "生成项目；省略名称或芯片时打开交互界面",
        "tui" => "打开交互式项目配置界面",
        "list-chips" => "列出支持的芯片，不访问硬件",
        "list-options" => "列出指定芯片的生成选项",
        "explain" => "查看选项说明、依赖和冲突",
        "check" => "逐项渲染验证，不写入项目文件",
        "docs" => "显示 Embassy 文档链接",
        _ => "STM32、NRF 和 ESP 的 Embassy 交互式项目生成器",
    };
    let has_version = command.get_version().is_some();
    let mut command = command
        .about(about)
        .help_template("{about-with-newline}\n用法：{usage}\n\n{all-args}{after-help}")
        .subcommand_help_heading("命令")
        .disable_help_flag(true)
        .disable_version_flag(true)
        .mut_args(|arg| {
            let positional = arg.is_positional();
            let help = match arg.get_id().as_str() {
                "name" => "项目名称",
                "chip" => "目标芯片",
                "filter" => "按名称筛选芯片",
                "option" => "要查看的选项名称",
                "options" => "生成选项；可重复指定，自动添加依赖",
                "headless" => "禁止打开 TUI；缺少必要参数时直接报错",
                "output_path" => "已有输出父目录；默认为当前目录",
                "toolchain" => "Rust 工具链名称；不会自动安装",
                "logging" => "日志库：auto（自动）、defmt、log、none（关闭）",
                "panic_handler" => "Panic 处理方式：halt、reset、panic-halt、panic-reset、panic-rtt-target、esp-backtrace",
                "softdevice" => "Nordic Softdevice 型号：s112、s113、s122、s132、s140",
                "vscode" => "生成 VS Code 配置",
                "dry_run" => "仅渲染验证并列出文件，不写入",
                "lang" => "界面语言：en（英文）、zh（中文），优先于环境变量",
                _ => return arg,
            };
            let takes_values = arg.get_action().takes_values();
            let arg = arg.help(help).help_heading(if positional { "参数" } else { "选项" });
            if takes_values { arg.hide_possible_values(true).hide_default_value(true) } else { arg }
        })
        .arg(
            clap::Arg::new("help")
                .short('h')
                .long("help")
                .action(clap::ArgAction::Help)
                .help("显示帮助")
                .help_heading("选项"),
        )
        .mut_subcommands(localize_help);
    if has_version {
        command = command.arg(
            clap::Arg::new("version")
                .short('V')
                .long("version")
                .action(clap::ArgAction::Version)
                .help("显示版本")
                .help_heading("选项"),
        );
    }
    command
}
