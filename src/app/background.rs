// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit SA-PS License

use std::{
    fs,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use anyhow::{Context, Result, anyhow, bail};
use ratatui_comfy_toaster::ToastPreset;
use tokio::{
    runtime::{Builder as TokioRuntimeBuilder, Runtime as TokioRuntime},
    sync::{
        Semaphore,
        mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel},
    },
    task::JoinSet,
};

use crate::{
    changelog::{
        archive_changelog_markdown, find_archived_changelog_markdown,
        load_merged_std_changelog_memory, load_top_picks_edits, rebuild_history_summary_readme,
        record_std_changelog_created, record_std_changelog_error, record_std_changelog_generated,
        record_std_changelog_postponed, rls_changelog_gen, std_changelog_gen,
    },
    config::{
        BranchConfig, BranchScopeKind, IntegrationMode, ProjectConfig, RepoConfig, TargetFormat,
        TargetSpec,
    },
    git::BranchNameOption,
    git::{
        GitCancellation, RepoActivitySummary, branches_containing_ref_with_cancel,
        collect_all_branch_git_scope_contexts, current_branch_with_cancel, ensure_local_tag,
        is_mainline_branch_name, load_scope_activity_summary_with_cancel,
        semver_dev_branch_canonical_label, sorted_local_tags_with_cancel,
    },
    workflow::dialogs::{
        ChangeRange, RecentChangesDialog, RecentChangesTab, TagAction, TagDialog, TextInput,
        load_change_range_for_refs_with_cancel, load_change_range_for_tags_with_cancel,
        load_history_ranges_with_cancel, load_recent_change_range_with_cancel,
    },
    workflow::rls_now::format_branched_scope_tag,
    workflow::targets::{ProbeKind, TargetProbe, collect_bump_scopes},
    workflow::versioning::{BumpAction, VersionScheme},
    workflow::{OverviewBumpWorkflow, git_flow},
};

use super::{
    App, ChangelogPreviewDialog, CustomChangelogSelection, clamp_dialog_scroll, overview, rls_now,
    sanitize_tag_fragment, target_key_is_custom,
};

pub(crate) const BACKGROUND_MAX_PARALLEL_REPO_JOBS: usize = 4;
const GH_RELEASE_TIMEOUT: Duration = Duration::from_secs(45);
pub(crate) use crate::workflow::runtime::{
    GIT_PUSH_TIMEOUT, NETWORK_RETRY_ATTEMPTS, run_blocking_job, run_command_with_retry_async,
};
pub(crate) enum BackgroundJobRequest {
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
        request: Box<rls_now::ReleaseNowExecutionRequest>,
    },
    ReleaseNowMirrorSync {
        repo_root: String,
        gitlab_remote: Option<String>,
        github_remote: Option<String>,
        push: bool,
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

pub(crate) enum BackgroundJobOutput {
    OpenRecentChanges(RecentChangesDialog),
    PendingBumpMainBranch {
        integration_mode: IntegrationMode,
        repos: Vec<git_flow::RepoBranchState>,
        pending_action: PendingBumpAction,
    },
    OverviewBumpWarnings {
        scope_index: usize,
        workflow: OverviewBumpWorkflow,
        warnings: Vec<git_flow::UnexpectedStagedRepo>,
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
    ReleaseNowValidated(Box<rls_now::ReleaseNowValidation>),
    ReleaseNowLogChunk(Vec<String>),
    ReleaseNowCompleted(rls_now::ReleaseNowExecutionOutcome),
    ReleaseNowMirrorSyncResult(rls_now::ReleaseNowMirrorSyncResult),
    CreateTag {
        summary: String,
        replay_notices: Vec<String>,
        replay_errors: Vec<String>,
    },
}

pub(crate) type BackgroundJobResult = std::result::Result<BackgroundJobOutput, String>;
pub(crate) type BackgroundWorkerChannels = (
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

pub(crate) enum BackgroundJobMessagePayload {
    Progress(BackgroundJobOutput),
    Finished(BackgroundJobResult),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum BackgroundJobKind {
    RecentChanges,
    RepoScan,
    RecentChangesPrefetch,
    ChangelogPreview,
    OverviewActivity,
    ReleaseNow,
    TagAction,
}

#[derive(Clone, Copy)]
pub(crate) enum BackgroundJobPriority {
    Foreground,
    Refresh,
    Prefetch,
}

pub(crate) struct BackgroundJobRequestMessage {
    pub(crate) id: u64,
    pub(crate) kind: BackgroundJobKind,
    pub(crate) request: BackgroundJobRequest,
    pub(crate) cancel: GitCancellation,
}

pub(crate) struct BackgroundJobResultMessage {
    pub(crate) id: u64,
    pub(crate) kind: BackgroundJobKind,
    pub(crate) payload: BackgroundJobMessagePayload,
}

#[derive(Clone)]
pub(crate) struct BackgroundJobProgressSink {
    pub(crate) id: u64,
    pub(crate) kind: BackgroundJobKind,
    pub(crate) result_tx: UnboundedSender<BackgroundJobResultMessage>,
}

impl BackgroundJobProgressSink {
    pub(crate) fn send(&self, output: BackgroundJobOutput) {
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
            Self::ValidateReleaseNow { .. }
            | Self::RunReleaseNow { .. }
            | Self::ReleaseNowMirrorSync { .. } => BackgroundJobKind::ReleaseNow,
            Self::CreateTag { .. } => BackgroundJobKind::TagAction,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum RecentChangesLoadAction {
    RefreshCurrentScope,
    RotateScope(isize),
    SwitchTab(RecentChangesTab),
}

#[derive(Clone)]
pub(crate) struct OverviewBumpKindDialog {
    pub(crate) project_name: String,
    pub(crate) scope_label: String,
    pub(crate) scope_index: usize,
    pub(crate) scheme: VersionScheme,
    pub(crate) current_version: String,
    pub(crate) options: Vec<BumpAction>,
    pub(crate) selected: usize,
}

#[derive(Clone)]
pub(crate) struct OverviewBumpWorkflowDialog {
    pub(crate) project_name: String,
    pub(crate) scope_label: String,
    pub(crate) next_version: String,
    pub(crate) scope_index: usize,
    pub(crate) options: Vec<OverviewBumpWorkflow>,
    pub(crate) selected: usize,
    pub(crate) scroll: usize,
}

#[derive(Clone)]
pub(crate) struct OverviewBranchBumpDialog {
    pub(crate) project_name: String,
    pub(crate) scope_label: String,
    pub(crate) next_version: String,
    pub(crate) scope_index: usize,
    pub(crate) workflow: OverviewBumpWorkflow,
    pub(crate) options: Vec<BranchNameOption>,
    pub(crate) selected: usize,
    pub(crate) branch_name: TextInput,
    pub(crate) scroll: u16,
}

impl OverviewBranchBumpDialog {
    pub(crate) fn new(
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

    pub(crate) fn selected_option(&self) -> &BranchNameOption {
        &self.options[self.selected.min(self.options.len().saturating_sub(1))]
    }

    pub(crate) fn select(&mut self, index: usize) {
        self.selected = index.min(self.options.len().saturating_sub(1));
    }

    pub(crate) fn rotate(&mut self, delta: isize) {
        if self.options.is_empty() {
            self.selected = 0;
            return;
        }

        let len = self.options.len() as isize;
        self.selected = (self.selected as isize + delta).rem_euclid(len) as usize;
    }

    pub(crate) fn input_enabled(&self) -> bool {
        self.selected_option().requires_input()
    }

    pub(crate) fn input_label(&self) -> &'static str {
        self.selected_option().input_label()
    }

    pub(crate) fn input_hint(&self) -> &'static str {
        self.selected_option().input_hint()
    }

    pub(crate) fn branch_preview(&self) -> String {
        self.selected_option()
            .preview_with_input(Some(self.branch_name.value.trim()))
    }

    pub(crate) fn resolved_branch_name(&self) -> Result<String> {
        self.selected_option()
            .resolve_name(Some(self.branch_name.value.trim()))
    }

    pub(crate) fn scroll_by(&mut self, delta: i16) {
        self.scroll = if delta < 0 {
            self.scroll.saturating_sub(delta.unsigned_abs())
        } else {
            self.scroll.saturating_add(delta as u16)
        };
    }
}

impl OverviewBumpWorkflowDialog {
    pub(crate) fn new(
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

    pub(crate) fn selected_workflow(&self) -> OverviewBumpWorkflow {
        self.options[self.selected.min(self.options.len().saturating_sub(1))]
    }

    pub(crate) fn select(&mut self, index: usize) {
        self.selected = index.min(self.options.len().saturating_sub(1));
    }

    pub(crate) fn rotate(&mut self, delta: isize) {
        if self.options.is_empty() {
            self.selected = 0;
            return;
        }

        let len = self.options.len() as isize;
        self.selected = (self.selected as isize + delta).rem_euclid(len) as usize;
    }

    pub(crate) fn clamp_scroll(&mut self, visible_rows: usize) {
        clamp_dialog_scroll(
            &mut self.scroll,
            self.options.len(),
            visible_rows,
            Some(self.selected),
        );
    }
}

impl OverviewBumpKindDialog {
    pub(crate) fn new(
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

    pub(crate) fn selected_action(&self) -> BumpAction {
        self.options[self.selected.min(self.options.len().saturating_sub(1))]
    }

    pub(crate) fn preview_next_version(&self) -> Result<String> {
        self.scheme
            .bump(
                &self.current_version,
                self.selected_action(),
                chrono::Local::now().date_naive(),
            )
            .map_err(anyhow::Error::msg)
    }

    pub(crate) fn select(&mut self, index: usize) {
        self.selected = index.min(self.options.len().saturating_sub(1));
    }

    pub(crate) fn rotate(&mut self, delta: isize) {
        if self.options.is_empty() {
            self.selected = 0;
            return;
        }

        let len = self.options.len() as isize;
        self.selected = (self.selected as isize + delta).rem_euclid(len) as usize;
    }
}

#[derive(Clone)]
pub(crate) struct OverviewBumpWarningDialog {
    pub(crate) scope_index: usize,
    pub(crate) workflow: OverviewBumpWorkflow,
    pub(crate) repos: Vec<git_flow::UnexpectedStagedRepo>,
    pub(crate) selected: usize,
}

impl OverviewBumpWarningDialog {
    pub(crate) fn new(
        scope_index: usize,
        workflow: OverviewBumpWorkflow,
        repos: Vec<git_flow::UnexpectedStagedRepo>,
    ) -> Self {
        Self {
            scope_index,
            workflow,
            repos,
            selected: 1,
        }
    }

    pub(crate) fn select(&mut self, index: usize) {
        self.selected = index.min(2);
    }

    pub(crate) fn rotate(&mut self, delta: isize) {
        self.selected = (self.selected as isize + delta).rem_euclid(3) as usize;
    }

    pub(crate) fn selected_choice(&self) -> OverviewBumpWarningChoice {
        match self.selected {
            0 => OverviewBumpWarningChoice::Continue,
            1 => OverviewBumpWarningChoice::UnstageExtras,
            _ => OverviewBumpWarningChoice::Cancel,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum OverviewBumpWarningChoice {
    Continue,
    UnstageExtras,
    Cancel,
}

#[derive(Clone, Copy)]
pub(crate) enum PendingBumpAction {
    Standard,
    OverviewWorkflow { scope_index: usize },
}

#[derive(Clone)]
pub(crate) struct MainBranchWarningDialog {
    pub(crate) integration_mode: IntegrationMode,
    pub(crate) repos: Vec<git_flow::RepoBranchState>,
    pub(crate) pending_action: PendingBumpAction,
    pub(crate) selected: usize,
}

impl MainBranchWarningDialog {
    pub(crate) fn new(
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

    pub(crate) fn switch_label(&self) -> &'static str {
        match self.integration_mode {
            IntegrationMode::GitHubEnabled
            | IntegrationMode::GitLabEnabled
            | IntegrationMode::GitLabGitHubEnabled => "Switch to mainline & Sync & Bump",
            IntegrationMode::GitLocalOnly => "Switch to mainline & Bump",
            IntegrationMode::LocalOnly => "Continue",
        }
    }

    pub(crate) fn select(&mut self, index: usize) {
        self.selected = index.min(2);
    }

    pub(crate) fn rotate(&mut self, delta: isize) {
        self.selected = (self.selected as isize + delta).rem_euclid(3) as usize;
    }

    pub(crate) fn selected_choice(&self) -> MainBranchWarningChoice {
        match self.selected {
            0 => MainBranchWarningChoice::SwitchToMain,
            1 => MainBranchWarningChoice::IgnoreAndContinue,
            _ => MainBranchWarningChoice::Cancel,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum MainBranchWarningChoice {
    SwitchToMain,
    IgnoreAndContinue,
    Cancel,
}

#[derive(Clone)]
pub(crate) struct StdChangelogSubBranchDialog {
    pub(crate) pending_request: PendingTagRequest,
    pub(crate) previous_tag: String,
    pub(crate) branch_name: String,
    pub(crate) selected: usize,
}

impl StdChangelogSubBranchDialog {
    pub(crate) fn new(
        pending_request: PendingTagRequest,
        previous_tag: String,
        branch_name: String,
    ) -> Self {
        Self {
            pending_request,
            previous_tag,
            branch_name,
            selected: 1,
        }
    }

    pub(crate) fn select(&mut self, index: usize) {
        self.selected = index.min(2);
    }

    pub(crate) fn rotate(&mut self, delta: isize) {
        self.selected = (self.selected as isize + delta).rem_euclid(3) as usize;
    }

    pub(crate) fn selected_choice(&self) -> StdChangelogSubBranchChoice {
        match self.selected {
            0 => StdChangelogSubBranchChoice::GenerateNow,
            1 => StdChangelogSubBranchChoice::Postpone,
            _ => StdChangelogSubBranchChoice::Cancel,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum StdChangelogSubBranchChoice {
    GenerateNow,
    Postpone,
    Cancel,
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
    pub(crate) integration_mode: IntegrationMode,
    pub(crate) version_scheme: VersionScheme,
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
            integration_mode: IntegrationMode::LocalOnly,
            version_scheme: VersionScheme::SemVer,
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
            integration_mode: match branch.repo.as_ref() {
                None => IntegrationMode::LocalOnly,
                Some(repo) => crate::forge::integration_mode_for_repo_config(repo),
            },
            version_scheme: branch.version_scheme,
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

    pub(crate) fn build_branch(&self, require_probe: bool) -> Result<BranchConfig> {
        let name = self.name.value.trim();
        if name.is_empty() {
            bail!("scope name cannot be empty");
        }

        let target_path = self.target_path.value.trim();
        if target_path.is_empty() {
            bail!("scope '{}' target path cannot be empty", name);
        }

        let target_key = self.target_key.value.trim();
        if target_key.is_empty()
            && !crate::workflow::targets::is_plain_version_filename(target_path)
        {
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
            changelog_hide_pr_messages: false,
            changelog_hide_bump_messages: false,
            changelog_mini_commit_hashes: false,
            changelog_mirror_summary_to_root_changelog: false,
            changelog_wrap_detailed_if_top_picks: false,
            release_now: crate::config::ReleaseNowSettings::default(),
            version_scheme: self.version_scheme,
            targets: vec![TargetSpec {
                label: self.target_label.clone(),
                path: target_path.to_string(),
                key_path: target_key.to_string(),
                format,
            }],
            advanced_alias: crate::config::AdvancedAliasSettings::default(),
        })
    }
}

/// Change this preset and rebuild to preview the branched project-edit
/// "New Scope" validation error toast layout.
#[cfg(test)]
pub(crate) const NEW_SCOPE_ERROR_TOAST_PRESET: ToastPreset = ToastPreset::CompactHighlightStart;
#[cfg(not(test))]
const NEW_SCOPE_ERROR_TOAST_PRESET: ToastPreset = ToastPreset::MessageOnly;

#[derive(Clone)]
pub(crate) struct StatusMessage {
    pub(crate) id: u64,
    pub(crate) kind: StatusKind,
    pub(crate) text: String,
    pub(crate) toast_title: Option<String>,
    pub(crate) toast_preset: Option<ToastPreset>,
}

impl StatusMessage {
    pub(crate) fn info(text: impl Into<String>) -> Self {
        Self::new(StatusKind::Info, text)
    }

    pub(crate) fn success(text: impl Into<String>) -> Self {
        Self::new(StatusKind::Success, text)
    }

    pub(crate) fn warning(text: impl Into<String>) -> Self {
        Self::new(StatusKind::Warning, text)
    }

    pub(crate) fn error(text: impl Into<String>) -> Self {
        Self::new(StatusKind::Error, text)
    }

    pub(crate) fn with_toast_preset(
        mut self,
        preset: ToastPreset,
        title: impl Into<String>,
    ) -> Self {
        self.toast_preset = Some(preset);
        self.toast_title = Some(title.into());
        self
    }

    pub(crate) fn with_new_scope_toast_preview(mut self) -> Self {
        self.toast_preset = Some(NEW_SCOPE_ERROR_TOAST_PRESET);
        self.toast_title = Some("New Scope:".to_string());
        self
    }

    pub(crate) fn new(kind: StatusKind, text: impl Into<String>) -> Self {
        static NEXT_STATUS_ID: AtomicU64 = AtomicU64::new(1);
        Self {
            id: NEXT_STATUS_ID.fetch_add(1, Ordering::Relaxed),
            kind,
            text: text.into(),
            toast_title: None,
            toast_preset: None,
        }
    }
}

pub(crate) fn bump_toast_status(text: impl Into<String>) -> StatusMessage {
    StatusMessage::info(text).with_toast_preset(ToastPreset::GappedDotHighlightCenter, "Bump")
}

#[derive(Clone, Copy)]
pub(crate) enum StatusKind {
    Info,
    Success,
    Warning,
    Error,
}

pub(crate) fn spawn_background_worker() -> Result<BackgroundWorkerChannels> {
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

pub(crate) fn spawn_background_job_task(
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
        } => Ok(BackgroundJobOutput::ReleaseNowValidated(Box::new(
            run_blocking_job(move || {
                rls_now::validate_release_now(&project, scope_index, Some(cancel))
            })
            .await?,
        ))),
        BackgroundJobRequest::RunReleaseNow { request } => {
            Ok(BackgroundJobOutput::ReleaseNowCompleted(
                rls_now::execute_release_now_async(*request, cancel, move |lines| {
                    progress.send(BackgroundJobOutput::ReleaseNowLogChunk(lines));
                })
                .await?,
            ))
        }
        BackgroundJobRequest::ReleaseNowMirrorSync {
            repo_root,
            gitlab_remote,
            github_remote,
            push,
        } => Ok(BackgroundJobOutput::ReleaseNowMirrorSyncResult(
            run_blocking_job(move || {
                rls_now::run_mirror_sync_operation(
                    &repo_root,
                    gitlab_remote.as_deref(),
                    github_remote.as_deref(),
                    push,
                )
            })
            .await?,
        )),
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
                Ok(load_scope_activity_summary_with_cancel(
                    &context,
                    project.integration_mode,
                    cancel,
                )
                .ok())
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

pub(crate) fn apply_recent_changes_background_action(
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

pub(crate) fn prefetch_recent_changes(
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

pub(crate) struct BackgroundTagOutcome {
    pub(crate) summary: String,
    pub(crate) replay_notices: Vec<String>,
    pub(crate) replay_errors: Vec<String>,
}

#[derive(Default)]
pub(crate) struct PostponedReplayOutcome {
    pub(crate) notices: Vec<String>,
    pub(crate) errors: Vec<String>,
}

#[derive(Default)]
pub(crate) struct StandardChangelogExecutionOutcome {
    pub(crate) summary_notes: Vec<String>,
    pub(crate) replay_notices: Vec<String>,
    pub(crate) replay_errors: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StdChangelogExecutionPolicy {
    Auto,
    ForceGenerate,
    ForcePostpone,
}

#[derive(Clone)]
pub(crate) struct PendingTagRequest {
    pub(crate) dialog: TagDialog,
    pub(crate) changelog_enabled: bool,
    pub(crate) std_changelog_policy: StdChangelogExecutionPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StdChangelogDecision {
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
            let integration_mode = dialog.integration_mode;
            let release_notes = release_notes
                .as_deref()
                .ok_or_else(|| anyhow!("release notes should be available for release creation"))?;
            create_forge_release_with_retry_async(
                integration_mode,
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
        let integration_mode = dialog.integration_mode;
        let release_notes = release_notes
            .as_deref()
            .ok_or_else(|| anyhow!("release notes should be available for release creation"))?;
        create_forge_release_with_retry_async(
            integration_mode,
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

pub(crate) fn append_background_tag_summary_notes(summary: String, notes: &[String]) -> String {
    if notes.is_empty() {
        summary
    } else {
        format!("{} {}", summary, notes.join(" "))
    }
}

pub(crate) async fn execute_standard_changelog_for_tag(
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

pub(crate) fn execute_standard_changelog_for_tag_blocking(
    scope: &crate::git::GitScopeContext,
    tag_name: &str,
    branch_name: &str,
    std_changelog_policy: StdChangelogExecutionPolicy,
) -> Result<StandardChangelogExecutionOutcome> {
    let repo_root = &scope.repo_root;
    let top_picks_edits = current_release_top_picks_edits(repo_root);
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
            if top_picks_edits.is_none()
                && find_archived_changelog_markdown(repo_root, tag_name)?.is_some()
            {
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
                    let markdown = std_changelog_gen(
                        tag_name.to_string(),
                        &range.lines,
                        top_picks_edits.as_deref(),
                        scope.mini_commit_hashes,
                    )
                    .markdown;
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

pub(crate) fn ensure_std_changelog_memory_entry(
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

pub(crate) fn ensure_project_repo_gitignore_defaults(
    project: &crate::config::ProjectConfig,
) -> Result<()> {
    use std::collections::HashSet;

    let mut roots = HashSet::new();
    if let Some(repo) = project.repo.as_ref() {
        let root = repo.local_root.trim();
        if !root.is_empty() {
            roots.insert(root.to_string());
        }
    }
    for branch in &project.branches {
        if let Some(repo) = branch.repo.as_ref() {
            let root = repo.local_root.trim();
            if !root.is_empty() {
                roots.insert(root.to_string());
            }
        }
    }

    for root in roots {
        ensure_gitignore_entry(&root, "changelog_temp.md")?;
        ensure_gitignore_entry(&root, ".comfygit/syncmem/stdchlg-local.json")?;
    }

    Ok(())
}

pub(crate) fn ensure_gitignore_entry(repo_root: &str, entry: &str) -> Result<()> {
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

pub(crate) fn replay_postponed_std_changelogs_blocking(
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
        .filter(|entry| entry.generated == crate::changelog::StdChangelogGeneratedState::Postponed)
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

        let markdown = std_changelog_gen(
            entry.tag_from.clone(),
            &range.lines,
            None,
            scope.mini_commit_hashes,
        )
        .markdown;
        archive_changelog_markdown(repo_root, &entry.tag_from, &markdown)?;
        record_std_changelog_generated(repo_root, &entry.tag_from, &entry.tag_origin)?;
        outcome.notices.push(format!(
            "Replayed postponed changelog '{}' after it reached mainline lineage.",
            entry.tag_from
        ));
    }

    Ok(outcome)
}

pub(crate) fn previous_tag_for_replay(sorted_tags: &[String], tag_name: &str) -> Option<String> {
    let index = sorted_tags
        .iter()
        .position(|candidate| candidate.trim() == tag_name.trim())?;
    sorted_tags.get(index + 1).cloned()
}

pub(crate) fn decide_std_changelog_generation(
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

pub(crate) fn normalized_branch_names(branches: &[String]) -> Vec<String> {
    let mut names = branches
        .iter()
        .map(|branch| {
            let trimmed = branch.trim().trim_start_matches('*').trim();
            semver_dev_branch_canonical_label(trimmed)
        })
        .filter(|branch| !branch.is_empty())
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    names
}

pub(crate) fn is_mainline_branch(branch: &str, custom_main_branch: Option<&str>) -> bool {
    is_mainline_branch_name(branch, custom_main_branch)
}

pub(crate) fn current_release_top_picks_edits(repo_root: &str) -> Option<String> {
    let edits = load_top_picks_edits(repo_root);
    (!edits.trim().is_empty()).then_some(edits)
}

pub(crate) fn build_release_notes_markdown(
    tag_name: &str,
    scope: &crate::git::GitScopeContext,
) -> Result<String> {
    // Load project config for variator storage
    let variator_storage = crate::config::ConfigStore::locate()
        .ok()
        .and_then(|s| s.load().ok())
        .and_then(|c| c.projects.into_iter().next())
        .map(|p| p.variator_storage)
        .unwrap_or_default();

    let top_picks_edits = current_release_top_picks_edits(&scope.repo_root);
    let last_public_release = latest_public_release_tag(&scope.repo_root).ok().flatten();
    // Always render from the selected scope's current changelog settings so toggles
    // (like mini hashes) apply immediately across forges, including GitLab.
    let _ = find_archived_changelog_markdown(&scope.repo_root, tag_name)?;

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
            scope.hide_pr_messages,
            scope.hide_bump_messages,
            scope.mini_commit_hashes,
            scope.changelog_wrap_detailed_if_top_picks,
            top_picks_edits.as_deref(),
            variator_storage,
        )
        .markdown);
    }

    let recent_range = load_recent_change_range_with_cancel(scope, None)?;
    Ok(rls_changelog_gen(
        tag_name.to_string(),
        &recent_range.lines,
        None,
        scope.hide_pr_messages,
        scope.hide_bump_messages,
        scope.mini_commit_hashes,
        scope.changelog_wrap_detailed_if_top_picks,
        top_picks_edits.as_deref(),
        variator_storage,
    )
    .markdown)
}

fn build_scope_changelog_section(
    scope: &crate::git::GitScopeContext,
    tag_name: &str,
    cancel: Option<GitCancellation>,
) -> Result<String> {
    let variator_storage = crate::config::ConfigStore::locate()
        .ok()
        .and_then(|s| s.load().ok())
        .and_then(|c| c.projects.into_iter().next())
        .map(|p| p.variator_storage)
        .unwrap_or_default();
    let top_picks_edits = current_release_top_picks_edits(&scope.repo_root);
    let sorted_tags = sorted_local_tags_with_cancel(&scope.repo_root, cancel.clone())?;
    if let Some(previous_tag) = previous_tag_for_replay(&sorted_tags, tag_name) {
        let range = load_change_range_for_tags_with_cancel(scope, &previous_tag, tag_name, cancel)?;
        if range.lines.is_empty() {
            return Ok(String::new());
        }
        return Ok(rls_changelog_gen(
            tag_name.to_string(),
            &range.lines,
            Some(&previous_tag),
            scope.hide_pr_messages,
            scope.hide_bump_messages,
            scope.mini_commit_hashes,
            scope.changelog_wrap_detailed_if_top_picks,
            top_picks_edits.as_deref(),
            variator_storage,
        )
        .markdown);
    }

    let recent_range = load_recent_change_range_with_cancel(scope, cancel)?;
    Ok(rls_changelog_gen(
        tag_name.to_string(),
        &recent_range.lines,
        None,
        scope.hide_pr_messages,
        scope.hide_bump_messages,
        scope.mini_commit_hashes,
        scope.changelog_wrap_detailed_if_top_picks,
        top_picks_edits.as_deref(),
        variator_storage,
    )
    .markdown)
}

pub(crate) fn build_branched_core_release_notes(
    project: &ProjectConfig,
    core_tag: &str,
    core_scope_index: usize,
    cancel: Option<GitCancellation>,
) -> Result<String> {
    let contexts = collect_all_branch_git_scope_contexts(project)?;
    let core_scope = contexts
        .get(core_scope_index)
        .ok_or_else(|| anyhow!("core scope index {core_scope_index} is out of range"))?;
    let mut notes = build_release_notes_markdown(core_tag, core_scope)?;

    for (index, sibling) in contexts.iter().enumerate() {
        if index == core_scope_index {
            continue;
        }
        if !matches!(
            sibling.scope_kind,
            Some(BranchScopeKind::Module) | Some(BranchScopeKind::Service)
        ) {
            continue;
        }
        let composite_tag = format_branched_scope_tag(&sibling.suggested_tag_name, core_tag);
        let section = build_scope_changelog_section(sibling, &composite_tag, cancel.clone())?;
        if section.trim().is_empty() {
            continue;
        }
        notes.push_str("\n\n---\n\n");
        notes.push_str(&format!(
            "## {} — {}\n\n",
            sibling.display_name, composite_tag
        ));
        notes.push_str(section.trim());
    }

    Ok(notes)
}

pub(crate) fn append_branched_sibling_sections_to_notes(
    project: &ProjectConfig,
    core_tag: &str,
    core_scope_index: usize,
    notes: &mut String,
    cancel: Option<GitCancellation>,
) -> Result<()> {
    let contexts = collect_all_branch_git_scope_contexts(project)?;
    for (index, sibling) in contexts.iter().enumerate() {
        if index == core_scope_index {
            continue;
        }
        if !matches!(
            sibling.scope_kind,
            Some(BranchScopeKind::Module) | Some(BranchScopeKind::Service)
        ) {
            continue;
        }
        let composite_tag = format_branched_scope_tag(&sibling.suggested_tag_name, core_tag);
        let section = build_scope_changelog_section(sibling, &composite_tag, cancel.clone())?;
        if section.trim().is_empty() {
            continue;
        }
        notes.push_str("\n\n---\n\n");
        notes.push_str(&format!(
            "## {} — {}\n\n",
            sibling.display_name, composite_tag
        ));
        notes.push_str(section.trim());
    }
    Ok(())
}

pub(crate) fn latest_public_release_tag(repo_root: &str) -> Result<Option<String>> {
    let Some(forge) = crate::forge::detect_forge_for_repo(repo_root) else {
        return Ok(None);
    };
    let integration_mode = match forge {
        crate::forge::ForgeKind::GitHub => crate::config::IntegrationMode::GitHubEnabled,
        crate::forge::ForgeKind::GitLab => crate::config::IntegrationMode::GitLabEnabled,
    };
    crate::git::last_rls_version(repo_root, integration_mode, None)
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

async fn create_forge_release_with_retry_async(
    integration_mode: IntegrationMode,
    repo_root: String,
    tag_name: String,
    release_notes: String,
) -> Result<()> {
    let forge = crate::forge::require_forge_cli(integration_mode)?;
    let cli_name = forge.cli_name();
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
    let action_label = format!("{cli_name} release create");
    let release_result = run_command_with_retry_async(
        repo_root,
        cli_name,
        args,
        GH_RELEASE_TIMEOUT,
        NETWORK_RETRY_ATTEMPTS,
        &action_label,
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

impl App {
    pub(crate) fn register_background_job(&mut self, kind: BackgroundJobKind) -> u64 {
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

    pub(crate) fn clear_registered_background_job(&mut self, kind: BackgroundJobKind, id: u64) {
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

    pub(crate) fn is_background_result_stale(&self, message: &BackgroundJobResultMessage) -> bool {
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

    pub(crate) fn schedule_background_job(
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

    pub(crate) fn schedule_recent_changes_prefetch(&mut self) -> Result<()> {
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

    pub(crate) fn cancel_background_job_kind(&mut self, kind: BackgroundJobKind) {
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

    pub(crate) fn background_job_cancel(
        &self,
        kind: BackgroundJobKind,
        id: u64,
    ) -> GitCancellation {
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

    pub(crate) fn schedule_prefetch_overview_activity_cache(&mut self) -> Result<()> {
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

    pub(crate) fn schedule_refresh_overview_activity_cache(&mut self) -> Result<()> {
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

#[cfg(test)]
mod branched_release_notes_tests {
    use crate::workflow::rls_now::format_branched_scope_tag;

    #[test]
    fn merged_branched_notes_preserve_top_picks_headers_per_scope() {
        let composite_tag = format_branched_scope_tag("v0.5.2", "v0.9.1");
        let mut notes = "# Core Release\n\n### Top Picks\n- core fix".to_string();
        let sibling_body = "### Top Picks\n- module fix";
        notes.push_str("\n\n---\n\n");
        notes.push_str(&format!("## {} — {}\n\n", "API Module", composite_tag));
        notes.push_str(sibling_body);

        assert_eq!(composite_tag, "v0.5.2+core.0.9.1");
        assert!(notes.contains("## API Module — v0.5.2+core.0.9.1"));
        assert_eq!(notes.matches("### Top Picks").count(), 2);
        assert!(notes.contains("- core fix"));
        assert!(notes.contains("- module fix"));
    }
}
