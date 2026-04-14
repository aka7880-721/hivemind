// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyVersionBumper License v1.2
//
// For details, see the LICENSE file in the repository root.

use super::*;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, anyhow, bail};

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
    pub(super) repo_root: String,
    pub(super) tag_name: String,
    pub(super) options: Vec<ReleaseNowRunOption>,
    pub(super) selected_option: usize,
    pub(super) attach_changelog: bool,
    pub(super) release_notes_markdown: String,
    pub(super) release_notes_placeholder: String,
    pub(super) warning_message: Option<String>,
    pub(super) mode: ReleaseNowMode,
    pub(super) warning_confirm_selected: bool,
    pub(super) scroll: u16,
    pub(super) summary: Option<String>,
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
            repo_root: validation.repo_root,
            tag_name: validation.tag_name,
            options: validation.options,
            selected_option: 0,
            attach_changelog: true,
            release_notes_markdown: validation.release_notes_markdown,
            release_notes_placeholder: "Edit release notes in Markdown before publishing.".to_string(),
            warning_message: validation.warning_message,
            mode,
            warning_confirm_selected: false,
            scroll: 0,
            summary: None,
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
        self.scroll = self.scroll.saturating_add_signed(delta);
    }

    pub(super) fn apply_outcome(&mut self, outcome: ReleaseNowExecutionOutcome) {
        self.mode = ReleaseNowMode::Completed;
        self.summary = Some(outcome.summary);
        self.artifact_files = outcome.artifact_files;
        self.log_lines = outcome.log_lines;
        self.scroll = 0;
    }

    pub(super) fn body_title(&self) -> &'static str {
        match self.mode {
            ReleaseNowMode::BumpWarning => " Bump Check ",
            ReleaseNowMode::Configure => {
                if self.attach_changelog {
                    " Release Notes Preview "
                } else {
                    " Release Summary "
                }
            }
            ReleaseNowMode::Completed => " Release Log ",
        }
    }

    pub(super) fn rendered_body_lines(&self) -> Vec<Line<'static>> {
        match self.mode {
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
                if self.attach_changelog {
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
                            .style(Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
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
                    lines.extend(self.log_lines.iter().map(|line| Line::from(line.clone())));
                }
                lines
            }
        }
    }
}

#[derive(Clone)]
pub(super) struct ReleaseNowValidation {
    pub(super) project_name: String,
    pub(super) scope_label: String,
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
    pub(super) project_name: String,
    pub(super) scope_label: String,
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
        repo_root: scope.repo_root.clone(),
        tag_name: scope.suggested_tag_name.clone(),
        options,
        warning_message,
        release_notes_markdown,
    })
}

pub(super) fn build_execution_request(dialog: &ReleaseNowDialog) -> ReleaseNowExecutionRequest {
    ReleaseNowExecutionRequest {
        project_name: dialog.project_name.clone(),
        scope_label: dialog.scope_label.clone(),
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
) -> Result<ReleaseNowExecutionOutcome> {
    ensure_gh_authenticated()?;

    let mut log_lines = vec![format!(
        "Starting ReleaseNOW for {} using {}.",
        request.scope_label, request.selected_option_label
    )];

    for script in &request.scripts {
        let repo_root = request.repo_root.clone();
        let script_to_run = script.clone();
        let output = run_blocking_job(move || run_script_and_collect_logs(&repo_root, &script_to_run)).await?;
        log_lines.extend(output);
    }

    let repo_root = request.repo_root.clone();
    let artifact_dirs = request.artifact_dirs.clone();
    let artifact_files = run_blocking_job(move || discover_artifacts(&repo_root, &artifact_dirs)).await?;
    if artifact_files.is_empty() {
        bail!(
            "ReleaseNOW finished running scripts, but no artifacts were found under dist/latest for {}",
            request.selected_option_label
        )
    }

    let repo_root = request.repo_root.clone();
    let tag_name = request.tag_name.clone();
    let release_title = request.release_title.clone();
    let release_notes_markdown = request.release_notes_markdown.clone();
    let upload_files = artifact_files.clone();
    let release_log_lines = run_blocking_job(move || {
        create_or_update_github_release(
            &repo_root,
            &tag_name,
            &release_title,
            release_notes_markdown.as_deref(),
            &upload_files,
        )
    })
    .await?;
    log_lines.extend(release_log_lines);

    Ok(ReleaseNowExecutionOutcome {
        summary: format!(
            "ReleaseNOW published '{}' with {} artifact(s) using {}.",
            request.tag_name,
            artifact_files.len(),
            request.selected_option_label
        ),
        artifact_files,
        log_lines,
    })
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

fn run_script_and_collect_logs(repo_root: &str, script: &ReleaseNowScript) -> Result<Vec<String>> {
    let script_path = resolve_script_path(repo_root, &script.script_path)?;
    let display_path = script_path.display().to_string();
    let (program, args) = script_command(&script_path)?;
    let output = run_command_with_capture(repo_root, &program, &args, RELEASE_NOW_TIMEOUT, &format!("run {} script", script.label))?;

    let mut lines = vec![format!("[{}] Running {}", script.label, display_path)];
    lines.extend(prefix_output_lines(
        &script.label,
        "stdout",
        &String::from_utf8_lossy(&output.stdout),
    ));
    lines.extend(prefix_output_lines(
        &script.label,
        "stderr",
        &String::from_utf8_lossy(&output.stderr),
    ));
    lines.push(format!("[{}] Completed successfully.", script.label));
    Ok(lines)
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
        return Ok((
            "pwsh".to_string(),
            vec![
                "-NoProfile".to_string(),
                "-ExecutionPolicy".to_string(),
                "Bypass".to_string(),
                "-File".to_string(),
                script_path.display().to_string(),
            ],
        ));
    }

    Ok((script_path.display().to_string(), Vec::new()))
}

fn prefix_output_lines(label: &str, stream: &str, output: &str) -> Vec<String> {
    let lines = split_output_lines(output);
    if lines.is_empty() {
        Vec::new()
    } else {
        lines
            .into_iter()
            .map(|line| format!("[{}][{}] {}", label, stream, line))
            .collect()
    }
}

fn run_command_with_capture(
    repo_root: &str,
    program: &str,
    args: &[String],
    timeout: Duration,
    action: &str,
) -> Result<std::process::Output> {
    let mut command = Command::new(program);
    command
        .current_dir(repo_root)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .with_context(|| format!("failed to start {} in '{}'", action, repo_root))?;
    let started_at = Instant::now();

    loop {
        if let Some(status) = child.try_wait().with_context(|| format!("failed to poll {}", action))? {
            let output = child
                .wait_with_output()
                .with_context(|| format!("failed to collect output for {}", action))?;
            if status.success() {
                return Ok(output);
            }

            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let detail = if stderr.is_empty() { stdout } else { stderr };
            bail!("{} failed: {}", action, detail)
        }

        if started_at.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait_with_output();
            bail!("{} timed out after {}s", action, timeout.as_secs())
        }

        std::thread::sleep(Duration::from_millis(100));
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

fn create_or_update_github_release(
    repo_root: &str,
    tag_name: &str,
    release_title: &str,
    release_notes_markdown: Option<&str>,
    artifact_files: &[String],
) -> Result<Vec<String>> {
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

    let mut log_lines = Vec::new();
    if release_exists {
        let mut upload_args = vec!["release".to_string(), "upload".to_string(), tag_name.to_string()];
        upload_args.extend(artifact_files.iter().cloned());
        upload_args.push("--clobber".to_string());
        let output = run_command_with_capture(repo_root, "gh", &upload_args, RELEASE_NOW_TIMEOUT, "gh release upload")?;
        log_lines.push(format!("Updated existing GitHub release '{}'.", tag_name));
        log_lines.extend(prefix_output_lines("gh", "stdout", &String::from_utf8_lossy(&output.stdout)));
        log_lines.extend(prefix_output_lines("gh", "stderr", &String::from_utf8_lossy(&output.stderr)));

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
            let output = run_command_with_capture(repo_root, "gh", &edit_args, RELEASE_NOW_TIMEOUT, "gh release edit")?;
            log_lines.extend(prefix_output_lines("gh", "stdout", &String::from_utf8_lossy(&output.stdout)));
            log_lines.extend(prefix_output_lines("gh", "stderr", &String::from_utf8_lossy(&output.stderr)));
        }
    } else {
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
        let output = run_command_with_capture(repo_root, "gh", &create_args, RELEASE_NOW_TIMEOUT, "gh release create")?;
        log_lines.push(format!("Created GitHub release '{}'.", tag_name));
        log_lines.extend(prefix_output_lines("gh", "stdout", &String::from_utf8_lossy(&output.stdout)));
        log_lines.extend(prefix_output_lines("gh", "stderr", &String::from_utf8_lossy(&output.stderr)));
    }

    if let Some(notes_file) = notes_file {
        let _ = fs::remove_file(notes_file);
    }

    Ok(log_lines)
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
}
