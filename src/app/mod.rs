// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit SA-PS License
//
// For details, see the LICENSE file in the repository root.

use std::time::{Duration, Instant};

use anyhow::{Result, anyhow, bail};
use arboard::Clipboard;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Flex, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};
use ratatui_comfy_toaster::{ToastEngine, ToastEngineBuilder, ToastProgressBarStyle};
use tokio::{
    runtime::Runtime as TokioRuntime,
    sync::mpsc::{UnboundedReceiver, UnboundedSender, error::TryRecvError},
};
use tui_textarea::TextArea as TuiTextArea;

use crate::{
    changelog::write_changelog_markdown,
    config::{AppConfig, ConfigStore, FooterContent, IntegrationMode, ProjectConfig, ProjectType},
    git::{GitCancellation, RepoActivitySummary, collect_all_branch_git_scope_contexts},
    tui::{
        HelpModal, OverviewTab, OverviewTileData, PixelLogo, ProjectEditDialog, ProjectEditFocus,
        ProjectWizard, TILE_WIDTH, WizardField, center_vertically, centered_rect,
        choose_header_content, overview_tab_rects, render_overview_tabs, render_overview_tile,
        tile_height,
    },
    workflow::{
        dialogs::{BumpDialog, RecentChangesDialog, RecentChangesTab, TagAction, TagDialog},
        targets::{BumpScope, ProbeKind, collect_bump_scopes, write_target_version},
        versioning::VersionScheme,
    },
};

mod help_context;
mod overview;
mod project_settings;
mod ps_alias;
mod render;

use self::project_settings::{ProjectSettingsState, ProjectSettingsTab};
use crate::changelog::top_picks as changelog_tp;
pub(crate) use crate::workflow::rls_now;
pub(crate) use crate::workflow::{OverviewBumpWorkflow, git_flow, overview_bump_workflow_options};

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
const SUPPORT_EMAIL: &str = " dev@comfyhome.io ";
pub(crate) const FORM_LABEL_WIDTH: u16 = 18;
pub(crate) const BROWSE_BUTTON_WIDTH: u16 = 12;
const BUTTON_ROW_HEIGHT: u16 = 3;
const BUTTON_GAP_HEIGHT: u16 = 3;
pub(crate) const SHORTCUT_HINT_COLOR: Color = Color::Yellow;
pub(crate) const ACTIVE_UI_TICK_INTERVAL: Duration = Duration::from_millis(100);
pub(crate) const IDLE_UI_POLL_INTERVAL: Duration = Duration::from_secs(1);
pub(crate) const GIT_BRANCH_COLORS: [Color; 6] = [
    Color::Green,
    Color::Cyan,
    Color::Yellow,
    Color::Magenta,
    Color::Blue,
    Color::Red,
];

mod terminal;

pub use terminal::run;

pub(crate) struct App {
    config_store: ConfigStore,
    config: AppConfig,
    screen: Screen,
    selected_project: usize,
    dashboard_focus: DashboardPane,
    clipboard: Option<Clipboard>,
    fallback_clipboard: Option<String>,
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
    project_rects: Vec<(Rect, usize)>,
    drag_project: Option<usize>,
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
    top_picks_editor_dialog: Option<changelog_tp::TopPicksEditorDialog>,
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
    pub(crate) snif_dialog: Option<crate::tui::SnifModal>,
    help_modal: Option<HelpModal>,
    overview_tab_strip_area: Option<Rect>,
    project_settings_tab_strip_area: Option<Rect>,
    recent_changes_tab_strip_area: Option<Rect>,
    last_mouse_column: u16,
    last_mouse_row: u16,
    has_mouse_position: bool,
    hit_targets: Vec<HitTarget>,
    last_text_input_click_target: Option<TextInputClickTarget>,
    last_text_input_click_at: Option<Instant>,
    last_recent_change_click_target: Option<RecentChangeClickTarget>,
    last_recent_change_click_at: Option<Instant>,
    commit_rename_textarea_click_at: Option<Instant>,
    commit_rename_textarea_rect: Option<Rect>,
    release_now_notes_textarea_click_at: Option<Instant>,
    top_picks_editor_click_at: Option<Instant>,
    top_picks_editor_rect: Option<Rect>,
    pub(crate) status: StatusMessage,
    last_status_toast_id: u64,
    toaster: ToastEngine<()>,
    logo: PixelLogo,
    footer_auto_hidden: bool,
    footer_manual_override: bool,
    pending_changelog_write: Option<PendingChangelogWrite>,
    should_quit: bool,
}

mod background;
mod input;
mod state;
mod widgets;

#[cfg(target_os = "linux")]
pub(crate) use terminal::{copy_text_via_linux_clipboard_cli, paste_from_linux_clipboard_cli};

pub(crate) use background::*;
pub(crate) use state::*;
pub(crate) use widgets::*;

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
        let clipboard = Clipboard::new().ok();
        Ok(Self {
            config_store,
            config,
            screen: Screen::Dashboard,
            selected_project: 0,
            dashboard_focus: DashboardPane::Projects,
            clipboard,
            fallback_clipboard: None,
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
            project_rects: Vec::new(),
            drag_project: None,
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
            top_picks_editor_dialog: None,
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
            snif_dialog: None,
            help_modal: None,
            overview_tab_strip_area: None,
            project_settings_tab_strip_area: None,
            recent_changes_tab_strip_area: None,
            last_mouse_column: 0,
            last_mouse_row: 0,
            has_mouse_position: false,
            hit_targets: Vec::new(),
            last_text_input_click_target: None,
            last_text_input_click_at: None,
            last_recent_change_click_target: None,
            last_recent_change_click_at: None,
            commit_rename_textarea_click_at: None,
            commit_rename_textarea_rect: None,
            release_now_notes_textarea_click_at: None,
            top_picks_editor_click_at: None,
            top_picks_editor_rect: None,
            last_status_toast_id: status.id,
            toaster: ToastEngineBuilder::new(Rect::default())
                .default_duration(Duration::from_secs(2))
                .default_progress_bar(true)
                .default_progress_bar_style(ToastProgressBarStyle::Minimal)
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::changelog::build_document_from_git_log;
    use crate::config::{BranchConfig, BranchScopeKind, RepoConfig, TargetFormat, TargetSpec};
    use crate::tui::HelpContext;
    use crate::workflow::dialogs::TextInput;
    use crate::workflow::targets::{BumpTarget, ProbeKind, TargetProbe};
    use crate::workflow::versioning::BumpAction;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::project_settings::ProjectSettingsFocus;

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
        if let Some(scope) = wizard.current_scope_mut() {
            scope.integration_mode = IntegrationMode::GitHubEnabled;
        }

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
            ..Default::default()
        }];
        app.selected_project = 0;
        app.screen = Screen::Dashboard;
        app.dashboard_focus = DashboardPane::Overview;
        app.overview_tab = OverviewTab::ProjectSettings;
        app.project_settings_tab = ProjectSettingsTab::Distro;
        project_settings::sync_project_settings_state(&mut app);
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
            ..Default::default()
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
                    changelog_hide_pr_messages: false,
                    changelog_hide_bump_messages: false,
                    changelog_mini_commit_hashes: false,
                    changelog_mirror_summary_to_root_changelog: false,
                    changelog_wrap_detailed_if_top_picks: false,
                    release_now: crate::config::ReleaseNowSettings::default(),
                    version_scheme: VersionScheme::SemVer,
                    targets: Vec::new(),
                    advanced_alias: Default::default(),
                },
                BranchConfig {
                    name: "api".to_string(),
                    label: "API".to_string(),
                    scope_kind: BranchScopeKind::Service,
                    repo: None,
                    changelog_enabled: false,
                    changelog_path: None,
                    changelog_hide_pr_messages: false,
                    changelog_hide_bump_messages: false,
                    changelog_mini_commit_hashes: false,
                    changelog_mirror_summary_to_root_changelog: false,
                    changelog_wrap_detailed_if_top_picks: false,
                    release_now: crate::config::ReleaseNowSettings::default(),
                    version_scheme: VersionScheme::SemVer,
                    targets: Vec::new(),
                    advanced_alias: Default::default(),
                },
            ],
            repo: None,
            ..Default::default()
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
    fn project_edit_opens_branched_scope_from_focused_tile() {
        let mut app = App::new_for_tests().expect("app should initialize");
        app.config.projects = vec![ProjectConfig {
            name: "demo".to_string(),
            alias: String::new(),
            project_type: ProjectType::Branched,
            integration_mode: IntegrationMode::GitHubEnabled,
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
                    repo: Some(RepoConfig {
                        local_root: "C:/core".to_string(),
                        remote_url: Some("https://example.test/core.git".to_string()),
                        ..RepoConfig::default()
                    }),
                    changelog_enabled: false,
                    changelog_path: None,
                    changelog_hide_pr_messages: false,
                    changelog_hide_bump_messages: false,
                    changelog_mini_commit_hashes: false,
                    changelog_mirror_summary_to_root_changelog: false,
                    changelog_wrap_detailed_if_top_picks: false,
                    release_now: crate::config::ReleaseNowSettings::default(),
                    version_scheme: VersionScheme::SemVer,
                    targets: vec![TargetSpec {
                        label: "Version".to_string(),
                        path: "C:/core/Cargo.toml".to_string(),
                        key_path: "package.version".to_string(),
                        format: TargetFormat::Toml,
                    }],
                    advanced_alias: Default::default(),
                },
                BranchConfig {
                    name: "api".to_string(),
                    label: "API".to_string(),
                    scope_kind: BranchScopeKind::Service,
                    repo: Some(RepoConfig {
                        local_root: "C:/api".to_string(),
                        remote_url: Some("https://example.test/api.git".to_string()),
                        ..RepoConfig::default()
                    }),
                    changelog_enabled: false,
                    changelog_path: None,
                    changelog_hide_pr_messages: false,
                    changelog_hide_bump_messages: false,
                    changelog_mini_commit_hashes: false,
                    changelog_mirror_summary_to_root_changelog: false,
                    changelog_wrap_detailed_if_top_picks: false,
                    release_now: crate::config::ReleaseNowSettings::default(),
                    version_scheme: VersionScheme::CalVerYearMonthMicro,
                    targets: vec![TargetSpec {
                        label: "Version".to_string(),
                        path: "C:/api/package.json".to_string(),
                        key_path: "package.version".to_string(),
                        format: TargetFormat::Json,
                    }],
                    advanced_alias: Default::default(),
                },
            ],
            repo: None,
            ..Default::default()
        }];
        app.screen = Screen::Dashboard;
        app.overview_focused_scope = 1;

        app.open_project_edit_dialog()
            .expect("project edit should open");

        let dialog = app
            .project_edit_dialog
            .as_ref()
            .expect("project edit dialog should be present");
        assert_eq!(dialog.selected_scope, 1);
        assert_eq!(dialog.focus, ProjectEditFocus::ProjectType);
        assert_eq!(dialog.repo_root.value(), "C:/api");
    }

    #[test]
    fn project_edit_remove_scope_requests_delete_when_last_scope_remains() {
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
            branches: vec![BranchConfig {
                name: "core".to_string(),
                label: "Core".to_string(),
                scope_kind: BranchScopeKind::Branch,
                repo: None,
                changelog_enabled: false,
                changelog_path: None,
                changelog_hide_pr_messages: false,
                changelog_hide_bump_messages: false,
                changelog_mini_commit_hashes: false,
                changelog_mirror_summary_to_root_changelog: false,
                changelog_wrap_detailed_if_top_picks: false,
                release_now: crate::config::ReleaseNowSettings::default(),
                version_scheme: VersionScheme::SemVer,
                targets: vec![TargetSpec {
                    label: "Version".to_string(),
                    path: "Cargo.toml".to_string(),
                    key_path: "package.version".to_string(),
                    format: TargetFormat::Toml,
                }],
                advanced_alias: Default::default(),
            }],
            repo: None,
            ..Default::default()
        }];

        app.open_project_edit_dialog()
            .expect("project edit should open");
        if let Some(dialog) = &mut app.project_edit_dialog {
            dialog.focus = ProjectEditFocus::RemoveScope;
        }

        app.handle_key(KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE))
            .expect("delete should open confirmation");

        assert!(app.delete_confirmation_dialog.is_some());
    }

    #[test]
    fn project_edit_empty_new_scope_uses_titled_error_status() {
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
            branches: vec![BranchConfig {
                name: "core".to_string(),
                label: "Core".to_string(),
                scope_kind: BranchScopeKind::Branch,
                repo: None,
                changelog_enabled: false,
                changelog_path: None,
                changelog_hide_pr_messages: false,
                changelog_hide_bump_messages: false,
                changelog_mini_commit_hashes: false,
                changelog_mirror_summary_to_root_changelog: false,
                changelog_wrap_detailed_if_top_picks: false,
                release_now: crate::config::ReleaseNowSettings::default(),
                version_scheme: VersionScheme::SemVer,
                targets: vec![TargetSpec {
                    label: "Version".to_string(),
                    path: "Cargo.toml".to_string(),
                    key_path: "package.version".to_string(),
                    format: TargetFormat::Toml,
                }],
                advanced_alias: Default::default(),
            }],
            repo: None,
            ..Default::default()
        }];

        app.open_project_edit_dialog()
            .expect("project edit should open");
        if let Some(dialog) = &mut app.project_edit_dialog {
            dialog.add_scope();
        }

        app.save_project_edit().expect("save should be handled");

        assert_eq!(
            app.status.text,
            "scope 'scope-2' target path cannot be empty"
        );
        assert_eq!(app.status.toast_preset, Some(NEW_SCOPE_ERROR_TOAST_PRESET));
        assert_eq!(app.status.toast_title.as_deref(), Some("New Scope:"));
        assert!(app.project_edit_dialog.is_some());
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

    #[test]
    fn question_mark_toggles_context_help() -> Result<()> {
        let mut app = App::new_for_tests()?;
        assert!(app.help_modal.is_none());

        app.handle_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE))?;
        let modal = app.help_modal.as_ref().expect("help should open");
        assert_eq!(modal.context, HelpContext::DashboardProjects);

        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))?;
        assert!(app.help_modal.is_none());
        Ok(())
    }
}
