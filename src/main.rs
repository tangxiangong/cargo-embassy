mod chip;
mod cli;
mod config;
mod cortex;
mod generate;
mod tui;

use anyhow::{Context, Result};
use clap::Parser;
use cli::{Cli, Command};
use config::Catalog;

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}
fn run() -> Result<()> {
    let mut argv: Vec<_> = std::env::args_os().collect();
    if argv.get(1).is_some_and(|s| s == "embassy") {
        argv.remove(1);
    }
    let cli = Cli::parse_from(argv);
    if matches!(cli.command, Some(Command::Docs)) {
        println!(
            "Embassy: https://embassy.dev/book/\nESP: https://docs.espressif.com/projects/rust/book/"
        );
        return Ok(());
    }
    let catalog = Catalog::new()?;
    let force_tui = matches!(cli.command, Some(Command::Tui(_)));
    let args = match cli.command {
        Some(Command::ListChips { filter }) => {
            for name in &catalog.chips {
                if filter
                    .as_ref()
                    .is_none_or(|f| chip::normalize(name).contains(&chip::normalize(f)))
                {
                    println!("{name}");
                }
            }
            return Ok(());
        }
        Some(Command::ListOptions { chip }) => {
            let p = catalog.project(catalog.chip(&chip)?)?;
            let mut seen = std::collections::BTreeSet::new();
            for o in &p.config.flat_options {
                if !o.name.is_empty()
                    && o.selection_group != "chip"
                    && p.config.is_option_compatible(o)
                    && seen.insert(&o.name)
                {
                    println!(
                        "{}: {} [group: {}; requires: {}]",
                        o.name,
                        o.display_name,
                        o.selection_group,
                        o.requires.join(", ")
                    );
                }
            }
            return Ok(());
        }
        Some(Command::Explain { option, chip }) => {
            let p = catalog.project(catalog.chip(&chip)?)?;
            let o = p
                .config
                .flat_options
                .iter()
                .find(|o| o.name == option && p.config.is_option_compatible(o))
                .context("Unknown or incompatible option")?;
            println!(
                "{}\n{}\n{}\nGroup: {}\nRequires: {}\n",
                o.name,
                o.display_name,
                o.help,
                o.selection_group,
                o.requires.join(", ")
            );
            return Ok(());
        }
        Some(Command::Check { chip }) => {
            let chip = catalog.chip(&chip)?;
            let p = catalog.project(chip.clone())?;
            let names: std::collections::BTreeSet<_> = p
                .config
                .flat_options
                .iter()
                .filter(|o| {
                    !o.name.is_empty()
                        && o.selection_group != "chip"
                        && p.config.is_option_compatible(o)
                })
                .map(|o| o.name.clone())
                .collect();
            let mut failures = vec![];
            for name in names {
                let mut p = catalog.project(chip.clone())?;
                p.name = "embassy-render-check".into();
                let result = p
                    .enable(&name)
                    .and_then(|()| catalog.refresh(&mut p))
                    .and_then(|()| generate::plan(&catalog, &p));
                match result {
                    Ok(files) => println!("OK {name}: {} files", files.len()),
                    Err(e) => failures.push(format!("{name}: {e:#}")),
                }
            }
            anyhow::ensure!(
                failures.is_empty(),
                "Render checks failed:\n{}",
                failures.join("\n")
            );
            return Ok(());
        }
        Some(Command::Init(args)) => args,
        Some(Command::Tui(args)) => args,
        None => cli.args,
        Some(Command::Docs) => unreachable!(),
    };
    let interactive = !args.headless && (force_tui || args.name.is_none() || args.chip.is_none());
    let p = if interactive {
        let Some(p) = tui::run(&catalog, &args)? else {
            return Ok(());
        };
        p
    } else {
        args.project(&catalog)?
    };
    let files = generate::plan(&catalog, &p)?;
    if args.dry_run {
        for path in files.keys() {
            println!("{path}");
        }
    } else {
        let path = generate::write(&p, &files)?;
        println!(
            "Created {} ({} files)\nNext: cd {} && cargo build",
            path.display(),
            files.len(),
            path.display()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests;
