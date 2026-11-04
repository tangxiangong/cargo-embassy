use crate::i18n::{self, text as t};
use crate::{
    cli::GenerateArgs,
    config::{Catalog, Project},
    generate,
};
use anyhow::{Result, ensure};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use esp_generate::template::GeneratorOptionItem;
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};
use std::io::{self, IsTerminal};

struct Restore;
impl Drop for Restore {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, crossterm::cursor::Show);
    }
}
#[derive(PartialEq)]
enum Page {
    Setup,
    Options,
    Review,
}

pub fn run(catalog: &Catalog, seed: &GenerateArgs) -> Result<Option<Project>> {
    ensure!(
        io::stdin().is_terminal() && io::stdout().is_terminal(),
        crate::tr!(
            "TUI requires an interactive terminal; use init <name> --chip <chip> --headless",
            "TUI 需要交互式终端；脚本请使用 init <name> --chip <chip> --headless"
        )
    );
    let mut name = seed.name.clone().unwrap_or_else(|| "my-project".into());
    let mut output = if seed.output_path.is_empty() {
        ".".into()
    } else {
        seed.output_path.clone()
    };
    let mut toolchain = seed.toolchain.clone().unwrap_or_default();
    let mut query = seed.chip.clone().unwrap_or_default();
    let mut toolchains = vec![String::new()];
    if let Ok(result) = std::process::Command::new("rustup")
        .args(["toolchain", "list"])
        .output()
        && result.status.success()
    {
        toolchains.extend(
            String::from_utf8_lossy(&result.stdout)
                .lines()
                .filter_map(|line| line.split_whitespace().next())
                .map(String::from),
        );
    }
    let mut selected_chip = 0usize;
    let mut focus = 0usize;
    let mut page = Page::Setup;
    let mut project: Option<Project> = None;
    let mut path: Vec<usize> = vec![];
    let mut selected = 0usize;
    let mut message = String::new();
    let mut preview = vec![];
    enable_raw_mode()?;
    let _restore = Restore;
    execute!(io::stdout(), EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    loop {
        let matches: Vec<_> = catalog
            .chips
            .iter()
            .filter(|s| crate::chip::normalize(s).contains(&crate::chip::normalize(&query)))
            .collect();
        selected_chip = selected_chip.min(matches.len().saturating_sub(1));
        let rows = project.as_ref().map(|p| rows(p, &path)).unwrap_or_default();
        if page == Page::Options {
            selected = selected.min(rows.len().saturating_sub(1));
        }
        terminal.draw(|f| {
            let area = f.area();
            if area.width < 60 || area.height < 20 { f.render_widget(Paragraph::new(t("Resize terminal to at least 60 x 20.\nEsc / Ctrl-C: cancel", "请将终端调整至至少 60 × 20。\nEsc / Ctrl-C：取消")).wrap(Wrap { trim: false }),area); return; }
            let layout = Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(3),Constraint::Min(5),Constraint::Length(5),Constraint::Length(2)]).split(area);
            let title = match page { Page::Setup => t("1 / Project and chip", "1 / 项目与芯片"), Page::Options => t("2 / Configure features", "2 / 功能配置"), Page::Review => t("3 / Review and generate", "3 / 预览并生成") };
            f.render_widget(Paragraph::new(format!("  CARGO EMBASSY    |    {title}\n  F3: 中文 / English")).style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)).block(Block::default().borders(Borders::BOTTOM)),layout[0]);
            let mut help = String::new();
            match page {
                Page::Setup => {
                    let body = Layout::default().constraints([Constraint::Length(5),Constraint::Min(1)]).split(layout[1]);
                    let values = [crate::tr!("Project name: {name}", "项目名称：{name}"),crate::tr!("Chip search: {query}", "芯片筛选：{query}"),crate::tr!("Output parent: {output}", "输出父目录：{output}"),crate::tr!("Toolchain: {}", "工具链：{}",if toolchain.is_empty() {t("auto", "自动")} else {&toolchain})];
                    let text = values.iter().enumerate().map(|(i,v)| format!("{} {v}",if focus==i {">"} else {" "})).collect::<Vec<_>>().join("\n");
                    f.render_widget(Paragraph::new(text),body[0]);
                    let list = List::new(matches.iter().map(|s| ListItem::new(s.as_str()))).block(Block::default().borders(Borders::ALL).title(crate::tr!("Chips — {} matches", "芯片 — {} 个匹配项",matches.len()))).highlight_style(Style::default().fg(Color::Black).bg(Color::Cyan)).highlight_symbol("> ");
                    let mut state = ListState::default().with_selected(Some(selected_chip));
                    f.render_stateful_widget(list,body[1],&mut state);
                    help = t("Type to edit the focused field. Tab changes fields. Up/Down selects a chip. Enter opens options. Toolchain: Left/Right cycles installed toolchains; auto uses stable / esp, or nightly for stack protection.", "直接输入以编辑当前字段；Tab 切换字段，上下键选择芯片，Enter 进入配置。工具链字段可用左右键切换本机已安装项；自动模式选择 stable / esp，启用栈保护时选择 nightly。").into();
                }
                Page::Options => if let Some(p) = &project {
                    let list = List::new(rows.iter().map(|(_,item)| {
                        let (prefix, available) = match item { GeneratorOptionItem::Category(_) => (">",true),GeneratorOptionItem::Option(o) => (if p.has(&o.name) {"[x]"} else {"[ ]"},p.config.is_option_compatible(o)) };
                        ListItem::new(format!("{prefix} {}{}",i18n::title(item.name(), item.title()),if available {""} else {t(" (unavailable)", "（不可用）")})).style(if available {Style::default()} else {Style::default().fg(Color::DarkGray)})
                    })).block(Block::default().borders(Borders::ALL).title(format!("{} / {}",p.chip.name,if path.is_empty() {t("Options", "选项")} else {t("Category", "分类")}))).highlight_style(Style::default().fg(Color::Black).bg(Color::Cyan)).highlight_symbol("> ");
                    let mut state = ListState::default().with_selected(Some(selected));
                    f.render_stateful_widget(list,layout[1],&mut state);
                    if let Some((_,item)) = rows.get(selected) {
                        help = i18n::title(item.name(), item.title()).to_owned();
                        if let GeneratorOptionItem::Option(o) = item {
                            let evicted = p.config.would_force_deselect(o);
                            if evicted.iter().any(|name| name == "embassy") { help.push_str(t("\nRequired by Embassy; cannot be disabled.", "\nEmbassy 依赖此项，不能关闭。")); } else if !evicted.is_empty() { help.push_str(&crate::tr!("\nWill clear: {}", "\n将取消：{}", evicted.join(", "))); }
                            if !o.requires.is_empty() { help.push_str(&crate::tr!("\nRequires: {}", "\n依赖：{}", o.requires.join(", "))); }
                        }
                        let description = i18n::help(item.name(), item.help());
                        if !description.is_empty() { help.push_str(&format!("\n{description}")); }
                    }
                },
                Page::Review => if let Some(p) = &project {
                    let text = crate::tr!("Project: {}\nDestination: {}/{}\nChip: {}\nOptions: {}\n\nFiles ({}):\n{}", "项目：{}\n位置：{}/{}\n芯片：{}\n选项：{}\n\n文件（{}）：\n{}",p.name,p.output,p.name,p.chip.name,p.selected().join(", "),preview.len(),preview.join("\n"));
                    f.render_widget(Paragraph::new(text).wrap(Wrap{trim:false}).scroll((selected as u16,0)).block(Block::default().borders(Borders::ALL).title(t("Nothing has been written yet", "尚未写入任何文件"))),layout[1]);
                    help = t("Enter generates exactly this project. Esc returns to options. Ctrl-C cancels without writing. Up/Down scrolls the preview.", "Enter 生成当前项目；Esc 返回修改；Ctrl-C 取消且不写入文件；上下键滚动预览。").into();
                },
            }
            f.render_widget(Paragraph::new(if message.is_empty() {help} else {message.clone()}).wrap(Wrap{trim:false}).style(Style::default().fg(if message.is_empty(){Color::Gray}else{Color::Yellow})).block(Block::default().borders(Borders::TOP)),layout[2]);
            f.render_widget(Paragraph::new(match page {Page::Setup=>t("Tab: field  ↑↓: chip  Enter: configure  Ctrl-C/Esc: cancel", "Tab：字段  ↑↓：芯片  Enter：配置  Ctrl-C/Esc：取消"),Page::Options=>t("↑↓: move  Enter/Space: select  Esc: back  F2: review  Ctrl-C: cancel", "↑↓：移动  Enter/空格：选择  Esc：返回  F2：预览  Ctrl-C：取消"),Page::Review=>t("Enter: generate  ↑↓: scroll  Esc: edit  Ctrl-C: cancel", "Enter：生成  ↑↓：滚动  Esc：修改  Ctrl-C：取消")}).wrap(Wrap { trim: false }),layout[3]);
        })?;
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind == KeyEventKind::Release {
            continue;
        }
        if key.code == KeyCode::F(3) {
            i18n::set(if i18n::current() == i18n::Language::En {
                i18n::Language::Zh
            } else {
                i18n::Language::En
            });
            message.clear();
            continue;
        }
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return Ok(None);
        }
        let size = terminal.size()?;
        if size.width < 60 || size.height < 20 {
            if key.code == KeyCode::Esc {
                return Ok(None);
            }
            continue;
        }
        message.clear();
        match page {
            Page::Setup => match key.code {
                KeyCode::Esc => return Ok(None),
                KeyCode::Tab => focus = (focus + 1) % 4,
                KeyCode::BackTab => focus = (focus + 3) % 4,
                KeyCode::Left | KeyCode::Right if focus == 3 => {
                    let index = toolchains.iter().position(|t| t == &toolchain).unwrap_or(0);
                    let index = if key.code == KeyCode::Left {
                        (index + toolchains.len() - 1) % toolchains.len()
                    } else {
                        (index + 1) % toolchains.len()
                    };
                    toolchain = toolchains[index].clone();
                }
                KeyCode::Up => selected_chip = selected_chip.saturating_sub(1),
                KeyCode::Down => {
                    selected_chip = (selected_chip + 1).min(matches.len().saturating_sub(1))
                }
                KeyCode::Enter => {
                    let result = (|| -> Result<Project> {
                        let chip_name = matches.get(selected_chip).ok_or_else(|| {
                            anyhow::anyhow!(crate::tr!("No matching chip", "没有匹配的芯片"))
                        })?;
                        let mut p = catalog.project(catalog.chip(chip_name)?)?;
                        let same_chip = project
                            .as_ref()
                            .is_some_and(|old| old.chip.name == p.chip.name);
                        if same_chip {
                            if let Some(old) = &project {
                                p.config.selected = old.config.selected.clone();
                            }
                        } else {
                            seed.apply(&mut p, catalog)?;
                        }
                        p.name = name.clone();
                        p.output = output.clone();
                        p.toolchain = toolchain.clone();
                        catalog.refresh(&mut p)?;
                        p.validate()?;
                        Ok(p)
                    })();
                    match result {
                        Ok(p) => {
                            project = Some(p);
                            page = Page::Options;
                            path.clear();
                            selected = 0;
                        }
                        Err(e) => message = e.to_string(),
                    }
                }
                KeyCode::Backspace => match focus {
                    0 => {
                        name.pop();
                    }
                    1 => {
                        query.pop();
                        selected_chip = 0;
                    }
                    2 => {
                        output.pop();
                    }
                    _ => {
                        toolchain.pop();
                    }
                },
                KeyCode::Char(c)
                    if !c.is_control()
                        && !key
                            .modifiers
                            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    match focus {
                        0 => name.push(c),
                        1 => {
                            query.push(c);
                            selected_chip = 0;
                        }
                        2 => output.push(c),
                        _ => toolchain.push(c),
                    }
                }
                _ => {}
            },
            Page::Options => match key.code {
                KeyCode::Up => selected = selected.saturating_sub(1),
                KeyCode::Down => selected = (selected + 1).min(rows.len().saturating_sub(1)),
                KeyCode::Esc | KeyCode::Left => {
                    if path.pop().is_none() {
                        page = Page::Setup;
                    }
                    selected = 0;
                }
                KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Right => {
                    if let Some((index, item)) = rows.get(selected) {
                        match item {
                            GeneratorOptionItem::Category(_) => {
                                path.push(*index);
                                selected = 0;
                            }
                            GeneratorOptionItem::Option(o) => {
                                if let Some(p) = project.as_mut()
                                    && let Err(e) =
                                        p.toggle(&o.name).and_then(|()| catalog.refresh(p))
                                {
                                    message = e.to_string();
                                }
                            }
                        }
                    }
                }
                KeyCode::F(2) => {
                    if let Some(p) = &project {
                        match generate::plan(catalog, p) {
                            Ok(files) => {
                                preview = files.into_keys().collect();
                                page = Page::Review;
                                selected = 0;
                            }
                            Err(e) => message = format!("{e:#}"),
                        }
                    }
                }
                _ => {}
            },
            Page::Review => match key.code {
                KeyCode::Enter => return Ok(project),
                KeyCode::Esc => {
                    page = Page::Options;
                    selected = 0;
                }
                KeyCode::Up => selected = selected.saturating_sub(1),
                KeyCode::Down => selected = (selected + 1).min(preview.len() + 8),
                _ => {}
            },
        }
    }
}

fn rows(p: &Project, path: &[usize]) -> Vec<(usize, GeneratorOptionItem)> {
    let mut items = &p.config.options;
    for index in path {
        if let Some(GeneratorOptionItem::Category(c)) = items.get(*index) {
            items = &c.options;
        }
    }
    items
        .iter()
        .enumerate()
        .filter(|(_, item)| visible(p, item))
        .map(|(i, item)| (i, item.clone()))
        .collect()
}

fn visible(p: &Project, item: &GeneratorOptionItem) -> bool {
    match item {
        GeneratorOptionItem::Option(o) => {
            !o.name.is_empty()
                && o.selection_group != "chip"
                && o.selection_group != "base-template"
                && p.config.is_option_compatible(o)
        }
        GeneratorOptionItem::Category(c) => {
            c.name != "toolchain"
                && c.name != "chip"
                && c.requires.iter().all(|requirement| {
                    if let Some(name) = requirement.strip_prefix('!') {
                        !p.has(name)
                    } else {
                        p.has(requirement)
                    }
                })
                && c.options.iter().any(|item| visible(p, item))
        }
    }
}
