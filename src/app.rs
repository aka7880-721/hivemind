// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyVersionBumper License v1.1.
// You may use, modify, and redistribute this file for non‑commercial purposes only,
// provided that attribution is preserved and Branding Elements remain intact.
//
// For details, see the LICENSE file in the repository root.

use std::{
    fs,
    io,
    path::Path,
    process::Command,
    time::Duration,
};

use anyhow::{Context, Result, anyhow, bail};
use arboard::Clipboard;
use chrono::Local;
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
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};
use toml_edit::{DocumentMut, Item, Value, value};

use crate::{
    branding::{ASCII_HEADER, PixelLogo},
    config::{
        AppConfig, BranchConfig, ConfigStore, IntegrationMode, ProjectConfig, ProjectType,
        RepoConfig, TargetFormat, TargetSpec,
    },
    versioning::{BumpAction, VersionScheme},
};

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
const SUPPORT_EMAIL: &str = " dev@comfyhome.io ";

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
    hit_targets: Vec<HitTarget>,
    status: StatusMessage,
    logo: PixelLogo,
    should_quit: bool,
}

impl App {
    fn new() -> Result<Self> {
        let config_store = ConfigStore::locate()?;
        let config = config_store.load()?;
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
            hit_targets: Vec::new(),
            status: StatusMessage::info("Press N to create your first project, or Q to quit."),
            logo: PixelLogo::load(),
            should_quit: false,
        })
    }

    fn draw(&mut self, frame: &mut Frame) {
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

        self.render_footer(frame, root[3]);
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

        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(4),
                Constraint::Length(logo.width()),
                Constraint::Length(6),
                Constraint::Min(30),
            ])
            .split(inner);

        let logo_area = center_vertically(chunks[1], logo.lines().len() as u16);
        frame.render_widget(Paragraph::new(logo.lines().to_vec()), logo_area);

        let version_label = format!("v{}", APP_VERSION);
        let banner = ASCII_HEADER
            .into_iter()
            .map(|line| {
                if let Some(index) = line.find("{APP_VERSION}") {
                    let prefix = &line[..index];
                    let suffix = &line[index + "{APP_VERSION}".len()..];
                    let mut spans = Vec::new();
                    spans.push(Span::styled(prefix, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)));
                    spans.push(Span::styled(&version_label, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)));
                    spans.push(Span::styled(suffix, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)));
                    Line::from(spans)
                } else {
                    Line::from(Span::styled(line, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)))
                }
            })
            .collect::<Vec<_>>();

        let banner_area = center_vertically(chunks[3], banner.len() as u16);
        frame.render_widget(Paragraph::new(banner), banner_area);
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
        let labels = [
            ("Dashboard", Screen::Dashboard),
            ("New Project", Screen::Wizard),
            ("Settings", Screen::Settings),
            ("Quit", Screen::Dashboard),
        ];

        let widths = labels
            .iter()
            .map(|(label, _)| Constraint::Length(label.len() as u16 + 4))
            .collect::<Vec<_>>();
        let layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(widths)
            .split(area);

        for (index, ((label, screen), rect)) in labels.into_iter().zip(layout.into_iter()).enumerate() {
            let mut style = Style::default().fg(Color::White);
            if index < 3 && self.screen == screen {
                style = style.bg(Color::DarkGray).add_modifier(Modifier::BOLD);
            }
            if index == 3 {
                style = Style::default().fg(Color::Red);
                self.hit_targets.push(HitTarget::new(*rect, HitAction::Quit));
            } else {
                self.hit_targets.push(HitTarget::new(*rect, HitAction::Switch(screen)));
            }
            frame.render_widget(Paragraph::new(format!(" {} ", label)).style(style), *rect);
        }
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
                lines.push(Line::from("- V opens recent git changes from the configured repo"));
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
        let block = Block::default().borders(Borders::ALL).title(" Recent Changes ").border_style(Style::default().fg(Color::Cyan));
        let inner = block.inner(popup);
        frame.render_widget(block, popup);

        let sections = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(5), Constraint::Min(8), Constraint::Length(3)])
            .split(inner);

        let header = vec![
            Line::from(format!("Project: {}", dialog.project_name)).bold(),
            Line::from(format!("Repo: {}", dialog.repo_root)),
            Line::from(format!("View: {}", dialog.range_label)),
            Line::from("Up/Down or mouse wheel scrolls. Esc closes."),
        ];
        frame.render_widget(Paragraph::new(header).wrap(Wrap { trim: false }), sections[0]);

        let body_block = Block::default().borders(Borders::ALL).title(" git log ");
        let body_inner = body_block.inner(sections[1]);
        frame.render_widget(body_block, sections[1]);
        let body = if dialog.lines.is_empty() {
            vec![Line::from("No recent changes to display.")]
        } else {
            dialog.lines.iter().cloned().map(Line::from).collect::<Vec<_>>()
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
            .split(sections[2]);
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
            .constraints([Constraint::Length(4), Constraint::Min(8), Constraint::Length(3)])
            .split(inner);

        let header = vec![
            Line::from(format!("Project: {}", dialog.project_name)).bold(),
            Line::from("Edit paths and git settings, then press F2 or Save."),
            Line::from("Tab/Shift+Tab moves between fields. Mouse clicks also focus fields."),
        ];
        frame.render_widget(Paragraph::new(header).wrap(Wrap { trim: false }), sections[0]);

        let field_rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Length(2); dialog.fields.len()])
            .split(sections[1]);
        for (index, (field, row)) in dialog.fields.iter().zip(field_rows.iter()).enumerate() {
            let focused = dialog.focus_index == index;
            let row_split = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Length(20), Constraint::Min(10)])
                .split(*row);
            frame.render_widget(Paragraph::new(field.label.clone()), row_split[0]);
            let style = if focused {
                Style::default().fg(Color::White)
            } else {
                Style::default().fg(Color::White)
            };
            let block = if focused {
                Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::Cyan))
            } else {
                Block::default().borders(Borders::ALL)
            };
            frame.render_widget(
                Paragraph::new(field.input.display_value(focused)).style(style).block(block),
                row_split[1],
            );
            self.hit_targets.push(HitTarget::new(*row, HitAction::EditProjectField(index)));
        }

        let buttons = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(18), Constraint::Min(10)])
            .split(sections[2]);
        frame.render_widget(
            Paragraph::new(" Save ").block(Block::default().borders(Borders::ALL).title(" action ")).style(Style::default().fg(Color::Green)),
            buttons[0],
        );
        frame.render_widget(
            Paragraph::new(" Cancel ").block(Block::default().borders(Borders::ALL).title(" action ")).style(Style::default().fg(Color::Red)),
            buttons[1],
        );
        self.hit_targets.push(HitTarget::new(buttons[0], HitAction::SaveProjectEdit));
        self.hit_targets.push(HitTarget::new(buttons[1], HitAction::CancelProjectEdit));
    }

    fn render_wizard(&mut self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(58), Constraint::Percentage(42)])
            .split(area);

        let block = Block::default().borders(Borders::ALL).title(" New Project Wizard ");
        let inner = block.inner(chunks[0]);
        frame.render_widget(block, chunks[0]);

        let fields = self.wizard.visible_fields();
        let button_gap_count = fields
            .iter()
            .filter(|field| matches!(field, WizardField::Validate | WizardField::Save))
            .count() as u16; // add extra gap after validate and save buttons for better separation
        let preferred_row_height = 3; // use taller rows if they fit to improve button styling
        let row_height = if (fields.len() as u16 * preferred_row_height) + button_gap_count <= inner.height {
            preferred_row_height
        } else {
            2
        };
        let button_gap = if row_height >= 3 { 1 } else { 0 }; // add a gap after action buttons only if rows are tall enough to accommodate it without looking cramped

        let mut constraints = Vec::with_capacity(fields.len() + button_gap_count as usize);
        for field in &fields {
            constraints.push(Constraint::Length(row_height));
            if button_gap > 0 && matches!(field, WizardField::Validate | WizardField::Save) {
                constraints.push(Constraint::Length(button_gap));
            }
        }

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(inner);

        let mut row_index = 0; // separate index to account for button gaps in the layout
        for field in &fields {
            let row = rows[row_index];
            row_index += 1;
            if button_gap > 0 && matches!(field, WizardField::Validate | WizardField::Save) {
                row_index += 1;
            }

            let focused = *field == self.wizard.focus;
            let (label, action) = self.wizard.render_field(*field);
            let value = self.wizard.display_value_for_field(*field, focused);
            self.render_form_row(frame, row, label, value, focused, action.clone());
            self.hit_targets.push(HitTarget::new(row, action));
        }

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
            lines.push(Line::from("Use F5 or the Validate row to read the target file."));
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
        action: HitAction,
    ) {
        let label_width = 18;
        let label_area = center_vertically(
            Rect {
                x: area.x,
                y: area.y,
                width: label_width,
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

        let is_action_button = matches!(action, HitAction::ValidateWizard | HitAction::SaveWizard | HitAction::CancelWizard);
        let field_area = if is_action_button {
            let button_width = (value.len() as u16 + 4).max(12);
            Rect {
                x: area.x + label_width,
                y: area.y,
                width: button_width.min(area.width.saturating_sub(label_width)),
                height: area.height,
            }
        } else {
            Rect {
                x: area.x + label_width,
                y: area.y,
                width: area.width.saturating_sub(label_width),
                height: area.height,
            }
        };
        let field_area = center_vertically(field_area, area.height.min(3)); // vertically center the field, but cap its height at 3 for better button styling

        let (base_style, block) = match action {
            HitAction::ValidateWizard => (
                Style::default().fg(Color::Black).bg(Color::Yellow),
                Block::default().borders(Borders::ALL).title(" action "),
            ),
            HitAction::SaveWizard => (
                Style::default().fg(Color::Black).bg(Color::Green),
                Block::default().borders(Borders::ALL).title(" action "),
            ),
            HitAction::CancelWizard => (
                Style::default().fg(Color::White).bg(Color::Red),
                Block::default().borders(Borders::ALL).title(" action "),
            ),
            _ => (Style::default(), Block::default().borders(Borders::ALL)),
        };
        let style = if focused {
            if is_action_button {
                base_style.add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Rgb(235, 235, 235))
            }
        } else {
            if is_action_button {
                base_style
            } else {
                Style::default().fg(Color::Rgb(235, 235, 235))
            }
        };
        let block = if focused && !is_action_button {
            block.border_style(Style::default().fg(Color::Cyan))
        } else {
            block
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(value, style))).block(block),
            field_area,
        );
    }

    fn render_footer(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default().borders(Borders::ALL).title(" Controls ");
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let help = match self.screen {
            Screen::Settings if self.project_edit_dialog.is_some() => "Tab move | F2 save | Esc cancel",
            Screen::Dashboard if self.tag_dialog.is_some() => "Type tag name | Left/Right action | Enter run | Esc cancel",
            Screen::Dashboard if self.recent_changes_dialog.is_some() => "Up/Down scroll | T create tag | Esc close",
            Screen::Dashboard if self.bump_dialog.is_some() => "Left/Right change bump action | Enter apply | Esc cancel",
            Screen::Dashboard => "N new project | B bump | V recent changes | T create tag | Up/Down select | S settings | Q quit",
            Screen::Settings => "Up/Down select project | E edit selected project | D dashboard | N new project | Q quit",
            Screen::Wizard => "Tab move | Left/Right change enums | F5 read target | F2 save | Esc cancel",
        };
        let color = match self.status.kind {
            StatusKind::Info => Color::Cyan,
            StatusKind::Success => Color::Green,
            StatusKind::Warning => Color::Yellow,
            StatusKind::Error => Color::Red,
        };
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
            .split(inner);
        frame.render_widget(Paragraph::new(help), chunks[0]);
        frame.render_widget(Paragraph::new(self.status.text.clone()).style(Style::default().fg(color)), chunks[1]);
    }

    fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('v') {
            self.paste_from_clipboard();
            return Ok(());
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

        if key.code == KeyCode::Char('q') && key.modifiers.is_empty() {
            self.should_quit = true;
            return Ok(());
        }

        match self.screen {
            Screen::Dashboard => self.handle_dashboard_key(key),
            Screen::Settings => self.handle_settings_key(key),
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

    fn handle_wizard_key(&mut self, key: KeyEvent) -> Result<()> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s') {
            return self.save_wizard_project();
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
                self.status = StatusMessage::info("Recent changes closed.");
            }
            KeyCode::Up => self.scroll_recent_changes(-1),
            KeyCode::Down => self.scroll_recent_changes(1),
            KeyCode::PageUp => self.scroll_recent_changes(-8),
            KeyCode::PageDown => self.scroll_recent_changes(8),
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
                    if dialog.is_cancel_focused() {
                        self.project_edit_dialog = None;
                        self.status = StatusMessage::info("Project edit cancelled.");
                        return Ok(());
                    }
                }
            }
            KeyCode::F(2) => return self.save_project_edit(),
            _ => {
                if let Some(dialog) = &mut self.project_edit_dialog {
                    dialog.handle_text_input(key);
                }
            }
        }
        Ok(())
    }

    fn handle_mouse(&mut self, mouse: MouseEvent) {
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
                if let Some(action) = self.hit_targets.iter().find_map(|target| {
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
            HitAction::EditProjectField(index) => {
                if let Some(dialog) = &mut self.project_edit_dialog {
                    dialog.focus_index = index;
                }
            }
            HitAction::SaveProjectEdit => return self.save_project_edit(),
            HitAction::CancelProjectEdit => {
                self.project_edit_dialog = None;
                self.status = StatusMessage::info("Project edit cancelled.");
            }
            HitAction::CycleBumpAction(delta) => self.rotate_bump_action(delta),
            HitAction::ApplyBump => return self.apply_bump(),
            HitAction::CancelBump => {
                self.bump_dialog = None;
                self.status = StatusMessage::info("Bump preview closed.");
            }
            HitAction::CloseRecentChanges => {
                self.recent_changes_dialog = None;
                self.status = StatusMessage::info("Recent changes closed.");
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
        self.status = StatusMessage::info("Showing recent git history for the selected project.");
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

    fn open_project_edit_dialog(&mut self) -> Result<()> {
        let project_index = self.selected_project;
        let project = self.selected_project()?;
        let dialog = ProjectEditDialog::from_project(project_index, project)?;
        self.project_edit_dialog = Some(dialog);
        self.status = StatusMessage::info("Amend repo roots, remotes, or target paths, then save the project.");
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

    fn open_tag_dialog(&mut self) -> Result<()> {
        let project = self.selected_project()?.clone();
        let dialog = TagDialog::from_project(&project)?;
        self.bump_dialog = None;
        self.project_edit_dialog = None;
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
}

fn center_vertically(area: Rect, content_height: u16) -> Rect {
    let height = content_height.min(area.height);
    let offset = area.height.saturating_sub(height) / 2;
    Rect {
        x: area.x,
        y: area.y + offset,
        width: area.width,
        height,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Screen {
    Dashboard,
    Wizard,
    Settings,
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
    EditProjectField(usize),
    SaveProjectEdit,
    CancelProjectEdit,
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
struct RecentChangesDialog {
    project_name: String,
    repo_root: String,
    range_label: String,
    lines: Vec<String>,
    scroll: u16,
}

impl RecentChangesDialog {
    fn from_project(project: &ProjectConfig) -> Result<Self> {
        let repo_root = project_repo_root(project)?;
        ensure_git_repo(&repo_root)?;

        let describe = run_git(&repo_root, &["describe", "--tags", "--abbrev=0"])?;
        let (range_label, lines) = if describe.success {
            let tag = describe.stdout.trim().to_string();
            let range = format!("{}..HEAD", tag);
            let output = run_git_checked(&repo_root, &["log", "--oneline", "--graph", &range])?;
            let lines = split_output_lines(&output);
            (range, lines)
        } else {
            let output = run_git_checked(&repo_root, &["log", "--oneline", "--graph", "-n", "60"])?;
            ("no tags found; showing the latest 60 commits".to_string(), split_output_lines(&output))
        };

        Ok(Self {
            project_name: project.name.clone(),
            repo_root,
            range_label,
            lines,
            scroll: 0,
        })
    }
}

#[derive(Clone)]
struct TagDialog {
    project_name: String,
    repo_root: String,
    remote_spec: Option<String>,
    tag_name: TextInput,
    actions: Vec<TagAction>,
    action_index: usize,
}

impl TagDialog {
    fn from_project(project: &ProjectConfig) -> Result<Self> {
        let repo_root = project_repo_root(project)?;
        ensure_git_repo(&repo_root)?;
        let default_tag = suggested_tag_name(project);
        let remote_spec = project.repo.as_ref().and_then(|repo| repo.remote_url.clone());
        let actions = match project.integration_mode {
            IntegrationMode::LocalOnly => bail!("local-only projects do not support git tags"),
            IntegrationMode::GitLocalOnly => vec![TagAction::CreateLocal],
            IntegrationMode::GitHubEnabled => vec![
                TagAction::CreateLocal,
                TagAction::CreateAndPush,
                TagAction::CreatePushAndRelease,
            ],
        };
        Ok(Self {
            project_name: project.name.clone(),
            repo_root,
            remote_spec,
            tag_name: TextInput::with_value(default_tag),
            actions,
            action_index: 0,
        })
    }

    fn selected_action(&self) -> TagAction {
        self.actions[self.action_index]
    }

    fn rotate_action(&mut self, delta: isize) {
        if self.actions.len() <= 1 {
            self.action_index = 0;
            return;
        }
        let len = self.actions.len() as isize;
        self.action_index = (self.action_index as isize + delta).rem_euclid(len) as usize;
    }
}

#[derive(Clone, Copy)]
enum TagAction {
    CreateLocal,
    CreateAndPush,
    CreatePushAndRelease,
}

impl TagAction {
    fn display_name(self) -> &'static str {
        match self {
            TagAction::CreateLocal => "Local Tag",
            TagAction::CreateAndPush => "Tag + Push",
            TagAction::CreatePushAndRelease => "Tag + Push + Release",
        }
    }
}

#[derive(Clone)]
struct ProjectEditDialog {
    project_index: usize,
    project_name: String,
    fields: Vec<ProjectEditField>,
    focus_index: usize,
}

impl ProjectEditDialog {
    fn from_project(project_index: usize, project: &ProjectConfig) -> Result<Self> {
        let mut fields = Vec::new();
        if let Some(repo) = &project.repo {
            fields.push(ProjectEditField {
                label: "Repo root".to_string(),
                input: TextInput::with_value(repo.local_root.clone()),
                kind: ProjectEditFieldKind::RepoRoot,
            });
            if project.integration_mode.requires_remote() {
                fields.push(ProjectEditField {
                    label: "Remote URL".to_string(),
                    input: TextInput::with_value(repo.remote_url.clone().unwrap_or_default()),
                    kind: ProjectEditFieldKind::RemoteUrl,
                });
            }
        }

        if project.project_type == ProjectType::AllInOne {
            for (target_index, target) in project.targets.iter().enumerate() {
                fields.push(ProjectEditField {
                    label: format!("Target {} path", target_index + 1),
                    input: TextInput::with_value(target.path.clone()),
                    kind: ProjectEditFieldKind::TargetPath { branch_index: None, target_index },
                });
            }
        } else {
            for (branch_index, branch) in project.branches.iter().enumerate() {
                for (target_index, target) in branch.targets.iter().enumerate() {
                    fields.push(ProjectEditField {
                        label: format!("{} path", branch.name),
                        input: TextInput::with_value(target.path.clone()),
                        kind: ProjectEditFieldKind::TargetPath {
                            branch_index: Some(branch_index),
                            target_index,
                        },
                    });
                }
            }
        }

        if fields.is_empty() {
            bail!("there are no editable settings for the selected project yet");
        }

        Ok(Self {
            project_index,
            project_name: project.name.clone(),
            fields,
            focus_index: 0,
        })
    }

    fn focus_next(&mut self) {
        self.focus_index = (self.focus_index + 1) % (self.fields.len() + 2);
    }

    fn focus_previous(&mut self) {
        self.focus_index = (self.focus_index + self.fields.len() + 1) % (self.fields.len() + 2);
    }

    fn is_save_focused(&self) -> bool {
        self.focus_index == self.fields.len()
    }

    fn is_cancel_focused(&self) -> bool {
        self.focus_index == self.fields.len() + 1
    }

    fn handle_text_input(&mut self, key: KeyEvent) {
        if let Some(field) = self.fields.get_mut(self.focus_index) {
            field.input.handle_key(key);
        }
    }

    fn insert_text(&mut self, text: &str) -> bool {
        if let Some(field) = self.fields.get_mut(self.focus_index) {
            field.input.insert_str(text);
            return true;
        }
        false
    }

    fn apply(&self, project: &mut ProjectConfig) -> Result<()> {
        for field in &self.fields {
            let value = field.input.value.trim();
            if value.is_empty() {
                bail!("{} cannot be empty", field.label);
            }

            match field.kind {
                ProjectEditFieldKind::RepoRoot => {
                    let repo = project.repo.as_mut().ok_or_else(|| anyhow!("project is not git-backed"))?;
                    repo.local_root = value.to_string();
                }
                ProjectEditFieldKind::RemoteUrl => {
                    let repo = project.repo.as_mut().ok_or_else(|| anyhow!("project is not git-backed"))?;
                    repo.remote_url = Some(value.to_string());
                }
                ProjectEditFieldKind::TargetPath { branch_index: None, target_index } => {
                    let target = project.targets.get_mut(target_index).ok_or_else(|| anyhow!("target index is out of range"))?;
                    target.path = value.to_string();
                }
                ProjectEditFieldKind::TargetPath { branch_index: Some(branch_index), target_index } => {
                    let branch = project.branches.get_mut(branch_index).ok_or_else(|| anyhow!("branch index is out of range"))?;
                    let target = branch.targets.get_mut(target_index).ok_or_else(|| anyhow!("target index is out of range"))?;
                    target.path = value.to_string();
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone)]
struct ProjectEditField {
    label: String,
    input: TextInput,
    kind: ProjectEditFieldKind,
}

#[derive(Clone, Copy)]
enum ProjectEditFieldKind {
    RepoRoot,
    RemoteUrl,
    TargetPath { branch_index: Option<usize>, target_index: usize },
}

#[derive(Clone)]
struct BumpDialog {
    project_name: String,
    scheme: VersionScheme,
    current_version: String,
    targets: Vec<BumpTarget>,
    action_index: usize,
}

impl BumpDialog {
    fn from_project(project: &ProjectConfig) -> Result<Self> {
        let targets = collect_bump_targets(project)?;
        if targets.is_empty() {
            bail!("selected project does not contain any configured targets");
        }

        let scheme = targets[0].scheme;
        if targets.iter().any(|target| target.scheme != scheme) {
            bail!("projects with mixed version schemes are not supported in the bump preview yet");
        }

        let current_version = targets[0].current_version.clone();
        if targets.iter().any(|target| target.current_version != current_version) {
            bail!("configured targets do not currently share the same version value");
        }

        Ok(Self {
            project_name: project.name.clone(),
            scheme,
            current_version,
            targets,
            action_index: 0,
        })
    }

    fn actions(&self) -> &'static [BumpAction] {
        self.scheme.supported_actions()
    }

    fn selected_action(&self) -> BumpAction {
        self.actions()[self.action_index]
    }

    fn rotate_action(&mut self, delta: isize) {
        let actions = self.actions();
        if actions.len() <= 1 {
            self.action_index = 0;
            return;
        }
        let len = actions.len() as isize;
        let next = (self.action_index as isize + delta).rem_euclid(len);
        self.action_index = next as usize;
    }

    fn preview_next_version(&self) -> Result<String> {
        let today = Local::now().date_naive();
        self.scheme
            .bump(&self.current_version, self.selected_action(), today)
            .map_err(anyhow::Error::msg)
    }
}

#[derive(Clone)]
struct BumpTarget {
    label: String,
    path: String,
    key_path: String,
    format: TargetFormat,
    scheme: VersionScheme,
    current_version: String,
}

#[derive(Clone)]
struct StatusMessage {
    kind: StatusKind,
    text: String,
}

impl StatusMessage {
    fn info(text: impl Into<String>) -> Self {
        Self { kind: StatusKind::Info, text: text.into() }
    }

    fn success(text: impl Into<String>) -> Self {
        Self { kind: StatusKind::Success, text: text.into() }
    }

    fn warning(text: impl Into<String>) -> Self {
        Self { kind: StatusKind::Warning, text: text.into() }
    }

    fn error(text: impl Into<String>) -> Self {
        Self { kind: StatusKind::Error, text: text.into() }
    }
}

#[derive(Clone, Copy)]
enum StatusKind {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Clone)]
struct TargetProbe {
    kind: ProbeKind,
    message: String,
    version: Option<String>,
    format: Option<TargetFormat>,
}

#[derive(Clone, Copy)]
enum ProbeKind {
    Success,
    Warning,
    Error,
}

#[derive(Clone)]
struct TextInput {
    value: String,
    cursor: usize,
}

impl TextInput {
    fn with_value(value: impl Into<String>) -> Self {
        let value = value.into();
        let cursor = value.len();
        Self { value, cursor }
    }

    fn insert(&mut self, character: char) {
        self.value.insert(self.cursor, character);
        self.cursor += character.len_utf8();
    }

    fn insert_str(&mut self, text: &str) {
        self.value.insert_str(self.cursor, text);
        self.cursor = (self.cursor + text.len()).min(self.value.len());
    }

    fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        self.cursor -= 1;
        self.value.remove(self.cursor);
    }

    fn delete(&mut self) {
        if self.cursor >= self.value.len() {
            return;
        }
        self.value.remove(self.cursor);
    }

    fn move_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    fn move_right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.value.len());
    }

    fn home(&mut self) {
        self.cursor = 0;
    }

    fn end(&mut self) {
        self.cursor = self.value.len();
    }

    fn handle_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char(character) if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT => self.insert(character),
            KeyCode::Backspace => self.backspace(),
            KeyCode::Delete => self.delete(),
            KeyCode::Left => self.move_left(),
            KeyCode::Right => self.move_right(),
            KeyCode::Home => self.home(),
            KeyCode::End => self.end(),
            _ => {}
        }
    }

    fn display_value(&self, focused: bool) -> String {
        if !focused {
            return self.value.clone();
        }

        let cursor = self.cursor.min(self.value.len());
        let (left, right) = self.value.split_at(cursor);
        if right.is_empty() {
            format!("{}|", left)
        } else {
            format!("{}|{}", left, right)
        }
    }
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
    }

    fn focus_previous(&mut self) {
        let fields = self.visible_fields();
        let index = fields.iter().position(|field| *field == self.focus).unwrap_or(0);
        self.focus = fields[(index + fields.len() - 1) % fields.len()];
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

    fn display_value_for_field(&self, field: WizardField, focused: bool) -> String {
        match field {
            WizardField::Name => self.name.display_value(focused),
            WizardField::ProjectType => format!("< {} >", self.project_type.display_name()),
            WizardField::BranchName => self.branch_name.display_value(focused),
            WizardField::VersionScheme => format!("< {} >", self.version_scheme.display_name()),
            WizardField::IntegrationMode => format!("< {} >", self.integration_mode.display_name()),
            WizardField::TargetPath => self.target_path.display_value(focused),
            WizardField::TargetKey => self.target_key.display_value(focused),
            WizardField::RepoRoot => self.repo_root.display_value(focused),
            WizardField::RemoteUrl => self.remote_url.display_value(focused),
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
                self.last_probe = None;
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
    }

    fn handle_text_input(&mut self, key: KeyEvent) {
        let Some(input) = self.active_input_mut() else {
            return;
        };
        match key.code {
            KeyCode::Char(character) if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT => {
                input.insert(character);
                self.last_probe = None;
            }
            KeyCode::Backspace => {
                input.backspace();
                self.last_probe = None;
            }
            KeyCode::Delete => {
                input.delete();
                self.last_probe = None;
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
            self.last_probe = None;
            return true;
        }
        false
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

fn probe_target(path: &str, key_path: &str, scheme: VersionScheme) -> Result<TargetProbe> {
    if path.is_empty() {
        bail!("target path is empty");
    }
    if key_path.is_empty() {
        bail!("target key is empty");
    }

    let target = read_target_value(path, key_path, TargetFormat::Auto)?;
    let format = target.format;
    let version = target.version;

    let kind = match scheme.validate(&version) {
        Ok(()) => ProbeKind::Success,
        Err(_) => ProbeKind::Warning,
    };
    let message = match scheme.validate(&version) {
        Ok(()) => format!("{} -> {} matches {}", path, key_path, scheme.display_name()),
        Err(error) => format!("{} -> {} is readable, but '{}' does not match {}: {}", path, key_path, version, scheme.display_name(), error),
    };

    Ok(TargetProbe {
        kind,
        message,
        version: Some(version),
        format: Some(format),
    })
}

fn collect_bump_targets(project: &ProjectConfig) -> Result<Vec<BumpTarget>> {
    let mut targets = Vec::new();

    if project.project_type == ProjectType::AllInOne {
        for target in &project.targets {
            let target_value = read_target_value(&target.path, &target.key_path, target.format)?;
            targets.push(BumpTarget {
                label: target.label.clone(),
                path: target.path.clone(),
                key_path: target.key_path.clone(),
                format: target_value.format,
                scheme: project.version_scheme,
                current_version: target_value.version,
            });
        }
    } else {
        for branch in &project.branches {
            let scheme = if project.unified_versioning { project.version_scheme } else { branch.version_scheme };
            for target in &branch.targets {
                let target_value = read_target_value(&target.path, &target.key_path, target.format)?;
                targets.push(BumpTarget {
                    label: format!("{} / {}", branch.name, target.label),
                    path: target.path.clone(),
                    key_path: target.key_path.clone(),
                    format: target_value.format,
                    scheme,
                    current_version: target_value.version,
                });
            }
        }
    }

    Ok(targets)
}

#[derive(Clone)]
struct TargetValue {
    version: String,
    format: TargetFormat,
}

fn read_target_value(path: &str, key_path: &str, hint: TargetFormat) -> Result<TargetValue> {
    let content = fs::read_to_string(path).with_context(|| format!("failed to read {}", path))?;
    let format = if hint == TargetFormat::Auto {
        detect_format(path, &content)?
    } else {
        hint
    };

    let version = match format {
        TargetFormat::Json => extract_json_value(&content, key_path)?,
        TargetFormat::Toml => extract_toml_value(&content, key_path)?,
        TargetFormat::Auto => unreachable!(),
    };

    Ok(TargetValue { version, format })
}

fn write_target_version(target: &BumpTarget, new_version: &str) -> Result<()> {
    let content = fs::read_to_string(&target.path).with_context(|| format!("failed to read {}", target.path))?;
    match target.format {
        TargetFormat::Json => write_json_value(&target.path, &content, &target.key_path, new_version),
        TargetFormat::Toml => write_toml_value(&target.path, &content, &target.key_path, new_version),
        TargetFormat::Auto => bail!("cannot write target with unresolved format"),
    }
}

fn detect_format(path: &str, content: &str) -> Result<TargetFormat> {
    let extension = Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase());

    match extension.as_deref() {
        Some("json") => Ok(TargetFormat::Json),
        Some("toml") => Ok(TargetFormat::Toml),
        _ => {
            if serde_json::from_str::<serde_json::Value>(content).is_ok() {
                Ok(TargetFormat::Json)
            } else if toml::from_str::<toml::Value>(content).is_ok() {
                Ok(TargetFormat::Toml)
            } else {
                Err(anyhow!("unable to detect JSON or TOML format from target file"))
            }
        }
    }
}

fn write_json_value(path: &str, content: &str, key_path: &str, new_value: &str) -> Result<()> {
    let mut value = serde_json::from_str::<serde_json::Value>(content).context("invalid JSON target")?;
    let located = locate_json_value_mut(&mut value, key_path)?;
    *located = serde_json::Value::String(new_value.to_string());
    let mut rendered = serde_json::to_string_pretty(&value).context("failed to serialize JSON target")?;
    rendered.push('\n');
    fs::write(path, rendered).with_context(|| format!("failed to write {}", path))?;
    Ok(())
}

fn write_toml_value(path: &str, content: &str, key_path: &str, new_value: &str) -> Result<()> {
    let mut document = content.parse::<DocumentMut>().context("invalid TOML target")?;
    let target_key = if locate_toml_item_mut(document.as_item_mut(), key_path).is_ok() {
        key_path.to_string()
    } else if !key_path.contains('.') {
        if let Some(package) = document.as_item().get("package") {
            if package.get(key_path).is_some() {
                format!("package.{}", key_path)
            } else {
                key_path.to_string()
            }
        } else {
            key_path.to_string()
        }
    } else {
        key_path.to_string()
    };

    let item = locate_toml_item_mut(document.as_item_mut(), &target_key)?;
    if item.is_value() {
        *item = Item::Value(Value::from(new_value.to_string()));
    } else {
        *item = value(new_value);
    }
    fs::write(path, document.to_string()).with_context(|| format!("failed to write {}", path))?;
    Ok(())
}

fn extract_json_value(content: &str, key_path: &str) -> Result<String> {
    let value = serde_json::from_str::<serde_json::Value>(content).context("invalid JSON target")?;
    let located = key_path.split('.').try_fold(&value, |current, segment| {
        current.get(segment).ok_or_else(|| anyhow!("missing key '{}'", key_path))
    })?;
    located
        .as_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow!("key '{}' is present, but its value is not a string", key_path))
}

fn extract_toml_value(content: &str, key_path: &str) -> Result<String> {
    let value = toml::from_str::<toml::Value>(content).context("invalid TOML target")?;
    let key_path = expand_toml_key_path(&value, key_path);
    let located = locate_toml_value(&value, &key_path)?;
    located
        .as_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow!("key '{}' is present, but its value is not a string", key_path))
}

fn expand_toml_key_path<'a>(value: &'a toml::Value, key_path: &'a str) -> std::borrow::Cow<'a, str> {
    if key_path.contains('.') {
        return std::borrow::Cow::Borrowed(key_path);
    }

    if value.get(key_path).is_some() {
        return std::borrow::Cow::Borrowed(key_path);
    }

    if let Some(package) = value.get("package") {
        if package.get(key_path).is_some() {
            return std::borrow::Cow::Owned(format!("package.{}", key_path));
        }
    }

    std::borrow::Cow::Borrowed(key_path)
}

fn locate_toml_value<'a>(value: &'a toml::Value, key_path: &str) -> Result<&'a toml::Value> {
    let mut current = value;
    for segment in key_path.split('.') {
        current = current
            .get(segment)
            .ok_or_else(|| anyhow!("missing key '{}'", key_path))?;
    }
    Ok(current)
}

fn locate_json_value_mut<'a>(value: &'a mut serde_json::Value, key_path: &str) -> Result<&'a mut serde_json::Value> {
    let mut current = value;
    for segment in key_path.split('.') {
        current = current
            .get_mut(segment)
            .ok_or_else(|| anyhow!("missing key '{}'", key_path))?;
    }
    Ok(current)
}

fn locate_toml_item_mut<'a>(item: &'a mut Item, key_path: &str) -> Result<&'a mut Item> {
    let mut current = item;
    for segment in key_path.split('.') {
        current = current
            .get_mut(segment)
            .ok_or_else(|| anyhow!("missing key '{}'", key_path))?;
    }
    Ok(current)
}

fn centered_rect(area: Rect, width_percent: u16, height_percent: u16) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - height_percent) / 2),
            Constraint::Percentage(height_percent),
            Constraint::Percentage((100 - height_percent) / 2),
        ])
        .split(area);
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - width_percent) / 2),
            Constraint::Percentage(width_percent),
            Constraint::Percentage((100 - width_percent) / 2),
        ])
        .split(vertical[1]);
    horizontal[1]
}

fn sanitize_pasted_text(text: &str) -> String {
    text.chars().filter(|character| *character != '\r' && *character != '\n').collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_cargo_toml_version_without_package_prefix() {
        let content = r#"
[package]
name = "comfy-version-bumper"
version = "0.1.0"
edition = "2024"
"#;
        let resolved = extract_toml_value(content, "version").expect("should resolve package.version");
        assert_eq!(resolved, "0.1.0");
    }
}

fn project_repo_root(project: &ProjectConfig) -> Result<String> {
    let repo = project
        .repo
        .as_ref()
        .ok_or_else(|| anyhow!("this project is local-only and has no git repository configured"))?;
    Ok(repo.local_root.clone())
}

fn suggested_tag_name(project: &ProjectConfig) -> String {
    if let Ok(targets) = collect_bump_targets(project) {
        if let Some(first) = targets.first() {
            if targets.iter().all(|target| target.current_version == first.current_version) {
                return format!("v{}", first.current_version);
            }
        }
    }

    let fallback = project
        .name
        .chars()
        .map(|character| if character.is_ascii_alphanumeric() { character.to_ascii_lowercase() } else { '-' })
        .collect::<String>();
    fallback.trim_matches('-').to_string()
}

fn ensure_git_repo(repo_root: &str) -> Result<()> {
    let output = run_git_checked(repo_root, &["rev-parse", "--is-inside-work-tree"])?;
    if output.trim() == "true" {
        Ok(())
    } else {
        bail!("{} is not a git working tree", repo_root)
    }
}

fn ensure_local_tag(repo_root: &str, tag_name: &str) -> Result<bool> {
    let existing = run_git_checked(repo_root, &["tag", "--list", tag_name])?;
    if existing.lines().any(|line| line.trim() == tag_name) {
        Ok(false)
    } else {
        run_git_checked(repo_root, &["tag", tag_name])?;
        Ok(true)
    }
}

fn ensure_gh_available() -> Result<()> {
    let output = Command::new("gh")
        .arg("--version")
        .output()
        .context("failed to invoke gh; install GitHub CLI to create releases")?;
    if output.status.success() {
        Ok(())
    } else {
        bail!("gh is not available or not functioning; install GitHub CLI to create releases")
    }
}

struct GitOutput {
    success: bool,
    stdout: String,
    stderr: String,
}

fn run_git(repo_root: &str, args: &[&str]) -> Result<GitOutput> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(args)
        .output()
        .with_context(|| format!("failed to run git in {}", repo_root))?;

    Ok(GitOutput {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

fn run_git_checked(repo_root: &str, args: &[&str]) -> Result<String> {
    let output = run_git(repo_root, args)?;
    if output.success {
        Ok(output.stdout)
    } else {
        let details = output.stderr.trim();
        if details.is_empty() {
            bail!("git {:?} failed in {}", args, repo_root)
        } else {
            bail!("git {:?} failed in {}: {}", args, repo_root, details)
        }
    }
}

fn run_gh_checked(repo_root: &str, args: &[&str]) -> Result<String> {
    let output = Command::new("gh")
        .current_dir(repo_root)
        .args(args)
        .output()
        .with_context(|| format!("failed to run gh in {}", repo_root))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        let details = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if details.is_empty() {
            bail!("gh {:?} failed in {}", args, repo_root)
        } else {
            bail!("gh {:?} failed in {}: {}", args, repo_root, details)
        }
    }
}

fn split_output_lines(output: &str) -> Vec<String> {
    output
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}