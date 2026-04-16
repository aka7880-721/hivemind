// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyVersionBumper License v1.2
//
fn ensure_not_cancelled(cancel: &GitCancellation) -> Result<()> {
    if cancel.is_cancelled() {
        bail!("ReleaseNOW cancelled by user")
    }
    Ok(())
}
// For details, see the LICENSE file in the repository root.

use super::*;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc::{RecvTimeoutError, Sender as StdSender, channel},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, anyhow, bail};
use tokio::{
    sync::mpsc::{UnboundedSender, unbounded_channel},
    task::spawn_blocking,
};

use crate::{
    config::ReleaseNowSettings,
    git::{
        GitScopeContext, ensure_git_repo_with_cancel, run_git_checked_with_cancel,
        run_git_with_cancel,
    },
};

const RELEASE_NOW_TIMEOUT: Duration = Duration::from_secs(60 * 30);
const RECENT_BUMP_WINDOW_SECS: i64 = 15 * 60;
const DEFAULT_RELEASE_NOTES: &str = "# Release Notes\n\nAdd release highlights here before publishing.";

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ReleaseNowMode {
    BumpWarning,
    Configure,
    Completed,
}

#[derive(Clone)]
pub(super) struct ReleaseNowDialog {
    pub(super) project_name: String,
    pub(super) scope_label: String,
    pub(super) scope: GitScopeContext,
    pub(super) changelog_enabled: bool,
    pub(super) repo_root: String,
    pub(super) tag_name: String,
    pub(super) options: Vec<ReleaseNowRunOption>,
    pub(super) selected_option: usize,
    pub(super) attach_changelog: bool,
    pub(super) release_notes_markdown: String,
    pub(super) release_notes_placeholder: String,
    pub(super) warning_message: Option<String>,
    pub(super) mode: ReleaseNowMode,
    pub(super) running: bool,
    pub(super) auto_follow: bool,
    pub(super) cancel_requested: bool,
    pub(super) warning_confirm_selected: bool,
    pub(super) scroll: u16,
    pub(super) body_viewport_height: u16,
    pub(super) selection_anchor: Option<usize>,
    pub(super) selection_focus: Option<usize>,
    pub(super) summary: Option<String>,
    pub(super) summary_is_warning: bool,
    pub(super) summary_is_error: bool,
    pub(super) artifact_files: Vec<String>,
    pub(super) log_lines: Vec<String>,
}

impl ReleaseNowDialog {
    pub(super) fn from_validation(validation: ReleaseNowValidation) -> Self {
        let mode = if validation.warning_message.is_some() {
            ReleaseNowMode::BumpWarning
        } else {
            ReleaseNowMode::Configure
        };

        Self {
            project_name: validation.project_name,
            scope_label: validation.scope_label,
            scope: validation.scope,
            changelog_enabled: validation.changelog_enabled,
            repo_root: validation.repo_root,
            tag_name: validation.tag_name,
            options: validation.options,
            selected_option: 0,
            attach_changelog: true,
            release_notes_markdown: validation.release_notes_markdown,
            release_notes_placeholder: "Edit release notes in Markdown before publishing.".to_string(),
            warning_message: validation.warning_message,
            mode,
            running: false,
            auto_follow: false,
            cancel_requested: false,
            warning_confirm_selected: false,
            scroll: 0,
            body_viewport_height: 0,
            selection_anchor: None,
            selection_focus: None,
            summary: None,
            summary_is_warning: false,
            summary_is_error: false,
            artifact_files: Vec::new(),
            log_lines: Vec::new(),
        }
    }

    pub(super) fn is_warning_mode(&self) -> bool {
        self.mode == ReleaseNowMode::BumpWarning
    }

    pub(super) fn is_completed(&self) -> bool {
        self.mode == ReleaseNowMode::Completed
    }

    pub(super) fn is_running(&self) -> bool {
        self.running
    }

    pub(super) fn auto_follow(&self) -> bool {
        self.auto_follow
    }

    pub(super) fn cancel_requested(&self) -> bool {
        self.cancel_requested
    }

    pub(super) fn set_body_viewport_height(&mut self, height: u16) {
        self.body_viewport_height = height;
        if self.running && self.auto_follow {
            self.scroll_to_tail();
        }
    }

    pub(super) fn selected_option(&self) -> &ReleaseNowRunOption {
        &self.options[self.selected_option.min(self.options.len().saturating_sub(1))]
    }

    pub(super) fn cycle_option(&mut self, delta: isize) {
        if self.options.is_empty() {
            self.selected_option = 0;
            return;
        }

        let len = self.options.len() as isize;
        self.selected_option = (self.selected_option as isize + delta).rem_euclid(len) as usize;
    }

    pub(super) fn toggle_attach_changelog(&mut self) {
        self.attach_changelog = !self.attach_changelog;
    }

    pub(super) fn toggle_warning_selection(&mut self) {
        self.warning_confirm_selected = !self.warning_confirm_selected;
    }

    pub(super) fn proceed_past_warning(&mut self) {
        self.mode = ReleaseNowMode::Configure;
        self.warning_confirm_selected = false;
        self.scroll = 0;
    }

    pub(super) fn scroll_by(&mut self, delta: i16) {
        if self.running && self.auto_follow && delta != 0 {
            self.scroll_to_tail();
            self.auto_follow = false;
        }
        self.scroll = self.scroll.saturating_add_signed(delta);
    }

    pub(super) fn begin_running(&mut self) {
        self.running = true;
        self.mode = ReleaseNowMode::Configure;
        self.auto_follow = true;
        self.cancel_requested = false;
        self.clear_body_selection();
        self.summary = None;
        self.summary_is_warning = false;
        self.summary_is_error = false;
        self.artifact_files.clear();
        self.log_lines.clear();
        self.scroll = 0;
    }

    pub(super) fn toggle_auto_follow(&mut self) -> bool {
        self.auto_follow = !self.auto_follow;
        if self.auto_follow {
            self.scroll_to_tail();
        }
        self.auto_follow
    }

    pub(super) fn mark_cancel_requested(&mut self) {
        if self.cancel_requested {
            return;
        }

        self.cancel_requested = true;
        self.append_log_lines(vec!["Cancellation requested. Waiting for the running command to stop...".to_string()]);
    }

    pub(super) fn append_log_lines(&mut self, lines: Vec<String>) {
        if lines.is_empty() {
            return;
        }

        self.log_lines.extend(lines);
        if self.running && self.auto_follow {
            self.scroll_to_tail();
        }
    }

    pub(super) fn apply_outcome(&mut self, outcome: ReleaseNowExecutionOutcome) {
        self.running = false;
        self.auto_follow = false;
        self.cancel_requested = false;
        self.clear_body_selection();
        self.mode = ReleaseNowMode::Completed;
        self.summary = Some(outcome.summary);
        self.summary_is_warning = false;
        self.summary_is_error = false;
        self.artifact_files = outcome.artifact_files;
        self.append_log_lines(outcome.log_lines);
        self.scroll = 0;
    }

    pub(super) fn apply_cancelled(&mut self, message: String) {
        self.running = false;
        self.auto_follow = false;
        self.cancel_requested = false;
        self.clear_body_selection();
        self.mode = ReleaseNowMode::Completed;
        self.summary = Some(message);
        self.summary_is_warning = true;
        self.summary_is_error = false;
        self.artifact_files.clear();
        self.scroll = 0;
    }

    pub(super) fn apply_failure(&mut self, error_message: String) {
        self.running = false;
        self.auto_follow = false;
        self.cancel_requested = false;
        self.clear_body_selection();
        self.mode = ReleaseNowMode::Completed;
        self.summary = Some(format!("ReleaseNOW failed: {}", error_message));
        self.summary_is_warning = false;
        self.summary_is_error = true;
        self.artifact_files.clear();
        if self.log_lines.is_empty() {
            self.log_lines
                .push("ReleaseNOW failed before any logs were captured.".to_string());
        }
        self.scroll = 0;
    }

    fn scroll_to_tail(&mut self) {
        self.scroll = self.tail_scroll_offset();
    }

    pub(super) fn scroll_offset(&self) -> u16 {
        self.scroll.min(self.max_scroll_offset())
    }

    fn max_scroll_offset(&self) -> u16 {
        self.body_plain_lines()
            .len()
            .saturating_sub(self.body_viewport_height.max(1) as usize)
            .min(u16::MAX as usize) as u16
    }

    fn tail_scroll_offset(&self) -> u16 {
        self.max_scroll_offset()
    }

    pub(super) fn begin_body_selection(&mut self, row_offset: u16) -> bool {
        let Some(index) = self.body_line_index_for_row(row_offset) else {
            return false;
        };

        if self.running {
            self.auto_follow = false;
        }
        self.selection_anchor = Some(index);
        self.selection_focus = Some(index);
        true
    }

    pub(super) fn update_body_selection(&mut self, row_offset: u16) -> bool {
        let Some(anchor) = self.selection_anchor else {
            return false;
        };
        let Some(index) = self.body_line_index_for_row(row_offset) else {
            return false;
        };

        self.selection_anchor = Some(anchor);
        self.selection_focus = Some(index);
        true
    }

    pub(super) fn has_body_selection(&self) -> bool {
        self.selection_anchor.is_some() && self.selection_focus.is_some()
    }

    pub(super) fn selected_body_text(&self) -> Option<String> {
        let (start, end) = self.selection_range()?;
        let lines = self.body_plain_lines();
        Some(lines[start..=end].join("\n"))
    }

    fn clear_body_selection(&mut self) {
        self.selection_anchor = None;
        self.selection_focus = None;
    }

    fn selection_range(&self) -> Option<(usize, usize)> {
        let start = self.selection_anchor?;
        let end = self.selection_focus?;
        Some((start.min(end), start.max(end)))
    }

    fn body_line_index_for_row(&self, row_offset: u16) -> Option<usize> {
        let count = self.body_plain_lines().len();
        if count == 0 {
            return None;
        }

        let index = self.scroll_offset() as usize + row_offset as usize;
        (index < count).then_some(index)
    }

    pub(super) fn body_title(&self) -> &'static str {
        match self.mode {
            ReleaseNowMode::BumpWarning => " Bump Check ",
            ReleaseNowMode::Configure => {
                if self.running {
                    " Live Log "
                } else if self.attach_changelog {
                    " Release Notes Preview "
                } else {
                    " Release Summary "
                }
            }
            ReleaseNowMode::Completed => " Release Log ",
        }
    }

    pub(super) fn rendered_body_lines(&self) -> Vec<Line<'static>> {
        let lines = match self.mode {
            ReleaseNowMode::BumpWarning => {
                let mut lines = vec![
                    Line::from("Recent bump validation did not find a very recent release tag.")
                        .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                    Line::raw(""),
                ];
                if let Some(message) = &self.warning_message {
                    lines.extend(message.lines().map(|line| Line::from(line.to_string())));
                }
                lines
            }
            ReleaseNowMode::Configure => {
                if self.running {
                    if self.log_lines.is_empty() {
                        vec![Line::from("Waiting for ReleaseNOW output...")]
                    } else {
                        self.log_display_lines()
                    }
                } else if self.attach_changelog {
                    self.release_notes_markdown
                        .lines()
                        .map(|line| Line::from(line.to_string()))
                        .collect()
                } else {
                    vec![
                        Line::from("Changelog attachment is disabled for this release.")
                            .style(Style::default().fg(Color::DarkGray)),
                        Line::raw(""),
                        Line::from(format!(
                            "Run option: {}",
                            self.selected_option().label
                        )),
                        Line::from(format!("Tag: {}", self.tag_name)),
                        Line::from("Enable changelog attachment to preview and edit release notes."),
                    ]
                }
            }
            ReleaseNowMode::Completed => {
                let mut lines = Vec::new();
                if let Some(summary) = &self.summary {
                    lines.push(
                        Line::from(summary.clone())
                            .style(
                                Style::default()
                                    .fg(if self.summary_is_error {
                                        Color::Red
                                    } else if self.summary_is_warning {
                                        Color::Yellow
                                    } else {
                                        Color::Green
                                    })
                                    .add_modifier(Modifier::BOLD),
                            ),
                    );
                    lines.push(Line::raw(""));
                }
                if !self.artifact_files.is_empty() {
                    lines.push(Line::from("Artifacts").style(Style::default().add_modifier(Modifier::BOLD)));
                    lines.extend(
                        self.artifact_files
                            .iter()
                            .map(|file| Line::from(format!("- {}", file))),
                    );
                    lines.push(Line::raw(""));
                }
                lines.push(Line::from("Log").style(Style::default().add_modifier(Modifier::BOLD)));
                if self.log_lines.is_empty() {
                    lines.push(Line::from("No script or release logs were captured."));
                } else {
                    lines.extend(self.log_display_lines());
                }
                lines
            }
        };

        self.highlight_selected_lines(lines)
    }

    fn log_display_lines(&self) -> Vec<Line<'static>> {
        self.log_lines.iter().map(|line| ansi_line_to_ratatui(line)).collect()
    }

    fn body_plain_lines(&self) -> Vec<String> {
        match self.mode {
            ReleaseNowMode::BumpWarning => {
                let mut lines = vec![
                    "Recent bump validation did not find a very recent release tag.".to_string(),
                    String::new(),
                ];
                if let Some(message) = &self.warning_message {
                    lines.extend(message.lines().map(|line| line.to_string()));
                }
                lines
            }
            ReleaseNowMode::Configure => {
                if self.running {
                    if self.log_lines.is_empty() {
                        vec!["Waiting for ReleaseNOW output...".to_string()]
                    } else {
                        self.log_lines
                            .iter()
                            .map(|line| strip_terminal_control_sequences(line))
                            .collect()
                    }
                } else if self.attach_changelog {
                    self.release_notes_markdown.lines().map(|line| line.to_string()).collect()
                } else {
                    vec![
                        "Changelog attachment is disabled for this release.".to_string(),
                        String::new(),
                        format!("Run option: {}", self.selected_option().label),
                        format!("Tag: {}", self.tag_name),
                        "Enable changelog attachment to preview and edit release notes.".to_string(),
                    ]
                }
            }
            ReleaseNowMode::Completed => {
                let mut lines = Vec::new();
                if let Some(summary) = &self.summary {
                    lines.push(summary.clone());
                    lines.push(String::new());
                }
                if !self.artifact_files.is_empty() {
                    lines.push("Artifacts".to_string());
                    lines.extend(self.artifact_files.iter().map(|file| format!("- {}", file)));
                    lines.push(String::new());
                }
                lines.push("Log".to_string());
                if self.log_lines.is_empty() {
                    lines.push("No script or release logs were captured.".to_string());
                } else {
                    lines.extend(
                        self.log_lines
                            .iter()
                            .map(|line| strip_terminal_control_sequences(line)),
                    );
                }
                lines
            }
        }
    }

    fn highlight_selected_lines(&self, lines: Vec<Line<'static>>) -> Vec<Line<'static>> {
        let Some((start, end)) = self.selection_range() else {
            return lines;
        };

        lines
            .into_iter()
            .enumerate()
            .map(|(index, line)| {
                if index >= start && index <= end {
                    highlight_line(line)
                } else {
                    line
                }
            })
            .collect()
    }
}

#[derive(Clone)]
pub(super) struct ReleaseNowValidation {
    pub(super) project_name: String,
    pub(super) scope_label: String,
    pub(super) scope: GitScopeContext,
    pub(super) changelog_enabled: bool,
    pub(super) repo_root: String,
    pub(super) tag_name: String,
    pub(super) options: Vec<ReleaseNowRunOption>,
    pub(super) warning_message: Option<String>,
    pub(super) release_notes_markdown: String,
}

#[derive(Clone)]
pub(super) struct ReleaseNowRunOption {
    pub(super) label: String,
    pub(super) scripts: Vec<ReleaseNowScript>,
    pub(super) artifact_dirs: Vec<String>,
}

#[derive(Clone)]
pub(super) struct ReleaseNowScript {
    pub(super) label: String,
    pub(super) script_path: String,
}

#[derive(Clone)]
pub(super) struct ReleaseNowExecutionRequest {
    pub(super) scope_label: String,
    pub(super) scope: GitScopeContext,
    pub(super) changelog_enabled: bool,
    pub(super) repo_root: String,
    pub(super) tag_name: String,
    pub(super) release_title: String,
    pub(super) selected_option_label: String,
    pub(super) scripts: Vec<ReleaseNowScript>,
    pub(super) artifact_dirs: Vec<String>,
    pub(super) release_notes_markdown: Option<String>,
}

#[derive(Clone)]
pub(super) struct ReleaseNowExecutionOutcome {
    pub(super) summary: String,
    pub(super) artifact_files: Vec<String>,
    pub(super) log_lines: Vec<String>,
}

pub(super) fn validate_release_now(
    project: &ProjectConfig,
    scope_index: usize,
    cancel: Option<GitCancellation>,
) -> Result<ReleaseNowValidation> {
    if project.integration_mode != IntegrationMode::GitHubEnabled {
        bail!("ReleaseNOW requires a GitHub-enabled project with a configured remote")
    }

    ensure_gh_authenticated()?;

    let contexts = collect_all_branch_git_scope_contexts(project)?;
    if contexts.is_empty() {
        bail!("ReleaseNOW requires at least one git-backed scope")
    }

    let scope_index = scope_index.min(contexts.len().saturating_sub(1));
    let scope = contexts[scope_index].clone();
    let options = collect_release_now_options(project.release_now_for_scope(scope_index))?;
    let warning_message = build_recent_bump_warning(project, &contexts, scope_index, cancel.clone())?;
    let release_notes_markdown = build_release_notes_markdown(&scope.suggested_tag_name, &scope)
        .unwrap_or_else(|_| DEFAULT_RELEASE_NOTES.to_string());

    Ok(ReleaseNowValidation {
        project_name: project.name.clone(),
        scope_label: scope
            .scope_kind
            .map(|kind| format!("{} ({})", scope.display_name, kind.display_name()))
            .unwrap_or_else(|| scope.display_name.clone()),
        scope: scope.clone(),
        changelog_enabled: project.changelog_enabled_for_scope(scope_index),
        repo_root: scope.repo_root.clone(),
        tag_name: scope.suggested_tag_name.clone(),
        options,
        warning_message,
        release_notes_markdown,
    })
}

pub(super) fn build_execution_request(dialog: &ReleaseNowDialog) -> ReleaseNowExecutionRequest {
    ReleaseNowExecutionRequest {
        scope_label: dialog.scope_label.clone(),
        scope: dialog.scope.clone(),
        changelog_enabled: dialog.changelog_enabled,
        repo_root: dialog.repo_root.clone(),
        tag_name: dialog.tag_name.clone(),
        release_title: format!("{} {}", dialog.project_name, dialog.tag_name),
        selected_option_label: dialog.selected_option().label.clone(),
        scripts: dialog.selected_option().scripts.clone(),
        artifact_dirs: dialog.selected_option().artifact_dirs.clone(),
        release_notes_markdown: dialog
            .attach_changelog
            .then(|| dialog.release_notes_markdown.trim().to_string())
            .filter(|notes| !notes.is_empty()),
    }
}

pub(super) async fn execute_release_now_async(
    request: ReleaseNowExecutionRequest,
    cancel: GitCancellation,
    mut emit_progress: impl FnMut(Vec<String>) + Send,
) -> Result<ReleaseNowExecutionOutcome> {
    ensure_gh_authenticated()?;
    ensure_not_cancelled(&cancel)?;

    emit_progress(vec![format!(
        "Starting ReleaseNOW for {} using {}.",
        request.scope_label, request.selected_option_label
    )]);

    for script in &request.scripts {
        ensure_not_cancelled(&cancel)?;
        run_script_with_live_logs(&request.repo_root, script, cancel.clone(), &mut emit_progress).await?;
    }

    ensure_not_cancelled(&cancel)?;
    emit_progress(vec!["Scanning dist/latest for release artifacts...".to_string()]);
    let repo_root = request.repo_root.clone();
    let artifact_dirs = request.artifact_dirs.clone();
    let artifact_files = run_blocking_job(move || discover_artifacts(&repo_root, &artifact_dirs)).await?;
    if artifact_files.is_empty() {
        bail!(
            "ReleaseNOW finished running scripts, but no artifacts were found under dist/latest for {}",
            request.selected_option_label
        )
    }
    emit_progress(vec![format!("Discovered {} artifact(s).", artifact_files.len())]);

    ensure_not_cancelled(&cancel)?;
    emit_progress(vec![format!("Ensuring local tag '{}' exists.", request.tag_name)]);
    let repo_root_for_tag = request.repo_root.clone();
    let tag_name_for_tag = request.tag_name.clone();
    let created_local_tag = run_blocking_job(move || ensure_local_tag(&repo_root_for_tag, &tag_name_for_tag, None)).await?;
    emit_progress(vec![if created_local_tag {
        format!("Created local tag '{}'.", request.tag_name)
    } else {
        format!("Local tag '{}' already exists; reconciling changelog state.", request.tag_name)
    }]);

    let mut release_notes = Vec::new();
    if request.changelog_enabled {
        let repo_root_for_branch = request.repo_root.clone();
        let branch_name = run_blocking_job(move || current_branch_with_cancel(&repo_root_for_branch, None)).await?;
        emit_progress(vec!["Syncing standard changelog archive, summary, and memory state.".to_string()]);
        let std_outcome = execute_standard_changelog_for_tag(
            &request.scope,
            &request.tag_name,
            &branch_name,
            StdChangelogExecutionPolicy::Auto,
        ).await?;
        for line in &std_outcome.summary_notes {
            emit_progress(vec![line.clone()]);
        }
        for line in &std_outcome.replay_notices {
            emit_progress(vec![line.clone()]);
        }
        for line in &std_outcome.replay_errors {
            emit_progress(vec![format!("Warning: {}", line)]);
        }
        release_notes.extend(std_outcome.summary_notes);
    }

    create_or_update_github_release(
        &request.repo_root,
        &request.tag_name,
        &request.release_title,
        request.release_notes_markdown.as_deref(),
        &artifact_files,
        cancel,
        &mut emit_progress,
    )
    .await?;

    Ok(ReleaseNowExecutionOutcome {
        summary: append_background_tag_summary_notes(
            format!(
                "ReleaseNOW published '{}' with {} artifact(s) using {}.",
                request.tag_name,
                artifact_files.len(),
                request.selected_option_label
            ),
            &release_notes,
        ),
        artifact_files,
        log_lines: Vec::new(),
    })
}

pub(super) fn is_cancelled_error(message: &str) -> bool {
    message.contains("cancelled by user")
}

fn collect_release_now_options(settings: &ReleaseNowSettings) -> Result<Vec<ReleaseNowRunOption>> {
    if !settings.enabled {
        bail!("ReleaseNOW is disabled for this scope in Project Settings -> Distro")
    }

    let mut individual = Vec::new();
    push_release_option(
        &mut individual,
        "Windows",
        settings.windows_script.as_str(),
        &["windows-x64"],
    );
    push_release_option(
        &mut individual,
        "Linux ARM",
        settings.linux_arm_script.as_str(),
        &["linux-arm64"],
    );
    push_release_option(
        &mut individual,
        "Linux AMD",
        settings.linux_amd_script.as_str(),
        &["linux-amd64"],
    );
    push_release_option(
        &mut individual,
        "MacOS",
        settings.macos_script.as_str(),
        &["macos-x86_64", "macos-aarch64"],
    );

    if individual.is_empty() {
        bail!("No ReleaseNOW scripts are configured for this scope")
    }

    let mut combined_scripts = Vec::new();
    let mut combined_artifact_dirs = Vec::new();
    for option in &individual {
        combined_scripts.extend(option.scripts.clone());
        for dir in &option.artifact_dirs {
            if !combined_artifact_dirs.iter().any(|existing| existing == dir) {
                combined_artifact_dirs.push(dir.clone());
            }
        }
    }

    let mut options = vec![ReleaseNowRunOption {
        label: "All configured".to_string(),
        scripts: combined_scripts,
        artifact_dirs: combined_artifact_dirs,
    }];
    options.extend(individual);
    Ok(options)
}

fn push_release_option(
    options: &mut Vec<ReleaseNowRunOption>,
    label: &str,
    script_path: &str,
    artifact_dirs: &[&str],
) {
    let trimmed = script_path.trim();
    if trimmed.is_empty() {
        return;
    }

    options.push(ReleaseNowRunOption {
        label: label.to_string(),
        scripts: vec![ReleaseNowScript {
            label: label.to_string(),
            script_path: trimmed.to_string(),
        }],
        artifact_dirs: artifact_dirs.iter().map(|dir| (*dir).to_string()).collect(),
    });
}

fn build_recent_bump_warning(
    project: &ProjectConfig,
    contexts: &[GitScopeContext],
    scope_index: usize,
    cancel: Option<GitCancellation>,
) -> Result<Option<String>> {
    let affected_scope_indexes = if project.unified_versioning {
        (0..contexts.len()).collect::<Vec<_>>()
    } else {
        vec![scope_index.min(contexts.len().saturating_sub(1))]
    };

    let now_seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before UNIX epoch")?
        .as_secs() as i64;
    let mut warnings = Vec::new();

    for index in affected_scope_indexes {
        let scope = contexts
            .get(index)
            .ok_or_else(|| anyhow!("selected ReleaseNOW scope no longer exists"))?;
        match latest_bump_timestamp(scope, cancel.clone())? {
            Some(timestamp) => {
                let age = now_seconds.saturating_sub(timestamp);
                if age > RECENT_BUMP_WINDOW_SECS {
                    warnings.push(format!(
                        "- {}: latest tag was {} ago",
                        scope.display_name,
                        format_age(age)
                    ));
                }
            }
            None => warnings.push(format!("- {}: no release tag was found", scope.display_name)),
        }
    }

    if warnings.is_empty() {
        Ok(None)
    } else {
        Ok(Some(format!(
            "ReleaseNOW expected a bump within the last 15 minutes.\n{}",
            warnings.join("\n")
        )))
    }
}

fn latest_bump_timestamp(scope: &GitScopeContext, cancel: Option<GitCancellation>) -> Result<Option<i64>> {
    ensure_git_repo_with_cancel(&scope.repo_root, cancel.clone())?;
    let describe = run_git_with_cancel(&scope.repo_root, &["describe", "--tags", "--abbrev=0"], cancel.clone())?;
    if !describe.success {
        return Ok(None);
    }

    let tag = describe.stdout.trim().to_string();
    if tag.is_empty() {
        return Ok(None);
    }

    let timestamp = run_git_checked_with_cancel(
        &scope.repo_root,
        &["log", "-1", "--format=%ct", &tag],
        cancel,
    )?;
    Ok(timestamp.trim().parse::<i64>().ok())
}

fn format_age(age_seconds: i64) -> String {
    let minutes = (age_seconds / 60).max(0);
    if minutes < 60 {
        format!("{} minute(s)", minutes.max(1))
    } else if minutes < 60 * 24 {
        format!("{} hour(s)", (minutes / 60).max(1))
    } else {
        format!("{} day(s)", (minutes / (60 * 24)).max(1))
    }
}

fn ensure_gh_authenticated() -> Result<()> {
    ensure_gh_available()?;
    let output = Command::new("gh")
        .args(["auth", "status"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context("failed to invoke gh auth status")?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let detail = if stderr.is_empty() { stdout } else { stderr };
        bail!("GitHub CLI is not authenticated: {}", detail)
    }
}

async fn run_script_with_live_logs(
    repo_root: &str,
    script: &ReleaseNowScript,
    cancel: GitCancellation,
    emit_progress: &mut impl FnMut(Vec<String>),
) -> Result<()> {
    let script_path = resolve_script_path(repo_root, &script.script_path)?;
    let display_path = script_path.display().to_string();
    let (program, args) = script_command(&script_path)?;
    emit_progress(vec![format!("[{}] Running {}", script.label, display_path)]);
    let repo_root = repo_root.to_string();
    let action = format!("run {} script", script.label);
    let log_label = script.label.clone();
    run_blocking_streaming_operation(
        move |progress_tx| {
            run_command_with_streaming(
                &repo_root,
                &program,
                &args,
                RELEASE_NOW_TIMEOUT,
                &action,
                &log_label,
                &cancel,
                &progress_tx,
            )
        },
        emit_progress,
    )
    .await?;
    emit_progress(vec![format!("[{}] Completed successfully.", script.label)]);
    Ok(())
}

fn resolve_script_path(repo_root: &str, script_path: &str) -> Result<PathBuf> {
    let trimmed = script_path.trim();
    if trimmed.is_empty() {
        bail!("ReleaseNOW script path is empty")
    }

    let path = PathBuf::from(trimmed);
    let resolved = if path.is_absolute() {
        path
    } else {
        Path::new(repo_root).join(path)
    };
    if resolved.exists() {
        Ok(resolved)
    } else {
        bail!("configured ReleaseNOW script '{}' was not found", resolved.display())
    }
}

fn script_command(script_path: &Path) -> Result<(String, Vec<String>)> {
    let extension = script_path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default();

    if extension == "ps1" {
        let escaped_path = script_path.display().to_string().replace('\'', "''");
        return Ok((
            "pwsh".to_string(),
            vec![
                "-NoProfile".to_string(),
                "-ExecutionPolicy".to_string(),
                "Bypass".to_string(),
                "-Command".to_string(),
                format!(
                    r"& {{ $PSStyle.OutputRendering = 'Ansi'; $InformationPreference = 'Continue'; & '{}' 6>&1 }}",
                    escaped_path
                ),
            ],
        ));
    }

    Ok((script_path.display().to_string(), Vec::new()))
}

async fn run_blocking_streaming_operation<T>(
    operation: impl FnOnce(UnboundedSender<Vec<String>>) -> Result<T> + Send + 'static,
    emit_progress: &mut impl FnMut(Vec<String>),
) -> Result<T>
where
    T: Send + 'static,
{
    let (progress_tx, mut progress_rx) = unbounded_channel::<Vec<String>>();
    let handle = spawn_blocking(move || operation(progress_tx));
    tokio::pin!(handle);

    loop {
        tokio::select! {
            maybe_lines = progress_rx.recv() => {
                if let Some(lines) = maybe_lines {
                    emit_progress(lines);
                }
            }
            result = &mut handle => {
                let value = result
                    .map_err(|error| anyhow!("background task failed: {error}"))??;
                while let Ok(lines) = progress_rx.try_recv() {
                    emit_progress(lines);
                }
                return Ok(value);
            }
        }
    }
}

fn run_command_with_streaming(
    repo_root: &str,
    program: &str,
    args: &[String],
    timeout_window: Duration,
    action: &str,
    log_label: &str,
    cancel: &GitCancellation,
    progress_tx: &UnboundedSender<Vec<String>>,
) -> Result<()> {
    let mut command = Command::new(program);
    command
        .current_dir(repo_root)
        .args(args)
        .env("CARGO_TERM_COLOR", "always")
        .env("CARGO_TERM_PROGRESS_WHEN", "always")
        .env("CARGO_TERM_PROGRESS_WIDTH", "120")
        .env("CLICOLOR_FORCE", "1")
        .env("TERM", "xterm-256color")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .with_context(|| format!("failed to start {} in '{}'", action, repo_root))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("failed to capture stdout for {}", action))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow!("failed to capture stderr for {}", action))?;

    let (line_tx, line_rx) = channel::<(String, String)>();
    let stdout_thread = spawn_stream_reader(stdout, "stdout", line_tx.clone());
    let stderr_thread = spawn_stream_reader(stderr, "stderr", line_tx);
    let started_at = Instant::now();

    loop {
        match line_rx.recv_timeout(Duration::from_millis(100)) {
            Ok((stream, line)) => {
                let _ = progress_tx.send(vec![format!("[{}][{}] {}", log_label, stream, line)]);
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {}
        }

        if cancel.is_cancelled() {
            let _ = terminate_process_tree(&mut child);
            join_stream_reader(stdout_thread, action)?;
            join_stream_reader(stderr_thread, action)?;
            drain_stream_lines(&line_rx, log_label, progress_tx);
            bail!("ReleaseNOW cancelled by user")
        }

        if let Some(status) = child.try_wait().with_context(|| format!("failed to poll {}", action))? {
            join_stream_reader(stdout_thread, action)?;
            join_stream_reader(stderr_thread, action)?;
            drain_stream_lines(&line_rx, log_label, progress_tx);

            if status.success() {
                return Ok(());
            }
            bail!("{} failed with exit code {:?}", action, status.code())
        }

        if started_at.elapsed() >= timeout_window {
            let _ = terminate_process_tree(&mut child);
            join_stream_reader(stdout_thread, action)?;
            join_stream_reader(stderr_thread, action)?;
            drain_stream_lines(&line_rx, log_label, progress_tx);
            bail!("{} timed out after {}s", action, timeout_window.as_secs())
        }
    }
}

fn discover_artifacts(repo_root: &str, artifact_dirs: &[String]) -> Result<Vec<String>> {
    let mut files = Vec::new();
    for dir in artifact_dirs {
        let root = Path::new(repo_root).join("dist").join("latest").join(dir);
        if !root.exists() {
            continue;
        }
        collect_files_recursive(&root, &mut files)?;
    }
    files.sort();
    files.dedup();
    Ok(files)
}

fn collect_files_recursive(root: &Path, files: &mut Vec<String>) -> Result<()> {
    for entry in fs::read_dir(root).with_context(|| format!("failed to read '{}'", root.display()))? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_files_recursive(&path, files)?;
        } else if path.is_file() {
            files.push(path.display().to_string());
        }
    }
    Ok(())
}

async fn create_or_update_github_release(
    repo_root: &str,
    tag_name: &str,
    release_title: &str,
    release_notes_markdown: Option<&str>,
    artifact_files: &[String],
    cancel: GitCancellation,
    emit_progress: &mut impl FnMut(Vec<String>),
) -> Result<()> {
    ensure_not_cancelled(&cancel)?;
    let notes_file = release_notes_markdown
        .filter(|notes| !notes.trim().is_empty())
        .map(write_release_notes_file)
        .transpose()?;

    let release_exists = Command::new("gh")
        .current_dir(repo_root)
        .args(["release", "view", tag_name])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("failed to check for an existing GitHub release")?
        .success();

    let result = async {
        if release_exists {
            emit_progress(vec![format!("Updating existing GitHub release '{}'.", tag_name)]);
            let repo_root_owned = repo_root.to_string();
            let upload_cancel = cancel.clone();

            let mut upload_args = vec!["release".to_string(), "upload".to_string(), tag_name.to_string()];
            upload_args.extend(artifact_files.iter().cloned());
            upload_args.push("--clobber".to_string());
            run_blocking_streaming_operation(
                move |progress_tx| {
                    run_command_with_streaming(
                        &repo_root_owned,
                        "gh",
                        &upload_args,
                        RELEASE_NOW_TIMEOUT,
                        "gh release upload",
                        "gh",
                        &upload_cancel,
                        &progress_tx,
                    )
                },
                emit_progress,
            )
            .await?;

            if let Some(notes_file) = &notes_file {
                let edit_args = vec![
                    "release".to_string(),
                    "edit".to_string(),
                    tag_name.to_string(),
                    "--title".to_string(),
                    release_title.to_string(),
                    "--notes-file".to_string(),
                    notes_file.display().to_string(),
                ];
                let repo_root_owned = repo_root.to_string();
                let edit_cancel = cancel.clone();
                run_blocking_streaming_operation(
                    move |progress_tx| {
                        run_command_with_streaming(
                            &repo_root_owned,
                            "gh",
                            &edit_args,
                            RELEASE_NOW_TIMEOUT,
                            "gh release edit",
                            "gh",
                            &edit_cancel,
                            &progress_tx,
                        )
                    },
                    emit_progress,
                )
                .await?;
            }
        } else {
            emit_progress(vec![format!("Creating GitHub release '{}'.", tag_name)]);

            let mut create_args = vec![
                "release".to_string(),
                "create".to_string(),
                tag_name.to_string(),
            ];
            create_args.extend(artifact_files.iter().cloned());
            create_args.push("--title".to_string());
            create_args.push(release_title.to_string());
            if let Some(notes_file) = &notes_file {
                create_args.push("--notes-file".to_string());
                create_args.push(notes_file.display().to_string());
            }
            let repo_root = repo_root.to_string();
            let create_cancel = cancel.clone();
            run_blocking_streaming_operation(
                move |progress_tx| {
                    run_command_with_streaming(
                        &repo_root,
                        "gh",
                        &create_args,
                        RELEASE_NOW_TIMEOUT,
                        "gh release create",
                        "gh",
                        &create_cancel,
                        &progress_tx,
                    )
                },
                emit_progress,
            )
            .await?;
        }

        Ok(())
    }
    .await;

    if let Some(notes_file) = notes_file {
        let _ = fs::remove_file(notes_file);
    }

    result
}

fn spawn_stream_reader<R>(
    stream: R,
    stream_name: &'static str,
    line_tx: StdSender<(String, String)>,
) -> thread::JoinHandle<Result<()>>
where
    R: std::io::Read + Send + 'static,
{
    thread::spawn(move || read_command_stream(stream, stream_name, line_tx))
}

fn read_command_stream<R>(stream: R, stream_name: &'static str, line_tx: StdSender<(String, String)>) -> Result<()>
where
    R: std::io::Read,
{
    let mut stream = stream;
    let mut buffer = [0_u8; 1024];
    let mut pending = Vec::new();
    let mut last_was_cr = false;

    loop {
        let read = stream.read(&mut buffer)?;
        if read == 0 {
            break;
        }

        for byte in &buffer[..read] {
            match *byte {
                b'\r' => {
                    flush_stream_fragment(&mut pending, stream_name, &line_tx);
                    last_was_cr = true;
                }
                b'\n' => {
                    if !last_was_cr {
                        flush_stream_fragment(&mut pending, stream_name, &line_tx);
                    }
                    last_was_cr = false;
                }
                byte => {
                    pending.push(byte);
                    last_was_cr = false;
                }
            }
        }
    }

    flush_stream_fragment(&mut pending, stream_name, &line_tx);
    Ok(())
}

fn join_stream_reader(handle: thread::JoinHandle<Result<()>>, action: &str) -> Result<()> {
    handle
        .join()
        .map_err(|_| anyhow!("failed to join output reader thread for {}", action))??;
    Ok(())
}

fn drain_stream_lines(
    line_rx: &std::sync::mpsc::Receiver<(String, String)>,
    log_label: &str,
    progress_tx: &UnboundedSender<Vec<String>>,
) {
    while let Ok((stream, line)) = line_rx.try_recv() {
        let _ = progress_tx.send(vec![format!("[{}][{}] {}", log_label, stream, line)]);
    }
}

fn terminate_process_tree(child: &mut std::process::Child) -> Result<()> {
    if cfg!(windows) {
        let status = Command::new("taskkill")
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();

        if status.as_ref().map(|value| !value.success()).unwrap_or(true) {
            let _ = child.kill();
        }
    } else {
        let _ = child.kill();
    }

    let _ = child.wait();
    Ok(())
}

fn flush_stream_fragment(
    pending: &mut Vec<u8>,
    stream_name: &'static str,
    line_tx: &StdSender<(String, String)>,
) {
    if pending.is_empty() {
        return;
    }

    let fragment = String::from_utf8_lossy(pending).to_string();
    pending.clear();
    for chunk in split_output_lines(&fragment) {
        let _ = line_tx.send((stream_name.to_string(), chunk));
    }
}

fn strip_terminal_control_sequences(line: &str) -> String {
    let mut result = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' {
            match chars.peek().copied() {
                Some('[') => {
                    let _ = chars.next();
                    for next in chars.by_ref() {
                        if ('@'..='~').contains(&next) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    let _ = chars.next();
                    let mut previous = None;
                    for next in chars.by_ref() {
                        if next == '\u{7}' || (previous == Some('\u{1b}') && next == '\\') {
                            break;
                        }
                        previous = Some(next);
                    }
                }
                _ => {}
            }
            continue;
        }

        if ch == '\u{8}' {
            let _ = result.pop();
            continue;
        }

        if ch.is_control() && ch != '\n' && ch != '\t' {
            continue;
        }

        result.push(ch);
    }

    result
}

fn ansi_line_to_ratatui(line: &str) -> Line<'static> {
    let mut spans = Vec::new();
    let mut style = Style::default();
    let mut text = String::new();
    let mut chars = line.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' && chars.peek() == Some(&'[') {
            let _ = chars.next();
            let mut sequence = String::new();
            while let Some(next) = chars.next() {
                if next == 'm' {
                    break;
                }
                sequence.push(next);
            }

            if !text.is_empty() {
                spans.push(Span::styled(std::mem::take(&mut text), style));
            }
            style = apply_ansi_sgr(style, &sequence);
            continue;
        }

        text.push(ch);
    }

    if !text.is_empty() || spans.is_empty() {
        spans.push(Span::styled(text, style));
    }

    Line::from(spans)
}

fn apply_ansi_sgr(mut style: Style, sequence: &str) -> Style {
    let codes = if sequence.is_empty() {
        vec![0]
    } else {
        sequence
            .split(';')
            .filter_map(|part| part.parse::<u16>().ok())
            .collect::<Vec<_>>()
    };

    for code in codes {
        style = match code {
            0 => Style::default(),
            1 => style.add_modifier(Modifier::BOLD),
            22 => style.remove_modifier(Modifier::BOLD),
            30 => style.fg(Color::Black),
            31 => style.fg(Color::Red),
            32 => style.fg(Color::Green),
            33 => style.fg(Color::Yellow),
            34 => style.fg(Color::Blue),
            35 => style.fg(Color::Magenta),
            36 => style.fg(Color::Cyan),
            37 => style.fg(Color::Gray),
            39 => style.fg(Color::Reset),
            40 => style.bg(Color::Black),
            41 => style.bg(Color::Red),
            42 => style.bg(Color::Green),
            43 => style.bg(Color::Yellow),
            44 => style.bg(Color::Blue),
            45 => style.bg(Color::Magenta),
            46 => style.bg(Color::Cyan),
            47 => style.bg(Color::Gray),
            49 => style.bg(Color::Reset),
            90 => style.fg(Color::DarkGray),
            91 => style.fg(Color::LightRed),
            92 => style.fg(Color::LightGreen),
            93 => style.fg(Color::LightYellow),
            94 => style.fg(Color::LightBlue),
            95 => style.fg(Color::LightMagenta),
            96 => style.fg(Color::LightCyan),
            97 => style.fg(Color::White),
            100 => style.bg(Color::DarkGray),
            101 => style.bg(Color::LightRed),
            102 => style.bg(Color::LightGreen),
            103 => style.bg(Color::LightYellow),
            104 => style.bg(Color::LightBlue),
            105 => style.bg(Color::LightMagenta),
            106 => style.bg(Color::LightCyan),
            107 => style.bg(Color::White),
            _ => style,
        };
    }

    style
}

fn highlight_line(line: Line<'static>) -> Line<'static> {
    let highlight = Style::default().bg(Color::Rgb(55, 80, 140));
    Line::from(
        line.spans
            .into_iter()
            .map(|span| Span::styled(span.content, span.style.patch(highlight)))
            .collect::<Vec<_>>(),
    )
}

fn write_release_notes_file(notes: &str) -> Result<PathBuf> {
    let file_path = std::env::temp_dir().join(format!(
        "cvb-release-now-notes-{}-{}.md",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::write(&file_path, notes)
        .with_context(|| format!("failed to write release notes to '{}'", file_path.display()))?;
    Ok(file_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ReleaseNowSettings;

    #[test]
    fn release_now_options_keep_all_configured_first() {
        let settings = ReleaseNowSettings {
            enabled: true,
            windows_script: "scripts/releaseNOW.ps1".to_string(),
            linux_arm_script: String::new(),
            linux_amd_script: "scripts/releaseNOW-linux_amd64.ps1".to_string(),
            macos_script: String::new(),
        };

        let options = collect_release_now_options(&settings).expect("options should build");
        assert_eq!(options[0].label, "All configured");
        assert_eq!(options.len(), 3);
        assert_eq!(options[1].label, "Windows");
        assert_eq!(options[2].label, "Linux AMD");
    }

    #[test]
    fn strip_terminal_control_sequences_removes_ansi_sequences() {
        let raw = "[Windows][stderr] \u{1b}[1m\u{1b}[91merror\u{1b}[0m: build failed";
        assert_eq!(
            strip_terminal_control_sequences(raw),
            "[Windows][stderr] error: build failed"
        );
    }

    #[test]
    fn strip_terminal_control_sequences_applies_backspaces() {
        assert_eq!(strip_terminal_control_sequences("abc\u{8}d"), "abd");
    }
}
