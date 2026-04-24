// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit License v1.2
//
// For details, see the LICENSE file in the repository root.

use std::{
    collections::HashSet,
    fs, io,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

#[cfg(windows)]
use std::os::windows::io::AsRawHandle;

use anyhow::{Context, Result, anyhow, bail};
use arboard::Clipboard;
use crossterm::{
    event::{
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent,
        MouseEventKind,
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
use tokio::{
    runtime::{Builder as TokioRuntimeBuilder, Runtime as TokioRuntime},
    sync::{
        Semaphore,
        mpsc::{UnboundedReceiver, UnboundedSender, error::TryRecvError, unbounded_channel},
    },
    task::JoinSet,
    time::sleep,
};
use tui_tabs::TabNav;
use tui_textarea::{Input as TextAreaInput, Key as TextAreaKey, TextArea as TuiTextArea};

#[cfg(windows)]
use windows_sys::Win32::System::Console::{
    ENABLE_ECHO_INPUT, ENABLE_EXTENDED_FLAGS, ENABLE_LINE_INPUT, ENABLE_PROCESSED_INPUT,
    ENABLE_VIRTUAL_TERMINAL_INPUT, GetConsoleMode, SetConsoleMode,
};

use crate::{
    branding::{PixelLogo, choose_header_content},
    changelog::{
        ChangelogDocument, archive_changelog_markdown, ensure_previous_public_release_header,
        find_archived_changelog_markdown, rebuild_history_summary_readme, rls_changelog_gen,
        std_changelog_gen, write_changelog_markdown, write_temp_changelog_markdown,
    },
    cli::{
        CommitRenamePlan, prepare_commit_rename, push_branch_force_with_lease,
        rename_commit_with_subject,
    },
    config::{
        AppConfig, BranchConfig, BranchScopeKind, ConfigStore, FooterContent, IntegrationMode,
        ProjectConfig, ProjectType, RepoConfig, TargetFormat, TargetSpec,
    },
    dialogs::{
        BumpDialog, ChangeRange, RecentChangesDialog, RecentChangesTab, TagAction, TagDialog,
        TextInput, load_change_range_for_refs_with_cancel, load_change_range_for_tags_with_cancel,
        load_history_ranges_with_cancel, load_recent_change_range_with_cancel,
    },
    git::{
        GitCancellation, RepoActivitySummary, branches_containing_ref_with_cancel,
        collect_all_branch_git_scope_contexts, current_branch_with_cancel, ensure_gh_available,
        ensure_local_tag, is_mainline_branch_name, latest_local_tag_with_cancel,
        load_scope_activity_summary_with_cancel, run_git, run_git_checked,
        sorted_local_tags_with_cancel, split_output_lines,
    },
    git_br::BranchNameOption,
    mmr::{
        load_merged_std_changelog_memory, record_std_changelog_created, record_std_changelog_error,
        record_std_changelog_generated, record_std_changelog_postponed,
    },
    overview_pg::{OverviewTab, overview_tab_rects, overview_tabs, render_overview_tabs},
    project_edit::{ProjectEditDialog, ProjectEditFocus},
    project_wizard::{ProjectWizard, WizardField},
    targets::{
        BumpScope, BumpTarget, ProbeKind, TargetProbe, collect_bump_scopes, probe_target,
        write_target_version,
    },
    tiles::{OverviewTileData, TILE_WIDTH, render_overview_tile, tile_height},
    ui::{center_vertically, centered_rect},
    versioning::{BumpAction, VersionScheme},
};

#[path = "git_flow.rs"]
pub(crate) mod git_flow;
#[path = "overview.rs"]
mod overview;
#[path = "p-s-s.rs"]
mod p_s_s;
#[path = "render.rs"]
mod render;
#[path = "rls-now.rs"]
mod rls_now;

use self::p_s_s::{ProjectSettingsFocus, ProjectSettingsState, ProjectSettingsTab};

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
const SUPPORT_EMAIL: &str = " dev@comfyhome.io ";
const FORM_LABEL_WIDTH: u16 = 18;
const BROWSE_BUTTON_WIDTH: u16 = 12;
const BUTTON_ROW_HEIGHT: u16 = 3;
const BUTTON_GAP_HEIGHT: u16 = 3;
const SHORTCUT_HINT_COLOR: Color = Color::Yellow;
const ACTIVE_UI_TICK_INTERVAL: Duration = Duration::from_millis(100);
const IDLE_UI_POLL_INTERVAL: Duration = Duration::from_secs(1);
const BACKGROUND_MAX_PARALLEL_REPO_JOBS: usize = 4;
const NETWORK_RETRY_ATTEMPTS: usize = 2;
const NETWORK_RETRY_DELAY: Duration = Duration::from_millis(750);
const GIT_PUSH_TIMEOUT: Duration = Duration::from_secs(20);
const GH_RELEASE_TIMEOUT: Duration = Duration::from_secs(45);
const GIT_BRANCH_COLORS: [Color; 6] = [
    Color::Green,
    Color::Cyan,
    Color::Yellow,
    Color::Magenta,
    Color::Blue,
    Color::Red,
];

pub fn run() -> Result<()> {
    restore_console_input_mode();
    let mut terminal = setup_terminal()?;
    let result = run_app(&mut terminal);
    let restore_result = restore_terminal(&mut terminal);
    restore_console_input_mode();
    restore_result?;
    result
}

fn restore_console_input_mode() {
    #[cfg(windows)]
    {
        let handle = io::stdin().as_raw_handle();
        if handle.is_null() {
            return;
        }

        let mut mode = 0;
        let stdin = handle as windows_sys::Win32::Foundation::HANDLE;

        unsafe {
            if GetConsoleMode(stdin, &mut mode) == 0 {
                return;
            }

            let restored_mode = (mode
                | ENABLE_PROCESSED_INPUT
                | ENABLE_LINE_INPUT
                | ENABLE_ECHO_INPUT
                | ENABLE_EXTENDED_FLAGS)
                & !ENABLE_VIRTUAL_TERMINAL_INPUT;

            let _ = SetConsoleMode(stdin, restored_mode);
        }
    }
}

fn setup_terminal() -> Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode().context("failed to enable raw mode")?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableBracketedPaste
    )
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
    terminal
        .show_cursor()
        .context("failed to show the cursor")?;
    Ok(())
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    let mut app = App::new()?;
    app.prime_selected_project_dashboard_data();
    let mut needs_draw = true;

    while !app.should_quit {
        if needs_draw {
            terminal.draw(|frame| app.draw(frame))?;
            needs_draw = false;
        }

        match app.try_finish_background_job() {
            Ok(true) => {
                needs_draw = true;
                continue;
            }
            Ok(false) => {}
            Err(error) => {
                app.status = StatusMessage::error(error.to_string());
                needs_draw = true;
                continue;
            }
        }

        if event::poll(app.next_poll_timeout()).context("event polling failed")? {
            match event::read().context("event read failed")? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    if let Err(error) = app.handle_key(key) {
                        app.status = StatusMessage::error(error.to_string());
                    }
                    needs_draw = true;
                }
                Event::Mouse(mouse) => {
                    app.handle_mouse(mouse);
                    needs_draw = true;
                }
                Event::Paste(text) => {
                    app.handle_paste(text);
                    needs_draw = true;
                }
                Event::Resize(_, _) => needs_draw = true,
                Event::FocusGained | Event::FocusLost => {}
                Event::Key(_) => {}
            }
        } else if app.tick_ui_state() {
            needs_draw = true;
        }
    }

    Ok(())
}

struct App {
    config_store: ConfigStore,
    config: AppConfig,
    screen: Screen,
    selected_project: usize,
    dashboard_focus: DashboardPane,
    overview_tab: OverviewTab,
    overview_show_recent_tab: bool,
    project_settings_tab: ProjectSettingsTab,
    project_settings_state: ProjectSettingsState,
    overview_focused_scope: usize,
    overview_recent_changes: Option<RecentChangesDialog>,
    overview_recent_project: Option<usize>,
    overview_recent_error: Option<String>,
    overview_tile_project: Option<usize>,
    overview_activity_project: Option<usize>,
    overview_activity_summaries: Vec<Option<RepoActivitySummary>>,
    overview_scope_order: Vec<usize>,
    overview_pending_versions: Vec<String>,
    overview_tile_dev_modes: Vec<usize>,
    overview_tile_rls_modes: Vec<usize>,
    overview_tile_last_rotation_at: Instant,
    overview_tile_scroll: usize,
    overview_tile_viewport: Option<Rect>,
    overview_recent_viewport: Option<Rect>,
    release_now_log_viewport: Option<Rect>,
    overview_tile_rects: Vec<(Rect, usize)>,
    overview_drag_scope: Option<usize>,
    wizard: ProjectWizard,
    bump_dialog: Option<BumpDialog>,
    overview_bump_kind_dialog: Option<OverviewBumpKindDialog>,
    overview_bump_workflow_dialog: Option<OverviewBumpWorkflowDialog>,
    overview_branch_bump_dialog: Option<OverviewBranchBumpDialog>,
    overview_bump_warning_dialog: Option<OverviewBumpWarningDialog>,
    main_branch_warning_dialog: Option<MainBranchWarningDialog>,
    std_changelog_sub_branch_dialog: Option<StdChangelogSubBranchDialog>,
    changelog_preview_dialog: Option<ChangelogPreviewDialog>,
    recent_changes_dialog: Option<RecentChangesDialog>,
    commit_rename_dialog: Option<CommitRenameDialog>,
    tag_dialog: Option<TagDialog>,
    tag_annotation_dialog: Option<TagAnnotationDialog>,
    release_now_dialog: Option<rls_now::ReleaseNowDialog>,
    release_now_notes_dialog: Option<TagAnnotationDialog>,
    delete_confirmation_dialog: Option<DeleteConfirmationDialog>,
    progress_dialog: Option<ProgressDialog>,
    foreground_request_tx: UnboundedSender<BackgroundJobRequestMessage>,
    refresh_request_tx: UnboundedSender<BackgroundJobRequestMessage>,
    prefetch_request_tx: UnboundedSender<BackgroundJobRequestMessage>,
    background_result_rx: UnboundedReceiver<BackgroundJobResultMessage>,
    _background_runtime: TokioRuntime,
    background_job_active: bool,
    background_jobs_inflight: usize,
    next_background_job_id: u64,
    active_foreground_job_id: Option<u64>,
    current_recent_changes_job_id: Option<u64>,
    current_recent_changes_prefetch_job_id: Option<u64>,
    current_changelog_preview_job_id: Option<u64>,
    current_overview_activity_job_id: Option<u64>,
    current_release_now_job_id: Option<u64>,
    overview_activity_job_inflight: bool,
    overview_activity_refresh_inflight: bool,
    overview_activity_refresh_pending: bool,
    current_recent_changes_cancel: Option<GitCancellation>,
    current_recent_changes_prefetch_cancel: Option<GitCancellation>,
    current_changelog_preview_cancel: Option<GitCancellation>,
    current_overview_activity_cancel: Option<GitCancellation>,
    current_release_now_cancel: Option<GitCancellation>,
    project_edit_dialog: Option<ProjectEditDialog>,
    browser_dialog: Option<FileBrowserDialog>,
    hit_targets: Vec<HitTarget>,
    last_text_input_click_target: Option<TextInputClickTarget>,
    last_text_input_click_at: Option<Instant>,
    last_recent_change_click_target: Option<RecentChangeClickTarget>,
    last_recent_change_click_at: Option<Instant>,
    status: StatusMessage,
    last_status_toast_id: u64,
    transient_toaster: ToastEngine<()>,
    sticky_toaster: ToastEngine<()>,
    logo: PixelLogo,
    footer_auto_hidden: bool,
    footer_manual_override: bool,
    pending_changelog_write: Option<PendingChangelogWrite>,
    should_quit: bool,
}

impl App {
    fn new() -> Result<Self> {
        let config_store = ConfigStore::locate()?;
        Self::new_with_config_store(config_store)
    }

    fn new_with_config_store(config_store: ConfigStore) -> Result<Self> {
        let config = config_store.load()?;
        let status = StatusMessage::info("Press N to create your first project, or Q to quit.");
        let (
            background_runtime,
            foreground_request_tx,
            refresh_request_tx,
            prefetch_request_tx,
            background_result_rx,
        ) = spawn_background_worker()?;
        Ok(Self {
            config_store,
            config,
            screen: Screen::Dashboard,
            selected_project: 0,
            dashboard_focus: DashboardPane::Projects,
            overview_tab: OverviewTab::Overview,
            overview_show_recent_tab: false,
            project_settings_tab: ProjectSettingsTab::General,
            project_settings_state: ProjectSettingsState::default(),
            overview_focused_scope: 0,
            overview_recent_changes: None,
            overview_recent_project: None,
            overview_recent_error: None,
            overview_tile_project: None,
            overview_activity_project: None,
            overview_activity_summaries: Vec::new(),
            overview_scope_order: Vec::new(),
            overview_pending_versions: Vec::new(),
            overview_tile_dev_modes: Vec::new(),
            overview_tile_rls_modes: Vec::new(),
            overview_tile_last_rotation_at: Instant::now(),
            overview_tile_scroll: 0,
            overview_tile_viewport: None,
            overview_recent_viewport: None,
            release_now_log_viewport: None,
            overview_tile_rects: Vec::new(),
            overview_drag_scope: None,
            wizard: ProjectWizard::default(),
            bump_dialog: None,
            overview_bump_kind_dialog: None,
            overview_bump_workflow_dialog: None,
            overview_branch_bump_dialog: None,
            overview_bump_warning_dialog: None,
            main_branch_warning_dialog: None,
            std_changelog_sub_branch_dialog: None,
            changelog_preview_dialog: None,
            recent_changes_dialog: None,
            commit_rename_dialog: None,
            tag_dialog: None,
            tag_annotation_dialog: None,
            release_now_dialog: None,
            release_now_notes_dialog: None,
            delete_confirmation_dialog: None,
            progress_dialog: None,
            foreground_request_tx,
            refresh_request_tx,
            prefetch_request_tx,
            background_result_rx,
            _background_runtime: background_runtime,
            background_job_active: false,
            background_jobs_inflight: 0,
            next_background_job_id: 1,
            active_foreground_job_id: None,
            current_recent_changes_job_id: None,
            current_recent_changes_prefetch_job_id: None,
            current_changelog_preview_job_id: None,
            current_overview_activity_job_id: None,
            current_release_now_job_id: None,
            overview_activity_job_inflight: false,
            overview_activity_refresh_inflight: false,
            overview_activity_refresh_pending: false,
            current_recent_changes_cancel: None,
            current_recent_changes_prefetch_cancel: None,
            current_changelog_preview_cancel: None,
            current_overview_activity_cancel: None,
            current_release_now_cancel: None,
            project_edit_dialog: None,
            browser_dialog: None,
            hit_targets: Vec::new(),
            last_text_input_click_target: None,
            last_text_input_click_at: None,
            last_recent_change_click_target: None,
            last_recent_change_click_at: None,
            last_status_toast_id: status.id,
            transient_toaster: ToastEngineBuilder::new(Rect::default())
                .default_duration(Duration::from_secs(2))
                .build(),
            sticky_toaster: ToastEngineBuilder::new(Rect::default())
                .default_duration(Duration::from_secs(2))
                .build(),
            status,
            logo: PixelLogo::load(),
            footer_auto_hidden: false,
            footer_manual_override: false,
            pending_changelog_write: None,
            should_quit: false,
        })
    }

    #[cfg(test)]
    fn new_for_tests() -> Result<Self> {
        let unique = format!(
            "cg-test-config-{}.toml",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        let path = std::env::temp_dir().join(unique);
        Self::new_with_config_store(ConfigStore::with_path(path))
    }

    fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('v') {
            self.paste_from_clipboard();
            return Ok(());
        }

        if self.try_handle_toast_shortcut(key) {
            return Ok(());
        }

        if self.progress_dialog.is_some() {
            return Ok(());
        }

        if self.browser_dialog.is_some() {
            return self.handle_browser_key(key);
        }

        if self.release_now_notes_dialog.is_some() {
            return self.handle_release_now_notes_key(key);
        }

        if self.release_now_dialog.is_some() {
            return self.handle_release_now_key(key);
        }

        if self.delete_confirmation_dialog.is_some() {
            return self.handle_delete_confirmation_key(key);
        }

        if self.commit_rename_dialog.is_some() {
            return self.handle_commit_rename_key(key);
        }

        if self.project_edit_dialog.is_some() {
            return self.handle_project_edit_key(key);
        }

        if self.tag_annotation_dialog.is_some() {
            return self.handle_tag_annotation_key(key);
        }

        if self.main_branch_warning_dialog.is_some() {
            return self.handle_main_branch_warning_key(key);
        }

        if self.std_changelog_sub_branch_dialog.is_some() {
            return self.handle_std_changelog_sub_branch_key(key);
        }

        if self.tag_dialog.is_some() {
            return self.handle_tag_key(key);
        }

        if self.changelog_preview_dialog.is_some() {
            return self.handle_changelog_preview_key(key);
        }

        if self.recent_changes_dialog.is_some() {
            return self.handle_recent_changes_key(key);
        }

        if self.overview_bump_warning_dialog.is_some() {
            return self.handle_overview_bump_warning_key(key);
        }

        if self.overview_bump_kind_dialog.is_some() {
            return self.handle_overview_bump_kind_key(key);
        }

        if self.overview_branch_bump_dialog.is_some() {
            return self.handle_overview_branch_bump_key(key);
        }

        if self.overview_bump_workflow_dialog.is_some() {
            return self.handle_overview_bump_workflow_key(key);
        }

        if self.bump_dialog.is_some() {
            return self.handle_bump_key(key);
        }

        if self.screen == Screen::Dashboard
            && self.overview_tab == OverviewTab::ProjectSettings
            && p_s_s::captures_text_input(self)
        {
            return self.handle_dashboard_key(key);
        }

        if self.handle_tab_shortcut(key) {
            return Ok(());
        }

        if self.try_handle_ui_shortcut(key)? {
            return Ok(());
        }

        if key.code == KeyCode::Char('q')
            && key.modifiers.is_empty()
            && !(matches!(self.screen, Screen::Wizard) && self.wizard.focus_accepts_text())
            && !self
                .project_edit_dialog
                .as_ref()
                .map(|dialog| dialog.focus_accepts_text())
                .unwrap_or(false)
            && !p_s_s::captures_text_input(self)
        {
            self.should_quit = true;
            return Ok(());
        }

        match self.screen {
            Screen::Dashboard => self.handle_dashboard_key(key),
            Screen::UiSettings => self.handle_ui_settings_key(key),
            Screen::Wizard => self.handle_wizard_key(key),
        }
    }

    fn handle_dashboard_key(&mut self, key: KeyEvent) -> Result<()> {
        if p_s_s::try_handle_project_settings_key(self, key)? {
            return Ok(());
        }

        match key.code {
            KeyCode::Char('r') | KeyCode::Char('R')
                if key.modifiers.contains(KeyModifiers::CONTROL)
                    && self.dashboard_focus == DashboardPane::Overview
                    && self.overview_recent_changes.is_some() =>
            {
                return self.open_commit_rename_from_view(RecentChangeView::Overview);
            }
            KeyCode::Char('n') => self.open_wizard(),
            KeyCode::Char('e') => self.open_project_edit_dialog()?,
            KeyCode::Char('d') | KeyCode::Char('D') => self.request_dashboard_delete()?,
            KeyCode::Char('l') | KeyCode::Char('L') => self.open_release_now_with_scope(None)?,
            KeyCode::Char('b') => self.open_bump_dialog()?,
            KeyCode::Char('g') => self.open_recent_changes()?,
            KeyCode::Char('c') | KeyCode::Char('C') => {
                self.open_dashboard_changelog_preview(None)?
            }
            KeyCode::Char('t') => self.open_tag_dialog()?,
            KeyCode::Char('r') | KeyCode::Char('R') => self.reload_dashboard_overview_data()?,
            KeyCode::Tab | KeyCode::BackTab => self.toggle_dashboard_focus(),
            KeyCode::Up => {
                if self.dashboard_focus == DashboardPane::Overview {
                    if !self.scroll_dashboard_recent_changes(-1) {
                        let _ = self.scroll_dashboard_tiles(-1);
                    }
                } else {
                    self.move_project_selection(-1);
                }
            }
            KeyCode::Down => {
                if self.dashboard_focus == DashboardPane::Overview {
                    if !self.scroll_dashboard_recent_changes(1) {
                        let _ = self.scroll_dashboard_tiles(1);
                    }
                } else {
                    self.move_project_selection(1);
                }
            }
            KeyCode::Left if self.dashboard_focus == DashboardPane::Overview => {
                self.move_dashboard_overview_focus(-1)?;
            }
            KeyCode::Right if self.dashboard_focus == DashboardPane::Overview => {
                self.move_dashboard_overview_focus(1)?;
            }
            KeyCode::PageUp
                if self.dashboard_focus == DashboardPane::Overview
                    && !self.scroll_dashboard_recent_changes(-6) =>
            {
                let _ = self.scroll_dashboard_tiles(-1);
            }
            KeyCode::PageDown
                if self.dashboard_focus == DashboardPane::Overview
                    && !self.scroll_dashboard_recent_changes(6) =>
            {
                let _ = self.scroll_dashboard_tiles(1);
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_delete_confirmation_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Left | KeyCode::Right | KeyCode::Tab | KeyCode::BackTab => {
                if let Some(dialog) = &mut self.delete_confirmation_dialog {
                    dialog.toggle_selection();
                }
            }
            KeyCode::Enter => {
                if self
                    .delete_confirmation_dialog
                    .as_ref()
                    .map(|dialog| dialog.confirm_selected)
                    .unwrap_or(false)
                {
                    return self.confirm_delete_request();
                }
                self.cancel_delete_request();
            }
            KeyCode::Char('y') | KeyCode::Char('Y') => return self.confirm_delete_request(),
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => self.cancel_delete_request(),
            _ => {}
        }
        Ok(())
    }

    fn handle_release_now_key(&mut self, key: KeyEvent) -> Result<()> {
        let warning_mode = self
            .release_now_dialog
            .as_ref()
            .map(rls_now::ReleaseNowDialog::is_warning_mode)
            .unwrap_or(false);
        let running = self
            .release_now_dialog
            .as_ref()
            .map(rls_now::ReleaseNowDialog::is_running)
            .unwrap_or(false);
        let completed = self
            .release_now_dialog
            .as_ref()
            .map(rls_now::ReleaseNowDialog::is_completed)
            .unwrap_or(false);

        if warning_mode {
            match key.code {
                KeyCode::Left | KeyCode::Right | KeyCode::Tab | KeyCode::BackTab => {
                    if let Some(dialog) = &mut self.release_now_dialog {
                        dialog.toggle_warning_selection();
                    }
                }
                KeyCode::Enter => {
                    let proceed = self
                        .release_now_dialog
                        .as_ref()
                        .map(|dialog| dialog.warning_confirm_selected)
                        .unwrap_or(false);
                    if proceed {
                        if let Some(dialog) = &mut self.release_now_dialog {
                            dialog.proceed_past_warning();
                        }
                    } else {
                        self.close_release_now_dialog();
                    }
                }
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    if let Some(dialog) = &mut self.release_now_dialog {
                        dialog.proceed_past_warning();
                    }
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                    self.close_release_now_dialog()
                }
                _ => {}
            }
            return Ok(());
        }

        if running {
            match key.code {
                KeyCode::Char('f') | KeyCode::Char('F') | KeyCode::End => {
                    self.toggle_release_now_auto_follow()
                }
                KeyCode::Char('x') | KeyCode::Char('X') => self.request_cancel_release_now(),
                KeyCode::Up => self.scroll_release_now(-1),
                KeyCode::Down => self.scroll_release_now(1),
                KeyCode::PageUp => self.scroll_release_now(-6),
                KeyCode::PageDown => self.scroll_release_now(6),
                KeyCode::Esc => {
                    self.status = StatusMessage::warning(
                        "ReleaseNOW is still running. Wait for it to finish before closing the dialog.",
                    );
                }
                _ => {}
            }
            return Ok(());
        }

        if completed {
            match key.code {
                KeyCode::Esc | KeyCode::Enter => self.close_release_now_dialog(),
                KeyCode::Up => self.scroll_release_now(-1),
                KeyCode::Down => self.scroll_release_now(1),
                KeyCode::PageUp => self.scroll_release_now(-6),
                KeyCode::PageDown => self.scroll_release_now(6),
                _ => {}
            }
            return Ok(());
        }

        match key.code {
            KeyCode::Esc => self.close_release_now_dialog(),
            KeyCode::Left => {
                if let Some(dialog) = &mut self.release_now_dialog {
                    dialog.cycle_option(-1);
                }
            }
            KeyCode::Right => {
                if let Some(dialog) = &mut self.release_now_dialog {
                    dialog.cycle_option(1);
                }
            }
            KeyCode::Char('c') | KeyCode::Char('C') => {
                if let Some(dialog) = &mut self.release_now_dialog {
                    dialog.toggle_attach_changelog();
                }
            }
            KeyCode::Char('e') | KeyCode::Char('E') => self.open_release_now_notes_dialog()?,
            KeyCode::Enter | KeyCode::F(2) => return self.request_run_release_now(),
            KeyCode::Up => self.scroll_release_now(-1),
            KeyCode::Down => self.scroll_release_now(1),
            KeyCode::PageUp => self.scroll_release_now(-6),
            KeyCode::PageDown => self.scroll_release_now(6),
            _ => {}
        }
        Ok(())
    }

    fn handle_release_now_notes_key(&mut self, key: KeyEvent) -> Result<()> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s') {
            return self.save_release_now_notes();
        }

        match key.code {
            KeyCode::Esc => {
                self.release_now_notes_dialog = None;
                self.status = StatusMessage::info("Release notes editor closed.");
            }
            KeyCode::F(2) => return self.save_release_now_notes(),
            _ => {
                if let Some(dialog) = &mut self.release_now_notes_dialog
                    && let Some(input) = convert_to_textarea_input(key)
                {
                    dialog.editor.input(input);
                }
            }
        }

        Ok(())
    }

    fn handle_ui_settings_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Char('d') => {
                self.screen = Screen::Dashboard;
                self.dashboard_focus = DashboardPane::Projects;
            }
            KeyCode::Char('n') => self.open_wizard(),
            KeyCode::Char('h') | KeyCode::Char('H') => {
                self.toggle_footer()?;
            }
            KeyCode::Char('t') | KeyCode::Enter | KeyCode::Char(' ') => {
                self.toggle_tab_hints()?;
            }
            KeyCode::Char('c') | KeyCode::Char('C') | KeyCode::Right => {
                self.cycle_footer_content(1)?
            }
            KeyCode::Left => self.cycle_footer_content(-1)?,
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
            KeyCode::Enter | KeyCode::F(2) => self.request_apply_bump()?,
            _ => {}
        }
        Ok(())
    }

    fn handle_overview_bump_workflow_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => self.cancel_overview_bump_workflow(),
            KeyCode::Up | KeyCode::BackTab => self.rotate_overview_bump_workflow(-1),
            KeyCode::Down | KeyCode::Tab => self.rotate_overview_bump_workflow(1),
            KeyCode::Char(character) => {
                if let Some(index) = digit_to_index(character) {
                    self.select_overview_bump_workflow(index);
                }
            }
            KeyCode::Enter | KeyCode::F(2) => return self.request_confirm_overview_bump_workflow(),
            _ => {}
        }
        Ok(())
    }

    fn handle_overview_bump_kind_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => self.cancel_overview_bump_kind(),
            KeyCode::Up | KeyCode::BackTab => self.rotate_overview_bump_kind(-1),
            KeyCode::Down | KeyCode::Tab => self.rotate_overview_bump_kind(1),
            KeyCode::Char(character) => {
                if let Some(index) = digit_to_index(character) {
                    self.select_overview_bump_kind(index);
                }
            }
            KeyCode::Enter | KeyCode::F(2) => return self.confirm_overview_bump_kind(),
            _ => {}
        }
        Ok(())
    }

    fn handle_overview_branch_bump_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => self.cancel_overview_branch_bump(),
            KeyCode::Up | KeyCode::BackTab => {
                if let Some(dialog) = &mut self.overview_branch_bump_dialog {
                    dialog.rotate(-1);
                }
            }
            KeyCode::Down | KeyCode::Tab => {
                if let Some(dialog) = &mut self.overview_branch_bump_dialog {
                    dialog.rotate(1);
                }
            }
            KeyCode::Char(character) if key.modifiers.is_empty() => {
                if let Some(index) = digit_to_index(character) {
                    if let Some(dialog) = &mut self.overview_branch_bump_dialog {
                        dialog.select(index);
                    }
                    return Ok(());
                }
                if let Some(dialog) = &mut self.overview_branch_bump_dialog
                    && dialog.input_enabled()
                {
                    dialog.branch_name.insert(character);
                }
            }
            KeyCode::Char(character) if key.modifiers == KeyModifiers::SHIFT => {
                if let Some(dialog) = &mut self.overview_branch_bump_dialog
                    && dialog.input_enabled()
                {
                    dialog.branch_name.insert(character);
                }
            }
            KeyCode::Enter | KeyCode::F(2) => return self.confirm_overview_branch_bump(),
            KeyCode::Backspace => {
                if let Some(dialog) = &mut self.overview_branch_bump_dialog
                    && dialog.input_enabled()
                {
                    dialog.branch_name.backspace();
                }
            }
            KeyCode::Delete => {
                if let Some(dialog) = &mut self.overview_branch_bump_dialog
                    && dialog.input_enabled()
                {
                    dialog.branch_name.delete();
                }
            }
            KeyCode::Left => {
                if let Some(dialog) = &mut self.overview_branch_bump_dialog
                    && dialog.input_enabled()
                {
                    dialog.branch_name.move_left();
                }
            }
            KeyCode::Right => {
                if let Some(dialog) = &mut self.overview_branch_bump_dialog
                    && dialog.input_enabled()
                {
                    dialog.branch_name.move_right();
                }
            }
            KeyCode::Home => {
                if let Some(dialog) = &mut self.overview_branch_bump_dialog
                    && dialog.input_enabled()
                {
                    dialog.branch_name.home();
                }
            }
            KeyCode::End => {
                if let Some(dialog) = &mut self.overview_branch_bump_dialog
                    && dialog.input_enabled()
                {
                    dialog.branch_name.end();
                }
            }
            KeyCode::PageUp => {
                if let Some(dialog) = &mut self.overview_branch_bump_dialog {
                    dialog.scroll_by(-3);
                }
            }
            KeyCode::PageDown => {
                if let Some(dialog) = &mut self.overview_branch_bump_dialog {
                    dialog.scroll_by(3);
                }
            }
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

    fn handle_main_branch_warning_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => self.cancel_main_branch_warning(),
            KeyCode::Up | KeyCode::BackTab => self.rotate_main_branch_warning(-1),
            KeyCode::Down | KeyCode::Tab => self.rotate_main_branch_warning(1),
            KeyCode::Char('1') => self.select_main_branch_warning(0),
            KeyCode::Char('2') => self.select_main_branch_warning(1),
            KeyCode::Char('3') => self.select_main_branch_warning(2),
            KeyCode::Enter | KeyCode::F(2) => return self.confirm_main_branch_warning(),
            _ => {}
        }
        Ok(())
    }

    fn handle_std_changelog_sub_branch_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => self.cancel_std_changelog_sub_branch_warning(),
            KeyCode::Up | KeyCode::BackTab => self.rotate_std_changelog_sub_branch_warning(-1),
            KeyCode::Down | KeyCode::Tab => self.rotate_std_changelog_sub_branch_warning(1),
            KeyCode::Char('1') => self.select_std_changelog_sub_branch_warning(0),
            KeyCode::Char('2') => self.select_std_changelog_sub_branch_warning(1),
            KeyCode::Char('3') => self.select_std_changelog_sub_branch_warning(2),
            KeyCode::Enter | KeyCode::F(2) => {
                return self.confirm_std_changelog_sub_branch_warning();
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_changelog_preview_key(&mut self, key: KeyEvent) -> Result<()> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s') {
            return self.save_changelog_preview();
        }

        let mut refresh_selection = None;

        match key.code {
            KeyCode::Esc => self.cancel_changelog_preview(),
            KeyCode::F(2) => return self.confirm_changelog_preview(),
            KeyCode::Enter
                if self
                    .changelog_preview_dialog
                    .as_ref()
                    .is_some_and(|dialog| dialog.workflow.is_none()) =>
            {
                return self.confirm_changelog_preview();
            }
            KeyCode::Tab
                if self
                    .changelog_preview_dialog
                    .as_ref()
                    .and_then(|dialog| dialog.custom_range.as_ref())
                    .is_some() =>
            {
                if let Some(custom_range) = self
                    .changelog_preview_dialog
                    .as_mut()
                    .and_then(|dialog| dialog.custom_range.as_mut())
                {
                    custom_range.cycle_focus(1);
                }
            }
            KeyCode::BackTab
                if self
                    .changelog_preview_dialog
                    .as_ref()
                    .and_then(|dialog| dialog.custom_range.as_ref())
                    .is_some() =>
            {
                if let Some(custom_range) = self
                    .changelog_preview_dialog
                    .as_mut()
                    .and_then(|dialog| dialog.custom_range.as_mut())
                {
                    custom_range.cycle_focus(-1);
                }
            }
            KeyCode::Char('1')
                if self
                    .changelog_preview_dialog
                    .as_ref()
                    .and_then(|dialog| dialog.custom_range.as_ref())
                    .is_some() =>
            {
                if let Some(custom_range) = self
                    .changelog_preview_dialog
                    .as_mut()
                    .and_then(|dialog| dialog.custom_range.as_mut())
                {
                    custom_range.select_focus(CustomChangelogRangeFocus::From);
                }
            }
            KeyCode::Char('2')
                if self
                    .changelog_preview_dialog
                    .as_ref()
                    .and_then(|dialog| dialog.custom_range.as_ref())
                    .is_some() =>
            {
                if let Some(custom_range) = self
                    .changelog_preview_dialog
                    .as_mut()
                    .and_then(|dialog| dialog.custom_range.as_mut())
                {
                    custom_range.select_focus(CustomChangelogRangeFocus::To);
                }
            }
            KeyCode::Left
                if self
                    .changelog_preview_dialog
                    .as_ref()
                    .and_then(|dialog| dialog.custom_range.as_ref())
                    .is_some() =>
            {
                if let Some(custom_range) = self
                    .changelog_preview_dialog
                    .as_mut()
                    .and_then(|dialog| dialog.custom_range.as_mut())
                    && custom_range.adjust_focused_selection(-1)
                {
                    refresh_selection = custom_range.selection();
                }
            }
            KeyCode::Right
                if self
                    .changelog_preview_dialog
                    .as_ref()
                    .and_then(|dialog| dialog.custom_range.as_ref())
                    .is_some() =>
            {
                if let Some(custom_range) = self
                    .changelog_preview_dialog
                    .as_mut()
                    .and_then(|dialog| dialog.custom_range.as_mut())
                    && custom_range.adjust_focused_selection(1)
                {
                    refresh_selection = custom_range.selection();
                }
            }
            KeyCode::Char('r') | KeyCode::Char('R')
                if self
                    .changelog_preview_dialog
                    .as_ref()
                    .and_then(|dialog| dialog.custom_range.as_ref())
                    .is_some() =>
            {
                refresh_selection = self
                    .changelog_preview_dialog
                    .as_ref()
                    .and_then(|dialog| dialog.custom_range.as_ref())
                    .and_then(CustomChangelogRangeState::selection);
            }
            KeyCode::PageUp => self.scroll_changelog_preview(-8),
            KeyCode::PageDown => self.scroll_changelog_preview(8),
            _ => {
                if let Some(dialog) = &mut self.changelog_preview_dialog {
                    if dialog.workflow.is_none() {
                        return Ok(());
                    }
                    if let Some(input) = convert_to_textarea_input(key) {
                        dialog.release_message.input(input);
                    }
                }
            }
        }

        if let Some(selection) = refresh_selection {
            return self.open_dashboard_changelog_preview(Some(selection));
        }

        Ok(())
    }

    fn handle_recent_changes_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => {
                self.recent_changes_dialog = None;
                self.cancel_background_job_kind(BackgroundJobKind::RecentChanges);
                self.cancel_background_job_kind(BackgroundJobKind::RecentChangesPrefetch);
                self.current_recent_changes_job_id = None;
                self.status = StatusMessage::info("Git log closed.");
            }
            KeyCode::Char('r') | KeyCode::Char('R')
                if key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                return self.open_commit_rename_from_view(RecentChangeView::Popup);
            }
            KeyCode::Up => self.scroll_recent_changes(-1),
            KeyCode::Down => self.scroll_recent_changes(1),
            KeyCode::PageUp => self.scroll_recent_changes(-8),
            KeyCode::PageDown => self.scroll_recent_changes(8),
            KeyCode::Tab => {
                if let Some(dialog) = &mut self.recent_changes_dialog {
                    if dialog.active_tab == RecentChangesTab::Recent && !dialog.history_loaded {
                        self.schedule_recent_changes_action(
                            "Loading tag history for the selected scope.",
                            RecentChangesLoadAction::SwitchTab(RecentChangesTab::History),
                        )?;
                    } else {
                        dialog.cycle_tab(1)?;
                    }
                }
            }
            KeyCode::BackTab => {
                if let Some(dialog) = &mut self.recent_changes_dialog {
                    dialog.cycle_tab(-1)?;
                }
            }
            KeyCode::Char('1') => {
                if let Some(dialog) = &mut self.recent_changes_dialog {
                    dialog.switch_tab(RecentChangesTab::Recent)?;
                }
            }
            KeyCode::Char('2') => {
                if let Some(dialog) = &mut self.recent_changes_dialog {
                    if dialog.history_loaded {
                        dialog.switch_tab(RecentChangesTab::History)?;
                    } else {
                        self.schedule_recent_changes_action(
                            "Loading tag history for the selected scope.",
                            RecentChangesLoadAction::SwitchTab(RecentChangesTab::History),
                        )?;
                    }
                }
            }
            KeyCode::Char('[') if self.recent_changes_dialog.is_some() => {
                self.schedule_recent_changes_action(
                    "Loading git history for the previous scope.",
                    RecentChangesLoadAction::RotateScope(-1),
                )?;
            }
            KeyCode::Char(']') if self.recent_changes_dialog.is_some() => {
                self.schedule_recent_changes_action(
                    "Loading git history for the next scope.",
                    RecentChangesLoadAction::RotateScope(1),
                )?;
            }
            KeyCode::Char('r') | KeyCode::Char('R') if self.recent_changes_dialog.is_some() => {
                self.schedule_recent_changes_action(
                    "Refreshing git history for the current scope.",
                    RecentChangesLoadAction::RefreshCurrentScope,
                )?;
            }
            KeyCode::Left => {
                if let Some(dialog) = &mut self.recent_changes_dialog {
                    if dialog.active_tab == RecentChangesTab::Recent && dialog.can_select_scope() {
                        self.schedule_recent_changes_action(
                            "Loading git history for the previous scope.",
                            RecentChangesLoadAction::RotateScope(-1),
                        )?;
                    } else if dialog.active_tab == RecentChangesTab::History {
                        dialog.navigate_history(1);
                    }
                }
            }
            KeyCode::Right => {
                if let Some(dialog) = &mut self.recent_changes_dialog {
                    if dialog.active_tab == RecentChangesTab::Recent && dialog.can_select_scope() {
                        self.schedule_recent_changes_action(
                            "Loading git history for the next scope.",
                            RecentChangesLoadAction::RotateScope(1),
                        )?;
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

    fn handle_commit_rename_key(&mut self, key: KeyEvent) -> Result<()> {
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('p') | KeyCode::Char('P'))
        {
            self.toggle_commit_rename_force_push();
            return Ok(());
        }

        match key.code {
            KeyCode::Esc => {
                self.commit_rename_dialog = None;
                self.status = StatusMessage::info("Commit rename cancelled.");
            }
            KeyCode::Enter | KeyCode::F(2) => return self.apply_commit_rename(),
            KeyCode::Tab | KeyCode::Char(' ')
                if self
                    .commit_rename_dialog
                    .as_ref()
                    .map(|dialog| dialog.plan.touches_pushed_history)
                    .unwrap_or(false) =>
            {
                self.toggle_commit_rename_force_push();
            }
            _ => {
                if let Some(dialog) = &mut self.commit_rename_dialog {
                    dialog.message_input.handle_key(key);
                }
            }
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
                if let Some(dialog) = &mut self.tag_annotation_dialog
                    && let Some(input) = convert_to_textarea_input(key)
                {
                    dialog.editor.input(input);
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

        if self.progress_dialog.is_some() {
            return;
        }

        if self.release_now_notes_dialog.is_some() {
            match mouse.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    if let Some(action) = self.resolve_hit_action(mouse.column, mouse.row, false)
                        && let Err(error) = self.handle_hit_action(action)
                    {
                        self.status = StatusMessage::error(error.to_string());
                    }
                    return;
                }
                _ => return,
            }
        }

        if self.release_now_dialog.is_some() {
            let in_log_viewport = self
                .release_now_log_viewport
                .map(|viewport| rect_contains(viewport, mouse.column, mouse.row))
                .unwrap_or(false);
            match mouse.kind {
                MouseEventKind::ScrollUp => {
                    self.scroll_release_now(-2);
                    return;
                }
                MouseEventKind::ScrollDown => {
                    self.scroll_release_now(2);
                    return;
                }
                MouseEventKind::Down(MouseButton::Left) => {
                    if in_log_viewport && self.begin_release_now_log_selection(mouse.row) {
                        return;
                    }
                    if let Some(action) = self.resolve_hit_action(mouse.column, mouse.row, false)
                        && let Err(error) = self.handle_hit_action(action)
                    {
                        self.status = StatusMessage::error(error.to_string());
                    }
                    return;
                }
                MouseEventKind::Drag(MouseButton::Left) => {
                    if in_log_viewport && self.update_release_now_log_selection(mouse.row) {
                        return;
                    }
                    return;
                }
                MouseEventKind::Down(MouseButton::Right) => {
                    if in_log_viewport {
                        self.copy_selected_release_now_log(mouse.row);
                    }
                    return;
                }
                MouseEventKind::Up(MouseButton::Left) => return,
                _ => return,
            }
        }

        if self.delete_confirmation_dialog.is_some() {
            match mouse.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    if let Some(action) = self.resolve_hit_action(mouse.column, mouse.row, false)
                        && let Err(error) = self.handle_hit_action(action)
                    {
                        self.status = StatusMessage::error(error.to_string());
                    }
                    return;
                }
                MouseEventKind::ScrollUp
                | MouseEventKind::ScrollDown
                | MouseEventKind::Down(MouseButton::Right)
                | MouseEventKind::Drag(MouseButton::Left)
                | MouseEventKind::Up(MouseButton::Left) => return,
                _ => return,
            }
        }

        if self.commit_rename_dialog.is_some() {
            match mouse.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    if let Some(action) = self.resolve_hit_action(mouse.column, mouse.row, false)
                        && let Err(error) = self.handle_hit_action(action)
                    {
                        self.status = StatusMessage::error(error.to_string());
                    }
                    return;
                }
                MouseEventKind::ScrollUp
                | MouseEventKind::ScrollDown
                | MouseEventKind::Down(MouseButton::Right)
                | MouseEventKind::Drag(MouseButton::Left)
                | MouseEventKind::Up(MouseButton::Left) => return,
                _ => return,
            }
        }

        if self.overview_bump_kind_dialog.is_some() {
            match mouse.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    if let Some(action) = self.resolve_hit_action(mouse.column, mouse.row, false)
                        && let Err(error) = self.handle_hit_action(action)
                    {
                        self.status = StatusMessage::error(error.to_string());
                    }
                    return;
                }
                MouseEventKind::ScrollUp => {
                    self.rotate_overview_bump_kind(-1);
                    return;
                }
                MouseEventKind::ScrollDown => {
                    self.rotate_overview_bump_kind(1);
                    return;
                }
                MouseEventKind::Down(MouseButton::Right)
                | MouseEventKind::Drag(MouseButton::Left)
                | MouseEventKind::Up(MouseButton::Left) => return,
                _ => return,
            }
        }

        if self.recent_changes_dialog.is_some()
            && self.commit_rename_dialog.is_none()
            && self.tag_dialog.is_none()
            && self.tag_annotation_dialog.is_none()
        {
            match mouse.kind {
                MouseEventKind::ScrollUp => {
                    self.scroll_recent_changes(-2);
                    return;
                }
                MouseEventKind::ScrollDown => {
                    self.scroll_recent_changes(2);
                    return;
                }
                MouseEventKind::Down(MouseButton::Left) => {
                    if let Some(action) = self.resolve_hit_action(mouse.column, mouse.row, false)
                        && let Err(error) = self.handle_hit_action(action)
                    {
                        self.status = StatusMessage::error(error.to_string());
                    }
                    return;
                }
                MouseEventKind::Down(MouseButton::Right)
                | MouseEventKind::Drag(MouseButton::Left)
                | MouseEventKind::Up(MouseButton::Left) => return,
                _ => return,
            }
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
                MouseEventKind::Down(MouseButton::Left) => {
                    if let Some(action) = self.resolve_hit_action(mouse.column, mouse.row, false)
                        && let Err(error) = self.handle_hit_action(action)
                    {
                        self.status = StatusMessage::error(error.to_string());
                    }
                    return;
                }
                MouseEventKind::Down(MouseButton::Right)
                | MouseEventKind::Drag(MouseButton::Left)
                | MouseEventKind::Up(MouseButton::Left) => return,
                _ => {}
            }
        }

        match mouse.kind {
            MouseEventKind::ScrollUp => {
                if self.project_edit_dialog.is_some() {
                    self.scroll_project_edit_body(-1);
                } else if self.changelog_preview_dialog.is_some() {
                    self.scroll_changelog_preview(-2);
                } else if self.overview_bump_workflow_dialog.is_some() || self.tag_dialog.is_some()
                {
                } else if self.recent_changes_dialog.is_some() {
                    self.scroll_recent_changes(-2);
                } else if self.bump_dialog.is_some() {
                    self.rotate_bump_action(-1);
                } else if self.screen == Screen::Wizard {
                    self.scroll_wizard_body(-1);
                } else if self.screen == Screen::Dashboard
                    && self.overview_tab == OverviewTab::ProjectSettings
                {
                    self.scroll_project_settings(-1);
                } else if self.screen == Screen::Dashboard
                    && self.overview_tab == OverviewTab::Overview
                {
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
                } else if self.screen == Screen::Dashboard
                    && self.overview_tab == OverviewTab::RecentChanges
                {
                    if let Some(dialog) = &mut self.overview_recent_changes {
                        dialog.scroll_by(-2);
                    }
                } else if self.screen == Screen::Dashboard {
                    self.move_project_selection(-1);
                }
            }
            MouseEventKind::ScrollDown => {
                if self.project_edit_dialog.is_some() {
                    self.scroll_project_edit_body(1);
                } else if self.changelog_preview_dialog.is_some() {
                    self.scroll_changelog_preview(2);
                } else if self.overview_bump_workflow_dialog.is_some() || self.tag_dialog.is_some()
                {
                } else if self.recent_changes_dialog.is_some() {
                    self.scroll_recent_changes(2);
                } else if self.bump_dialog.is_some() {
                    self.rotate_bump_action(1);
                } else if self.screen == Screen::Wizard {
                    self.scroll_wizard_body(1);
                } else if self.screen == Screen::Dashboard
                    && self.overview_tab == OverviewTab::ProjectSettings
                {
                    self.scroll_project_settings(1);
                } else if self.screen == Screen::Dashboard
                    && self.overview_tab == OverviewTab::Overview
                {
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
                } else if self.screen == Screen::Dashboard
                    && self.overview_tab == OverviewTab::RecentChanges
                {
                    if let Some(dialog) = &mut self.overview_recent_changes {
                        dialog.scroll_by(2);
                    }
                } else if self.screen == Screen::Dashboard {
                    self.move_project_selection(1);
                }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                if self.overview_bump_workflow_dialog.is_none()
                    && self.screen == Screen::Dashboard
                    && self.overview_tab == OverviewTab::Overview
                {
                    self.overview_drag_scope =
                        self.overview_tile_rects
                            .iter()
                            .rev()
                            .find_map(|(rect, scope)| {
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
                    if let Some(scope_index) = self.overview_drag_scope
                        && let Err(error) = self.select_dashboard_overview_scope(scope_index)
                    {
                        self.status = StatusMessage::error(error.to_string());
                    }
                }

                if let Some((action, rect)) =
                    self.resolve_hit_target(mouse.column, mouse.row, false)
                {
                    let maybe_click_target = self.text_input_click_target(&action);
                    let maybe_recent_change_target = self.recent_change_click_target(&action);
                    let mut select_all = false;
                    let mut open_commit_rename = None;
                    if let Some(target) = maybe_click_target {
                        let now = Instant::now();
                        if self.last_text_input_click_target == Some(target)
                            && self
                                .last_text_input_click_at
                                .map(|previous| {
                                    now.duration_since(previous) <= Duration::from_millis(400)
                                })
                                .unwrap_or(false)
                        {
                            select_all = true;
                        }
                        self.last_text_input_click_target = Some(target);
                        self.last_text_input_click_at = Some(now);
                    } else {
                        self.last_text_input_click_target = None;
                        self.last_text_input_click_at = None;
                    }

                    if let Some(target) = maybe_recent_change_target {
                        let now = Instant::now();
                        if self.last_recent_change_click_target == Some(target)
                            && self
                                .last_recent_change_click_at
                                .map(|previous| {
                                    now.duration_since(previous) <= Duration::from_millis(400)
                                })
                                .unwrap_or(false)
                        {
                            open_commit_rename = Some(target.view);
                        }
                        self.last_recent_change_click_target = Some(target);
                        self.last_recent_change_click_at = Some(now);
                    } else {
                        self.last_recent_change_click_target = None;
                        self.last_recent_change_click_at = None;
                    }

                    if let Err(error) = self.handle_hit_action(action) {
                        self.status = StatusMessage::error(error.to_string());
                    }

                    if select_all {
                        if let Some(input) = self.active_text_input_mut() {
                            input.select_all();
                        }
                    } else if maybe_click_target.is_some() {
                        self.set_text_input_cursor_from_mouse(rect, mouse.column);
                    }

                    if let Some(view) = open_commit_rename
                        && let Err(error) = self.open_commit_rename_from_view(view)
                    {
                        self.status = StatusMessage::error(error.to_string());
                    }
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if let Some(from_scope) = self.overview_drag_scope {
                    let target_scope =
                        self.overview_tile_rects
                            .iter()
                            .rev()
                            .find_map(|(rect, scope)| {
                                (mouse.column >= rect.x
                                    && mouse.column < rect.x + rect.width
                                    && mouse.row >= rect.y
                                    && mouse.row < rect.y + rect.height)
                                    .then_some(*scope)
                            });
                    if let Some(to_scope) = target_scope
                        && to_scope != from_scope
                    {
                        self.reorder_dashboard_tile_scope(from_scope, to_scope);
                        self.overview_drag_scope = Some(to_scope);
                    }
                }

                if let Some((action, rect)) =
                    self.resolve_hit_target(mouse.column, mouse.row, false)
                    && let Some(last_target) = self.last_text_input_click_target
                    && last_target.same_field_action(&action)
                {
                    self.update_text_input_drag_selection(rect, mouse.column);
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                self.overview_drag_scope = None;
            }
            MouseEventKind::Down(MouseButton::Right) => {
                if self.overview_bump_workflow_dialog.is_none()
                    && self.screen == Screen::Dashboard
                    && self.overview_tab == OverviewTab::Overview
                    && let Some(scope_index) =
                        self.overview_tile_rects
                            .iter()
                            .rev()
                            .find_map(|(rect, scope)| {
                                (mouse.column >= rect.x
                                    && mouse.column < rect.x + rect.width
                                    && mouse.row >= rect.y
                                    && mouse.row < rect.y + rect.height)
                                    .then_some(*scope)
                            })
                    && let Err(error) = self.select_dashboard_overview_scope(scope_index)
                {
                    self.status = StatusMessage::error(error.to_string());
                }
                let selected_text = self
                    .active_text_input_mut()
                    .and_then(|input| input.selected_text().map(str::to_string));
                if let Some(selection) = selected_text {
                    self.copy_text_to_clipboard(&selection);
                    return;
                }

                let action = self.resolve_hit_action(mouse.column, mouse.row, true);
                if action.is_none() && self.active_text_input_mut().is_some() {
                    self.paste_from_clipboard();
                    return;
                }

                if let Some(action) = action {
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
            KeyCode::Char('u') | KeyCode::Char('U') => {
                return self.confirm_browser_directory_selection();
            }
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

        if let Some(dialog) = &mut self.changelog_preview_dialog
            && dialog.workflow.is_some()
        {
            dialog.release_message.insert_str(text);
            self.status = StatusMessage::info("Pasted into the release notes.");
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

                if let Some(dialog) = &mut self.changelog_preview_dialog
                    && dialog.workflow.is_some()
                {
                    dialog.release_message.insert_str(text);
                    self.status = StatusMessage::info("Pasted into the release notes.");
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
        if let Some(dialog) = &mut self.project_edit_dialog
            && dialog.insert_text(text)
        {
            return true;
        }

        if let Some(dialog) = &mut self.tag_dialog {
            dialog.tag_name.insert_str(text);
            return true;
        }

        if self.screen == Screen::Dashboard
            && self.overview_tab == OverviewTab::ProjectSettings
            && p_s_s::insert_project_settings_text(self, text)
        {
            return true;
        }

        if self.screen == Screen::Wizard && self.wizard.insert_text(text) {
            return true;
        }

        false
    }

    fn handle_hit_action(&mut self, action: HitAction) -> Result<()> {
        match action {
            HitAction::Switch(screen) => {
                self.screen = screen;
                if screen == Screen::Dashboard {
                    self.dashboard_focus = DashboardPane::Projects;
                }
                if screen == Screen::Wizard {
                    self.wizard = ProjectWizard::default();
                }
            }
            HitAction::SelectOverviewTab(tab) => {
                self.overview_tab = tab;
                self.dashboard_focus = DashboardPane::Overview;
            }
            HitAction::SelectProjectSettingsTab(tab) => {
                self.overview_tab = OverviewTab::ProjectSettings;
                self.project_settings_tab = tab;
                self.dashboard_focus = DashboardPane::Overview;
                p_s_s::sync_project_settings_state(self);
            }
            HitAction::SelectProjectSettingsField(field) => {
                return p_s_s::activate_project_settings_field(self, field);
            }
            HitAction::BrowseProjectSettingsField(field) => {
                p_s_s::set_project_settings_focus(self, field);
                return p_s_s::open_browser_for_project_settings_focus(self);
            }
            HitAction::SelectProject(index) => {
                self.selected_project = index.min(self.config.projects.len().saturating_sub(1));
                self.prime_selected_project_dashboard_data();
                self.dashboard_focus = DashboardPane::Projects;
            }
            HitAction::SelectOverviewScope(scope_index) => {
                return self.select_dashboard_overview_scope(scope_index);
            }
            HitAction::OpenOverviewReleaseNow(scope_index) => {
                self.dashboard_focus = DashboardPane::Overview;
                return self.open_overview_release_now(scope_index);
            }
            HitAction::BeginOverviewBump(scope_index) => {
                self.dashboard_focus = DashboardPane::Overview;
                return self.begin_overview_bump(scope_index);
            }
            HitAction::CycleOverviewTileInfo(scope_index, row) => {
                self.dashboard_focus = DashboardPane::Overview;
                return self.cycle_overview_tile_info(scope_index, row);
            }
            HitAction::SelectOverviewBumpWorkflow(index) => {
                self.select_overview_bump_workflow(index)
            }
            HitAction::ConfirmOverviewBumpWorkflow => {
                return self.request_confirm_overview_bump_workflow();
            }
            HitAction::CancelOverviewBumpWorkflow => self.cancel_overview_bump_workflow(),
            HitAction::SelectOverviewBumpKind(index) => self.select_overview_bump_kind(index),
            HitAction::ConfirmOverviewBumpKind => return self.confirm_overview_bump_kind(),
            HitAction::CancelOverviewBumpKind => self.cancel_overview_bump_kind(),
            HitAction::SelectOverviewBumpWarningChoice(index) => {
                self.select_overview_bump_warning(index)
            }
            HitAction::SelectMainBranchWarningChoice(index) => {
                self.select_main_branch_warning(index)
            }
            HitAction::SelectStdChangelogSubBranchChoice(index) => {
                self.select_std_changelog_sub_branch_warning(index)
            }
            HitAction::ConfirmChangelogPreview => return self.confirm_changelog_preview(),
            HitAction::SaveChangelogPreview => return self.save_changelog_preview(),
            HitAction::CancelChangelogPreview => self.cancel_changelog_preview(),
            HitAction::ScrollChangelogPreview(delta) => self.scroll_changelog_preview(delta),
            HitAction::AdjustOverviewVersion(scope_index, control, delta) => {
                return self.adjust_overview_pending_version(scope_index, control, delta);
            }
            HitAction::ResetOverviewPendingVersion(scope_index) => {
                return self.reset_overview_pending_version(scope_index);
            }
            HitAction::OpenOverviewTagDialog(scope_index) => {
                return self.open_overview_tag_dialog(scope_index);
            }
            HitAction::EditProjectField(field) => {
                if let Some(dialog) = &mut self.project_edit_dialog {
                    dialog.focus = field;
                }
            }
            HitAction::ProjectEditScopeAction(action) => {
                return self.apply_project_edit_scope_action(action);
            }
            HitAction::SaveProjectEdit => return self.save_project_edit(),
            HitAction::RemoveProject => return self.remove_project(),
            HitAction::CancelProjectEdit => {
                self.project_edit_dialog = None;
                self.status = StatusMessage::info("Project edit cancelled.");
            }
            HitAction::CycleReleaseNowOption(delta) => {
                if let Some(dialog) = &mut self.release_now_dialog {
                    dialog.cycle_option(delta);
                }
            }
            HitAction::ToggleReleaseNowChangelog => {
                if let Some(dialog) = &mut self.release_now_dialog {
                    dialog.toggle_attach_changelog();
                }
            }
            HitAction::EditReleaseNowNotes => return self.open_release_now_notes_dialog(),
            HitAction::RunReleaseNow => return self.request_run_release_now(),
            HitAction::ContinueReleaseNowWarning => {
                if let Some(dialog) = &mut self.release_now_dialog {
                    dialog.proceed_past_warning();
                }
            }
            HitAction::ToggleReleaseNowAutoFollow => self.toggle_release_now_auto_follow(),
            HitAction::CancelReleaseNowRun => self.request_cancel_release_now(),
            HitAction::ScrollReleaseNow(delta) => self.scroll_release_now(delta),
            HitAction::SaveReleaseNowNotes => return self.save_release_now_notes(),
            HitAction::CancelReleaseNowNotes => {
                self.release_now_notes_dialog = None;
                self.status = StatusMessage::info("Release notes editor closed.");
            }
            HitAction::CloseReleaseNow => self.close_release_now_dialog(),
            HitAction::ConfirmDeleteRequest => return self.confirm_delete_request(),
            HitAction::CancelDeleteRequest => self.cancel_delete_request(),
            HitAction::ToggleTabHints => return self.toggle_tab_hints(),
            HitAction::ToggleFooter => return self.toggle_footer(),
            HitAction::CycleFooterContent(delta) => return self.cycle_footer_content(delta),
            HitAction::BrowseWizardTargetPath => {
                return self.open_browser(BrowseTarget::WizardTargetPath);
            }
            HitAction::BrowseWizardRepoRoot => {
                return self.open_browser(BrowseTarget::WizardRepoRoot);
            }
            HitAction::BrowseProjectTargetPath => {
                return self.open_browser(BrowseTarget::ProjectEditTargetPath);
            }
            HitAction::BrowseProjectRepoRoot => {
                return self.open_browser(BrowseTarget::ProjectEditRepoRoot);
            }
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
                    if tab == RecentChangesTab::History && !dialog.history_loaded {
                        self.schedule_recent_changes_action(
                            "Loading tag history for the selected scope.",
                            RecentChangesLoadAction::SwitchTab(RecentChangesTab::History),
                        )?;
                    } else {
                        dialog.switch_tab(tab)?;
                    }
                }
            }
            HitAction::CycleRecentChangesScope(delta) => {
                if self.recent_changes_dialog.is_some() {
                    let message = if delta.is_negative() {
                        "Loading git history for the previous scope."
                    } else {
                        "Loading git history for the next scope."
                    };
                    self.schedule_recent_changes_action(
                        message,
                        RecentChangesLoadAction::RotateScope(delta),
                    )?;
                }
            }
            HitAction::CycleBumpScope(delta) => self.rotate_bump_scope(delta),
            HitAction::CycleBumpAction(delta) => self.rotate_bump_action(delta),
            HitAction::ApplyBump => return self.request_apply_bump(),
            HitAction::CancelBump => {
                self.bump_dialog = None;
                self.status = StatusMessage::info("Bump preview closed.");
            }
            HitAction::ConfirmOverviewBranchBump => return self.confirm_overview_branch_bump(),
            HitAction::CancelOverviewBranchBump => self.cancel_overview_branch_bump(),
            HitAction::CloseRecentChanges => {
                self.recent_changes_dialog = None;
                self.cancel_background_job_kind(BackgroundJobKind::RecentChanges);
                self.cancel_background_job_kind(BackgroundJobKind::RecentChangesPrefetch);
                self.current_recent_changes_job_id = None;
                self.status = StatusMessage::info("Git log closed.");
            }
            HitAction::ScrollRecentChanges(delta) => self.scroll_recent_changes(delta),
            HitAction::SelectRecentChangeLine(view, line_index) => {
                self.select_recent_change_line(view, line_index)
            }
            HitAction::ToggleCommitRenameForcePush => self.toggle_commit_rename_force_push(),
            HitAction::SaveCommitRename => return self.apply_commit_rename(),
            HitAction::CancelCommitRename => {
                self.commit_rename_dialog = None;
                self.status = StatusMessage::info("Commit rename cancelled.");
            }
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
        let preferred_scope = self.selected_project().ok().and_then(|project| {
            (!project.unified_versioning).then_some(self.overview_focused_scope)
        });
        self.open_recent_changes_with_scope(preferred_scope)
    }

    fn open_overview_tag_dialog(&mut self, scope_index: usize) -> Result<()> {
        let project = self.selected_project()?.clone();
        let preferred_scope = if project.unified_versioning {
            None
        } else {
            Some(scope_index)
        };
        self.open_tag_dialog_with_scope(preferred_scope, None)
    }

    fn open_overview_release_now(&mut self, scope_index: usize) -> Result<()> {
        self.open_release_now_with_scope(Some(scope_index))
    }

    fn open_release_now_with_scope(&mut self, preferred_scope: Option<usize>) -> Result<()> {
        let project = self.selected_project()?.clone();
        let scope_index = preferred_scope.unwrap_or_else(|| {
            if project.project_type == ProjectType::Branched {
                self.overview_focused_scope
                    .min(project.branches.len().saturating_sub(1))
            } else {
                0
            }
        });

        self.bump_dialog = None;
        self.tag_dialog = None;
        self.tag_annotation_dialog = None;
        self.release_now_dialog = None;
        self.release_now_notes_dialog = None;
        self.project_edit_dialog = None;
        self.browser_dialog = None;
        self.schedule_progress_job(
            " Validating ReleaseNOW ",
            format!("Checking ReleaseNOW prerequisites for {}.", project.name),
            BackgroundJobRequest::ValidateReleaseNow {
                project,
                scope_index,
            },
        )?;
        self.status =
            StatusMessage::info("Validating ReleaseNOW prerequisites for the selected scope.");
        Ok(())
    }

    fn open_recent_changes_with_scope(&mut self, preferred_scope: Option<usize>) -> Result<()> {
        let project = self.selected_project()?.clone();
        if !project.integration_mode.requires_repo() {
            bail!("git log requires a git-backed project");
        }

        self.bump_dialog = None;
        self.tag_dialog = None;
        self.project_edit_dialog = None;
        self.schedule_progress_job(
            " Loading Git Commits ",
            format!("Loading git history for {}.", project.name),
            BackgroundJobRequest::OpenRecentChanges {
                project,
                preferred_scope,
            },
        )?;
        self.status = StatusMessage::info("Loading git history for the selected project.");
        Ok(())
    }

    fn open_release_now_notes_dialog(&mut self) -> Result<()> {
        let dialog = self
            .release_now_dialog
            .as_ref()
            .ok_or_else(|| anyhow!("ReleaseNOW is not open"))?;
        if !dialog.attach_changelog {
            bail!("enable changelog attachment before editing release notes")
        }
        self.release_now_notes_dialog = Some(TagAnnotationDialog::with_placeholder(
            &dialog.release_notes_markdown,
            dialog.release_notes_placeholder.as_str(),
        ));
        self.status = StatusMessage::info("Editing ReleaseNOW release notes.");
        Ok(())
    }

    fn save_release_now_notes(&mut self) -> Result<()> {
        let notes = self
            .release_now_notes_dialog
            .as_ref()
            .ok_or_else(|| anyhow!("ReleaseNOW release notes editor is not open"))?
            .editor
            .lines()
            .join("\n");
        let dialog = self
            .release_now_dialog
            .as_mut()
            .ok_or_else(|| anyhow!("ReleaseNOW is not open"))?;
        dialog.release_notes_markdown = notes;
        self.release_now_notes_dialog = None;
        self.status = StatusMessage::success("ReleaseNOW release notes updated.");
        Ok(())
    }

    fn request_run_release_now(&mut self) -> Result<()> {
        let request = {
            let dialog = self
                .release_now_dialog
                .as_ref()
                .ok_or_else(|| anyhow!("ReleaseNOW is not open"))?;
            if dialog.is_warning_mode() {
                bail!("confirm the recent bump warning before running ReleaseNOW")
            }
            if dialog.is_running() {
                bail!("ReleaseNOW is already running")
            }
            if dialog.is_completed() {
                self.close_release_now_dialog();
                return Ok(());
            }

            rls_now::build_execution_request(dialog)
        };

        if let Some(dialog) = &mut self.release_now_dialog {
            dialog.begin_running();
        }

        self.schedule_foreground_job(BackgroundJobRequest::RunReleaseNow { request })?;
        self.status = StatusMessage::info(
            "Running ReleaseNOW for the selected scope. Live logs will stream into the dialog.",
        );
        Ok(())
    }

    fn close_release_now_dialog(&mut self) {
        if self
            .release_now_dialog
            .as_ref()
            .map(rls_now::ReleaseNowDialog::is_running)
            .unwrap_or(false)
        {
            self.status = StatusMessage::warning(
                "ReleaseNOW is still running. Wait for it to finish before closing the dialog.",
            );
            return;
        }

        self.release_now_notes_dialog = None;
        self.release_now_dialog = None;
        self.status = StatusMessage::info("ReleaseNOW closed.");
    }

    fn scroll_release_now(&mut self, delta: i16) {
        if let Some(dialog) = &mut self.release_now_dialog {
            dialog.scroll_by(delta);
        }
    }

    fn begin_release_now_log_selection(&mut self, mouse_row: u16) -> bool {
        let Some(viewport) = self.release_now_log_viewport else {
            return false;
        };
        let Some(dialog) = &mut self.release_now_dialog else {
            return false;
        };

        dialog.begin_body_selection(mouse_row.saturating_sub(viewport.y))
    }

    fn update_release_now_log_selection(&mut self, mouse_row: u16) -> bool {
        let Some(viewport) = self.release_now_log_viewport else {
            return false;
        };
        let Some(dialog) = &mut self.release_now_dialog else {
            return false;
        };

        dialog.update_body_selection(mouse_row.saturating_sub(viewport.y))
    }

    fn copy_selected_release_now_log(&mut self, mouse_row: u16) {
        let Some(viewport) = self.release_now_log_viewport else {
            return;
        };
        let Some(dialog) = &mut self.release_now_dialog else {
            return;
        };

        if !dialog.has_body_selection() {
            let _ = dialog.begin_body_selection(mouse_row.saturating_sub(viewport.y));
        }

        if let Some(text) = dialog.selected_body_text() {
            self.copy_text_to_clipboard(&text);
        }
    }

    fn toggle_release_now_auto_follow(&mut self) {
        if let Some(dialog) = &mut self.release_now_dialog {
            let enabled = dialog.toggle_auto_follow();
            self.status = StatusMessage::info(if enabled {
                "ReleaseNOW auto-follow resumed."
            } else {
                "ReleaseNOW auto-follow paused."
            });
        }
    }

    fn request_cancel_release_now(&mut self) {
        let Some(dialog) = &mut self.release_now_dialog else {
            return;
        };
        if !dialog.is_running() {
            return;
        }
        if dialog.cancel_requested() {
            self.status = StatusMessage::warning("ReleaseNOW cancellation is already in progress.");
            return;
        }

        if let Some(cancel) = &self.current_release_now_cancel {
            cancel.cancel();
            dialog.mark_cancel_requested();
            self.status = StatusMessage::warning(
                "Cancelling ReleaseNOW. Waiting for the current step to stop.",
            );
        }
    }

    fn schedule_foreground_job(&mut self, request: BackgroundJobRequest) -> Result<u64> {
        if self.background_job_active {
            bail!("another background job is already running");
        }

        let request_id =
            self.schedule_background_job(BackgroundJobPriority::Foreground, request)?;
        self.background_job_active = true;
        self.active_foreground_job_id = Some(request_id);

        Ok(request_id)
    }

    fn schedule_progress_job(
        &mut self,
        title: impl Into<String>,
        message: impl Into<String>,
        request: BackgroundJobRequest,
    ) -> Result<()> {
        let request_id = self.schedule_foreground_job(request)?;

        self.progress_dialog = Some(ProgressDialog {
            title: title.into(),
            message: message.into(),
        });

        debug_assert_eq!(self.active_foreground_job_id, Some(request_id));

        Ok(())
    }

    fn try_finish_background_job(&mut self) -> Result<bool> {
        if self.background_jobs_inflight == 0 {
            return Ok(false);
        }

        let message = match self.background_result_rx.try_recv() {
            Ok(message) => message,
            Err(TryRecvError::Empty) => return Ok(false),
            Err(TryRecvError::Disconnected) => {
                self.progress_dialog = None;
                self.background_job_active = false;
                self.active_foreground_job_id = None;
                self.background_jobs_inflight = 0;
                bail!("background worker stopped unexpectedly");
            }
        };

        let terminal = matches!(message.payload, BackgroundJobMessagePayload::Finished(_));

        if terminal {
            self.background_jobs_inflight = self.background_jobs_inflight.saturating_sub(1);
            if self.active_foreground_job_id == Some(message.id) {
                self.progress_dialog = None;
                self.background_job_active = false;
                self.active_foreground_job_id = None;
            }

            if self.current_overview_activity_job_id == Some(message.id)
                && message.kind == BackgroundJobKind::OverviewActivity
            {
                self.overview_activity_job_inflight = false;
                if self.overview_activity_refresh_inflight {
                    self.overview_activity_refresh_inflight = false;
                    if self.overview_activity_refresh_pending {
                        self.overview_activity_refresh_pending = false;
                        self.schedule_refresh_overview_activity_cache()?;
                    }
                }
            }
        }

        if self.is_background_result_stale(&message) {
            return Ok(true);
        }

        match message.payload {
            BackgroundJobMessagePayload::Progress(output) => {
                self.apply_background_job_output(output)?
            }
            BackgroundJobMessagePayload::Finished(result) => match result {
                Ok(output) => self.apply_background_job_output(output)?,
                Err(error_message) => {
                    let release_now_error = if message.kind == BackgroundJobKind::ReleaseNow {
                        Some(rls_now::format_user_facing_error(&error_message))
                    } else {
                        None
                    };
                    if message.kind == BackgroundJobKind::ReleaseNow
                        && let Some(dialog) = &mut self.release_now_dialog
                    {
                        if rls_now::is_cancelled_error(&error_message) {
                            dialog.apply_cancelled(error_message.clone());
                        } else {
                            dialog.apply_failure(error_message.clone());
                        }
                    }
                    self.status = if message.kind == BackgroundJobKind::ReleaseNow
                        && rls_now::is_cancelled_error(&error_message)
                    {
                        StatusMessage::warning(error_message)
                    } else if let Some(formatted_error) = release_now_error {
                        StatusMessage::error(formatted_error)
                    } else {
                        StatusMessage::error(error_message)
                    };
                }
            },
        }

        Ok(true)
    }

    fn apply_background_job_output(&mut self, output: BackgroundJobOutput) -> Result<()> {
        match output {
            BackgroundJobOutput::OpenRecentChanges(dialog) => {
                self.recent_changes_dialog = Some(dialog);
                let _ = self.schedule_recent_changes_prefetch();
                self.status = StatusMessage::info("Showing git log for the selected project.");
            }
            BackgroundJobOutput::PendingBumpMainBranch {
                integration_mode,
                repos,
                pending_action,
            } => {
                if repos.is_empty() {
                    self.resume_pending_bump_action(pending_action)?;
                } else {
                    self.main_branch_warning_dialog = Some(MainBranchWarningDialog::new(
                        integration_mode,
                        repos,
                        pending_action,
                    ));
                    self.status = StatusMessage::warning(
                        "It seems like you are not on main branch. Please, choose what would you like to do...",
                    );
                }
            }
            BackgroundJobOutput::OverviewBumpWarnings {
                scope_index,
                workflow,
                warnings,
            } => {
                if warnings.is_empty() {
                    overview::continue_overview_bump_workflow_confirmation(
                        self,
                        scope_index,
                        workflow,
                    )?;
                } else {
                    self.overview_bump_warning_dialog = Some(OverviewBumpWarningDialog::new(
                        scope_index,
                        workflow,
                        warnings,
                    ));
                    self.status = StatusMessage::warning(
                        "Previously staged files were found. Review them before committing the bump.",
                    );
                }
            }
            BackgroundJobOutput::RecentChanges {
                dialog,
                status_message,
            } => {
                self.recent_changes_dialog = Some(dialog);
                let _ = self.schedule_recent_changes_prefetch();
                if let Some(message) = status_message {
                    self.status = StatusMessage::info(message);
                }
            }
            BackgroundJobOutput::RecentChangesPrefetch {
                project_name,
                next_scope_index,
                prefetched_recent_range,
                history_scope_index,
                prefetched_history_ranges,
            } => {
                if let Some(dialog) = &mut self.recent_changes_dialog
                    && dialog.project_name == project_name
                {
                    if let (Some(scope_index), Some(range)) =
                        (next_scope_index, prefetched_recent_range)
                    {
                        dialog.apply_prefetched_recent_range(scope_index, range);
                    }
                    if let (Some(scope_index), Some(ranges)) =
                        (history_scope_index, prefetched_history_ranges)
                    {
                        dialog.apply_prefetched_history_ranges(scope_index, ranges);
                    }
                }
            }
            BackgroundJobOutput::OpenChangelogPreview(dialog) => {
                self.open_changelog_preview(*dialog)
            }
            BackgroundJobOutput::OverviewActivityCache {
                project_index,
                summaries,
            } => {
                if self.selected_project == project_index {
                    self.overview_activity_summaries = summaries;
                    self.overview_activity_project = Some(project_index);
                }
            }
            BackgroundJobOutput::ReleaseNowValidated(validation) => {
                let project_name = validation.project_name.clone();
                let warning_pending = validation.warning_message.is_some();
                self.release_now_dialog =
                    Some(rls_now::ReleaseNowDialog::from_validation(validation));
                self.release_now_notes_dialog = None;
                self.status = if warning_pending {
                    StatusMessage::warning(
                        "ReleaseNOW found an older-than-expected bump. Confirm before continuing.",
                    )
                } else {
                    StatusMessage::info(format!("ReleaseNOW is ready for {}.", project_name))
                };
            }
            BackgroundJobOutput::ReleaseNowLogChunk(lines) => {
                if let Some(dialog) = &mut self.release_now_dialog {
                    dialog.append_log_lines(lines);
                }
            }
            BackgroundJobOutput::ReleaseNowCompleted(outcome) => {
                let summary = outcome.summary.clone();
                if let Some(dialog) = &mut self.release_now_dialog {
                    dialog.apply_outcome(outcome);
                }
                self.status = StatusMessage::success(summary);
            }
            BackgroundJobOutput::CreateTag {
                summary,
                replay_notices,
                replay_errors,
            } => {
                self.sync_dashboard_overview_after_repo_change();
                self.tag_dialog = None;
                self.tag_annotation_dialog = None;
                self.status = StatusMessage::success(summary);
                for notice in replay_notices {
                    self.show_transient_toast(StatusKind::Info, notice);
                }
                for error in replay_errors {
                    self.show_sticky_error_toast(error);
                }
            }
        }

        Ok(())
    }

    fn schedule_recent_changes_action(
        &mut self,
        message: impl Into<String>,
        action: RecentChangesLoadAction,
    ) -> Result<()> {
        let dialog = self
            .recent_changes_dialog
            .clone()
            .ok_or_else(|| anyhow!("git log is not open"))?;

        self.schedule_progress_job(
            " Loading Git Commits ",
            message,
            BackgroundJobRequest::RecentChanges { dialog, action },
        )
    }

    fn resolve_hit_target(
        &self,
        column: u16,
        row: u16,
        right_click: bool,
    ) -> Option<(HitAction, Rect)> {
        self.hit_targets
            .iter()
            .enumerate()
            .filter_map(|(index, target)| {
                if !target.contains(column, row) {
                    return None;
                }

                let action = if right_click {
                    target.right_action.clone()
                } else {
                    Some(target.action.clone())
                }?;

                if self.browser_dialog.is_some() && !matches!(action, HitAction::BrowserSelect(_)) {
                    return None;
                }

                if self.delete_confirmation_dialog.is_some()
                    && !matches!(
                        action,
                        HitAction::ConfirmDeleteRequest | HitAction::CancelDeleteRequest
                    )
                {
                    return None;
                }

                if self.overview_bump_kind_dialog.is_some()
                    && !matches!(
                        action,
                        HitAction::SelectOverviewBumpKind(_)
                            | HitAction::ConfirmOverviewBumpKind
                            | HitAction::CancelOverviewBumpKind
                    )
                {
                    return None;
                }

                if self.recent_changes_dialog.is_some()
                    && self.commit_rename_dialog.is_none()
                    && self.tag_dialog.is_none()
                    && self.tag_annotation_dialog.is_none()
                    && !matches!(
                        action,
                        HitAction::SelectRecentChangesTab(_)
                            | HitAction::CycleRecentChangesScope(_)
                            | HitAction::CloseRecentChanges
                            | HitAction::ScrollRecentChanges(_)
                            | HitAction::SelectRecentChangeLine(_, _)
                            | HitAction::OpenTagDialog
                    )
                {
                    return None;
                }

                if self.commit_rename_dialog.is_some()
                    && !matches!(
                        action,
                        HitAction::ToggleCommitRenameForcePush
                            | HitAction::SaveCommitRename
                            | HitAction::CancelCommitRename
                    )
                {
                    return None;
                }

                if self.release_now_notes_dialog.is_some()
                    && !matches!(
                        action,
                        HitAction::SaveReleaseNowNotes | HitAction::CancelReleaseNowNotes
                    )
                {
                    return None;
                }

                if self.release_now_dialog.is_some()
                    && self.release_now_notes_dialog.is_none()
                    && !matches!(
                        action,
                        HitAction::CycleReleaseNowOption(_)
                            | HitAction::ToggleReleaseNowChangelog
                            | HitAction::EditReleaseNowNotes
                            | HitAction::RunReleaseNow
                            | HitAction::ContinueReleaseNowWarning
                            | HitAction::ToggleReleaseNowAutoFollow
                            | HitAction::CancelReleaseNowRun
                            | HitAction::ScrollReleaseNow(_)
                            | HitAction::CloseReleaseNow
                    )
                {
                    return None;
                }

                if self.std_changelog_sub_branch_dialog.is_some()
                    && !matches!(action, HitAction::SelectStdChangelogSubBranchChoice(_))
                {
                    return None;
                }

                Some((
                    target.rect.width as u32 * target.rect.height as u32,
                    usize::MAX - index,
                    action,
                    target.rect,
                ))
            })
            .min_by_key(|(area, reverse_index, _, _)| (*area, *reverse_index))
            .map(|(_, _, action, rect)| (action, rect))
    }

    fn resolve_hit_action(&self, column: u16, row: u16, right_click: bool) -> Option<HitAction> {
        self.resolve_hit_target(column, row, right_click)
            .map(|(action, _)| action)
    }

    fn text_input_click_target(&self, action: &HitAction) -> Option<TextInputClickTarget> {
        Some(match action {
            HitAction::WizardField(field) => TextInputClickTarget::Wizard(*field),
            HitAction::EditProjectField(field) => TextInputClickTarget::ProjectEdit(*field),
            HitAction::SelectProjectSettingsField(field) => {
                TextInputClickTarget::ProjectSettings(*field)
            }
            _ => return None,
        })
    }

    fn recent_change_click_target(&self, action: &HitAction) -> Option<RecentChangeClickTarget> {
        match action {
            HitAction::SelectRecentChangeLine(view, line_index) => Some(RecentChangeClickTarget {
                view: *view,
                line_index: *line_index,
            }),
            _ => None,
        }
    }

    fn active_text_input_mut(&mut self) -> Option<&mut TextInput> {
        if matches!(self.screen, Screen::Wizard) {
            return self.wizard.active_input_mut();
        }

        if let Some(dialog) = &mut self.project_edit_dialog {
            return dialog.active_input_mut();
        }

        if self.screen == Screen::Dashboard && self.overview_tab == OverviewTab::ProjectSettings {
            return self.project_settings_state.active_input_mut();
        }

        None
    }

    fn set_text_input_cursor_from_mouse(&mut self, rect: Rect, column: u16) {
        if let Some(input) = self.active_text_input_mut() {
            let click_offset = column.saturating_sub(rect.x + FORM_LABEL_WIDTH) as usize;
            let field_width = rect.width.saturating_sub(FORM_LABEL_WIDTH) as usize;
            let cursor = input.cursor_position_at_click(click_offset, field_width, true);
            input.begin_selection_at(cursor);
        }
    }

    fn update_text_input_drag_selection(&mut self, rect: Rect, column: u16) {
        if let Some(input) = self.active_text_input_mut() {
            let click_offset = column.saturating_sub(rect.x + FORM_LABEL_WIDTH) as usize;
            let field_width = rect.width.saturating_sub(FORM_LABEL_WIDTH) as usize;
            let cursor = input.cursor_position_at_click(click_offset, field_width, true);
            input.set_cursor_position(cursor);
        }
    }

    fn schedule_overview_workflow_changelog_preview(
        &mut self,
        scope_index: usize,
        workflow: OverviewBumpWorkflow,
    ) -> Result<()> {
        let project = self.selected_project()?.clone();
        self.schedule_progress_job(
            " Generating Changelog ",
            "Building changelog preview from current git history.",
            BackgroundJobRequest::OpenOverviewWorkflowChangelog {
                project,
                scope_index,
                workflow,
                pending_versions: self.overview_pending_versions.clone(),
            },
        )?;
        self.status = StatusMessage::info("Generating changelog preview from current git history.");
        Ok(())
    }

    fn ensure_dashboard_recent_changes(&mut self) {
        overview::ensure_dashboard_recent_changes(self);
    }

    fn invalidate_overview_cache(&mut self) {
        overview::invalidate_overview_cache(self);
    }

    fn prime_selected_project_dashboard_data(&mut self) {
        self.ensure_dashboard_recent_changes();
        let _ = self.schedule_prefetch_overview_activity_cache();
    }

    fn next_poll_timeout(&self) -> Duration {
        if self.background_jobs_inflight > 0 || self.transient_toaster.has_toast() {
            ACTIVE_UI_TICK_INTERVAL
        } else {
            IDLE_UI_POLL_INTERVAL
        }
    }

    fn tick_ui_state(&mut self) -> bool {
        let had_transient_toast = self.transient_toaster.has_toast();
        let had_sticky_toast = self.sticky_toaster.has_toast();
        self.transient_toaster.tick();
        self.sticky_toaster.tick();

        had_transient_toast
            || self.transient_toaster.has_toast() != had_transient_toast
            || self.sticky_toaster.has_toast() != had_sticky_toast
            || overview::tick_dashboard_tile_rotation(self)
    }

    fn sync_dashboard_overview_after_repo_change(&mut self) {
        self.invalidate_overview_cache();
        self.ensure_dashboard_recent_changes();
        let _ = self.schedule_refresh_overview_activity_cache();
    }

    fn reload_dashboard_overview_data(&mut self) -> Result<()> {
        let project = self.selected_project()?;
        if !project.integration_mode.requires_repo() {
            self.status =
                StatusMessage::info("Selected project has no git-backed dashboard data to reload.");
            return Ok(());
        }

        let preferred_scope = self
            .overview_recent_changes
            .as_ref()
            .map(|dialog| dialog.selected_scope)
            .unwrap_or(self.overview_focused_scope);

        self.invalidate_overview_cache();
        self.ensure_dashboard_recent_changes();
        if let Some(dialog) = &mut self.overview_recent_changes {
            let scope_index = preferred_scope.min(dialog.scopes.len().saturating_sub(1));
            if scope_index != dialog.selected_scope {
                dialog.select_scope(scope_index)?;
            }
        }
        self.schedule_refresh_overview_activity_cache()?;
        self.status =
            StatusMessage::info("Refreshing dashboard repo data for the selected project.");
        Ok(())
    }

    fn reorder_dashboard_tile_scope(&mut self, from_scope: usize, to_scope: usize) {
        overview::reorder_dashboard_tile_scope(self, from_scope, to_scope);
    }

    fn scroll_dashboard_tiles(&mut self, delta: isize) -> Result<()> {
        overview::scroll_dashboard_tiles(self, delta)
    }

    fn cycle_overview_tile_info(
        &mut self,
        scope_index: usize,
        row: OverviewTileInfoRow,
    ) -> Result<()> {
        overview::cycle_overview_tile_info(self, scope_index, row)
    }

    fn move_dashboard_overview_focus(&mut self, delta: isize) -> Result<()> {
        overview::move_dashboard_overview_focus(self, delta)
    }

    fn select_dashboard_overview_scope(&mut self, scope_index: usize) -> Result<()> {
        overview::select_dashboard_overview_scope(self, scope_index)
    }

    fn begin_overview_bump(&mut self, scope_index: usize) -> Result<()> {
        overview::begin_overview_bump(self, scope_index)
    }

    fn select_overview_bump_kind(&mut self, index: usize) {
        overview::select_overview_bump_kind(self, index);
    }

    fn rotate_overview_bump_kind(&mut self, delta: isize) {
        overview::rotate_overview_bump_kind(self, delta);
    }

    fn cancel_overview_bump_kind(&mut self) {
        overview::cancel_overview_bump_kind(self);
    }

    fn confirm_overview_bump_kind(&mut self) -> Result<()> {
        overview::confirm_overview_bump_kind(self)
    }

    fn select_overview_bump_workflow(&mut self, index: usize) {
        overview::select_overview_bump_workflow(self, index);
    }

    fn rotate_overview_bump_workflow(&mut self, delta: isize) {
        overview::rotate_overview_bump_workflow(self, delta);
    }

    fn cancel_overview_bump_workflow(&mut self) {
        overview::cancel_overview_bump_workflow(self);
    }

    fn select_overview_bump_warning(&mut self, index: usize) {
        overview::select_overview_bump_warning(self, index);
    }

    fn rotate_overview_bump_warning(&mut self, delta: isize) {
        overview::rotate_overview_bump_warning(self, delta);
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
        overview::adjust_overview_pending_version(self, scope_index, control, delta)
    }

    fn reset_overview_pending_version(&mut self, scope_index: usize) -> Result<()> {
        overview::reset_overview_pending_version(self, scope_index)
    }

    fn open_dashboard_changelog_preview(
        &mut self,
        selection: Option<CustomChangelogSelection>,
    ) -> Result<()> {
        let project = self.selected_project()?.clone();
        let scope_index = if project.project_type == ProjectType::Branched {
            self.overview_focused_scope
                .min(project.branches.len().saturating_sub(1))
        } else {
            0
        };
        if !project.integration_mode.requires_repo() {
            bail!("changelog preview requires a git-backed project");
        }
        if !project.changelog_enabled_for_scope(scope_index) {
            bail!("changelog generation is disabled for the selected scope");
        }

        self.schedule_progress_job(
            " Generating Changelog ",
            "Building custom changelog preview.",
            BackgroundJobRequest::OpenDashboardChangelogPreview {
                project,
                scope_index,
                pending_versions: self.overview_pending_versions.clone(),
                selection,
            },
        )?;
        self.status = StatusMessage::info("Generating custom changelog preview.");
        Ok(())
    }

    fn request_confirm_overview_bump_workflow(&mut self) -> Result<()> {
        let Some(dialog) = &self.overview_bump_workflow_dialog else {
            return Ok(());
        };
        if !self.are_we_on_main(PendingBumpAction::OverviewWorkflow {
            scope_index: dialog.scope_index,
        })? {
            return Ok(());
        }
        self.confirm_overview_bump_workflow()
    }

    fn confirm_overview_bump_workflow(&mut self) -> Result<()> {
        overview::confirm_overview_bump_workflow(self)
    }

    fn confirm_overview_bump_warning(&mut self) -> Result<()> {
        overview::confirm_overview_bump_warning(self)
    }

    fn confirm_overview_branch_bump(&mut self) -> Result<()> {
        overview::confirm_overview_branch_bump(self)
    }

    fn cancel_overview_branch_bump(&mut self) {
        self.overview_branch_bump_dialog = None;
        self.status = StatusMessage::info("Tile bump action cancelled.");
    }

    fn are_we_on_main(&mut self, pending_action: PendingBumpAction) -> Result<bool> {
        let project = self.selected_project()?.clone();
        if !project.integration_mode.requires_repo() {
            return Ok(true);
        }

        let affected_scope_indexes =
            self.affected_scope_indexes_for_pending_bump(pending_action)?;
        if affected_scope_indexes.is_empty() {
            return Ok(true);
        }

        self.schedule_progress_job(
            " Checking Branch State ",
            "Checking repositories for non-main branches before continuing.",
            BackgroundJobRequest::CheckPendingBumpMainBranch {
                project,
                affected_scope_indexes,
                pending_action,
            },
        )?;
        self.status =
            StatusMessage::info("Checking repositories for non-main branches before continuing.");
        Ok(false)
    }

    fn affected_scope_indexes_for_pending_bump(
        &self,
        pending_action: PendingBumpAction,
    ) -> Result<Vec<usize>> {
        let project = self.selected_project()?;
        match pending_action {
            PendingBumpAction::Standard => {
                let dialog = self
                    .bump_dialog
                    .as_ref()
                    .ok_or_else(|| anyhow!("no bump preview is in progress"))?;
                if dialog.unified_versioning {
                    Ok((0..dialog.scopes.len()).collect())
                } else {
                    Ok(vec![dialog.selected_scope])
                }
            }
            PendingBumpAction::OverviewWorkflow { scope_index } => {
                if project.unified_versioning {
                    Ok((0..project.branches.len().max(1)).collect())
                } else {
                    Ok(vec![scope_index])
                }
            }
        }
    }

    fn select_main_branch_warning(&mut self, index: usize) {
        if let Some(dialog) = &mut self.main_branch_warning_dialog {
            dialog.select(index);
        }
    }

    fn rotate_main_branch_warning(&mut self, delta: isize) {
        if let Some(dialog) = &mut self.main_branch_warning_dialog {
            dialog.rotate(delta);
        }
    }

    fn cancel_main_branch_warning(&mut self) {
        self.main_branch_warning_dialog = None;
        self.status = StatusMessage::info("Bump cancelled.");
    }

    fn confirm_main_branch_warning(&mut self) -> Result<()> {
        let Some(dialog) = self.main_branch_warning_dialog.clone() else {
            return Ok(());
        };

        match dialog.selected_choice() {
            MainBranchWarningChoice::SwitchToMain => {
                git_flow::switch_repos_to_main(&dialog.repos, dialog.integration_mode)?;
                self.main_branch_warning_dialog = None;
                self.resume_pending_bump_action(dialog.pending_action)?;
            }
            MainBranchWarningChoice::IgnoreAndContinue => {
                self.main_branch_warning_dialog = None;
                self.resume_pending_bump_action(dialog.pending_action)?;
            }
            MainBranchWarningChoice::Cancel => self.cancel_main_branch_warning(),
        }
        Ok(())
    }

    fn open_changelog_preview(&mut self, dialog: ChangelogPreviewDialog) {
        self.pending_changelog_write = None;
        let preview_only = dialog.workflow.is_none();
        let custom_range = dialog.custom_range.is_some();
        self.changelog_preview_dialog = Some(dialog);
        self.status = StatusMessage::info(if preview_only {
            if custom_range {
                "Showing the custom changelog preview. Use Tab to switch From/To, Left/Right to change the range, and Ctrl+S to save changelog_temp.md."
            } else {
                "Showing the generated changelog preview for the current git history."
            }
        } else {
            "Review the generated changelog, add an optional release message, then confirm the bump."
        });
    }

    fn cancel_changelog_preview(&mut self) {
        self.changelog_preview_dialog = None;
        self.cancel_background_job_kind(BackgroundJobKind::ChangelogPreview);
        self.current_changelog_preview_job_id = None;
        self.pending_changelog_write = None;
        self.status = StatusMessage::info("Changelog preview closed.");
    }

    fn save_changelog_preview(&mut self) -> Result<()> {
        let Some(dialog) = self.changelog_preview_dialog.as_ref() else {
            return Ok(());
        };

        let release_message = dialog.release_message_value();
        let written_paths = dialog
            .entries
            .iter()
            .map(|entry| {
                let markdown = entry.rendered_markdown(&release_message);
                write_temp_changelog_markdown(&entry.repo_root, &markdown)
            })
            .collect::<Result<Vec<_>>>()?;

        self.status = if written_paths.len() == 1 {
            StatusMessage::success(format!(
                "Saved changelog preview to {}.",
                written_paths[0].display()
            ))
        } else {
            StatusMessage::success(format!(
                "Saved changelog previews to changelog_temp.md in {} repositories.",
                written_paths.len()
            ))
        };
        Ok(())
    }

    fn scroll_changelog_preview(&mut self, delta: i16) {
        if let Some(dialog) = &mut self.changelog_preview_dialog {
            let max_scroll = dialog
                .preview_line_count()
                .saturating_sub(1)
                .min(u16::MAX as usize) as u16;
            if delta.is_negative() {
                dialog.scroll = dialog.scroll.saturating_sub(delta.unsigned_abs());
            } else {
                dialog.scroll = dialog.scroll.saturating_add(delta as u16).min(max_scroll);
            }
        }
    }

    fn confirm_changelog_preview(&mut self) -> Result<()> {
        let Some(dialog) = self.changelog_preview_dialog.take() else {
            return Ok(());
        };

        if dialog.workflow.is_none() {
            self.cancel_background_job_kind(BackgroundJobKind::ChangelogPreview);
            self.current_changelog_preview_job_id = None;
            self.status = StatusMessage::info("Changelog preview closed.");
            return Ok(());
        }

        self.pending_changelog_write = Some(dialog.prepare_pending_write());
        self.cancel_background_job_kind(BackgroundJobKind::ChangelogPreview);
        self.current_changelog_preview_job_id = None;
        let branch_name = dialog
            .workflow
            .filter(|workflow| workflow.requires_branch())
            .and_then(|_| {
                self.overview_branch_bump_dialog
                    .as_ref()
                    .map(|branch_dialog| branch_dialog.branch_name.value.trim().to_string())
            });
        overview::execute_overview_bump_workflow(
            self,
            dialog.scope_index,
            dialog
                .workflow
                .expect("workflow preview should execute a workflow"),
            branch_name.as_deref(),
        )?;
        self.overview_bump_warning_dialog = None;
        self.overview_branch_bump_dialog = None;
        self.overview_bump_workflow_dialog = None;
        Ok(())
    }

    fn take_matching_pending_changelog_write(
        &mut self,
        scope_index: usize,
        workflow: OverviewBumpWorkflow,
    ) -> Option<PendingChangelogWrite> {
        let matches = self
            .pending_changelog_write
            .as_ref()
            .is_some_and(|pending| {
                pending.scope_index == scope_index && pending.workflow == workflow
            });
        if matches {
            self.pending_changelog_write.take()
        } else {
            None
        }
    }

    fn resume_pending_bump_action(&mut self, pending_action: PendingBumpAction) -> Result<()> {
        match pending_action {
            PendingBumpAction::Standard => self.apply_bump(),
            PendingBumpAction::OverviewWorkflow { .. } => self.confirm_overview_bump_workflow(),
        }
    }

    fn open_tag_dialog_with_scope(
        &mut self,
        preferred_scope: Option<usize>,
        preferred_action: Option<TagAction>,
    ) -> Result<()> {
        let project = self.selected_project()?.clone();
        let dialog = TagDialog::from_project(&project, preferred_scope, preferred_action)?;
        self.bump_dialog = None;
        self.project_edit_dialog = None;
        self.browser_dialog = None;
        self.tag_annotation_dialog = None;
        self.tag_dialog = Some(dialog);
        self.status = StatusMessage::info(
            "Review the proposed tag name, add an optional annotation, then run the tag action.",
        );
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

    fn scroll_project_settings(&mut self, delta: isize) {
        p_s_s::scroll_project_settings(self, delta);
    }

    fn select_browser_index(&mut self, index: usize) {
        let mut confirm_selection = false;
        if let Some(dialog) = &mut self.browser_dialog {
            let len = dialog.explorer.files().len();
            if len == 0 || index >= len {
                return;
            }
            let already_selected = dialog.explorer.selected_idx() == index;
            dialog.explorer.set_selected_idx(index);
            if already_selected {
                confirm_selection = true;
            }
        }
        if confirm_selection {
            let _ = self.confirm_browser_selection();
        }
    }

    fn open_project_edit_dialog(&mut self) -> Result<()> {
        let project_index = self.selected_project;
        let project = self.selected_project()?;
        let dialog = ProjectEditDialog::from_project(project_index, project)?;
        self.browser_dialog = None;
        self.project_edit_dialog = Some(dialog);
        self.status =
            StatusMessage::info("Amend project settings, then save or remove the project.");
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
        self.prime_selected_project_dashboard_data();
        p_s_s::invalidate_project_settings_state(self);
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

        self.request_project_deletion(dialog.project_index)
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
        let Some(dialog) = self.tag_dialog.clone() else {
            return Ok(());
        };
        let changelog_enabled = self
            .selected_project()?
            .changelog_enabled_for_scope(dialog.selected_scope);

        let tag_name = dialog.tag_name.value.trim();
        if tag_name.is_empty() {
            bail!("tag name cannot be empty");
        }

        let request = PendingTagRequest {
            dialog,
            changelog_enabled,
            std_changelog_policy: StdChangelogExecutionPolicy::Auto,
        };

        if let Some(warning_dialog) = self.build_std_changelog_sub_branch_dialog(&request)? {
            self.std_changelog_sub_branch_dialog = Some(warning_dialog);
            self.status = StatusMessage::warning(
                "Standard changelog is on a non-main sub-branch. Choose whether to generate now or postpone.",
            );
            return Ok(());
        }

        self.schedule_pending_tag_request(request)
    }

    fn build_std_changelog_sub_branch_dialog(
        &self,
        request: &PendingTagRequest,
    ) -> Result<Option<StdChangelogSubBranchDialog>> {
        if !request.changelog_enabled {
            return Ok(None);
        }

        let repo_root = &request.dialog.active_scope().repo_root;
        let branch_name = current_branch_with_cancel(repo_root, None)?;
        let Some(previous_tag) = latest_local_tag_with_cancel(repo_root, None)? else {
            return Ok(None);
        };
        let previous_branches =
            branches_containing_ref_with_cancel(repo_root, &previous_tag, None)?;
        let head_branches = branches_containing_ref_with_cancel(repo_root, "HEAD", None)?;
        let decision = decide_std_changelog_generation(
            &previous_tag,
            &branch_name,
            &previous_branches,
            &head_branches,
            request.dialog.active_scope().main_branch_name.as_deref(),
        );

        match decision {
            StdChangelogDecision::PostponeOnSubBranch(sub_branch) => Ok(Some(
                StdChangelogSubBranchDialog::new(request.clone(), previous_tag, sub_branch),
            )),
            _ => Ok(None),
        }
    }

    fn schedule_pending_tag_request(&mut self, request: PendingTagRequest) -> Result<()> {
        let tag_name = request.dialog.tag_name.value.trim().to_string();
        let message = match request.dialog.selected_action() {
            TagAction::CreateLocal => format!(
                "Creating local tag '{}' and generating release notes if needed.",
                tag_name
            ),
            TagAction::CreateAndPush => format!(
                "Creating and pushing tag '{}' for the selected scope.",
                tag_name
            ),
            TagAction::CreatePushAndRelease => format!(
                "Creating, pushing, and publishing tag '{}' with generated release notes.",
                tag_name
            ),
        };
        self.schedule_progress_job(
            " Running Tag Action ",
            message.clone(),
            BackgroundJobRequest::CreateTag {
                dialog: request.dialog,
                changelog_enabled: request.changelog_enabled,
                std_changelog_policy: request.std_changelog_policy,
            },
        )?;
        self.status = StatusMessage::info(message);
        Ok(())
    }

    fn select_std_changelog_sub_branch_warning(&mut self, index: usize) {
        if let Some(dialog) = &mut self.std_changelog_sub_branch_dialog {
            dialog.select(index);
        }
    }

    fn rotate_std_changelog_sub_branch_warning(&mut self, delta: isize) {
        if let Some(dialog) = &mut self.std_changelog_sub_branch_dialog {
            dialog.rotate(delta);
        }
    }

    fn cancel_std_changelog_sub_branch_warning(&mut self) {
        self.std_changelog_sub_branch_dialog = None;
        self.status =
            StatusMessage::info("Standard changelog decision cancelled. Tag dialog is still open.");
    }

    fn confirm_std_changelog_sub_branch_warning(&mut self) -> Result<()> {
        let Some(dialog) = self.std_changelog_sub_branch_dialog.clone() else {
            return Ok(());
        };

        match dialog.selected_choice() {
            StdChangelogSubBranchChoice::GenerateNow => {
                self.std_changelog_sub_branch_dialog = None;
                let mut request = dialog.pending_request;
                request.std_changelog_policy = StdChangelogExecutionPolicy::ForceGenerate;
                self.schedule_pending_tag_request(request)?;
            }
            StdChangelogSubBranchChoice::Postpone => {
                self.std_changelog_sub_branch_dialog = None;
                let mut request = dialog.pending_request;
                request.std_changelog_policy = StdChangelogExecutionPolicy::ForcePostpone;
                self.schedule_pending_tag_request(request)?;
            }
            StdChangelogSubBranchChoice::Cancel => self.cancel_std_changelog_sub_branch_warning(),
        }

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
        self.cancel_background_job_kind(BackgroundJobKind::RecentChanges);
        self.cancel_background_job_kind(BackgroundJobKind::RecentChangesPrefetch);
        self.current_recent_changes_job_id = None;
        self.tag_dialog = None;
        self.project_edit_dialog = None;
        self.browser_dialog = None;
        self.bump_dialog = Some(dialog);
        self.status = StatusMessage::info(
            "Review the preview, then press Enter to apply the bump for the active target set.",
        );
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

    fn request_apply_bump(&mut self) -> Result<()> {
        if !self.are_we_on_main(PendingBumpAction::Standard)? {
            return Ok(());
        }
        self.apply_bump()
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
            self.status = StatusMessage::info(
                "Version bump applied. Review the suggested tag-and-push action next.",
            );
        } else {
            self.sync_dashboard_overview_after_repo_change();
        }
        Ok(())
    }

    fn open_wizard(&mut self) {
        self.wizard = ProjectWizard::default();
        self.browser_dialog = None;
        self.screen = Screen::Wizard;
        self.status =
            StatusMessage::info("Configure a project and read each target file before saving.");
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
        let next = next as usize;
        if self.selected_project != next {
            self.selected_project = next;
            self.prime_selected_project_dashboard_data();
        }
    }

    fn request_dashboard_delete(&mut self) -> Result<()> {
        let Some(project) = self.config.projects.get(self.selected_project).cloned() else {
            self.status = StatusMessage::info("No project is selected.");
            return Ok(());
        };

        match project.project_type {
            ProjectType::AllInOne => self.request_project_deletion(self.selected_project),
            ProjectType::Branched => {
                if project.branches.is_empty() {
                    return self.request_project_deletion(self.selected_project);
                }

                let scope_index = self
                    .overview_focused_scope
                    .min(project.branches.len().saturating_sub(1));
                self.request_scope_deletion(self.selected_project, scope_index)
            }
        }
    }

    fn request_project_deletion(&mut self, project_index: usize) -> Result<()> {
        let project = self
            .config
            .projects
            .get(project_index)
            .ok_or_else(|| anyhow!("selected project no longer exists"))?;
        self.delete_confirmation_dialog = Some(DeleteConfirmationDialog::project(
            project_index,
            project.name.clone(),
        ));
        self.status =
            StatusMessage::warning(format!("Confirm deletion of project '{}'.", project.name));
        Ok(())
    }

    fn request_scope_deletion(&mut self, project_index: usize, scope_index: usize) -> Result<()> {
        let project = self
            .config
            .projects
            .get(project_index)
            .ok_or_else(|| anyhow!("selected project no longer exists"))?;
        if project.project_type != ProjectType::Branched {
            return self.request_project_deletion(project_index);
        }

        let branch = project
            .branches
            .get(scope_index)
            .ok_or_else(|| anyhow!("selected scope no longer exists"))?;
        self.delete_confirmation_dialog = Some(DeleteConfirmationDialog::scope(
            project_index,
            project.name.clone(),
            scope_index,
            branch.display_name().to_string(),
            branch.scope_kind,
            project.branches.len() == 1,
        ));
        self.status = StatusMessage::warning(format!(
            "Confirm deletion of scope '{}' from project '{}'.",
            branch.display_name(),
            project.name
        ));
        Ok(())
    }

    fn confirm_delete_request(&mut self) -> Result<()> {
        let Some(dialog) = self.delete_confirmation_dialog.clone() else {
            return Ok(());
        };
        self.delete_confirmation_dialog = None;

        match dialog.target {
            DeleteConfirmationTarget::Project { project_index, .. } => {
                self.delete_project_at(project_index)
            }
            DeleteConfirmationTarget::Scope {
                project_index,
                scope_index,
                removes_project,
                ..
            } => {
                if removes_project {
                    self.delete_last_scope_project(project_index, scope_index)
                } else {
                    self.delete_scope_at(project_index, scope_index)
                }
            }
        }
    }

    fn cancel_delete_request(&mut self) {
        self.delete_confirmation_dialog = None;
        self.status = StatusMessage::info("Deletion cancelled.");
    }

    fn delete_project_at(&mut self, project_index: usize) -> Result<()> {
        if project_index >= self.config.projects.len() {
            bail!("selected project no longer exists");
        }

        let removed = self.config.projects.remove(project_index);
        self.finish_delete_mutation(project_index)?;
        self.status = StatusMessage::success(format!("Removed project '{}'.", removed.name));
        Ok(())
    }

    fn delete_scope_at(&mut self, project_index: usize, scope_index: usize) -> Result<()> {
        let (project_name, scope_name, remaining_scopes) = {
            let project = self
                .config
                .projects
                .get_mut(project_index)
                .ok_or_else(|| anyhow!("selected project no longer exists"))?;
            if project.project_type != ProjectType::Branched {
                bail!("selected project does not contain removable scopes");
            }
            if scope_index >= project.branches.len() {
                bail!("selected scope no longer exists");
            }
            let removed = project.branches.remove(scope_index);
            (
                project.name.clone(),
                removed.display_name().to_string(),
                project.branches.len(),
            )
        };

        self.finish_delete_mutation(project_index)?;
        if remaining_scopes > 0 {
            self.overview_focused_scope = scope_index.min(remaining_scopes.saturating_sub(1));
        }
        self.status = StatusMessage::success(format!(
            "Removed scope '{}' from project '{}'.",
            scope_name, project_name
        ));
        Ok(())
    }

    fn delete_last_scope_project(
        &mut self,
        project_index: usize,
        scope_index: usize,
    ) -> Result<()> {
        let (project_name, scope_name) = {
            let project = self
                .config
                .projects
                .get(project_index)
                .ok_or_else(|| anyhow!("selected project no longer exists"))?;
            if project.project_type != ProjectType::Branched {
                bail!("selected project does not contain removable scopes");
            }
            let branch = project
                .branches
                .get(scope_index)
                .ok_or_else(|| anyhow!("selected scope no longer exists"))?;
            (project.name.clone(), branch.display_name().to_string())
        };

        self.config.projects.remove(project_index);
        self.finish_delete_mutation(project_index)?;
        self.status = StatusMessage::success(format!(
            "Removed scope '{}' and deleted project '{}' because it had no scopes left.",
            scope_name, project_name
        ));
        Ok(())
    }

    fn finish_delete_mutation(&mut self, selected_index_hint: usize) -> Result<()> {
        self.config_store.save(&self.config)?;
        self.project_edit_dialog = None;
        self.browser_dialog = None;
        if self.config.projects.is_empty() {
            self.selected_project = 0;
            self.overview_focused_scope = 0;
        } else {
            self.selected_project =
                selected_index_hint.min(self.config.projects.len().saturating_sub(1));
        }
        self.invalidate_overview_cache();
        self.prime_selected_project_dashboard_data();
        p_s_s::invalidate_project_settings_state(self);
        Ok(())
    }

    fn validate_wizard_target(&mut self) {
        let (target_path, target_key) = if self.wizard.project_type == ProjectType::Branched {
            self.wizard
                .current_scope()
                .map(|scope| {
                    (
                        scope.target_path.value().trim().to_string(),
                        scope.target_key.value().trim().to_string(),
                    )
                })
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
                    ProbeKind::Success => StatusMessage::success(
                        "Target file is readable and the selected key matches the chosen scheme.",
                    ),
                    ProbeKind::Warning => StatusMessage::warning(
                        "Target file is readable, but the detected version does not match the chosen scheme.",
                    ),
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
        if let Some(repo) = project.repo.as_ref() {
            ensure_gitignore_entry(&repo.local_root, ".comfygit/syncmem/stdchlg-local.json")?;
        }
        self.config.projects.push(project);
        self.config_store.save(&self.config)?;
        self.selected_project = self.config.projects.len().saturating_sub(1);
        self.invalidate_overview_cache();
        self.prime_selected_project_dashboard_data();
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
        self.status =
            StatusMessage::info("Browse to a file or directory, then press Enter to select it.");
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
            BrowseTarget::ProjectSettingsChangelogPath
            | BrowseTarget::ProjectSettingsReleaseNowWindows
            | BrowseTarget::ProjectSettingsReleaseNowLinuxArm
            | BrowseTarget::ProjectSettingsReleaseNowLinuxAmd
            | BrowseTarget::ProjectSettingsReleaseNowMacOs => {
                p_s_s::initial_browser_path(self, target).unwrap_or_default()
            }
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
            self.status = StatusMessage::warning(
                "Select a directory for Repo root, or press U to use the current file's folder.",
            );
            return Ok(());
        }

        if !select_directories && !selected.is_file() {
            self.status = StatusMessage::warning(
                "Select a file for Target path. Use Right to enter directories.",
            );
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
            BrowseTarget::ProjectSettingsChangelogPath
            | BrowseTarget::ProjectSettingsReleaseNowWindows
            | BrowseTarget::ProjectSettingsReleaseNowLinuxArm
            | BrowseTarget::ProjectSettingsReleaseNowLinuxAmd
            | BrowseTarget::ProjectSettingsReleaseNowMacOs => {
                if p_s_s::apply_browser_selection(self, target, selected)? {
                    self.browser_dialog = None;
                    self.status = StatusMessage::success("Selection applied.");
                    return Ok(());
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

    fn update_footer_visibility(&mut self, viewport_height: u16) {
        if viewport_height <= 25 {
            if !self.config.ui.hide_footer && !self.footer_manual_override {
                self.config.ui.hide_footer = true;
                self.footer_auto_hidden = true;
            }
        } else if self.footer_auto_hidden {
            self.config.ui.hide_footer = false;
            self.footer_auto_hidden = false;
        }
    }

    fn toggle_footer(&mut self) -> Result<()> {
        if self.footer_auto_hidden {
            self.footer_auto_hidden = false;
        }
        self.footer_manual_override = true;
        self.config.ui.hide_footer = !self.config.ui.hide_footer;
        self.config_store.save(&self.config)?;
        self.status = StatusMessage::success(if self.config.ui.hide_footer {
            "Footer hidden. Press H to show it again."
        } else {
            "Footer shown."
        });
        Ok(())
    }

    fn cycle_footer_content(&mut self, delta: i32) -> Result<()> {
        self.config.ui.footer_content = if delta >= 0 {
            self.config.ui.footer_content.next()
        } else {
            self.config.ui.footer_content.previous()
        };
        self.config_store.save(&self.config)?;
        self.status = StatusMessage::success(format!(
            "Footer content alignment set to {}.",
            self.config.ui.footer_content.display_name()
        ));
        Ok(())
    }

    fn handle_tab_shortcut(&mut self, key: KeyEvent) -> bool {
        if !key.modifiers.is_empty() {
            return false;
        }

        if matches!(self.screen, Screen::Wizard) && self.wizard.focus_accepts_text() {
            return false;
        }

        if self.screen == Screen::Dashboard && self.dashboard_focus == DashboardPane::Overview {
            let target = if let KeyCode::Char(digit @ '1'..='4') = key.code {
                let index = (digit as u8 - b'1') as usize;
                overview_tabs(self.overview_show_recent_tab)
                    .get(index)
                    .copied()
            } else {
                None
            };
            if let Some(target) = target {
                self.overview_tab = target;
                return true;
            }
        }

        let target = match key.code {
            KeyCode::Char('1') => Some(Screen::Dashboard),
            KeyCode::Char('2') => Some(Screen::Wizard),
            KeyCode::Char('3') => Some(Screen::UiSettings),
            _ => None,
        };

        let Some(target) = target else {
            return false;
        };

        match target {
            Screen::Wizard => self.open_wizard(),
            Screen::Dashboard => {
                self.screen = Screen::Dashboard;
                self.dashboard_focus = DashboardPane::Projects;
            }
            _ => self.screen = target,
        }
        true
    }

    fn try_handle_ui_shortcut(&mut self, key: KeyEvent) -> Result<bool> {
        if key.modifiers.is_empty() && matches!(key.code, KeyCode::Char('h') | KeyCode::Char('H')) {
            if matches!(self.screen, Screen::Wizard) && self.wizard.focus_accepts_text() {
                return Ok(false);
            }
            if self
                .project_edit_dialog
                .as_ref()
                .map(|dialog| dialog.focus_accepts_text())
                .unwrap_or(false)
            {
                return Ok(false);
            }
            if p_s_s::captures_text_input(self) {
                return Ok(false);
            }
            self.toggle_footer()?;
            return Ok(true);
        }
        Ok(false)
    }

    fn toggle_dashboard_focus(&mut self) {
        self.dashboard_focus = match self.dashboard_focus {
            DashboardPane::Projects => DashboardPane::Overview,
            DashboardPane::Overview => DashboardPane::Projects,
        };
    }

    fn scroll_dashboard_recent_changes(&mut self, delta: i16) -> bool {
        if let Some(dialog) = &mut self.overview_recent_changes {
            dialog.scroll_by(delta);
            true
        } else {
            false
        }
    }

    fn open_commit_rename_from_view(&mut self, view: RecentChangeView) -> Result<()> {
        let (repo_root, commit_hash) = match view {
            RecentChangeView::Popup => {
                let dialog = self
                    .recent_changes_dialog
                    .as_ref()
                    .ok_or_else(|| anyhow!("the git log popup is not open"))?;
                (
                    dialog.active_scope().repo_root.clone(),
                    dialog.selected_commit_hash().ok_or_else(|| {
                        anyhow!("select a commit line before renaming its message")
                    })?,
                )
            }
            RecentChangeView::Overview => {
                let dialog = self.overview_recent_changes.as_ref().ok_or_else(|| {
                    anyhow!("recent changes are not available for the selected project")
                })?;
                (
                    dialog.active_scope().repo_root.clone(),
                    dialog.selected_commit_hash().ok_or_else(|| {
                        anyhow!("select a commit line before renaming its message")
                    })?,
                )
            }
        };

        let plan = prepare_commit_rename(&repo_root, &commit_hash)?;
        self.commit_rename_dialog = Some(CommitRenameDialog::new(view, plan));
        self.status = StatusMessage::info("Edit the commit message and press Enter to save it.");
        Ok(())
    }

    fn toggle_commit_rename_force_push(&mut self) {
        if let Some(dialog) = &mut self.commit_rename_dialog
            && dialog.plan.touches_pushed_history
        {
            dialog.push_after_rename = !dialog.push_after_rename;
        }
    }

    fn apply_commit_rename(&mut self) -> Result<()> {
        let Some(dialog) = self.commit_rename_dialog.take() else {
            return Ok(());
        };

        let outcome = match rename_commit_with_subject(&dialog.plan, &dialog.message_input.value) {
            Ok(outcome) => outcome,
            Err(error) => {
                self.commit_rename_dialog = Some(dialog);
                return Err(error);
            }
        };

        if dialog.push_after_rename
            && dialog.plan.touches_pushed_history
            && let Err(error) = push_branch_force_with_lease(&dialog.plan.repo_root)
        {
            self.commit_rename_dialog = Some(dialog);
            return Err(error);
        }

        self.sync_dashboard_overview_after_repo_change();
        if let Some(recent_dialog) = &mut self.recent_changes_dialog {
            let _ = recent_dialog.refresh_current_scope_cancellable(None);
        }

        let mut summary = format!(
            "Renamed {} to '{}'.",
            outcome.target_commit, outcome.new_subject
        );
        if dialog.push_after_rename && dialog.plan.touches_pushed_history {
            summary.push_str(" Force-pushed with --force-with-lease.");
        }
        self.status = StatusMessage::success(summary);
        Ok(())
    }

    fn select_recent_change_line(&mut self, view: RecentChangeView, line_index: usize) {
        match view {
            RecentChangeView::Popup => {
                if let Some(dialog) = &mut self.recent_changes_dialog {
                    dialog.select_line(line_index);
                }
            }
            RecentChangeView::Overview => {
                if let Some(dialog) = &mut self.overview_recent_changes {
                    dialog.select_line(line_index);
                }
            }
        }
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
                let interaction = self.sticky_toaster.handle_click(
                    mouse.column,
                    mouse.row,
                    ToastMouseButton::Left,
                );
                self.handle_toast_interaction(interaction)
            }
            MouseEventKind::Down(MouseButton::Right) => {
                let interaction = self.sticky_toaster.handle_click(
                    mouse.column,
                    mouse.row,
                    ToastMouseButton::Right,
                );
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
            StatusKind::Info => self
                .transient_toaster
                .show_toast(builder.toast_type(ToastType::Info)),
            StatusKind::Success => self
                .transient_toaster
                .show_toast(builder.toast_type(ToastType::Success)),
            StatusKind::Warning => self
                .transient_toaster
                .show_toast(builder.toast_type(ToastType::Warning)),
            StatusKind::Error => self
                .sticky_toaster
                .show_toast(builder.toast_type(ToastType::Error).keep_on(1)),
        }
    }

    fn show_transient_toast(&mut self, kind: StatusKind, text: impl Into<String>) {
        let builder = ToastBuilder::new(text.into().into());
        match kind {
            StatusKind::Info => self
                .transient_toaster
                .show_toast(builder.toast_type(ToastType::Info)),
            StatusKind::Success => self
                .transient_toaster
                .show_toast(builder.toast_type(ToastType::Success)),
            StatusKind::Warning => self
                .transient_toaster
                .show_toast(builder.toast_type(ToastType::Warning)),
            StatusKind::Error => self
                .sticky_toaster
                .show_toast(builder.toast_type(ToastType::Error).keep_on(1)),
        }
    }

    fn show_sticky_error_toast(&mut self, text: impl Into<String>) {
        self.sticky_toaster.show_toast(
            ToastBuilder::new(text.into().into())
                .toast_type(ToastType::Error)
                .keep_on(1),
        );
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Screen {
    Dashboard,
    Wizard,
    UiSettings,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DashboardPane {
    Projects,
    Overview,
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum TextInputClickTarget {
    Wizard(WizardField),
    ProjectEdit(ProjectEditFocus),
    ProjectSettings(ProjectSettingsFocus),
}

impl TextInputClickTarget {
    fn same_field_action(&self, action: &HitAction) -> bool {
        match (self, action) {
            (&TextInputClickTarget::Wizard(a), &HitAction::WizardField(b)) => a == b,
            (&TextInputClickTarget::ProjectEdit(a), &HitAction::EditProjectField(b)) => a == b,
            (
                &TextInputClickTarget::ProjectSettings(a),
                &HitAction::SelectProjectSettingsField(b),
            ) => a == b,
            _ => false,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum RecentChangeView {
    Overview,
    Popup,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct RecentChangeClickTarget {
    view: RecentChangeView,
    line_index: usize,
}

#[derive(Clone)]
pub(crate) enum HitAction {
    Switch(Screen),
    SelectOverviewTab(OverviewTab),
    SelectProjectSettingsTab(ProjectSettingsTab),
    SelectProjectSettingsField(ProjectSettingsFocus),
    SelectProject(usize),
    SelectOverviewScope(usize),
    BrowseProjectSettingsField(ProjectSettingsFocus),
    OpenOverviewReleaseNow(usize),
    BeginOverviewBump(usize),
    CycleOverviewTileInfo(usize, OverviewTileInfoRow),
    SelectOverviewBumpKind(usize),
    ConfirmOverviewBumpKind,
    CancelOverviewBumpKind,
    SelectOverviewBumpWorkflow(usize),
    ConfirmOverviewBumpWorkflow,
    CancelOverviewBumpWorkflow,
    SelectOverviewBumpWarningChoice(usize),
    SelectMainBranchWarningChoice(usize),
    SelectStdChangelogSubBranchChoice(usize),
    ConfirmChangelogPreview,
    SaveChangelogPreview,
    CancelChangelogPreview,
    ScrollChangelogPreview(i16),
    AdjustOverviewVersion(usize, OverviewVersionControl, i32),
    ResetOverviewPendingVersion(usize),
    OpenOverviewTagDialog(usize),
    EditProjectField(ProjectEditFocus),
    ProjectEditScopeAction(ScopeAction),
    SaveProjectEdit,
    RemoveProject,
    CancelProjectEdit,
    CycleReleaseNowOption(isize),
    ToggleReleaseNowChangelog,
    EditReleaseNowNotes,
    RunReleaseNow,
    ContinueReleaseNowWarning,
    ToggleReleaseNowAutoFollow,
    CancelReleaseNowRun,
    ScrollReleaseNow(i16),
    SaveReleaseNowNotes,
    CancelReleaseNowNotes,
    CloseReleaseNow,
    ConfirmDeleteRequest,
    CancelDeleteRequest,
    ToggleTabHints,
    ToggleFooter,
    CycleFooterContent(i32),
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
    SelectRecentChangeLine(RecentChangeView, usize),
    ToggleCommitRenameForcePush,
    SaveCommitRename,
    CancelCommitRename,
    OpenTagDialog,
    OpenTagAnnotation,
    CycleTagScope(isize),
    CycleTagAction(isize),
    CycleBumpAction(isize),
    CycleBumpScope(isize),
    ApplyBump,
    CancelBump,
    ConfirmOverviewBranchBump,
    CancelOverviewBranchBump,
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
pub(crate) enum ScopeAction {
    Add,
    Remove,
    MoveUp,
    MoveDown,
}

#[derive(Clone, Copy)]
pub(crate) enum OverviewVersionControl {
    Major,
    Minor,
    Patch,
    Whole,
}

#[derive(Clone, Copy)]
pub(crate) enum OverviewTileInfoRow {
    Dev,
    Release,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CustomChangelogRangeFocus {
    From,
    To,
}

#[derive(Clone)]
struct CustomChangelogSelection {
    from_ref: String,
    to_ref: Option<String>,
}

#[derive(Clone)]
struct CustomChangelogRangeState {
    scope_name: String,
    tags: Vec<String>,
    from_index: usize,
    to_index: Option<usize>,
    focus: CustomChangelogRangeFocus,
}

impl CustomChangelogRangeState {
    fn new(
        scope_name: String,
        tags: Vec<String>,
        selection: Option<CustomChangelogSelection>,
    ) -> Self {
        let mut state = Self {
            scope_name,
            tags,
            from_index: 0,
            to_index: None,
            focus: CustomChangelogRangeFocus::From,
        };

        if let Some(selection) = selection {
            if let Some(from_index) = state.tags.iter().position(|tag| tag == &selection.from_ref) {
                state.from_index = from_index;
            }
            state.to_index = selection
                .to_ref
                .as_ref()
                .and_then(|to_ref| state.tags.iter().position(|tag| tag == to_ref))
                .filter(|to_index| *to_index < state.from_index);
        }

        state.ensure_valid_to_index();
        state
    }

    fn has_tags(&self) -> bool {
        !self.tags.is_empty()
    }

    fn focus_label(&self, focus: CustomChangelogRangeFocus) -> &'static str {
        if self.focus == focus { ">" } else { " " }
    }

    fn current_from_ref(&self) -> Option<&str> {
        self.tags.get(self.from_index).map(String::as_str)
    }

    fn current_to_ref(&self) -> &str {
        self.to_index
            .and_then(|index| self.tags.get(index))
            .map(String::as_str)
            .unwrap_or("HEAD")
    }

    fn range_label(&self) -> String {
        self.current_from_ref()
            .map(|from_ref| format!("{}..{}", from_ref, self.current_to_ref()))
            .unwrap_or_else(|| "no tags found; showing the latest 60 commits".to_string())
    }

    fn selection(&self) -> Option<CustomChangelogSelection> {
        Some(CustomChangelogSelection {
            from_ref: self.current_from_ref()?.to_string(),
            to_ref: self
                .to_index
                .and_then(|index| self.tags.get(index))
                .cloned(),
        })
    }

    fn cycle_focus(&mut self, delta: isize) {
        let focuses = [
            CustomChangelogRangeFocus::From,
            CustomChangelogRangeFocus::To,
        ];
        let current = match self.focus {
            CustomChangelogRangeFocus::From => 0,
            CustomChangelogRangeFocus::To => 1,
        } as isize;
        let next = (current + delta).rem_euclid(focuses.len() as isize) as usize;
        self.focus = focuses[next];
    }

    fn select_focus(&mut self, focus: CustomChangelogRangeFocus) {
        self.focus = focus;
    }

    fn adjust_focused_selection(&mut self, delta: isize) -> bool {
        if !self.has_tags() || delta == 0 {
            return false;
        }

        match self.focus {
            CustomChangelogRangeFocus::From => self.adjust_from(delta),
            CustomChangelogRangeFocus::To => self.adjust_to(delta),
        }
    }

    fn display_from(&self) -> String {
        self.current_from_ref().unwrap_or("<no tags>").to_string()
    }

    fn display_to(&self) -> String {
        self.current_to_ref().to_string()
    }

    fn ensure_valid_to_index(&mut self) {
        if self.tags.is_empty() {
            self.from_index = 0;
            self.to_index = None;
            return;
        }

        self.from_index = self.from_index.min(self.tags.len().saturating_sub(1));
        if self.from_index == 0 {
            self.to_index = None;
        } else if self
            .to_index
            .is_some_and(|to_index| to_index >= self.from_index)
        {
            self.to_index = Some(self.from_index - 1);
        }
    }

    fn adjust_from(&mut self, delta: isize) -> bool {
        let len = self.tags.len();
        if len == 0 {
            return false;
        }

        let next =
            (self.from_index as isize + delta).clamp(0, len.saturating_sub(1) as isize) as usize;
        if next == self.from_index {
            return false;
        }

        self.from_index = next;
        self.ensure_valid_to_index();
        true
    }

    fn adjust_to(&mut self, delta: isize) -> bool {
        let max_position = self.from_index;
        let current_position = self.to_index.map(|to_index| to_index + 1).unwrap_or(0);
        let next_position =
            (current_position as isize + delta).clamp(0, max_position as isize) as usize;
        if next_position == current_position {
            return false;
        }

        self.to_index = if next_position == 0 {
            None
        } else {
            Some(next_position - 1)
        };
        true
    }
}

struct ChangelogPreviewDialog {
    project_name: String,
    next_version: String,
    scope_index: usize,
    workflow: Option<OverviewBumpWorkflow>,
    custom_range: Option<CustomChangelogRangeState>,
    entries: Vec<ChangelogPreviewEntry>,
    release_message: TuiTextArea<'static>,
    release_message_placeholder: String,
    scroll: u16,
}

#[derive(Clone)]
struct DeleteConfirmationDialog {
    target: DeleteConfirmationTarget,
    confirm_selected: bool,
}

impl DeleteConfirmationDialog {
    fn project(project_index: usize, project_name: String) -> Self {
        Self {
            target: DeleteConfirmationTarget::Project {
                project_index,
                project_name,
            },
            confirm_selected: false,
        }
    }

    fn scope(
        project_index: usize,
        project_name: String,
        scope_index: usize,
        scope_name: String,
        scope_kind: BranchScopeKind,
        removes_project: bool,
    ) -> Self {
        Self {
            target: DeleteConfirmationTarget::Scope {
                project_index,
                project_name,
                scope_index,
                scope_name,
                scope_kind,
                removes_project,
            },
            confirm_selected: false,
        }
    }

    fn toggle_selection(&mut self) {
        self.confirm_selected = !self.confirm_selected;
    }
}

#[derive(Clone)]
enum DeleteConfirmationTarget {
    Project {
        project_index: usize,
        project_name: String,
    },
    Scope {
        project_index: usize,
        project_name: String,
        scope_index: usize,
        scope_name: String,
        scope_kind: BranchScopeKind,
        removes_project: bool,
    },
}

impl ChangelogPreviewDialog {
    fn new(
        project_name: String,
        next_version: String,
        scope_index: usize,
        workflow: OverviewBumpWorkflow,
        entries: Vec<ChangelogPreviewEntry>,
    ) -> Self {
        Self {
            project_name,
            next_version,
            scope_index,
            workflow: Some(workflow),
            custom_range: None,
            entries,
            release_message: new_release_message_editor(""),
            release_message_placeholder: "Optional multi-line release notes in Markdown"
                .to_string(),
            scroll: 0,
        }
    }

    fn preview_only(
        project_name: String,
        next_version: String,
        scope_index: usize,
        custom_range: Option<CustomChangelogRangeState>,
        entries: Vec<ChangelogPreviewEntry>,
    ) -> Self {
        Self {
            project_name,
            next_version,
            scope_index,
            workflow: None,
            custom_range,
            entries,
            release_message: new_release_message_editor(""),
            release_message_placeholder: "Optional multi-line release notes in Markdown"
                .to_string(),
            scroll: 0,
        }
    }

    fn release_message_value(&self) -> String {
        let release_message = self.release_message.lines().join("\n");
        if release_message.trim().is_empty() {
            String::new()
        } else {
            release_message
        }
    }

    fn combined_preview_markdown(&self) -> String {
        let release_message = self.release_message_value();
        let mut lines = Vec::new();
        for (index, entry) in self.entries.iter().enumerate() {
            if self.entries.len() > 1 {
                lines.push(format!("### Repo: {}", entry.repo_root));
                lines.push(format!("Path: `{}`", entry.changelog_path));
                lines.push(String::new());
            }

            let rendered = entry.rendered_markdown(&release_message);
            lines.extend(rendered.lines().map(ToOwned::to_owned));
            if index + 1 < self.entries.len() {
                lines.push(String::new());
            }
        }
        lines.join("\n")
    }

    fn preview_line_count(&self) -> usize {
        tui_markdown::from_str(&self.combined_preview_markdown())
            .lines
            .len()
    }

    fn prepare_pending_write(&self) -> PendingChangelogWrite {
        let release_message = self.release_message_value();
        PendingChangelogWrite {
            scope_index: self.scope_index,
            workflow: self
                .workflow
                .expect("workflow preview required to prepare changelog write"),
            entries: self
                .entries
                .iter()
                .map(|entry| PreparedChangelogEntry {
                    repo_root: entry.repo_root.clone(),
                    changelog_path: entry.changelog_path.clone(),
                    stage_path: entry.stage_path.clone(),
                    markdown: entry.rendered_markdown(&release_message),
                })
                .collect(),
        }
    }
}

fn new_release_message_editor(existing_release_message: &str) -> TuiTextArea<'static> {
    let mut editor = if existing_release_message.trim().is_empty() {
        TuiTextArea::default()
    } else {
        TuiTextArea::from(existing_release_message.lines())
    };
    editor.set_placeholder_text("Optional multi-line release notes in Markdown");
    editor.set_tab_length(2);
    editor.set_max_histories(100);
    editor
}

#[derive(Clone)]
struct ChangelogPreviewEntry {
    repo_root: String,
    changelog_path: String,
    stage_path: String,
    document: ChangelogDocument,
}

impl ChangelogPreviewEntry {
    fn rendered_markdown(&self, release_message: &str) -> String {
        let document = if release_message.trim().is_empty() {
            self.document.clone()
        } else {
            self.document
                .clone()
                .with_release_message(release_message.to_string())
        };
        document.render_markdown().markdown
    }
}

#[derive(Clone)]
struct PreparedChangelogEntry {
    repo_root: String,
    changelog_path: String,
    stage_path: String,
    markdown: String,
}

#[derive(Clone)]
struct PendingChangelogWrite {
    scope_index: usize,
    workflow: OverviewBumpWorkflow,
    entries: Vec<PreparedChangelogEntry>,
}

struct ProgressDialog {
    title: String,
    message: String,
}

enum BackgroundJobRequest {
    OpenRecentChanges {
        project: ProjectConfig,
        preferred_scope: Option<usize>,
    },
    CheckPendingBumpMainBranch {
        project: ProjectConfig,
        affected_scope_indexes: Vec<usize>,
        pending_action: PendingBumpAction,
    },
    CheckOverviewBumpWarnings {
        project: ProjectConfig,
        scope_index: usize,
        workflow: OverviewBumpWorkflow,
    },
    RecentChanges {
        dialog: RecentChangesDialog,
        action: RecentChangesLoadAction,
    },
    OpenDashboardChangelogPreview {
        project: ProjectConfig,
        scope_index: usize,
        pending_versions: Vec<String>,
        selection: Option<CustomChangelogSelection>,
    },
    OpenOverviewWorkflowChangelog {
        project: ProjectConfig,
        scope_index: usize,
        workflow: OverviewBumpWorkflow,
        pending_versions: Vec<String>,
    },
    RefreshOverviewActivity {
        project_index: usize,
        project: ProjectConfig,
    },
    ValidateReleaseNow {
        project: ProjectConfig,
        scope_index: usize,
    },
    RunReleaseNow {
        request: rls_now::ReleaseNowExecutionRequest,
    },
    PrefetchRecentChanges {
        dialog: RecentChangesDialog,
    },
    CreateTag {
        dialog: TagDialog,
        changelog_enabled: bool,
        std_changelog_policy: StdChangelogExecutionPolicy,
    },
}

enum BackgroundJobOutput {
    OpenRecentChanges(RecentChangesDialog),
    PendingBumpMainBranch {
        integration_mode: IntegrationMode,
        repos: Vec<git_flow::RepoBranchState>,
        pending_action: PendingBumpAction,
    },
    OverviewBumpWarnings {
        scope_index: usize,
        workflow: OverviewBumpWorkflow,
        warnings: Vec<UnexpectedStagedRepo>,
    },
    RecentChanges {
        dialog: RecentChangesDialog,
        status_message: Option<String>,
    },
    RecentChangesPrefetch {
        project_name: String,
        next_scope_index: Option<usize>,
        prefetched_recent_range: Option<ChangeRange>,
        history_scope_index: Option<usize>,
        prefetched_history_ranges: Option<Vec<ChangeRange>>,
    },
    OpenChangelogPreview(Box<ChangelogPreviewDialog>),
    OverviewActivityCache {
        project_index: usize,
        summaries: Vec<Option<RepoActivitySummary>>,
    },
    ReleaseNowValidated(rls_now::ReleaseNowValidation),
    ReleaseNowLogChunk(Vec<String>),
    ReleaseNowCompleted(rls_now::ReleaseNowExecutionOutcome),
    CreateTag {
        summary: String,
        replay_notices: Vec<String>,
        replay_errors: Vec<String>,
    },
}

type BackgroundJobResult = std::result::Result<BackgroundJobOutput, String>;
type BackgroundWorkerChannels = (
    TokioRuntime,
    UnboundedSender<BackgroundJobRequestMessage>,
    UnboundedSender<BackgroundJobRequestMessage>,
    UnboundedSender<BackgroundJobRequestMessage>,
    UnboundedReceiver<BackgroundJobResultMessage>,
);
type PrefetchedRecentChanges = (
    Option<usize>,
    Option<ChangeRange>,
    Option<usize>,
    Option<Vec<ChangeRange>>,
);

enum BackgroundJobMessagePayload {
    Progress(BackgroundJobOutput),
    Finished(BackgroundJobResult),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BackgroundJobKind {
    RecentChanges,
    RepoScan,
    RecentChangesPrefetch,
    ChangelogPreview,
    OverviewActivity,
    ReleaseNow,
    TagAction,
}

#[derive(Clone, Copy)]
enum BackgroundJobPriority {
    Foreground,
    Refresh,
    Prefetch,
}

struct BackgroundJobRequestMessage {
    id: u64,
    kind: BackgroundJobKind,
    request: BackgroundJobRequest,
    cancel: GitCancellation,
}

struct BackgroundJobResultMessage {
    id: u64,
    kind: BackgroundJobKind,
    payload: BackgroundJobMessagePayload,
}

#[derive(Clone)]
struct BackgroundJobProgressSink {
    id: u64,
    kind: BackgroundJobKind,
    result_tx: UnboundedSender<BackgroundJobResultMessage>,
}

impl BackgroundJobProgressSink {
    fn send(&self, output: BackgroundJobOutput) {
        let _ = self.result_tx.send(BackgroundJobResultMessage {
            id: self.id,
            kind: self.kind,
            payload: BackgroundJobMessagePayload::Progress(output),
        });
    }
}

impl BackgroundJobRequest {
    fn kind(&self) -> BackgroundJobKind {
        match self {
            Self::OpenRecentChanges { .. } | Self::RecentChanges { .. } => {
                BackgroundJobKind::RecentChanges
            }
            Self::CheckPendingBumpMainBranch { .. } | Self::CheckOverviewBumpWarnings { .. } => {
                BackgroundJobKind::RepoScan
            }
            Self::PrefetchRecentChanges { .. } => BackgroundJobKind::RecentChangesPrefetch,
            Self::OpenDashboardChangelogPreview { .. }
            | Self::OpenOverviewWorkflowChangelog { .. } => BackgroundJobKind::ChangelogPreview,
            Self::RefreshOverviewActivity { .. } => BackgroundJobKind::OverviewActivity,
            Self::ValidateReleaseNow { .. } | Self::RunReleaseNow { .. } => {
                BackgroundJobKind::ReleaseNow
            }
            Self::CreateTag { .. } => BackgroundJobKind::TagAction,
        }
    }
}

#[derive(Clone, Copy)]
enum RecentChangesLoadAction {
    RefreshCurrentScope,
    RotateScope(isize),
    SwitchTab(RecentChangesTab),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OverviewBumpWorkflow {
    JustBump,
    Commit,
    CommitAndTag,
    CommitAndPush,
    BranchCommit,
    BranchCommitAndPush,
}

pub(crate) fn overview_bump_workflow_options(
    integration_mode: IntegrationMode,
) -> Vec<OverviewBumpWorkflow> {
    match integration_mode {
        IntegrationMode::LocalOnly => vec![OverviewBumpWorkflow::JustBump],
        IntegrationMode::GitLocalOnly => vec![
            OverviewBumpWorkflow::JustBump,
            OverviewBumpWorkflow::Commit,
            OverviewBumpWorkflow::CommitAndTag,
        ],
        IntegrationMode::GitHubEnabled => vec![
            OverviewBumpWorkflow::JustBump,
            OverviewBumpWorkflow::Commit,
            OverviewBumpWorkflow::CommitAndPush,
            OverviewBumpWorkflow::BranchCommit,
            OverviewBumpWorkflow::BranchCommitAndPush,
        ],
    }
}

impl OverviewBumpWorkflow {
    fn display_name(self) -> &'static str {
        match self {
            OverviewBumpWorkflow::JustBump => "Just bump",
            OverviewBumpWorkflow::Commit => "Bump & Commit",
            OverviewBumpWorkflow::CommitAndTag => "Bump & Commit & Tag",
            OverviewBumpWorkflow::CommitAndPush => "Bump & Commit & Push",
            OverviewBumpWorkflow::BranchCommit => "Branch & Bump & Commit",
            OverviewBumpWorkflow::BranchCommitAndPush => "Branch & Bump & Commit & Push",
        }
    }

    fn description(self) -> &'static str {
        match self {
            OverviewBumpWorkflow::JustBump => "Writes the updated version files only.",
            OverviewBumpWorkflow::Commit => {
                "Stages the version files and commits them with the standard bump message."
            }
            OverviewBumpWorkflow::CommitAndTag => {
                "Stages and commits the version files, then creates a tag named after the new version."
            }
            OverviewBumpWorkflow::CommitAndPush => {
                "Stages and commits the version files, then pushes the bump commit to the configured remote."
            }
            OverviewBumpWorkflow::BranchCommit => {
                "Creates a new branch, stages and commits the version files there, and leaves pushing for later."
            }
            OverviewBumpWorkflow::BranchCommitAndPush => {
                "Creates a new branch, stages and commits the version files there, then pushes the new branch to the configured remote."
            }
        }
    }

    fn requires_push(self) -> bool {
        matches!(
            self,
            OverviewBumpWorkflow::CommitAndPush | OverviewBumpWorkflow::BranchCommitAndPush
        )
    }

    fn requires_tag(self) -> bool {
        matches!(self, OverviewBumpWorkflow::CommitAndTag)
    }

    pub(crate) fn requires_branch(self) -> bool {
        matches!(
            self,
            OverviewBumpWorkflow::BranchCommit | OverviewBumpWorkflow::BranchCommitAndPush
        )
    }
}

#[derive(Clone)]
struct OverviewBumpKindDialog {
    project_name: String,
    scope_label: String,
    scope_index: usize,
    scheme: VersionScheme,
    current_version: String,
    options: Vec<BumpAction>,
    selected: usize,
}

#[derive(Clone)]
pub(crate) struct RepoBumpOperation {
    pub(crate) repo_root: String,
    pub(crate) remote_spec: Option<String>,
    pub(crate) stage_paths: Vec<String>,
}

#[derive(Clone)]
struct OverviewBumpWorkflowDialog {
    project_name: String,
    scope_label: String,
    next_version: String,
    scope_index: usize,
    options: Vec<OverviewBumpWorkflow>,
    selected: usize,
    scroll: usize,
}

#[derive(Clone)]
struct OverviewBranchBumpDialog {
    project_name: String,
    scope_label: String,
    next_version: String,
    scope_index: usize,
    workflow: OverviewBumpWorkflow,
    options: Vec<BranchNameOption>,
    selected: usize,
    branch_name: TextInput,
    scroll: u16,
}

impl OverviewBranchBumpDialog {
    fn new(
        project_name: String,
        scope_label: String,
        next_version: String,
        scope_index: usize,
        workflow: OverviewBumpWorkflow,
        options: Vec<BranchNameOption>,
    ) -> Self {
        Self {
            project_name,
            scope_label,
            next_version,
            scope_index,
            workflow,
            options,
            selected: 0,
            branch_name: TextInput::with_value(""),
            scroll: 0,
        }
    }

    fn selected_option(&self) -> &BranchNameOption {
        &self.options[self.selected.min(self.options.len().saturating_sub(1))]
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

    fn input_enabled(&self) -> bool {
        self.selected_option().requires_input()
    }

    fn input_label(&self) -> &'static str {
        self.selected_option().input_label()
    }

    fn input_hint(&self) -> &'static str {
        self.selected_option().input_hint()
    }

    fn branch_preview(&self) -> String {
        self.selected_option()
            .preview_with_input(Some(self.branch_name.value.trim()))
    }

    fn resolved_branch_name(&self) -> Result<String> {
        self.selected_option()
            .resolve_name(Some(self.branch_name.value.trim()))
    }

    fn scroll_by(&mut self, delta: i16) {
        self.scroll = if delta < 0 {
            self.scroll.saturating_sub(delta.unsigned_abs())
        } else {
            self.scroll.saturating_add(delta as u16)
        };
    }
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
            scroll: 0,
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

    fn clamp_scroll(&mut self, visible_rows: usize) {
        clamp_dialog_scroll(
            &mut self.scroll,
            self.options.len(),
            visible_rows,
            Some(self.selected),
        );
    }
}

impl OverviewBumpKindDialog {
    fn new(
        project_name: String,
        scope_label: String,
        scope_index: usize,
        scheme: VersionScheme,
        current_version: String,
        options: Vec<BumpAction>,
    ) -> Self {
        let selected = options.len().saturating_sub(1);
        Self {
            project_name,
            scope_label,
            scope_index,
            scheme,
            current_version,
            options,
            selected,
        }
    }

    fn selected_action(&self) -> BumpAction {
        self.options[self.selected.min(self.options.len().saturating_sub(1))]
    }

    fn preview_next_version(&self) -> Result<String> {
        self.scheme
            .bump(
                &self.current_version,
                self.selected_action(),
                chrono::Local::now().date_naive(),
            )
            .map_err(anyhow::Error::msg)
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
    fn new(
        scope_index: usize,
        workflow: OverviewBumpWorkflow,
        repos: Vec<UnexpectedStagedRepo>,
    ) -> Self {
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

#[derive(Clone, Copy)]
enum PendingBumpAction {
    Standard,
    OverviewWorkflow { scope_index: usize },
}

#[derive(Clone)]
struct MainBranchWarningDialog {
    integration_mode: IntegrationMode,
    repos: Vec<git_flow::RepoBranchState>,
    pending_action: PendingBumpAction,
    selected: usize,
}

impl MainBranchWarningDialog {
    fn new(
        integration_mode: IntegrationMode,
        repos: Vec<git_flow::RepoBranchState>,
        pending_action: PendingBumpAction,
    ) -> Self {
        Self {
            integration_mode,
            repos,
            pending_action,
            selected: 0,
        }
    }

    fn switch_label(&self) -> &'static str {
        match self.integration_mode {
            IntegrationMode::GitHubEnabled => "Switch to mainline & Sync & Bump",
            IntegrationMode::GitLocalOnly => "Switch to mainline & Bump",
            IntegrationMode::LocalOnly => "Continue",
        }
    }

    fn select(&mut self, index: usize) {
        self.selected = index.min(2);
    }

    fn rotate(&mut self, delta: isize) {
        self.selected = (self.selected as isize + delta).rem_euclid(3) as usize;
    }

    fn selected_choice(&self) -> MainBranchWarningChoice {
        match self.selected {
            0 => MainBranchWarningChoice::SwitchToMain,
            1 => MainBranchWarningChoice::IgnoreAndContinue,
            _ => MainBranchWarningChoice::Cancel,
        }
    }
}

#[derive(Clone, Copy)]
enum MainBranchWarningChoice {
    SwitchToMain,
    IgnoreAndContinue,
    Cancel,
}

#[derive(Clone)]
struct StdChangelogSubBranchDialog {
    pending_request: PendingTagRequest,
    previous_tag: String,
    branch_name: String,
    selected: usize,
}

impl StdChangelogSubBranchDialog {
    fn new(pending_request: PendingTagRequest, previous_tag: String, branch_name: String) -> Self {
        Self {
            pending_request,
            previous_tag,
            branch_name,
            selected: 1,
        }
    }

    fn select(&mut self, index: usize) {
        self.selected = index.min(2);
    }

    fn rotate(&mut self, delta: isize) {
        self.selected = (self.selected as isize + delta).rem_euclid(3) as usize;
    }

    fn selected_choice(&self) -> StdChangelogSubBranchChoice {
        match self.selected {
            0 => StdChangelogSubBranchChoice::GenerateNow,
            1 => StdChangelogSubBranchChoice::Postpone,
            _ => StdChangelogSubBranchChoice::Cancel,
        }
    }
}

#[derive(Clone, Copy)]
enum StdChangelogSubBranchChoice {
    GenerateNow,
    Postpone,
    Cancel,
}

#[derive(Clone)]
struct UnexpectedStagedRepo {
    repo_root: String,
    extra_paths: Vec<String>,
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
        .with_context(|| {
            format!(
                "failed to refresh {} after updating {}",
                lock_path.display(),
                target.path
            )
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let detail = if stderr.is_empty() { stdout } else { stderr };
        bail!(
            "failed to refresh {} after updating {}: {}",
            lock_path.display(),
            target.path,
            detail
        );
    }

    Ok(())
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

#[derive(Clone)]
pub(crate) struct ScopeDraft {
    pub(crate) name: TextInput,
    pub(crate) label: String,
    pub(crate) label_follows_name: bool,
    pub(crate) changelog_enabled: bool,
    pub(crate) target_label: String,
    pub(crate) target_path: TextInput,
    pub(crate) target_key: TextInput,
    pub(crate) target_key_custom: bool,
    pub(crate) scope_kind: BranchScopeKind,
    pub(crate) repo: Option<RepoConfig>,
    pub(crate) format: TargetFormat,
    pub(crate) last_probe: Option<TargetProbe>,
}

impl ScopeDraft {
    pub(crate) fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            name: TextInput::with_value(name.clone()),
            label: name.clone(),
            label_follows_name: true,
            changelog_enabled: false,
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

    pub(crate) fn from_target(name: impl Into<String>, target: &TargetSpec) -> Self {
        let mut scope = Self::new(name);
        scope.target_label = target.label.clone();
        scope.target_path = TextInput::with_value(target.path.clone());
        scope.target_key = TextInput::with_value(target.key_path.clone());
        scope.target_key_custom = target_key_is_custom(&target.path, &target.key_path);
        scope.format = target.format;
        scope
    }

    pub(crate) fn from_branch(branch: &BranchConfig) -> Result<Self> {
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
            changelog_enabled: branch.changelog_enabled,
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

    pub(crate) fn display_name(&self) -> String {
        let name = self.name.value.trim();
        if name.is_empty() {
            "(unnamed scope)".to_string()
        } else if self.label_follows_name || self.label.trim().is_empty() || self.label == name {
            name.to_string()
        } else {
            format!("{} [{}]", self.label, name)
        }
    }

    pub(crate) fn sync_label_if_needed(&mut self) {
        if self.label_follows_name {
            self.label = self.name.value.trim().to_string();
        }
    }

    pub(crate) fn build_branch(
        &self,
        version_scheme: VersionScheme,
        require_probe: bool,
    ) -> Result<BranchConfig> {
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
                Some(probe) if matches!(probe.kind, ProbeKind::Success) => {
                    probe.format.unwrap_or(self.format)
                }
                Some(_) | None => bail!("scope '{}' must be read successfully before saving", name),
            }
        } else {
            self.last_probe
                .as_ref()
                .and_then(|probe| probe.format)
                .unwrap_or(self.format)
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
            changelog_enabled: self.changelog_enabled,
            changelog_path: None,
            release_now: crate::config::ReleaseNowSettings::default(),
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

fn spawn_background_worker() -> Result<BackgroundWorkerChannels> {
    let runtime = TokioRuntimeBuilder::new_multi_thread()
        .worker_threads(2)
        .thread_name("cg-bg")
        .enable_all()
        .build()
        .context("failed to create tokio runtime for background jobs")?;
    let (foreground_tx, mut foreground_rx) = unbounded_channel::<BackgroundJobRequestMessage>();
    let (refresh_tx, mut refresh_rx) = unbounded_channel::<BackgroundJobRequestMessage>();
    let (prefetch_tx, mut prefetch_rx) = unbounded_channel::<BackgroundJobRequestMessage>();
    let (result_tx, result_rx) = unbounded_channel::<BackgroundJobResultMessage>();

    runtime.spawn(async move {
        loop {
            tokio::select! {
                biased;
                Some(request) = foreground_rx.recv() => {
                    spawn_background_job_task(request, result_tx.clone());
                }
                Some(request) = refresh_rx.recv() => {
                    spawn_background_job_task(request, result_tx.clone());
                }
                Some(request) = prefetch_rx.recv() => {
                    spawn_background_job_task(request, result_tx.clone());
                }
                else => break,
            }
        }
    });

    Ok((runtime, foreground_tx, refresh_tx, prefetch_tx, result_rx))
}

fn spawn_background_job_task(
    request: BackgroundJobRequestMessage,
    result_tx: UnboundedSender<BackgroundJobResultMessage>,
) {
    tokio::spawn(async move {
        let progress = BackgroundJobProgressSink {
            id: request.id,
            kind: request.kind,
            result_tx: result_tx.clone(),
        };
        let result = run_background_job(request.request, request.cancel, progress)
            .await
            .map_err(|error| error.to_string());
        let _ = result_tx.send(BackgroundJobResultMessage {
            id: request.id,
            kind: request.kind,
            payload: BackgroundJobMessagePayload::Finished(result),
        });
    });
}

async fn run_blocking_job<T, F>(operation: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T> + Send + 'static,
{
    tokio::task::spawn_blocking(operation)
        .await
        .map_err(|error| anyhow!("background task failed: {error}"))?
}

async fn run_background_job(
    request: BackgroundJobRequest,
    cancel: GitCancellation,
    progress: BackgroundJobProgressSink,
) -> Result<BackgroundJobOutput> {
    match request {
        BackgroundJobRequest::OpenRecentChanges {
            project,
            preferred_scope,
        } => Ok(BackgroundJobOutput::OpenRecentChanges(
            run_blocking_job(move || {
                RecentChangesDialog::from_project_with_scope_cancellable(
                    &project,
                    preferred_scope.unwrap_or(0),
                    Some(cancel),
                )
            })
            .await?,
        )),
        BackgroundJobRequest::CheckPendingBumpMainBranch {
            project,
            affected_scope_indexes,
            pending_action,
        } => {
            let integration_mode = project.integration_mode;
            let repos = run_blocking_job(move || {
                let scopes = collect_bump_scopes(&project)?;
                let git_contexts = collect_all_branch_git_scope_contexts(&project)?;
                git_flow::collect_non_main_repo_states_with_cancel(
                    &project,
                    &scopes,
                    &git_contexts,
                    &affected_scope_indexes,
                    Some(cancel),
                )
            })
            .await?;
            Ok(BackgroundJobOutput::PendingBumpMainBranch {
                integration_mode,
                repos,
                pending_action,
            })
        }
        BackgroundJobRequest::CheckOverviewBumpWarnings {
            project,
            scope_index,
            workflow,
        } => {
            let warnings = run_blocking_job(move || {
                overview::collect_overview_bump_warnings(&project, scope_index, Some(cancel))
            })
            .await?;
            Ok(BackgroundJobOutput::OverviewBumpWarnings {
                scope_index,
                workflow,
                warnings,
            })
        }
        BackgroundJobRequest::RecentChanges { dialog, action } => {
            let (dialog, status_message) = run_blocking_job(move || {
                apply_recent_changes_background_action(dialog, action, Some(cancel))
            })
            .await?;
            Ok(BackgroundJobOutput::RecentChanges {
                dialog,
                status_message,
            })
        }
        BackgroundJobRequest::OpenDashboardChangelogPreview {
            project,
            scope_index,
            pending_versions,
            selection,
        } => Ok(BackgroundJobOutput::OpenChangelogPreview(Box::new(
            overview::build_dashboard_changelog_preview_dialog_async(
                &project,
                scope_index,
                &pending_versions,
                selection,
                Some(cancel),
            )
            .await?,
        ))),
        BackgroundJobRequest::OpenOverviewWorkflowChangelog {
            project,
            scope_index,
            workflow,
            pending_versions,
        } => Ok(BackgroundJobOutput::OpenChangelogPreview(Box::new(
            overview::build_overview_workflow_changelog_preview_dialog_async(
                &project,
                scope_index,
                workflow,
                &pending_versions,
                Some(cancel),
            )
            .await?,
        ))),
        BackgroundJobRequest::RefreshOverviewActivity {
            project_index,
            project,
        } => Ok(BackgroundJobOutput::OverviewActivityCache {
            project_index,
            summaries: load_overview_activity_summaries_async(project, Some(cancel)).await?,
        }),
        BackgroundJobRequest::ValidateReleaseNow {
            project,
            scope_index,
        } => Ok(BackgroundJobOutput::ReleaseNowValidated(
            run_blocking_job(move || {
                rls_now::validate_release_now(&project, scope_index, Some(cancel))
            })
            .await?,
        )),
        BackgroundJobRequest::RunReleaseNow { request } => {
            Ok(BackgroundJobOutput::ReleaseNowCompleted(
                rls_now::execute_release_now_async(request, cancel, move |lines| {
                    progress.send(BackgroundJobOutput::ReleaseNowLogChunk(lines));
                })
                .await?,
            ))
        }
        BackgroundJobRequest::PrefetchRecentChanges { dialog } => {
            let project_name = dialog.project_name.clone();
            let (
                next_scope_index,
                prefetched_recent_range,
                history_scope_index,
                prefetched_history_ranges,
            ) = run_blocking_job(move || prefetch_recent_changes(dialog, Some(cancel))).await?;
            Ok(BackgroundJobOutput::RecentChangesPrefetch {
                project_name,
                next_scope_index,
                prefetched_recent_range,
                history_scope_index,
                prefetched_history_ranges,
            })
        }
        BackgroundJobRequest::CreateTag {
            dialog,
            changelog_enabled,
            std_changelog_policy,
        } => {
            let outcome =
                run_create_tag_job_async(dialog, changelog_enabled, std_changelog_policy).await?;
            Ok(BackgroundJobOutput::CreateTag {
                summary: outcome.summary,
                replay_notices: outcome.replay_notices,
                replay_errors: outcome.replay_errors,
            })
        }
    }
}

async fn load_overview_activity_summaries_async(
    project: ProjectConfig,
    cancel: Option<GitCancellation>,
) -> Result<Vec<Option<RepoActivitySummary>>> {
    let contexts = collect_all_branch_git_scope_contexts(&project)?;
    let semaphore = std::sync::Arc::new(Semaphore::new(BACKGROUND_MAX_PARALLEL_REPO_JOBS.max(1)));
    let mut tasks = JoinSet::new();

    for (index, context) in contexts.into_iter().enumerate() {
        let semaphore = semaphore.clone();
        let cancel = cancel.clone();
        tasks.spawn(async move {
            let _permit = semaphore
                .acquire_owned()
                .await
                .map_err(|_| anyhow!("activity summary worker pool is unavailable"))?;
            let summary = run_blocking_job(move || {
                Ok(load_scope_activity_summary_with_cancel(&context, cancel).ok())
            })
            .await?;
            Ok::<_, anyhow::Error>((index, summary))
        });
    }

    let mut summaries = Vec::new();
    summaries.resize_with(tasks.len(), || None);

    while let Some(result) = tasks.join_next().await {
        let (index, summary) =
            result.map_err(|error| anyhow!("activity summary task failed: {error}"))??;
        if let Some(slot) = summaries.get_mut(index) {
            *slot = summary;
        }
    }

    Ok(summaries)
}

fn apply_recent_changes_background_action(
    mut dialog: RecentChangesDialog,
    action: RecentChangesLoadAction,
    cancel: Option<GitCancellation>,
) -> Result<(RecentChangesDialog, Option<String>)> {
    let status_message = match action {
        RecentChangesLoadAction::RefreshCurrentScope => {
            dialog.refresh_current_scope_cancellable(cancel)?;
            Some("Refreshed git history for the current scope.".to_string())
        }
        RecentChangesLoadAction::RotateScope(delta) => {
            dialog.rotate_scope_cancellable(delta, cancel)?;
            None
        }
        RecentChangesLoadAction::SwitchTab(tab) => {
            dialog.switch_tab_cancellable(tab, cancel)?;
            None
        }
    };

    Ok((dialog, status_message))
}

fn prefetch_recent_changes(
    dialog: RecentChangesDialog,
    cancel: Option<GitCancellation>,
) -> Result<PrefetchedRecentChanges> {
    let next_scope_index = if dialog.can_select_scope() {
        Some((dialog.selected_scope + 1) % dialog.scopes.len())
    } else {
        None
    };
    let prefetched_recent_range = next_scope_index
        .filter(|index| {
            dialog
                .prefetched_recent_ranges
                .get(*index)
                .and_then(|entry| entry.as_ref())
                .is_none()
        })
        .map(|index| load_recent_change_range_with_cancel(&dialog.scopes[index], cancel.clone()))
        .transpose()?;
    let history_scope_index = (!dialog.history_loaded
        && dialog
            .prefetched_history_ranges
            .get(dialog.selected_scope)
            .and_then(|entry| entry.as_ref())
            .is_none())
    .then_some(dialog.selected_scope);
    let prefetched_history_ranges = history_scope_index
        .map(|index| load_history_ranges_with_cancel(&dialog.scopes[index], cancel))
        .transpose()?;

    Ok((
        next_scope_index,
        prefetched_recent_range,
        history_scope_index,
        prefetched_history_ranges,
    ))
}

struct BackgroundTagOutcome {
    summary: String,
    replay_notices: Vec<String>,
    replay_errors: Vec<String>,
}

#[derive(Default)]
struct PostponedReplayOutcome {
    notices: Vec<String>,
    errors: Vec<String>,
}

#[derive(Default)]
struct StandardChangelogExecutionOutcome {
    summary_notes: Vec<String>,
    replay_notices: Vec<String>,
    replay_errors: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StdChangelogExecutionPolicy {
    Auto,
    ForceGenerate,
    ForcePostpone,
}

#[derive(Clone)]
struct PendingTagRequest {
    dialog: TagDialog,
    changelog_enabled: bool,
    std_changelog_policy: StdChangelogExecutionPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum StdChangelogDecision {
    Generate,
    IgnoreNotOnMain,
    PostponeOnSubBranch(String),
    SkipNoPreviousTag,
}

async fn run_create_tag_job_async(
    dialog: TagDialog,
    changelog_enabled: bool,
    std_changelog_policy: StdChangelogExecutionPolicy,
) -> Result<BackgroundTagOutcome> {
    let active_scope = dialog.active_scope().clone();
    let repo_root = active_scope.repo_root.clone();
    let project_name = dialog.project_name.clone();
    let action = dialog.selected_action();
    let remote_spec = active_scope.remote_spec.clone();
    let annotation = dialog.annotation.trim().to_string();
    let tag_name = dialog.tag_name.value.trim().to_string();
    let active_scope_for_notes = active_scope.clone();
    let repo_root_for_branch = repo_root.clone();
    let branch_name =
        run_blocking_job(move || current_branch_with_cancel(&repo_root_for_branch, None)).await?;
    let tag_name_for_create = tag_name.clone();
    let annotation_for_create = annotation.clone();

    let repo_root_for_create = repo_root.clone();
    let created = run_blocking_job(move || {
        ensure_local_tag(
            &repo_root_for_create,
            &tag_name_for_create,
            if annotation_for_create.is_empty() {
                None
            } else {
                Some(annotation_for_create.as_str())
            },
        )
    })
    .await?;

    let mut summary_notes = Vec::new();
    let mut standard_outcome = StandardChangelogExecutionOutcome::default();

    let release_notes = if created || matches!(action, TagAction::CreatePushAndRelease) {
        let tag_name_for_notes = tag_name.clone();
        Some(
            run_blocking_job(move || {
                build_release_notes_markdown(&tag_name_for_notes, &active_scope_for_notes)
            })
            .await?,
        )
    } else {
        None
    };

    if changelog_enabled {
        standard_outcome = execute_standard_changelog_for_tag(
            &active_scope,
            &tag_name,
            &branch_name,
            std_changelog_policy,
        )
        .await?;
        summary_notes.extend(standard_outcome.summary_notes.clone());

        if matches!(
            action,
            TagAction::CreateAndPush | TagAction::CreatePushAndRelease
        ) {
            let remote_spec =
                remote_spec.ok_or_else(|| anyhow!("no remote is configured for this project"))?;
            run_git_push_with_retry_async(repo_root.clone(), remote_spec, tag_name.clone()).await?;
        }

        if matches!(action, TagAction::CreatePushAndRelease) {
            run_blocking_job(ensure_gh_available).await?;
            let release_notes = release_notes
                .as_deref()
                .ok_or_else(|| anyhow!("release notes should be available for release creation"))?;
            create_github_release_with_retry_async(
                repo_root.clone(),
                tag_name.clone(),
                release_notes.to_string(),
            )
            .await?;
        }

        let scope_notice = if active_scope.scope_kind.is_some() {
            format!(" for {}", active_scope.display_name)
        } else {
            String::new()
        };
        let summary = match action {
            TagAction::CreateLocal if created => format!(
                "Created local tag '{}' in {}{}.",
                tag_name, project_name, scope_notice
            ),
            TagAction::CreateLocal => format!(
                "Tag '{}' already existed locally in {}{}.",
                tag_name, project_name, scope_notice
            ),
            TagAction::CreateAndPush => format!(
                "Tag '{}' is present locally and has been pushed for {}{}.",
                tag_name, project_name, scope_notice
            ),
            TagAction::CreatePushAndRelease => format!(
                "Tag '{}' was created, pushed, and released for {}{}.",
                tag_name, project_name, scope_notice
            ),
        };

        return Ok(BackgroundTagOutcome {
            summary: if annotation.is_empty() {
                append_background_tag_summary_notes(summary, &summary_notes)
            } else {
                append_background_tag_summary_notes(
                    format!("{} Annotation included.", summary),
                    &summary_notes,
                )
            },
            replay_notices: standard_outcome.replay_notices,
            replay_errors: standard_outcome.replay_errors,
        });
    }

    if matches!(
        action,
        TagAction::CreateAndPush | TagAction::CreatePushAndRelease
    ) {
        let remote_spec =
            remote_spec.ok_or_else(|| anyhow!("no remote is configured for this project"))?;
        run_git_push_with_retry_async(repo_root.clone(), remote_spec, tag_name.clone()).await?;
    }

    if matches!(action, TagAction::CreatePushAndRelease) {
        run_blocking_job(ensure_gh_available).await?;
        let release_notes = release_notes
            .as_deref()
            .ok_or_else(|| anyhow!("release notes should be available for release creation"))?;
        create_github_release_with_retry_async(
            repo_root.clone(),
            tag_name.clone(),
            release_notes.to_string(),
        )
        .await?;
    }

    let scope_notice = if active_scope.scope_kind.is_some() {
        format!(" for {}", active_scope.display_name)
    } else {
        String::new()
    };
    let summary = match action {
        TagAction::CreateLocal if created => format!(
            "Created local tag '{}' in {}{}.",
            tag_name, project_name, scope_notice
        ),
        TagAction::CreateLocal => format!(
            "Tag '{}' already existed locally in {}{}.",
            tag_name, project_name, scope_notice
        ),
        TagAction::CreateAndPush => format!(
            "Tag '{}' is present locally and has been pushed for {}{}.",
            tag_name, project_name, scope_notice
        ),
        TagAction::CreatePushAndRelease => format!(
            "Tag '{}' was created, pushed, and released for {}{}.",
            tag_name, project_name, scope_notice
        ),
    };

    Ok(BackgroundTagOutcome {
        summary: if annotation.is_empty() {
            append_background_tag_summary_notes(summary, &summary_notes)
        } else {
            append_background_tag_summary_notes(
                format!("{} Annotation included.", summary),
                &summary_notes,
            )
        },
        replay_notices: standard_outcome.replay_notices,
        replay_errors: standard_outcome.replay_errors,
    })
}

fn append_background_tag_summary_notes(summary: String, notes: &[String]) -> String {
    if notes.is_empty() {
        summary
    } else {
        format!("{} {}", summary, notes.join(" "))
    }
}

async fn execute_standard_changelog_for_tag(
    scope: &crate::git::GitScopeContext,
    tag_name: &str,
    branch_name: &str,
    std_changelog_policy: StdChangelogExecutionPolicy,
) -> Result<StandardChangelogExecutionOutcome> {
    let scope = scope.clone();
    let tag_name = tag_name.to_string();
    let branch_name = branch_name.to_string();
    run_blocking_job(move || {
        execute_standard_changelog_for_tag_blocking(
            &scope,
            &tag_name,
            &branch_name,
            std_changelog_policy,
        )
    })
    .await
}

fn execute_standard_changelog_for_tag_blocking(
    scope: &crate::git::GitScopeContext,
    tag_name: &str,
    branch_name: &str,
    std_changelog_policy: StdChangelogExecutionPolicy,
) -> Result<StandardChangelogExecutionOutcome> {
    let repo_root = &scope.repo_root;
    ensure_std_changelog_memory_entry(repo_root, tag_name, branch_name)?;

    let sorted_tags = sorted_local_tags_with_cancel(repo_root, None)?;
    let previous_tag = previous_tag_for_replay(&sorted_tags, tag_name);
    let decision = match std_changelog_policy {
        StdChangelogExecutionPolicy::ForceGenerate => StdChangelogDecision::Generate,
        StdChangelogExecutionPolicy::ForcePostpone => {
            StdChangelogDecision::PostponeOnSubBranch(branch_name.to_string())
        }
        StdChangelogExecutionPolicy::Auto => {
            if let Some(previous_tag) = previous_tag.as_deref() {
                let previous_branches =
                    branches_containing_ref_with_cancel(repo_root, previous_tag, None)?;
                let new_branches = branches_containing_ref_with_cancel(repo_root, tag_name, None)?;
                decide_std_changelog_generation(
                    previous_tag,
                    branch_name,
                    &previous_branches,
                    &new_branches,
                    scope.main_branch_name.as_deref(),
                )
            } else {
                StdChangelogDecision::SkipNoPreviousTag
            }
        }
    };

    let mut outcome = StandardChangelogExecutionOutcome::default();
    match decision {
        StdChangelogDecision::Generate => {
            if find_archived_changelog_markdown(repo_root, tag_name)?.is_some() {
                rebuild_history_summary_readme(repo_root)?;
                record_std_changelog_generated(repo_root, tag_name, branch_name)?;
            } else if let Some(previous_tag) = previous_tag.as_deref() {
                let range =
                    load_change_range_for_tags_with_cancel(scope, previous_tag, tag_name, None)?;
                if range.lines.is_empty() {
                    let reason = "standard changelog range was empty".to_string();
                    record_std_changelog_error(repo_root, tag_name, branch_name, &reason)?;
                    outcome.summary_notes.push("Standard changelog was not generated because the computed tag range was empty.".to_string());
                } else {
                    let markdown = std_changelog_gen(tag_name.to_string(), &range.lines).markdown;
                    archive_changelog_markdown(repo_root, tag_name, &markdown)?;
                    record_std_changelog_generated(repo_root, tag_name, branch_name)?;
                }
            } else {
                outcome.summary_notes.push(
                    "Standard changelog was not generated because no previous tag was found."
                        .to_string(),
                );
            }
        }
        StdChangelogDecision::IgnoreNotOnMain => {
            outcome.summary_notes.push("Standard changelog was not generated because this tag is not yet on mainline lineage.".to_string());
        }
        StdChangelogDecision::PostponeOnSubBranch(branch) => {
            record_std_changelog_postponed(repo_root, tag_name, branch_name)?;
            outcome.summary_notes.push(format!(
                "Standard changelog was postponed because '{}' already has tags on sub-branch '{}'.",
                tag_name, branch
            ));
        }
        StdChangelogDecision::SkipNoPreviousTag => {
            outcome.summary_notes.push(
                "Standard changelog was not generated because no previous tag was found."
                    .to_string(),
            );
        }
    }

    let replay_outcome = if is_mainline_branch(branch_name, scope.main_branch_name.as_deref()) {
        replay_postponed_std_changelogs_blocking(
            scope,
            repo_root,
            branch_name,
            scope.main_branch_name.as_deref(),
        )?
    } else {
        PostponedReplayOutcome::default()
    };
    if !replay_outcome.notices.is_empty() {
        outcome.summary_notes.push(format!(
            "Replayed {} postponed changelog(s).",
            replay_outcome.notices.len()
        ));
    }
    if !replay_outcome.errors.is_empty() {
        outcome.summary_notes.push(format!(
            "{} postponed changelog replay error(s) occurred. See sticky toasts.",
            replay_outcome.errors.len()
        ));
    }
    outcome.replay_notices = replay_outcome.notices;
    outcome.replay_errors = replay_outcome.errors;
    Ok(outcome)
}

fn ensure_std_changelog_memory_entry(
    repo_root: &str,
    tag_name: &str,
    branch_name: &str,
) -> Result<()> {
    let memory = load_merged_std_changelog_memory(repo_root)?;
    if memory.entries.iter().any(|entry| {
        entry.tag_from.trim() == tag_name.trim() && entry.tag_origin.trim() == branch_name.trim()
    }) {
        return Ok(());
    }

    record_std_changelog_created(repo_root, tag_name, branch_name)
}

fn ensure_gitignore_entry(repo_root: &str, entry: &str) -> Result<()> {
    let gitignore_path = Path::new(repo_root).join(".gitignore");
    let mut lines = if gitignore_path.exists() {
        fs::read_to_string(&gitignore_path)
            .with_context(|| format!("failed to read .gitignore in '{}'", repo_root))?
            .lines()
            .map(str::to_string)
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    let normalized_entry = entry.trim();
    if lines.iter().any(|line| line.trim() == normalized_entry) {
        return Ok(());
    }

    if !lines.is_empty() && !lines.last().unwrap().is_empty() {
        lines.push(String::new());
    }
    lines.push(normalized_entry.to_string());

    fs::write(&gitignore_path, lines.join("\n") + "\n")
        .with_context(|| format!("failed to update .gitignore in '{}'", repo_root))?;

    Ok(())
}

fn replay_postponed_std_changelogs_blocking(
    scope: &crate::git::GitScopeContext,
    repo_root: &str,
    branch_name: &str,
    custom_main_branch: Option<&str>,
) -> Result<PostponedReplayOutcome> {
    if !is_mainline_branch(branch_name, custom_main_branch) {
        return Ok(PostponedReplayOutcome::default());
    }

    let memory = load_merged_std_changelog_memory(repo_root)?;
    let mut postponed = memory
        .entries
        .iter()
        .filter(|entry| entry.generated == crate::mmr::StdChangelogGeneratedState::Postponed)
        .cloned()
        .collect::<Vec<_>>();
    postponed.sort_by(|left, right| left.ts.cmp(&right.ts));
    if postponed.is_empty() {
        return Ok(PostponedReplayOutcome::default());
    }

    let sorted_tags = sorted_local_tags_with_cancel(repo_root, None)?;
    let mut outcome = PostponedReplayOutcome::default();
    for entry in postponed {
        let mainline_branches =
            branches_containing_ref_with_cancel(repo_root, &entry.tag_from, None)?;
        if !mainline_branches
            .iter()
            .any(|branch| is_mainline_branch(branch, custom_main_branch))
        {
            continue;
        }

        if find_archived_changelog_markdown(repo_root, &entry.tag_from)?.is_some() {
            record_std_changelog_generated(repo_root, &entry.tag_from, &entry.tag_origin)?;
            outcome.notices.push(format!("Replayed postponed changelog '{}' was already archived and has been marked generated.", entry.tag_from));
            continue;
        }

        let Some(previous_tag) = previous_tag_for_replay(&sorted_tags, &entry.tag_from) else {
            let reason = "no previous tag found for postponed replay".to_string();
            record_std_changelog_error(repo_root, &entry.tag_from, &entry.tag_origin, &reason)?;
            outcome.errors.push(format!(
                "Postponed changelog '{}' could not be replayed: {}.",
                entry.tag_from, reason
            ));
            continue;
        };

        let range =
            load_change_range_for_tags_with_cancel(scope, &previous_tag, &entry.tag_from, None)?;
        if range.lines.is_empty() {
            let reason = "replayed postponed changelog range was empty".to_string();
            record_std_changelog_error(repo_root, &entry.tag_from, &entry.tag_origin, &reason)?;
            outcome.errors.push(format!(
                "Postponed changelog '{}' could not be replayed: {}.",
                entry.tag_from, reason
            ));
            continue;
        }

        let markdown = std_changelog_gen(entry.tag_from.clone(), &range.lines).markdown;
        archive_changelog_markdown(repo_root, &entry.tag_from, &markdown)?;
        record_std_changelog_generated(repo_root, &entry.tag_from, &entry.tag_origin)?;
        outcome.notices.push(format!(
            "Replayed postponed changelog '{}' after it reached mainline lineage.",
            entry.tag_from
        ));
    }

    Ok(outcome)
}

fn previous_tag_for_replay(sorted_tags: &[String], tag_name: &str) -> Option<String> {
    let index = sorted_tags
        .iter()
        .position(|candidate| candidate.trim() == tag_name.trim())?;
    sorted_tags.get(index + 1).cloned()
}

fn decide_std_changelog_generation(
    previous_tag: &str,
    current_branch: &str,
    previous_branches: &[String],
    new_branches: &[String],
    custom_main_branch: Option<&str>,
) -> StdChangelogDecision {
    if is_mainline_branch(current_branch, custom_main_branch) {
        return StdChangelogDecision::Generate;
    }

    let previous_has_main = previous_branches
        .iter()
        .any(|branch| is_mainline_branch(branch, custom_main_branch));
    let new_has_main = new_branches
        .iter()
        .any(|branch| is_mainline_branch(branch, custom_main_branch));
    if previous_has_main && new_has_main {
        return StdChangelogDecision::Generate;
    }
    if previous_has_main && !new_has_main {
        return StdChangelogDecision::IgnoreNotOnMain;
    }

    let previous_normalized = normalized_branch_names(previous_branches);
    let new_normalized = normalized_branch_names(new_branches);
    if previous_normalized == new_normalized && new_normalized.len() == 1 {
        let branch = new_normalized[0].clone();
        if !is_mainline_branch(&branch, custom_main_branch) {
            let _ = previous_tag;
            return StdChangelogDecision::PostponeOnSubBranch(branch);
        }
    }

    StdChangelogDecision::IgnoreNotOnMain
}

fn normalized_branch_names(branches: &[String]) -> Vec<String> {
    let mut names = branches
        .iter()
        .map(|branch| branch.trim().trim_start_matches('*').trim().to_string())
        .filter(|branch| !branch.is_empty())
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    names
}

fn is_mainline_branch(branch: &str, custom_main_branch: Option<&str>) -> bool {
    is_mainline_branch_name(branch, custom_main_branch)
}

fn build_release_notes_markdown(
    tag_name: &str,
    scope: &crate::git::GitScopeContext,
) -> Result<String> {
    let last_public_release = latest_public_release_tag(&scope.repo_root).ok().flatten();
    if let Some(markdown) = find_archived_changelog_markdown(&scope.repo_root, tag_name)? {
        return Ok(ensure_previous_public_release_header(
            &markdown,
            tag_name,
            last_public_release.as_deref(),
        ));
    }

    if let Some(last_public_release) =
        last_public_release.filter(|tag| tag.trim() != tag_name.trim())
    {
        let local_tags = sorted_local_tags_with_cancel(&scope.repo_root, None)?;
        let release_range = if local_tags
            .iter()
            .any(|candidate| candidate.trim() == tag_name.trim())
        {
            load_change_range_for_tags_with_cancel(scope, &last_public_release, tag_name, None)?
        } else {
            load_change_range_for_refs_with_cancel(scope, &last_public_release, "HEAD", None)?
        };
        return Ok(rls_changelog_gen(
            tag_name.to_string(),
            &release_range.lines,
            Some(&last_public_release),
        )
        .markdown);
    }

    let recent_range = load_recent_change_range_with_cancel(scope, None)?;
    Ok(rls_changelog_gen(tag_name.to_string(), &recent_range.lines, None).markdown)
}

fn latest_public_release_tag(repo_root: &str) -> Result<Option<String>> {
    crate::git_stt::last_rls_version(repo_root, None)
}

async fn run_git_push_with_retry_async(
    repo_root: String,
    remote_spec: String,
    tag_name: String,
) -> Result<()> {
    let args = vec!["push".to_string(), remote_spec, tag_name];
    run_command_with_retry_async(
        repo_root,
        "git",
        args,
        GIT_PUSH_TIMEOUT,
        NETWORK_RETRY_ATTEMPTS,
        "git push",
    )
    .await
}

async fn create_github_release_with_retry_async(
    repo_root: String,
    tag_name: String,
    release_notes: String,
) -> Result<()> {
    let notes_file = std::env::temp_dir().join(format!(
        "cg-release-notes-{}-{}.md",
        std::process::id(),
        sanitize_tag_fragment(&tag_name)
    ));
    fs::write(&notes_file, &release_notes).with_context(|| {
        format!(
            "failed to write release notes to '{}'",
            notes_file.display()
        )
    })?;

    let notes_file_string = notes_file.to_string_lossy().into_owned();
    let args = vec![
        "release".to_string(),
        "create".to_string(),
        tag_name,
        "--notes-file".to_string(),
        notes_file_string,
    ];
    let release_result = run_command_with_retry_async(
        repo_root,
        "gh",
        args,
        GH_RELEASE_TIMEOUT,
        NETWORK_RETRY_ATTEMPTS,
        "gh release create",
    )
    .await;
    let cleanup_result = fs::remove_file(&notes_file);

    release_result?;
    cleanup_result.with_context(|| {
        format!(
            "failed to remove temporary release notes file '{}'",
            notes_file.display()
        )
    })?;
    Ok(())
}

async fn run_command_with_retry_async(
    repo_root: String,
    program: &'static str,
    args: Vec<String>,
    timeout: Duration,
    attempts: usize,
    action: &'static str,
) -> Result<()> {
    let total_attempts = attempts.max(1);
    let mut last_error = None;

    for attempt in 1..=total_attempts {
        let repo_root_for_attempt = repo_root.clone();
        let args_for_attempt = args.clone();
        match run_blocking_job(move || {
            run_command_checked_with_timeout(
                &repo_root_for_attempt,
                program,
                &args_for_attempt,
                timeout,
                action,
            )
        })
        .await
        {
            Ok(()) => return Ok(()),
            Err(error) => {
                last_error = Some(error);
                if attempt < total_attempts {
                    sleep(NETWORK_RETRY_DELAY).await;
                }
            }
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow!("{action} failed")))
}

fn run_command_checked_with_timeout(
    repo_root: &str,
    program: &str,
    args: &[String],
    timeout: Duration,
    action: &str,
) -> Result<()> {
    let mut command = Command::new(program);
    command
        .current_dir(repo_root)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .with_context(|| format!("failed to start {action} in '{}'", repo_root))?;
    let started_at = Instant::now();

    loop {
        if let Some(status) = child
            .try_wait()
            .with_context(|| format!("failed to poll {action}"))?
        {
            let output = child
                .wait_with_output()
                .with_context(|| format!("failed to collect output for {action}"))?;
            if status.success() {
                return Ok(());
            }

            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let detail = if stderr.is_empty() { stdout } else { stderr };
            bail!("{action} failed: {detail}");
        }

        if started_at.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait_with_output();
            bail!("{action} timed out after {}s", timeout.as_secs());
        }

        std::thread::sleep(Duration::from_millis(100));
    }
}

impl App {
    fn register_background_job(&mut self, kind: BackgroundJobKind) -> u64 {
        self.cancel_background_job_kind(kind);
        let id = self.next_background_job_id;
        self.next_background_job_id += 1;
        self.background_jobs_inflight += 1;
        let cancel = GitCancellation::new();
        match kind {
            BackgroundJobKind::RecentChanges => {
                self.current_recent_changes_job_id = Some(id);
                self.current_recent_changes_cancel = Some(cancel);
            }
            BackgroundJobKind::RepoScan => {}
            BackgroundJobKind::RecentChangesPrefetch => {
                self.current_recent_changes_prefetch_job_id = Some(id);
                self.current_recent_changes_prefetch_cancel = Some(cancel);
            }
            BackgroundJobKind::ChangelogPreview => {
                self.current_changelog_preview_job_id = Some(id);
                self.current_changelog_preview_cancel = Some(cancel);
            }
            BackgroundJobKind::OverviewActivity => {
                self.current_overview_activity_job_id = Some(id);
                self.current_overview_activity_cancel = Some(cancel);
            }
            BackgroundJobKind::ReleaseNow => {
                self.current_release_now_job_id = Some(id);
                self.current_release_now_cancel = Some(cancel);
            }
            BackgroundJobKind::TagAction => {}
        }
        id
    }

    fn clear_registered_background_job(&mut self, kind: BackgroundJobKind, id: u64) {
        match kind {
            BackgroundJobKind::RecentChanges if self.current_recent_changes_job_id == Some(id) => {
                self.current_recent_changes_job_id = None;
                self.current_recent_changes_cancel = None;
            }
            BackgroundJobKind::RepoScan => {}
            BackgroundJobKind::RecentChangesPrefetch
                if self.current_recent_changes_prefetch_job_id == Some(id) =>
            {
                self.current_recent_changes_prefetch_job_id = None;
                self.current_recent_changes_prefetch_cancel = None;
            }
            BackgroundJobKind::ChangelogPreview
                if self.current_changelog_preview_job_id == Some(id) =>
            {
                self.current_changelog_preview_job_id = None;
                self.current_changelog_preview_cancel = None;
            }
            BackgroundJobKind::OverviewActivity
                if self.current_overview_activity_job_id == Some(id) =>
            {
                self.current_overview_activity_job_id = None;
                self.current_overview_activity_cancel = None;
            }
            BackgroundJobKind::ReleaseNow if self.current_release_now_job_id == Some(id) => {
                self.current_release_now_job_id = None;
                self.current_release_now_cancel = None;
            }
            _ => {}
        }
    }

    fn is_background_result_stale(&self, message: &BackgroundJobResultMessage) -> bool {
        match message.kind {
            BackgroundJobKind::RecentChanges => {
                self.current_recent_changes_job_id != Some(message.id)
            }
            BackgroundJobKind::RepoScan => false,
            BackgroundJobKind::RecentChangesPrefetch => {
                self.current_recent_changes_prefetch_job_id != Some(message.id)
            }
            BackgroundJobKind::ChangelogPreview => {
                self.current_changelog_preview_job_id != Some(message.id)
            }
            BackgroundJobKind::OverviewActivity => {
                self.current_overview_activity_job_id != Some(message.id)
            }
            BackgroundJobKind::ReleaseNow => self.current_release_now_job_id != Some(message.id),
            BackgroundJobKind::TagAction => false,
        }
    }

    fn schedule_background_job(
        &mut self,
        priority: BackgroundJobPriority,
        request: BackgroundJobRequest,
    ) -> Result<u64> {
        let kind = request.kind();
        let request_id = self.register_background_job(kind);
        let cancel = self.background_job_cancel(kind, request_id);
        let message = BackgroundJobRequestMessage {
            id: request_id,
            kind,
            request,
            cancel,
        };

        let send_result = match priority {
            BackgroundJobPriority::Foreground => self.foreground_request_tx.send(message),
            BackgroundJobPriority::Refresh => self.refresh_request_tx.send(message),
            BackgroundJobPriority::Prefetch => self.prefetch_request_tx.send(message),
        };

        if let Err(error) = send_result {
            self.background_jobs_inflight = self.background_jobs_inflight.saturating_sub(1);
            self.clear_registered_background_job(kind, request_id);
            bail!("failed to queue background job: {error}");
        }

        Ok(request_id)
    }

    fn schedule_recent_changes_prefetch(&mut self) -> Result<()> {
        let Some(dialog) = self.recent_changes_dialog.clone() else {
            return Ok(());
        };

        let should_prefetch_next_scope = dialog.can_select_scope()
            && dialog
                .prefetched_recent_ranges
                .get((dialog.selected_scope + 1) % dialog.scopes.len())
                .and_then(|entry| entry.as_ref())
                .is_none();
        let should_prefetch_history = !dialog.history_loaded
            && dialog
                .prefetched_history_ranges
                .get(dialog.selected_scope)
                .and_then(|entry| entry.as_ref())
                .is_none();

        if !should_prefetch_next_scope && !should_prefetch_history {
            return Ok(());
        }

        let _ = self.schedule_background_job(
            BackgroundJobPriority::Prefetch,
            BackgroundJobRequest::PrefetchRecentChanges { dialog },
        )?;
        Ok(())
    }

    fn cancel_background_job_kind(&mut self, kind: BackgroundJobKind) {
        match kind {
            BackgroundJobKind::RecentChanges => {
                if let Some(cancel) = self.current_recent_changes_cancel.take() {
                    cancel.cancel();
                }
            }
            BackgroundJobKind::RepoScan => {}
            BackgroundJobKind::RecentChangesPrefetch => {
                if let Some(cancel) = self.current_recent_changes_prefetch_cancel.take() {
                    cancel.cancel();
                }
            }
            BackgroundJobKind::ChangelogPreview => {
                if let Some(cancel) = self.current_changelog_preview_cancel.take() {
                    cancel.cancel();
                }
            }
            BackgroundJobKind::OverviewActivity => {
                if let Some(cancel) = self.current_overview_activity_cancel.take() {
                    cancel.cancel();
                }
            }
            BackgroundJobKind::ReleaseNow => {
                if let Some(cancel) = self.current_release_now_cancel.take() {
                    cancel.cancel();
                }
            }
            BackgroundJobKind::TagAction => {}
        }
    }

    fn background_job_cancel(&self, kind: BackgroundJobKind, id: u64) -> GitCancellation {
        match kind {
            BackgroundJobKind::RecentChanges => self
                .current_recent_changes_cancel
                .clone()
                .filter(|_| self.current_recent_changes_job_id == Some(id))
                .unwrap_or_default(),
            BackgroundJobKind::RepoScan => GitCancellation::default(),
            BackgroundJobKind::RecentChangesPrefetch => self
                .current_recent_changes_prefetch_cancel
                .clone()
                .filter(|_| self.current_recent_changes_prefetch_job_id == Some(id))
                .unwrap_or_default(),
            BackgroundJobKind::ChangelogPreview => self
                .current_changelog_preview_cancel
                .clone()
                .filter(|_| self.current_changelog_preview_job_id == Some(id))
                .unwrap_or_default(),
            BackgroundJobKind::OverviewActivity => self
                .current_overview_activity_cancel
                .clone()
                .filter(|_| self.current_overview_activity_job_id == Some(id))
                .unwrap_or_default(),
            BackgroundJobKind::ReleaseNow => self
                .current_release_now_cancel
                .clone()
                .filter(|_| self.current_release_now_job_id == Some(id))
                .unwrap_or_default(),
            BackgroundJobKind::TagAction => GitCancellation::default(),
        }
    }

    fn schedule_prefetch_overview_activity_cache(&mut self) -> Result<()> {
        let Some(project) = self.config.projects.get(self.selected_project).cloned() else {
            return Ok(());
        };
        if !project.integration_mode.requires_repo()
            || self.overview_activity_project == Some(self.selected_project)
            || self.overview_activity_job_inflight
        {
            return Ok(());
        }

        let _ = self.schedule_background_job(
            BackgroundJobPriority::Prefetch,
            BackgroundJobRequest::RefreshOverviewActivity {
                project_index: self.selected_project,
                project,
            },
        )?;
        self.overview_activity_job_inflight = true;
        Ok(())
    }

    fn schedule_refresh_overview_activity_cache(&mut self) -> Result<()> {
        let Some(project) = self.config.projects.get(self.selected_project).cloned() else {
            return Ok(());
        };
        if !project.integration_mode.requires_repo() {
            return Ok(());
        }

        if self.overview_activity_refresh_inflight {
            self.overview_activity_refresh_pending = true;
            return Ok(());
        }

        let _ = self.schedule_background_job(
            BackgroundJobPriority::Refresh,
            BackgroundJobRequest::RefreshOverviewActivity {
                project_index: self.selected_project,
                project,
            },
        )?;
        self.overview_activity_job_inflight = true;
        self.overview_activity_refresh_inflight = true;
        self.overview_activity_refresh_pending = false;
        Ok(())
    }
}

fn sanitize_pasted_text(text: &str) -> String {
    text.chars()
        .filter(|character| *character != '\r' && *character != '\n')
        .collect()
}

fn sanitize_tag_fragment(text: &str) -> String {
    let sanitized = text
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    sanitized.trim_matches('-').to_string()
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
        Self::with_placeholder(existing_annotation, "Optional multi-line tag annotation")
    }

    fn with_placeholder(existing_annotation: &str, placeholder: &str) -> Self {
        let mut editor = if existing_annotation.trim().is_empty() {
            TuiTextArea::default()
        } else {
            TuiTextArea::from(existing_annotation.lines())
        };
        editor.set_placeholder_text(placeholder);
        editor.set_tab_length(2);
        editor.set_max_histories(100);
        Self {
            editor,
            placeholder: placeholder.to_string(),
        }
    }
}

struct CommitRenameDialog {
    view: RecentChangeView,
    plan: CommitRenamePlan,
    message_input: TextInput,
    push_after_rename: bool,
}

impl CommitRenameDialog {
    fn new(view: RecentChangeView, plan: CommitRenamePlan) -> Self {
        let mut message_input = TextInput::with_value(plan.current_subject.clone());
        message_input.select_all();
        Self {
            view,
            plan,
            message_input,
            push_after_rename: false,
        }
    }
}

#[derive(Clone, Copy)]
enum BrowseTarget {
    WizardTargetPath,
    WizardRepoRoot,
    ProjectEditTargetPath,
    ProjectEditRepoRoot,
    ProjectSettingsChangelogPath,
    ProjectSettingsReleaseNowWindows,
    ProjectSettingsReleaseNowLinuxArm,
    ProjectSettingsReleaseNowLinuxAmd,
    ProjectSettingsReleaseNowMacOs,
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
        let explorer = configure_file_explorer(
            FileExplorerBuilder::default(),
            &initial_path,
            select_directories,
        )?;
        let title = match target {
            BrowseTarget::WizardRepoRoot | BrowseTarget::ProjectEditRepoRoot => "Browse Repo Root",
            BrowseTarget::ProjectSettingsChangelogPath => "Browse Changelog Path",
            BrowseTarget::ProjectSettingsReleaseNowWindows
            | BrowseTarget::ProjectSettingsReleaseNowLinuxArm
            | BrowseTarget::ProjectSettingsReleaseNowLinuxAmd
            | BrowseTarget::ProjectSettingsReleaseNowMacOs => "Browse Release Script",
            _ => "Browse Target Path",
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
        return builder
            .working_file(path)
            .build()
            .map_err(anyhow::Error::from);
    }
    if path.is_dir() {
        if select_directories {
            return builder
                .working_file(path)
                .build()
                .map_err(anyhow::Error::from);
        }
        return builder
            .working_dir(path)
            .build()
            .map_err(anyhow::Error::from);
    }

    if let Some(parent) = path.parent().filter(|parent| parent.is_dir()) {
        return builder
            .working_dir(parent.to_path_buf())
            .build()
            .map_err(anyhow::Error::from);
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

pub(crate) fn derive_repo_root_from_target_path(path: &str) -> Option<String> {
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
                    .fg(git_branch_color(
                        index.saturating_sub(graph_base_column) / 2,
                    ))
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

fn highlight_git_log_line(line: Line<'static>) -> Line<'static> {
    let highlight = Style::default().bg(Color::Rgb(55, 80, 140));
    Line::from(
        line.spans
            .into_iter()
            .map(|span| Span::styled(span.content, span.style.patch(highlight)))
            .collect::<Vec<_>>(),
    )
}

fn git_hash_color(prefix: &str, graph_base_column: usize) -> Option<Color> {
    prefix
        .chars()
        .enumerate()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .find_map(|(index, character)| {
            is_git_graph_character(character).then_some(git_branch_color(
                index.saturating_sub(graph_base_column) / 2,
            ))
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

        let end_byte = if end < indices.len() {
            indices[end].0
        } else {
            line.len()
        };
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

    let (visible_text, visible_cursor_col) =
        annotation_visible_segment(line, active_cursor_col.unwrap_or(0), content_width);
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

        if chars.is_empty() || (visible_cursor_col >= chars.len() && chars.len() < content_width) {
            spans.push(Span::styled(
                " ".to_string(),
                Style::default().fg(Color::Black).bg(Color::Cyan),
            ));
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

    let start = cursor_col
        .saturating_sub(width.saturating_sub(1))
        .min(characters.len().saturating_sub(width));
    let end = (start + width).min(characters.len());
    let visible = characters[start..end].iter().collect::<String>();
    (visible, cursor_col.saturating_sub(start))
}

pub(crate) fn dialog_form_row_height(viewport_height: u16) -> u16 {
    if viewport_height >= 8 {
        3
    } else if viewport_height >= 4 {
        2
    } else {
        1
    }
}

pub(crate) fn dialog_visible_rows(viewport_height: u16, row_height: u16) -> usize {
    (viewport_height / row_height.max(1)).max(1) as usize
}

pub(crate) fn clamp_dialog_scroll(
    scroll_offset: &mut usize,
    total_rows: usize,
    visible_rows: usize,
    focus_index: Option<usize>,
) {
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

fn render_vertical_overflow_indicators(
    frame: &mut Frame,
    area: Rect,
    show_above: bool,
    show_below: bool,
) {
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
            Paragraph::new("↑↑↑").alignment(Alignment::Right).style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
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
            Paragraph::new("↓↓↓").alignment(Alignment::Right).style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            bottom_rect,
        );
    }
}

pub(crate) fn rotate_scope_kind(scope_kind: BranchScopeKind, delta: i32) -> BranchScopeKind {
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

pub(crate) fn default_target_key_for_path(path: &str) -> &'static str {
    target_key_presets(path)[0]
}

pub(crate) fn target_key_is_custom(path: &str, value: &str) -> bool {
    !target_key_presets(path)
        .into_iter()
        .any(|preset| preset == value.trim())
}

pub(crate) fn cycle_target_key_preset(path: &str, current: &str, delta: i32) -> String {
    let presets = target_key_presets(path);
    let current_index = presets
        .iter()
        .position(|preset| *preset == current.trim())
        .unwrap_or(0) as i32;
    let next_index =
        (current_index + if delta >= 0 { 1 } else { -1 }).rem_euclid(presets.len() as i32) as usize;
    presets[next_index].to_string()
}

fn wizard_form_row_button(field: WizardField) -> Option<FormRowButton> {
    match field {
        WizardField::TargetPath => Some(FormRowButton::new(
            "Browse",
            HitAction::BrowseWizardTargetPath,
        )),
        WizardField::TargetKey => Some(FormRowButton::new(
            "Custom",
            HitAction::EnableWizardCustomTargetKey,
        )),
        WizardField::RepoRoot => Some(FormRowButton::new(
            "Browse",
            HitAction::BrowseWizardRepoRoot,
        )),
        _ => None,
    }
}

fn project_edit_form_row_button(field: ProjectEditFocus) -> Option<FormRowButton> {
    match field {
        ProjectEditFocus::TargetPath => Some(FormRowButton::new(
            "Browse",
            HitAction::BrowseProjectTargetPath,
        )),
        ProjectEditFocus::TargetKey => Some(FormRowButton::new(
            "Custom",
            HitAction::EnableProjectCustomTargetKey,
        )),
        ProjectEditFocus::RepoRoot => Some(FormRowButton::new(
            "Browse",
            HitAction::BrowseProjectRepoRoot,
        )),
        _ => None,
    }
}

fn dashboard_tile_columns(width: u16) -> usize {
    ((width + 1) / (TILE_WIDTH + 1)).max(1) as usize
}

fn rect_contains(rect: Rect, column: u16, row: u16) -> bool {
    column >= rect.x && column < rect.x + rect.width && row >= rect.y && row < rect.y + rect.height
}

fn digit_to_index(character: char) -> Option<usize> {
    character
        .to_digit(10)
        .and_then(|digit| digit.checked_sub(1))
        .map(|digit| digit as usize)
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

fn adjust_semver_overview_value(
    current: &str,
    control: OverviewVersionControl,
    delta: i32,
) -> Result<String> {
    let mut parts = current
        .split('.')
        .map(|part| {
            part.parse::<i32>()
                .map_err(|_| anyhow!("invalid semver component '{}'", part))
        })
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
    match control {
        OverviewVersionControl::Major => {
            parts[1] = 0;
            parts[2] = 0;
        }
        OverviewVersionControl::Minor => {
            parts[2] = 0;
        }
        OverviewVersionControl::Patch | OverviewVersionControl::Whole => {}
    }
    Ok(format!("{}.{}.{}", parts[0], parts[1], parts[2]))
}

fn adjust_numeric_tail_overview_value(current: &str, delta: i32) -> Result<String> {
    let mut parts = current
        .split('.')
        .map(|part| {
            part.parse::<i32>()
                .map_err(|_| anyhow!("invalid numeric component '{}'", part))
        })
        .collect::<Result<Vec<_>>>()?;
    let last = parts
        .last_mut()
        .ok_or_else(|| anyhow!("overview version is empty"))?;
    *last = (*last + delta).max(0);
    Ok(parts
        .into_iter()
        .map(|part| part.to_string())
        .collect::<Vec<_>>()
        .join("."))
}

fn browser_visible_range(total: usize, selected: usize, height: usize) -> (usize, usize) {
    if total == 0 || height == 0 {
        return (0, 0);
    }

    let start = selected
        .saturating_sub(height / 2)
        .min(total.saturating_sub(height));
    let end = (start + height).min(total);
    (start, end)
}

fn main_screen_from_index(index: usize) -> Screen {
    match index {
        1 => Screen::Wizard,
        2 => Screen::UiSettings,
        _ => Screen::Dashboard,
    }
}

fn header_height_for_viewport(_total_height: u16) -> u16 {
    if _total_height <= 18 {
        2
    } else if _total_height <= 22 {
        3
    } else if _total_height < 40 {
        7
    } else {
        9
    }
}

fn should_use_recent_changes_tab(area_height: u16, max_tile_height: u16) -> bool {
    area_height < max_tile_height.saturating_add(1).saturating_add(8)
}

fn main_tabs_shortcut_spans() -> Vec<Span<'static>> {
    shortcut_key_label("NUM", " Tabs")
}

fn ui_settings_footer_line() -> Line<'static> {
    let mut spans = main_tabs_shortcut_spans();
    spans.push(Span::raw(" | "));
    spans.extend(shortcut_key_label("T", "oggle Tab Hints"));
    spans.push(Span::raw(" | "));
    spans.extend(shortcut_key_label("C", "ycle Footer Content"));
    spans.push(Span::raw(" | "));
    spans.extend(shortcut_key_label("H", "ide Footer"));
    spans.push(Span::raw(" | "));
    spans.extend(shortcut_key_label("N", "ew Project"));
    spans.push(Span::raw(" | "));
    spans.extend(shortcut_key_label("Q", "uit"));
    Line::from(spans)
}

fn shortcut_token(token: &str) -> Vec<Span<'static>> {
    vec![Span::styled(
        token.to_string(),
        Style::default()
            .fg(SHORTCUT_HINT_COLOR)
            .add_modifier(Modifier::BOLD),
    )]
}

fn shortcut_key_label(key: &str, rest: &str) -> Vec<Span<'static>> {
    let mut spans = shortcut_token(key);
    spans.push(Span::raw(rest.to_string()));
    spans
}

impl App {
    fn main_tab_labels(&self) -> Vec<String> {
        ["Dashboard", "New Project", "UI Settings"]
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
            Screen::UiSettings => 2,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::changelog::build_document_from_git_log;

    #[test]
    fn derive_repo_root_uses_parent_directory() {
        let derived = derive_repo_root_from_target_path("C:/repo/subdir/package.json");
        assert_eq!(derived.as_deref(), Some("C:/repo/subdir"));
    }

    #[test]
    fn editing_repo_root_does_not_invalidate_target_probe() {
        let mut wizard = ProjectWizard {
            last_probe: Some(TargetProbe {
                kind: ProbeKind::Success,
                message: "ok".to_string(),
                version: Some("1.2.3".to_string()),
                format: Some(TargetFormat::Json),
            }),
            focus: WizardField::RepoRoot,
            ..ProjectWizard::default()
        };

        wizard.insert_text("C:/repo");

        assert!(matches!(
            wizard.last_probe.as_ref().map(|probe| probe.kind),
            Some(ProbeKind::Success)
        ));
    }

    #[test]
    fn compact_viewports_use_fixed_header_height() {
        assert_eq!(header_height_for_viewport(22), 3);
        assert_eq!(header_height_for_viewport(23), 7);
        assert_eq!(header_height_for_viewport(39), 7);
        assert_eq!(header_height_for_viewport(40), 9);
    }

    #[test]
    fn recent_changes_tab_appears_when_vertical_space_is_tight() {
        assert!(should_use_recent_changes_tab(15, 7));
        assert!(!should_use_recent_changes_tab(20, 7));
    }

    #[test]
    fn changelog_preview_release_notes_preserve_multiline_markdown() {
        let entry = ChangelogPreviewEntry {
            repo_root: "C:/repo".to_string(),
            changelog_path: "CHANGELOG.md".to_string(),
            stage_path: "CHANGELOG.md".to_string(),
            document: build_document_from_git_log(
                "v0.6.0",
                &["feat: add changelog preview".to_string()],
            ),
        };
        let mut dialog = ChangelogPreviewDialog::new(
            "Demo".to_string(),
            "0.6.0".to_string(),
            0,
            OverviewBumpWorkflow::CommitAndTag,
            vec![entry],
        );
        dialog.release_message = new_release_message_editor("Intro line\n\n- bullet item");

        let markdown = dialog.combined_preview_markdown();
        let pending_write = dialog.prepare_pending_write();

        assert!(markdown.contains("Intro line\n\n- bullet item"));
        assert!(
            pending_write.entries[0]
                .markdown
                .contains("Intro line\n\n- bullet item")
        );
    }

    #[test]
    fn editing_target_path_invalidates_target_probe() {
        let mut wizard = ProjectWizard {
            last_probe: Some(TargetProbe {
                kind: ProbeKind::Success,
                message: "ok".to_string(),
                version: Some("1.2.3".to_string()),
                format: Some(TargetFormat::Json),
            }),
            focus: WizardField::TargetPath,
            ..ProjectWizard::default()
        };

        wizard.insert_text("C:/repo/package.json");

        assert!(wizard.last_probe.is_none());
    }

    #[test]
    fn branched_wizard_builds_multiple_scopes() {
        let mut wizard = ProjectWizard {
            project_type: ProjectType::Branched,
            ..ProjectWizard::default()
        };
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

        let project = wizard
            .build_project()
            .expect("branched project should build");

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
        let mut wizard = ProjectWizard {
            project_type: ProjectType::Branched,
            ..ProjectWizard::default()
        };
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

        let error = wizard
            .build_project()
            .expect_err("duplicate scope names should fail");
        assert!(error.to_string().contains("unique"));
    }

    #[test]
    fn wizard_body_window_keeps_focused_field_visible_when_viewport_is_short() {
        let mut wizard = ProjectWizard {
            project_type: ProjectType::Branched,
            integration_mode: IntegrationMode::GitHubEnabled,
            focus: WizardField::RemoteUrl,
            ..ProjectWizard::default()
        };

        let (visible_fields, row_height, show_above, show_below) = wizard.refresh_body_window(6);

        assert_eq!(row_height, 2);
        assert!(visible_fields.contains(&WizardField::RemoteUrl));
        assert!(show_above);
        assert!(show_below);
    }

    #[test]
    fn target_key_switches_to_toml_default_when_target_path_changes() {
        let mut wizard = ProjectWizard {
            focus: WizardField::TargetPath,
            ..ProjectWizard::default()
        };

        wizard.insert_text("C:/repo/Cargo.toml");

        assert_eq!(wizard.target_key.value(), "package.version");
        assert!(!wizard.target_key_custom);
    }

    #[test]
    fn browser_modal_hit_resolution_ignores_background_targets() {
        let mut app = App::new_for_tests().expect("app should initialize");
        app.browser_dialog = Some(
            FileBrowserDialog::new(
                BrowseTarget::ProjectSettingsReleaseNowWindows,
                String::new(),
            )
            .expect("browser dialog should build"),
        );
        app.hit_targets.push(HitTarget::new(
            Rect {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            },
            HitAction::SelectProject(0),
        ));
        app.hit_targets.push(HitTarget::new(
            Rect {
                x: 0,
                y: 0,
                width: 10,
                height: 1,
            },
            HitAction::BrowserSelect(3),
        ));

        assert!(matches!(
            app.resolve_hit_action(0, 0, false),
            Some(HitAction::BrowserSelect(3))
        ));
    }

    #[test]
    fn pss_text_input_captures_global_shortcuts() {
        let mut app = App::new_for_tests().expect("app should initialize");
        app.config.projects = vec![ProjectConfig {
            name: "demo".to_string(),
            alias: String::new(),
            project_type: ProjectType::AllInOne,
            integration_mode: IntegrationMode::LocalOnly,
            unified_versioning: true,
            version_scheme: VersionScheme::SemVer,
            changelog: crate::config::ChangelogSettings::default(),
            release_now: crate::config::ReleaseNowSettings {
                enabled: true,
                ..Default::default()
            },
            tile_info: crate::config::TileInfoSettings::default(),
            targets: Vec::new(),
            branches: Vec::new(),
            repo: None,
        }];
        app.selected_project = 0;
        app.screen = Screen::Dashboard;
        app.dashboard_focus = DashboardPane::Overview;
        app.overview_tab = OverviewTab::ProjectSettings;
        app.project_settings_tab = ProjectSettingsTab::Distro;
        p_s_s::sync_project_settings_state(&mut app);
        app.project_settings_state.focus = ProjectSettingsFocus::ReleaseNowWindows;

        app.handle_key(KeyEvent::new(KeyCode::Char('2'), KeyModifiers::NONE))
            .expect("key handling should succeed");

        assert!(matches!(app.screen, Screen::Dashboard));
        assert_eq!(app.project_settings_state.release_now_windows.value(), "2");
    }

    #[test]
    fn dashboard_delete_shortcut_confirms_before_removing_project() {
        let mut app = App::new_for_tests().expect("app should initialize");
        app.config.projects = vec![ProjectConfig {
            name: "demo".to_string(),
            alias: String::new(),
            project_type: ProjectType::AllInOne,
            integration_mode: IntegrationMode::LocalOnly,
            unified_versioning: true,
            version_scheme: VersionScheme::SemVer,
            changelog: crate::config::ChangelogSettings::default(),
            release_now: crate::config::ReleaseNowSettings::default(),
            tile_info: crate::config::TileInfoSettings::default(),
            targets: Vec::new(),
            branches: Vec::new(),
            repo: None,
        }];
        app.screen = Screen::Dashboard;

        app.handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE))
            .expect("delete shortcut should open confirmation");

        assert!(app.delete_confirmation_dialog.is_some());
        assert_eq!(app.config.projects.len(), 1);

        app.handle_key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE))
            .expect("confirming deletion should succeed");

        assert!(app.delete_confirmation_dialog.is_none());
        assert!(app.config.projects.is_empty());
    }

    #[test]
    fn std_changelog_decision_generates_on_main_branch() {
        let decision = decide_std_changelog_generation(
            "v0.7.3",
            "main",
            &["main".to_string()],
            &["main".to_string()],
            None,
        );

        assert_eq!(decision, StdChangelogDecision::Generate);
    }

    #[test]
    fn custom_changelog_range_defaults_to_latest_tag_to_head() {
        let state = CustomChangelogRangeState::new(
            "main".to_string(),
            vec![
                "v1.2.0".to_string(),
                "v1.1.0".to_string(),
                "v1.0.0".to_string(),
            ],
            None,
        );

        assert_eq!(state.current_from_ref(), Some("v1.2.0"));
        assert_eq!(state.current_to_ref(), "HEAD");
        assert_eq!(state.range_label(), "v1.2.0..HEAD");
    }

    #[test]
    fn custom_changelog_range_keeps_to_ref_newer_than_from_ref() {
        let mut state = CustomChangelogRangeState::new(
            "main".to_string(),
            vec![
                "v1.2.0".to_string(),
                "v1.1.0".to_string(),
                "v1.0.0".to_string(),
            ],
            Some(CustomChangelogSelection {
                from_ref: "v1.0.0".to_string(),
                to_ref: Some("v1.2.0".to_string()),
            }),
        );

        assert_eq!(state.range_label(), "v1.0.0..v1.2.0");

        state.select_focus(CustomChangelogRangeFocus::From);
        assert!(state.adjust_focused_selection(-1));

        assert_eq!(state.current_from_ref(), Some("v1.1.0"));
        assert_eq!(state.current_to_ref(), "v1.2.0");
        assert_eq!(state.range_label(), "v1.1.0..v1.2.0");

        assert!(state.adjust_focused_selection(-1));
        assert_eq!(state.current_from_ref(), Some("v1.2.0"));
        assert_eq!(state.current_to_ref(), "HEAD");
    }

    #[test]
    fn std_changelog_decision_ignores_when_new_tag_is_not_on_main() {
        let decision = decide_std_changelog_generation(
            "v0.7.3",
            "feature-a",
            &["main".to_string()],
            &["feature-a".to_string()],
            None,
        );

        assert_eq!(decision, StdChangelogDecision::IgnoreNotOnMain);
    }

    #[test]
    fn std_changelog_decision_postpones_when_tags_share_single_sub_branch() {
        let decision = decide_std_changelog_generation(
            "v0.7.3",
            "feature-a",
            &["feature-a".to_string()],
            &["feature-a".to_string()],
            None,
        );

        assert_eq!(
            decision,
            StdChangelogDecision::PostponeOnSubBranch("feature-a".to_string())
        );
    }

    #[test]
    fn std_changelog_decision_normalizes_branch_markers() {
        let decision = decide_std_changelog_generation(
            "v0.7.3",
            "feature-a",
            &["* feature-a".to_string()],
            &["feature-a".to_string()],
            None,
        );

        assert_eq!(
            decision,
            StdChangelogDecision::PostponeOnSubBranch("feature-a".to_string())
        );
    }

    #[test]
    fn std_changelog_decision_generates_on_custom_main_branch() {
        let decision = decide_std_changelog_generation(
            "v0.7.3",
            "trunk",
            &["trunk".to_string()],
            &["trunk".to_string()],
            Some("trunk"),
        );

        assert_eq!(decision, StdChangelogDecision::Generate);
    }

    #[test]
    fn std_changelog_sub_branch_dialog_defaults_to_postpone() {
        let dialog = StdChangelogSubBranchDialog::new(
            PendingTagRequest {
                dialog: TagDialog {
                    project_name: "demo".to_string(),
                    scopes: Vec::new(),
                    selected_scope: 0,
                    tag_name: TextInput::with_value("v0.7.4"),
                    annotation: String::new(),
                    actions: vec![TagAction::CreateLocal],
                    integration_mode: IntegrationMode::GitLocalOnly,
                    action_index: 0,
                },
                changelog_enabled: true,
                std_changelog_policy: StdChangelogExecutionPolicy::Auto,
            },
            "v0.7.3".to_string(),
            "feature-a".to_string(),
        );

        assert!(matches!(
            dialog.selected_choice(),
            StdChangelogSubBranchChoice::Postpone
        ));
    }

    #[test]
    fn replay_uses_next_older_sorted_tag() {
        let tags = vec![
            "v0.7.5".to_string(),
            "v0.7.4".to_string(),
            "v0.7.3".to_string(),
        ];

        assert_eq!(
            previous_tag_for_replay(&tags, "v0.7.4"),
            Some("v0.7.3".to_string())
        );
        assert_eq!(previous_tag_for_replay(&tags, "v0.7.3"), None);
    }

    #[test]
    fn dashboard_delete_shortcut_removes_focused_scope_for_branched_projects() {
        let mut app = App::new_for_tests().expect("app should initialize");
        app.config.projects = vec![ProjectConfig {
            name: "demo".to_string(),
            alias: String::new(),
            project_type: ProjectType::Branched,
            integration_mode: IntegrationMode::LocalOnly,
            unified_versioning: false,
            version_scheme: VersionScheme::SemVer,
            changelog: crate::config::ChangelogSettings::default(),
            release_now: crate::config::ReleaseNowSettings::default(),
            tile_info: crate::config::TileInfoSettings::default(),
            targets: Vec::new(),
            branches: vec![
                BranchConfig {
                    name: "core".to_string(),
                    label: "Core".to_string(),
                    scope_kind: BranchScopeKind::Branch,
                    repo: None,
                    changelog_enabled: false,
                    changelog_path: None,
                    release_now: crate::config::ReleaseNowSettings::default(),
                    version_scheme: VersionScheme::SemVer,
                    targets: Vec::new(),
                },
                BranchConfig {
                    name: "api".to_string(),
                    label: "API".to_string(),
                    scope_kind: BranchScopeKind::Service,
                    repo: None,
                    changelog_enabled: false,
                    changelog_path: None,
                    release_now: crate::config::ReleaseNowSettings::default(),
                    version_scheme: VersionScheme::SemVer,
                    targets: Vec::new(),
                },
            ],
            repo: None,
        }];
        app.screen = Screen::Dashboard;
        app.overview_focused_scope = 1;

        app.handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE))
            .expect("delete shortcut should open scope confirmation");
        app.handle_key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE))
            .expect("confirming scope deletion should succeed");

        assert_eq!(app.config.projects.len(), 1);
        assert_eq!(app.config.projects[0].branches.len(), 1);
        assert_eq!(app.config.projects[0].branches[0].name, "core");
        assert_eq!(app.overview_focused_scope, 0);
    }

    #[test]
    fn cargo_lock_is_staged_for_relative_cargo_manifest_targets() {
        let unique = format!(
            "cg-stage-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        let repo_root = std::env::temp_dir().join(unique);
        let crate_dir = repo_root.join("core");
        std::fs::create_dir_all(&crate_dir).expect("crate dir");
        std::fs::write(
            crate_dir.join("Cargo.toml"),
            "[package]\nname='demo'\nversion='1.2.3'\n",
        )
        .expect("manifest");
        std::fs::write(crate_dir.join("Cargo.lock"), "# lock\n").expect("lockfile");

        let targets = vec![BumpTarget {
            label: "Version".to_string(),
            path: "core/Cargo.toml".to_string(),
            key_path: "package.version".to_string(),
            format: TargetFormat::Toml,
            current_version: "1.2.3".to_string(),
        }];

        let staged =
            git_flow::collect_stage_paths_for_targets(&repo_root.display().to_string(), &targets);

        assert_eq!(
            staged,
            vec!["core/Cargo.toml".to_string(), "core/Cargo.lock".to_string()]
        );

        let _ = std::fs::remove_dir_all(repo_root);
    }

    #[test]
    fn custom_target_key_mode_enables_text_entry() {
        let mut wizard = ProjectWizard {
            focus: WizardField::TargetKey,
            ..ProjectWizard::default()
        };

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
        let major_bumped = adjust_pending_version_value(
            VersionScheme::SemVer,
            "1.2.3",
            OverviewVersionControl::Major,
            1,
        )
        .expect("major bump should succeed");

        assert_eq!(incremented, "1.3.0");
        assert_eq!(decremented, "1.2.2");
        assert_eq!(major_bumped, "2.0.0");
    }

    #[test]
    fn github_bump_workflow_options_match_requested_order() {
        assert_eq!(
            overview_bump_workflow_options(IntegrationMode::GitHubEnabled),
            vec![
                OverviewBumpWorkflow::JustBump,
                OverviewBumpWorkflow::Commit,
                OverviewBumpWorkflow::CommitAndPush,
                OverviewBumpWorkflow::BranchCommit,
                OverviewBumpWorkflow::BranchCommitAndPush,
            ]
        );
    }

    #[test]
    fn overview_bump_kind_defaults_to_lowest_supported_increment() {
        let dialog = OverviewBumpKindDialog::new(
            "Demo".to_string(),
            "All configured scopes".to_string(),
            0,
            VersionScheme::SemVer,
            "1.2.3".to_string(),
            VersionScheme::SemVer.supported_actions().to_vec(),
        );

        assert_eq!(dialog.selected_action(), BumpAction::Patch);
    }
}
