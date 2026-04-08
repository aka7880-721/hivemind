// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyVersionBumper License v1.2
//
// For details, see the LICENSE file in the repository root.

use std::{
    io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use anyhow::{Context, Result, anyhow, bail};
use arboard::Clipboard;
use crossterm::{
    event::{
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste,
        EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton,
        MouseEvent, MouseEventKind,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Flex, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};
use ratatui_comfy_toaster::{
    ToastBuilder, ToastEngine, ToastEngineBuilder, ToastInteraction, ToastMouseButton,
    ToastShortcut, ToastType,
};
use ratatui_explorer::{FileExplorer, FileExplorerBuilder};
use tui_tabs::TabNav;

use crate::{
    branding::{PixelLogo, choose_header_content},
    config::{
        AppConfig, BranchConfig, ConfigStore, IntegrationMode, ProjectConfig, ProjectType,
        RepoConfig, TargetFormat, TargetSpec,
    },
    dialogs::{BumpDialog, RecentChangesDialog, RecentChangesTab, TagDialog, TagAction, TextInput},
    git::{ensure_gh_available, ensure_local_tag, run_gh_checked, run_git_checked},
    targets::{ProbeKind, TargetProbe, probe_target, write_target_version},
    ui::{center_vertically, centered_rect},
    versioning::VersionScheme,
};

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
const SUPPORT_EMAIL: &str = " dev@comfyhome.io ";
const FORM_LABEL_WIDTH: u16 = 18;
const BROWSE_BUTTON_WIDTH: u16 = 12;
const BUTTON_ROW_HEIGHT: u16 = 3;
const BUTTON_GAP_HEIGHT: u16 = 3;

pub fn run() -> Result<()> {
    let mut terminal = setup_terminal()?;
    let result = run_app(&mut terminal);
    restore_terminal(&mut terminal)?;
    result
}

fn setup_terminal() -> Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode().context("failed to enable raw mode")?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture, EnableBracketedPaste)
        .context("failed to enter the alternate screen")?;
    Terminal::new(CrosstermBackend::new(stdout)).context("failed to create terminal")
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    disable_raw_mode().context("failed to disable raw mode")?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture,
        DisableBracketedPaste
    )
        .context("failed to leave the alternate screen")?;
    terminal.show_cursor().context("failed to show the cursor")?;
    Ok(())
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    let mut app = App::new()?;

    while !app.should_quit {
        terminal.draw(|frame| app.draw(frame))?;

        if event::poll(Duration::from_millis(200)).context("event polling failed")? {
            match event::read().context("event read failed")? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    if let Err(error) = app.handle_key(key) {
                        app.status = StatusMessage::error(error.to_string());
                    }
                }
                Event::Mouse(mouse) => app.handle_mouse(mouse),
                Event::Paste(text) => app.handle_paste(text),
                Event::Resize(_, _) => {}
                Event::FocusGained | Event::FocusLost => {}
                Event::Key(_) => {}
            }
        }
    }

    Ok(())
}

struct App {
    config_store: ConfigStore,
    config: AppConfig,
    screen: Screen,
    selected_project: usize,
    wizard: ProjectWizard,
    bump_dialog: Option<BumpDialog>,
    recent_changes_dialog: Option<RecentChangesDialog>,
    tag_dialog: Option<TagDialog>,
    project_edit_dialog: Option<ProjectEditDialog>,
    browser_dialog: Option<FileBrowserDialog>,
    hit_targets: Vec<HitTarget>,
    status: StatusMessage,
    last_status_toast_id: u64,
    transient_toaster: ToastEngine<()>,
    sticky_toaster: ToastEngine<()>,
    logo: PixelLogo,
    should_quit: bool,
}

impl App {
    fn new() -> Result<Self> {
        let config_store = ConfigStore::locate()?;
        let config = config_store.load()?;
        let status = StatusMessage::info("Press N to create your first project, or Q to quit.");
        Ok(Self {
            config_store,
            config,
            screen: Screen::Dashboard,
            selected_project: 0,
            wizard: ProjectWizard::default(),
            bump_dialog: None,
            recent_changes_dialog: None,
            tag_dialog: None,
            project_edit_dialog: None,
            browser_dialog: None,
            hit_targets: Vec::new(),
            last_status_toast_id: status.id,
            transient_toaster: ToastEngineBuilder::new(Rect::default())
                .default_duration(Duration::from_secs(2))
                .build(),
            sticky_toaster: ToastEngineBuilder::new(Rect::default())
                .default_duration(Duration::from_secs(2))
                .build(),
            status,
            logo: PixelLogo::load(),
            should_quit: false,
        })
    }

    fn draw(&mut self, frame: &mut Frame) {
        self.transient_toaster.tick();
        self.sticky_toaster.tick();
        self.sync_status_toasts();
        self.hit_targets.clear();

        let root = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(12),
                Constraint::Length(3),
                Constraint::Min(12),
                Constraint::Length(3),
            ])
            .split(frame.area());

        self.render_header(frame, root[0]);
        self.render_nav(frame, root[1]);

        match self.screen {
            Screen::Dashboard => self.render_dashboard(frame, root[2]),
            Screen::Wizard => self.render_wizard(frame, root[2]),
            Screen::Settings => self.render_settings(frame, root[2]),
            Screen::UiSettings => self.render_ui_settings(frame, root[2]),
        }

        if self.bump_dialog.is_some() {
            self.render_bump_dialog(frame, frame.area());
        }
        if self.recent_changes_dialog.is_some() {
            self.render_recent_changes_dialog(frame, frame.area());
        }
        if self.tag_dialog.is_some() {
            self.render_tag_dialog(frame, frame.area());
        }
        if self.project_edit_dialog.is_some() {
            self.render_project_edit_dialog(frame, frame.area());
        }
        if self.browser_dialog.is_some() {
            self.render_browser_dialog(frame, frame.area());
        }

        self.render_footer(frame, root[3]);
        self.transient_toaster.set_area(frame.area());
        self.sticky_toaster.set_area(frame.area());
        frame.render_widget(&self.transient_toaster, frame.area());
        frame.render_widget(&self.sticky_toaster, frame.area());
    }

    fn render_header(&mut self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" © 2026 ComfyHome™ " )
            .border_style(Style::default().fg(Color::Cyan));
        let inner = block.inner(area);
        frame.render_widget(block, area);
        self.render_header_contact(frame, area);

        let logo = self.logo.render(inner.height);
        let header = choose_header_content(inner.width, logo.width(), &format!("v{}", APP_VERSION));

        if header.show_logo() {
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Fill(1),
                    Constraint::Length(header.logo_margin()),
                    Constraint::Length(logo.width()),
                    Constraint::Length(header.logo_gap()),
                    Constraint::Length(header.banner().width()),
                    Constraint::Fill(1),
                ])
                .flex(Flex::Center)
                .split(inner);

            let logo_area = center_vertically(chunks[2], logo.lines().len() as u16);
            frame.render_widget(Paragraph::new(logo.lines().to_vec()), logo_area);

            let banner_area = center_vertically(chunks[4], header.banner().lines().len() as u16);
            frame.render_widget(Paragraph::new(header.banner().lines().to_vec()), banner_area);
        } else {
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Fill(1),
                    Constraint::Length(header.banner().width()),
                    Constraint::Fill(1),
                ])
                .flex(Flex::Center)
                .split(inner);
            let banner_area = center_vertically(chunks[1], header.banner().lines().len() as u16);
            frame.render_widget(Paragraph::new(header.banner().lines().to_vec()), banner_area);
        }
    }

    fn render_header_contact(&self, frame: &mut Frame, area: Rect) {
        if area.width <= SUPPORT_EMAIL.len() as u16 + 4 {
            return;
        }

        let contact_width = SUPPORT_EMAIL.len() as u16;
        let contact_area = Rect {
            x: area.x + area.width.saturating_sub(contact_width + 2),
            y: area.y,
            width: contact_width,
            height: 1,
        };
        frame.render_widget(
            Paragraph::new(SUPPORT_EMAIL).style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            contact_area,
        );
    }

    fn render_nav(&mut self, frame: &mut Frame, area: Rect) {
        let sections = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(10), Constraint::Length(12)])
            .split(area);

        let labels = self.main_tab_labels();
        let label_refs = labels.iter().map(String::as_str).collect::<Vec<_>>();
        let tabs = TabNav::new(&label_refs, self.current_main_tab_index())
            .highlight_style(Style::default().fg(Color::Cyan))
            .border_style(Style::default().fg(Color::DarkGray))
            .style(Style::default().fg(Color::White))
            .indicator(None);
        frame.render_widget(tabs, sections[0]);

        let widths = labels
            .iter()
            .map(|label| Constraint::Length(label.chars().count() as u16 + 6))
            .collect::<Vec<_>>();
        let layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(widths)
            .split(sections[0]);
        for (index, rect) in layout.iter().enumerate() {
            self.hit_targets.push(HitTarget::new(*rect, HitAction::Switch(main_screen_from_index(index))));
        }

        frame.render_widget(
            Paragraph::new(" Q Quit ")
                .style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))
                .alignment(Alignment::Center),
            sections[1],
        );
        self.hit_targets.push(HitTarget::new(sections[1], HitAction::Quit));
    }

    fn render_dashboard(&mut self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(38), Constraint::Min(40)])
            .split(area);

        let left_block = Block::default().borders(Borders::ALL).title(" Projects ");
        let left_inner = left_block.inner(chunks[0]);
        frame.render_widget(left_block, chunks[0]);

        if self.config.projects.is_empty() {
            let onboarding = vec![
                Line::from("No projects have been configured yet.".bold()),
                Line::from(""),
                Line::from("Press N or click New Project to start onboarding."),
                Line::from("This first slice stores config in your user profile."),
                Line::from("Branched projects are scaffolded with an initial branch."),
                Line::from("Target validation reads JSON or TOML and checks a key path."),
            ];
            frame.render_widget(Paragraph::new(onboarding).wrap(Wrap { trim: false }), left_inner);
        } else {
            let items = self
                .config
                .projects
                .iter()
                .map(|project| ListItem::new(vec![
                    Line::from(project.name.clone()).style(Style::default().add_modifier(Modifier::BOLD)),
                    Line::from(project.summary()).style(Style::default().fg(Color::Gray)),
                ]))
                .collect::<Vec<_>>();
            let mut state = ListState::default();
            state.select(Some(self.selected_project.min(self.config.projects.len().saturating_sub(1))));
            let list = List::new(items)
                .highlight_style(Style::default().bg(Color::Blue).fg(Color::Black))
                .highlight_symbol("> ");
            frame.render_stateful_widget(list, left_inner, &mut state);

            let row_height = 2_u16;
            for (index, _) in self.config.projects.iter().enumerate() {
                let rect = Rect {
                    x: left_inner.x,
                    y: left_inner.y + index as u16 * row_height,
                    width: left_inner.width,
                    height: row_height,
                };
                if rect.y < left_inner.y + left_inner.height {
                    self.hit_targets.push(HitTarget::new(rect, HitAction::SelectProject(index)));
                }
            }
        }

        let right_block = Block::default().borders(Borders::ALL).title(" Project Detail ");
        let right_inner = right_block.inner(chunks[1]);
        frame.render_widget(right_block, chunks[1]);

        let lines = if let Some(project) = self.config.projects.get(self.selected_project) {
            let mut lines = project
                .detail_lines()
                .into_iter()
                .map(Line::from)
                .collect::<Vec<_>>();
            lines.push(Line::raw(""));
            lines.push(Line::from("Available actions:".yellow().bold()));
            lines.push(Line::from("- B opens a bump preview for the selected project"));
            if project.integration_mode.requires_repo() {
                lines.push(Line::from("- V opens view changes from the configured repo"));
                lines.push(Line::from("- T creates a local tag in the configured repo"));
            } else {
                lines.push(Line::from("- git actions unlock once the project is git-backed"));
            }
            lines.push(Line::from("- the current slice writes JSON and TOML targets"));
            lines
        } else {
            vec![
                Line::from("Supported schemes:".bold()),
                Line::from("- SemVer MAJOR.MINOR.PATCH → e.g. 1.4.2"),
                Line::from("- CalVer YYYY.MM.Micro → e.g. 2024.06.3"),
                Line::from("- CalVer YY.MM.Micro → e.g. 24.06.3"),
                Line::from("- CalVer YYYY.MM.DD.Micro → e.g. 2024.06.15.3"),
                Line::from("- Hybrid YYYY.MINOR.PATCH → e.g. 2024.4.2"),
                Line::from("- Hybrid YYYY.PATCH → e.g. 2024.2"),
            ]
        };
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), right_inner);
    }

    fn render_bump_dialog(&mut self, frame: &mut Frame, area: Rect) {
        let Some(dialog) = &self.bump_dialog else {
            return;
        };

        let popup = centered_rect(area, 74, 58);
        frame.render_widget(Clear, popup);
        let block = Block::default().borders(Borders::ALL).title(" Bump Preview ").border_style(Style::default().fg(Color::Cyan));
        let inner = block.inner(popup);
        frame.render_widget(block, popup);

        let sections = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(7),
                Constraint::Min(6),
                Constraint::Length(3),
            ])
            .split(inner);

        let next_version = dialog.preview_next_version();
        let mut summary = vec![
            Line::from(format!("Project: {}", dialog.project_name)).bold(),
            Line::from(format!("Scheme: {}", dialog.scheme.display_name())),
            Line::from(format!("Current version: {}", dialog.current_version)),
            Line::from(format!("Action: < {} >", dialog.selected_action().display_name())).style(Style::default().fg(Color::Yellow)),
        ];
        match next_version {
            Ok(next_version) => summary.push(Line::from(format!("Next version: {}", next_version)).style(Style::default().fg(Color::Green).add_modifier(Modifier::BOLD))),
            Err(error) => summary.push(Line::from(format!("Next version: {}", error)).style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))),
        }
        summary.push(Line::from("Left/Right changes the action. Enter applies it to every listed target."));
        frame.render_widget(Paragraph::new(summary).wrap(Wrap { trim: false }), sections[0]);

        let target_lines = dialog
            .targets
            .iter()
            .flat_map(|target| {
                [
                    Line::from(target.label.clone()).style(Style::default().add_modifier(Modifier::BOLD)),
                    Line::from(format!("  {} -> {} [{}]", target.path, target.key_path, target.format.display_name())),
                ]
            })
            .collect::<Vec<_>>();
        let target_block = Block::default().borders(Borders::ALL).title(" Targets ");
        let target_inner = target_block.inner(sections[1]);
        frame.render_widget(target_block, sections[1]);
        frame.render_widget(Paragraph::new(target_lines).wrap(Wrap { trim: false }), target_inner);

        let button_row = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(20), Constraint::Length(20), Constraint::Min(10)])
            .split(sections[2]);
        let action_rect = button_row[0];
        let apply_rect = button_row[1];
        let cancel_rect = button_row[2];
        frame.render_widget(Paragraph::new(format!(" < {} > ", dialog.selected_action().display_name())).block(Block::default().borders(Borders::ALL).title(" action ")).style(Style::default().fg(Color::Yellow)), action_rect);
        frame.render_widget(Paragraph::new(" Apply ").block(Block::default().borders(Borders::ALL).title(" action ")).style(Style::default().fg(Color::Green)), apply_rect);
        frame.render_widget(Paragraph::new(" Cancel ").block(Block::default().borders(Borders::ALL).title(" action ")).style(Style::default().fg(Color::Red)), cancel_rect);
        self.hit_targets.push(HitTarget::new(action_rect, HitAction::CycleBumpAction(1)));
        self.hit_targets.push(HitTarget::new(apply_rect, HitAction::ApplyBump));
        self.hit_targets.push(HitTarget::new(cancel_rect, HitAction::CancelBump));
    }

    fn render_recent_changes_dialog(&mut self, frame: &mut Frame, area: Rect) {
        let Some(dialog) = &self.recent_changes_dialog else {
            return;
        };

        let popup = centered_rect(area, 82, 72);
        frame.render_widget(Clear, popup);
        let block = Block::default().borders(Borders::ALL).title(" Git Commits ").border_style(Style::default().fg(Color::Cyan));
        let inner = block.inner(popup);
        frame.render_widget(block, popup);

        let sections = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(4), Constraint::Length(3), Constraint::Min(8), Constraint::Length(3)])
            .split(inner);

        let header = vec![
            Line::from(format!("Project: {}", dialog.project_name)).bold(),
            Line::from(format!("Repo: {}", dialog.repo_root)),
            Line::from(format!("View: {}", dialog.current_range().label)),
            Line::from("Tab switches view. Left/Right moves history when History is active."),
        ];
        frame.render_widget(Paragraph::new(header).wrap(Wrap { trim: false }), sections[0]);

        let tab_labels = ["Recent Changes", "History"];
        let tab_index = if dialog.active_tab == RecentChangesTab::Recent { 0 } else { 1 };
        let tabs = TabNav::new(&tab_labels, tab_index)
            .highlight_style(Style::default().fg(Color::Cyan))
            .border_style(Style::default().fg(Color::DarkGray))
            .style(Style::default().fg(Color::White))
            .indicator(None);
        frame.render_widget(tabs, sections[1]);

        let tab_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(22), Constraint::Length(14)])
            .split(sections[1]);
        self.hit_targets.push(HitTarget::new(tab_layout[0], HitAction::SelectRecentChangesTab(RecentChangesTab::Recent)));
        self.hit_targets.push(HitTarget::new(tab_layout[1], HitAction::SelectRecentChangesTab(RecentChangesTab::History)));

        let body_block = Block::default().borders(Borders::ALL).title(" git log ");
        let body_inner = body_block.inner(sections[2]);
        frame.render_widget(body_block, sections[2]);
        let body = if dialog.current_range().lines.is_empty() {
            vec![Line::from(if dialog.active_tab == RecentChangesTab::History {
                "No history range is available yet."
            } else {
                "No recent changes to display."
            })]
        } else {
            dialog.current_range().lines.iter().cloned().map(Line::from).collect::<Vec<_>>()
        };
        frame.render_widget(
            Paragraph::new(body)
                .wrap(Wrap { trim: false })
                .scroll((dialog.scroll, 0)),
            body_inner,
        );

        let footer = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(18), Constraint::Length(18), Constraint::Min(10)])
            .split(sections[3]);
        frame.render_widget(
            Paragraph::new(" Scroll ").block(Block::default().borders(Borders::ALL).title(" action ")).style(Style::default().fg(Color::Yellow)),
            footer[0],
        );
        frame.render_widget(
            Paragraph::new(" Create Tag ").block(Block::default().borders(Borders::ALL).title(" action ")).style(Style::default().fg(Color::Green)),
            footer[1],
        );
        frame.render_widget(
            Paragraph::new(" Close ").block(Block::default().borders(Borders::ALL).title(" action ")).style(Style::default().fg(Color::Red)),
            footer[2],
        );
        self.hit_targets.push(HitTarget::new(footer[0], HitAction::ScrollRecentChanges(3)));
        self.hit_targets.push(HitTarget::new(footer[1], HitAction::OpenTagDialog));
        self.hit_targets.push(HitTarget::new(footer[2], HitAction::CloseRecentChanges));
    }

    fn render_tag_dialog(&mut self, frame: &mut Frame, area: Rect) {
        let Some(dialog) = &self.tag_dialog else {
            return;
        };

        let popup = centered_rect(area, 70, 34);
        frame.render_widget(Clear, popup);
        let block = Block::default().borders(Borders::ALL).title(" Create Local Tag ").border_style(Style::default().fg(Color::Cyan));
        let inner = block.inner(popup);
        frame.render_widget(block, popup);

        let sections = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(4), Constraint::Length(3), Constraint::Min(4), Constraint::Length(3)])
            .split(inner);

        let header = vec![
            Line::from(format!("Project: {}", dialog.project_name)).bold(),
            Line::from(format!("Repo: {}", dialog.repo_root)),
            Line::from(format!("Action: < {} >", dialog.selected_action().display_name())).style(Style::default().fg(Color::Yellow)),
            Line::from("Edit the tag name, then press Enter to run the selected action."),
        ];
        frame.render_widget(Paragraph::new(header).wrap(Wrap { trim: false }), sections[0]);

        let input_row = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(20), Constraint::Min(10)])
            .split(sections[1]);
        frame.render_widget(Paragraph::new("Tag name"), input_row[0]);
        let input_block = Block::default()
            .borders(Borders::ALL)
            .title(" value ")
            .border_style(Style::default().fg(Color::Cyan));
        frame.render_widget(
            Paragraph::new(dialog.tag_name.display_value(true))
                .block(input_block)
                .style(Style::default().fg(Color::White)),
            input_row[1],
        );

        let notes = vec![
            Line::from(format!("Suggested tag: {}", dialog.tag_name.value)),
            Line::from(match dialog.selected_action() {
                TagAction::CreateLocal => "Creates a local tag only.",
                TagAction::CreateAndPush => "Creates the local tag if needed, then pushes it.",
                TagAction::CreatePushAndRelease => "Creates the tag, pushes it, then runs `gh release create --generate-notes`.",
            }),
        ];
        frame.render_widget(Paragraph::new(notes).wrap(Wrap { trim: false }), sections[2]);

        let footer = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(18), Constraint::Length(18), Constraint::Min(10)])
            .split(sections[3]);
        frame.render_widget(
            Paragraph::new(format!(" < {} > ", dialog.selected_action().display_name())).block(Block::default().borders(Borders::ALL).title(" action ")).style(Style::default().fg(Color::Yellow)),
            footer[0],
        );
        frame.render_widget(
            Paragraph::new(" Run ").block(Block::default().borders(Borders::ALL).title(" action ")).style(Style::default().fg(Color::Green)),
            footer[1],
        );
        frame.render_widget(
            Paragraph::new(" Cancel ").block(Block::default().borders(Borders::ALL).title(" action ")).style(Style::default().fg(Color::Red)),
            footer[2],
        );
        self.hit_targets.push(HitTarget::new(footer[0], HitAction::CycleTagAction(1)));
        self.hit_targets.push(HitTarget::new(footer[1], HitAction::CreateTag));
        self.hit_targets.push(HitTarget::new(footer[2], HitAction::CancelTagDialog));
    }

    fn render_settings(&mut self, frame: &mut Frame, area: Rect) {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(38), Constraint::Min(40)])
            .split(area);

        let left_block = Block::default().borders(Borders::ALL).title(" Projects ");
        let left_inner = left_block.inner(columns[0]);
        frame.render_widget(left_block, columns[0]);

        if self.config.projects.is_empty() {
            let lines = vec![
                Line::from("No saved projects yet.".bold()),
                Line::from("Press N to create one before editing settings."),
            ];
            frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), left_inner);
        } else {
            let items = self
                .config
                .projects
                .iter()
                .map(|project| ListItem::new(vec![
                    Line::from(project.name.clone()).style(Style::default().add_modifier(Modifier::BOLD)),
                    Line::from(project.integration_mode.display_name()).style(Style::default().fg(Color::Gray)),
                ]))
                .collect::<Vec<_>>();
            let mut state = ListState::default();
            state.select(Some(self.selected_project.min(self.config.projects.len().saturating_sub(1))));
            let list = List::new(items)
                .highlight_style(Style::default().bg(Color::Blue).fg(Color::Black))
                .highlight_symbol("> ");
            frame.render_stateful_widget(list, left_inner, &mut state);

            let row_height = 2_u16;
            for (index, _) in self.config.projects.iter().enumerate() {
                let rect = Rect {
                    x: left_inner.x,
                    y: left_inner.y + index as u16 * row_height,
                    width: left_inner.width,
                    height: row_height,
                };
                if rect.y < left_inner.y + left_inner.height {
                    self.hit_targets.push(HitTarget::new(rect, HitAction::SelectProject(index)));
                }
            }
        }

        let right_block = Block::default().borders(Borders::ALL).title(" Settings ");
        let right_inner = right_block.inner(columns[1]);
        frame.render_widget(right_block, columns[1]);

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(8), Constraint::Length(3)])
            .split(right_inner);

        let mut lines = vec![
            Line::from(format!("Config file: {}", self.config_store.path().display())).bold(),
            Line::from(format!("Schema version: {}", self.config.schema_version)),
            Line::from(format!("Saved projects: {}", self.config.projects.len())),
            Line::from(format!("Mouse hints: {}", if self.config.ui.show_mouse_hints { "on" } else { "off" })),
            Line::from(format!("Tab hints: {}", if self.config.ui.show_tab_hints { "on" } else { "off" })),
            Line::raw(""),
        ];
        if let Some(project) = self.config.projects.get(self.selected_project) {
            lines.push(Line::from(format!("Selected project: {}", project.name)).yellow().bold());
            lines.extend(project.detail_lines().into_iter().map(Line::from));
            lines.push(Line::raw(""));
            lines.push(Line::from("Press E to amend repo roots, remotes, and target paths."));
            lines.push(Line::from("Up/Down switches the selected project."));
        } else {
            lines.push(Line::from("Select a project here once you have saved one."));
        }
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), rows[0]);

        let buttons = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(20), Constraint::Length(18), Constraint::Min(10)])
            .split(rows[1]);
        frame.render_widget(
            Paragraph::new(" Edit Project ").block(Block::default().borders(Borders::ALL).title(" action ")).style(Style::default().fg(Color::Green)),
            buttons[0],
        );
        frame.render_widget(
            Paragraph::new(" Dashboard ").block(Block::default().borders(Borders::ALL).title(" action ")).style(Style::default().fg(Color::Yellow)),
            buttons[1],
        );
        self.hit_targets.push(HitTarget::new(buttons[0], HitAction::OpenProjectEdit));
        self.hit_targets.push(HitTarget::new(buttons[1], HitAction::Switch(Screen::Dashboard)));
    }

    fn render_ui_settings(&mut self, frame: &mut Frame, area: Rect) {
        let block = Block::default().borders(Borders::ALL).title(" UI Settings ");
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let sections = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(8), Constraint::Length(BUTTON_ROW_HEIGHT)])
            .split(inner);

        let lines = vec![
            Line::from("Adjust interface preferences for the current config.").bold(),
            Line::raw(""),
            Line::from(format!(
                "Tab hints: {}",
                if self.config.ui.show_tab_hints { "visible" } else { "hidden" }
            )),
            Line::from("When enabled, the main tabs show [1]..[4] hints."),
            Line::from("Press Enter, Space, T, Left, or Right to toggle."),
        ];
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), sections[0]);

        self.render_button_row(
            frame,
            sections[1],
            &[DialogButton::new(
                if self.config.ui.show_tab_hints { "Hide Tab Hints" } else { "Show Tab Hints" },
                true,
                HitAction::ToggleTabHints,
                Style::default().fg(Color::Black).bg(Color::Rgb(140, 220, 180)),
            )],
        );
    }

    fn render_project_edit_dialog(&mut self, frame: &mut Frame, area: Rect) {
        let Some(dialog) = &self.project_edit_dialog else {
            return;
        };

        let popup = centered_rect(area, 78, 64);
        frame.render_widget(Clear, popup);
        let block = Block::default().borders(Borders::ALL).title(" Edit Project ").border_style(Style::default().fg(Color::Cyan));
        let inner = block.inner(popup);
        frame.render_widget(block, popup);

        let sections = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(4),
                Constraint::Min(6),
                Constraint::Length(BUTTON_GAP_HEIGHT),
                Constraint::Length(BUTTON_ROW_HEIGHT),
            ])
            .split(inner);

        let header = vec![
            Line::from(format!("Project: {}", dialog.project_name)).bold(),
            Line::from("Edit the same core fields as New Project, then press F2 or Save."),
            Line::from("Tab/Shift+Tab moves between fields. Left/Right changes enums. Ctrl+O browses. Del removes the project."),
        ];
        frame.render_widget(Paragraph::new(header).wrap(Wrap { trim: false }), sections[0]);

        let fields = dialog
            .visible_fields()
            .into_iter()
            .filter(|field| !matches!(field, ProjectEditFocus::Save | ProjectEditFocus::Remove | ProjectEditFocus::Cancel))
            .collect::<Vec<_>>();
        let preferred_row_height = 3;
        let row_height = if (fields.len() as u16 * preferred_row_height) <= sections[1].height {
            preferred_row_height
        } else {
            2
        };
        let constraints = vec![Constraint::Length(row_height); fields.len()];

        let field_rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(sections[1]);

        for (field, row) in fields.iter().zip(field_rows.iter()) {
            let focused = *field == dialog.focus;
            let (label, action) = dialog.render_field(*field);
            let browse_action = project_edit_browse_action(*field);
            let value = dialog.display_value_for_field(*field, focused, visible_field_width(row.width, browse_action.is_some()));
            let browse_rect = self.render_form_row(frame, *row, label, value, focused, action.clone(), browse_action.clone());
            self.hit_targets.push(HitTarget::new(*row, action));
            if let (Some(rect), Some(action)) = (browse_rect, browse_action) {
                self.hit_targets.push(HitTarget::new(rect, action));
            }
        }

        self.render_button_row(
            frame,
            sections[3],
            &[
                DialogButton::new("Save", dialog.focus == ProjectEditFocus::Save, HitAction::SaveProjectEdit, Style::default().fg(Color::Black).bg(Color::Green)),
                DialogButton::new("Remove", dialog.focus == ProjectEditFocus::Remove, HitAction::RemoveProject, Style::default().fg(Color::White).bg(Color::Red)),
                DialogButton::new("Cancel", dialog.focus == ProjectEditFocus::Cancel, HitAction::CancelProjectEdit, Style::default().fg(Color::Black).bg(Color::Rgb(230, 190, 90))),
            ],
        );
    }

    fn render_wizard(&mut self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(58), Constraint::Percentage(42)])
            .split(area);

        let block = Block::default().borders(Borders::ALL).title(" New Project Wizard ");
        let inner = block.inner(chunks[0]);
        frame.render_widget(block, chunks[0]);

        let left_sections = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(6),
                Constraint::Length(BUTTON_GAP_HEIGHT),
                Constraint::Length(BUTTON_ROW_HEIGHT),
            ])
            .split(inner);

        let fields = self
            .wizard
            .visible_fields()
            .into_iter()
            .filter(|field| !matches!(field, WizardField::Validate | WizardField::Save | WizardField::Cancel))
            .collect::<Vec<_>>();
        let preferred_row_height = 3;
        let row_height = if (fields.len() as u16 * preferred_row_height) <= left_sections[0].height {
            preferred_row_height
        } else {
            2
        };
        let constraints = vec![Constraint::Length(row_height); fields.len()];

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(left_sections[0]);

        for (field, row) in fields.iter().zip(rows.iter()) {
            let focused = *field == self.wizard.focus;
            let (label, action) = self.wizard.render_field(*field);
            let browse_action = wizard_browse_action(*field);
            let value = self.wizard.display_value_for_field(*field, focused, visible_field_width(row.width, browse_action.is_some()));
            let browse_rect = self.render_form_row(frame, *row, label, value, focused, action.clone(), browse_action.clone());
            self.hit_targets.push(HitTarget::new(*row, action));
            if let (Some(rect), Some(action)) = (browse_rect, browse_action) {
                self.hit_targets.push(HitTarget::new(rect, action));
            }
        }

        self.render_button_row(
            frame,
            left_sections[2],
            &[
                DialogButton::new("Read", self.wizard.focus == WizardField::Validate, HitAction::ValidateWizard, Style::default().fg(Color::Black).bg(Color::Yellow)),
                DialogButton::new("Save", self.wizard.focus == WizardField::Save, HitAction::SaveWizard, Style::default().fg(Color::Black).bg(Color::Green)),
                DialogButton::new("Cancel", self.wizard.focus == WizardField::Cancel, HitAction::CancelWizard, Style::default().fg(Color::White).bg(Color::Red)),
            ],
        );

        let side_block = Block::default().borders(Borders::ALL).title(" Validation And Notes ");
        let side_inner = side_block.inner(chunks[1]);
        frame.render_widget(side_block, chunks[1]);

        let mut lines = vec![
            Line::from("Wizard notes".bold()),
            Line::from(""),
            Line::from(format!("Project type: {}", self.wizard.project_type.display_name())),
            Line::from(format!("Integration: {}", self.wizard.integration_mode.display_name())),
            Line::from(format!("Version scheme: {}", self.wizard.version_scheme.display_name())),
            Line::from(format!("Example: {}", self.wizard.version_scheme.example())),
            Line::from(format!("Rule: {}", self.wizard.version_scheme.description())),
            Line::raw(""),
            Line::from("Current slice scaffolds branched projects with one initial branch."),
            Line::from("Later slices will add branch management, bump preview, and git flows."),
            Line::raw(""),
        ];

        if let Some(probe) = &self.wizard.last_probe {
            let color = match probe.kind {
                ProbeKind::Success => Color::Green,
                ProbeKind::Warning => Color::Yellow,
                ProbeKind::Error => Color::Red,
            };
            lines.push(Line::from(Span::styled("Read target", Style::default().fg(color).add_modifier(Modifier::BOLD))));
            lines.push(Line::from(probe.message.clone()));
            if let Some(version) = &probe.version {
                lines.push(Line::from(format!("Detected version: {}", version)));
            }
            if let Some(format) = probe.format {
                lines.push(Line::from(format!("Detected format: {}", format.display_name())));
            }
        } else {
            lines.push(Line::from("Use F5 or Read to inspect the target file."));
            lines.push(Line::from("The wizard checks that the configured key exists and is a string."));
        }
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), side_inner);
    }

    fn render_form_row(
        &self,
        frame: &mut Frame,
        area: Rect,
        label: &'static str,
        value: String,
        focused: bool,
        _action: HitAction,
        browse_action: Option<HitAction>,
    ) -> Option<Rect> {
        let label_area = center_vertically(
            Rect {
                x: area.x,
                y: area.y,
                width: FORM_LABEL_WIDTH,
                height: area.height,
            },
            1,
        );
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                label,
                Style::default().fg(Color::Rgb(220, 220, 220)), // light gray font color for labels
            ))),
            label_area,
        );

        let row = if browse_action.is_some() {
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Length(FORM_LABEL_WIDTH),
                    Constraint::Min(10),
                    Constraint::Length(1),
                    Constraint::Length(BROWSE_BUTTON_WIDTH),
                ])
                .split(area)
        } else {
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Length(FORM_LABEL_WIDTH), Constraint::Min(10)])
                .split(area)
        };

        let field_index = if browse_action.is_some() { 1 } else { 1 };
        let field_area = center_vertically(row[field_index], area.height.min(3));
        let style = Style::default().fg(Color::Rgb(235, 235, 235));
        let block = if focused {
            Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::Cyan))
        } else {
            Block::default().borders(Borders::ALL)
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(value, style))).block(block),
            field_area,
        );

        if browse_action.is_some() {
            let button_area = center_vertically(row[3], area.height.min(3));
            frame.render_widget(
                Paragraph::new("Browse")
                    .alignment(Alignment::Center)
                    .style(Style::default().fg(Color::Black).bg(Color::Cyan))
                    .block(Block::default().borders(Borders::ALL)),
                button_area,
            );
            return Some(button_area);
        }

        None
    }

    fn render_button_row(&mut self, frame: &mut Frame, area: Rect, buttons: &[DialogButton]) {
        if buttons.is_empty() {
            return;
        }

        let mut constraints = Vec::with_capacity(buttons.len() * 2 + 1);
        constraints.push(Constraint::Fill(1));
        for button in buttons {
            constraints.push(Constraint::Length((button.label.len() as u16 + 6).max(14)));
            constraints.push(Constraint::Fill(1));
        }
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(constraints)
            .flex(Flex::Center)
            .split(area);

        for (index, button) in buttons.iter().enumerate() {
            let rect = center_vertically(chunks[(index * 2) + 1], area.height.min(3));
            let style = if button.focused {
                button.style.add_modifier(Modifier::BOLD)
            } else {
                button.style
            };
            let block = if button.focused {
                Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::Cyan))
            } else {
                Block::default().borders(Borders::ALL)
            };
            frame.render_widget(
                Paragraph::new(button.label)
                    .alignment(Alignment::Center)
                    .style(style)
                    .block(block),
                rect,
            );
            self.hit_targets.push(HitTarget::new(rect, button.action.clone()));
        }
    }

    fn render_footer(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default().borders(Borders::ALL).title(" Controls ");
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let help = if self.browser_dialog.is_some() {
            "Arrows navigate | Enter select | U use folder | Mouse click or wheel | Esc cancel"
        } else if self.project_edit_dialog.is_some() {
            "Tab move | Left/Right change enums | Ctrl+O browse | F2 save | Del remove | Esc cancel"
        } else if self.tag_dialog.is_some() {
            "Type tag name | Left/Right action | Enter run | Esc cancel"
        } else if self.recent_changes_dialog.is_some() {
            "1/2 switch tabs | Left/Right history | Up/Down scroll | T create tag | Esc close"
        } else if self.bump_dialog.is_some() {
            "Left/Right change bump action | Enter apply | Esc cancel"
        } else {
            match self.screen {
                Screen::Dashboard => "1-4 tabs | N new project | B bump | V view changes | T create tag | Up/Down select | Q quit",
                Screen::Settings => "1-4 tabs | Up/Down select project | E edit selected project | N new project | Q quit",
                Screen::UiSettings => "1-4 tabs | Enter, Space, T, Left, Right toggle tab hints | N new project | Q quit",
                Screen::Wizard => "Tab move | Left/Right change enums | Ctrl+O browse | F5 read target | F2 save | Esc cancel",
            }
        };
        frame.render_widget(Paragraph::new(help), inner);
    }

    fn render_browser_dialog(&mut self, frame: &mut Frame, area: Rect) {
        let Some(dialog) = &self.browser_dialog else {
            return;
        };

        let popup = centered_rect(area, 78, 72);
        frame.render_widget(Clear, popup);
        let block = Block::default()
            .borders(Borders::ALL)
            .title(format!(" {} ", dialog.title))
            .border_style(Style::default().fg(Color::Cyan));
        let inner = block.inner(popup);
        frame.render_widget(block, popup);

        let sections = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(2), Constraint::Min(8), Constraint::Length(2)])
            .split(inner);

        let instructions = if dialog.select_directories {
            "Arrows navigate | Enter use folder | U use current file's folder | Esc cancel"
        } else {
            "Arrows navigate | Enter select file | Mouse click selects | Esc cancel"
        };
        frame.render_widget(Paragraph::new(instructions), sections[0]);

        let body_block = Block::default()
            .borders(Borders::ALL)
            .title(format!(" {} ", dialog.explorer.cwd().display()));
        let body_inner = body_block.inner(sections[1]);
        frame.render_widget(body_block, sections[1]);

        let files = dialog.explorer.files();
        let selected = dialog.explorer.selected_idx();
        let (start, end) = browser_visible_range(files.len(), selected, body_inner.height as usize);
        let items = files[start..end]
            .iter()
            .map(|file| ListItem::new(file.name.clone()))
            .collect::<Vec<_>>();
        let mut state = ListState::default();
        state.select(Some(selected.saturating_sub(start)));
        let list = List::new(items)
            .highlight_style(Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD))
            .highlight_symbol("> ");
        frame.render_stateful_widget(list, body_inner, &mut state);

        for (offset, _) in files[start..end].iter().enumerate() {
            let rect = Rect {
                x: body_inner.x,
                y: body_inner.y + offset as u16,
                width: body_inner.width,
                height: 1,
            };
            self.hit_targets.push(HitTarget::new(rect, HitAction::BrowserSelect(start + offset)));
        }

        let selected_path = dialog.explorer.current().path.display().to_string();
        frame.render_widget(
            Paragraph::new(format!("Selected: {}", selected_path)).wrap(Wrap { trim: false }),
            sections[2],
        );
    }

    fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('v') {
            self.paste_from_clipboard();
            return Ok(());
        }

        if self.try_handle_toast_shortcut(key) {
            return Ok(());
        }

        if self.browser_dialog.is_some() {
            return self.handle_browser_key(key);
        }

        if self.project_edit_dialog.is_some() {
            return self.handle_project_edit_key(key);
        }

        if self.tag_dialog.is_some() {
            return self.handle_tag_key(key);
        }

        if self.recent_changes_dialog.is_some() {
            return self.handle_recent_changes_key(key);
        }

        if self.bump_dialog.is_some() {
            return self.handle_bump_key(key);
        }

        if self.handle_tab_shortcut(key) {
            return Ok(());
        }

        if key.code == KeyCode::Char('q') && key.modifiers.is_empty() {
            self.should_quit = true;
            return Ok(());
        }

        match self.screen {
            Screen::Dashboard => self.handle_dashboard_key(key),
            Screen::Settings => self.handle_settings_key(key),
            Screen::UiSettings => self.handle_ui_settings_key(key),
            Screen::Wizard => self.handle_wizard_key(key),
        }
    }

    fn handle_dashboard_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Char('n') => self.open_wizard(),
            KeyCode::Char('b') => self.open_bump_dialog()?,
            KeyCode::Char('v') => self.open_recent_changes()?,
            KeyCode::Char('t') => self.open_tag_dialog()?,
            KeyCode::Char('s') => self.screen = Screen::Settings,
            KeyCode::Up => self.move_project_selection(-1),
            KeyCode::Down => self.move_project_selection(1),
            _ => {}
        }
        Ok(())
    }

    fn handle_settings_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Char('d') => self.screen = Screen::Dashboard,
            KeyCode::Char('n') => self.open_wizard(),
            KeyCode::Char('e') => self.open_project_edit_dialog()?,
            KeyCode::Up => self.move_project_selection(-1),
            KeyCode::Down => self.move_project_selection(1),
            _ => {}
        }
        Ok(())
    }

    fn handle_ui_settings_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Char('d') => self.screen = Screen::Dashboard,
            KeyCode::Char('n') => self.open_wizard(),
            KeyCode::Char('t') | KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Left | KeyCode::Right => {
                self.toggle_tab_hints()?;
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_wizard_key(&mut self, key: KeyEvent) -> Result<()> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s') {
            return self.save_wizard_project();
        }

        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('o') {
            return self.open_browser_for_wizard_focus();
        }

        if self.wizard.focus_accepts_text() {
            match key.code {
                KeyCode::Esc => {
                    self.screen = Screen::Dashboard;
                    self.status = StatusMessage::info("Wizard cancelled.");
                }
                KeyCode::Tab | KeyCode::Down => self.wizard.focus_next(),
                KeyCode::BackTab | KeyCode::Up => self.wizard.focus_previous(),
                KeyCode::F(5) => self.validate_wizard_target(),
                KeyCode::F(2) => return self.save_wizard_project(),
                KeyCode::Enter => {
                    self.wizard.focus_next();
                }
                _ => self.wizard.handle_text_input(key),
            }
            return Ok(());
        }

        match key.code {
            KeyCode::Esc => {
                self.screen = Screen::Dashboard;
                self.status = StatusMessage::info("Wizard cancelled.");
            }
            KeyCode::Tab | KeyCode::Down => self.wizard.focus_next(),
            KeyCode::BackTab | KeyCode::Up => self.wizard.focus_previous(),
            KeyCode::F(5) => self.validate_wizard_target(),
            KeyCode::F(2) => return self.save_wizard_project(),
            KeyCode::Enter => return self.activate_wizard_focus(),
            KeyCode::Left => self.wizard.adjust_current_enum(-1),
            KeyCode::Right => self.wizard.adjust_current_enum(1),
            _ => self.wizard.handle_text_input(key),
        }
        Ok(())
    }

    fn handle_bump_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => {
                self.bump_dialog = None;
                self.status = StatusMessage::info("Bump preview closed.");
            }
            KeyCode::Left => self.rotate_bump_action(-1),
            KeyCode::Right => self.rotate_bump_action(1),
            KeyCode::Enter | KeyCode::F(2) => self.apply_bump()?,
            _ => {}
        }
        Ok(())
    }

    fn handle_recent_changes_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => {
                self.recent_changes_dialog = None;
                self.status = StatusMessage::info("View changes closed.");
            }
            KeyCode::Up => self.scroll_recent_changes(-1),
            KeyCode::Down => self.scroll_recent_changes(1),
            KeyCode::PageUp => self.scroll_recent_changes(-8),
            KeyCode::PageDown => self.scroll_recent_changes(8),
            KeyCode::Tab => {
                if let Some(dialog) = &mut self.recent_changes_dialog {
                    dialog.cycle_tab(1);
                }
            }
            KeyCode::BackTab => {
                if let Some(dialog) = &mut self.recent_changes_dialog {
                    dialog.cycle_tab(-1);
                }
            }
            KeyCode::Char('1') => {
                if let Some(dialog) = &mut self.recent_changes_dialog {
                    dialog.switch_tab(RecentChangesTab::Recent);
                }
            }
            KeyCode::Char('2') => {
                if let Some(dialog) = &mut self.recent_changes_dialog {
                    dialog.switch_tab(RecentChangesTab::History);
                }
            }
            KeyCode::Left => {
                if let Some(dialog) = &mut self.recent_changes_dialog {
                    if dialog.active_tab == RecentChangesTab::History {
                        dialog.navigate_history(1);
                    }
                }
            }
            KeyCode::Right => {
                if let Some(dialog) = &mut self.recent_changes_dialog {
                    if dialog.active_tab == RecentChangesTab::History {
                        dialog.navigate_history(-1);
                    }
                }
            }
            KeyCode::Char('t') => self.open_tag_dialog()?,
            _ => {}
        }
        Ok(())
    }

    fn handle_tag_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => {
                self.tag_dialog = None;
                self.status = StatusMessage::info("Tag creation cancelled.");
            }
            KeyCode::Left => self.rotate_tag_action(-1),
            KeyCode::Right => self.rotate_tag_action(1),
            KeyCode::Enter | KeyCode::F(2) => self.create_local_tag()?,
            _ => {
                if let Some(dialog) = &mut self.tag_dialog {
                    dialog.tag_name.handle_key(key);
                }
            }
        }
        Ok(())
    }

    fn handle_project_edit_key(&mut self, key: KeyEvent) -> Result<()> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('o') {
            return self.open_browser_for_project_edit_focus();
        }

        let focus_accepts_text = self
            .project_edit_dialog
            .as_ref()
            .map(|dialog| dialog.focus_accepts_text())
            .unwrap_or(false);

        if focus_accepts_text {
            match key.code {
                KeyCode::Esc => {
                    self.project_edit_dialog = None;
                    self.status = StatusMessage::info("Project edit cancelled.");
                }
                KeyCode::Tab | KeyCode::Down => {
                    if let Some(dialog) = &mut self.project_edit_dialog {
                        dialog.focus_next();
                    }
                }
                KeyCode::BackTab | KeyCode::Up => {
                    if let Some(dialog) = &mut self.project_edit_dialog {
                        dialog.focus_previous();
                    }
                }
                KeyCode::F(2) => return self.save_project_edit(),
                KeyCode::Enter => {
                    if let Some(dialog) = &mut self.project_edit_dialog {
                        dialog.focus_next();
                    }
                }
                _ => {
                    if let Some(dialog) = &mut self.project_edit_dialog {
                        dialog.handle_text_input(key);
                    }
                }
            }
            return Ok(());
        }

        match key.code {
            KeyCode::Esc => {
                self.project_edit_dialog = None;
                self.status = StatusMessage::info("Project edit cancelled.");
            }
            KeyCode::Tab | KeyCode::Down => {
                if let Some(dialog) = &mut self.project_edit_dialog {
                    dialog.focus_next();
                }
            }
            KeyCode::BackTab | KeyCode::Up => {
                if let Some(dialog) = &mut self.project_edit_dialog {
                    dialog.focus_previous();
                }
            }
            KeyCode::Enter => {
                if let Some(dialog) = &self.project_edit_dialog {
                    if dialog.is_save_focused() {
                        return self.save_project_edit();
                    }
                    if dialog.is_remove_focused() {
                        return self.remove_project();
                    }
                    if dialog.is_cancel_focused() {
                        self.project_edit_dialog = None;
                        self.status = StatusMessage::info("Project edit cancelled.");
                        return Ok(());
                    }
                }
            }
            KeyCode::F(2) => return self.save_project_edit(),
            KeyCode::Delete if key.modifiers.is_empty() => return self.remove_project(),
            KeyCode::Left => {
                if let Some(dialog) = &mut self.project_edit_dialog {
                    dialog.adjust_current_enum(-1);
                }
            }
            KeyCode::Right => {
                if let Some(dialog) = &mut self.project_edit_dialog {
                    dialog.adjust_current_enum(1);
                }
            }
            _ => {
                if let Some(dialog) = &mut self.project_edit_dialog {
                    dialog.handle_text_input(key);
                }
            }
        }
        Ok(())
    }

    fn handle_mouse(&mut self, mouse: MouseEvent) {
        if self.handle_toast_mouse(mouse) {
            return;
        }

        if self.browser_dialog.is_some() {
            match mouse.kind {
                MouseEventKind::ScrollUp => {
                    self.move_browser_selection(-1);
                    return;
                }
                MouseEventKind::ScrollDown => {
                    self.move_browser_selection(1);
                    return;
                }
                _ => {}
            }
        }

        match mouse.kind {
            MouseEventKind::ScrollUp => {
                if self.project_edit_dialog.is_some() {
                } else if self.tag_dialog.is_some() {
                } else if self.recent_changes_dialog.is_some() {
                    self.scroll_recent_changes(-2);
                } else if self.bump_dialog.is_some() {
                    self.rotate_bump_action(-1);
                } else if matches!(self.screen, Screen::Dashboard | Screen::Settings) {
                    self.move_project_selection(-1);
                }
            }
            MouseEventKind::ScrollDown => {
                if self.project_edit_dialog.is_some() {
                } else if self.tag_dialog.is_some() {
                } else if self.recent_changes_dialog.is_some() {
                    self.scroll_recent_changes(2);
                } else if self.bump_dialog.is_some() {
                    self.rotate_bump_action(1);
                } else if matches!(self.screen, Screen::Dashboard | Screen::Settings) {
                    self.move_project_selection(1);
                }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(action) = self.hit_targets.iter().rev().find_map(|target| {
                    if target.contains(mouse.column, mouse.row) {
                        Some(target.action.clone())
                    } else {
                        None
                    }
                }) {
                    if let Err(error) = self.handle_hit_action(action) {
                        self.status = StatusMessage::error(error.to_string());
                    }
                }
            }
            MouseEventKind::Down(MouseButton::Right) => self.paste_from_clipboard(),
            _ => {}
        }
    }

    fn handle_browser_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => {
                self.browser_dialog = None;
                self.status = StatusMessage::info("Browse cancelled.");
            }
            KeyCode::Enter | KeyCode::F(2) => return self.confirm_browser_selection(),
            KeyCode::Char('u') | KeyCode::Char('U') => return self.confirm_browser_directory_selection(),
            _ => {
                if let Some(dialog) = &mut self.browser_dialog {
                    let event = Event::Key(key);
                    dialog.explorer.handle(&event)?;
                }
            }
        }
        Ok(())
    }

    fn handle_paste(&mut self, text: String) {
        let sanitized = sanitize_pasted_text(&text);
        if self.insert_text(&sanitized) {
            self.status = StatusMessage::info("Pasted into the active field.");
        }
    }

    fn paste_from_clipboard(&mut self) {
        let Ok(mut clipboard) = Clipboard::new() else {
            self.status = StatusMessage::warning("Clipboard is not available in this environment.");
            return;
        };

        match clipboard.get_text() {
            Ok(text) => {
                let sanitized = sanitize_pasted_text(&text);
                if self.insert_text(&sanitized) {
                    self.status = StatusMessage::info("Pasted into the active field.");
                } else {
                    self.status = StatusMessage::warning("No editable field is focused.");
                }
            }
            Err(_) => {
                self.status = StatusMessage::warning("Clipboard paste failed.");
            }
        }
    }

    fn insert_text(&mut self, text: &str) -> bool {
        if let Some(dialog) = &mut self.project_edit_dialog {
            if dialog.insert_text(text) {
                return true;
            }
        }

        if let Some(dialog) = &mut self.tag_dialog {
            dialog.tag_name.insert_str(text);
            return true;
        }

        if self.screen == Screen::Wizard {
            if self.wizard.insert_text(text) {
                return true;
            }
        }

        false
    }

    fn handle_hit_action(&mut self, action: HitAction) -> Result<()> {
        match action {
            HitAction::Switch(screen) => {
                self.screen = screen;
                if screen == Screen::Wizard {
                    self.wizard = ProjectWizard::default();
                }
            }
            HitAction::Quit => self.should_quit = true,
            HitAction::SelectProject(index) => {
                self.selected_project = index.min(self.config.projects.len().saturating_sub(1));
            }
            HitAction::OpenProjectEdit => return self.open_project_edit_dialog(),
            HitAction::EditProjectField(field) => {
                if let Some(dialog) = &mut self.project_edit_dialog {
                    dialog.focus = field;
                }
            }
            HitAction::SaveProjectEdit => return self.save_project_edit(),
            HitAction::RemoveProject => return self.remove_project(),
            HitAction::CancelProjectEdit => {
                self.project_edit_dialog = None;
                self.status = StatusMessage::info("Project edit cancelled.");
            }
            HitAction::ToggleTabHints => return self.toggle_tab_hints(),
            HitAction::BrowseWizardTargetPath => return self.open_browser(BrowseTarget::WizardTargetPath),
            HitAction::BrowseWizardRepoRoot => return self.open_browser(BrowseTarget::WizardRepoRoot),
            HitAction::BrowseProjectTargetPath => return self.open_browser(BrowseTarget::ProjectEditTargetPath),
            HitAction::BrowseProjectRepoRoot => return self.open_browser(BrowseTarget::ProjectEditRepoRoot),
            HitAction::BrowserSelect(index) => self.select_browser_index(index),
            HitAction::SelectRecentChangesTab(tab) => {
                if let Some(dialog) = &mut self.recent_changes_dialog {
                    dialog.switch_tab(tab);
                }
            }
            HitAction::CycleBumpAction(delta) => self.rotate_bump_action(delta),
            HitAction::ApplyBump => return self.apply_bump(),
            HitAction::CancelBump => {
                self.bump_dialog = None;
                self.status = StatusMessage::info("Bump preview closed.");
            }
            HitAction::CloseRecentChanges => {
                self.recent_changes_dialog = None;
                self.status = StatusMessage::info("View changes closed.");
            }
            HitAction::ScrollRecentChanges(delta) => self.scroll_recent_changes(delta),
            HitAction::OpenTagDialog => return self.open_tag_dialog(),
            HitAction::CycleTagAction(delta) => self.rotate_tag_action(delta),
            HitAction::CreateTag => return self.create_local_tag(),
            HitAction::CancelTagDialog => {
                self.tag_dialog = None;
                self.status = StatusMessage::info("Tag creation cancelled.");
            }
            HitAction::WizardField(field) => self.wizard.focus = field,
            HitAction::ValidateWizard => self.validate_wizard_target(),
            HitAction::SaveWizard => return self.save_wizard_project(),
            HitAction::CancelWizard => {
                self.screen = Screen::Dashboard;
                self.status = StatusMessage::info("Wizard cancelled.");
            }
        }
        Ok(())
    }

    fn open_recent_changes(&mut self) -> Result<()> {
        let project = self.selected_project()?.clone();
        let dialog = RecentChangesDialog::from_project(&project)?;
        self.bump_dialog = None;
        self.tag_dialog = None;
        self.project_edit_dialog = None;
        self.recent_changes_dialog = Some(dialog);
        self.status = StatusMessage::info("Showing git changes for the selected project.");
        Ok(())
    }

    fn scroll_recent_changes(&mut self, delta: i16) {
        if let Some(dialog) = &mut self.recent_changes_dialog {
            if delta.is_negative() {
                dialog.scroll = dialog.scroll.saturating_sub(delta.unsigned_abs());
            } else {
                dialog.scroll = dialog.scroll.saturating_add(delta as u16);
            }
        }
    }

    fn move_browser_selection(&mut self, delta: isize) {
        if let Some(dialog) = &mut self.browser_dialog {
            if dialog.explorer.files().is_empty() {
                return;
            }
            let len = dialog.explorer.files().len() as isize;
            let next = (dialog.explorer.selected_idx() as isize + delta).clamp(0, len - 1) as usize;
            dialog.explorer.set_selected_idx(next);
        }
    }

    fn select_browser_index(&mut self, index: usize) {
        let mut confirm_file = false;
        if let Some(dialog) = &mut self.browser_dialog {
            let len = dialog.explorer.files().len();
            if len == 0 || index >= len {
                return;
            }
            let already_selected = dialog.explorer.selected_idx() == index;
            dialog.explorer.set_selected_idx(index);
            if already_selected && !dialog.select_directories && dialog.explorer.current().path.is_file() {
                confirm_file = true;
            }
        }
        if confirm_file {
            let _ = self.confirm_browser_selection();
        }
    }

    fn open_project_edit_dialog(&mut self) -> Result<()> {
        let project_index = self.selected_project;
        let project = self.selected_project()?;
        let dialog = ProjectEditDialog::from_project(project_index, project)?;
        self.browser_dialog = None;
        self.project_edit_dialog = Some(dialog);
        self.status = StatusMessage::info("Amend project settings, then save or remove the project.");
        Ok(())
    }

    fn save_project_edit(&mut self) -> Result<()> {
        let dialog = self
            .project_edit_dialog
            .clone()
            .ok_or_else(|| anyhow!("no project edit is in progress"))?;
        let project = self
            .config
            .projects
            .get_mut(dialog.project_index)
            .ok_or_else(|| anyhow!("selected project no longer exists"))?;
        dialog.apply(project)?;
        self.config_store.save(&self.config)?;
        self.project_edit_dialog = None;
        self.status = StatusMessage::success("Project settings updated.");
        Ok(())
    }

    fn remove_project(&mut self) -> Result<()> {
        let dialog = self
            .project_edit_dialog
            .clone()
            .ok_or_else(|| anyhow!("no project edit is in progress"))?;
        if dialog.project_index >= self.config.projects.len() {
            bail!("selected project no longer exists");
        }

        let removed = self.config.projects.remove(dialog.project_index);
        self.config_store.save(&self.config)?;
        self.project_edit_dialog = None;
        if self.config.projects.is_empty() {
            self.selected_project = 0;
        } else {
            self.selected_project = dialog.project_index.min(self.config.projects.len().saturating_sub(1));
        }
        self.status = StatusMessage::success(format!("Removed project '{}'.", removed.name));
        Ok(())
    }

    fn open_tag_dialog(&mut self) -> Result<()> {
        let project = self.selected_project()?.clone();
        let dialog = TagDialog::from_project(&project)?;
        self.bump_dialog = None;
        self.project_edit_dialog = None;
        self.browser_dialog = None;
        self.tag_dialog = Some(dialog);
        self.status = StatusMessage::info("Review the proposed tag name, then press Enter to create it locally.");
        Ok(())
    }

    fn rotate_tag_action(&mut self, delta: isize) {
        if let Some(dialog) = &mut self.tag_dialog {
            dialog.rotate_action(delta);
        }
    }

    fn create_local_tag(&mut self) -> Result<()> {
        let Some(dialog) = &self.tag_dialog else {
            return Ok(());
        };

        let tag_name = dialog.tag_name.value.trim();
        if tag_name.is_empty() {
            bail!("tag name cannot be empty");
        }

        let repo_root = dialog.repo_root.clone();
        let project_name = dialog.project_name.clone();
        let action = dialog.selected_action();
        let remote_spec = dialog.remote_spec.clone();
        let tag_name = tag_name.to_string();
        let created = ensure_local_tag(&repo_root, &tag_name)?;

        if matches!(action, TagAction::CreateAndPush | TagAction::CreatePushAndRelease) {
            let remote_spec = remote_spec.ok_or_else(|| anyhow!("no remote is configured for this project"))?;
            run_git_checked(&repo_root, &["push", &remote_spec, &tag_name])?;
        }

        if matches!(action, TagAction::CreatePushAndRelease) {
            ensure_gh_available()?;
            run_gh_checked(&repo_root, &["release", "create", &tag_name, "--generate-notes"])?;
        }

        self.tag_dialog = None;
        let summary = match action {
            TagAction::CreateLocal if created => format!("Created local tag '{}' in {}.", tag_name, project_name),
            TagAction::CreateLocal => format!("Tag '{}' already existed locally in {}.", tag_name, project_name),
            TagAction::CreateAndPush => format!("Tag '{}' is present locally and has been pushed for {}.", tag_name, project_name),
            TagAction::CreatePushAndRelease => format!("Tag '{}' was created, pushed, and released for {}.", tag_name, project_name),
        };
        self.status = StatusMessage::success(summary);
        Ok(())
    }

    fn selected_project(&self) -> Result<&ProjectConfig> {
        self.config
            .projects
            .get(self.selected_project)
            .ok_or_else(|| anyhow!("no project is selected"))
    }

    fn open_bump_dialog(&mut self) -> Result<()> {
        let project = self.selected_project()?;
        let dialog = BumpDialog::from_project(project)?;
        self.recent_changes_dialog = None;
        self.tag_dialog = None;
        self.project_edit_dialog = None;
        self.browser_dialog = None;
        self.bump_dialog = Some(dialog);
        self.status = StatusMessage::info("Review the preview, then press Enter to apply the bump.");
        Ok(())
    }

    fn rotate_bump_action(&mut self, delta: isize) {
        if let Some(dialog) = &mut self.bump_dialog {
            dialog.rotate_action(delta);
        }
    }

    fn apply_bump(&mut self) -> Result<()> {
        let Some(dialog) = &self.bump_dialog else {
            return Ok(());
        };

        let next_version = dialog.preview_next_version().map_err(anyhow::Error::msg)?;
        for target in &dialog.targets {
            write_target_version(target, &next_version)?;
        }

        let target_count = dialog.targets.len();
        self.bump_dialog = None;
        let repo_backed = self.selected_project()?.integration_mode.requires_repo();
        self.status = StatusMessage::success(format!(
            "Updated {} target{} to {}.",
            target_count,
            if target_count == 1 { "" } else { "s" },
            next_version
        ));
        if repo_backed {
            self.open_tag_dialog()?;
            self.status = StatusMessage::info("Version bump applied. Review the suggested tag action next.");
        }
        Ok(())
    }

    fn open_wizard(&mut self) {
        self.wizard = ProjectWizard::default();
        self.browser_dialog = None;
        self.screen = Screen::Wizard;
        self.status = StatusMessage::info("Configure a project and read the target file before saving.");
    }

    fn activate_wizard_focus(&mut self) -> Result<()> {
        match self.wizard.focus {
            WizardField::Validate => {
                self.validate_wizard_target();
                Ok(())
            }
            WizardField::Save => self.save_wizard_project(),
            WizardField::Cancel => {
                self.screen = Screen::Dashboard;
                self.status = StatusMessage::info("Wizard cancelled.");
                Ok(())
            }
            _ => {
                self.wizard.focus_next();
                Ok(())
            }
        }
    }

    fn move_project_selection(&mut self, delta: isize) {
        if self.config.projects.is_empty() {
            return;
        }
        let len = self.config.projects.len() as isize;
        let next = (self.selected_project as isize + delta).clamp(0, len - 1);
        self.selected_project = next as usize;
    }

    fn validate_wizard_target(&mut self) {
        match probe_target(
            self.wizard.target_path.value.trim(),
            self.wizard.target_key.value.trim(),
            self.wizard.version_scheme,
        ) {
            Ok(probe) => {
                self.status = match probe.kind {
                    ProbeKind::Success => {
                        StatusMessage::success("Target file is readable and the selected key matches the chosen scheme.")
                    }
                    ProbeKind::Warning => {
                        StatusMessage::warning("Target file is readable, but the detected version does not match the chosen scheme.")
                    }
                    ProbeKind::Error => StatusMessage::error("Target validation failed."),
                };
                self.wizard.last_probe = Some(probe);
            }
            Err(error) => {
                self.status = StatusMessage::error(error.to_string());
                self.wizard.last_probe = Some(TargetProbe {
                    kind: ProbeKind::Error,
                    message: error.to_string(),
                    version: None,
                    format: None,
                });
            }
        }
    }

    fn save_wizard_project(&mut self) -> Result<()> {
        let project = self.wizard.build_project()?;
        self.config.projects.push(project);
        self.config_store.save(&self.config)?;
        self.selected_project = self.config.projects.len().saturating_sub(1);
        self.screen = Screen::Dashboard;
        self.status = StatusMessage::success("Project saved to the user config directory.");
        Ok(())
    }

    fn open_browser_for_wizard_focus(&mut self) -> Result<()> {
        let target = match self.wizard.focus {
            WizardField::TargetPath => BrowseTarget::WizardTargetPath,
            WizardField::RepoRoot => BrowseTarget::WizardRepoRoot,
            _ => return Ok(()),
        };
        self.open_browser(target)
    }

    fn open_browser_for_project_edit_focus(&mut self) -> Result<()> {
        let Some(dialog) = &self.project_edit_dialog else {
            return Ok(());
        };
        let target = match dialog.focus {
            ProjectEditFocus::TargetPath => BrowseTarget::ProjectEditTargetPath,
            ProjectEditFocus::RepoRoot => BrowseTarget::ProjectEditRepoRoot,
            _ => return Ok(()),
        };
        self.open_browser(target)
    }

    fn open_browser(&mut self, target: BrowseTarget) -> Result<()> {
        let dialog = FileBrowserDialog::new(target, self.initial_browser_path(target))?;
        self.browser_dialog = Some(dialog);
        self.status = StatusMessage::info("Browse to a file or directory, then press Enter to select it.");
        Ok(())
    }

    fn initial_browser_path(&self, target: BrowseTarget) -> String {
        match target {
            BrowseTarget::WizardTargetPath => self.wizard.target_path.value().to_string(),
            BrowseTarget::WizardRepoRoot => self.wizard.repo_root.value().to_string(),
            BrowseTarget::ProjectEditTargetPath => self
                .project_edit_dialog
                .as_ref()
                .map(|dialog| dialog.target_path.value().to_string())
                .unwrap_or_default(),
            BrowseTarget::ProjectEditRepoRoot => self
                .project_edit_dialog
                .as_ref()
                .map(|dialog| dialog.repo_root.value().to_string())
                .unwrap_or_default(),
        }
    }

    fn confirm_browser_selection(&mut self) -> Result<()> {
        let Some(dialog) = &self.browser_dialog else {
            return Ok(());
        };

        let selected = dialog.explorer.current().path.clone();
        let target = dialog.target;
        let select_directories = dialog.select_directories;

        if select_directories && !selected.is_dir() {
            self.status = StatusMessage::warning("Select a directory for Repo root.");
            return Ok(());
        }

        if !select_directories && !selected.is_file() {
            self.status = StatusMessage::warning("Select a file for Target path. Use Right to enter directories.");
            return Ok(());
        }

        let selected = selected.display().to_string();
        match target {
            BrowseTarget::WizardTargetPath => self.wizard.set_target_path_from_browse(selected),
            BrowseTarget::WizardRepoRoot => self.wizard.set_repo_root_from_browse(selected),
            BrowseTarget::ProjectEditTargetPath => {
                if let Some(dialog) = &mut self.project_edit_dialog {
                    dialog.set_target_path_from_browse(selected);
                }
            }
            BrowseTarget::ProjectEditRepoRoot => {
                if let Some(dialog) = &mut self.project_edit_dialog {
                    dialog.set_repo_root_from_browse(selected);
                }
            }
        }

        self.browser_dialog = None;
        self.status = StatusMessage::success("Selection applied.");
        Ok(())
    }

    fn confirm_browser_directory_selection(&mut self) -> Result<()> {
        let Some(dialog) = &self.browser_dialog else {
            return Ok(());
        };
        if !dialog.select_directories {
            return Ok(());
        }

        let selected = dialog.explorer.current().path.clone();
        let directory = if selected.is_dir() {
            selected
        } else if let Some(parent) = selected.parent() {
            parent.to_path_buf()
        } else {
            selected
        };

        let selected = directory.display().to_string();
        match dialog.target {
            BrowseTarget::WizardRepoRoot => self.wizard.set_repo_root_from_browse(selected),
            BrowseTarget::ProjectEditRepoRoot => {
                if let Some(dialog) = &mut self.project_edit_dialog {
                    dialog.set_repo_root_from_browse(selected);
                }
            }
            _ => {}
        }

        self.browser_dialog = None;
        self.status = StatusMessage::success("Folder selection applied.");
        Ok(())
    }

    fn toggle_tab_hints(&mut self) -> Result<()> {
        self.config.ui.show_tab_hints = !self.config.ui.show_tab_hints;
        self.config_store.save(&self.config)?;
        self.status = StatusMessage::success(if self.config.ui.show_tab_hints {
            "Tab hints enabled."
        } else {
            "Tab hints hidden."
        });
        Ok(())
    }

    fn handle_tab_shortcut(&mut self, key: KeyEvent) -> bool {
        if !key.modifiers.is_empty() {
            return false;
        }

        if matches!(self.screen, Screen::Wizard) && self.wizard.focus_accepts_text() {
            return false;
        }

        let target = match key.code {
            KeyCode::Char('1') => Some(Screen::Dashboard),
            KeyCode::Char('2') => Some(Screen::Wizard),
            KeyCode::Char('3') => Some(Screen::Settings),
            KeyCode::Char('4') => Some(Screen::UiSettings),
            _ => None,
        };

        let Some(target) = target else {
            return false;
        };

        match target {
            Screen::Wizard => self.open_wizard(),
            _ => self.screen = target,
        }
        true
    }

    fn try_handle_toast_shortcut(&mut self, key: KeyEvent) -> bool {
        if key.modifiers.contains(KeyModifiers::ALT)
            && matches!(key.code, KeyCode::Char('x') | KeyCode::Char('X'))
        {
            let interaction = self.sticky_toaster.handle_shortcut(ToastShortcut::Dismiss);
            return self.handle_toast_interaction(interaction);
        }

        if key.code == KeyCode::F(5) && self.screen != Screen::Wizard {
            let interaction = self.sticky_toaster.handle_shortcut(ToastShortcut::Copy);
            return self.handle_toast_interaction(interaction);
        }

        false
    }

    fn handle_toast_mouse(&mut self, mouse: MouseEvent) -> bool {
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let interaction = self
                    .sticky_toaster
                    .handle_click(mouse.column, mouse.row, ToastMouseButton::Left);
                self.handle_toast_interaction(interaction)
            }
            MouseEventKind::Down(MouseButton::Right) => {
                let interaction = self
                    .sticky_toaster
                    .handle_click(mouse.column, mouse.row, ToastMouseButton::Right);
                self.handle_toast_interaction(interaction)
            }
            _ => false,
        }
    }

    fn handle_toast_interaction(&mut self, interaction: ToastInteraction) -> bool {
        match interaction {
            ToastInteraction::None => false,
            ToastInteraction::Dismissed => true,
            ToastInteraction::CopyRequested(text) => {
                self.copy_text_to_clipboard(&text);
                true
            }
        }
    }

    fn copy_text_to_clipboard(&mut self, text: &str) {
        let Ok(mut clipboard) = Clipboard::new() else {
            self.status = StatusMessage::warning("Clipboard is not available in this environment.");
            return;
        };

        if clipboard.set_text(text.to_string()).is_ok() {
            self.status = StatusMessage::info("Copied to clipboard.");
        } else {
            self.status = StatusMessage::warning("Clipboard copy failed.");
        }
    }

    fn sync_status_toasts(&mut self) {
        if self.status.id == self.last_status_toast_id {
            return;
        }

        self.last_status_toast_id = self.status.id;
        let builder = ToastBuilder::new(self.status.text.clone().into());
        match self.status.kind {
            StatusKind::Info => self.transient_toaster.show_toast(builder.toast_type(ToastType::Info)),
            StatusKind::Success => self.transient_toaster.show_toast(builder.toast_type(ToastType::Success)),
            StatusKind::Warning => self.transient_toaster.show_toast(builder.toast_type(ToastType::Warning)),
            StatusKind::Error => self.sticky_toaster.show_toast(
                builder
                    .toast_type(ToastType::Error)
                    .keep_on(1),
            ),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Screen {
    Dashboard,
    Wizard,
    Settings,
    UiSettings,
}

#[derive(Clone)]
struct HitTarget {
    rect: Rect,
    action: HitAction,
}

impl HitTarget {
    fn new(rect: Rect, action: HitAction) -> Self {
        Self { rect, action }
    }

    fn contains(&self, column: u16, row: u16) -> bool {
        column >= self.rect.x
            && column < self.rect.x + self.rect.width
            && row >= self.rect.y
            && row < self.rect.y + self.rect.height
    }
}

#[derive(Clone)]
enum HitAction {
    Switch(Screen),
    Quit,
    SelectProject(usize),
    OpenProjectEdit,
    EditProjectField(ProjectEditFocus),
    SaveProjectEdit,
    RemoveProject,
    CancelProjectEdit,
    ToggleTabHints,
    BrowseWizardTargetPath,
    BrowseWizardRepoRoot,
    BrowseProjectTargetPath,
    BrowseProjectRepoRoot,
    BrowserSelect(usize),
    SelectRecentChangesTab(RecentChangesTab),
    CloseRecentChanges,
    ScrollRecentChanges(i16),
    OpenTagDialog,
    CycleTagAction(isize),
    CycleBumpAction(isize),
    ApplyBump,
    CancelBump,
    CreateTag,
    CancelTagDialog,
    WizardField(WizardField),
    ValidateWizard,
    SaveWizard,
    CancelWizard,
}

#[derive(Clone)]
struct ProjectEditDialog {
    project_index: usize,
    project_name: String,
    name: TextInput,
    branch_name: TextInput,
    target_path: TextInput,
    target_key: TextInput,
    repo_root: TextInput,
    remote_url: TextInput,
    project_type: ProjectType,
    integration_mode: IntegrationMode,
    version_scheme: VersionScheme,
    focus: ProjectEditFocus,
}

impl ProjectEditDialog {
    fn from_project(project_index: usize, project: &ProjectConfig) -> Result<Self> {
        let primary_target = if project.project_type == ProjectType::AllInOne {
            project.targets.first()
        } else {
            project.branches.first().and_then(|branch| branch.targets.first())
        }
        .ok_or_else(|| anyhow!("selected project does not contain any editable targets yet"))?;

        let branch_name = if project.project_type == ProjectType::Branched {
            project
                .branches
                .first()
                .map(|branch| branch.name.clone())
                .ok_or_else(|| anyhow!("branched project does not contain any branches"))?
        } else {
            String::new()
        };

        let repo_root = project.repo.as_ref().map(|repo| repo.local_root.clone()).unwrap_or_default();
        let remote_url = project
            .repo
            .as_ref()
            .and_then(|repo| repo.remote_url.clone())
            .unwrap_or_default();

        Ok(Self {
            project_index,
            project_name: project.name.clone(),
            name: TextInput::with_value(project.name.clone()),
            branch_name: TextInput::with_value(branch_name),
            target_path: TextInput::with_value(primary_target.path.clone()),
            target_key: TextInput::with_value(primary_target.key_path.clone()),
            repo_root: TextInput::with_value(repo_root),
            remote_url: TextInput::with_value(remote_url),
            project_type: project.project_type,
            integration_mode: project.integration_mode,
            version_scheme: project.version_scheme,
            focus: ProjectEditFocus::Name,
        })
    }

    fn focus_next(&mut self) {
        let fields = self.visible_fields();
        let index = fields.iter().position(|field| *field == self.focus).unwrap_or(0);
        self.focus = fields[(index + 1) % fields.len()];
        self.ensure_focus_visible();
    }

    fn focus_previous(&mut self) {
        let fields = self.visible_fields();
        let index = fields.iter().position(|field| *field == self.focus).unwrap_or(0);
        self.focus = fields[(index + fields.len() - 1) % fields.len()];
        self.ensure_focus_visible();
    }

    fn is_save_focused(&self) -> bool {
        self.focus == ProjectEditFocus::Save
    }

    fn is_remove_focused(&self) -> bool {
        self.focus == ProjectEditFocus::Remove
    }

    fn is_cancel_focused(&self) -> bool {
        self.focus == ProjectEditFocus::Cancel
    }

    fn focus_accepts_text(&self) -> bool {
        matches!(
            self.focus,
            ProjectEditFocus::Name
                | ProjectEditFocus::BranchName
                | ProjectEditFocus::TargetPath
                | ProjectEditFocus::TargetKey
                | ProjectEditFocus::RepoRoot
                | ProjectEditFocus::RemoteUrl
        )
    }

    fn visible_fields(&self) -> Vec<ProjectEditFocus> {
        let mut fields = vec![ProjectEditFocus::Name, ProjectEditFocus::ProjectType];
        if self.project_type == ProjectType::Branched {
            fields.push(ProjectEditFocus::BranchName);
        }
        fields.extend([
            ProjectEditFocus::VersionScheme,
            ProjectEditFocus::IntegrationMode,
            ProjectEditFocus::TargetPath,
            ProjectEditFocus::TargetKey,
        ]);
        if self.integration_mode.requires_repo() {
            fields.push(ProjectEditFocus::RepoRoot);
        }
        if self.integration_mode.requires_remote() {
            fields.push(ProjectEditFocus::RemoteUrl);
        }
        fields.extend([ProjectEditFocus::Save, ProjectEditFocus::Remove, ProjectEditFocus::Cancel]);
        fields
    }

    fn render_field(&self, field: ProjectEditFocus) -> (&'static str, HitAction) {
        match field {
            ProjectEditFocus::Name => ("Project name", HitAction::EditProjectField(field)),
            ProjectEditFocus::ProjectType => ("Project type", HitAction::EditProjectField(field)),
            ProjectEditFocus::BranchName => ("Initial branch", HitAction::EditProjectField(field)),
            ProjectEditFocus::VersionScheme => ("Version scheme", HitAction::EditProjectField(field)),
            ProjectEditFocus::IntegrationMode => ("Integration", HitAction::EditProjectField(field)),
            ProjectEditFocus::TargetPath => ("Target path", HitAction::EditProjectField(field)),
            ProjectEditFocus::TargetKey => ("Target key", HitAction::EditProjectField(field)),
            ProjectEditFocus::RepoRoot => ("Repo root", HitAction::EditProjectField(field)),
            ProjectEditFocus::RemoteUrl => ("Remote URL", HitAction::EditProjectField(field)),
            ProjectEditFocus::Save => ("Save", HitAction::SaveProjectEdit),
            ProjectEditFocus::Remove => ("Remove", HitAction::RemoveProject),
            ProjectEditFocus::Cancel => ("Cancel", HitAction::CancelProjectEdit),
        }
    }

    fn display_value_for_field(&self, field: ProjectEditFocus, focused: bool, max_width: usize) -> String {
        match field {
            ProjectEditFocus::Name => self.name.display_value_with_width(focused, max_width),
            ProjectEditFocus::ProjectType => format!("< {} >", self.project_type.display_name()),
            ProjectEditFocus::BranchName => self.branch_name.display_value_with_width(focused, max_width),
            ProjectEditFocus::VersionScheme => format!("< {} >", self.version_scheme.display_name()),
            ProjectEditFocus::IntegrationMode => format!("< {} >", self.integration_mode.display_name()),
            ProjectEditFocus::TargetPath => self.target_path.display_value_with_width(focused, max_width),
            ProjectEditFocus::TargetKey => self.target_key.display_value_with_width(focused, max_width),
            ProjectEditFocus::RepoRoot => self.repo_root.display_value_with_width(focused, max_width),
            ProjectEditFocus::RemoteUrl => self.remote_url.display_value_with_width(focused, max_width),
            ProjectEditFocus::Save => "Persist project".to_string(),
            ProjectEditFocus::Remove => "Delete project".to_string(),
            ProjectEditFocus::Cancel => "Discard changes".to_string(),
        }
    }

    fn adjust_current_enum(&mut self, delta: i32) {
        match self.focus {
            ProjectEditFocus::ProjectType => {
                self.project_type = if delta >= 0 {
                    self.project_type.next()
                } else {
                    self.project_type.previous()
                };
            }
            ProjectEditFocus::VersionScheme => {
                self.version_scheme = if delta >= 0 {
                    self.version_scheme.next()
                } else {
                    self.version_scheme.previous()
                };
            }
            ProjectEditFocus::IntegrationMode => {
                self.integration_mode = if delta >= 0 {
                    self.integration_mode.next()
                } else {
                    self.integration_mode.previous()
                };
            }
            _ => {}
        }
        self.prefill_repo_root_from_target_path();
        self.ensure_focus_visible();
    }

    fn handle_text_input(&mut self, key: KeyEvent) {
        let Some(input) = self.active_input_mut() else {
            return;
        };
        match key.code {
            KeyCode::Char(character) if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT => input.insert(character),
            KeyCode::Backspace => input.backspace(),
            KeyCode::Delete => input.delete(),
            KeyCode::Left => input.move_left(),
            KeyCode::Right => input.move_right(),
            KeyCode::Home => input.home(),
            KeyCode::End => input.end(),
            _ => {}
        }
        if self.focus == ProjectEditFocus::TargetPath {
            self.prefill_repo_root_from_target_path();
        }
    }

    fn insert_text(&mut self, text: &str) -> bool {
        if let Some(input) = self.active_input_mut() {
            input.insert_str(text);
            if self.focus == ProjectEditFocus::TargetPath {
                self.prefill_repo_root_from_target_path();
            }
            return true;
        }
        false
    }

    fn active_input_mut(&mut self) -> Option<&mut TextInput> {
        match self.focus {
            ProjectEditFocus::Name => Some(&mut self.name),
            ProjectEditFocus::BranchName => Some(&mut self.branch_name),
            ProjectEditFocus::TargetPath => Some(&mut self.target_path),
            ProjectEditFocus::TargetKey => Some(&mut self.target_key),
            ProjectEditFocus::RepoRoot => Some(&mut self.repo_root),
            ProjectEditFocus::RemoteUrl => Some(&mut self.remote_url),
            _ => None,
        }
    }

    fn ensure_focus_visible(&mut self) {
        let fields = self.visible_fields();
        if !fields.contains(&self.focus) {
            self.focus = fields.first().copied().unwrap_or(ProjectEditFocus::Name);
        }
    }

    fn prefill_repo_root_from_target_path(&mut self) {
        if !self.repo_root.is_empty() {
            return;
        }
        if let Some(repo_root) = derive_repo_root_from_target_path(self.target_path.value()) {
            self.repo_root.set_value(repo_root);
        }
    }

    fn set_target_path_from_browse(&mut self, path: String) {
        self.target_path.set_value(path);
        self.prefill_repo_root_from_target_path();
    }

    fn set_repo_root_from_browse(&mut self, path: String) {
        self.repo_root.set_value(path);
    }

    fn apply(&self, project: &mut ProjectConfig) -> Result<()> {
        let project_name = self.name.value.trim();
        if project_name.is_empty() {
            bail!("project name cannot be empty");
        }

        let target_path = self.target_path.value.trim();
        if target_path.is_empty() {
            bail!("target path cannot be empty");
        }

        let target_key = self.target_key.value.trim();
        if target_key.is_empty() {
            bail!("target key cannot be empty");
        }

        let branch_name = self.branch_name.value.trim();
        if self.project_type == ProjectType::Branched && branch_name.is_empty() {
            bail!("initial branch cannot be empty");
        }

        let existing_target = if project.project_type == ProjectType::AllInOne {
            project.targets.first()
        } else {
            project.branches.first().and_then(|branch| branch.targets.first())
        }
        .ok_or_else(|| anyhow!("selected project does not contain any editable targets yet"))?
        .clone();

        project.name = project_name.to_string();
        project.project_type = self.project_type;
        project.integration_mode = self.integration_mode;
        project.unified_versioning = true;
        project.version_scheme = self.version_scheme;

        let target = TargetSpec {
            label: existing_target.label,
            path: target_path.to_string(),
            key_path: target_key.to_string(),
            format: existing_target.format,
        };

        if self.project_type == ProjectType::AllInOne {
            project.targets = vec![target];
            project.branches.clear();
        } else {
            project.targets.clear();
            project.branches = vec![BranchConfig {
                name: branch_name.to_string(),
                version_scheme: self.version_scheme,
                targets: vec![target],
            }];
        }

        if self.integration_mode.requires_repo() {
            let repo_root = self.repo_root.value.trim();
            if repo_root.is_empty() {
                bail!("repo root cannot be empty");
            }
            if !Path::new(repo_root).is_dir() {
                bail!("repo root does not exist: {}", repo_root);
            }

            let remote_url = if self.integration_mode.requires_remote() {
                let remote_url = self.remote_url.value.trim();
                if remote_url.is_empty() {
                    bail!("remote URL cannot be empty");
                }
                Some(remote_url.to_string())
            } else {
                None
            };

            project.repo = Some(RepoConfig {
                local_root: repo_root.to_string(),
                remote_url,
            });
        } else {
            project.repo = None;
        }

        Ok(())
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ProjectEditFocus {
    Name,
    ProjectType,
    BranchName,
    VersionScheme,
    IntegrationMode,
    TargetPath,
    TargetKey,
    RepoRoot,
    RemoteUrl,
    Save,
    Remove,
    Cancel,
}

#[derive(Clone)]
struct StatusMessage {
    id: u64,
    kind: StatusKind,
    text: String,
}

impl StatusMessage {
    fn info(text: impl Into<String>) -> Self {
        Self::new(StatusKind::Info, text)
    }

    fn success(text: impl Into<String>) -> Self {
        Self::new(StatusKind::Success, text)
    }

    fn warning(text: impl Into<String>) -> Self {
        Self::new(StatusKind::Warning, text)
    }

    fn error(text: impl Into<String>) -> Self {
        Self::new(StatusKind::Error, text)
    }

    fn new(kind: StatusKind, text: impl Into<String>) -> Self {
        static NEXT_STATUS_ID: AtomicU64 = AtomicU64::new(1);
        Self {
            id: NEXT_STATUS_ID.fetch_add(1, Ordering::Relaxed),
            kind,
            text: text.into(),
        }
    }
}

#[derive(Clone, Copy)]
enum StatusKind {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum WizardField {
    Name,
    ProjectType,
    BranchName,
    VersionScheme,
    IntegrationMode,
    TargetPath,
    TargetKey,
    RepoRoot,
    RemoteUrl,
    Validate,
    Save,
    Cancel,
}

#[derive(Clone)]
struct ProjectWizard {
    name: TextInput,
    branch_name: TextInput,
    target_path: TextInput,
    target_key: TextInput,
    repo_root: TextInput,
    remote_url: TextInput,
    project_type: ProjectType,
    integration_mode: IntegrationMode,
    version_scheme: VersionScheme,
    focus: WizardField,
    last_probe: Option<TargetProbe>,
}

impl Default for ProjectWizard {
    fn default() -> Self {
        Self {
            name: TextInput::with_value(""),
            branch_name: TextInput::with_value("core"),
            target_path: TextInput::with_value(""),
            target_key: TextInput::with_value("version"),
            repo_root: TextInput::with_value(""),
            remote_url: TextInput::with_value(""),
            project_type: ProjectType::AllInOne,
            integration_mode: IntegrationMode::LocalOnly,
            version_scheme: VersionScheme::SemVer,
            focus: WizardField::Name,
            last_probe: None,
        }
    }
}

impl ProjectWizard {
    fn focus_accepts_text(&self) -> bool {
        matches!(
            self.focus,
            WizardField::Name
                | WizardField::BranchName
                | WizardField::TargetPath
                | WizardField::TargetKey
                | WizardField::RepoRoot
                | WizardField::RemoteUrl
        )
    }

    fn visible_fields(&self) -> Vec<WizardField> {
        let mut fields = vec![
            WizardField::Name,
            WizardField::ProjectType,
        ];
        if self.project_type == ProjectType::Branched {
            fields.push(WizardField::BranchName);
        }
        fields.extend([
            WizardField::VersionScheme,
            WizardField::IntegrationMode,
            WizardField::TargetPath,
            WizardField::TargetKey,
        ]);
        if self.integration_mode.requires_repo() {
            fields.push(WizardField::RepoRoot);
        }
        if self.integration_mode.requires_remote() {
            fields.push(WizardField::RemoteUrl);
        }
        fields.extend([WizardField::Validate, WizardField::Save, WizardField::Cancel]);
        fields
    }

    fn focus_next(&mut self) {
        let fields = self.visible_fields();
        let index = fields.iter().position(|field| *field == self.focus).unwrap_or(0);
        self.focus = fields[(index + 1) % fields.len()];
        self.ensure_focus_visible();
    }

    fn focus_previous(&mut self) {
        let fields = self.visible_fields();
        let index = fields.iter().position(|field| *field == self.focus).unwrap_or(0);
        self.focus = fields[(index + fields.len() - 1) % fields.len()];
        self.ensure_focus_visible();
    }

    fn render_field(&self, field: WizardField) -> (&'static str, HitAction) {
        match field {
            WizardField::Name => ("Project name", HitAction::WizardField(field)),
            WizardField::ProjectType => (
                "Project type",
                HitAction::WizardField(field),
            ),
            WizardField::BranchName => (
                "Initial branch",
                HitAction::WizardField(field),
            ),
            WizardField::VersionScheme => (
                "Version scheme",
                HitAction::WizardField(field),
            ),
            WizardField::IntegrationMode => (
                "Integration",
                HitAction::WizardField(field),
            ),
            WizardField::TargetPath => ("Target path", HitAction::WizardField(field)),
            WizardField::TargetKey => ("Target key", HitAction::WizardField(field)),
            WizardField::RepoRoot => ("Repo root", HitAction::WizardField(field)),
            WizardField::RemoteUrl => ("Remote URL", HitAction::WizardField(field)),
            WizardField::Validate => ("Read", HitAction::ValidateWizard),
            WizardField::Save => ("Save", HitAction::SaveWizard),
            WizardField::Cancel => ("Cancel", HitAction::CancelWizard),
        }
    }

    fn display_value_for_field(&self, field: WizardField, focused: bool, max_width: usize) -> String {
        match field {
            WizardField::Name => self.name.display_value_with_width(focused, max_width),
            WizardField::ProjectType => format!("< {} >", self.project_type.display_name()),
            WizardField::BranchName => self.branch_name.display_value_with_width(focused, max_width),
            WizardField::VersionScheme => format!("< {} >", self.version_scheme.display_name()),
            WizardField::IntegrationMode => format!("< {} >", self.integration_mode.display_name()),
            WizardField::TargetPath => self.target_path.display_value_with_width(focused, max_width),
            WizardField::TargetKey => self.target_key.display_value_with_width(focused, max_width),
            WizardField::RepoRoot => self.repo_root.display_value_with_width(focused, max_width),
            WizardField::RemoteUrl => self.remote_url.display_value_with_width(focused, max_width),
            WizardField::Validate => "Validate target".to_string(),
            WizardField::Save => "Persist project".to_string(),
            WizardField::Cancel => "Discard changes".to_string(),
        }
    }

    fn adjust_current_enum(&mut self, delta: i32) {
        match self.focus {
            WizardField::ProjectType => {
                self.project_type = if delta >= 0 {
                    self.project_type.next()
                } else {
                    self.project_type.previous()
                };
            }
            WizardField::VersionScheme => {
                self.version_scheme = if delta >= 0 {
                    self.version_scheme.next()
                } else {
                    self.version_scheme.previous()
                };
                self.last_probe = None;
            }
            WizardField::IntegrationMode => {
                self.integration_mode = if delta >= 0 {
                    self.integration_mode.next()
                } else {
                    self.integration_mode.previous()
                };
            }
            _ => {}
        }
        self.prefill_repo_root_from_target_path();
        self.ensure_focus_visible();
    }

    fn handle_text_input(&mut self, key: KeyEvent) {
        let Some(input) = self.active_input_mut() else {
            return;
        };
        match key.code {
            KeyCode::Char(character) if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT => {
                input.insert(character);
                self.after_text_edit();
            }
            KeyCode::Backspace => {
                input.backspace();
                self.after_text_edit();
            }
            KeyCode::Delete => {
                input.delete();
                self.after_text_edit();
            }
            KeyCode::Left => input.move_left(),
            KeyCode::Right => input.move_right(),
            KeyCode::Home => input.home(),
            KeyCode::End => input.end(),
            _ => {}
        }
    }

    fn active_input_mut(&mut self) -> Option<&mut TextInput> {
        match self.focus {
            WizardField::Name => Some(&mut self.name),
            WizardField::BranchName => Some(&mut self.branch_name),
            WizardField::TargetPath => Some(&mut self.target_path),
            WizardField::TargetKey => Some(&mut self.target_key),
            WizardField::RepoRoot => Some(&mut self.repo_root),
            WizardField::RemoteUrl => Some(&mut self.remote_url),
            _ => None,
        }
    }

    fn insert_text(&mut self, text: &str) -> bool {
        if let Some(input) = self.active_input_mut() {
            input.insert_str(text);
            self.after_text_edit();
            return true;
        }
        false
    }

    fn after_text_edit(&mut self) {
        if matches!(self.focus, WizardField::TargetPath | WizardField::TargetKey) {
            self.last_probe = None;
        }
        if self.focus == WizardField::TargetPath {
            self.prefill_repo_root_from_target_path();
        }
    }

    fn ensure_focus_visible(&mut self) {
        let fields = self.visible_fields();
        if !fields.contains(&self.focus) {
            self.focus = fields.first().copied().unwrap_or(WizardField::Name);
        }
    }

    fn prefill_repo_root_from_target_path(&mut self) {
        if !self.repo_root.is_empty() {
            return;
        }
        if let Some(repo_root) = derive_repo_root_from_target_path(self.target_path.value()) {
            self.repo_root.set_value(repo_root);
        }
    }

    fn set_target_path_from_browse(&mut self, path: String) {
        self.target_path.set_value(path);
        self.last_probe = None;
        self.prefill_repo_root_from_target_path();
    }

    fn set_repo_root_from_browse(&mut self, path: String) {
        self.repo_root.set_value(path);
    }

    fn build_project(&self) -> Result<ProjectConfig> {
        if self.name.value.trim().is_empty() {
            bail!("project name is required");
        }
        if self.target_path.value.trim().is_empty() {
            bail!("target path is required");
        }
        if self.target_key.value.trim().is_empty() {
            bail!("target key is required");
        }
        match &self.last_probe {
            Some(probe) if matches!(probe.kind, ProbeKind::Success) => {}
            Some(_) => bail!("read the target successfully before saving"),
            None => bail!("validate the target before saving"),
        }

        let target = TargetSpec {
            label: "Version".to_string(),
            path: self.target_path.value.trim().to_string(),
            key_path: self.target_key.value.trim().to_string(),
            format: self.last_probe.as_ref().and_then(|probe| probe.format).unwrap_or(TargetFormat::Auto),
        };

        let repo = if self.integration_mode.requires_repo() {
            let root = self.repo_root.value.trim();
            if root.is_empty() {
                bail!("repo root is required for git-backed projects");
            }
            if !Path::new(root).is_dir() {
                bail!("repo root does not exist: {}", root);
            }
            let remote = if self.integration_mode.requires_remote() {
                let value = self.remote_url.value.trim();
                if value.is_empty() {
                    bail!("remote URL is required for GitHub-enabled projects");
                }
                Some(value.to_string())
            } else {
                None
            };
            Some(RepoConfig { local_root: root.to_string(), remote_url: remote })
        } else {
            None
        };

        let project = if self.project_type == ProjectType::AllInOne {
            ProjectConfig {
                name: self.name.value.trim().to_string(),
                project_type: ProjectType::AllInOne,
                integration_mode: self.integration_mode,
                unified_versioning: true,
                version_scheme: self.version_scheme,
                targets: vec![target],
                branches: Vec::new(),
                repo,
            }
        } else {
            let branch_name = self.branch_name.value.trim();
            if branch_name.is_empty() {
                bail!("initial branch name is required");
            }
            ProjectConfig {
                name: self.name.value.trim().to_string(),
                project_type: ProjectType::Branched,
                integration_mode: self.integration_mode,
                unified_versioning: true,
                version_scheme: self.version_scheme,
                targets: Vec::new(),
                branches: vec![BranchConfig {
                    name: branch_name.to_string(),
                    version_scheme: self.version_scheme,
                    targets: vec![target],
                }],
                repo,
            }
        };

        Ok(project)
    }
}

fn sanitize_pasted_text(text: &str) -> String {
    text.chars().filter(|character| *character != '\r' && *character != '\n').collect()
}

#[derive(Clone)]
struct DialogButton {
    label: &'static str,
    focused: bool,
    action: HitAction,
    style: Style,
}

impl DialogButton {
    fn new(label: &'static str, focused: bool, action: HitAction, style: Style) -> Self {
        Self {
            label,
            focused,
            action,
            style,
        }
    }
}

#[derive(Clone, Copy)]
enum BrowseTarget {
    WizardTargetPath,
    WizardRepoRoot,
    ProjectEditTargetPath,
    ProjectEditRepoRoot,
}

struct FileBrowserDialog {
    title: &'static str,
    target: BrowseTarget,
    explorer: FileExplorer,
    select_directories: bool,
}

impl FileBrowserDialog {
    fn new(target: BrowseTarget, initial_path: String) -> Result<Self> {
        let select_directories = matches!(
            target,
            BrowseTarget::WizardRepoRoot | BrowseTarget::ProjectEditRepoRoot
        );
        let explorer = configure_file_explorer(FileExplorerBuilder::default(), &initial_path, select_directories)?;
        let title = if select_directories {
            "Browse Repo Root"
        } else {
            "Browse Target Path"
        };

        Ok(Self {
            title,
            target,
            explorer,
            select_directories,
        })
    }
}

fn configure_file_explorer(
    builder: FileExplorerBuilder,
    initial_path: &str,
    select_directories: bool,
) -> Result<FileExplorer> {
    let initial = initial_path.trim();
    if initial.is_empty() {
        return builder.build().map_err(anyhow::Error::from);
    }

    let path = PathBuf::from(initial);
    if path.is_file() && !select_directories {
        return builder.working_file(path).build().map_err(anyhow::Error::from);
    }
    if path.is_dir() {
        if select_directories {
            return builder.working_file(path).build().map_err(anyhow::Error::from);
        }
        return builder.working_dir(path).build().map_err(anyhow::Error::from);
    }

    if let Some(parent) = path.parent().filter(|parent| parent.is_dir()) {
        return builder.working_dir(parent.to_path_buf()).build().map_err(anyhow::Error::from);
    }

    builder.build().map_err(anyhow::Error::from)
}

fn visible_field_width(row_width: u16, has_browse: bool) -> usize {
    let browse_width = if has_browse {
        BROWSE_BUTTON_WIDTH + 1
    } else {
        0
    };
    row_width
        .saturating_sub(FORM_LABEL_WIDTH)
        .saturating_sub(browse_width)
        .saturating_sub(2)
        .max(1) as usize
}

fn derive_repo_root_from_target_path(path: &str) -> Option<String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return None;
    }

    Path::new(trimmed)
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(|parent| parent.display().to_string())
}

fn wizard_browse_action(field: WizardField) -> Option<HitAction> {
    match field {
        WizardField::TargetPath => Some(HitAction::BrowseWizardTargetPath),
        WizardField::RepoRoot => Some(HitAction::BrowseWizardRepoRoot),
        _ => None,
    }
}

fn project_edit_browse_action(field: ProjectEditFocus) -> Option<HitAction> {
    match field {
        ProjectEditFocus::TargetPath => Some(HitAction::BrowseProjectTargetPath),
        ProjectEditFocus::RepoRoot => Some(HitAction::BrowseProjectRepoRoot),
        _ => None,
    }
}

fn browser_visible_range(total: usize, selected: usize, height: usize) -> (usize, usize) {
    if total == 0 || height == 0 {
        return (0, 0);
    }

    let start = selected.saturating_sub(height / 2).min(total.saturating_sub(height));
    let end = (start + height).min(total);
    (start, end)
}

fn main_screen_from_index(index: usize) -> Screen {
    match index {
        1 => Screen::Wizard,
        2 => Screen::Settings,
        3 => Screen::UiSettings,
        _ => Screen::Dashboard,
    }
}

impl App {
    fn main_tab_labels(&self) -> Vec<String> {
        ["Dashboard", "New Project", "Settings", "UI Settings"]
            .into_iter()
            .enumerate()
            .map(|(index, label)| {
                if self.config.ui.show_tab_hints {
                    format!("{} [{}]", label, index + 1)
                } else {
                    label.to_string()
                }
            })
            .collect()
    }

    fn current_main_tab_index(&self) -> usize {
        match self.screen {
            Screen::Dashboard => 0,
            Screen::Wizard => 1,
            Screen::Settings => 2,
            Screen::UiSettings => 3,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_repo_root_uses_parent_directory() {
        let derived = derive_repo_root_from_target_path("C:/repo/subdir/package.json");
        assert_eq!(derived.as_deref(), Some("C:/repo/subdir"));
    }

    #[test]
    fn editing_repo_root_does_not_invalidate_target_probe() {
        let mut wizard = ProjectWizard::default();
        wizard.last_probe = Some(TargetProbe {
            kind: ProbeKind::Success,
            message: "ok".to_string(),
            version: Some("1.2.3".to_string()),
            format: Some(TargetFormat::Json),
        });
        wizard.focus = WizardField::RepoRoot;

        wizard.insert_text("C:/repo");

        assert!(matches!(wizard.last_probe.as_ref().map(|probe| probe.kind), Some(ProbeKind::Success)));
    }

    #[test]
    fn editing_target_path_invalidates_target_probe() {
        let mut wizard = ProjectWizard::default();
        wizard.last_probe = Some(TargetProbe {
            kind: ProbeKind::Success,
            message: "ok".to_string(),
            version: Some("1.2.3".to_string()),
            format: Some(TargetFormat::Json),
        });
        wizard.focus = WizardField::TargetPath;

        wizard.insert_text("C:/repo/package.json");

        assert!(wizard.last_probe.is_none());
    }
}
