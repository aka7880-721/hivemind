// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit SA-PS License
//
fn ensure_not_cancelled(cancel: &GitCancellation) -> Result<()> {
    if cancel.is_cancelled() {
        bail!("ReleaseNOW cancelled by user")
    }
    Ok(())
}

fn stage_release_now_generated_files(repo_root: &str) -> Result<bool> {
    let mut paths = Vec::new();
    if Path::new(repo_root).join(".changelogs").is_dir() {
        paths.push(".changelogs".to_string());
    }
    if Path::new(repo_root)
        .join(".comfygit")
        .join("syncmem")
        .join("stdchlg.json")
        .is_file()
    {
        paths.push(".comfygit/syncmem/stdchlg.json".to_string());
    }

    if paths.is_empty() {
        return Ok(false);
    }

    let mut args = vec!["add".to_string(), "--".to_string()];
    args.extend(paths);
    let arg_refs = args.iter().map(|s| s.as_str()).collect::<Vec<_>>();
    run_git_checked(repo_root, &arg_refs)?;
    Ok(true)
}

fn has_staged_changes(repo_root: &str) -> Result<bool> {
    Ok(!run_git(repo_root, &["diff", "--cached", "--quiet", "--exit-code"])?.success)
}

fn commit_release_now_generated_files(repo_root: &str, tag_name: &str) -> Result<bool> {
    if !has_staged_changes(repo_root)? {
        return Ok(false);
    }

    let commit_message = format!(
        "~: ReleaseNOW! → {} has just been released via ComfyGit!",
        tag_name
    );
    run_git_checked(repo_root, &["commit", "-m", &commit_message])?;
    Ok(true)
}

struct ReleaseNowGeneratedFilesCommit {
    previous_head: String,
}

fn current_head_commit(repo_root: &str) -> Result<String> {
    run_git_checked(repo_root, &["rev-parse", "HEAD"]).map(|head| head.trim().to_string())
}

fn create_release_now_generated_files_commit(
    repo_root: &str,
    tag_name: &str,
) -> Result<Option<ReleaseNowGeneratedFilesCommit>> {
    if !stage_release_now_generated_files(repo_root)? || !has_staged_changes(repo_root)? {
        return Ok(None);
    }

    let previous_head = current_head_commit(repo_root)?;
    if commit_release_now_generated_files(repo_root, tag_name)? {
        Ok(Some(ReleaseNowGeneratedFilesCommit { previous_head }))
    } else {
        Ok(None)
    }
}

fn rollback_release_now_generated_files_commit(
    repo_root: &str,
    generated_commit: &ReleaseNowGeneratedFilesCommit,
) -> Result<()> {
    run_git_checked(
        repo_root,
        &["reset", "--soft", &generated_commit.previous_head],
    )?;
    Ok(())
}

// For details, see the LICENSE file in the repository root.

use super::*;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc::{Receiver, RecvTimeoutError, Sender as StdSender, channel},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, anyhow, bail};
use tokio::{
    sync::mpsc::{UnboundedSender, unbounded_channel},
    task::spawn_blocking,
};

use crate::{
    config::{ReleaseNowQuickDownloadsSettings, ReleaseNowSettings},
    git::GitScopeContext,
    git_stt::recent_merge_check,
};

#[path = "rls-now-qd.rs"]
mod rls_now_qd;

const RELEASE_NOW_TIMEOUT: Duration = Duration::from_secs(60 * 60);
const DEFAULT_RELEASE_NOTES: &str =
    "# Release Notes\n\nAdd release highlights here before publishing.";

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
    pub(super) quick_downloads: ReleaseNowQuickDownloadsSettings,
    pub(super) readme_injection_enabled: bool,
    pub(super) readme_inject_at_row: u16,
    pub(super) release_title_template: String,
    pub(super) started_at: Option<Instant>,
    /// Elapsed time frozen when the run stops (success, failure, or cancel).
    pub(super) frozen_elapsed: Option<Duration>,
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
            release_notes_placeholder: "Edit release notes in Markdown before publishing."
                .to_string(),
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
            quick_downloads: validation.quick_downloads,
            readme_injection_enabled: validation.readme_injection_enabled,
            readme_inject_at_row: validation.readme_inject_at_row,
            release_title_template: validation.release_title_template,
            started_at: None,
            frozen_elapsed: None,
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
        &self.options[self
            .selected_option
            .min(self.options.len().saturating_sub(1))]
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
        self.scroll = self
            .scroll
            .saturating_add_signed(delta)
            .min(self.max_scroll_offset());
    }

    pub(super) fn begin_running(&mut self) {
        self.running = true;
        self.started_at = Some(Instant::now());
        self.frozen_elapsed = None;
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
        self.append_log_lines(vec![
            "Cancellation requested. Waiting for the running command to stop...".to_string(),
        ]);
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
        self.frozen_elapsed = self.started_at.map(|started| started.elapsed());
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
        self.frozen_elapsed = self.started_at.map(|started| started.elapsed());
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
        let formatted_error = format_user_facing_error(&error_message);
        self.frozen_elapsed = self.started_at.map(|started| started.elapsed());
        self.running = false;
        self.auto_follow = false;
        self.cancel_requested = false;
        self.clear_body_selection();
        self.mode = ReleaseNowMode::Completed;
        self.summary = Some(formatted_error.clone());
        self.summary_is_warning = false;
        self.summary_is_error = true;
        self.artifact_files.clear();
        if self.log_lines.is_empty() {
            self.log_lines
                .push("ReleaseNOW failed before any logs were captured.".to_string());
        }
        self.log_lines
            .push(format!("[ReleaseNOW][summary] {}", formatted_error));
        self.scroll = 0;
    }

    pub(super) fn elapsed_label(&self) -> String {
        let elapsed = if let Some(frozen) = self.frozen_elapsed {
            frozen
        } else if self.running {
            self.started_at
                .map(|started| started.elapsed())
                .unwrap_or_default()
        } else {
            Duration::ZERO
        };
        let hours = elapsed.as_secs() / 3600;
        let minutes = (elapsed.as_secs() % 3600) / 60;
        let seconds = elapsed.as_secs() % 60;
        if hours > 0 {
            format!("{hours:02}:{minutes:02}:{seconds:02}")
        } else {
            format!("{minutes:02}:{seconds:02}")
        }
    }

    pub(super) fn scroll_to_tail(&mut self) {
        self.scroll = self.tail_scroll_offset();
    }

    pub(super) fn scroll_to_start(&mut self) {
        self.scroll = 0;
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
            ReleaseNowMode::BumpWarning => " Merge Check ",
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
                    Line::from(
                        "Recent merge validation did not find a very recent pull request merge.",
                    )
                    .style(
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
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
                        Line::from(format!("Run option: {}", self.selected_option().label)),
                        Line::from(format!("Tag: {}", self.tag_name)),
                        Line::from(
                            "Enable changelog attachment to preview and edit release notes.",
                        ),
                    ]
                }
            }
            ReleaseNowMode::Completed => {
                let mut lines = Vec::new();
                if let Some(summary) = &self.summary {
                    lines.push(
                        Line::from(summary.clone()).style(
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
                    lines.push(
                        Line::from("Artifacts")
                            .style(Style::default().add_modifier(Modifier::BOLD)),
                    );
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
        self.log_lines
            .iter()
            .map(|line| ansi_line_to_ratatui(line))
            .collect()
    }

    fn body_plain_lines(&self) -> Vec<String> {
        match self.mode {
            ReleaseNowMode::BumpWarning => {
                let mut lines = vec![
                    "Recent merge validation did not find a very recent pull request merge."
                        .to_string(),
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
                    self.release_notes_markdown
                        .lines()
                        .map(|line| line.to_string())
                        .collect()
                } else {
                    vec![
                        "Changelog attachment is disabled for this release.".to_string(),
                        String::new(),
                        format!("Run option: {}", self.selected_option().label),
                        format!("Tag: {}", self.tag_name),
                        "Enable changelog attachment to preview and edit release notes."
                            .to_string(),
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
    pub(super) quick_downloads: ReleaseNowQuickDownloadsSettings,
    pub(super) readme_injection_enabled: bool,
    pub(super) readme_inject_at_row: u16,
    pub(super) release_title_template: String,
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
    pub(super) quick_downloads: ReleaseNowQuickDownloadsSettings,
    pub(super) readme_injection_enabled: bool,
    pub(super) readme_inject_at_row: u16,
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
    let warning_message =
        build_recent_merge_warning(project, &contexts, scope_index, cancel.clone())?;
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
        quick_downloads: project
            .release_now_for_scope(scope_index)
            .quick_downloads
            .clone(),
        readme_injection_enabled: project
            .release_now_for_scope(scope_index)
            .readme_injection_enabled,
        readme_inject_at_row: project
            .release_now_for_scope(scope_index)
            .readme_inject_at_row,
        release_title_template: project
            .release_now_for_scope(scope_index)
            .release_title_template
            .clone(),
    })
}

pub(super) fn build_execution_request(dialog: &ReleaseNowDialog) -> ReleaseNowExecutionRequest {
    ReleaseNowExecutionRequest {
        scope_label: dialog.scope_label.clone(),
        scope: dialog.scope.clone(),
        changelog_enabled: dialog.changelog_enabled,
        repo_root: dialog.repo_root.clone(),
        tag_name: dialog.tag_name.clone(),
        release_title: {
            let tmpl = dialog.release_title_template.trim();
            if tmpl.is_empty() {
                format!("{} {}", dialog.project_name, dialog.tag_name)
            } else {
                tmpl.replace("{version}", &dialog.tag_name)
            }
        },
        selected_option_label: dialog.selected_option().label.clone(),
        scripts: dialog.selected_option().scripts.clone(),
        artifact_dirs: dialog.selected_option().artifact_dirs.clone(),
        release_notes_markdown: dialog
            .attach_changelog
            .then(|| dialog.release_notes_markdown.trim().to_string())
            .filter(|notes| !notes.is_empty()),
        quick_downloads: dialog.quick_downloads.clone(),
        readme_injection_enabled: dialog.readme_injection_enabled,
        readme_inject_at_row: dialog.readme_inject_at_row,
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
        run_script_with_live_logs(
            &request.repo_root,
            script,
            cancel.clone(),
            &mut emit_progress,
        )
        .await?;
    }

    ensure_not_cancelled(&cancel)?;
    let artifact_files = if request.artifact_dirs.is_empty() {
        emit_progress(vec![
            "No artifact directories configured; skipping artifact scan (source-only release).".to_string(),
        ]);
        Vec::new()
    } else {
        emit_progress(vec![
            "Scanning dist/latest for release artifacts...".to_string(),
        ]);
        let repo_root = request.repo_root.clone();
        let artifact_dirs = request.artifact_dirs.clone();
        let files =
            run_blocking_job(move || discover_artifacts(&repo_root, &artifact_dirs)).await?;
        if files.is_empty() {
            bail!(
                "ReleaseNOW finished running scripts, but no artifacts were found under dist/latest for {}",
                request.selected_option_label
            )
        }
        emit_progress(vec![format!("Discovered {} artifact(s).", files.len())]);
        files
    };

    ensure_not_cancelled(&cancel)?;
    emit_progress(vec![format!(
        "Ensuring local tag '{}' exists.",
        request.tag_name
    )]);
    let repo_root_for_tag = request.repo_root.clone();
    let tag_name_for_tag = request.tag_name.clone();
    let created_local_tag =
        run_blocking_job(move || ensure_local_tag(&repo_root_for_tag, &tag_name_for_tag, None))
            .await?;
    emit_progress(vec![if created_local_tag {
        format!("Created local tag '{}'.", request.tag_name)
    } else {
        format!(
            "Local tag '{}' already exists; reconciling changelog state.",
            request.tag_name
        )
    }]);

    let mut release_notes = Vec::new();
    if request.changelog_enabled {
        let repo_root_for_branch = request.repo_root.clone();
        let branch_name =
            run_blocking_job(move || current_branch_with_cancel(&repo_root_for_branch, None))
                .await?;
        emit_progress(vec![
            "Syncing standard changelog archive, summary, and memory state.".to_string(),
        ]);
        let std_outcome = execute_standard_changelog_for_tag(
            &request.scope,
            &request.tag_name,
            &branch_name,
            StdChangelogExecutionPolicy::Auto,
        )
        .await?;
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

    // QD HTML is built from the same artifact list attached to this release (see rls_now_qd).
    let mut qd_warnings = Vec::new();
    let release_notes_for_github = rls_now_qd::finalize_release_notes_with_quick_downloads(
        request.release_notes_markdown.clone(),
        request.scope.remote_spec.as_deref(),
        &request.tag_name,
        &artifact_files,
        &request.quick_downloads,
        &mut qd_warnings,
    );
    for warning in qd_warnings {
        emit_progress(vec![format!("Warning: {}", warning)]);
    }

    create_or_update_github_release(
        &request.repo_root,
        &request.tag_name,
        request.scope.remote_spec.as_deref(),
        &request.release_title,
        release_notes_for_github.as_deref(),
        &artifact_files,
        cancel.clone(),
        &mut emit_progress,
    )
    .await?;

    if request.readme_injection_enabled {
        ensure_not_cancelled(&cancel)?;
        emit_progress(vec![
            "Injecting 👀 What's new block into README.md.".to_string(),
        ]);
        let inj_repo_root = request.repo_root.clone();
        let inj_tag = request.tag_name.clone();
        let inj_markdown = request.release_notes_markdown.clone().unwrap_or_default();
        let inj_row = request.readme_inject_at_row;
        let inj_remote = request.scope.remote_spec.clone();
        let inj_result = run_blocking_job(move || {
            super::rls_now_inj::inject_whats_new(&super::rls_now_inj::ReadmeInjectionParams {
                repo_root: &inj_repo_root,
                tag_name: &inj_tag,
                changelog_markdown: &inj_markdown,
                inject_at_row: inj_row,
                remote_url: inj_remote.as_deref(),
            })?;
            run_git_checked(&inj_repo_root, &["add", "README.md"])?;
            Ok::<(), anyhow::Error>(())
        })
        .await;
        match inj_result {
            Ok(()) => emit_progress(vec!["README.md updated with What's new block.".to_string()]),
            Err(e) => emit_progress(vec![format!("Warning: README injection skipped: {}", e)]),
        }
    }

    if request.changelog_enabled || request.readme_injection_enabled {
        ensure_not_cancelled(&cancel)?;
        let repo_root_for_commit = request.repo_root.clone();
        let tag_name_for_commit = request.tag_name.clone();
        let generated_commit = run_blocking_job(move || {
            create_release_now_generated_files_commit(&repo_root_for_commit, &tag_name_for_commit)
        })
        .await?;

        if let Some(generated_commit) = generated_commit {
            let remote_spec = request.scope.remote_spec.clone().ok_or_else(|| {
                anyhow!("ReleaseNOW requires a configured git remote to push generated files")
            })?;
            let repo_root_for_branch = request.repo_root.clone();
            let cancel_for_branch = cancel.clone();
            let branch_name = run_blocking_job(move || {
                current_branch_with_cancel(&repo_root_for_branch, Some(cancel_for_branch))
            })
            .await?;

            emit_progress(vec![format!(
                "Pushing generated ReleaseNOW files to {}.",
                remote_spec
            )]);
            if let Err(push_error) = run_command_with_retry_async(
                request.repo_root.clone(),
                "git",
                vec!["push".to_string(), remote_spec.clone(), branch_name],
                GIT_PUSH_TIMEOUT,
                NETWORK_RETRY_ATTEMPTS,
                "git push",
            )
            .await
            {
                let repo_root_for_rollback = request.repo_root.clone();
                let rollback_result = run_blocking_job(move || {
                    rollback_release_now_generated_files_commit(
                        &repo_root_for_rollback,
                        &generated_commit,
                    )
                })
                .await;
                if let Err(rollback_error) = rollback_result {
                    return Err(anyhow!(
                        "{}; additionally failed to roll back the generated ReleaseNOW commit: {}",
                        push_error,
                        rollback_error
                    ));
                }
                return Err(push_error);
            }
        }
    }

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

pub(super) fn format_user_facing_error(message: &str) -> String {
    let normalized = message.to_ascii_lowercase();
    let detail = extract_relevant_error_detail(message);

    if normalized.contains("git push failed") {
        return build_guided_error(
            "ReleaseNOW could not push to the remote.",
            "Verify git authentication, remote write access, and whether the branch or tag is protected, then retry. Open the ReleaseNOW log for the exact git output.",
            detail.as_deref(),
        );
    }

    if normalized.contains("run windows script failed") {
        return build_guided_error(
            "ReleaseNOW Windows build script failed.",
            "Run the configured Windows script manually in PowerShell from the repository root and fix the first failing command shown in the ReleaseNOW log.",
            detail.as_deref(),
        );
    }

    if normalized.contains("configured releasenow script") && normalized.contains("was not found") {
        return build_guided_error(
            "ReleaseNOW could not find the configured script.",
            "Update Project Settings -> Distro so the selected platform points to a valid script path, then retry.",
            detail.as_deref(),
        );
    }

    if normalized.contains("no artifacts were found under dist/latest") {
        return build_guided_error(
            "ReleaseNOW finished the scripts but found no artifacts to publish.",
            "Make sure the script writes release files under dist/latest for the selected platform before retrying.",
            detail.as_deref(),
        );
    }

    if normalized.contains("gh release") || normalized.contains("github release") {
        return build_guided_error(
            "ReleaseNOW could not create the GitHub release.",
            "Check that GitHub CLI is authenticated and that the repository, tag, and release permissions are valid, then retry.",
            detail.as_deref(),
        );
    }

    build_guided_error(
        "ReleaseNOW failed.",
        "Open the ReleaseNOW log, copy the first concrete error line, fix that issue, and retry.",
        detail.as_deref(),
    )
}

fn build_guided_error(summary: &str, guidance: &str, detail: Option<&str>) -> String {
    match detail {
        Some(detail) if !detail.is_empty() => format!("{summary} {guidance} Detail: {detail}"),
        _ => format!("{summary} {guidance}"),
    }
}

fn extract_relevant_error_detail(message: &str) -> Option<String> {
    let cleaned = strip_terminal_control_sequences(message);
    let detail_source = cleaned
        .split_once(": ")
        .map(|(_, rest)| rest)
        .unwrap_or(cleaned.as_str());

    let preferred = detail_source
        .split(" | ")
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .find(|segment| {
            let lower = segment.to_ascii_lowercase();
            lower.contains("fatal:")
                || lower.contains("error:")
                || lower.contains("denied")
                || lower.contains("rejected")
                || lower.contains("not found")
                || lower.contains("failed")
        })
        .or_else(|| {
            detail_source
                .split(" | ")
                .map(str::trim)
                .find(|segment| !segment.is_empty())
        })?;

    Some(truncate_error_detail(preferred, 220))
}

fn truncate_error_detail(detail: &str, max_len: usize) -> String {
    let trimmed = detail.trim();
    if trimmed.chars().count() <= max_len {
        return trimmed.to_string();
    }

    let truncated = trimmed
        .chars()
        .take(max_len.saturating_sub(3))
        .collect::<String>();
    format!("{}...", truncated)
}

fn format_exit_code(code: Option<i32>) -> String {
    code.map(|value| value.to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn collect_release_now_options(settings: &ReleaseNowSettings) -> Result<Vec<ReleaseNowRunOption>> {
    if !settings.enabled {
        bail!("ReleaseNOW is disabled for this scope in Project Settings -> Distro")
    }

    let mut individual = Vec::new();
    push_release_option(
        &mut individual,
        "General",
        settings.general_script.as_str(),
        &[],
    );
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
            if !combined_artifact_dirs
                .iter()
                .any(|existing| existing == dir)
            {
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

fn build_recent_merge_warning(
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

    let mut warnings = Vec::new();

    for index in affected_scope_indexes {
        let scope = contexts
            .get(index)
            .ok_or_else(|| anyhow!("selected ReleaseNOW scope no longer exists"))?;
        let check = recent_merge_check(&scope.repo_root, &scope.git_pathspecs(), cancel.clone())?;
        if check != "pass" {
            warnings.push(format!(
                "- {}: no recent pull request merge was found",
                scope.display_name
            ));
        }
    }

    if warnings.is_empty() {
        Ok(None)
    } else {
        Ok(Some(format!(
            "ReleaseNOW! expected a recent pull request merge within the last 5 minutes. You can safely ignore this warning if you are intentionally running a release without a recent merge. Just confirm with the yellow-ish button below.\n\n\n{}",
            warnings.join("\n")
        )))
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
    let (path_str, extra_args) = parse_shell_args(&script.script_path);
    let script_path = resolve_script_path(repo_root, path_str)?;
    let display_path = script_path.display().to_string();
    let (program, mut args) = script_command(&script_path)?;
    args.extend(extra_args);
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

fn parse_shell_args(input: &str) -> (&str, Vec<String>) {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return ("", Vec::new());
    }

    // Find end of first token (path), respecting quotes
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut path_end = 0;

    for (i, c) in trimmed.char_indices() {
        match c {
            '\'' if !in_double_quote => {
                in_single_quote = !in_single_quote;
            }
            '"' if !in_single_quote => {
                in_double_quote = !in_double_quote;
            }
            c if c.is_whitespace()
                && !in_single_quote
                && !in_double_quote
                && path_end == 0
                && i > 0 =>
            {
                path_end = i;
                break;
            }
            _ => {}
        }
    }

    if path_end == 0 {
        path_end = trimmed.len();
    }

    let path = &trimmed[..path_end].trim();
    let rest = &trimmed[path_end..].trim_start();

    // Parse remaining arguments
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;

    for c in rest.chars() {
        match c {
            '\'' if !in_double => {
                in_single = !in_single;
            }
            '"' if !in_single => {
                in_double = !in_double;
            }
            c if c.is_whitespace() && !in_single && !in_double => {
                if !current.is_empty() {
                    args.push(current.clone());
                    current.clear();
                }
            }
            c => {
                current.push(c);
            }
        }
    }

    if !current.is_empty() {
        args.push(current);
    }

    (path, args)
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
        bail!(
            "configured ReleaseNOW script '{}' was not found",
            resolved.display()
        )
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

#[allow(clippy::too_many_arguments)]
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
    let mut recent_lines: Vec<String> = Vec::new();

    loop {
        match line_rx.recv_timeout(Duration::from_millis(100)) {
            Ok((stream, line)) => {
                let lines = collect_stream_lines(&line_rx, log_label, Some((stream, line)));
                if !lines.is_empty() {
                    let _ = progress_tx.send(lines.clone());
                    recent_lines.extend(lines);
                    if recent_lines.len() > 20 {
                        recent_lines.drain(0..recent_lines.len() - 20);
                    }
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {}
        }

        if cancel.is_cancelled() {
            let _ = terminate_process_tree(&mut child);
            join_stream_reader(stdout_thread, action)?;
            join_stream_reader(stderr_thread, action)?;
            let lines = collect_stream_lines(&line_rx, log_label, None);
            if !lines.is_empty() {
                let _ = progress_tx.send(lines.clone());
                recent_lines.extend(lines);
                if recent_lines.len() > 20 {
                    recent_lines.drain(0..recent_lines.len() - 20);
                }
            }
            bail!("ReleaseNOW cancelled by user")
        }

        if let Some(status) = child
            .try_wait()
            .with_context(|| format!("failed to poll {}", action))?
        {
            join_stream_reader(stdout_thread, action)?;
            join_stream_reader(stderr_thread, action)?;
            let lines = collect_stream_lines(&line_rx, log_label, None);
            if !lines.is_empty() {
                let _ = progress_tx.send(lines.clone());
                recent_lines.extend(lines);
                if recent_lines.len() > 20 {
                    recent_lines.drain(0..recent_lines.len() - 20);
                }
            }

            if status.success() {
                return Ok(());
            }

            if recent_lines.is_empty() {
                bail!(
                    "{} failed with exit code {}",
                    action,
                    format_exit_code(status.code())
                );
            }

            bail!(
                "{} failed with exit code {}: {}",
                action,
                format_exit_code(status.code()),
                recent_lines.join(" | ")
            )
        }

        if started_at.elapsed() >= timeout_window {
            let _ = terminate_process_tree(&mut child);
            join_stream_reader(stdout_thread, action)?;
            join_stream_reader(stderr_thread, action)?;
            let lines = collect_stream_lines(&line_rx, log_label, None);
            if !lines.is_empty() {
                let _ = progress_tx.send(lines.clone());
            }
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
    for entry in
        fs::read_dir(root).with_context(|| format!("failed to read '{}'", root.display()))?
    {
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

#[allow(clippy::too_many_arguments)]
async fn create_or_update_github_release(
    repo_root: &str,
    tag_name: &str,
    remote_spec: Option<&str>,
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
            emit_progress(vec![format!(
                "Updating existing GitHub release '{}'.",
                tag_name
            )]);
            let repo_root_owned = repo_root.to_string();
            let upload_cancel = cancel.clone();

            let mut upload_args = vec![
                "release".to_string(),
                "upload".to_string(),
                tag_name.to_string(),
            ];
            upload_args.extend(artifact_files.iter().cloned());
            upload_args.push("--clobber".to_string());
            #[allow(clippy::too_many_arguments)]
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
            let remote_spec = remote_spec.ok_or_else(|| {
                anyhow!("ReleaseNOW requires a configured git remote to publish a GitHub release")
            })?;
            emit_progress(vec![format!(
                "Pushing tag '{}' to {}.",
                tag_name, remote_spec
            )]);

            let repo_root_owned = repo_root.to_string();
            let push_cancel = cancel.clone();
            let push_args = vec![
                "push".to_string(),
                remote_spec.to_string(),
                tag_name.to_string(),
            ];
            run_blocking_streaming_operation(
                move |progress_tx| {
                    run_command_with_streaming(
                        &repo_root_owned,
                        "git",
                        &push_args,
                        RELEASE_NOW_TIMEOUT,
                        "git push",
                        "git",
                        &push_cancel,
                        &progress_tx,
                    )
                },
                emit_progress,
            )
            .await?;

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

fn read_command_stream<R>(
    stream: R,
    stream_name: &'static str,
    line_tx: StdSender<(String, String)>,
) -> Result<()>
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

fn collect_stream_lines(
    line_rx: &Receiver<(String, String)>,
    log_label: &str,
    first_line: Option<(String, String)>,
) -> Vec<String> {
    let mut lines = Vec::new();

    if let Some((stream, line)) = first_line {
        lines.push(format!("[{}][{}] {}", log_label, stream, line));
    }

    while let Ok((stream, line)) = line_rx.try_recv() {
        lines.push(format!("[{}][{}] {}", log_label, stream, line));
    }

    lines
}

fn terminate_process_tree(child: &mut std::process::Child) -> Result<()> {
    if cfg!(windows) {
        let status = Command::new("taskkill")
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();

        if status
            .as_ref()
            .map(|value| !value.success())
            .unwrap_or(true)
        {
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
            for next in chars.by_ref() {
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
        "cg-release-now-notes-{}-{}.md",
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
    use std::{env, fs};

    fn create_temp_repo_dir(test_name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir = env::temp_dir().join(format!(
            "comfygit-{}-{}-{}",
            test_name,
            std::process::id(),
            unique
        ));
        fs::create_dir_all(&dir).expect("create temp repo dir");
        dir
    }

    #[test]
    fn release_now_options_keep_all_configured_first() {
        let settings = ReleaseNowSettings {
            enabled: true,
            windows_script: "scripts/releaseNOW.ps1".to_string(),
            linux_arm_script: String::new(),
            linux_amd_script: "scripts/releaseNOW-linux_amd64.ps1".to_string(),
            macos_script: String::new(),
            ..Default::default()
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

    #[test]
    fn collect_stream_lines_drains_all_buffered_output() {
        let (line_tx, line_rx) = channel();
        line_tx
            .send(("stdout".to_string(), "line 2".to_string()))
            .expect("send line 2");
        line_tx
            .send(("stderr".to_string(), "line 3".to_string()))
            .expect("send line 3");

        let lines = collect_stream_lines(
            &line_rx,
            "Linux AMD",
            Some(("stdout".to_string(), "line 1".to_string())),
        );

        assert_eq!(
            lines,
            vec![
                "[Linux AMD][stdout] line 1".to_string(),
                "[Linux AMD][stdout] line 2".to_string(),
                "[Linux AMD][stderr] line 3".to_string(),
            ]
        );
    }

    #[test]
    fn format_user_facing_error_guides_git_push_failures() {
        let message = "git push failed with exit code 1: [git][stderr] remote: Permission to org/repo denied to user.";

        let formatted = format_user_facing_error(message);

        assert!(formatted.contains("could not push to the remote"));
        assert!(formatted.contains("authentication"));
        assert!(formatted.contains("Permission to org/repo denied to user"));
    }

    #[test]
    fn format_user_facing_error_guides_windows_script_failures() {
        let message = "run Windows script failed with exit code 1: [Windows][stderr] error: cargo build failed";

        let formatted = format_user_facing_error(message);

        assert!(formatted.contains("Windows build script failed"));
        assert!(formatted.contains("Run the configured Windows script manually in PowerShell"));
        assert!(formatted.contains("cargo build failed"));
    }

    #[test]
    fn format_exit_code_removes_debug_option_wrapper() {
        assert_eq!(format_exit_code(Some(1)), "1");
        assert_eq!(format_exit_code(None), "unknown");
    }

    #[test]
    fn rollback_release_now_generated_files_commit_restores_previous_head() {
        let repo_dir = create_temp_repo_dir("release-now-rollback");
        let repo_root = repo_dir.to_string_lossy().to_string();

        run_git_checked(&repo_root, &["init"]).expect("init repo");
        run_git_checked(&repo_root, &["config", "user.name", "ComfyGit Tests"])
            .expect("configure user.name");
        run_git_checked(
            &repo_root,
            &["config", "user.email", "tests@comfygit.invalid"],
        )
        .expect("configure user.email");

        fs::write(repo_dir.join("README.md"), "seed\n").expect("write seed file");
        run_git_checked(&repo_root, &["add", "README.md"]).expect("stage seed file");
        run_git_checked(&repo_root, &["commit", "-m", "seed"]).expect("commit seed file");

        let previous_head = current_head_commit(&repo_root).expect("read initial head");
        let syncmem_dir = repo_dir.join(".comfygit").join("syncmem");
        fs::create_dir_all(&syncmem_dir).expect("create syncmem dir");
        fs::write(syncmem_dir.join("stdchlg.json"), "{}\n").expect("write syncmem file");

        let generated_commit = create_release_now_generated_files_commit(&repo_root, "v1.2.3")
            .expect("create generated commit")
            .expect("generated commit should exist");

        let release_commit_subject = run_git_checked(&repo_root, &["log", "-1", "--pretty=%s"])
            .expect("read release commit subject");
        assert!(release_commit_subject.contains("ReleaseNOW! → v1.2.3 has just been released"));

        rollback_release_now_generated_files_commit(&repo_root, &generated_commit)
            .expect("roll back generated commit");

        assert_eq!(
            current_head_commit(&repo_root).expect("read restored head"),
            previous_head
        );

        let status = run_git_checked(&repo_root, &["status", "--short"])
            .expect("read staged status after rollback");
        assert!(status.contains("A  .comfygit/syncmem/stdchlg.json"));

        fs::remove_dir_all(&repo_dir).expect("remove temp repo dir");
    }
}
