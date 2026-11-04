use anyhow::{Result, bail};
use std::{cell::Cell, ffi::OsString};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Language {
    #[default]
    En,
    Zh,
}

thread_local! { static LANGUAGE: Cell<Language> = const { Cell::new(Language::En) }; }
pub fn current() -> Language {
    LANGUAGE.get()
}
pub fn set(language: Language) {
    LANGUAGE.set(language);
}
pub fn text<'a>(en: &'a str, zh: &'a str) -> &'a str {
    if current() == Language::Zh { zh } else { en }
}

#[macro_export]
macro_rules! tr {
    ($en:literal, $zh:literal $(, $arg:expr)* $(,)?) => {
        if $crate::i18n::current() == $crate::i18n::Language::Zh {
            format!($zh $(, $arg)*)
        } else {
            format!($en $(, $arg)*)
        }
    };
}

pub fn detect(mut env: impl FnMut(&str) -> Option<String>) -> Result<Language> {
    if let Some(value) = env("CARGO_EMBASSY_LANG").filter(|v| !v.is_empty()) {
        return parse(&value);
    }
    for key in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Some(value) = env(key).filter(|v| !v.is_empty()) {
            let prefix = value.split(['_', '-', '.', '@']).next().unwrap_or("");
            return Ok(if prefix.eq_ignore_ascii_case("zh") {
                Language::Zh
            } else {
                Language::En
            });
        }
    }
    Ok(Language::En)
}

fn parse(value: &str) -> Result<Language> {
    match value {
        "en" => Ok(Language::En),
        "zh" => Ok(Language::Zh),
        _ => bail!("Invalid language / 无效语言 '{value}'; use --lang en or --lang zh"),
    }
}

/// Remove the global language flag before Clap's subcommand conflict handling.
/// Everything after `--` remains positional input.
pub fn prepare(
    argv: Vec<OsString>,
    env: impl FnMut(&str) -> Option<String>,
) -> Result<(Language, Vec<OsString>)> {
    use clap::CommandFactory;
    let command = crate::cli::Cli::command();
    let value_flags: Vec<String> = command
        .get_arguments()
        .filter(|arg| arg.get_action().takes_values())
        .flat_map(|arg| {
            arg.get_long()
                .map(|long| format!("--{long}"))
                .into_iter()
                .chain(arg.get_short().map(|short| format!("-{short}")))
        })
        .collect();
    let mut explicit = None;
    let mut filtered: Vec<OsString> = Vec::with_capacity(argv.len());
    let mut args = argv.into_iter();
    if let Some(program) = args.next() {
        filtered.push(program);
    }
    while let Some(arg) = args.next() {
        if arg == "--" {
            filtered.push(arg);
            filtered.extend(args);
            break;
        }
        let is_language = arg == "--lang" || arg.to_str().is_some_and(|s| s.starts_with("--lang="));
        if is_language
            && let Some(previous) = filtered.last()
            && value_flags.iter().any(|flag| previous == flag.as_str())
        {
            bail!(
                "Missing value for {} before --lang / --lang 前的 {} 缺少参数值",
                previous.to_string_lossy(),
                previous.to_string_lossy()
            );
        }
        if arg == "--lang" {
            let value = args.next().ok_or_else(|| {
                anyhow::anyhow!("--lang requires en or zh / --lang 需要指定 en 或 zh")
            })?;
            explicit = Some(parse(&value.to_string_lossy())?);
        } else if let Some(value) = arg.to_str().and_then(|s| s.strip_prefix("--lang=")) {
            explicit = Some(parse(value)?);
        } else {
            filtered.push(arg);
        }
    }
    Ok((
        match explicit {
            Some(lang) => lang,
            None => detect(env)?,
        },
        filtered,
    ))
}

pub fn title<'a>(name: &str, fallback: &'a str) -> &'a str {
    if current() == Language::En {
        return fallback;
    }
    match name {
        "runtime" => "运行时与连接",
        "logging" => "日志库",
        "debugging" => "烧录、Panic 与测试",
        "optional" => "项目选项",
        "editor" => "编辑器集成",
        "guidance" | "coding-agent-guidance" => "AI 编程助手指引",
        "module" => "模组 / 开发板",
        "flashing-probe-rs" => "烧录、日志与调试（probe-rs）",
        "flashing-espflash" => "烧录、日志与调试（espflash）",
        "unstable-hal" => "启用不稳定 HAL 功能",
        "alloc" => "启用堆内存分配",
        "wifi" => "启用 Wi-Fi（esp-radio）",
        "ble-trouble" => "启用 BLE（esp-radio / trouble）",
        "embassy" => "Embassy 异步运行时",
        "stack-smashing-protection" => "栈溢出破坏检测",
        "probe-rs" => "使用 probe-rs 烧录和监视",
        "defmt" => "defmt 二进制日志",
        "log" => "log 文本日志",
        "panic-halt" => "Panic 时停止",
        "panic-reset" => "Panic 时复位",
        "panic-rtt-target" => "通过 RTT 输出 Panic",
        "esp-backtrace" => "使用 esp-backtrace 处理 Panic",
        "embedded-test" => "生成板上测试",
        "cargo-embed" => "生成 Embed.toml",
        "wokwi" => "Wokwi 仿真",
        "ci" => "GitHub Actions 持续集成",
        "vscode" => "VS Code 配置",
        "helix" => "Helix 配置",
        "zed" => "Zed 配置",
        "neovim-classic" => "Neovim（lspconfig）",
        "neovim-rustaceanvim" => "Neovim（rustaceanvim）",
        "agents" => "AGENTS.md 指引",
        "claude" => "CLAUDE.md 指引",
        "codex" => "CODEX.md 指引",
        "gemini" => "GEMINI.md 指引",
        "s112" => "BLE / Nordic Softdevice S112",
        "s113" => "BLE / Nordic Softdevice S113",
        "s122" => "BLE 扫描 / Nordic Softdevice S122",
        "s132" => "BLE / Nordic Softdevice S132",
        "s140" => "BLE / Nordic Softdevice S140",
        _ => fallback,
    }
}

pub fn help<'a>(name: &str, fallback: &'a str) -> &'a str {
    if current() == Language::En {
        return fallback;
    }
    match name {
        "module" => "选择具体模组后，生成器会根据模组配置避开保留的 GPIO；此项可不选。",
        "embassy" => "启用 Embassy 异步运行时；ESP 自动启用实验性 HAL。",
        "unstable-hal" => "启用实验性 HAL API。这些功能不保证稳定，未来版本可能修改或移除。",
        "alloc" => {
            "启用并初始化堆内存。ESP 使用 esp-alloc；Cortex-M 使用可用 RAM 的四分之一，上限 16 KiB。"
        }
        "wifi" => {
            "使用 esp-radio 初始化 Wi-Fi。自动启用分配器、实验性 HAL 和 Embassy；网络凭据由应用配置。"
        }
        "ble-trouble" => {
            "使用 esp-radio 和 trouble 初始化 BLE；自动启用分配器、实验性 HAL 和 Embassy。"
        }
        "stack-smashing-protection" => {
            "增加栈保护检查，需要 nightly Rust；Xtensa 使用 esp 工具链。检查会增加运行开销。"
        }
        "probe-rs" => {
            "通过 JTAG 调试接口烧录、监视和调试。RTT 需要 probe-rs；部分芯片需要独立调试探针，USB-UART 转接器不能替代探针。"
        }
        "defmt" => {
            "输出紧凑二进制日志，由 probe-rs 或 espflash 解码。与 log 互斥；取消选择可关闭应用日志。"
        }
        "log" => {
            "输出文本日志。Cortex-M 使用 RTT，ESP 使用 esp-println / espflash；与 defmt 互斥。"
        }
        "panic-halt" => "Panic 时停止执行。",
        "panic-reset" => "Panic 时重启 MCU。",
        "panic-rtt-target" => "通过 RTT 报告 Panic 信息；ESP 会自动启用 probe-rs。",
        "esp-backtrace" => "输出 ESP Panic 回溯信息，使用 espflash 监视。",
        "embedded-test" => {
            "生成 embedded-test 测试程序。运行测试需要开发板及调试探针；生成时不会访问硬件。"
        }
        "cargo-embed" => "生成 cargo-embed 的烧录和 RTT 监视配置。",
        "wokwi" => "生成适用于所选 ESP 芯片的 Wokwi 电路图和仿真配置。",
        "ci" => "生成 GitHub Actions 工作流，检查编译、格式和 Clippy；生成时不会触发远端工作流。",
        "vscode" | "helix" | "zed" | "neovim-classic" | "neovim-rustaceanvim" => {
            "为生成的嵌入式项目配置编辑器及 rust-analyzer；Neovim 的两种集成方式互斥。"
        }
        "agents" | "claude" | "codex" | "gemini" => {
            "生成项目编译和硬件访问指引；只能选择一种指引文件。"
        }
        "s122" => "生成 BLE 中心设备扫描程序。需另行烧录 S122，并在运行前核对 memory.x。",
        "s112" | "s113" | "s132" | "s140" => {
            "生成 BLE GATT 外设程序。需另行烧录对应 Softdevice，并根据 BLE 配置核对 memory.x。"
        }
        _ if fallback.is_empty() => "",
        _ => fallback,
    }
}

pub fn clap_error(error: &clap::Error) -> String {
    use clap::error::{ContextKind, ErrorKind};
    let summary = match error.kind() {
        ErrorKind::UnknownArgument => "无法识别参数",
        ErrorKind::InvalidValue | ErrorKind::ValueValidation => "参数值无效",
        ErrorKind::MissingRequiredArgument => "缺少必要参数",
        ErrorKind::MissingSubcommand | ErrorKind::InvalidSubcommand => "缺少或无法识别子命令",
        ErrorKind::ArgumentConflict => "参数之间存在冲突",
        ErrorKind::TooManyValues => "参数值过多",
        ErrorKind::TooFewValues | ErrorKind::WrongNumberOfValues => "参数值数量不正确",
        _ => "命令行参数解析失败",
    };
    let mut result = format!("错误：{summary}\n");
    for (kind, value) in error.context() {
        let label = match kind {
            ContextKind::InvalidSubcommand => "子命令",
            ContextKind::ValidSubcommand => "可用子命令",
            ContextKind::SuggestedSubcommand => "建议子命令",
            ContextKind::PriorArg => "冲突参数",
            ContextKind::ActualNumValues => "实际值数量",
            ContextKind::ExpectedNumValues => "所需值数量",
            ContextKind::MinValues => "最少值数量",
            ContextKind::Custom => "详细信息",
            ContextKind::InvalidArg => "参数",
            ContextKind::InvalidValue => "输入值",
            ContextKind::ValidValue => "可用值",
            ContextKind::SuggestedArg => "建议参数",
            ContextKind::SuggestedValue => "建议值",
            ContextKind::Usage => "用法",
            _ => continue,
        };
        let rendered = value.to_string();
        let rendered = if kind == ContextKind::Usage {
            rendered.strip_prefix("Usage: ").unwrap_or(&rendered)
        } else {
            &rendered
        };
        result.push_str(&format!("{label}：{rendered}\n"));
    }
    result.push_str("使用 --help 查看帮助。\n");
    result
}

pub fn help_output(rendered: &str) -> String {
    rendered.replace(
        "Print this message or the help of the given subcommand(s)",
        "显示帮助或指定子命令的帮助",
    )
}
