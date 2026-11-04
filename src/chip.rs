use anyhow::{Context, Result, bail};
use probe_rs::config::{CoreType, Registry, Target};
use std::sync::LazyLock;
static REGISTRY: LazyLock<Registry> = LazyLock::new(Registry::from_builtin_families);
pub fn get_target_by_name(name: &str) -> Result<Target> {
    Ok(REGISTRY.get_target_by_name(name)?)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    Stm32,
    Nrf,
    Esp,
}

#[derive(Debug, Clone)]
pub struct Chip {
    pub name: String,
    pub family: Family,
    pub target: String,
    pub hal_feature: String,
}

pub fn normalize(s: &str) -> String {
    s.to_ascii_lowercase().replace('-', "_")
}

impl Chip {
    pub fn cortex(name: &str) -> Result<Self> {
        let target = get_target_by_name(name).context(crate::tr!(
            "Unknown chip in bundled probe-rs database",
            "内置 probe-rs 数据库中没有该芯片"
        ))?;
        let name = target.name;
        let lower = normalize(&name);
        let family = if lower.starts_with("stm32") {
            Family::Stm32
        } else if lower.starts_with("nrf52") {
            Family::Nrf
        } else {
            bail!(crate::tr!(
                "Unsupported Cortex-M family: {name}",
                "不支持的 Cortex-M 系列：{name}"
            ))
        };
        let core = target
            .cores
            .first()
            .context(crate::tr!("Chip has no core", "芯片没有处理器内核信息"))?;
        let triple = match core.core_type {
            CoreType::Armv6m => "thumbv6m-none-eabi",
            CoreType::Armv7m => "thumbv7m-none-eabi",
            CoreType::Armv7em => "thumbv7em-none-eabi",
            CoreType::Armv8m => "thumbv8m.main-none-eabi",
            _ => bail!(crate::tr!(
                "Not a supported Cortex-M chip: {name}",
                "不支持的 Cortex-M 芯片：{name}"
            )),
        };
        let hal_feature = if family == Family::Nrf {
            lower.split('_').next().unwrap_or(&lower).to_owned()
        } else {
            lower
        };
        Ok(Self {
            name,
            family,
            target: triple.into(),
            hal_feature,
        })
    }
}

pub fn cortex_names() -> Result<Vec<String>> {
    let mut names = REGISTRY.search_chips("");
    names.retain(|s| {
        let s = normalize(s);
        s.starts_with("stm32") || s.starts_with("nrf52")
    });
    names.sort();
    names.dedup();
    Ok(names)
}
