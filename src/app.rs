// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyVersionBumper License v1.2
//
// For details, see the LICENSE file in the repository root.

use std::{
    collections::HashSet,
    io,
    path::{Path, PathBuf},
    process::Command,
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
use ratatui_explorer::{FileExplorer, FileExplorerBuilder, Input as ExplorerInput};
use tui_tabs::TabNav;
use tui_textarea::{Input as TextAreaInput, Key as TextAreaKey, TextArea as TuiTextArea};

use crate::{
    branding::{PixelLogo, choose_header_content},
    config::{
        AppConfig, BranchConfig, BranchScopeKind, ConfigStore, IntegrationMode, ProjectConfig,
        ProjectType, RepoConfig, TargetFormat, TargetSpec,
    },
    dialogs::{BumpDialog, RecentChangesDialog, RecentChangesTab, TagDialog, TagAction, TextInput},
    git::{
        collect_all_branch_git_scope_contexts, ensure_gh_available, ensure_local_tag,
        load_scope_activity_summary, run_gh_checked, run_git, run_git_checked, split_output_lines,
    },
    overview_pg::{OverviewTab, overview_tab_rects, render_overview_tabs},
    targets::{BumpScope, BumpTarget, ProbeKind, TargetProbe, collect_bump_scopes, probe_target, write_target_version},
    tiles::{OverviewTileData, render_overview_tile, tile_height, TILE_WIDTH},
    ui::{center_vertically, centered_rect},
    versioning::VersionScheme,
};

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
const SUPPORT_EMAIL: &str = " dev@comfyhome.io ";
const FORM_LABEL_WIDTH: u16 = 18;
const BROWSE_BUTTON_WIDTH: u16 = 12;
const BUTTON_ROW_HEIGHT: u16 = 3;
const BUTTON_GAP_HEIGHT: u16 = 3;
const GIT_BRANCH_COLORS: [Color; 6] = [
    Color::Green,
    Color::Cyan,
    Color::Yellow,
    Color::Magenta,
    Color::Blue,
    Color::Red,
];

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
    overview_tab: OverviewTab,
    overview_show_recent_tab: bool,
    overview_recent_changes: Option<RecentChangesDialog>,
    overview_recent_project: Option<usize>,
    overview_recent_error: Option<String>,
    overview_tile_project: Option<usize>,
    overview_scope_order: Vec<usize>,
    overview_pending_versions: Vec<String>,
    overview_tile_scroll: usize,
    overview_tile_viewport: Option<Rect>,
    overview_recent_viewport: Option<Rect>,
    overview_tile_rects: Vec<(Rect, usize)>,
    overview_drag_scope: Option<usize>,
    wizard: ProjectWizard,
    bump_dialog: Option<BumpDialog>,
    overview_bump_workflow_dialog: Option<OverviewBumpWorkflowDialog>,
    overview_bump_warning_dialog: Option<OverviewBumpWarningDialog>,
    recent_changes_dialog: Option<RecentChangesDialog>,
    tag_dialog: Option<TagDialog>,
    tag_annotation_dialog: Option<TagAnnotationDialog>,
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
            overview_tab: OverviewTab::Overview,
            overview_show_recent_tab: false,
            overview_recent_changes: None,
            overview_recent_project: None,
            overview_recent_error: None,
            overview_tile_project: None,
            overview_scope_order: Vec::new(),
            overview_pending_versions: Vec::new(),
            overview_tile_scroll: 0,
            overview_tile_viewport: None,
            overview_recent_viewport: None,
            overview_tile_rects: Vec::new(),
            overview_drag_scope: None,
            wizard: ProjectWizard::default(),
            bump_dialog: None,
            overview_bump_workflow_dialog: None,
            overview_bump_warning_dialog: None,
            recent_changes_dialog: None,
            tag_dialog: None,
            tag_annotation_dialog: None,
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
        self.overview_tile_viewport = None;
        self.overview_recent_viewport = None;
        self.overview_tile_rects.clear();

        let header_height = header_height_for_viewport(frame.area().height);
        let root = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(header_height),
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
        if self.overview_bump_workflow_dialog.is_some() {
            self.render_overview_bump_workflow_dialog(frame, frame.area());
        }
        if self.overview_bump_warning_dialog.is_some() {
            self.render_overview_bump_warning_dialog(frame, frame.area());
        }
        if self.recent_changes_dialog.is_some() {
            self.render_recent_changes_dialog(frame, frame.area());
        }
        if self.tag_dialog.is_some() {
            self.render_tag_dialog(frame, frame.area());
        }
        if self.tag_annotation_dialog.is_some() {
            self.render_tag_annotation_dialog(frame, frame.area());
        }
        if self.project_edit_dialog.is_some() {
            self.render_project_edit_dialog(frame, frame.area());
        }
        if self.browser_dialog.is_some() {
            self.render_browser_dialog(frame, frame.area());
        }

        self.render_footer(frame, root[3]);
        self.transient_toaster.set_area(frame.area());
        let transient_area = self.transient_toaster.toast_area();
        if self.transient_toaster.has_toast() {
            self.sticky_toaster.set_area_avoiding(frame.area(), &[transient_area]);
        } else {
            self.sticky_toaster.set_area(frame.area());
        }
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
        if area.width == 0 || area.height == 0 {
            return;
        }

        let labels = self.main_tab_labels();
        let widths = labels
            .iter()
            .map(|label| (label.chars().count() as u16 + 6).max(14))
            .collect::<Vec<_>>();

        let active_index = self.current_main_tab_index();
        let border_rect = Rect {
            x: area.x,
            y: area.y + area.height.saturating_sub(1),
            width: area.width,
            height: 1,
        };
        frame.render_widget(
            Paragraph::new("─".repeat(area.width as usize)).style(Style::default().fg(Color::DarkGray)),
            border_rect,
        );

        let left_rect = Rect { x: area.x, y: area.y, width: widths[0].min(area.width), height: area.height };
        self.render_main_nav_tab(frame, left_rect, &labels[0], active_index == 0);
        self.hit_targets.push(HitTarget::new(left_rect, HitAction::Switch(main_screen_from_index(0))));

        let right_total_width = widths.iter().skip(1).copied().sum::<u16>();
        let mut current_x = area.x + area.width.saturating_sub(right_total_width);
        for index in 1..labels.len() {
            let width = widths[index].min(area.x + area.width - current_x);
            if width == 0 {
                continue;
            }
            let rect = Rect {
                x: current_x,
                y: area.y,
                width,
                height: area.height,
            };
            self.render_main_nav_tab(frame, rect, &labels[index], active_index == index);
            self.hit_targets.push(HitTarget::new(rect, HitAction::Switch(main_screen_from_index(index))));
            current_x = current_x.saturating_add(widths[index]);
        }
    }

    fn render_main_nav_tab(&self, frame: &mut Frame, area: Rect, label: &str, selected: bool) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let border_style = if selected {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let label_style = if selected {
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        let block = Block::default()
            .borders(Borders::TOP | Borders::LEFT | Borders::RIGHT)
            .border_style(border_style);
        let inner = block.inner(area);
        frame.render_widget(block, area);
        frame.render_widget(Paragraph::new(label).alignment(Alignment::Center).style(label_style), inner);

        if selected && area.width > 2 {
            let clear_rect = Rect {
                x: area.x + 1,
                y: area.y + area.height.saturating_sub(1),
                width: area.width.saturating_sub(2),
                height: 1,
            };
            frame.render_widget(Paragraph::new(" ".repeat(clear_rect.width as usize)), clear_rect);
        }
    }

    fn scope_repo_roots(&self, project: &ProjectConfig, scope_count: usize) -> Vec<Option<String>> {
        if project.integration_mode.requires_repo() {
            match collect_all_branch_git_scope_contexts(project) {
                Ok(contexts) => (0..scope_count)
                    .map(|index| contexts.get(index).map(|context| context.repo_root.clone()))
                    .collect(),
                Err(_) => vec![None; scope_count],
            }
        } else {
            vec![None; scope_count]
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
                Line::from("Branched projects can now manage multiple scopes in the wizard."),
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

        let right_sections = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(8)])
            .split(chunks[1]);
        self.overview_show_recent_tab = self.should_use_recent_changes_tab(right_sections[1]);
        if !self.overview_show_recent_tab && self.overview_tab == OverviewTab::RecentChanges {
            self.overview_tab = OverviewTab::Overview;
        }
        render_overview_tabs(frame, right_sections[0], self.overview_tab, self.overview_show_recent_tab);
        for (tab, rect) in overview_tab_rects(right_sections[0], self.overview_show_recent_tab) {
            self.hit_targets.push(HitTarget::new(rect, HitAction::SelectOverviewTab(tab)));
        }

        let overview_body = Block::default().borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM);
        let overview_inner = overview_body.inner(right_sections[1]);
        frame.render_widget(overview_body, right_sections[1]);

        if self.overview_tab == OverviewTab::ProjectDetail {
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
            frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), overview_inner);
        } else {
            self.render_dashboard_overview(frame, overview_inner);
        }
    }

    fn render_dashboard_overview(&mut self, frame: &mut Frame, area: Rect) {
        let Some(project) = self.config.projects.get(self.selected_project).cloned() else {
            frame.render_widget(
                Paragraph::new(vec![
                    Line::from("Overview".bold()),
                    Line::from("Select or create a project to populate the overview page."),
                ])
                .wrap(Wrap { trim: false }),
                area,
            );
            return;
        };

        self.ensure_dashboard_recent_changes();

        let scopes = match collect_bump_scopes(&project) {
            Ok(scopes) => scopes,
            Err(error) => {
                frame.render_widget(
                    Paragraph::new(vec![
                        Line::from("Overview".bold()),
                        Line::from(error.to_string()).style(Style::default().fg(Color::Red)),
                    ])
                    .wrap(Wrap { trim: false }),
                    area,
                );
                return;
            }
        };
        self.ensure_dashboard_tile_state(&scopes);

        let tile_columns = dashboard_tile_columns(area.width).max(1);
        let tile_rows = self.overview_scope_order.len().max(1).div_ceil(tile_columns);
        let max_tile_height = scopes
            .iter()
            .map(|scope| tile_height(scope.scheme))
            .max()
            .unwrap_or(7);

        if self.overview_show_recent_tab && self.overview_tab == OverviewTab::RecentChanges {
            self.render_overview_recent_changes(frame, area);
            return;
        }

        if self.overview_show_recent_tab {
            self.render_dashboard_tiles(frame, area, &project, &scopes);
            return;
        }

        let row_height = max_tile_height.saturating_add(1);
        let desired_tile_height = tile_rows as u16 * row_height - 1;
        let tile_height_budget = area.height.saturating_sub(9).max(max_tile_height.min(area.height));
        let tile_section_height = desired_tile_height.min(tile_height_budget).max(max_tile_height.min(area.height));
        let sections = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(tile_section_height), Constraint::Length(1), Constraint::Min(8)])
            .split(area);

        self.render_dashboard_tiles(frame, sections[0], &project, &scopes);

        self.render_overview_recent_changes(frame, sections[2]);
    }

    fn render_overview_recent_changes(&mut self, frame: &mut Frame, area: Rect) {
        let recent_block = Block::default().borders(Borders::ALL).title(" Recent Changes ");
        let recent_inner = recent_block.inner(area);
        self.overview_recent_viewport = Some(recent_inner);
        frame.render_widget(recent_block, area);

        let recent_lines = if let Some(dialog) = &self.overview_recent_changes {
            let mut lines = vec![
                Line::from(format!(
                    "Scope: {} ({})",
                    dialog.active_scope().display_name,
                    dialog.active_scope().scope_kind.map(|kind| kind.display_name()).unwrap_or("Project")
                ))
                .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Line::from(format!("View: {}", dialog.current_range().label)).style(Style::default().fg(Color::Gray)),
                Line::raw(""),
            ];
            if dialog.current_range().lines.is_empty() {
                lines.push(Line::from("No recent changes to display."));
            } else {
                let graph_base_column = git_graph_base_column(&dialog.current_range().lines);
                lines.extend(
                    dialog
                        .current_range()
                        .lines
                        .iter()
                        .map(|line| colorize_git_log_line(line, graph_base_column)),
                );
            }
            lines
        } else if let Some(error) = &self.overview_recent_error {
            vec![
                Line::from("Recent changes are unavailable.").style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Line::from(error.clone()),
            ]
        } else {
            vec![Line::from("Recent changes are not available for local-only projects.")]
        };
        let scroll = self.overview_recent_changes.as_ref().map(|dialog| dialog.scroll).unwrap_or(0);
        frame.render_widget(
            Paragraph::new(recent_lines)
                .scroll((scroll, 0))
                .wrap(Wrap { trim: false }),
            recent_inner,
        );
    }

    fn should_use_recent_changes_tab(&self, area: Rect) -> bool {
        let Some(project) = self.config.projects.get(self.selected_project) else {
            return false;
        };
        let Ok(scopes) = collect_bump_scopes(project) else {
            return false;
        };
        let max_tile_height = scopes
            .iter()
            .map(|scope| tile_height(scope.scheme))
            .max()
            .unwrap_or(7);
        should_use_recent_changes_tab(area.height, max_tile_height)
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
            Line::from(format!(
                "Mode: {}",
                if dialog.unified_versioning {
                    "Project-wide synchronized"
                } else {
                    "Selected scope only"
                }
            )),
            Line::from(format!("Scheme: {}", dialog.active_scheme().display_name())),
            Line::from(format!("Current version: {}", dialog.current_version_label())),
            Line::from(format!("Action: < {} >", dialog.selected_action().display_name())).style(Style::default().fg(Color::Yellow)),
        ];
        if dialog.can_select_scope() {
            let scope = dialog.active_scope();
            summary.push(Line::from(format!(
                "Selected scope: < {} ({}) >",
                scope.display_name,
                scope.scope_kind.map(|kind| kind.display_name()).unwrap_or("Project")
            )));
        }
        match next_version {
            Ok(next_version) => summary.push(Line::from(format!("Next version: {}", next_version)).style(Style::default().fg(Color::Green).add_modifier(Modifier::BOLD))),
            Err(error) => summary.push(Line::from(format!("Next version: {}", error)).style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))),
        }
        summary.push(Line::from(if dialog.can_select_scope() {
            "Left/Right changes the action. Up/Down changes scope. Enter applies to the selected scope."
        } else {
            "Left/Right changes the action. Enter applies it to every listed target."
        }));
        frame.render_widget(Paragraph::new(summary).wrap(Wrap { trim: false }), sections[0]);

        let target_lines = dialog
            .scopes
            .iter()
            .enumerate()
            .flat_map(|(index, scope)| {
                let marker = if dialog.can_select_scope() && index == dialog.selected_scope {
                    ">"
                } else {
                    "-"
                };
                let header_style = if scope.has_mismatch() {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else if dialog.can_select_scope() && index == dialog.selected_scope {
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().add_modifier(Modifier::BOLD)
                };
                std::iter::once(
                    Line::from(format!(
                        "{} {} ({}) -> {}",
                        marker,
                        scope.display_name,
                        scope.scope_kind.map(|kind| kind.display_name()).unwrap_or("Project"),
                        scope.version_label()
                    ))
                    .style(header_style),
                )
                .chain(scope.targets.iter().map(|target| {
                    Line::from(format!(
                        "  {}: {} -> {} [{}]",
                        target.label,
                        target.path,
                        target.key_path,
                        target.format.display_name()
                    ))
                }))
            })
            .collect::<Vec<_>>();
        let target_block = Block::default().borders(Borders::ALL).title(" Targets ");
        let target_inner = target_block.inner(sections[1]);
        frame.render_widget(target_block, sections[1]);
        frame.render_widget(Paragraph::new(target_lines).wrap(Wrap { trim: false }), target_inner);

        let mut buttons = Vec::new();
        if dialog.can_select_scope() {
            buttons.push(DialogButton::new(
                format!("Scope: < {} >", dialog.active_scope().display_name),
                false,
                HitAction::CycleBumpScope(1),
                Style::default().fg(Color::Black).bg(Color::Rgb(140, 220, 180)),
            ));
        }
        buttons.push(DialogButton::new(
            format!("< {} >", dialog.selected_action().display_name()),
            false,
            HitAction::CycleBumpAction(1),
            Style::default().fg(Color::Black).bg(Color::Yellow),
        ));
        buttons.push(DialogButton::new("Apply", false, HitAction::ApplyBump, Style::default().fg(Color::Black).bg(Color::Green)));
        buttons.push(DialogButton::new("Cancel", false, HitAction::CancelBump, Style::default().fg(Color::White).bg(Color::Red)));
        self.render_button_row(frame, sections[2], &buttons);
    }

    fn render_overview_bump_workflow_dialog(&mut self, frame: &mut Frame, area: Rect) {
        let Some(dialog) = &self.overview_bump_workflow_dialog else {
            return;
        };

        let popup = centered_rect(area, 78, 46);
        frame.render_widget(Clear, popup);
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Bump Action ")
            .border_style(Style::default().fg(Color::Cyan));
        let inner = block.inner(popup);
        frame.render_widget(block, popup);

        let sections = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(5), Constraint::Min(9), Constraint::Length(BUTTON_ROW_HEIGHT)])
            .split(inner);

        let header = vec![
            Line::from(format!("Project: {}", dialog.project_name)).bold(),
            Line::from(format!("Scope: {}", dialog.scope_label)),
            Line::from(format!("Next version: {}", dialog.next_version)).style(Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Line::from("What would you like to do?"),
        ];
        frame.render_widget(Paragraph::new(header).wrap(Wrap { trim: false }), sections[0]);

        let option_rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Length(3); dialog.options.len()])
            .split(sections[1]);

        for (index, (option, row)) in dialog.options.iter().zip(option_rows.iter()).enumerate() {
            let selected = index == dialog.selected;
            let row_block = Block::default().borders(Borders::ALL).border_style(if selected {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default().fg(Color::DarkGray)
            });
            let row_inner = row_block.inner(*row);
            frame.render_widget(row_block, *row);
            let lines = vec![
                Line::from(format!("{}. {}", index + 1, option.display_name())).style(if selected {
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().add_modifier(Modifier::BOLD)
                }),
                Line::from(option.description()),
            ];
            frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), row_inner);
            self.hit_targets.push(HitTarget::new(*row, HitAction::SelectOverviewBumpWorkflow(index)));
        }

        self.render_button_row(
            frame,
            sections[2],
            &[
                DialogButton::new("Run", false, HitAction::ConfirmOverviewBumpWorkflow, Style::default().fg(Color::Black).bg(Color::Green)),
                DialogButton::new("Cancel", false, HitAction::CancelOverviewBumpWorkflow, Style::default().fg(Color::White).bg(Color::Red)),
            ],
        );
    }

    fn render_overview_bump_warning_dialog(&mut self, frame: &mut Frame, area: Rect) {
        let Some(dialog) = &self.overview_bump_warning_dialog else {
            return;
        };

        let popup = centered_rect(area, 82, 54);
        frame.render_widget(Clear, popup);
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Staged Files Warning ")
            .border_style(Style::default().fg(Color::Yellow));
        let inner = block.inner(popup);
        frame.render_widget(block, popup);

        let sections = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(4), Constraint::Min(10), Constraint::Length(BUTTON_ROW_HEIGHT)])
            .split(inner);

        let header = vec![
            Line::from("Previously staged files will be included in the bump commit.")
                .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Line::from("Choose whether to continue, unstage the unrelated files, or cancel this bump."),
            Line::from(format!("Action: {}", dialog.workflow.display_name())),
        ];
        frame.render_widget(Paragraph::new(header).wrap(Wrap { trim: false }), sections[0]);

        let mut lines = Vec::new();
        for repo in &dialog.repos {
            lines.push(Line::from(format!("Repo: {}", repo.repo_root)).style(Style::default().add_modifier(Modifier::BOLD)));
            for path in &repo.extra_paths {
                lines.push(Line::from(format!("  - {}", path)));
            }
            lines.push(Line::raw(""));
        }
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), sections[1]);

        self.render_button_row(
            frame,
            sections[2],
            &[
                DialogButton::new(
                    "Continue",
                    dialog.selected == 0,
                    HitAction::SelectOverviewBumpWarningChoice(0),
                    Style::default().fg(Color::Black).bg(Color::Yellow),
                ),
                DialogButton::new(
                    "Unstage Extras",
                    dialog.selected == 1,
                    HitAction::SelectOverviewBumpWarningChoice(1),
                    Style::default().fg(Color::Black).bg(Color::Rgb(140, 220, 180)),
                ),
                DialogButton::new(
                    "Cancel",
                    dialog.selected == 2,
                    HitAction::SelectOverviewBumpWarningChoice(2),
                    Style::default().fg(Color::White).bg(Color::Red),
                ),
            ],
        );
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
            Line::from(format!(
                "Scope: {} ({})",
                dialog.active_scope().display_name,
                dialog.active_scope().scope_kind.map(|kind| kind.display_name()).unwrap_or("Project")
            )),
            Line::from(format!("Repo: {}", dialog.active_scope().repo_root)),
            Line::from(format!("View: {}", dialog.current_range().label)),
            Line::from(if dialog.can_select_scope() {
                "Tab switches view. Left/Right changes scope only on Recent. In History, Left/Right browses tag windows. [ and ] still change scope."
            } else {
                "Tab switches view. Left/Right moves history when History is active."
            }),
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
        let graph_base_column = git_graph_base_column(&dialog.current_range().lines);
        let body = if dialog.current_range().lines.is_empty() {
            vec![Line::from(if dialog.active_tab == RecentChangesTab::History {
                "No history range is available yet."
            } else {
                "No recent changes to display."
            })]
        } else {
            dialog
                .current_range()
                .lines
                .iter()
                .map(|line| colorize_git_log_line(line, graph_base_column))
                .collect::<Vec<_>>()
        };
        frame.render_widget(
            Paragraph::new(body)
                .wrap(Wrap { trim: false })
                .scroll((dialog.scroll, 0)),
            body_inner,
        );

        let mut buttons = Vec::new();
        if dialog.can_select_scope() {
            buttons.push(DialogButton::new(
                format!("Scope: < {} >", dialog.active_scope().display_name),
                false,
                HitAction::CycleRecentChangesScope(1),
                Style::default().fg(Color::Black).bg(Color::Rgb(140, 220, 180)),
            ));
        }
        buttons.push(DialogButton::new("Scroll", false, HitAction::ScrollRecentChanges(3), Style::default().fg(Color::Black).bg(Color::Yellow)));
        buttons.push(DialogButton::new("Create Tag", false, HitAction::OpenTagDialog, Style::default().fg(Color::Black).bg(Color::Green)));
        buttons.push(DialogButton::new("Close", false, HitAction::CloseRecentChanges, Style::default().fg(Color::White).bg(Color::Red)));
        self.render_button_row(frame, sections[3], &buttons);
    }

    fn render_tag_dialog(&mut self, frame: &mut Frame, area: Rect) {
        let Some(dialog) = &self.tag_dialog else {
            return;
        };

        let popup = centered_rect(area, 74, 42);
        frame.render_widget(Clear, popup);
        let block = Block::default().borders(Borders::ALL).title(" Create Tag ").border_style(Style::default().fg(Color::Cyan));
        let inner = block.inner(popup);
        frame.render_widget(block, popup);

        let sections = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(4), Constraint::Length(3), Constraint::Min(8), Constraint::Length(BUTTON_ROW_HEIGHT)])
            .split(inner);

        let header = vec![
            Line::from(format!("Project: {}", dialog.project_name)).bold(),
            Line::from(format!(
                "Scope: {} ({})",
                dialog.active_scope().display_name,
                dialog.active_scope().scope_kind.map(|kind| kind.display_name()).unwrap_or("Project")
            )),
            Line::from(format!("Repo: {}", dialog.active_scope().repo_root)),
            Line::from(format!("Action: < {} >", dialog.selected_action().display_name())).style(Style::default().fg(Color::Yellow)),
            Line::from(if dialog.can_select_scope() {
                "Edit the tag name, add an optional annotation, then run the selected action. [ and ] change scope."
            } else {
                "Edit the tag name, add an optional annotation, then run the selected action."
            }),
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
            Line::from(format!("Suggested tag: {}", dialog.active_scope().suggested_tag_name)),
            Line::from(if dialog.annotation.trim().is_empty() {
                "Annotation: none"
            } else {
                "Annotation: attached"
            }),
            Line::from(match dialog.selected_action() {
                TagAction::CreateLocal => "Creates a local tag only.",
                TagAction::CreateAndPush => "Creates the local tag if needed, then pushes it.",
                TagAction::CreatePushAndRelease => "Creates the tag, pushes it, then runs `gh release create --generate-notes`.",
            }),
        ];
        frame.render_widget(Paragraph::new(notes).wrap(Wrap { trim: false }), sections[2]);

        let mut buttons = Vec::new();
        if dialog.can_select_scope() {
            buttons.push(DialogButton::new(
                format!("Scope: < {} >", dialog.active_scope().display_name),
                false,
                HitAction::CycleTagScope(1),
                Style::default().fg(Color::Black).bg(Color::Rgb(140, 220, 180)),
            ));
        }
        buttons.push(DialogButton::new(
            format!("< {} >", dialog.selected_action().display_name()),
            false,
            HitAction::CycleTagAction(1),
            Style::default().fg(Color::Black).bg(Color::Yellow),
        ));
        buttons.push(DialogButton::new(
            if dialog.annotation.trim().is_empty() {
                "Annotation"
            } else {
                "Annotation Added"
            },
            false,
            HitAction::OpenTagAnnotation,
            Style::default().fg(Color::Black).bg(Color::Rgb(140, 220, 180)),
        ));
        buttons.push(DialogButton::new("Run", false, HitAction::CreateTag, Style::default().fg(Color::Black).bg(Color::Green)));
        buttons.push(DialogButton::new("Cancel", false, HitAction::CancelTagDialog, Style::default().fg(Color::White).bg(Color::Red)));
        self.render_button_row(frame, sections[3], &buttons);
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
        let Some(_) = &self.project_edit_dialog else {
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

        let (project_name, field_rows_data, show_above, show_below, save_focused, remove_focused, cancel_focused) = {
            let dialog = self.project_edit_dialog.as_mut().expect("dialog checked above");
            let (fields, row_height, top, bottom) = dialog.refresh_body_window(sections[1].height);
            let constraints = vec![Constraint::Length(row_height); fields.len()];
            let field_rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints(constraints)
                .split(sections[1]);
            let rows = fields
                .iter()
                .zip(field_rows.iter())
                .map(|(field, row)| {
                    let focused = *field == dialog.focus;
                    let (label, action) = dialog.render_field(*field);
                    let side_button = project_edit_form_row_button(*field);
                    let value = dialog.display_value_for_field(*field, focused, visible_field_width(row.width, side_button.is_some()));
                    (*row, label, action, side_button, focused, value)
                })
                .collect::<Vec<_>>();
            (
                dialog.project_name.clone(),
                rows,
                top,
                bottom,
                dialog.focus == ProjectEditFocus::Save,
                dialog.focus == ProjectEditFocus::Remove,
                dialog.focus == ProjectEditFocus::Cancel,
            )
        };

        let header = vec![
            Line::from(format!("Project: {}", project_name)).bold(),
            Line::from("Edit the same core fields as New Project, then press F2 or Save."),
            Line::from("Tab/Shift+Tab moves between fields. Left/Right changes enums. Enter applies scope action rows. Ctrl+O browses. PgUp/PgDn or wheel scrolls. Del removes the project."),
        ];
        frame.render_widget(Paragraph::new(header).wrap(Wrap { trim: false }), sections[0]);

        for (row, label, action, side_button, focused, value) in field_rows_data {
            let button_rect = self.render_form_row(frame, row, label, value, focused, action.clone(), side_button.clone());
            self.hit_targets.push(HitTarget::new(row, action));
            if let (Some(rect), Some(button)) = (button_rect, side_button) {
                self.hit_targets.push(HitTarget::new(rect, button.action));
            }
        }
        render_vertical_overflow_indicators(frame, sections[1], show_above, show_below);

        self.render_button_row(
            frame,
            sections[3],
            &[
                DialogButton::new("Save", save_focused, HitAction::SaveProjectEdit, Style::default().fg(Color::Black).bg(Color::Green)),
                DialogButton::new("Remove", remove_focused, HitAction::RemoveProject, Style::default().fg(Color::White).bg(Color::Red)),
                DialogButton::new("Cancel", cancel_focused, HitAction::CancelProjectEdit, Style::default().fg(Color::Black).bg(Color::Rgb(230, 190, 90))),
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

        let (fields, row_height, show_above, show_below) = self.wizard.refresh_body_window(left_sections[0].height);
        let constraints = vec![Constraint::Length(row_height); fields.len()];

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(left_sections[0]);

        for (field, row) in fields.iter().zip(rows.iter()) {
            let focused = *field == self.wizard.focus;
            let (label, action) = self.wizard.render_field(*field);
            let side_button = wizard_form_row_button(*field);
            let value = self.wizard.display_value_for_field(*field, focused, visible_field_width(row.width, side_button.is_some()));
            let button_rect = self.render_form_row(frame, *row, label, value, focused, action.clone(), side_button.clone());
            self.hit_targets.push(HitTarget::new(*row, action));
            if let (Some(rect), Some(button)) = (button_rect, side_button) {
                self.hit_targets.push(HitTarget::new(rect, button.action));
            }
        }
        render_vertical_overflow_indicators(frame, left_sections[0], show_above, show_below);

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
            Line::from(if self.wizard.project_type == ProjectType::Branched {
                format!(
                    "Unified versioning: {}",
                    if self.wizard.unified_versioning { "on" } else { "off" }
                )
            } else {
                "Unified versioning: always on for all-in-one projects".to_string()
            }),
            Line::from(format!("Example: {}", self.wizard.version_scheme.example())),
            Line::from(format!("Rule: {}", self.wizard.version_scheme.description())),
            Line::raw(""),
            Line::from(if self.wizard.project_type == ProjectType::Branched {
                format!("Scopes: {} configured. Left/Right changes the selected scope.", self.wizard.scopes.len())
            } else {
                "All-in-one projects manage one target directly.".to_string()
            }),
            Line::from("Use the scope action rows to add, remove, or reorder branched entries."),
            Line::raw(""),
        ];

        let active_probe = if self.wizard.project_type == ProjectType::Branched {
            self.wizard.current_scope().and_then(|scope| scope.last_probe.as_ref())
        } else {
            self.wizard.last_probe.as_ref()
        };

        if self.wizard.project_type == ProjectType::Branched {
            let validated = self
                .wizard
                .scopes
                .iter()
                .filter(|scope| matches!(scope.last_probe.as_ref().map(|probe| probe.kind), Some(ProbeKind::Success)))
                .count();
            lines.push(Line::from(format!("Validated scopes: {}/{}", validated, self.wizard.scopes.len())));
        }

        if let Some(probe) = active_probe {
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
            lines.push(Line::from("Use F5 or Read to inspect the selected target file."));
            lines.push(Line::from("Save requires every branched scope to be read successfully."));
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
        side_button: Option<FormRowButton>,
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

        let row = if side_button.is_some() {
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

        let field_index = 1;
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

        if let Some(button) = side_button {
            let button_area = center_vertically(row[3], area.height.min(3));
            frame.render_widget(
                Paragraph::new(button.label)
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
            constraints.push(Constraint::Length((button.label.chars().count() as u16 + 6).max(14)));
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
                Paragraph::new(button.label.as_str())
                    .alignment(Alignment::Center)
                    .style(style)
                    .block(block),
                rect,
            );
            self.hit_targets.push(HitTarget::new(rect, button.action.clone()));
        }
    }

    fn render_tag_annotation_dialog(&mut self, frame: &mut Frame, area: Rect) {
        let Some(dialog) = &self.tag_annotation_dialog else {
            return;
        };

        let popup = centered_rect(area, 78, 62);
        frame.render_widget(Clear, popup);
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Tag Annotation ")
            .border_style(Style::default().fg(Color::Cyan));
        let inner = block.inner(popup);
        frame.render_widget(block, popup);

        let sections = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(2), Constraint::Min(8), Constraint::Length(BUTTON_ROW_HEIGHT)])
            .split(inner);

        frame.render_widget(
            Paragraph::new("Enter inserts a new line. F2 or Ctrl+S saves the annotation. Esc cancels."),
            sections[0],
        );
        self.render_tag_annotation_editor(frame, sections[1], dialog);

        self.render_button_row(
            frame,
            sections[2],
            &[
                DialogButton::new("Save", false, HitAction::SaveTagAnnotation, Style::default().fg(Color::Black).bg(Color::Green)),
                DialogButton::new("Cancel", false, HitAction::CancelTagAnnotation, Style::default().fg(Color::White).bg(Color::Red)),
            ],
        );
    }

    fn render_tag_annotation_editor(&self, frame: &mut Frame, area: Rect, dialog: &TagAnnotationDialog) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Annotation ")
            .border_style(Style::default().fg(Color::Cyan));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let lines = dialog.editor.lines();
        let (cursor_row, cursor_col) = dialog.editor.cursor();
        let visible_height = inner.height.max(1) as usize;
        let start_row = cursor_row.saturating_sub(visible_height / 2).min(lines.len().saturating_sub(visible_height));
        let end_row = (start_row + visible_height).min(lines.len());
        let number_width = end_row.max(1).to_string().len().max(2);
        let content_width = inner.width.saturating_sub(number_width as u16 + 1).max(1) as usize;

        let body = if lines.len() == 1 && lines[0].is_empty() {
            vec![Line::from(Span::styled(
                dialog.placeholder.as_str(),
                Style::default().fg(Color::DarkGray),
            ))]
        } else {
            lines[start_row..end_row]
                .iter()
                .enumerate()
                .map(|(offset, line)| {
                    let row_index = start_row + offset;
                    let active = row_index == cursor_row;
                    render_annotation_line(line, row_index + 1, number_width, content_width, active.then_some(cursor_col))
                })
                .collect::<Vec<_>>()
        };

        frame.render_widget(Paragraph::new(body), inner);
    }

    fn render_footer(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default().borders(Borders::ALL).title(" Controls ");
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let help = if self.browser_dialog.is_some() {
            "Arrows navigate | Enter open folder or select file | U use folder | Mouse click or wheel | Esc cancel"
        } else if self.project_edit_dialog.is_some() {
            "Tab move | Left/Right change enums | PgUp/PgDn or wheel scroll | Ctrl+O browse | F2 save | Del remove | Esc cancel"
        } else if self.tag_annotation_dialog.is_some() {
            "Type annotation | Enter newline | F2 or Ctrl+S save | Esc cancel"
        } else if self.tag_dialog.is_some() {
            "Type tag name | [ ] scope | A annotation | Left/Right action | Enter run | Esc cancel"
        } else if self.recent_changes_dialog.is_some() {
            "1/2 switch tabs | [ ] scope | Left/Right history | Up/Down scroll | T create tag | Esc close"
        } else if self.bump_dialog.is_some() {
            "Up/Down scope | Left/Right change bump action | Enter apply | Esc cancel"
        } else if self.overview_bump_workflow_dialog.is_some() {
            "1-3 or Up/Down choose action | Enter run | Esc cancel"
        } else if self.overview_bump_warning_dialog.is_some() {
            "1-3 or Up/Down choose warning action | Enter confirm | Esc cancel"
        } else {
            match self.screen {
                Screen::Dashboard => "1-4 tabs | Left/Right overview tabs | N new project | B bump | V view changes | T create tag | Up/Down select or scroll recent | Q quit",
                Screen::Settings => "1-4 tabs | Up/Down select project | E edit selected project | N new project | Q quit",
                Screen::UiSettings => "1-4 tabs | Enter, Space, T, Left, Right toggle tab hints | N new project | Q quit",
                Screen::Wizard => "Tab move | Left/Right change enums | PgUp/PgDn or wheel scroll | Ctrl+O browse | F5 read target | F2 save | Esc cancel",
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
            "Arrows navigate | Enter open folder | U use highlighted folder | Esc cancel"
        } else {
            "Arrows navigate | Enter select file or open folder | Mouse click selects | Esc cancel"
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

        if self.tag_annotation_dialog.is_some() {
            return self.handle_tag_annotation_key(key);
        }

        if self.tag_dialog.is_some() {
            return self.handle_tag_key(key);
        }

        if self.recent_changes_dialog.is_some() {
            return self.handle_recent_changes_key(key);
        }

        if self.overview_bump_warning_dialog.is_some() {
            return self.handle_overview_bump_warning_key(key);
        }

        if self.overview_bump_workflow_dialog.is_some() {
            return self.handle_overview_bump_workflow_key(key);
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
            KeyCode::Up => {
                if self.overview_show_recent_tab && self.overview_tab == OverviewTab::RecentChanges {
                    if let Some(dialog) = &mut self.overview_recent_changes {
                        dialog.scroll_by(-1);
                    }
                } else {
                    self.move_project_selection(-1);
                }
            }
            KeyCode::Down => {
                if self.overview_show_recent_tab && self.overview_tab == OverviewTab::RecentChanges {
                    if let Some(dialog) = &mut self.overview_recent_changes {
                        dialog.scroll_by(1);
                    }
                } else {
                    self.move_project_selection(1);
                }
            }
            KeyCode::Left => self.cycle_overview_tab(-1),
            KeyCode::Right => self.cycle_overview_tab(1),
            KeyCode::PageUp => {
                if let Some(dialog) = &mut self.overview_recent_changes {
                    dialog.scroll_by(-6);
                }
            }
            KeyCode::PageDown => {
                if let Some(dialog) = &mut self.overview_recent_changes {
                    dialog.scroll_by(6);
                }
            }
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
                KeyCode::PageUp => self.scroll_wizard_body(-3),
                KeyCode::PageDown => self.scroll_wizard_body(3),
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
            KeyCode::PageUp => self.scroll_wizard_body(-3),
            KeyCode::PageDown => self.scroll_wizard_body(3),
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
            KeyCode::Up | KeyCode::BackTab => self.rotate_bump_scope(-1),
            KeyCode::Down | KeyCode::Tab => self.rotate_bump_scope(1),
            KeyCode::Left => self.rotate_bump_action(-1),
            KeyCode::Right => self.rotate_bump_action(1),
            KeyCode::Enter | KeyCode::F(2) => self.apply_bump()?,
            _ => {}
        }
        Ok(())
    }

    fn handle_overview_bump_workflow_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => self.cancel_overview_bump_workflow(),
            KeyCode::Up | KeyCode::BackTab => self.rotate_overview_bump_workflow(-1),
            KeyCode::Down | KeyCode::Tab => self.rotate_overview_bump_workflow(1),
            KeyCode::Char('1') => self.select_overview_bump_workflow(0),
            KeyCode::Char('2') => self.select_overview_bump_workflow(1),
            KeyCode::Char('3') => self.select_overview_bump_workflow(2),
            KeyCode::Enter | KeyCode::F(2) => return self.confirm_overview_bump_workflow(),
            _ => {}
        }
        Ok(())
    }

    fn handle_overview_bump_warning_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => self.cancel_overview_bump_warning(),
            KeyCode::Up | KeyCode::BackTab => self.rotate_overview_bump_warning(-1),
            KeyCode::Down | KeyCode::Tab => self.rotate_overview_bump_warning(1),
            KeyCode::Char('1') => self.select_overview_bump_warning(0),
            KeyCode::Char('2') => self.select_overview_bump_warning(1),
            KeyCode::Char('3') => self.select_overview_bump_warning(2),
            KeyCode::Enter | KeyCode::F(2) => return self.confirm_overview_bump_warning(),
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
            KeyCode::Char('[') => {
                if let Some(dialog) = &mut self.recent_changes_dialog {
                    dialog.rotate_scope(-1)?;
                }
            }
            KeyCode::Char(']') => {
                if let Some(dialog) = &mut self.recent_changes_dialog {
                    dialog.rotate_scope(1)?;
                }
            }
            KeyCode::Left => {
                if let Some(dialog) = &mut self.recent_changes_dialog {
                    if dialog.active_tab == RecentChangesTab::Recent && dialog.can_select_scope() {
                        dialog.rotate_scope(-1)?;
                    } else if dialog.active_tab == RecentChangesTab::History {
                        dialog.navigate_history(1);
                    }
                }
            }
            KeyCode::Right => {
                if let Some(dialog) = &mut self.recent_changes_dialog {
                    if dialog.active_tab == RecentChangesTab::Recent && dialog.can_select_scope() {
                        dialog.rotate_scope(1)?;
                    } else if dialog.active_tab == RecentChangesTab::History {
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
                self.tag_annotation_dialog = None;
                self.status = StatusMessage::info("Tag creation cancelled.");
            }
            KeyCode::Char('[') => self.rotate_tag_scope(-1),
            KeyCode::Char(']') => self.rotate_tag_scope(1),
            KeyCode::Char('a') => self.open_tag_annotation_dialog()?,
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

    fn handle_tag_annotation_key(&mut self, key: KeyEvent) -> Result<()> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s') {
            return self.save_tag_annotation();
        }

        match key.code {
            KeyCode::Esc => {
                self.tag_annotation_dialog = None;
                self.status = StatusMessage::info("Tag annotation editor closed.");
            }
            KeyCode::F(2) => return self.save_tag_annotation(),
            _ => {
                if let Some(dialog) = &mut self.tag_annotation_dialog {
                    if let Some(input) = convert_to_textarea_input(key) {
                        dialog.editor.input(input);
                    }
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
                KeyCode::PageUp => self.scroll_project_edit_body(-3),
                KeyCode::PageDown => self.scroll_project_edit_body(3),
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
            KeyCode::PageUp => self.scroll_project_edit_body(-3),
            KeyCode::PageDown => self.scroll_project_edit_body(3),
            KeyCode::Enter => {
                if let Some(dialog) = &self.project_edit_dialog {
                    if dialog.is_save_focused() {
                        return self.save_project_edit();
                    }
                    if dialog.focus == ProjectEditFocus::AddScope {
                        return self.apply_project_edit_scope_action(ScopeAction::Add);
                    }
                    if dialog.focus == ProjectEditFocus::RemoveScope {
                        return self.apply_project_edit_scope_action(ScopeAction::Remove);
                    }
                    if dialog.focus == ProjectEditFocus::MoveScopeUp {
                        return self.apply_project_edit_scope_action(ScopeAction::MoveUp);
                    }
                    if dialog.focus == ProjectEditFocus::MoveScopeDown {
                        return self.apply_project_edit_scope_action(ScopeAction::MoveDown);
                    }
                    if dialog.focus == ProjectEditFocus::TargetKey {
                        if let Some(dialog) = &mut self.project_edit_dialog {
                            dialog.enable_custom_target_key();
                        }
                        self.status = StatusMessage::info("Custom target key input enabled.");
                        return Ok(());
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
                    self.scroll_project_edit_body(-1);
                } else if self.overview_bump_workflow_dialog.is_some() {
                } else if self.tag_dialog.is_some() {
                } else if self.recent_changes_dialog.is_some() {
                    self.scroll_recent_changes(-2);
                } else if self.bump_dialog.is_some() {
                    self.rotate_bump_action(-1);
                } else if self.screen == Screen::Wizard {
                    self.scroll_wizard_body(-1);
                } else if self.screen == Screen::Dashboard && self.overview_tab == OverviewTab::Overview {
                    if self
                        .overview_recent_viewport
                        .map(|viewport| rect_contains(viewport, mouse.column, mouse.row))
                        .unwrap_or(false)
                    {
                        if let Some(dialog) = &mut self.overview_recent_changes {
                            dialog.scroll_by(-2);
                        } else {
                            self.move_project_selection(-1);
                        }
                    } else if self
                        .overview_tile_viewport
                        .map(|viewport| rect_contains(viewport, mouse.column, mouse.row))
                        .unwrap_or(false)
                    {
                        if let Err(error) = self.scroll_dashboard_tiles(-1) {
                            self.status = StatusMessage::error(error.to_string());
                        }
                    } else if let Some(dialog) = &mut self.overview_recent_changes {
                        dialog.scroll_by(-2);
                    } else {
                        self.move_project_selection(-1);
                    }
                } else if self.screen == Screen::Dashboard && self.overview_tab == OverviewTab::RecentChanges {
                    if let Some(dialog) = &mut self.overview_recent_changes {
                        dialog.scroll_by(-2);
                    }
                } else if matches!(self.screen, Screen::Dashboard | Screen::Settings) {
                    self.move_project_selection(-1);
                }
            }
            MouseEventKind::ScrollDown => {
                if self.project_edit_dialog.is_some() {
                    self.scroll_project_edit_body(1);
                } else if self.overview_bump_workflow_dialog.is_some() {
                } else if self.tag_dialog.is_some() {
                } else if self.recent_changes_dialog.is_some() {
                    self.scroll_recent_changes(2);
                } else if self.bump_dialog.is_some() {
                    self.rotate_bump_action(1);
                } else if self.screen == Screen::Wizard {
                    self.scroll_wizard_body(1);
                } else if self.screen == Screen::Dashboard && self.overview_tab == OverviewTab::Overview {
                    if self
                        .overview_recent_viewport
                        .map(|viewport| rect_contains(viewport, mouse.column, mouse.row))
                        .unwrap_or(false)
                    {
                        if let Some(dialog) = &mut self.overview_recent_changes {
                            dialog.scroll_by(2);
                        } else {
                            self.move_project_selection(1);
                        }
                    } else if self
                        .overview_tile_viewport
                        .map(|viewport| rect_contains(viewport, mouse.column, mouse.row))
                        .unwrap_or(false)
                    {
                        if let Err(error) = self.scroll_dashboard_tiles(1) {
                            self.status = StatusMessage::error(error.to_string());
                        }
                    } else if let Some(dialog) = &mut self.overview_recent_changes {
                        dialog.scroll_by(2);
                    } else {
                        self.move_project_selection(1);
                    }
                } else if self.screen == Screen::Dashboard && self.overview_tab == OverviewTab::RecentChanges {
                    if let Some(dialog) = &mut self.overview_recent_changes {
                        dialog.scroll_by(2);
                    }
                } else if matches!(self.screen, Screen::Dashboard | Screen::Settings) {
                    self.move_project_selection(1);
                }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                if self.overview_bump_workflow_dialog.is_none()
                    && self.screen == Screen::Dashboard
                    && self.overview_tab == OverviewTab::Overview
                {
                    self.overview_drag_scope = self.overview_tile_rects.iter().rev().find_map(|(rect, scope)| {
                        if mouse.column >= rect.x
                            && mouse.column < rect.x + rect.width
                            && mouse.row >= rect.y
                            && mouse.row < rect.y + rect.height
                        {
                            Some(*scope)
                        } else {
                            None
                        }
                    });
                }
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
            MouseEventKind::Drag(MouseButton::Left) => {
                if let Some(from_scope) = self.overview_drag_scope {
                    let target_scope = self.overview_tile_rects.iter().rev().find_map(|(rect, scope)| {
                        (mouse.column >= rect.x
                            && mouse.column < rect.x + rect.width
                            && mouse.row >= rect.y
                            && mouse.row < rect.y + rect.height)
                            .then_some(*scope)
                    });
                    if let Some(to_scope) = target_scope {
                        if to_scope != from_scope {
                            self.reorder_dashboard_tile_scope(from_scope, to_scope);
                            self.overview_drag_scope = Some(to_scope);
                        }
                    }
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                self.overview_drag_scope = None;
            }
            MouseEventKind::Down(MouseButton::Right) => {
                if let Some(action) = self.hit_targets.iter().rev().find_map(|target| {
                    if target.contains(mouse.column, mouse.row) {
                        target.right_action.clone()
                    } else {
                        None
                    }
                }) {
                    if let Err(error) = self.handle_hit_action(action) {
                        self.status = StatusMessage::error(error.to_string());
                    }
                } else {
                    self.paste_from_clipboard();
                }
            }
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
        if let Some(dialog) = &mut self.tag_annotation_dialog {
            dialog.editor.insert_str(text);
            self.status = StatusMessage::info("Pasted into the tag annotation.");
            return;
        }

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
                if let Some(dialog) = &mut self.tag_annotation_dialog {
                    dialog.editor.insert_str(text);
                    self.status = StatusMessage::info("Pasted into the tag annotation.");
                    return;
                }

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
            HitAction::SelectOverviewTab(tab) => {
                self.overview_tab = tab;
            }
            HitAction::SelectProject(index) => {
                self.selected_project = index.min(self.config.projects.len().saturating_sub(1));
            }
            HitAction::SelectOverviewScope(scope_index) => return self.select_dashboard_overview_scope(scope_index),
            HitAction::OpenOverviewRecentChanges(scope_index) => return self.open_recent_changes_with_scope(Some(scope_index)),
            HitAction::BeginOverviewBump(scope_index) => return self.begin_overview_bump(scope_index),
            HitAction::SelectOverviewBumpWorkflow(index) => self.select_overview_bump_workflow(index),
            HitAction::ConfirmOverviewBumpWorkflow => return self.confirm_overview_bump_workflow(),
            HitAction::CancelOverviewBumpWorkflow => self.cancel_overview_bump_workflow(),
            HitAction::SelectOverviewBumpWarningChoice(index) => self.select_overview_bump_warning(index),
            HitAction::AdjustOverviewVersion(scope_index, control, delta) => {
                return self.adjust_overview_pending_version(scope_index, control, delta)
            }
            HitAction::ApplyOverviewVersionAndTag(scope_index) => {
                return self.apply_overview_pending_version(scope_index, true)
            }
            HitAction::OpenProjectEdit => return self.open_project_edit_dialog(),
            HitAction::EditProjectField(field) => {
                if let Some(dialog) = &mut self.project_edit_dialog {
                    dialog.focus = field;
                }
            }
            HitAction::ProjectEditScopeAction(action) => return self.apply_project_edit_scope_action(action),
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
            HitAction::EnableWizardCustomTargetKey => {
                self.wizard.enable_custom_target_key();
                self.status = StatusMessage::info("Custom target key input enabled.");
            }
            HitAction::EnableProjectCustomTargetKey => {
                if let Some(dialog) = &mut self.project_edit_dialog {
                    dialog.enable_custom_target_key();
                    self.status = StatusMessage::info("Custom target key input enabled.");
                }
            }
            HitAction::BrowserSelect(index) => self.select_browser_index(index),
            HitAction::SelectRecentChangesTab(tab) => {
                if let Some(dialog) = &mut self.recent_changes_dialog {
                    dialog.switch_tab(tab);
                }
            }
            HitAction::CycleRecentChangesScope(delta) => {
                if let Some(dialog) = &mut self.recent_changes_dialog {
                    dialog.rotate_scope(delta)?;
                }
            }
            HitAction::CycleBumpScope(delta) => self.rotate_bump_scope(delta),
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
            HitAction::OpenTagAnnotation => return self.open_tag_annotation_dialog(),
            HitAction::CycleTagScope(delta) => self.rotate_tag_scope(delta),
            HitAction::CycleTagAction(delta) => self.rotate_tag_action(delta),
            HitAction::CreateTag => return self.create_local_tag(),
            HitAction::SaveTagAnnotation => return self.save_tag_annotation(),
            HitAction::CancelTagAnnotation => {
                self.tag_annotation_dialog = None;
                self.status = StatusMessage::info("Tag annotation editor closed.");
            }
            HitAction::CancelTagDialog => {
                self.tag_dialog = None;
                self.tag_annotation_dialog = None;
                self.status = StatusMessage::info("Tag creation cancelled.");
            }
            HitAction::WizardField(field) => self.wizard.focus = field,
            HitAction::WizardScopeAction(action) => return self.apply_wizard_scope_action(action),
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
        self.open_recent_changes_with_scope(None)
    }

    fn open_recent_changes_with_scope(&mut self, preferred_scope: Option<usize>) -> Result<()> {
        let project = self.selected_project()?.clone();
        let dialog = RecentChangesDialog::from_project_with_scope(&project, preferred_scope.unwrap_or(0))?;
        self.bump_dialog = None;
        self.tag_dialog = None;
        self.project_edit_dialog = None;
        self.recent_changes_dialog = Some(dialog);
        self.status = StatusMessage::info("Showing git changes for the selected project.");
        Ok(())
    }

    fn ensure_dashboard_recent_changes(&mut self) {
        let Some(project) = self.config.projects.get(self.selected_project) else {
            self.overview_recent_project = None;
            self.overview_recent_changes = None;
            self.overview_recent_error = None;
            return;
        };

        let project_changed = self.overview_recent_project != Some(self.selected_project);
        self.overview_recent_project = Some(self.selected_project);
        if !project.integration_mode.requires_repo() {
            self.overview_recent_changes = None;
            self.overview_recent_error = None;
            return;
        }

        if project_changed || self.overview_recent_changes.is_none() {
            self.overview_recent_changes = None;
            self.overview_recent_error = None;
            match RecentChangesDialog::from_project(project) {
                Ok(dialog) => self.overview_recent_changes = Some(dialog),
                Err(error) => self.overview_recent_error = Some(error.to_string()),
            }
            return;
        }

        if let Some(dialog) = &mut self.overview_recent_changes {
            if let Err(error) = dialog.refresh_current_scope() {
                self.overview_recent_changes = None;
                self.overview_recent_error = Some(error.to_string());
            } else {
                self.overview_recent_error = None;
            }
        }
    }

    fn ensure_dashboard_tile_state(&mut self, scopes: &[BumpScope]) {
        if self.overview_tile_project == Some(self.selected_project)
            && self.overview_scope_order.len() == scopes.len()
            && self.overview_pending_versions.len() == scopes.len()
        {
            return;
        }

        self.overview_tile_project = Some(self.selected_project);
        self.overview_scope_order = (0..scopes.len()).collect();
        self.overview_pending_versions = scopes
            .iter()
            .map(|scope| scope.current_version.clone().unwrap_or_else(|| scope.version_label().to_string()))
            .collect();
        self.overview_tile_scroll = 0;
    }

    fn invalidate_overview_cache(&mut self) {
        self.overview_recent_project = None;
        self.overview_tile_project = None;
    }

    fn reorder_dashboard_tile_scope(&mut self, from_scope: usize, to_scope: usize) {
        let Some(from_index) = self.overview_scope_order.iter().position(|scope| *scope == from_scope) else {
            return;
        };
        let Some(to_index) = self.overview_scope_order.iter().position(|scope| *scope == to_scope) else {
            return;
        };
        if from_index == to_index {
            return;
        }

        let moved = self.overview_scope_order.remove(from_index);
        self.overview_scope_order.insert(to_index, moved);
    }

    fn scroll_dashboard_tiles(&mut self, delta: isize) -> Result<()> {
        let viewport = match self.overview_tile_viewport {
            Some(viewport) => viewport,
            None => return Ok(()),
        };
        let project = self.selected_project()?.clone();
        let scopes = collect_bump_scopes(&project)?;
        if scopes.is_empty() {
            self.overview_tile_scroll = 0;
            return Ok(());
        }

        let columns = dashboard_tile_columns(viewport.width).max(1);
        let row_height = scopes
            .iter()
            .map(|scope| tile_height(scope.scheme))
            .max()
            .unwrap_or(7)
            .saturating_add(1);
        let visible_rows = ((viewport.height.saturating_add(1)) / row_height.max(1)).max(1) as usize;
        let total_rows = self.overview_scope_order.len().div_ceil(columns);
        let max_scroll = total_rows.saturating_sub(visible_rows);
        self.overview_tile_scroll = (self.overview_tile_scroll as isize + delta)
            .clamp(0, max_scroll as isize) as usize;
        Ok(())
    }

    fn render_dashboard_tiles(&mut self, frame: &mut Frame, area: Rect, project: &ProjectConfig, scopes: &[BumpScope]) {
        self.overview_tile_viewport = Some(area);

        if scopes.is_empty() || area.width == 0 || area.height == 0 {
            return;
        }

        let git_contexts = collect_all_branch_git_scope_contexts(project).ok();
        let columns = dashboard_tile_columns(area.width).max(1);
        let vertical_gap = 1;
        let row_height = scopes
            .iter()
            .map(|scope| tile_height(scope.scheme))
            .max()
            .unwrap_or(7)
            .saturating_add(vertical_gap);
        let visible_rows = ((area.height.saturating_add(vertical_gap)) / row_height.max(1)).max(1) as usize;
        let total_rows = self.overview_scope_order.len().div_ceil(columns);
        let max_scroll = total_rows.saturating_sub(visible_rows);
        self.overview_tile_scroll = self.overview_tile_scroll.min(max_scroll);

        let visible_row_scopes = (self.overview_tile_scroll..(self.overview_tile_scroll + visible_rows).min(total_rows))
            .map(|row| {
                let start = row * columns;
                let end = (start + columns).min(self.overview_scope_order.len());
                self.overview_scope_order[start..end].to_vec()
            })
            .filter(|row| !row.is_empty())
            .collect::<Vec<_>>();

        let row_constraints = visible_row_scopes
            .iter()
            .map(|row| {
                let row_tile_height = row
                    .iter()
                    .filter_map(|scope_index| scopes.get(*scope_index))
                    .map(|scope| tile_height(scope.scheme))
                    .max()
                    .unwrap_or(7);
                Constraint::Length(row_tile_height)
            })
            .collect::<Vec<_>>();
        let row_areas = Layout::default()
            .direction(Direction::Vertical)
            .constraints(row_constraints)
            .flex(Flex::SpaceEvenly)
            .split(area);

        for (row_area, row_scopes) in row_areas.iter().zip(visible_row_scopes.iter()) {
            let column_areas = Layout::default()
                .direction(Direction::Horizontal)
                .constraints(vec![Constraint::Length(TILE_WIDTH.min(area.width)); row_scopes.len()])
                .flex(Flex::SpaceEvenly)
                .split(*row_area);

            for (cell_area, scope_index) in column_areas.iter().zip(row_scopes.iter().copied()) {
                let Some(scope) = scopes.get(scope_index) else {
                    continue;
                };

                let tile_rect = center_vertically(*cell_area, tile_height(scope.scheme));
                if tile_rect.width < 12 || tile_rect.height < 4 {
                    continue;
                }

                let activity = git_contexts
                    .as_ref()
                    .and_then(|entries| entries.get(scope_index))
                    .and_then(|context| load_scope_activity_summary(context).ok());
                let selected = self
                    .overview_recent_changes
                    .as_ref()
                    .map(|dialog| dialog.selected_scope == scope_index)
                    .unwrap_or(scope_index == 0);
                let tile = OverviewTileData {
                    name: scope.display_name.clone(),
                    scheme: scope.scheme,
                    preview_version: self
                        .overview_pending_versions
                        .get(scope_index)
                        .cloned()
                        .unwrap_or_else(|| scope.current_version.clone().unwrap_or_else(|| scope.version_label().to_string())),
                    commits_since_tag_label: activity
                        .as_ref()
                        .map(|summary| summary.commits_since_tag_label.clone())
                        .unwrap_or_else(|| "n/a".to_string()),
                    last_bump_label: activity
                        .as_ref()
                        .map(|summary| summary.last_bump_label.clone())
                        .unwrap_or_else(|| "n/a".to_string()),
                    last_commit_label: activity
                        .as_ref()
                        .map(|summary| summary.last_commit_label.clone())
                        .unwrap_or_else(|| "n/a".to_string()),
                    selected,
                };
                let hotspots = render_overview_tile(frame, tile_rect, &tile);
                self.overview_tile_rects.push((hotspots.tile_rect, scope_index));

                self.hit_targets.push(HitTarget::new(hotspots.title_rect, HitAction::SelectOverviewScope(scope_index)));
                self.hit_targets.push(HitTarget::new(hotspots.view_rect, HitAction::OpenOverviewRecentChanges(scope_index)));
                self.hit_targets.push(HitTarget::new(hotspots.bump_rect, HitAction::BeginOverviewBump(scope_index)));
                self.hit_targets.push(HitTarget::new(hotspots.tag_rect, HitAction::ApplyOverviewVersionAndTag(scope_index)));
                if let Some(rect) = hotspots.major_rect {
                    self.hit_targets.push(HitTarget::with_right_action(
                        rect,
                        HitAction::AdjustOverviewVersion(scope_index, OverviewVersionControl::Major, 1),
                        HitAction::AdjustOverviewVersion(scope_index, OverviewVersionControl::Major, -1),
                    ));
                }
                if let Some(rect) = hotspots.minor_rect {
                    self.hit_targets.push(HitTarget::with_right_action(
                        rect,
                        HitAction::AdjustOverviewVersion(scope_index, OverviewVersionControl::Minor, 1),
                        HitAction::AdjustOverviewVersion(scope_index, OverviewVersionControl::Minor, -1),
                    ));
                }
                if let Some(rect) = hotspots.patch_rect {
                    self.hit_targets.push(HitTarget::with_right_action(
                        rect,
                        HitAction::AdjustOverviewVersion(scope_index, OverviewVersionControl::Patch, 1),
                        HitAction::AdjustOverviewVersion(scope_index, OverviewVersionControl::Patch, -1),
                    ));
                }
                if let Some(rect) = hotspots.version_rect {
                    self.hit_targets.push(HitTarget::with_right_action(
                        rect,
                        HitAction::AdjustOverviewVersion(scope_index, OverviewVersionControl::Whole, 1),
                        HitAction::AdjustOverviewVersion(scope_index, OverviewVersionControl::Whole, -1),
                    ));
                }
            }
        }

        if self.overview_tile_scroll > 0 && area.height > 0 {
            let indicator = Rect {
                x: area.x,
                y: area.y,
                width: area.width,
                height: 1,
            };
            frame.render_widget(
                Paragraph::new("more scopes above").alignment(Alignment::Right).style(Style::default().fg(Color::DarkGray)),
                indicator,
            );
        }

        if self.overview_tile_scroll < max_scroll && area.height > 0 {
            let indicator = Rect {
                x: area.x,
                y: area.y + area.height.saturating_sub(1),
                width: area.width,
                height: 1,
            };
            frame.render_widget(
                Paragraph::new("more scopes below").alignment(Alignment::Right).style(Style::default().fg(Color::DarkGray)),
                indicator,
            );
        }
    }

    fn select_dashboard_overview_scope(&mut self, scope_index: usize) -> Result<()> {
        self.ensure_dashboard_recent_changes();
        if let Some(dialog) = &mut self.overview_recent_changes {
            dialog.select_scope(scope_index)?;
        }
        Ok(())
    }

    fn begin_overview_bump(&mut self, scope_index: usize) -> Result<()> {
        let project = self.selected_project()?.clone();
        if !project.integration_mode.requires_repo() {
            return self.apply_overview_pending_version(scope_index, false);
        }

        let scopes = collect_bump_scopes(&project)?;
        self.ensure_dashboard_tile_state(&scopes);
        let next_version = self
            .overview_pending_versions
            .get(scope_index)
            .cloned()
            .or_else(|| scopes.get(scope_index).and_then(|scope| scope.current_version.clone()))
            .ok_or_else(|| anyhow!("the selected scope does not have a resolved version value"))?;
        let scope_label = if project.unified_versioning {
            "All configured scopes".to_string()
        } else {
            scopes
                .get(scope_index)
                .map(|scope| scope.display_name.clone())
                .unwrap_or_else(|| project.name.clone())
        };
        let options = match project.integration_mode {
            IntegrationMode::LocalOnly => vec![OverviewBumpWorkflow::JustBump],
            IntegrationMode::GitLocalOnly => vec![
                OverviewBumpWorkflow::JustBump,
                OverviewBumpWorkflow::Commit,
                OverviewBumpWorkflow::CommitAndTag,
            ],
            IntegrationMode::GitHubEnabled => vec![
                OverviewBumpWorkflow::JustBump,
                OverviewBumpWorkflow::CommitAndPush,
                OverviewBumpWorkflow::CommitPushAndTag,
            ],
        };

        self.overview_bump_workflow_dialog = Some(OverviewBumpWorkflowDialog::new(
            project.name,
            scope_label,
            next_version,
            scope_index,
            options,
        ));
        self.status = StatusMessage::info("Choose how the tile bump should be applied.");
        Ok(())
    }

    fn select_overview_bump_workflow(&mut self, index: usize) {
        if let Some(dialog) = &mut self.overview_bump_workflow_dialog {
            dialog.select(index);
        }
    }

    fn rotate_overview_bump_workflow(&mut self, delta: isize) {
        if let Some(dialog) = &mut self.overview_bump_workflow_dialog {
            dialog.rotate(delta);
        }
    }

    fn cancel_overview_bump_workflow(&mut self) {
        self.overview_bump_workflow_dialog = None;
        self.status = StatusMessage::info("Tile bump action cancelled.");
    }

    fn select_overview_bump_warning(&mut self, index: usize) {
        if let Some(dialog) = &mut self.overview_bump_warning_dialog {
            dialog.select(index);
        }
    }

    fn rotate_overview_bump_warning(&mut self, delta: isize) {
        if let Some(dialog) = &mut self.overview_bump_warning_dialog {
            dialog.rotate(delta);
        }
    }

    fn cancel_overview_bump_warning(&mut self) {
        self.overview_bump_warning_dialog = None;
        self.overview_bump_workflow_dialog = None;
        self.status = StatusMessage::info("Tile bump action cancelled.");
    }

    fn adjust_overview_pending_version(
        &mut self,
        scope_index: usize,
        control: OverviewVersionControl,
        delta: i32,
    ) -> Result<()> {
        let project = self.selected_project()?.clone();
        let scopes = collect_bump_scopes(&project)?;
        self.ensure_dashboard_tile_state(&scopes);
        let Some(scope) = scopes.get(scope_index) else {
            return Ok(());
        };
        let current = self
            .overview_pending_versions
            .get(scope_index)
            .cloned()
            .unwrap_or_else(|| scope.current_version.clone().unwrap_or_else(|| scope.version_label().to_string()));
        let next = adjust_pending_version_value(scope.scheme, &current, control, delta)?;
        if project.unified_versioning {
            for pending in &mut self.overview_pending_versions {
                *pending = next.clone();
            }
        } else if let Some(pending) = self.overview_pending_versions.get_mut(scope_index) {
            *pending = next;
        }
        Ok(())
    }

    fn apply_overview_pending_version(&mut self, scope_index: usize, open_tag_after: bool) -> Result<()> {
        let project = self.selected_project()?.clone();
        let scopes = collect_bump_scopes(&project)?;
        let scope_repo_roots = self.scope_repo_roots(&project, scopes.len());
        self.ensure_dashboard_tile_state(&scopes);
        let affected_scope_indexes = if project.unified_versioning {
            (0..scopes.len()).collect::<Vec<_>>()
        } else {
            vec![scope_index]
        };
        let next_version = self
            .overview_pending_versions
            .get(scope_index)
            .cloned()
            .or_else(|| scopes.get(scope_index).and_then(|scope| scope.current_version.clone()))
            .ok_or_else(|| anyhow!("the selected scope does not have a resolved version value"))?;

        for index in &affected_scope_indexes {
            if let Some(scope) = scopes.get(*index) {
                for target in &scope.targets {
                    write_target_version(target, &next_version)?;
                    refresh_target_artifacts(target, scope_repo_roots.get(*index).and_then(|root| root.as_deref()))?;
                }
                if let Some(pending) = self.overview_pending_versions.get_mut(*index) {
                    *pending = next_version.clone();
                }
            }
        }

        self.invalidate_overview_cache();
        self.ensure_dashboard_recent_changes();

        if open_tag_after {
            if project.integration_mode.requires_repo() {
                let preferred_scope = if project.unified_versioning { None } else { Some(scope_index) };
                self.open_tag_dialog_with_scope(preferred_scope, Some(TagAction::CreateAndPush))?;
                self.status = StatusMessage::info("Version updated. Review the tag-and-push action next.");
            } else {
                self.status = StatusMessage::warning("Tagging requires a git-backed project.");
            }
        } else {
            self.status = StatusMessage::success(format!("Updated version to {} from the overview tile.", next_version));
        }

        Ok(())
    }

    fn confirm_overview_bump_workflow(&mut self) -> Result<()> {
        let Some(dialog) = self.overview_bump_workflow_dialog.clone() else {
            return Ok(());
        };

        if dialog.selected_workflow() != OverviewBumpWorkflow::JustBump {
            let warnings = self.collect_overview_bump_warnings(dialog.scope_index)?;
            if !warnings.is_empty() {
                self.overview_bump_warning_dialog = Some(OverviewBumpWarningDialog::new(
                    dialog.scope_index,
                    dialog.selected_workflow(),
                    warnings,
                ));
                self.status = StatusMessage::warning("Previously staged files were found. Review them before committing the bump.");
                return Ok(());
            }
        }

        self.execute_overview_bump_workflow(dialog.scope_index, dialog.selected_workflow())?;
        self.overview_bump_workflow_dialog = None;
        Ok(())
    }

    fn confirm_overview_bump_warning(&mut self) -> Result<()> {
        let Some(dialog) = self.overview_bump_warning_dialog.clone() else {
            return Ok(());
        };

        match dialog.selected_choice() {
            OverviewBumpWarningChoice::Continue => {
                self.execute_overview_bump_workflow(dialog.scope_index, dialog.workflow)?;
                self.overview_bump_warning_dialog = None;
                self.overview_bump_workflow_dialog = None;
            }
            OverviewBumpWarningChoice::UnstageExtras => {
                for repo in &dialog.repos {
                    unstage_paths(&repo.repo_root, &repo.extra_paths)?;
                }
                self.execute_overview_bump_workflow(dialog.scope_index, dialog.workflow)?;
                self.overview_bump_warning_dialog = None;
                self.overview_bump_workflow_dialog = None;
            }
            OverviewBumpWarningChoice::Cancel => self.cancel_overview_bump_warning(),
        }
        Ok(())
    }

    fn collect_overview_bump_warnings(&self, scope_index: usize) -> Result<Vec<UnexpectedStagedRepo>> {
        let project = self.selected_project()?.clone();
        let scopes = collect_bump_scopes(&project)?;
        let affected_scope_indexes = if project.unified_versioning {
            (0..scopes.len()).collect::<Vec<_>>()
        } else {
            vec![scope_index]
        };
        let git_contexts = collect_all_branch_git_scope_contexts(&project)?;
        let repo_operations = collect_repo_bump_operations(&project, &scopes, &git_contexts, &affected_scope_indexes)?;
        collect_unexpected_staged_paths(&repo_operations)
    }

    fn execute_overview_bump_workflow(&mut self, scope_index: usize, workflow: OverviewBumpWorkflow) -> Result<()> {
        let project = self.selected_project()?.clone();
        let scopes = collect_bump_scopes(&project)?;
        let scope_repo_roots = self.scope_repo_roots(&project, scopes.len());
        self.ensure_dashboard_tile_state(&scopes);
        let affected_scope_indexes = if project.unified_versioning {
            (0..scopes.len()).collect::<Vec<_>>()
        } else {
            vec![scope_index]
        };
        let next_version = self
            .overview_pending_versions
            .get(scope_index)
            .cloned()
            .or_else(|| scopes.get(scope_index).and_then(|scope| scope.current_version.clone()))
            .ok_or_else(|| anyhow!("the selected scope does not have a resolved version value"))?;

        for index in &affected_scope_indexes {
            if let Some(scope) = scopes.get(*index) {
                for target in &scope.targets {
                    write_target_version(target, &next_version)?;
                    refresh_target_artifacts(target, scope_repo_roots.get(*index).and_then(|root| root.as_deref()))?;
                }
                if let Some(pending) = self.overview_pending_versions.get_mut(*index) {
                    *pending = next_version.clone();
                }
            }
        }

        if workflow != OverviewBumpWorkflow::JustBump {
            let git_contexts = collect_all_branch_git_scope_contexts(&project)?;
            let repo_operations = collect_repo_bump_operations(&project, &scopes, &git_contexts, &affected_scope_indexes)?;
            apply_repo_bump_workflow(&repo_operations, &next_version, workflow)?;
        }

        self.invalidate_overview_cache();
        self.ensure_dashboard_recent_changes();

        let target_count = affected_scope_indexes
            .iter()
            .filter_map(|index| scopes.get(*index))
            .map(|scope| scope.targets.len())
            .sum::<usize>();
        let scope_notice = if project.unified_versioning {
            String::new()
        } else {
            scopes
                .get(scope_index)
                .map(|scope| format!(" in scope '{}'", scope.display_name))
                .unwrap_or_default()
        };
        self.status = StatusMessage::success(format!(
            "Updated {} target{}{} to {} via {}.",
            target_count,
            if target_count == 1 { "" } else { "s" },
            scope_notice,
            next_version,
            workflow.display_name()
        ));
        Ok(())
    }

    fn open_tag_dialog_with_scope(&mut self, preferred_scope: Option<usize>, preferred_action: Option<TagAction>) -> Result<()> {
        let project = self.selected_project()?.clone();
        let dialog = TagDialog::from_project(&project, preferred_scope, preferred_action)?;
        self.bump_dialog = None;
        self.project_edit_dialog = None;
        self.browser_dialog = None;
        self.tag_annotation_dialog = None;
        self.tag_dialog = Some(dialog);
        self.status = StatusMessage::info("Review the proposed tag name, add an optional annotation, then run the tag action.");
        Ok(())
    }

    fn scroll_recent_changes(&mut self, delta: i16) {
        if let Some(dialog) = &mut self.recent_changes_dialog {
            dialog.scroll_by(delta);
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

    fn scroll_project_edit_body(&mut self, delta: isize) {
        if let Some(dialog) = &mut self.project_edit_dialog {
            dialog.scroll_body(delta);
        }
    }

    fn scroll_wizard_body(&mut self, delta: isize) {
        self.wizard.scroll_body(delta);
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
        self.invalidate_overview_cache();
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
        self.invalidate_overview_cache();
        self.status = StatusMessage::success(format!("Removed project '{}'.", removed.name));
        Ok(())
    }

    fn open_tag_dialog(&mut self) -> Result<()> {
        let preferred_scope = self
            .recent_changes_dialog
            .as_ref()
            .and_then(|dialog| dialog.can_select_scope().then_some(dialog.selected_scope));
        self.open_tag_dialog_with_scope(preferred_scope, None)
    }

    fn rotate_tag_scope(&mut self, delta: isize) {
        if let Some(dialog) = &mut self.tag_dialog {
            dialog.rotate_scope(delta);
        }
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

        let active_scope = dialog.active_scope().clone();
        let repo_root = active_scope.repo_root.clone();
        let project_name = dialog.project_name.clone();
        let action = dialog.selected_action();
        let remote_spec = active_scope.remote_spec.clone();
        let annotation = dialog.annotation.trim().to_string();
        let tag_name = tag_name.to_string();
        let created = ensure_local_tag(
            &repo_root,
            &tag_name,
            if annotation.is_empty() { None } else { Some(annotation.as_str()) },
        )?;

        if matches!(action, TagAction::CreateAndPush | TagAction::CreatePushAndRelease) {
            let remote_spec = remote_spec.ok_or_else(|| anyhow!("no remote is configured for this project"))?;
            run_git_checked(&repo_root, &["push", &remote_spec, &tag_name])?;
        }

        if matches!(action, TagAction::CreatePushAndRelease) {
            ensure_gh_available()?;
            run_gh_checked(&repo_root, &["release", "create", &tag_name, "--generate-notes"])?;
        }

        self.tag_dialog = None;
        self.tag_annotation_dialog = None;
        let scope_notice = if active_scope.scope_kind.is_some() {
            format!(" for {}", active_scope.display_name)
        } else {
            String::new()
        };
        let summary = match action {
            TagAction::CreateLocal if created => format!("Created local tag '{}' in {}{}.", tag_name, project_name, scope_notice),
            TagAction::CreateLocal => format!("Tag '{}' already existed locally in {}{}.", tag_name, project_name, scope_notice),
            TagAction::CreateAndPush => format!("Tag '{}' is present locally and has been pushed for {}{}.", tag_name, project_name, scope_notice),
            TagAction::CreatePushAndRelease => format!("Tag '{}' was created, pushed, and released for {}{}.", tag_name, project_name, scope_notice),
        };
        self.status = StatusMessage::success(if annotation.is_empty() {
            summary
        } else {
            format!("{} Annotation included.", summary)
        });
        Ok(())
    }

    fn open_tag_annotation_dialog(&mut self) -> Result<()> {
        let current_annotation = self
            .tag_dialog
            .as_ref()
            .map(|dialog| dialog.annotation.clone())
            .ok_or_else(|| anyhow!("no tag dialog is active"))?;

        self.tag_annotation_dialog = Some(TagAnnotationDialog::new(&current_annotation));
        self.status = StatusMessage::info("Editing tag annotation. F2 or Ctrl+S saves it.");
        Ok(())
    }

    fn save_tag_annotation(&mut self) -> Result<()> {
        let dialog = self
            .tag_annotation_dialog
            .take()
            .ok_or_else(|| anyhow!("no tag annotation editor is active"))?;
        let annotation = dialog.editor.lines().join("\n");

        if let Some(tag_dialog) = &mut self.tag_dialog {
            tag_dialog.annotation = annotation;
        }

        self.status = StatusMessage::success("Tag annotation saved.");
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
        self.status = StatusMessage::info("Review the preview, then press Enter to apply the bump for the active target set.");
        Ok(())
    }

    fn rotate_bump_action(&mut self, delta: isize) {
        if let Some(dialog) = &mut self.bump_dialog {
            dialog.rotate_action(delta);
        }
    }

    fn rotate_bump_scope(&mut self, delta: isize) {
        if let Some(dialog) = &mut self.bump_dialog {
            dialog.rotate_scope(delta);
        }
    }

    fn apply_bump(&mut self) -> Result<()> {
        let Some(dialog) = &self.bump_dialog else {
            return Ok(());
        };

        let next_version = dialog.preview_next_version().map_err(anyhow::Error::msg)?;
        let targets = dialog.active_targets();
        for target in &targets {
            write_target_version(target, &next_version)?;
            refresh_target_artifacts(target, None)?;
        }

        let target_count = targets.len();
        let scope_notice = if dialog.unified_versioning {
            String::new()
        } else {
            format!(" in scope '{}'", dialog.active_scope().display_name)
        };
        let preferred_scope = if dialog.unified_versioning {
            None
        } else {
            Some(dialog.selected_scope)
        };
        self.bump_dialog = None;
        let repo_backed = self.selected_project()?.integration_mode.requires_repo();
        self.status = StatusMessage::success(format!(
            "Updated {} target{}{} to {}.",
            target_count,
            if target_count == 1 { "" } else { "s" },
            scope_notice,
            next_version
        ));
        if repo_backed {
            self.open_tag_dialog_with_scope(preferred_scope, Some(TagAction::CreateAndPush))?;
            self.status = StatusMessage::info("Version bump applied. Review the suggested tag-and-push action next.");
        }
        Ok(())
    }

    fn open_wizard(&mut self) {
        self.wizard = ProjectWizard::default();
        self.browser_dialog = None;
        self.screen = Screen::Wizard;
        self.status = StatusMessage::info("Configure a project and read each target file before saving.");
    }

    fn activate_wizard_focus(&mut self) -> Result<()> {
        match self.wizard.focus {
            WizardField::AddScope => self.apply_wizard_scope_action(ScopeAction::Add),
            WizardField::RemoveScope => self.apply_wizard_scope_action(ScopeAction::Remove),
            WizardField::MoveScopeUp => self.apply_wizard_scope_action(ScopeAction::MoveUp),
            WizardField::MoveScopeDown => self.apply_wizard_scope_action(ScopeAction::MoveDown),
            WizardField::TargetKey => {
                self.wizard.enable_custom_target_key();
                self.status = StatusMessage::info("Custom target key input enabled.");
                Ok(())
            }
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

    fn apply_wizard_scope_action(&mut self, action: ScopeAction) -> Result<()> {
        match action {
            ScopeAction::Add => {
                self.wizard.add_scope();
                self.status = StatusMessage::info("Added a new branched scope draft.");
            }
            ScopeAction::Remove => {
                self.wizard.remove_selected_scope()?;
                self.status = StatusMessage::info("Removed the selected branched scope.");
            }
            ScopeAction::MoveUp => {
                self.wizard.move_selected_scope(-1);
                self.status = StatusMessage::info("Moved the selected scope earlier.");
            }
            ScopeAction::MoveDown => {
                self.wizard.move_selected_scope(1);
                self.status = StatusMessage::info("Moved the selected scope later.");
            }
        }
        Ok(())
    }

    fn apply_project_edit_scope_action(&mut self, action: ScopeAction) -> Result<()> {
        let Some(dialog) = &mut self.project_edit_dialog else {
            return Ok(());
        };

        match action {
            ScopeAction::Add => {
                dialog.add_scope();
                self.status = StatusMessage::info("Added a new branched scope draft.");
            }
            ScopeAction::Remove => {
                dialog.remove_selected_scope()?;
                self.status = StatusMessage::info("Removed the selected branched scope.");
            }
            ScopeAction::MoveUp => {
                dialog.move_selected_scope(-1);
                self.status = StatusMessage::info("Moved the selected scope earlier.");
            }
            ScopeAction::MoveDown => {
                dialog.move_selected_scope(1);
                self.status = StatusMessage::info("Moved the selected scope later.");
            }
        }

        Ok(())
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
        let (target_path, target_key) = if self.wizard.project_type == ProjectType::Branched {
            self.wizard
                .current_scope()
                .map(|scope| (scope.target_path.value().trim().to_string(), scope.target_key.value().trim().to_string()))
                .unwrap_or_default()
        } else {
            (
                self.wizard.target_path.value.trim().to_string(),
                self.wizard.target_key.value.trim().to_string(),
            )
        };

        match probe_target(&target_path, &target_key, self.wizard.version_scheme) {
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
                if self.wizard.project_type == ProjectType::Branched {
                    if let Some(scope) = self.wizard.current_scope_mut() {
                        scope.last_probe = Some(probe);
                    }
                } else {
                    self.wizard.last_probe = Some(probe);
                }
            }
            Err(error) => {
                self.status = StatusMessage::error(error.to_string());
                let probe = TargetProbe {
                    kind: ProbeKind::Error,
                    message: error.to_string(),
                    version: None,
                    format: None,
                };
                if self.wizard.project_type == ProjectType::Branched {
                    if let Some(scope) = self.wizard.current_scope_mut() {
                        scope.last_probe = Some(probe);
                    }
                } else {
                    self.wizard.last_probe = Some(probe);
                }
            }
        }
    }

    fn save_wizard_project(&mut self) -> Result<()> {
        let project = self.wizard.build_project()?;
        self.config.projects.push(project);
        self.config_store.save(&self.config)?;
        self.selected_project = self.config.projects.len().saturating_sub(1);
        self.invalidate_overview_cache();
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
        let selected_name = dialog.explorer.current().name.clone();
        let target = dialog.target;
        let select_directories = dialog.select_directories;

        if selected.is_dir() {
            if let Some(dialog) = &mut self.browser_dialog {
                dialog.explorer.handle(ExplorerInput::Right)?;
            }
            self.status = StatusMessage::info(if selected_name == "../" {
                "Moved to the parent folder.".to_string()
            } else {
                format!("Entered folder '{}'.", selected_name)
            });
            return Ok(());
        }

        if select_directories && !selected.is_dir() {
            self.status = StatusMessage::warning("Select a directory for Repo root, or press U to use the current file's folder.");
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

    fn cycle_overview_tab(&mut self, delta: isize) {
        let tabs = if self.overview_show_recent_tab {
            [OverviewTab::Overview, OverviewTab::RecentChanges, OverviewTab::ProjectDetail].as_slice()
        } else {
            [OverviewTab::Overview, OverviewTab::ProjectDetail].as_slice()
        };
        let current = tabs
            .iter()
            .position(|tab| *tab == self.overview_tab)
            .unwrap_or(0) as isize;
        let next = (current + delta).rem_euclid(tabs.len() as isize) as usize;
        self.overview_tab = tabs[next];
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
    right_action: Option<HitAction>,
}

impl HitTarget {
    fn new(rect: Rect, action: HitAction) -> Self {
        Self {
            rect,
            action,
            right_action: None,
        }
    }

    fn with_right_action(rect: Rect, action: HitAction, right_action: HitAction) -> Self {
        Self {
            rect,
            action,
            right_action: Some(right_action),
        }
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
    SelectOverviewTab(OverviewTab),
    SelectProject(usize),
    SelectOverviewScope(usize),
    OpenOverviewRecentChanges(usize),
    BeginOverviewBump(usize),
    SelectOverviewBumpWorkflow(usize),
    ConfirmOverviewBumpWorkflow,
    CancelOverviewBumpWorkflow,
    SelectOverviewBumpWarningChoice(usize),
    AdjustOverviewVersion(usize, OverviewVersionControl, i32),
    ApplyOverviewVersionAndTag(usize),
    OpenProjectEdit,
    EditProjectField(ProjectEditFocus),
    ProjectEditScopeAction(ScopeAction),
    SaveProjectEdit,
    RemoveProject,
    CancelProjectEdit,
    ToggleTabHints,
    BrowseWizardTargetPath,
    BrowseWizardRepoRoot,
    BrowseProjectTargetPath,
    BrowseProjectRepoRoot,
    EnableWizardCustomTargetKey,
    EnableProjectCustomTargetKey,
    BrowserSelect(usize),
    SelectRecentChangesTab(RecentChangesTab),
    CycleRecentChangesScope(isize),
    CloseRecentChanges,
    ScrollRecentChanges(i16),
    OpenTagDialog,
    OpenTagAnnotation,
    CycleTagScope(isize),
    CycleTagAction(isize),
    CycleBumpAction(isize),
    CycleBumpScope(isize),
    ApplyBump,
    CancelBump,
    CreateTag,
    SaveTagAnnotation,
    CancelTagAnnotation,
    CancelTagDialog,
    WizardField(WizardField),
    WizardScopeAction(ScopeAction),
    ValidateWizard,
    SaveWizard,
    CancelWizard,
}

#[derive(Clone, Copy)]
enum ScopeAction {
    Add,
    Remove,
    MoveUp,
    MoveDown,
}

#[derive(Clone, Copy)]
enum OverviewVersionControl {
    Major,
    Minor,
    Patch,
    Whole,
}

#[derive(Clone)]
struct OverviewBumpWorkflowDialog {
    project_name: String,
    scope_label: String,
    next_version: String,
    scope_index: usize,
    options: Vec<OverviewBumpWorkflow>,
    selected: usize,
}

impl OverviewBumpWorkflowDialog {
    fn new(
        project_name: String,
        scope_label: String,
        next_version: String,
        scope_index: usize,
        options: Vec<OverviewBumpWorkflow>,
    ) -> Self {
        Self {
            project_name,
            scope_label,
            next_version,
            scope_index,
            options,
            selected: 0,
        }
    }

    fn selected_workflow(&self) -> OverviewBumpWorkflow {
        self.options[self.selected.min(self.options.len().saturating_sub(1))]
    }

    fn select(&mut self, index: usize) {
        self.selected = index.min(self.options.len().saturating_sub(1));
    }

    fn rotate(&mut self, delta: isize) {
        if self.options.is_empty() {
            self.selected = 0;
            return;
        }

        let len = self.options.len() as isize;
        self.selected = (self.selected as isize + delta).rem_euclid(len) as usize;
    }
}

#[derive(Clone)]
struct OverviewBumpWarningDialog {
    scope_index: usize,
    workflow: OverviewBumpWorkflow,
    repos: Vec<UnexpectedStagedRepo>,
    selected: usize,
}

impl OverviewBumpWarningDialog {
    fn new(scope_index: usize, workflow: OverviewBumpWorkflow, repos: Vec<UnexpectedStagedRepo>) -> Self {
        Self {
            scope_index,
            workflow,
            repos,
            selected: 1,
        }
    }

    fn select(&mut self, index: usize) {
        self.selected = index.min(2);
    }

    fn rotate(&mut self, delta: isize) {
        self.selected = (self.selected as isize + delta).rem_euclid(3) as usize;
    }

    fn selected_choice(&self) -> OverviewBumpWarningChoice {
        match self.selected {
            0 => OverviewBumpWarningChoice::Continue,
            1 => OverviewBumpWarningChoice::UnstageExtras,
            _ => OverviewBumpWarningChoice::Cancel,
        }
    }
}

#[derive(Clone, Copy)]
enum OverviewBumpWarningChoice {
    Continue,
    UnstageExtras,
    Cancel,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum OverviewBumpWorkflow {
    JustBump,
    Commit,
    CommitAndTag,
    CommitAndPush,
    CommitPushAndTag,
}

impl OverviewBumpWorkflow {
    fn display_name(self) -> &'static str {
        match self {
            OverviewBumpWorkflow::JustBump => "Just bump",
            OverviewBumpWorkflow::Commit => "Bump & Commit",
            OverviewBumpWorkflow::CommitAndTag => "Bump & Commit & Tag",
            OverviewBumpWorkflow::CommitAndPush => "Bump & Commit & Push",
            OverviewBumpWorkflow::CommitPushAndTag => "Bump & Commit & Push & Tag",
        }
    }

    fn description(self) -> &'static str {
        match self {
            OverviewBumpWorkflow::JustBump => "Writes the updated version files only.",
            OverviewBumpWorkflow::Commit => "Stages the version files and commits them with the standard bump message.",
            OverviewBumpWorkflow::CommitAndTag => "Stages and commits the version files, then creates a tag named after the new version.",
            OverviewBumpWorkflow::CommitAndPush => "Stages and commits the version files, then pushes the bump commit to the configured remote.",
            OverviewBumpWorkflow::CommitPushAndTag => "Stages and commits the version files, pushes the bump commit, then pushes a tag named after the new version.",
        }
    }

    fn requires_push(self) -> bool {
        matches!(self, OverviewBumpWorkflow::CommitAndPush | OverviewBumpWorkflow::CommitPushAndTag)
    }

    fn requires_tag(self) -> bool {
        matches!(self, OverviewBumpWorkflow::CommitAndTag | OverviewBumpWorkflow::CommitPushAndTag)
    }
}

#[derive(Clone)]
struct RepoBumpOperation {
    repo_root: String,
    remote_spec: Option<String>,
    stage_paths: Vec<String>,
}

#[derive(Clone)]
struct UnexpectedStagedRepo {
    repo_root: String,
    extra_paths: Vec<String>,
}

fn collect_repo_bump_operations(
    _project: &ProjectConfig,
    scopes: &[BumpScope],
    git_contexts: &[crate::git::GitScopeContext],
    affected_scope_indexes: &[usize],
) -> Result<Vec<RepoBumpOperation>> {
    let mut operations = Vec::<RepoBumpOperation>::new();

    for scope_index in affected_scope_indexes {
        let scope = scopes
            .get(*scope_index)
            .ok_or_else(|| anyhow!("the selected scope does not exist"))?;
        let context = git_contexts
            .get(*scope_index)
            .or_else(|| git_contexts.first())
            .ok_or_else(|| anyhow!("git scope metadata is unavailable for the selected bump targets"))?;
        let stage_paths = collect_stage_paths_for_targets(&context.repo_root, &scope.targets);

        if let Some(existing) = operations.iter_mut().find(|operation| operation.repo_root == context.repo_root) {
            for path in stage_paths {
                if !existing.stage_paths.iter().any(|candidate| candidate == &path) {
                    existing.stage_paths.push(path);
                }
            }
        } else {
            operations.push(RepoBumpOperation {
                repo_root: context.repo_root.clone(),
                remote_spec: context.remote_spec.clone(),
                stage_paths,
            });
        }
    }

    Ok(operations)
}

fn apply_repo_bump_workflow(
    operations: &[RepoBumpOperation],
    next_version: &str,
    workflow: OverviewBumpWorkflow,
) -> Result<()> {
    let commit_message = format!("bump: CVB version bump to {}", next_version);

    for operation in operations {
        if !operation.stage_paths.is_empty() {
            let mut add_args = vec!["add".to_string(), "--".to_string()];
            add_args.extend(operation.stage_paths.iter().cloned());
            run_git_checked_owned(&operation.repo_root, add_args)?;
        }

        if has_staged_changes(&operation.repo_root)? {
            run_git_checked(&operation.repo_root, &["commit", "-m", &commit_message])?;
        }

        if workflow.requires_tag() {
            ensure_local_tag(&operation.repo_root, next_version, None)?;
        }

        if workflow.requires_push() {
            let remote_spec = operation
                .remote_spec
                .as_deref()
                .ok_or_else(|| anyhow!("no remote is configured for this project"))?;
            run_git_checked(&operation.repo_root, &["push", remote_spec])?;
            if workflow.requires_tag() {
                run_git_checked(&operation.repo_root, &["push", remote_spec, next_version])?;
            }
        }
    }

    Ok(())
}

fn has_staged_changes(repo_root: &str) -> Result<bool> {
    Ok(!run_git(repo_root, &["diff", "--cached", "--quiet", "--exit-code"])?.success)
}

fn staged_paths(repo_root: &str) -> Result<Vec<String>> {
    Ok(split_output_lines(&run_git_checked(repo_root, &["diff", "--cached", "--name-only", "--diff-filter=ACMR"])?))
}

fn collect_unexpected_staged_paths(operations: &[RepoBumpOperation]) -> Result<Vec<UnexpectedStagedRepo>> {
    let mut warnings = Vec::new();

    for operation in operations {
        let expected = operation
            .stage_paths
            .iter()
            .map(|path| comparable_git_path(path))
            .collect::<HashSet<_>>();
        let extra_paths = staged_paths(&operation.repo_root)?
            .into_iter()
            .filter(|path| !expected.contains(&comparable_git_path(path)))
            .collect::<Vec<_>>();
        if !extra_paths.is_empty() {
            warnings.push(UnexpectedStagedRepo {
                repo_root: operation.repo_root.clone(),
                extra_paths,
            });
        }
    }

    Ok(warnings)
}

fn unstage_paths(repo_root: &str, paths: &[String]) -> Result<()> {
    if paths.is_empty() {
        return Ok(());
    }

    let mut args = vec!["restore".to_string(), "--staged".to_string(), "--".to_string()];
    args.extend(paths.iter().cloned());
    run_git_checked_owned(repo_root, args)?;
    Ok(())
}

fn collect_stage_paths_for_targets(repo_root: &str, targets: &[BumpTarget]) -> Vec<String> {
    let mut paths = Vec::new();

    for target in targets {
        push_stage_path(&mut paths, repo_root, &target.path);
        if target.format == TargetFormat::Toml {
            let target_path = resolve_repo_path(repo_root, &target.path);
            let is_cargo_manifest = target_path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.eq_ignore_ascii_case("Cargo.toml"));
            if is_cargo_manifest {
                let lock_path = target_path.with_file_name("Cargo.lock");
                if lock_path.is_file() {
                    push_stage_path(&mut paths, repo_root, &lock_path.display().to_string());
                }
            }
        }
    }

    paths
}

fn refresh_target_artifacts(target: &BumpTarget, repo_root: Option<&str>) -> Result<()> {
    if target.format != TargetFormat::Toml {
        return Ok(());
    }

    let target_path = resolve_target_path(repo_root, &target.path);
    let is_cargo_manifest = target_path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("Cargo.toml"));
    if !is_cargo_manifest {
        return Ok(());
    }

    let lock_path = target_path.with_file_name("Cargo.lock");
    if !lock_path.is_file() {
        return Ok(());
    }

    let output = Command::new("cargo")
        .arg("generate-lockfile")
        .arg("--manifest-path")
        .arg(&target_path)
        .output()
        .with_context(|| format!("failed to refresh {} after updating {}", lock_path.display(), target.path))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let detail = if stderr.is_empty() { stdout } else { stderr };
        bail!("failed to refresh {} after updating {}: {}", lock_path.display(), target.path, detail);
    }

    Ok(())
}

fn push_stage_path(paths: &mut Vec<String>, repo_root: &str, path: &str) {
    let candidate = normalize_repo_stage_path(repo_root, path);
    if !candidate.is_empty() && !paths.iter().any(|existing| existing == &candidate) {
        paths.push(candidate);
    }
}

fn normalize_repo_stage_path(repo_root: &str, path: &str) -> String {
    let candidate = Path::new(path);
    if candidate.is_absolute() {
        candidate
            .strip_prefix(repo_root)
            .map(|relative| relative.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|_| path.replace('\\', "/"))
    } else {
        path.replace('\\', "/")
    }
}

fn resolve_repo_path(repo_root: &str, path: &str) -> PathBuf {
    let candidate = Path::new(path);
    if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        Path::new(repo_root).join(candidate)
    }
}

fn resolve_target_path(repo_root: Option<&str>, path: &str) -> PathBuf {
    let candidate = Path::new(path);
    if candidate.is_absolute() {
        candidate.to_path_buf()
    } else if let Some(repo_root) = repo_root {
        Path::new(repo_root).join(candidate)
    } else {
        candidate.to_path_buf()
    }
}

fn comparable_git_path(path: &str) -> String {
    path.replace('\\', "/").to_ascii_lowercase()
}

fn run_git_checked_owned(repo_root: &str, args: Vec<String>) -> Result<String> {
    let arg_refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    run_git_checked(repo_root, &arg_refs)
}

#[derive(Clone)]
struct ScopeDraft {
    name: TextInput,
    label: String,
    label_follows_name: bool,
    target_label: String,
    target_path: TextInput,
    target_key: TextInput,
    target_key_custom: bool,
    scope_kind: BranchScopeKind,
    repo: Option<RepoConfig>,
    format: TargetFormat,
    last_probe: Option<TargetProbe>,
}

impl ScopeDraft {
    fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            name: TextInput::with_value(name.clone()),
            label: name.clone(),
            label_follows_name: true,
            target_label: "Version".to_string(),
            target_path: TextInput::with_value(""),
            target_key: TextInput::with_value("version"),
            target_key_custom: false,
            scope_kind: BranchScopeKind::Branch,
            repo: None,
            format: TargetFormat::Auto,
            last_probe: None,
        }
    }

    fn from_target(name: impl Into<String>, target: &TargetSpec) -> Self {
        let mut scope = Self::new(name);
        scope.target_label = target.label.clone();
        scope.target_path = TextInput::with_value(target.path.clone());
        scope.target_key = TextInput::with_value(target.key_path.clone());
        scope.target_key_custom = target_key_is_custom(&target.path, &target.key_path);
        scope.format = target.format;
        scope
    }

    fn from_branch(branch: &BranchConfig) -> Result<Self> {
        let target = branch
            .targets
            .first()
            .ok_or_else(|| anyhow!("branched project does not contain any editable targets yet"))?;
        let label = if branch.label.trim().is_empty() {
            branch.name.clone()
        } else {
            branch.label.clone()
        };
        Ok(Self {
            name: TextInput::with_value(branch.name.clone()),
            label,
            label_follows_name: branch.label.trim().is_empty() || branch.label == branch.name,
            target_label: target.label.clone(),
            target_path: TextInput::with_value(target.path.clone()),
            target_key: TextInput::with_value(target.key_path.clone()),
            target_key_custom: target_key_is_custom(&target.path, &target.key_path),
            scope_kind: branch.scope_kind,
            repo: branch.repo.clone(),
            format: target.format,
            last_probe: None,
        })
    }

    fn display_name(&self) -> String {
        let name = self.name.value.trim();
        if name.is_empty() {
            "(unnamed scope)".to_string()
        } else if self.label_follows_name || self.label.trim().is_empty() || self.label == name {
            name.to_string()
        } else {
            format!("{} [{}]", self.label, name)
        }
    }

    fn sync_label_if_needed(&mut self) {
        if self.label_follows_name {
            self.label = self.name.value.trim().to_string();
        }
    }

    fn build_branch(&self, version_scheme: VersionScheme, require_probe: bool) -> Result<BranchConfig> {
        let name = self.name.value.trim();
        if name.is_empty() {
            bail!("scope name cannot be empty");
        }

        let target_path = self.target_path.value.trim();
        if target_path.is_empty() {
            bail!("scope '{}' target path cannot be empty", name);
        }

        let target_key = self.target_key.value.trim();
        if target_key.is_empty() {
            bail!("scope '{}' target key cannot be empty", name);
        }

        let format = if require_probe {
            match &self.last_probe {
                Some(probe) if matches!(probe.kind, ProbeKind::Success) => probe.format.unwrap_or(self.format),
                Some(_) | None => bail!("scope '{}' must be read successfully before saving", name),
            }
        } else {
            self.last_probe.as_ref().and_then(|probe| probe.format).unwrap_or(self.format)
        };

        Ok(BranchConfig {
            name: name.to_string(),
            label: if self.label_follows_name || self.label.trim().is_empty() {
                name.to_string()
            } else {
                self.label.clone()
            },
            scope_kind: self.scope_kind,
            repo: self.repo.clone(),
            version_scheme,
            targets: vec![TargetSpec {
                label: self.target_label.clone(),
                path: target_path.to_string(),
                key_path: target_key.to_string(),
                format,
            }],
        })
    }
}

#[derive(Clone)]
struct ProjectEditDialog {
    project_index: usize,
    project_name: String,
    name: TextInput,
    target_path: TextInput,
    target_key: TextInput,
    target_key_custom: bool,
    scopes: Vec<ScopeDraft>,
    selected_scope: usize,
    field_scroll: usize,
    viewport_rows: usize,
    repo_root: TextInput,
    remote_url: TextInput,
    project_type: ProjectType,
    unified_versioning: bool,
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

        let scopes = if project.project_type == ProjectType::Branched {
            project
                .branches
                .iter()
                .map(ScopeDraft::from_branch)
                .collect::<Result<Vec<_>>>()?
        } else {
            vec![ScopeDraft::from_target("core", primary_target)]
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
            target_path: TextInput::with_value(primary_target.path.clone()),
            target_key: TextInput::with_value(primary_target.key_path.clone()),
            target_key_custom: target_key_is_custom(&primary_target.path, &primary_target.key_path),
            scopes,
            selected_scope: 0,
            field_scroll: 0,
            viewport_rows: 1,
            repo_root: TextInput::with_value(repo_root),
            remote_url: TextInput::with_value(remote_url),
            project_type: project.project_type,
            unified_versioning: project.unified_versioning,
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
                | ProjectEditFocus::ScopeName
                | ProjectEditFocus::TargetPath
                | ProjectEditFocus::RepoRoot
                | ProjectEditFocus::RemoteUrl
        ) || (self.focus == ProjectEditFocus::TargetKey && self.target_key_accepts_text())
    }

    fn visible_fields(&self) -> Vec<ProjectEditFocus> {
        let mut fields = vec![ProjectEditFocus::Name, ProjectEditFocus::ProjectType];
        if self.project_type == ProjectType::Branched {
            fields.extend([
                ProjectEditFocus::ScopeSelection,
                ProjectEditFocus::ScopeName,
                ProjectEditFocus::ScopeKind,
                ProjectEditFocus::UnifiedVersioning,
            ]);
        }
        fields.extend([
            ProjectEditFocus::VersionScheme,
            ProjectEditFocus::IntegrationMode,
            ProjectEditFocus::TargetPath,
            ProjectEditFocus::TargetKey,
        ]);
        if self.project_type == ProjectType::Branched {
            fields.extend([
                ProjectEditFocus::AddScope,
                ProjectEditFocus::RemoveScope,
                ProjectEditFocus::MoveScopeUp,
                ProjectEditFocus::MoveScopeDown,
            ]);
        }
        if self.integration_mode.requires_repo() {
            fields.push(ProjectEditFocus::RepoRoot);
        }
        if self.integration_mode.requires_remote() {
            fields.push(ProjectEditFocus::RemoteUrl);
        }
        fields.extend([ProjectEditFocus::Save, ProjectEditFocus::Remove, ProjectEditFocus::Cancel]);
        fields
    }

    fn body_fields(&self) -> Vec<ProjectEditFocus> {
        self.visible_fields()
            .into_iter()
            .filter(|field| !matches!(field, ProjectEditFocus::Save | ProjectEditFocus::Remove | ProjectEditFocus::Cancel))
            .collect()
    }

    fn render_field(&self, field: ProjectEditFocus) -> (&'static str, HitAction) {
        match field {
            ProjectEditFocus::Name => ("Project name", HitAction::EditProjectField(field)),
            ProjectEditFocus::ProjectType => ("Project type", HitAction::EditProjectField(field)),
            ProjectEditFocus::ScopeSelection => ("Scope", HitAction::EditProjectField(field)),
            ProjectEditFocus::ScopeName => ("Scope name", HitAction::EditProjectField(field)),
            ProjectEditFocus::ScopeKind => ("Scope kind", HitAction::EditProjectField(field)),
            ProjectEditFocus::VersionScheme => ("Version scheme", HitAction::EditProjectField(field)),
            ProjectEditFocus::UnifiedVersioning => ("Unified versioning", HitAction::EditProjectField(field)),
            ProjectEditFocus::IntegrationMode => ("Integration", HitAction::EditProjectField(field)),
            ProjectEditFocus::TargetPath => ("Target path", HitAction::EditProjectField(field)),
            ProjectEditFocus::TargetKey => ("Target key", HitAction::EditProjectField(field)),
            ProjectEditFocus::AddScope => ("Add scope", HitAction::ProjectEditScopeAction(ScopeAction::Add)),
            ProjectEditFocus::RemoveScope => ("Remove scope", HitAction::ProjectEditScopeAction(ScopeAction::Remove)),
            ProjectEditFocus::MoveScopeUp => ("Move scope up", HitAction::ProjectEditScopeAction(ScopeAction::MoveUp)),
            ProjectEditFocus::MoveScopeDown => ("Move scope down", HitAction::ProjectEditScopeAction(ScopeAction::MoveDown)),
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
            ProjectEditFocus::ScopeSelection => self.selected_scope_summary(),
            ProjectEditFocus::ScopeName => self
                .current_scope()
                .map(|scope| scope.name.display_value_with_width(focused, max_width))
                .unwrap_or_else(|| "(no scope)".to_string()),
            ProjectEditFocus::ScopeKind => self
                .current_scope()
                .map(|scope| format!("< {} >", scope.scope_kind.display_name()))
                .unwrap_or_else(|| format!("< {} >", BranchScopeKind::Branch.display_name())),
            ProjectEditFocus::VersionScheme => format!("< {} >", self.version_scheme.display_name()),
            ProjectEditFocus::UnifiedVersioning => {
                if self.project_type == ProjectType::Branched {
                    format!("< {} >", if self.unified_versioning { "Yes" } else { "No" })
                } else {
                    "Always yes for all-in-one projects".to_string()
                }
            }
            ProjectEditFocus::IntegrationMode => format!("< {} >", self.integration_mode.display_name()),
            ProjectEditFocus::TargetPath => {
                if self.project_type == ProjectType::Branched {
                    self.current_scope()
                        .map(|scope| scope.target_path.display_value_with_width(focused, max_width))
                        .unwrap_or_default()
                } else {
                    self.target_path.display_value_with_width(focused, max_width)
                }
            }
            ProjectEditFocus::TargetKey => {
                if self.project_type == ProjectType::Branched {
                    self.current_scope()
                        .map(|scope| {
                            if scope.target_key_custom {
                                scope.target_key.display_value_with_width(focused, max_width)
                            } else {
                                format!("< {} >", scope.target_key.value())
                            }
                        })
                        .unwrap_or_default()
                } else {
                    if self.target_key_custom {
                        self.target_key.display_value_with_width(focused, max_width)
                    } else {
                        format!("< {} >", self.target_key.value())
                    }
                }
            }
            ProjectEditFocus::AddScope => "Create a new scope draft".to_string(),
            ProjectEditFocus::RemoveScope => "Drop the selected scope".to_string(),
            ProjectEditFocus::MoveScopeUp => "Move the selected scope earlier".to_string(),
            ProjectEditFocus::MoveScopeDown => "Move the selected scope later".to_string(),
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
                if self.project_type == ProjectType::Branched {
                    self.seed_scope_from_primary_target();
                } else {
                    self.copy_selected_scope_to_primary_target();
                }
            }
            ProjectEditFocus::ScopeSelection => self.move_scope_selection(delta),
            ProjectEditFocus::ScopeKind => {
                if let Some(scope) = self.current_scope_mut() {
                    scope.scope_kind = rotate_scope_kind(scope.scope_kind, delta);
                }
            }
            ProjectEditFocus::TargetKey => self.rotate_target_key_preset(delta),
            ProjectEditFocus::VersionScheme => {
                self.version_scheme = if delta >= 0 {
                    self.version_scheme.next()
                } else {
                    self.version_scheme.previous()
                };
            }
            ProjectEditFocus::UnifiedVersioning => {
                if self.project_type == ProjectType::Branched {
                    self.unified_versioning = !self.unified_versioning;
                }
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
        if self.focus == ProjectEditFocus::ScopeName {
            if let Some(scope) = self.current_scope_mut() {
                scope.sync_label_if_needed();
            }
        }
        if self.focus == ProjectEditFocus::TargetPath {
            self.sync_target_key_preset_with_path();
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
            ProjectEditFocus::ScopeName => self.current_scope_mut().map(|scope| &mut scope.name),
            ProjectEditFocus::TargetPath => {
                if self.project_type == ProjectType::Branched {
                    self.current_scope_mut().map(|scope| &mut scope.target_path)
                } else {
                    Some(&mut self.target_path)
                }
            }
            ProjectEditFocus::TargetKey => {
                if self.project_type == ProjectType::Branched && self.current_scope().is_some_and(|scope| scope.target_key_custom) {
                    self.current_scope_mut().map(|scope| &mut scope.target_key)
                } else if self.project_type != ProjectType::Branched && self.target_key_custom {
                    Some(&mut self.target_key)
                } else {
                    None
                }
            }
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
        let body_fields = self.body_fields();
        self.sync_body_scroll(&body_fields);
    }

    fn refresh_body_window(&mut self, viewport_height: u16) -> (Vec<ProjectEditFocus>, u16, bool, bool) {
        let body_fields = self.body_fields();
        let row_height = dialog_form_row_height(viewport_height);
        self.viewport_rows = dialog_visible_rows(viewport_height, row_height);
        self.sync_body_scroll(&body_fields);
        let start = self.field_scroll.min(body_fields.len().saturating_sub(self.viewport_rows));
        let end = (start + self.viewport_rows).min(body_fields.len());
        (
            body_fields[start..end].to_vec(),
            row_height,
            start > 0,
            end < body_fields.len(),
        )
    }

    fn scroll_body(&mut self, delta: isize) {
        let body_fields = self.body_fields();
        if body_fields.is_empty() {
            self.field_scroll = 0;
            return;
        }

        let max_scroll = body_fields.len().saturating_sub(self.viewport_rows.max(1));
        let next = (self.field_scroll as isize + delta).clamp(0, max_scroll as isize) as usize;
        self.field_scroll = next;
    }

    fn sync_body_scroll(&mut self, body_fields: &[ProjectEditFocus]) {
        let visible_rows = self.viewport_rows.max(1);
        clamp_dialog_scroll(
            &mut self.field_scroll,
            body_fields.len(),
            visible_rows,
            body_fields.iter().position(|field| *field == self.focus),
        );
    }

    fn prefill_repo_root_from_target_path(&mut self) {
        if !self.repo_root.is_empty() {
            return;
        }
        let target_path = if self.project_type == ProjectType::Branched {
            self.current_scope().map(|scope| scope.target_path.value()).unwrap_or("")
        } else {
            self.target_path.value()
        };
        if let Some(repo_root) = derive_repo_root_from_target_path(target_path) {
            self.repo_root.set_value(repo_root);
        }
    }

    fn set_target_path_from_browse(&mut self, path: String) {
        if self.project_type == ProjectType::Branched {
            if let Some(scope) = self.current_scope_mut() {
                scope.target_path.set_value(path);
                if !scope.target_key_custom {
                    scope.target_key.set_value(default_target_key_for_path(scope.target_path.value()));
                }
            }
        } else {
            self.target_path.set_value(path);
            if !self.target_key_custom {
                self.target_key.set_value(default_target_key_for_path(self.target_path.value()));
            }
        }
        self.prefill_repo_root_from_target_path();
    }

    fn target_key_accepts_text(&self) -> bool {
        if self.project_type == ProjectType::Branched {
            self.current_scope().is_some_and(|scope| scope.target_key_custom)
        } else {
            self.target_key_custom
        }
    }

    fn enable_custom_target_key(&mut self) {
        if self.project_type == ProjectType::Branched {
            if let Some(scope) = self.current_scope_mut() {
                scope.target_key_custom = true;
            }
        } else {
            self.target_key_custom = true;
        }
        self.focus = ProjectEditFocus::TargetKey;
    }

    fn rotate_target_key_preset(&mut self, delta: i32) {
        if self.project_type == ProjectType::Branched {
            if let Some(scope) = self.current_scope_mut() {
                let next = cycle_target_key_preset(scope.target_path.value(), scope.target_key.value(), delta);
                scope.target_key.set_value(next);
                scope.target_key_custom = false;
            }
        } else {
            let next = cycle_target_key_preset(self.target_path.value(), self.target_key.value(), delta);
            self.target_key.set_value(next);
            self.target_key_custom = false;
        }
    }

    fn sync_target_key_preset_with_path(&mut self) {
        if self.project_type == ProjectType::Branched {
            if let Some(scope) = self.current_scope_mut() {
                if !scope.target_key_custom {
                    scope.target_key.set_value(default_target_key_for_path(scope.target_path.value()));
                }
            }
        } else if !self.target_key_custom {
            self.target_key.set_value(default_target_key_for_path(self.target_path.value()));
        }
    }

    fn set_repo_root_from_browse(&mut self, path: String) {
        self.repo_root.set_value(path);
    }

    fn apply(&self, project: &mut ProjectConfig) -> Result<()> {
        let project_name = self.name.value.trim();
        if project_name.is_empty() {
            bail!("project name cannot be empty");
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
        project.unified_versioning = self.project_type == ProjectType::AllInOne || self.unified_versioning;
        project.version_scheme = self.version_scheme;

        if self.project_type == ProjectType::AllInOne {
            let target_path = self.target_path.value.trim();
            if target_path.is_empty() {
                bail!("target path cannot be empty");
            }

            let target_key = self.target_key.value.trim();
            if target_key.is_empty() {
                bail!("target key cannot be empty");
            }

            let target = TargetSpec {
                label: existing_target.label,
                path: target_path.to_string(),
                key_path: target_key.to_string(),
                format: existing_target.format,
            };
            project.targets = vec![target];
            project.branches.clear();
        } else {
            project.targets.clear();
            project.branches = self.build_branches(false)?;
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

    fn current_scope(&self) -> Option<&ScopeDraft> {
        self.scopes.get(self.selected_scope)
    }

    fn current_scope_mut(&mut self) -> Option<&mut ScopeDraft> {
        self.scopes.get_mut(self.selected_scope)
    }

    fn selected_scope_summary(&self) -> String {
        let total = self.scopes.len();
        if total == 0 {
            "< no scopes >".to_string()
        } else {
            let summary = self
                .current_scope()
                .map(|scope| scope.display_name())
                .unwrap_or_else(|| "(unknown)".to_string());
            format!("< {}/{}: {} >", self.selected_scope + 1, total, summary)
        }
    }

    fn move_scope_selection(&mut self, delta: i32) {
        if self.scopes.is_empty() {
            return;
        }
        let len = self.scopes.len() as i32;
        let next = (self.selected_scope as i32 + delta).rem_euclid(len) as usize;
        self.selected_scope = next;
    }

    fn next_scope_name(&self) -> String {
        let mut index = self.scopes.len() + 1;
        loop {
            let candidate = format!("scope-{}", index);
            if self
                .scopes
                .iter()
                .all(|scope| !scope.name.value.trim().eq_ignore_ascii_case(&candidate))
            {
                return candidate;
            }
            index += 1;
        }
    }

    fn add_scope(&mut self) {
        let scope = ScopeDraft::new(self.next_scope_name());
        self.scopes.push(scope);
        self.selected_scope = self.scopes.len().saturating_sub(1);
        self.focus = ProjectEditFocus::ScopeName;
    }

    fn remove_selected_scope(&mut self) -> Result<()> {
        if self.scopes.len() <= 1 {
            bail!("branched projects need at least one scope");
        }
        self.scopes.remove(self.selected_scope);
        self.selected_scope = self.selected_scope.min(self.scopes.len().saturating_sub(1));
        self.focus = ProjectEditFocus::ScopeSelection;
        Ok(())
    }

    fn move_selected_scope(&mut self, delta: isize) {
        if self.scopes.len() < 2 {
            return;
        }
        let len = self.scopes.len() as isize;
        let next = (self.selected_scope as isize + delta).clamp(0, len - 1) as usize;
        if next != self.selected_scope {
            self.scopes.swap(self.selected_scope, next);
            self.selected_scope = next;
        }
    }

    fn seed_scope_from_primary_target(&mut self) {
        if self.scopes.is_empty() {
            self.scopes.push(ScopeDraft::new("core"));
            self.selected_scope = 0;
        }
        let target_path = self.target_path.value.trim().to_string();
        let target_key = self.target_key.value.trim().to_string();
        let target_key_custom = self.target_key_custom;
        if let Some(scope) = self.current_scope_mut() {
            if scope.target_path.value.trim().is_empty() && !target_path.is_empty() {
                scope.target_path.set_value(target_path);
            }
            if scope.target_key.value.trim().is_empty() && !target_key.is_empty() {
                scope.target_key.set_value(target_key);
                scope.target_key_custom = target_key_custom;
            }
        }
    }

    fn copy_selected_scope_to_primary_target(&mut self) {
        let selected = self.current_scope().map(|scope| {
            (
                scope.target_path.value().to_string(),
                scope.target_key.value().to_string(),
                scope.target_key_custom,
            )
        });
        if let Some((target_path, target_key, target_key_custom)) = selected {
            self.target_path.set_value(target_path);
            self.target_key.set_value(target_key);
            self.target_key_custom = target_key_custom;
        }
    }

    fn build_branches(&self, require_probe: bool) -> Result<Vec<BranchConfig>> {
        if self.scopes.is_empty() {
            bail!("branched projects need at least one scope");
        }

        let mut names = HashSet::new();
        let mut branches = Vec::with_capacity(self.scopes.len());
        for scope in &self.scopes {
            let branch = scope.build_branch(self.version_scheme, require_probe)?;
            let key = branch.name.trim().to_ascii_lowercase();
            if !names.insert(key) {
                bail!("scope names must be unique");
            }
            branches.push(branch);
        }
        Ok(branches)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ProjectEditFocus {
    Name,
    ProjectType,
    ScopeSelection,
    ScopeName,
    ScopeKind,
    VersionScheme,
    UnifiedVersioning,
    IntegrationMode,
    TargetPath,
    TargetKey,
    AddScope,
    RemoveScope,
    MoveScopeUp,
    MoveScopeDown,
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
    ScopeSelection,
    ScopeName,
    ScopeKind,
    VersionScheme,
    UnifiedVersioning,
    IntegrationMode,
    TargetPath,
    TargetKey,
    AddScope,
    RemoveScope,
    MoveScopeUp,
    MoveScopeDown,
    RepoRoot,
    RemoteUrl,
    Validate,
    Save,
    Cancel,
}

#[derive(Clone)]
struct ProjectWizard {
    name: TextInput,
    target_path: TextInput,
    target_key: TextInput,
    target_key_custom: bool,
    scopes: Vec<ScopeDraft>,
    selected_scope: usize,
    field_scroll: usize,
    viewport_rows: usize,
    repo_root: TextInput,
    remote_url: TextInput,
    project_type: ProjectType,
    unified_versioning: bool,
    integration_mode: IntegrationMode,
    version_scheme: VersionScheme,
    focus: WizardField,
    last_probe: Option<TargetProbe>,
}

impl Default for ProjectWizard {
    fn default() -> Self {
        Self {
            name: TextInput::with_value(""),
            target_path: TextInput::with_value(""),
            target_key: TextInput::with_value("version"),
            target_key_custom: false,
            scopes: vec![ScopeDraft::new("core")],
            selected_scope: 0,
            field_scroll: 0,
            viewport_rows: 1,
            repo_root: TextInput::with_value(""),
            remote_url: TextInput::with_value(""),
            project_type: ProjectType::AllInOne,
            unified_versioning: false,
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
                | WizardField::ScopeName
                | WizardField::TargetPath
                | WizardField::RepoRoot
                | WizardField::RemoteUrl
        ) || (self.focus == WizardField::TargetKey && self.target_key_accepts_text())
    }

    fn visible_fields(&self) -> Vec<WizardField> {
        let mut fields = vec![
            WizardField::Name,
            WizardField::ProjectType,
        ];
        if self.project_type == ProjectType::Branched {
            fields.extend([
                WizardField::ScopeSelection,
                WizardField::ScopeName,
                WizardField::ScopeKind,
                WizardField::UnifiedVersioning,
            ]);
        }
        fields.extend([
            WizardField::VersionScheme,
            WizardField::IntegrationMode,
            WizardField::TargetPath,
            WizardField::TargetKey,
        ]);
        if self.project_type == ProjectType::Branched {
            fields.extend([
                WizardField::AddScope,
                WizardField::RemoveScope,
                WizardField::MoveScopeUp,
                WizardField::MoveScopeDown,
            ]);
        }
        if self.integration_mode.requires_repo() {
            fields.push(WizardField::RepoRoot);
        }
        if self.integration_mode.requires_remote() {
            fields.push(WizardField::RemoteUrl);
        }
        fields.extend([WizardField::Validate, WizardField::Save, WizardField::Cancel]);
        fields
    }

    fn body_fields(&self) -> Vec<WizardField> {
        self.visible_fields()
            .into_iter()
            .filter(|field| !matches!(field, WizardField::Validate | WizardField::Save | WizardField::Cancel))
            .collect()
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
            WizardField::ScopeSelection => ("Scope", HitAction::WizardField(field)),
            WizardField::ScopeName => ("Scope name", HitAction::WizardField(field)),
            WizardField::ScopeKind => ("Scope kind", HitAction::WizardField(field)),
            WizardField::VersionScheme => (
                "Version scheme",
                HitAction::WizardField(field),
            ),
            WizardField::UnifiedVersioning => ("Unified versioning", HitAction::WizardField(field)),
            WizardField::IntegrationMode => (
                "Integration",
                HitAction::WizardField(field),
            ),
            WizardField::TargetPath => ("Target path", HitAction::WizardField(field)),
            WizardField::TargetKey => ("Target key", HitAction::WizardField(field)),
            WizardField::AddScope => ("Add scope", HitAction::WizardScopeAction(ScopeAction::Add)),
            WizardField::RemoveScope => ("Remove scope", HitAction::WizardScopeAction(ScopeAction::Remove)),
            WizardField::MoveScopeUp => ("Move scope up", HitAction::WizardScopeAction(ScopeAction::MoveUp)),
            WizardField::MoveScopeDown => ("Move scope down", HitAction::WizardScopeAction(ScopeAction::MoveDown)),
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
            WizardField::ScopeSelection => self.selected_scope_summary(),
            WizardField::ScopeName => self
                .current_scope()
                .map(|scope| scope.name.display_value_with_width(focused, max_width))
                .unwrap_or_else(|| "(no scope)".to_string()),
            WizardField::ScopeKind => self
                .current_scope()
                .map(|scope| format!("< {} >", scope.scope_kind.display_name()))
                .unwrap_or_else(|| format!("< {} >", BranchScopeKind::Branch.display_name())),
            WizardField::VersionScheme => format!("< {} >", self.version_scheme.display_name()),
            WizardField::UnifiedVersioning => {
                if self.project_type == ProjectType::Branched {
                    format!("< {} >", if self.unified_versioning { "Yes" } else { "No" })
                } else {
                    "Always yes for all-in-one projects".to_string()
                }
            }
            WizardField::IntegrationMode => format!("< {} >", self.integration_mode.display_name()),
            WizardField::TargetPath => {
                if self.project_type == ProjectType::Branched {
                    self.current_scope()
                        .map(|scope| scope.target_path.display_value_with_width(focused, max_width))
                        .unwrap_or_default()
                } else {
                    self.target_path.display_value_with_width(focused, max_width)
                }
            }
            WizardField::TargetKey => {
                if self.project_type == ProjectType::Branched {
                    self.current_scope()
                        .map(|scope| {
                            if scope.target_key_custom {
                                scope.target_key.display_value_with_width(focused, max_width)
                            } else {
                                format!("< {} >", scope.target_key.value())
                            }
                        })
                        .unwrap_or_default()
                } else {
                    if self.target_key_custom {
                        self.target_key.display_value_with_width(focused, max_width)
                    } else {
                        format!("< {} >", self.target_key.value())
                    }
                }
            }
            WizardField::AddScope => "Create a new scope draft".to_string(),
            WizardField::RemoveScope => "Drop the selected scope".to_string(),
            WizardField::MoveScopeUp => "Move the selected scope earlier".to_string(),
            WizardField::MoveScopeDown => "Move the selected scope later".to_string(),
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
                if self.project_type == ProjectType::Branched {
                    self.seed_scope_from_primary_target();
                } else {
                    self.copy_selected_scope_to_primary_target();
                }
            }
            WizardField::ScopeSelection => self.move_scope_selection(delta),
            WizardField::ScopeKind => {
                if let Some(scope) = self.current_scope_mut() {
                    scope.scope_kind = rotate_scope_kind(scope.scope_kind, delta);
                }
            }
            WizardField::TargetKey => self.rotate_target_key_preset(delta),
            WizardField::VersionScheme => {
                self.version_scheme = if delta >= 0 {
                    self.version_scheme.next()
                } else {
                    self.version_scheme.previous()
                };
                self.clear_validation_results();
            }
            WizardField::UnifiedVersioning => {
                if self.project_type == ProjectType::Branched {
                    self.unified_versioning = !self.unified_versioning;
                }
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
            WizardField::ScopeName => self.current_scope_mut().map(|scope| &mut scope.name),
            WizardField::TargetPath => {
                if self.project_type == ProjectType::Branched {
                    self.current_scope_mut().map(|scope| &mut scope.target_path)
                } else {
                    Some(&mut self.target_path)
                }
            }
            WizardField::TargetKey => {
                if self.project_type == ProjectType::Branched && self.current_scope().is_some_and(|scope| scope.target_key_custom) {
                    self.current_scope_mut().map(|scope| &mut scope.target_key)
                } else if self.project_type != ProjectType::Branched && self.target_key_custom {
                    Some(&mut self.target_key)
                } else {
                    None
                }
            }
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
            if self.project_type == ProjectType::Branched {
                if let Some(scope) = self.current_scope_mut() {
                    scope.last_probe = None;
                }
            } else {
                self.last_probe = None;
            }
        }
        if self.focus == WizardField::ScopeName {
            if let Some(scope) = self.current_scope_mut() {
                scope.sync_label_if_needed();
            }
        }
        if self.focus == WizardField::TargetPath {
            self.sync_target_key_preset_with_path();
            self.prefill_repo_root_from_target_path();
        }
    }

    fn ensure_focus_visible(&mut self) {
        let fields = self.visible_fields();
        if !fields.contains(&self.focus) {
            self.focus = fields.first().copied().unwrap_or(WizardField::Name);
        }
        let body_fields = self.body_fields();
        self.sync_body_scroll(&body_fields);
    }

    fn refresh_body_window(&mut self, viewport_height: u16) -> (Vec<WizardField>, u16, bool, bool) {
        let body_fields = self.body_fields();
        let row_height = dialog_form_row_height(viewport_height);
        self.viewport_rows = dialog_visible_rows(viewport_height, row_height);
        self.sync_body_scroll(&body_fields);
        let start = self.field_scroll.min(body_fields.len().saturating_sub(self.viewport_rows));
        let end = (start + self.viewport_rows).min(body_fields.len());
        (
            body_fields[start..end].to_vec(),
            row_height,
            start > 0,
            end < body_fields.len(),
        )
    }

    fn scroll_body(&mut self, delta: isize) {
        let body_fields = self.body_fields();
        if body_fields.is_empty() {
            self.field_scroll = 0;
            return;
        }

        let max_scroll = body_fields.len().saturating_sub(self.viewport_rows.max(1));
        let next = (self.field_scroll as isize + delta).clamp(0, max_scroll as isize) as usize;
        self.field_scroll = next;
    }

    fn sync_body_scroll(&mut self, body_fields: &[WizardField]) {
        let visible_rows = self.viewport_rows.max(1);
        clamp_dialog_scroll(
            &mut self.field_scroll,
            body_fields.len(),
            visible_rows,
            body_fields.iter().position(|field| *field == self.focus),
        );
    }

    fn prefill_repo_root_from_target_path(&mut self) {
        if !self.repo_root.is_empty() {
            return;
        }
        let target_path = if self.project_type == ProjectType::Branched {
            self.current_scope().map(|scope| scope.target_path.value()).unwrap_or("")
        } else {
            self.target_path.value()
        };
        if let Some(repo_root) = derive_repo_root_from_target_path(target_path) {
            self.repo_root.set_value(repo_root);
        }
    }

    fn set_target_path_from_browse(&mut self, path: String) {
        if self.project_type == ProjectType::Branched {
            if let Some(scope) = self.current_scope_mut() {
                scope.target_path.set_value(path);
                if !scope.target_key_custom {
                    scope.target_key.set_value(default_target_key_for_path(scope.target_path.value()));
                }
                scope.last_probe = None;
            }
        } else {
            self.target_path.set_value(path);
            if !self.target_key_custom {
                self.target_key.set_value(default_target_key_for_path(self.target_path.value()));
            }
            self.last_probe = None;
        }
        self.prefill_repo_root_from_target_path();
    }

    fn target_key_accepts_text(&self) -> bool {
        if self.project_type == ProjectType::Branched {
            self.current_scope().is_some_and(|scope| scope.target_key_custom)
        } else {
            self.target_key_custom
        }
    }

    fn enable_custom_target_key(&mut self) {
        if self.project_type == ProjectType::Branched {
            if let Some(scope) = self.current_scope_mut() {
                scope.target_key_custom = true;
            }
        } else {
            self.target_key_custom = true;
        }
        self.focus = WizardField::TargetKey;
    }

    fn rotate_target_key_preset(&mut self, delta: i32) {
        if self.project_type == ProjectType::Branched {
            if let Some(scope) = self.current_scope_mut() {
                let next = cycle_target_key_preset(scope.target_path.value(), scope.target_key.value(), delta);
                scope.target_key.set_value(next);
                scope.target_key_custom = false;
            }
        } else {
            let next = cycle_target_key_preset(self.target_path.value(), self.target_key.value(), delta);
            self.target_key.set_value(next);
            self.target_key_custom = false;
        }
    }

    fn sync_target_key_preset_with_path(&mut self) {
        if self.project_type == ProjectType::Branched {
            if let Some(scope) = self.current_scope_mut() {
                if !scope.target_key_custom {
                    scope.target_key.set_value(default_target_key_for_path(scope.target_path.value()));
                }
            }
        } else if !self.target_key_custom {
            self.target_key.set_value(default_target_key_for_path(self.target_path.value()));
        }
    }

    fn set_repo_root_from_browse(&mut self, path: String) {
        self.repo_root.set_value(path);
    }

    fn build_project(&self) -> Result<ProjectConfig> {
        if self.name.value.trim().is_empty() {
            bail!("project name is required");
        }

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
            ProjectConfig {
                name: self.name.value.trim().to_string(),
                project_type: ProjectType::Branched,
                integration_mode: self.integration_mode,
                unified_versioning: self.unified_versioning,
                version_scheme: self.version_scheme,
                targets: Vec::new(),
                branches: self.build_branches(true)?,
                repo,
            }
        };

        Ok(project)
    }

    fn current_scope(&self) -> Option<&ScopeDraft> {
        self.scopes.get(self.selected_scope)
    }

    fn current_scope_mut(&mut self) -> Option<&mut ScopeDraft> {
        self.scopes.get_mut(self.selected_scope)
    }

    fn selected_scope_summary(&self) -> String {
        let total = self.scopes.len();
        if total == 0 {
            "< no scopes >".to_string()
        } else {
            let summary = self
                .current_scope()
                .map(|scope| scope.display_name())
                .unwrap_or_else(|| "(unknown)".to_string());
            format!("< {}/{}: {} >", self.selected_scope + 1, total, summary)
        }
    }

    fn move_scope_selection(&mut self, delta: i32) {
        if self.scopes.is_empty() {
            return;
        }
        let len = self.scopes.len() as i32;
        let next = (self.selected_scope as i32 + delta).rem_euclid(len) as usize;
        self.selected_scope = next;
    }

    fn next_scope_name(&self) -> String {
        let mut index = self.scopes.len() + 1;
        loop {
            let candidate = format!("scope-{}", index);
            if self
                .scopes
                .iter()
                .all(|scope| !scope.name.value.trim().eq_ignore_ascii_case(&candidate))
            {
                return candidate;
            }
            index += 1;
        }
    }

    fn add_scope(&mut self) {
        self.scopes.push(ScopeDraft::new(self.next_scope_name()));
        self.selected_scope = self.scopes.len().saturating_sub(1);
        self.focus = WizardField::ScopeName;
    }

    fn remove_selected_scope(&mut self) -> Result<()> {
        if self.scopes.len() <= 1 {
            bail!("branched projects need at least one scope");
        }
        self.scopes.remove(self.selected_scope);
        self.selected_scope = self.selected_scope.min(self.scopes.len().saturating_sub(1));
        self.focus = WizardField::ScopeSelection;
        Ok(())
    }

    fn move_selected_scope(&mut self, delta: isize) {
        if self.scopes.len() < 2 {
            return;
        }
        let len = self.scopes.len() as isize;
        let next = (self.selected_scope as isize + delta).clamp(0, len - 1) as usize;
        if next != self.selected_scope {
            self.scopes.swap(self.selected_scope, next);
            self.selected_scope = next;
        }
    }

    fn clear_validation_results(&mut self) {
        self.last_probe = None;
        for scope in &mut self.scopes {
            scope.last_probe = None;
        }
    }

    fn seed_scope_from_primary_target(&mut self) {
        if self.scopes.is_empty() {
            self.scopes.push(ScopeDraft::new("core"));
            self.selected_scope = 0;
        }
        let target_path = self.target_path.value.trim().to_string();
        let target_key = self.target_key.value.trim().to_string();
        let target_key_custom = self.target_key_custom;
        let target_format = self.last_probe.as_ref().and_then(|probe| probe.format).unwrap_or(TargetFormat::Auto);
        if let Some(scope) = self.current_scope_mut() {
            if scope.target_path.value.trim().is_empty() && !target_path.is_empty() {
                scope.target_path.set_value(target_path);
                scope.format = target_format;
            }
            if scope.target_key.value.trim().is_empty() && !target_key.is_empty() {
                scope.target_key.set_value(target_key);
                scope.target_key_custom = target_key_custom;
            }
        }
    }

    fn copy_selected_scope_to_primary_target(&mut self) {
        let selected = self.current_scope().map(|scope| {
            (
                scope.target_path.value().to_string(),
                scope.target_key.value().to_string(),
                scope.target_key_custom,
                scope.last_probe.clone(),
            )
        });
        if let Some((target_path, target_key, target_key_custom, probe)) = selected {
            self.target_path.set_value(target_path);
            self.target_key.set_value(target_key);
            self.target_key_custom = target_key_custom;
            self.last_probe = probe;
        }
    }

    fn build_branches(&self, require_probe: bool) -> Result<Vec<BranchConfig>> {
        if self.scopes.is_empty() {
            bail!("branched projects need at least one scope");
        }

        let mut names = HashSet::new();
        let mut branches = Vec::with_capacity(self.scopes.len());
        for scope in &self.scopes {
            let branch = scope.build_branch(self.version_scheme, require_probe)?;
            let key = branch.name.trim().to_ascii_lowercase();
            if !names.insert(key) {
                bail!("scope names must be unique");
            }
            branches.push(branch);
        }
        Ok(branches)
    }
}

fn sanitize_pasted_text(text: &str) -> String {
    text.chars().filter(|character| *character != '\r' && *character != '\n').collect()
}

#[derive(Clone)]
struct DialogButton {
    label: String,
    focused: bool,
    action: HitAction,
    style: Style,
}

impl DialogButton {
    fn new(label: impl Into<String>, focused: bool, action: HitAction, style: Style) -> Self {
        Self {
            label: label.into(),
            focused,
            action,
            style,
        }
    }
}

#[derive(Clone)]
struct FormRowButton {
    label: &'static str,
    action: HitAction,
}

impl FormRowButton {
    fn new(label: &'static str, action: HitAction) -> Self {
        Self { label, action }
    }
}

struct TagAnnotationDialog {
    editor: TuiTextArea<'static>,
    placeholder: String,
}

impl TagAnnotationDialog {
    fn new(existing_annotation: &str) -> Self {
        let mut editor = if existing_annotation.trim().is_empty() {
            TuiTextArea::default()
        } else {
            TuiTextArea::from(existing_annotation.lines())
        };
        editor.set_placeholder_text("Optional multi-line tag annotation");
        editor.set_tab_length(2);
        editor.set_max_histories(100);
        Self {
            editor,
            placeholder: "Optional multi-line tag annotation".to_string(),
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

fn git_graph_base_column(lines: &[String]) -> usize {
    lines
        .iter()
        .flat_map(|line| line.char_indices())
        .filter_map(|(index, character)| matches!(character, '*' | '|').then_some(index))
        .min()
        .unwrap_or(0)
}

fn colorize_git_log_line(line: &str, graph_base_column: usize) -> Line<'static> {
    let Some((hash_start, hash_end)) = find_commit_hash_range(line) else {
        return Line::from(line.to_string());
    };

    let prefix = &line[..hash_start];
    let hash = &line[hash_start..hash_end];
    let suffix = &line[hash_end..];
    let hash_color = git_hash_color(prefix, graph_base_column).unwrap_or(Color::White);
    let mut spans = Vec::new();

    for (index, character) in prefix.chars().enumerate() {
        if is_git_graph_character(character) {
            spans.push(Span::styled(
                character.to_string(),
                Style::default()
                    .fg(git_branch_color(index.saturating_sub(graph_base_column) / 2))
                    .add_modifier(Modifier::BOLD),
            ));
        } else {
            spans.push(Span::raw(character.to_string()));
        }
    }

    spans.push(Span::styled(
        hash.to_string(),
        Style::default().fg(hash_color).add_modifier(Modifier::BOLD),
    ));
    if !suffix.is_empty() {
        spans.push(Span::raw(suffix.to_string()));
    }

    Line::from(spans)
}

fn git_hash_color(prefix: &str, graph_base_column: usize) -> Option<Color> {
    prefix
        .chars()
        .enumerate()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .find_map(|(index, character)| {
            is_git_graph_character(character)
                .then_some(git_branch_color(index.saturating_sub(graph_base_column) / 2))
        })
}

fn git_branch_color(slot: usize) -> Color {
    GIT_BRANCH_COLORS[slot % GIT_BRANCH_COLORS.len()]
}

fn is_git_graph_character(character: char) -> bool {
    matches!(character, '|' | '\\' | ',' | '/' | '*')
}

fn find_commit_hash_range(line: &str) -> Option<(usize, usize)> {
    let indices = line.char_indices().collect::<Vec<_>>();
    for (position, (byte_index, character)) in indices.iter().enumerate() {
        if !character.is_ascii_hexdigit() {
            continue;
        }

        let previous_is_space = position == 0 || indices[position - 1].1 == ' ';
        if !previous_is_space {
            continue;
        }

        let mut end = position;
        while end < indices.len() && indices[end].1.is_ascii_hexdigit() {
            end += 1;
        }

        if end - position < 7 {
            continue;
        }

        let next_is_space = end == indices.len() || indices[end].1 == ' ';
        if !next_is_space {
            continue;
        }

        let end_byte = if end < indices.len() { indices[end].0 } else { line.len() };
        return Some((*byte_index, end_byte));
    }

    None
}

fn convert_to_textarea_input(key: KeyEvent) -> Option<TextAreaInput> {
    let text_key = match key.code {
        KeyCode::Backspace => TextAreaKey::Backspace,
        KeyCode::Enter => TextAreaKey::Enter,
        KeyCode::Left => TextAreaKey::Left,
        KeyCode::Right => TextAreaKey::Right,
        KeyCode::Up => TextAreaKey::Up,
        KeyCode::Down => TextAreaKey::Down,
        KeyCode::Home => TextAreaKey::Home,
        KeyCode::End => TextAreaKey::End,
        KeyCode::PageUp => TextAreaKey::PageUp,
        KeyCode::PageDown => TextAreaKey::PageDown,
        KeyCode::Tab | KeyCode::BackTab => TextAreaKey::Tab,
        KeyCode::Delete => TextAreaKey::Delete,
        KeyCode::Esc => TextAreaKey::Esc,
        KeyCode::Char(character) => TextAreaKey::Char(character),
        _ => return None,
    };

    Some(TextAreaInput {
        key: text_key,
        ctrl: key.modifiers.contains(KeyModifiers::CONTROL),
        alt: key.modifiers.contains(KeyModifiers::ALT),
        shift: key.modifiers.contains(KeyModifiers::SHIFT),
    })
}

fn render_annotation_line(
    line: &str,
    line_number: usize,
    number_width: usize,
    content_width: usize,
    active_cursor_col: Option<usize>,
) -> Line<'static> {
    let mut spans = vec![Span::styled(
        format!("{:>width$} ", line_number, width = number_width),
        Style::default().fg(Color::DarkGray),
    )];

    let (visible_text, visible_cursor_col) = annotation_visible_segment(line, active_cursor_col.unwrap_or(0), content_width);
    if active_cursor_col.is_some() {
        let chars = visible_text.chars().collect::<Vec<_>>();
        let highlight_index = visible_cursor_col.min(content_width.saturating_sub(1));
        for (index, character) in chars.iter().enumerate() {
            let style = if index == highlight_index {
                Style::default().fg(Color::Black).bg(Color::Cyan)
            } else if active_cursor_col.is_some() {
                Style::default().bg(Color::Rgb(35, 45, 60))
            } else {
                Style::default()
            };
            spans.push(Span::styled(character.to_string(), style));
        }

        if chars.is_empty() {
            spans.push(Span::styled(" ".to_string(), Style::default().fg(Color::Black).bg(Color::Cyan)));
        } else if visible_cursor_col >= chars.len() && chars.len() < content_width {
            spans.push(Span::styled(" ".to_string(), Style::default().fg(Color::Black).bg(Color::Cyan)));
        }
    } else {
        spans.push(Span::raw(visible_text));
    }

    Line::from(spans)
}

fn annotation_visible_segment(line: &str, cursor_col: usize, width: usize) -> (String, usize) {
    let characters = line.chars().collect::<Vec<_>>();
    if width == 0 {
        return (String::new(), 0);
    }

    let start = cursor_col.saturating_sub(width.saturating_sub(1)).min(characters.len().saturating_sub(width));
    let end = (start + width).min(characters.len());
    let visible = characters[start..end].iter().collect::<String>();
    (visible, cursor_col.saturating_sub(start))
}

fn dialog_form_row_height(viewport_height: u16) -> u16 {
    if viewport_height >= 8 {
        3
    } else if viewport_height >= 4 {
        2
    } else {
        1
    }
}

fn dialog_visible_rows(viewport_height: u16, row_height: u16) -> usize {
    (viewport_height / row_height.max(1)).max(1) as usize
}

fn clamp_dialog_scroll(scroll_offset: &mut usize, total_rows: usize, visible_rows: usize, focus_index: Option<usize>) {
    let visible_rows = visible_rows.max(1);
    let max_scroll = total_rows.saturating_sub(visible_rows);
    *scroll_offset = (*scroll_offset).min(max_scroll);

    if let Some(focus_index) = focus_index {
        if focus_index < *scroll_offset {
            *scroll_offset = focus_index;
        } else if focus_index >= *scroll_offset + visible_rows {
            *scroll_offset = focus_index + 1 - visible_rows;
        }
    }
}

fn render_vertical_overflow_indicators(frame: &mut Frame, area: Rect, show_above: bool, show_below: bool) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let indicator_width = area.width.min(5);
    if show_above {
        let top_rect = Rect {
            x: area.x + area.width.saturating_sub(indicator_width),
            y: area.y,
            width: indicator_width,
            height: 1,
        };
        frame.render_widget(
            Paragraph::new("↑↑↑")
                .alignment(Alignment::Right)
                .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            top_rect,
        );
    }

    if show_below {
        let bottom_rect = Rect {
            x: area.x + area.width.saturating_sub(indicator_width),
            y: area.y + area.height.saturating_sub(1),
            width: indicator_width,
            height: 1,
        };
        frame.render_widget(
            Paragraph::new("↓↓↓")
                .alignment(Alignment::Right)
                .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            bottom_rect,
        );
    }
}

fn rotate_scope_kind(scope_kind: BranchScopeKind, delta: i32) -> BranchScopeKind {
    if delta >= 0 {
        match scope_kind {
            BranchScopeKind::Branch => BranchScopeKind::Module,
            BranchScopeKind::Module => BranchScopeKind::Service,
            BranchScopeKind::Service => BranchScopeKind::Branch,
        }
    } else {
        match scope_kind {
            BranchScopeKind::Branch => BranchScopeKind::Service,
            BranchScopeKind::Module => BranchScopeKind::Branch,
            BranchScopeKind::Service => BranchScopeKind::Module,
        }
    }
}

fn target_key_presets(path: &str) -> [&'static str; 3] {
    if path.trim().to_ascii_lowercase().ends_with(".toml") {
        ["package.version", "workspace.package.version", "version"]
    } else {
        ["version", "package.version", "workspace.package.version"]
    }
}

fn default_target_key_for_path(path: &str) -> &'static str {
    target_key_presets(path)[0]
}

fn target_key_is_custom(path: &str, value: &str) -> bool {
    !target_key_presets(path)
        .into_iter()
        .any(|preset| preset == value.trim())
}

fn cycle_target_key_preset(path: &str, current: &str, delta: i32) -> String {
    let presets = target_key_presets(path);
    let current_index = presets
        .iter()
        .position(|preset| *preset == current.trim())
        .unwrap_or(0) as i32;
    let next_index = (current_index + if delta >= 0 { 1 } else { -1 })
        .rem_euclid(presets.len() as i32) as usize;
    presets[next_index].to_string()
}

fn wizard_form_row_button(field: WizardField) -> Option<FormRowButton> {
    match field {
        WizardField::TargetPath => Some(FormRowButton::new("Browse", HitAction::BrowseWizardTargetPath)),
        WizardField::TargetKey => Some(FormRowButton::new("Custom", HitAction::EnableWizardCustomTargetKey)),
        WizardField::RepoRoot => Some(FormRowButton::new("Browse", HitAction::BrowseWizardRepoRoot)),
        _ => None,
    }
}

fn project_edit_form_row_button(field: ProjectEditFocus) -> Option<FormRowButton> {
    match field {
        ProjectEditFocus::TargetPath => Some(FormRowButton::new("Browse", HitAction::BrowseProjectTargetPath)),
        ProjectEditFocus::TargetKey => Some(FormRowButton::new("Custom", HitAction::EnableProjectCustomTargetKey)),
        ProjectEditFocus::RepoRoot => Some(FormRowButton::new("Browse", HitAction::BrowseProjectRepoRoot)),
        _ => None,
    }
}

fn dashboard_tile_columns(width: u16) -> usize {
    ((width + 1) / (TILE_WIDTH + 1)).max(1) as usize
}

fn rect_contains(rect: Rect, column: u16, row: u16) -> bool {
    column >= rect.x && column < rect.x + rect.width && row >= rect.y && row < rect.y + rect.height
}

fn adjust_pending_version_value(
    scheme: VersionScheme,
    current: &str,
    control: OverviewVersionControl,
    delta: i32,
) -> Result<String> {
    match scheme {
        VersionScheme::SemVer => adjust_semver_overview_value(current, control, delta),
        _ => adjust_numeric_tail_overview_value(current, delta),
    }
}

fn adjust_semver_overview_value(current: &str, control: OverviewVersionControl, delta: i32) -> Result<String> {
    let mut parts = current
        .split('.')
        .map(|part| part.parse::<i32>().map_err(|_| anyhow!("invalid semver component '{}'", part)))
        .collect::<Result<Vec<_>>>()?;
    if parts.len() != 3 {
        bail!("overview semver editing requires MAJOR.MINOR.PATCH");
    }

    let index = match control {
        OverviewVersionControl::Major => 0,
        OverviewVersionControl::Minor => 1,
        OverviewVersionControl::Patch | OverviewVersionControl::Whole => 2,
    };
    parts[index] = (parts[index] + delta).max(0);
    Ok(format!("{}.{}.{}", parts[0], parts[1], parts[2]))
}

fn adjust_numeric_tail_overview_value(current: &str, delta: i32) -> Result<String> {
    let mut parts = current
        .split('.')
        .map(|part| part.parse::<i32>().map_err(|_| anyhow!("invalid numeric component '{}'", part)))
        .collect::<Result<Vec<_>>>()?;
    let last = parts.last_mut().ok_or_else(|| anyhow!("overview version is empty"))?;
    *last = (*last + delta).max(0);
    Ok(parts.into_iter().map(|part| part.to_string()).collect::<Vec<_>>().join("."))
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

fn header_height_for_viewport(total_height: u16) -> u16 {
    if total_height < 40 { 6 } else { 12 }
}

fn should_use_recent_changes_tab(area_height: u16, max_tile_height: u16) -> bool {
    area_height < max_tile_height.saturating_add(1).saturating_add(8)
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
    fn compact_viewports_use_short_header() {
        assert_eq!(header_height_for_viewport(39), 6);
        assert_eq!(header_height_for_viewport(40), 12);
    }

    #[test]
    fn recent_changes_tab_appears_when_vertical_space_is_tight() {
        assert!(should_use_recent_changes_tab(15, 7));
        assert!(!should_use_recent_changes_tab(20, 7));
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

    #[test]
    fn branched_wizard_builds_multiple_scopes() {
        let mut wizard = ProjectWizard::default();
        wizard.project_type = ProjectType::Branched;
        wizard.name.set_value("demo-service");

        {
            let scope = wizard.current_scope_mut().expect("default scope");
            scope.name.set_value("core");
            scope.target_path.set_value("C:/repo/core/Cargo.toml");
            scope.target_key.set_value("package.version");
            scope.last_probe = Some(TargetProbe {
                kind: ProbeKind::Success,
                message: "ok".to_string(),
                version: Some("1.2.3".to_string()),
                format: Some(TargetFormat::Toml),
            });
        }

        wizard.add_scope();
        {
            let scope = wizard.current_scope_mut().expect("second scope");
            scope.name.set_value("api");
            scope.target_path.set_value("C:/repo/api/package.json");
            scope.target_key.set_value("version");
            scope.scope_kind = BranchScopeKind::Service;
            scope.last_probe = Some(TargetProbe {
                kind: ProbeKind::Success,
                message: "ok".to_string(),
                version: Some("1.2.3".to_string()),
                format: Some(TargetFormat::Json),
            });
        }

        let project = wizard.build_project().expect("branched project should build");

        assert_eq!(project.project_type, ProjectType::Branched);
        assert!(!project.unified_versioning);
        assert_eq!(project.branches.len(), 2);
        assert_eq!(project.branches[0].name, "core");
        assert_eq!(project.branches[1].name, "api");
        assert_eq!(project.branches[1].scope_kind, BranchScopeKind::Service);
        assert_eq!(project.branches[1].targets[0].format, TargetFormat::Json);
    }

    #[test]
    fn branched_wizard_rejects_duplicate_scope_names() {
        let mut wizard = ProjectWizard::default();
        wizard.project_type = ProjectType::Branched;
        wizard.name.set_value("demo-service");

        {
            let scope = wizard.current_scope_mut().expect("default scope");
            scope.name.set_value("core");
            scope.target_path.set_value("C:/repo/core/Cargo.toml");
            scope.target_key.set_value("package.version");
            scope.last_probe = Some(TargetProbe {
                kind: ProbeKind::Success,
                message: "ok".to_string(),
                version: Some("1.2.3".to_string()),
                format: Some(TargetFormat::Toml),
            });
        }

        wizard.add_scope();
        {
            let scope = wizard.current_scope_mut().expect("second scope");
            scope.name.set_value("core");
            scope.target_path.set_value("C:/repo/api/package.json");
            scope.target_key.set_value("version");
            scope.last_probe = Some(TargetProbe {
                kind: ProbeKind::Success,
                message: "ok".to_string(),
                version: Some("1.2.3".to_string()),
                format: Some(TargetFormat::Json),
            });
        }

        let error = wizard.build_project().expect_err("duplicate scope names should fail");
        assert!(error.to_string().contains("unique"));
    }

    #[test]
    fn wizard_body_window_keeps_focused_field_visible_when_viewport_is_short() {
        let mut wizard = ProjectWizard::default();
        wizard.project_type = ProjectType::Branched;
        wizard.integration_mode = IntegrationMode::GitHubEnabled;
        wizard.focus = WizardField::RemoteUrl;

        let (visible_fields, row_height, show_above, show_below) = wizard.refresh_body_window(6);

        assert_eq!(row_height, 2);
        assert!(visible_fields.contains(&WizardField::RemoteUrl));
        assert!(show_above);
        assert!(!show_below);
    }

    #[test]
    fn target_key_switches_to_toml_default_when_target_path_changes() {
        let mut wizard = ProjectWizard::default();
        wizard.focus = WizardField::TargetPath;

        wizard.insert_text("C:/repo/Cargo.toml");

        assert_eq!(wizard.target_key.value(), "package.version");
        assert!(!wizard.target_key_custom);
    }

    #[test]
    fn cargo_lock_is_staged_for_relative_cargo_manifest_targets() {
        let unique = format!(
            "cvb-stage-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        let repo_root = std::env::temp_dir().join(unique);
        let crate_dir = repo_root.join("core");
        std::fs::create_dir_all(&crate_dir).expect("crate dir");
        std::fs::write(crate_dir.join("Cargo.toml"), "[package]\nname='demo'\nversion='1.2.3'\n").expect("manifest");
        std::fs::write(crate_dir.join("Cargo.lock"), "# lock\n").expect("lockfile");

        let targets = vec![BumpTarget {
            label: "Version".to_string(),
            path: "core/Cargo.toml".to_string(),
            key_path: "package.version".to_string(),
            format: TargetFormat::Toml,
            current_version: "1.2.3".to_string(),
        }];

        let staged = collect_stage_paths_for_targets(&repo_root.display().to_string(), &targets);

        assert_eq!(staged, vec!["core/Cargo.toml".to_string(), "core/Cargo.lock".to_string()]);

        let _ = std::fs::remove_dir_all(repo_root);
    }

    #[test]
    fn custom_target_key_mode_enables_text_entry() {
        let mut wizard = ProjectWizard::default();
        wizard.focus = WizardField::TargetKey;

        assert!(!wizard.focus_accepts_text());

        wizard.enable_custom_target_key();

        assert!(wizard.target_key_custom);
        assert!(wizard.focus_accepts_text());
    }

    #[test]
    fn overview_semver_adjustment_supports_increment_and_decrement() {
        let incremented = adjust_pending_version_value(
            VersionScheme::SemVer,
            "1.2.3",
            OverviewVersionControl::Minor,
            1,
        )
        .expect("increment should succeed");
        let decremented = adjust_pending_version_value(
            VersionScheme::SemVer,
            "1.2.3",
            OverviewVersionControl::Patch,
            -1,
        )
        .expect("decrement should succeed");

        assert_eq!(incremented, "1.3.3");
        assert_eq!(decremented, "1.2.2");
    }
}
