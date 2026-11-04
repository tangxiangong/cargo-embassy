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
        let chip = catalog.chip(
            self.chip
                .as_deref()
                .context("--chip is required in headless mode")?,
        )?;
        let mut p = catalog.project(chip)?;
        p.name = self
            .name
            .clone()
            .context("Project name is required in headless mode")?;
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
                "Requested option '{name}' conflicts with another requested option"
            );
        }
        match self.logging {
            Some(Logging::Defmt) => ensure!(
                p.has("defmt"),
                "--logging defmt conflicts with selected options"
            ),
            Some(Logging::Log) => ensure!(
                p.has("log"),
                "--logging log conflicts with selected options"
            ),
            Some(Logging::None) => ensure!(
                !p.has("defmt") && !p.has("log"),
                "--logging none conflicts with selected options"
            ),
            _ => {}
        }
        if let Some(sd) = &self.softdevice {
            ensure!(
                p.has(&sd.to_ascii_lowercase()),
                "--softdevice conflicts with selected options"
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
                "--panic-handler conflicts with selected options"
            );
        }
        catalog.refresh(p)?;
        Ok(())
    }
}
