// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit SA-PS License
//
// For details, see the LICENSE file in the repository root.

fn ensure_not_cancelled(cancel: &GitCancellation) -> Result<()> {
    if cancel.is_cancelled() {
        bail!("ReleaseNOW cancelled by user")
    }
    Ok(())
}

fn release_now_generated_paths(
    repo_root: &str,
    mirror_summary_to_root_changelog: bool,
) -> Vec<String> {
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
    if mirror_summary_to_root_changelog && Path::new(repo_root).join("CHANGELOG.md").is_file() {
        paths.push("CHANGELOG.md".to_string());
    }
    paths
}

fn stage_release_now_generated_files(
    repo_root: &str,
    mirror_summary_to_root_changelog: bool,
) -> Result<bool> {
    let paths = release_now_generated_paths(repo_root, mirror_summary_to_root_changelog);
    if paths.is_empty() {
        return Ok(false);
    }

    let mut args = vec!["add".to_string(), "--".to_string()];
    args.extend(paths);
    let arg_refs = args.iter().map(|s| s.as_str()).collect::<Vec<_>>();
    run_git_checked(repo_root, &arg_refs)?;
    Ok(true)
}

fn has_staged_changes_for_paths(repo_root: &str, paths: &[String]) -> Result<bool> {
    let mut args = vec![
        "diff".to_string(),
        "--cached".to_string(),
        "--quiet".to_string(),
        "--exit-code".to_string(),
    ];
    if !paths.is_empty() {
        args.push("--".to_string());
        args.extend(paths.iter().cloned());
    }
    let arg_refs = args.iter().map(|s| s.as_str()).collect::<Vec<_>>();
    Ok(!run_git(repo_root, &arg_refs)?.success)
}

fn release_now_commit_subject(tag_name: &str) -> String {
    format!(
        "~: ReleaseNOW! → {} has just been released via ComfyGit!",
        tag_name
    )
}

pub(crate) fn release_now_delete_commit_subject(tag_name: &str) -> String {
    format!(
        "~: ReleaseNOW! → {} release has just been DELETED via ComfyGit!",
        tag_name
    )
}

fn release_now_artifacts_body(artifact_files: &[String]) -> String {
    let mut entries = artifact_files
        .iter()
        .map(|path| {
            Path::new(path)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(path.as_str())
                .trim()
                .to_string()
        })
        .filter(|name| !name.is_empty())
        .collect::<Vec<_>>();
    entries.sort_by_cached_key(|entry| entry.to_lowercase());
    entries.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    format!("Artifacts: {}", entries.join(", "))
}

fn parse_release_now_artifacts_from_commit_body(body: &str) -> Vec<String> {
    for line in body.lines() {
        if let Some(rest) = line.trim().strip_prefix("Artifacts:") {
            return rest
                .split(',')
                .map(str::trim)
                .filter(|entry| !entry.is_empty())
                .map(ToOwned::to_owned)
                .collect();
        }
    }
    Vec::new()
}

fn historical_release_now_artifacts_for_tag(
    repo_root: &str,
    tag_name: &str,
) -> Result<Vec<String>> {
    let output = run_git_checked(
        repo_root,
        &["log", "--format=%s%x1f%b%x1e", "--max-count=256"],
    )?;
    let release_subject = release_now_commit_subject(tag_name);
    let delete_subject = release_now_delete_commit_subject(tag_name);
    for entry in output.split('\x1e') {
        let trimmed = entry.trim();
        if trimmed.is_empty() {
            continue;
        }
        let mut fields = trimmed.splitn(2, '\x1f');
        let subject = fields.next().unwrap_or("").trim();
        let body = fields.next().unwrap_or("").trim();
        if subject == delete_subject {
            return Ok(Vec::new());
        }
        if subject == release_subject {
            return Ok(parse_release_now_artifacts_from_commit_body(body));
        }
    }
    Ok(Vec::new())
}

fn commit_release_now_generated_files(
    repo_root: &str,
    tag_name: &str,
    artifact_files: &[String],
    mirror_summary_to_root_changelog: bool,
) -> Result<bool> {
    let paths = release_now_generated_paths(repo_root, mirror_summary_to_root_changelog);
    if paths.is_empty() || !has_staged_changes_for_paths(repo_root, &paths)? {
        return Ok(false);
    }

    let mut args = vec![
        "commit".to_string(),
        "-m".to_string(),
        release_now_commit_subject(tag_name),
        "-m".to_string(),
        release_now_artifacts_body(artifact_files),
    ];
    args.push("--".to_string());
    args.extend(paths);
    let arg_refs = args.iter().map(|s| s.as_str()).collect::<Vec<_>>();
    run_git_checked(repo_root, &arg_refs)?;
    Ok(true)
}

fn commit_auto_injected_readme(repo_root: &str) -> Result<bool> {
    let readme_path = vec!["README.md".to_string()];
    if !has_staged_changes_for_paths(repo_root, &readme_path)? {
        return Ok(false);
    }

    run_git_checked(
        repo_root,
        &[
            "commit",
            "-m",
            "~ReleaseNOW: Changelog Auto-injection",
            "--",
            "README.md",
        ],
    )?;
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
    artifact_files: &[String],
    mirror_summary_to_root_changelog: bool,
) -> Result<Option<ReleaseNowGeneratedFilesCommit>> {
    let previous_head = current_head_commit(repo_root)?;
    let mut created_any = false;

    if commit_auto_injected_readme(repo_root)? {
        created_any = true;
    }

    if stage_release_now_generated_files(repo_root, mirror_summary_to_root_changelog)?
        && commit_release_now_generated_files(
            repo_root,
            tag_name,
            artifact_files,
            mirror_summary_to_root_changelog,
        )?
    {
        created_any = true;
    }

    Ok(created_any.then_some(ReleaseNowGeneratedFilesCommit { previous_head }))
}

fn stage_auto_injected_readme(
    repo_root: &str,
    tag_name: &str,
    changelog_markdown: &str,
    inject_at_row: u16,
    remote_url: Option<&str>,
    inject_only_top_picks: bool,
    inject_depth: crate::config::ReadmeInjectDepth,
) -> Result<()> {
    crate::workflow::rls_now_inj::inject_whats_new(
        &crate::workflow::rls_now_inj::ReadmeInjectionParams {
            repo_root,
            tag_name,
            changelog_markdown,
            inject_at_row,
            remote_url,
            inject_only_top_picks,
            inject_depth,
        },
    )?;
    run_git_checked(repo_root, &["add", "README.md"])?;
    Ok(())
}

fn remote_branch_head_commit(
    repo_root: &str,
    remote_name: &str,
    branch_name: &str,
) -> Result<Option<String>> {
    let output = run_git_checked(
        repo_root,
        &["ls-remote", "--heads", remote_name, branch_name],
    )?;
    let line = output.lines().find(|line| !line.trim().is_empty());
    let hash = line
        .and_then(|entry| entry.split_whitespace().next())
        .map(str::trim)
        .filter(|hash| !hash.is_empty())
        .map(ToOwned::to_owned);
    Ok(hash)
}

async fn confirm_remote_branch_head_with_retry(
    repo_root: String,
    remote_name: String,
    branch_name: String,
    expected_head: String,
    cancel: &GitCancellation,
    mut emit_progress: impl FnMut(Vec<String>),
) -> Result<()> {
    for (attempt, delay_secs) in [(1_u8, 5_u64), (2_u8, 10_u64)] {
        ensure_not_cancelled(cancel)?;
        emit_progress(vec![format!(
            "Waiting {}s to confirm the README auto-injection push on {}.",
            delay_secs, remote_name
        )]);
        sleep(Duration::from_secs(delay_secs)).await;
        ensure_not_cancelled(cancel)?;

        let repo_root_for_check = repo_root.clone();
        let remote_name_for_check = remote_name.clone();
        let branch_name_for_check = branch_name.clone();
        let remote_head = run_blocking_job(move || {
            remote_branch_head_commit(
                &repo_root_for_check,
                &remote_name_for_check,
                &branch_name_for_check,
            )
        })
        .await?;

        if remote_head.as_deref() == Some(expected_head.as_str()) {
            emit_progress(vec![
                "README auto-injection push confirmed on remote.".to_string(),
            ]);
            return Ok(());
        }

        if attempt == 1 {
            emit_progress(vec![format!(
                "README auto-injection push was not visible on {} after {}s; retrying confirmation once more.",
                remote_name, delay_secs
            )]);
        }
    }

    bail!(
        "README auto-injection push could not be confirmed on remote '{}' for branch '{}'",
        remote_name,
        branch_name
    )
}

async fn prepush_auto_injected_readme_async(
    request: &ReleaseNowExecutionRequest,
    cancel: &GitCancellation,
    mut emit_progress: impl FnMut(Vec<String>),
) -> Result<()> {
    ensure_not_cancelled(cancel)?;
    emit_progress(vec![
        "Injecting 👀 What's new block into README.md.".to_string(),
    ]);

    let inj_repo_root = request.repo_root.clone();
    let inj_tag = request.tag_name.clone();
    let inj_markdown = request.release_notes_markdown.clone().unwrap_or_default();
    let inj_row = request.readme_inject_at_row;
    let inj_remote = request.scope.remote_spec.clone();
    let inj_only_top_picks = request.readme_inject_only_top_picks;
    let inj_depth = request.readme_inject_depth;
    run_blocking_job(move || {
        stage_auto_injected_readme(
            &inj_repo_root,
            &inj_tag,
            &inj_markdown,
            inj_row,
            inj_remote.as_deref(),
            inj_only_top_picks,
            inj_depth,
        )
    })
    .await?;

    let repo_root_for_commit = request.repo_root.clone();
    let committed =
        run_blocking_job(move || commit_auto_injected_readme(&repo_root_for_commit)).await?;
    if !committed {
        emit_progress(vec![
            "README auto-injection produced no staged changes to commit.".to_string(),
        ]);
        return Ok(());
    }

    let remote_spec = request.scope.remote_spec.clone().ok_or_else(|| {
        anyhow!("ReleaseNOW requires a configured git remote to push the auto-injected README")
    })?;
    let repo_root_for_branch = request.repo_root.clone();
    let cancel_for_branch = cancel.clone();
    let branch_name = run_blocking_job(move || {
        current_branch_with_cancel(&repo_root_for_branch, Some(cancel_for_branch))
    })
    .await?;
    let repo_root_for_remote = request.repo_root.clone();
    let remote_name = run_blocking_job(move || {
        crate::git::resolve_push_remote_name(&repo_root_for_remote, &remote_spec)
    })
    .await?;
    let repo_root_for_head = request.repo_root.clone();
    let expected_head = run_blocking_job(move || current_head_commit(&repo_root_for_head)).await?;

    emit_progress(vec![format!(
        "Pushing README auto-injection commit to {}.",
        remote_name
    )]);
    run_command_with_retry_async(
        request.repo_root.clone(),
        "git",
        vec!["push".to_string(), remote_name.clone(), branch_name.clone()],
        GIT_PUSH_TIMEOUT,
        NETWORK_RETRY_ATTEMPTS,
        "git push",
    )
    .await?;

    confirm_remote_branch_head_with_retry(
        request.repo_root.clone(),
        remote_name,
        branch_name,
        expected_head,
        cancel,
        &mut emit_progress,
    )
    .await
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

fn mirror_summary_changelog_to_root(repo_root: &str) -> Result<bool> {
    let source = Path::new(repo_root).join(".changelogs").join("README.md");
    if !source.is_file() {
        return Ok(false);
    }
    let destination = Path::new(repo_root).join("CHANGELOG.md");
    let summary = fs::read_to_string(&source)
        .with_context(|| format!("failed to read {}", source.display()))?;
    let needs_update = fs::read_to_string(&destination)
        .map(|current| current != summary)
        .unwrap_or(true);
    if !needs_update {
        return Ok(false);
    }
    fs::write(&destination, summary)
        .with_context(|| format!("failed to write {}", destination.display()))?;
    Ok(true)
}

// For details, see the LICENSE file in the repository root.

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc::{Receiver, RecvTimeoutError, Sender as StdSender, channel},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, anyhow, bail};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use tokio::{
    sync::mpsc::{UnboundedSender, unbounded_channel},
    task::spawn_blocking,
    time::sleep,
};

use crate::{
    app::{
        StdChangelogExecutionPolicy, append_background_tag_summary_notes,
        build_release_notes_markdown, execute_standard_changelog_for_tag,
    },
    changelog::clear_top_picks_edits,
    config::{ProjectConfig, ReleaseNowQuickDownloadsSettings, ReleaseNowSettings},
    git::{
        GitCancellation, GitScopeContext, collect_all_branch_git_scope_contexts,
        current_branch_with_cancel, ensure_local_tag, recent_merge_check, run_git, run_git_checked,
        split_output_lines,
    },
    workflow::runtime::{
        GIT_PUSH_TIMEOUT, NETWORK_RETRY_ATTEMPTS, run_blocking_job, run_command_with_retry_async,
    },
};

#[path = "rls_now_qd.rs"]
mod rls_now_qd;

const RELEASE_NOW_TIMEOUT: Duration = Duration::from_secs(60 * 60);
const DEFAULT_RELEASE_NOTES: &str =
    "# Release Notes\n\nAdd release highlights here before publishing.";

fn resolve_release_push_remote(repo_root: &str, configured_remote: Option<&str>) -> Result<String> {
    if let Some(configured_remote) = configured_remote
        && !configured_remote.trim().is_empty()
    {
        return crate::git::resolve_push_remote_name(repo_root, configured_remote);
    }
    crate::git::default_push_remote_name(repo_root)
}

fn resolve_forge_for_release_request(
    request: &ReleaseNowExecutionRequest,
) -> Result<crate::forge::ForgeKind> {
    if let Some(remote_url) = request
        .scope
        .remote_spec
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if remote_url.contains("gitlab.com") {
            return Ok(crate::forge::ForgeKind::GitLab);
        }
        if remote_url.contains("github.com") {
            return Ok(crate::forge::ForgeKind::GitHub);
        }
    }

    crate::forge::detect_forge_for_repo(&request.repo_root).ok_or_else(|| {
        anyhow!("ReleaseNOW could not detect GitHub or GitLab from the repository remote")
    })
}

fn repo_selector_from_remote_url(
    forge: crate::forge::ForgeKind,
    remote_url: &str,
) -> Option<String> {
    let trimmed = remote_url.trim();
    let path = match forge {
        crate::forge::ForgeKind::GitHub => trimmed
            .strip_prefix("git@github.com:")
            .or_else(|| trimmed.strip_prefix("https://github.com/"))
            .or_else(|| trimmed.strip_prefix("ssh://git@github.com/"))?,
        crate::forge::ForgeKind::GitLab => trimmed
            .strip_prefix("git@gitlab.com:")
            .or_else(|| trimmed.strip_prefix("https://gitlab.com/"))
            .or_else(|| trimmed.strip_prefix("ssh://git@gitlab.com/"))?,
    };
    let path = path.trim_end_matches(".git").trim();
    if path.is_empty() {
        return None;
    }
    Some(path.to_string())
}

fn release_cli_repo_args(
    forge: crate::forge::ForgeKind,
    repo_root: &str,
    configured_remote: Option<&str>,
) -> Result<Vec<String>> {
    let remote_name = resolve_release_push_remote(repo_root, configured_remote)?;
    let remote_url = crate::git::run_git_checked(repo_root, &["remote", "get-url", &remote_name])?;
    let repo_selector =
        repo_selector_from_remote_url(forge, remote_url.trim()).ok_or_else(|| {
            anyhow!(
                "ReleaseNOW could not derive {} repository path from remote '{}'",
                forge.display_name(),
                remote_url.trim()
            )
        })?;
    Ok(vec!["-R".to_string(), repo_selector])
}

fn gitlab_release_asset_argument(path: &str) -> String {
    let lower = path.to_ascii_lowercase();
    let is_package = lower.ends_with(".deb")
        || lower.ends_with(".rpm")
        || lower.ends_with(".pkg")
        || lower.ends_with(".msi")
        || lower.ends_with(".appimage");
    if is_package {
        format!("{path}#{}#package", release_asset_label_from_path(path))
    } else {
        path.to_string()
    }
}

fn release_asset_label_from_path(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or(path)
        .to_string()
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReleaseNowMode {
    MirrorSync,
    BumpWarning,
    ExistingArtifacts,
    ArtifactsCustomize,
    Configure,
    Completed,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ReleaseNowArtifactStrategy {
    #[default]
    Pending,
    ReuseAll,
    RebuildAll,
    PerPlatform,
}

#[derive(Clone)]
pub(crate) struct ReleaseNowPlatformArtifactStatus {
    pub(crate) label: String,
    pub(crate) script: ReleaseNowScript,
    pub(crate) existing_files: Vec<String>,
    pub(crate) ready: bool,
}

#[derive(Clone)]
pub(crate) struct ReleaseNowDialog {
    pub(crate) project_name: String,
    pub(crate) scope_label: String,
    pub(crate) scope: GitScopeContext,
    pub(crate) changelog_enabled: bool,
    pub(crate) mirror_summary_to_root_changelog: bool,
    pub(crate) repo_root: String,
    pub(crate) tag_name: String,
    pub(crate) options: Vec<ReleaseNowRunOption>,
    pub(crate) selected_option: usize,
    pub(crate) attach_changelog: bool,
    pub(crate) release_notes_markdown: String,
    pub(crate) release_notes_placeholder: String,
    pub(crate) warning_message: Option<String>,
    pub(crate) mode: ReleaseNowMode,
    pub(crate) running: bool,
    pub(crate) auto_follow: bool,
    pub(crate) cancel_requested: bool,
    pub(crate) warning_confirm_selected: bool,
    pub(crate) platform_artifact_statuses: Vec<ReleaseNowPlatformArtifactStatus>,
    pub(crate) artifact_strategy: ReleaseNowArtifactStrategy,
    pub(crate) artifact_reuse_by_label: std::collections::HashMap<String, bool>,
    pub(crate) artifacts_choice_selected: usize,
    pub(crate) customize_selected_platform: usize,
    pub(crate) scroll: u16,
    pub(crate) body_viewport_height: u16,
    pub(crate) body_viewport_width: u16,
    /// Rendered line count for release-notes markdown preview (updated each frame).
    pub(crate) release_notes_display_line_count: usize,
    pub(crate) selection_anchor: Option<usize>,
    pub(crate) selection_focus: Option<usize>,
    pub(crate) summary: Option<String>,
    pub(crate) summary_is_warning: bool,
    pub(crate) summary_is_error: bool,
    pub(crate) artifact_files: Vec<String>,
    pub(crate) log_lines: Vec<String>,
    pub(crate) quick_downloads: ReleaseNowQuickDownloadsSettings,
    pub(crate) readme_injection_enabled: bool,
    pub(crate) readme_inject_only_top_picks: bool,
    pub(crate) readme_inject_depth: crate::config::ReadmeInjectDepth,
    pub(crate) readme_inject_at_row: u16,
    pub(crate) release_title_template: String,
    pub(crate) integration_mode: crate::config::IntegrationMode,
    pub(crate) mirror_sync_report: Option<crate::git::MirrorSyncReport>,
    pub(crate) mirror_sync_running: bool,
    pub(crate) mirror_sync_log_lines: Vec<String>,
    pub(crate) started_at: Option<Instant>,
    /// Elapsed time frozen when the run stops (success, failure, or cancel).
    pub(crate) frozen_elapsed: Option<Duration>,
}

impl ReleaseNowDialog {
    pub(crate) fn from_validation(validation: ReleaseNowValidation) -> Self {
        let mut dialog = Self {
            project_name: validation.project_name,
            integration_mode: validation.integration_mode,
            scope_label: validation.scope_label,
            scope: validation.scope,
            changelog_enabled: validation.changelog_enabled,
            mirror_summary_to_root_changelog: validation.mirror_summary_to_root_changelog,
            repo_root: validation.repo_root,
            tag_name: validation.tag_name,
            options: validation.options,
            selected_option: 0,
            attach_changelog: true,
            release_notes_markdown: validation.release_notes_markdown,
            release_notes_placeholder: "Edit release notes in Markdown before publishing."
                .to_string(),
            warning_message: validation.warning_message,
            mode: ReleaseNowMode::Configure,
            running: false,
            auto_follow: false,
            cancel_requested: false,
            warning_confirm_selected: false,
            platform_artifact_statuses: Vec::new(),
            artifact_strategy: ReleaseNowArtifactStrategy::Pending,
            artifact_reuse_by_label: std::collections::HashMap::new(),
            artifacts_choice_selected: 0,
            customize_selected_platform: 0,
            scroll: 0,
            body_viewport_height: 0,
            body_viewport_width: 0,
            release_notes_display_line_count: 0,
            selection_anchor: None,
            selection_focus: None,
            summary: None,
            summary_is_warning: false,
            summary_is_error: false,
            artifact_files: Vec::new(),
            log_lines: Vec::new(),
            quick_downloads: validation.quick_downloads,
            readme_injection_enabled: validation.readme_injection_enabled,
            readme_inject_only_top_picks: validation.readme_inject_only_top_picks,
            readme_inject_depth: validation.readme_inject_depth,
            readme_inject_at_row: validation.readme_inject_at_row,
            release_title_template: validation.release_title_template,
            mirror_sync_report: validation
                .mirror_sync_report
                .map(|report| (*report).clone()),
            mirror_sync_running: false,
            mirror_sync_log_lines: Vec::new(),
            started_at: None,
            frozen_elapsed: None,
        };

        dialog.apply_post_validation_preflight();
        dialog
    }

    fn apply_post_validation_preflight(&mut self) {
        if self.needs_mirror_sync_prompt() {
            self.mode = ReleaseNowMode::MirrorSync;
            self.scroll = 0;
            return;
        }
        if self.warning_message.is_some() {
            self.mode = ReleaseNowMode::BumpWarning;
            self.scroll = 0;
            return;
        }
        self.refresh_artifact_preflight();
        if self.should_prompt_for_existing_artifacts() {
            self.mode = ReleaseNowMode::ExistingArtifacts;
            self.init_per_platform_defaults();
        } else {
            self.mode = ReleaseNowMode::Configure;
        }
        self.scroll = 0;
    }

    pub(crate) fn needs_mirror_sync_prompt(&self) -> bool {
        self.mirror_sync_report
            .as_ref()
            .is_some_and(|report| !report.in_sync())
    }

    pub(crate) fn is_mirror_sync_mode(&self) -> bool {
        self.mode == ReleaseNowMode::MirrorSync
    }

    pub(crate) fn begin_mirror_sync(&mut self) {
        self.mirror_sync_running = true;
        self.mirror_sync_log_lines.clear();
        self.scroll = 0;
    }

    pub(crate) fn apply_mirror_sync_result(
        &mut self,
        report: crate::git::MirrorSyncReport,
        log_lines: Vec<String>,
    ) {
        self.mirror_sync_running = false;
        self.mirror_sync_report = Some(report.clone());
        self.mirror_sync_log_lines.extend(log_lines);
        if report.in_sync() {
            self.proceed_past_mirror_sync();
        }
        self.scroll = 0;
    }

    pub(crate) fn apply_mirror_sync_failure(&mut self, error_message: String) {
        self.mirror_sync_running = false;
        self.mirror_sync_log_lines
            .push(format!("[Mirror sync][error] {error_message}"));
        self.scroll = 0;
    }

    pub(crate) fn proceed_past_mirror_sync(&mut self) {
        if self.needs_mirror_sync_prompt() {
            return;
        }
        if self.warning_message.is_some() {
            self.mode = ReleaseNowMode::BumpWarning;
        } else {
            self.refresh_artifact_preflight();
            if self.should_prompt_for_existing_artifacts() {
                self.mode = ReleaseNowMode::ExistingArtifacts;
                self.artifacts_choice_selected = 0;
                self.init_per_platform_defaults();
            } else {
                self.mode = ReleaseNowMode::Configure;
            }
        }
        self.scroll = 0;
    }

    pub(crate) fn is_warning_mode(&self) -> bool {
        self.mode == ReleaseNowMode::BumpWarning
    }

    pub(crate) fn is_existing_artifacts_mode(&self) -> bool {
        matches!(
            self.mode,
            ReleaseNowMode::ExistingArtifacts | ReleaseNowMode::ArtifactsCustomize
        )
    }

    pub(crate) fn is_artifacts_customize_mode(&self) -> bool {
        self.mode == ReleaseNowMode::ArtifactsCustomize
    }

    pub(crate) fn should_prompt_for_existing_artifacts(&self) -> bool {
        self.platform_artifact_statuses
            .iter()
            .any(|status| status.ready)
    }

    pub(crate) fn refresh_artifact_preflight(&mut self) {
        self.platform_artifact_statuses = scan_artifacts_for_release_version(
            &self.repo_root,
            &self.tag_name,
            self.selected_option(),
        );
    }

    fn init_per_platform_defaults(&mut self) {
        self.artifact_reuse_by_label = self
            .platform_artifact_statuses
            .iter()
            .map(|status| (status.label.clone(), status.ready))
            .collect();
    }

    pub(crate) fn proceed_past_warning(&mut self) {
        self.refresh_artifact_preflight();
        if self.should_prompt_for_existing_artifacts() {
            self.mode = ReleaseNowMode::ExistingArtifacts;
            self.artifacts_choice_selected = 0;
            self.init_per_platform_defaults();
        } else {
            self.mode = ReleaseNowMode::Configure;
        }
        self.warning_confirm_selected = false;
        self.scroll = 0;
    }

    pub(crate) fn cycle_artifacts_choice(&mut self, delta: isize) {
        const CHOICES: usize = 4;
        self.artifacts_choice_selected =
            (self.artifacts_choice_selected as isize + delta).rem_euclid(CHOICES as isize) as usize;
    }

    pub(crate) fn confirm_existing_artifacts_choice(&mut self) {
        match self.artifacts_choice_selected {
            0 => {
                self.artifact_strategy = ReleaseNowArtifactStrategy::ReuseAll;
                self.mode = ReleaseNowMode::Configure;
            }
            1 => {
                self.artifact_strategy = ReleaseNowArtifactStrategy::RebuildAll;
                self.mode = ReleaseNowMode::Configure;
            }
            2 => {
                self.artifact_strategy = ReleaseNowArtifactStrategy::PerPlatform;
                self.mode = ReleaseNowMode::ArtifactsCustomize;
                self.customize_selected_platform = self
                    .customizable_platform_indices()
                    .first()
                    .copied()
                    .unwrap_or(0);
            }
            _ => {}
        }
        self.scroll = 0;
    }

    pub(crate) fn confirm_artifacts_customize(&mut self) {
        self.mode = ReleaseNowMode::Configure;
        self.scroll = 0;
    }

    pub(crate) fn cycle_customize_platform(&mut self, delta: isize) {
        let indices = self.customizable_platform_indices();
        if indices.is_empty() {
            self.customize_selected_platform = 0;
            return;
        }
        let current = indices
            .iter()
            .position(|index| *index == self.customize_selected_platform)
            .unwrap_or(0);
        let next = (current as isize + delta).rem_euclid(indices.len() as isize) as usize;
        self.customize_selected_platform = indices[next];
    }

    pub(crate) fn toggle_customize_platform_reuse(&mut self) {
        let Some(status) = self
            .platform_artifact_statuses
            .get(self.customize_selected_platform)
        else {
            return;
        };
        if !status.ready {
            return;
        }
        let entry = self
            .artifact_reuse_by_label
            .entry(status.label.clone())
            .or_insert(true);
        *entry = !*entry;
    }

    fn customizable_platform_indices(&self) -> Vec<usize> {
        self.platform_artifact_statuses
            .iter()
            .enumerate()
            .filter(|(_, status)| !status.script.artifact_dirs.is_empty())
            .map(|(index, _)| index)
            .collect()
    }

    pub(crate) fn scripts_to_run(&self) -> Vec<ReleaseNowScript> {
        let option = self.selected_option();
        match self.artifact_strategy {
            ReleaseNowArtifactStrategy::Pending | ReleaseNowArtifactStrategy::RebuildAll => {
                option.scripts.clone()
            }
            ReleaseNowArtifactStrategy::ReuseAll => option
                .scripts
                .iter()
                .filter(|script| {
                    !self
                        .platform_artifact_statuses
                        .iter()
                        .find(|status| status.label == script.label)
                        .is_some_and(|status| status.ready)
                })
                .cloned()
                .collect(),
            ReleaseNowArtifactStrategy::PerPlatform => option
                .scripts
                .iter()
                .filter(|script| {
                    !self
                        .artifact_reuse_by_label
                        .get(&script.label)
                        .copied()
                        .unwrap_or(false)
                })
                .cloned()
                .collect(),
        }
    }

    pub(crate) fn artifact_reuse_summary(&self) -> Option<String> {
        if self.artifact_strategy == ReleaseNowArtifactStrategy::Pending {
            return None;
        }
        let total = self.selected_option().scripts.len();
        let skipped = total.saturating_sub(self.scripts_to_run().len());
        if skipped == 0 {
            return None;
        }
        Some(format!(
            "Reusing existing dist/latest artifacts for {skipped} of {total} configured build(s)."
        ))
    }

    pub(crate) fn is_completed(&self) -> bool {
        self.mode == ReleaseNowMode::Completed
    }

    pub(crate) fn is_running(&self) -> bool {
        self.running
    }

    pub(crate) fn auto_follow(&self) -> bool {
        self.auto_follow
    }

    pub(crate) fn cancel_requested(&self) -> bool {
        self.cancel_requested
    }

    pub(crate) fn set_body_viewport(&mut self, height: u16, width: u16) {
        self.body_viewport_height = height;
        self.body_viewport_width = width;
        if self.running && self.auto_follow {
            self.scroll_to_tail();
        }
    }

    pub(crate) fn is_release_notes_preview(&self) -> bool {
        matches!(self.mode, ReleaseNowMode::Configure) && !self.running && self.attach_changelog
    }

    pub(crate) fn release_notes_layout_width(&self) -> u16 {
        self.body_viewport_width.saturating_sub(2).max(20)
    }

    pub(crate) fn clear_body_selection(&mut self) {
        self.selection_anchor = None;
        self.selection_focus = None;
    }

    fn release_notes_rendered_lines_fallback(&self) -> Vec<Line<'static>> {
        crate::tui::render_markdown(
            &self.release_notes_markdown,
            self.release_notes_layout_width(),
        )
        .lines
    }

    fn release_notes_plain_lines_from_view(view: &crate::tui::MarkdownView) -> Vec<String> {
        view.render()
            .lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect()
    }

    pub(crate) fn selected_option(&self) -> &ReleaseNowRunOption {
        &self.options[self
            .selected_option
            .min(self.options.len().saturating_sub(1))]
    }

    pub(crate) fn cycle_option(&mut self, delta: isize) {
        if self.options.is_empty() {
            self.selected_option = 0;
            return;
        }

        let len = self.options.len() as isize;
        self.selected_option = (self.selected_option as isize + delta).rem_euclid(len) as usize;
        if !self.running
            && matches!(
                self.mode,
                ReleaseNowMode::Configure
                    | ReleaseNowMode::ExistingArtifacts
                    | ReleaseNowMode::ArtifactsCustomize
            )
        {
            self.artifact_strategy = ReleaseNowArtifactStrategy::Pending;
            self.refresh_artifact_preflight();
            if self.should_prompt_for_existing_artifacts() {
                self.mode = ReleaseNowMode::ExistingArtifacts;
                self.artifacts_choice_selected = 0;
                self.init_per_platform_defaults();
            } else {
                self.mode = ReleaseNowMode::Configure;
            }
        }
    }

    pub(crate) fn toggle_attach_changelog(&mut self) {
        self.attach_changelog = !self.attach_changelog;
    }

    pub(crate) fn toggle_warning_selection(&mut self) {
        self.warning_confirm_selected = !self.warning_confirm_selected;
    }

    pub(crate) fn scroll_by(&mut self, delta: i16) {
        if self.running && self.auto_follow && delta != 0 {
            self.scroll_to_tail();
            self.auto_follow = false;
        }
        self.scroll = self
            .scroll
            .saturating_add_signed(delta)
            .min(self.max_scroll_offset());
    }

    pub(crate) fn back_from_artifacts_customize(&mut self) {
        self.mode = ReleaseNowMode::ExistingArtifacts;
        self.artifact_strategy = ReleaseNowArtifactStrategy::Pending;
        self.scroll = 0;
    }

    pub(crate) fn begin_running(&mut self) {
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
        if let Some(summary) = self.artifact_reuse_summary() {
            self.log_lines.push(summary);
        }
        self.scroll = 0;
    }

    pub(crate) fn toggle_auto_follow(&mut self) -> bool {
        self.auto_follow = !self.auto_follow;
        if self.auto_follow {
            self.scroll_to_tail();
        }
        self.auto_follow
    }

    pub(crate) fn mark_cancel_requested(&mut self) {
        if self.cancel_requested {
            return;
        }

        self.cancel_requested = true;
        self.append_log_lines(vec![
            "Cancellation requested. Waiting for the running command to stop...".to_string(),
        ]);
    }

    pub(crate) fn append_log_lines(&mut self, lines: Vec<String>) {
        if lines.is_empty() {
            return;
        }

        self.log_lines.extend(lines);
        if self.running && self.auto_follow {
            self.scroll_to_tail();
        }
    }

    pub(crate) fn apply_outcome(&mut self, outcome: ReleaseNowExecutionOutcome) {
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

    pub(crate) fn apply_cancelled(&mut self, message: String) {
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

    pub(crate) fn apply_failure(&mut self, error_message: String) {
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

    pub(crate) fn elapsed_label(&self) -> String {
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

    pub(crate) fn scroll_to_tail(&mut self) {
        self.scroll = self.tail_scroll_offset();
    }

    pub(crate) fn scroll_to_start(&mut self) {
        self.scroll = 0;
    }

    pub(crate) fn scroll_offset(&self) -> u16 {
        self.scroll.min(self.max_scroll_offset())
    }

    fn max_scroll_offset(&self) -> u16 {
        let line_count = if self.is_release_notes_preview() {
            self.release_notes_display_line_count
        } else {
            self.body_plain_lines(None).len()
        };
        line_count
            .saturating_sub(self.body_viewport_height.max(1) as usize)
            .min(u16::MAX as usize) as u16
    }

    fn tail_scroll_offset(&self) -> u16 {
        self.max_scroll_offset()
    }

    pub(crate) fn begin_body_selection(
        &mut self,
        row_offset: u16,
        notes_view: Option<&crate::tui::MarkdownView>,
    ) -> bool {
        let Some(index) = self.body_line_index_for_row(row_offset, notes_view) else {
            return false;
        };

        if self.running {
            self.auto_follow = false;
        }
        self.selection_anchor = Some(index);
        self.selection_focus = Some(index);
        true
    }

    pub(crate) fn update_body_selection(
        &mut self,
        row_offset: u16,
        notes_view: Option<&crate::tui::MarkdownView>,
    ) -> bool {
        let Some(anchor) = self.selection_anchor else {
            return false;
        };
        let Some(index) = self.body_line_index_for_row(row_offset, notes_view) else {
            return false;
        };

        self.selection_anchor = Some(anchor);
        self.selection_focus = Some(index);
        true
    }

    pub(crate) fn has_body_selection(&self) -> bool {
        self.selection_anchor.is_some() && self.selection_focus.is_some()
    }

    pub(crate) fn selected_body_text(
        &self,
        notes_view: Option<&crate::tui::MarkdownView>,
    ) -> Option<String> {
        let (start, end) = self.selection_range()?;
        let lines = self.body_plain_lines(notes_view);
        Some(lines[start..=end].join("\n"))
    }

    fn selection_range(&self) -> Option<(usize, usize)> {
        let start = self.selection_anchor?;
        let end = self.selection_focus?;
        Some((start.min(end), start.max(end)))
    }

    fn body_line_index_for_row(
        &self,
        row_offset: u16,
        notes_view: Option<&crate::tui::MarkdownView>,
    ) -> Option<usize> {
        let lines = self.body_plain_lines(notes_view);
        let count = lines.len();
        if count == 0 {
            return None;
        }

        if self.is_release_notes_preview() {
            let scroll_offset = self.scroll_offset() as usize;
            let index = scroll_offset + row_offset as usize;
            return Some(index.min(count.saturating_sub(1)));
        }

        let content_width = self.body_viewport_width.max(1) as usize;
        let scroll_offset = self.scroll_offset() as usize;
        let target_row = row_offset as usize;

        // Account for line wrapping: calculate which logical line contains the target row
        let mut terminal_rows_consumed = 0usize;
        for (i, line) in lines.iter().enumerate().skip(scroll_offset) {
            let char_count = line.chars().count();
            let rows_for_line = char_count.div_ceil(content_width).max(1);
            if terminal_rows_consumed + rows_for_line > target_row {
                return Some(i);
            }
            terminal_rows_consumed += rows_for_line;
        }

        // Target row is past the last line, return the last line
        Some(count.saturating_sub(1))
    }

    pub(crate) fn body_title(&self) -> &'static str {
        match self.mode {
            ReleaseNowMode::MirrorSync => " Mirror Sync ",
            ReleaseNowMode::BumpWarning => " Merge Check ",
            ReleaseNowMode::ExistingArtifacts => " Existing Artifacts ",
            ReleaseNowMode::ArtifactsCustomize => " Reuse / Rebuild ",
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

    pub(crate) fn rendered_body_lines(
        &self,
        notes_view: Option<&crate::tui::MarkdownView>,
    ) -> Vec<Line<'static>> {
        let lines = match self.mode {
            ReleaseNowMode::MirrorSync => self.mirror_sync_body_lines(),
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
            ReleaseNowMode::ExistingArtifacts => self.existing_artifacts_body_lines(),
            ReleaseNowMode::ArtifactsCustomize => self.artifacts_customize_body_lines(),
            ReleaseNowMode::Configure => {
                if self.running {
                    if self.log_lines.is_empty() {
                        vec![Line::from("Waiting for ReleaseNOW output...")]
                    } else {
                        self.log_display_lines()
                    }
                } else if self.attach_changelog {
                    notes_view
                        .map(|view| view.render().lines)
                        .unwrap_or_else(|| self.release_notes_rendered_lines_fallback())
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

    fn body_plain_lines(&self, notes_view: Option<&crate::tui::MarkdownView>) -> Vec<String> {
        match self.mode {
            ReleaseNowMode::MirrorSync => self.mirror_sync_plain_lines(),
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
            ReleaseNowMode::ExistingArtifacts => self.existing_artifacts_plain_lines(),
            ReleaseNowMode::ArtifactsCustomize => self.artifacts_customize_plain_lines(),
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
                    notes_view
                        .map(Self::release_notes_plain_lines_from_view)
                        .unwrap_or_else(|| {
                            self.release_notes_rendered_lines_fallback()
                                .iter()
                                .map(|line| {
                                    line.spans
                                        .iter()
                                        .map(|span| span.content.as_ref())
                                        .collect::<String>()
                                })
                                .collect()
                        })
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

    fn mirror_sync_body_lines(&self) -> Vec<Line<'static>> {
        self.mirror_sync_plain_lines()
            .into_iter()
            .map(Line::from)
            .collect()
    }

    fn mirror_sync_plain_lines(&self) -> Vec<String> {
        let mut lines = vec![
            "GitLab+GitHub projects require both remotes to track the same commit before ReleaseNOW can publish."
                .to_string(),
            String::new(),
        ];
        if let Some(report) = &self.mirror_sync_report {
            lines.extend(report.summary_lines());
        } else {
            lines.push("Mirror sync status is unavailable.".to_string());
        }
        if self.mirror_sync_running {
            lines.push(String::new());
            lines.push("Sync in progress...".to_string());
        }
        if !self.mirror_sync_log_lines.is_empty() {
            lines.push(String::new());
            lines.extend(self.mirror_sync_log_lines.clone());
        }
        lines
    }

    fn existing_artifacts_body_lines(&self) -> Vec<Line<'static>> {
        self.existing_artifacts_plain_lines()
            .into_iter()
            .map(Line::from)
            .collect()
    }

    fn existing_artifacts_plain_lines(&self) -> Vec<String> {
        let version = release_version_from_tag(&self.tag_name);
        let mut lines = vec![
            format!("dist/latest already contains version {version} artifacts for this release."),
            String::new(),
            "Choose whether to reuse the existing builds or run the configured scripts again."
                .to_string(),
            String::new(),
        ];
        for status in &self.platform_artifact_statuses {
            if status.script.artifact_dirs.is_empty() {
                continue;
            }
            let state = if status.ready {
                format!("ready ({} file(s))", status.existing_files.len())
            } else {
                "missing".to_string()
            };
            lines.push(format!("- {}: {state}", status.label));
        }
        lines
    }

    fn artifacts_customize_body_lines(&self) -> Vec<Line<'static>> {
        self.artifacts_customize_plain_lines()
            .into_iter()
            .map(|line| {
                if line.starts_with('>') {
                    Line::from(line).style(
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    )
                } else {
                    Line::from(line)
                }
            })
            .collect()
    }

    fn artifacts_customize_plain_lines(&self) -> Vec<String> {
        let mut lines = vec![
            "Toggle each ready platform between Reuse and Rebuild.".to_string(),
            "Platforms without matching artifacts must be rebuilt.".to_string(),
            String::new(),
        ];
        for (index, status) in self.platform_artifact_statuses.iter().enumerate() {
            if status.script.artifact_dirs.is_empty() {
                continue;
            }
            let reuse = self
                .artifact_reuse_by_label
                .get(&status.label)
                .copied()
                .unwrap_or(false);
            let action = if !status.ready {
                "Rebuild (required)"
            } else if reuse {
                "Reuse existing"
            } else {
                "Rebuild"
            };
            let prefix = if index == self.customize_selected_platform {
                ">"
            } else {
                " "
            };
            lines.push(format!(
                "{prefix} {}: {action} — {} file(s)",
                status.label,
                status.existing_files.len()
            ));
        }
        lines.push(String::new());
        lines.push("Space toggles Reuse/Rebuild. Enter continues to ReleaseNOW.".to_string());
        lines
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
pub(crate) struct ReleaseNowValidation {
    pub(crate) project_name: String,
    pub(crate) integration_mode: crate::config::IntegrationMode,
    pub(crate) scope_label: String,
    pub(crate) scope: GitScopeContext,
    pub(crate) changelog_enabled: bool,
    pub(crate) mirror_summary_to_root_changelog: bool,
    pub(crate) repo_root: String,
    pub(crate) tag_name: String,
    pub(crate) options: Vec<ReleaseNowRunOption>,
    pub(crate) warning_message: Option<String>,
    pub(crate) release_notes_markdown: String,
    pub(crate) quick_downloads: ReleaseNowQuickDownloadsSettings,
    pub(crate) readme_injection_enabled: bool,
    pub(crate) readme_inject_only_top_picks: bool,
    pub(crate) readme_inject_depth: crate::config::ReadmeInjectDepth,
    pub(crate) readme_inject_at_row: u16,
    pub(crate) release_title_template: String,
    pub(crate) mirror_sync_report: Option<Box<crate::git::MirrorSyncReport>>,
}

#[derive(Clone)]
pub(crate) struct ReleaseNowRunOption {
    pub(crate) label: String,
    pub(crate) scripts: Vec<ReleaseNowScript>,
    pub(crate) artifact_dirs: Vec<String>,
}

#[derive(Clone)]
pub(crate) struct ReleaseNowScript {
    pub(crate) label: String,
    pub(crate) script_path: String,
    pub(crate) artifact_dirs: Vec<String>,
}

#[derive(Clone)]
pub(crate) struct ReleaseNowExecutionRequest {
    pub(crate) scope_label: String,
    pub(crate) integration_mode: crate::config::IntegrationMode,
    pub(crate) scope: GitScopeContext,
    pub(crate) changelog_enabled: bool,
    pub(crate) mirror_summary_to_root_changelog: bool,
    pub(crate) repo_root: String,
    pub(crate) tag_name: String,
    pub(crate) release_title: String,
    pub(crate) selected_option_label: String,
    pub(crate) scripts: Vec<ReleaseNowScript>,
    pub(crate) artifact_dirs: Vec<String>,
    pub(crate) release_notes_markdown: Option<String>,
    pub(crate) quick_downloads: ReleaseNowQuickDownloadsSettings,
    pub(crate) readme_injection_enabled: bool,
    pub(crate) readme_inject_only_top_picks: bool,
    pub(crate) readme_inject_depth: crate::config::ReadmeInjectDepth,
    pub(crate) readme_inject_at_row: u16,
}

#[derive(Clone)]
pub(crate) struct ReleaseNowExecutionOutcome {
    pub(crate) summary: String,
    pub(crate) artifact_files: Vec<String>,
    pub(crate) log_lines: Vec<String>,
}

pub(crate) struct ReleaseNowMirrorSyncResult {
    pub(crate) report: crate::git::MirrorSyncReport,
    pub(crate) log_lines: Vec<String>,
}

pub(crate) fn run_mirror_sync_operation(
    repo_root: &str,
    gitlab_remote: Option<&str>,
    github_remote: Option<&str>,
    push: bool,
) -> Result<ReleaseNowMirrorSyncResult> {
    let mut log_lines = Vec::new();
    if push {
        log_lines = crate::git::push_mirror_sync(repo_root, gitlab_remote, github_remote)?;
    }
    let report = crate::git::check_mirror_sync(repo_root, gitlab_remote, github_remote)?;
    Ok(ReleaseNowMirrorSyncResult { report, log_lines })
}

pub(crate) fn validate_release_now(
    project: &ProjectConfig,
    scope_index: usize,
    cancel: Option<GitCancellation>,
) -> Result<ReleaseNowValidation> {
    if !project.integration_mode.is_forge_enabled() {
        bail!("ReleaseNOW requires a GitHub- or GitLab-enabled project with a configured remote")
    }

    crate::forge::ensure_forge_authenticated(project.integration_mode)?;

    let contexts = collect_all_branch_git_scope_contexts(project)?;
    if contexts.is_empty() {
        bail!("ReleaseNOW requires at least one git-backed scope")
    }

    let scope_index = scope_index.min(contexts.len().saturating_sub(1));
    let scope = contexts[scope_index].clone();

    let mirror_sync_report = if project.integration_mode.is_dual_forge() {
        Some(Box::new(
            crate::git::check_mirror_sync(
                &scope.repo_root,
                scope.remote_spec.as_deref(),
                scope.secondary_remote_spec.as_deref(),
            )
            .with_context(|| {
                format!(
                    "ReleaseNOW pre-flight mirror sync check failed for {}",
                    scope.display_name
                )
            })?,
        ))
    } else {
        None
    };

    let options = collect_release_now_options(project.release_now_for_scope(scope_index))?;
    let warning_message =
        build_recent_merge_warning(project, &contexts, scope_index, cancel.clone())?;
    let release_notes_markdown = build_release_notes_markdown(&scope.suggested_tag_name, &scope)
        .unwrap_or_else(|_| DEFAULT_RELEASE_NOTES.to_string());

    Ok(ReleaseNowValidation {
        project_name: project.name.clone(),
        integration_mode: project.integration_mode,
        scope_label: scope
            .scope_kind
            .map(|kind| format!("{} ({})", scope.display_name, kind.display_name()))
            .unwrap_or_else(|| scope.display_name.clone()),
        scope: scope.clone(),
        changelog_enabled: project.changelog_enabled_for_scope(scope_index),
        mirror_summary_to_root_changelog: project
            .changelog_mirror_summary_to_root_changelog_for_scope(scope_index),
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
        readme_inject_only_top_picks: project
            .release_now_for_scope(scope_index)
            .readme_inject_only_top_picks,
        readme_inject_depth: project
            .release_now_for_scope(scope_index)
            .readme_inject_depth,
        readme_inject_at_row: project
            .release_now_for_scope(scope_index)
            .readme_inject_at_row,
        release_title_template: project
            .release_now_for_scope(scope_index)
            .release_title_template
            .clone(),
        mirror_sync_report,
    })
}

pub(crate) fn build_execution_request(dialog: &ReleaseNowDialog) -> ReleaseNowExecutionRequest {
    ReleaseNowExecutionRequest {
        scope_label: dialog.scope_label.clone(),
        integration_mode: dialog.integration_mode,
        scope: dialog.scope.clone(),
        changelog_enabled: dialog.changelog_enabled,
        mirror_summary_to_root_changelog: dialog.mirror_summary_to_root_changelog,
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
        scripts: dialog.scripts_to_run(),
        artifact_dirs: dialog.selected_option().artifact_dirs.clone(),
        release_notes_markdown: dialog
            .attach_changelog
            .then(|| dialog.release_notes_markdown.trim().to_string())
            .filter(|notes| !notes.is_empty()),
        quick_downloads: dialog.quick_downloads.clone(),
        readme_injection_enabled: dialog.readme_injection_enabled,
        readme_inject_only_top_picks: dialog.readme_inject_only_top_picks,
        readme_inject_depth: dialog.readme_inject_depth,
        readme_inject_at_row: dialog.readme_inject_at_row,
    }
}

pub(crate) async fn execute_release_now_async(
    request: ReleaseNowExecutionRequest,
    cancel: GitCancellation,
    mut emit_progress: impl FnMut(Vec<String>) + Send,
) -> Result<ReleaseNowExecutionOutcome> {
    let forge = resolve_forge_for_release_request(&request)?;
    forge.ensure_authenticated()?;
    ensure_not_cancelled(&cancel)?;

    emit_progress(vec![format!(
        "Starting ReleaseNOW for {} using {}.",
        request.scope_label, request.selected_option_label
    )]);

    if request.scripts.is_empty() && !request.artifact_dirs.is_empty() {
        emit_progress(vec![
            "Skipping configured build scripts; reusing existing dist/latest artifacts."
                .to_string(),
        ]);
    }

    if request.readme_injection_enabled {
        prepush_auto_injected_readme_async(&request, &cancel, &mut emit_progress).await?;
    }

    let (mac_ci, local_scripts) =
        crate::workflow::rls_now_mac::partition_mac_scripts(&request.scripts, &request.tag_name);
    let mut mac_ci_warning: Option<String> = None;

    if let Some(mut mac_config) = mac_ci {
        mac_config.github_repo = Some(crate::forge::resolve_github_repo_slug_for_actions(
            &request.repo_root,
            request.scope.secondary_remote_spec.as_deref(),
        )?);
        let repo_root = request.repo_root.clone();
        let (session, trigger_lines) = crate::workflow::rls_now_mac::trigger_mac_ci_session(
            repo_root.clone(),
            mac_config.clone(),
        )
        .await?;
        emit_progress(trigger_lines);

        if request.selected_option_label == "All configured" {
            crate::workflow::rls_now_mac::stream_mac_ci_until_build_started(
                repo_root.clone(),
                session.clone(),
                cancel.clone(),
                &mut emit_progress,
            )
            .await?;

            for script in &local_scripts {
                ensure_not_cancelled(&cancel)?;
                run_script_with_live_logs(
                    &request.repo_root,
                    script,
                    cancel.clone(),
                    &mut emit_progress,
                )
                .await?;
            }

            match crate::workflow::rls_now_mac::finish_mac_ci_and_merge_artifacts(
                repo_root,
                session,
                mac_config.version,
                cancel.clone(),
                &mut emit_progress,
            )
            .await?
            {
                crate::workflow::rls_now_mac::MacCiFinishOutcome::Success => {}
                crate::workflow::rls_now_mac::MacCiFinishOutcome::Failed { warning } => {
                    mac_ci_warning = Some(warning);
                }
            }
        } else {
            match crate::workflow::rls_now_mac::finish_mac_ci_and_merge_artifacts(
                repo_root,
                session,
                mac_config.version,
                cancel.clone(),
                &mut emit_progress,
            )
            .await?
            {
                crate::workflow::rls_now_mac::MacCiFinishOutcome::Success => {}
                crate::workflow::rls_now_mac::MacCiFinishOutcome::Failed { warning } => {
                    bail!(warning);
                }
            }
        }
    } else {
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
    }

    if let Some(warning) = mac_ci_warning {
        emit_progress(vec![
            "[MacOS][warning] macOS build failed.".to_string(),
            format!("[MacOS][warning] {warning}"),
        ]);
    }

    ensure_not_cancelled(&cancel)?;
    let artifact_files = if request.artifact_dirs.is_empty() {
        emit_progress(vec![
            "No artifact directories configured; skipping artifact scan (source-only release)."
                .to_string(),
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

        if request.mirror_summary_to_root_changelog {
            let repo_root_for_mirror = request.repo_root.clone();
            let mirrored =
                run_blocking_job(move || mirror_summary_changelog_to_root(&repo_root_for_mirror))
                    .await?;
            if mirrored {
                emit_progress(vec![
                    "Mirrored .changelogs/README.md into repo_root/CHANGELOG.md.".to_string(),
                ]);
            }
        }
    }

    // QD HTML is built from the same artifact list attached to this release (see rls_now_qd).
    let mut qd_warnings = Vec::new();
    let historical_qd_artifacts =
        historical_release_now_artifacts_for_tag(&request.repo_root, &request.tag_name)?;
    let qd_artifacts =
        rls_now_qd::merge_artifacts_for_quick_downloads(&artifact_files, &historical_qd_artifacts);
    let release_notes_for_github = rls_now_qd::finalize_release_notes_with_quick_downloads(
        request.release_notes_markdown.clone(),
        request.scope.remote_spec.as_deref(),
        &request.tag_name,
        &qd_artifacts,
        &request.quick_downloads,
        &mut qd_warnings,
    );
    for warning in qd_warnings {
        emit_progress(vec![format!("Warning: {}", warning)]);
    }

    if request.integration_mode.is_dual_forge() {
        let primary_url = request
            .scope
            .remote_spec
            .as_deref()
            .ok_or_else(|| anyhow!("GitLab+GitHub project is missing the GitLab remote URL"))?;
        let secondary_url = request
            .scope
            .secondary_remote_spec
            .as_deref()
            .ok_or_else(|| anyhow!("GitLab+GitHub project is missing the GitHub remote URL"))?;

        let mut gitlab_warnings = Vec::new();
        let release_notes_for_gitlab = rls_now_qd::finalize_release_notes_with_quick_downloads(
            request.release_notes_markdown.clone(),
            Some(primary_url),
            &request.tag_name,
            &qd_artifacts,
            &request.quick_downloads,
            &mut gitlab_warnings,
        );
        for warning in gitlab_warnings {
            emit_progress(vec![format!("Warning: {}", warning)]);
        }

        let mut github_warnings = Vec::new();
        let release_notes_for_github_secondary =
            rls_now_qd::finalize_release_notes_with_quick_downloads(
                request.release_notes_markdown.clone(),
                Some(secondary_url),
                &request.tag_name,
                &qd_artifacts,
                &request.quick_downloads,
                &mut github_warnings,
            );
        for warning in github_warnings {
            emit_progress(vec![format!("Warning: {}", warning)]);
        }

        let primary_forge = request
            .integration_mode
            .forge_kind()
            .ok_or_else(|| anyhow!("GitLab+GitHub project is missing a primary forge"))?;
        let secondary_forge = request
            .integration_mode
            .secondary_forge_kind()
            .ok_or_else(|| anyhow!("GitLab+GitHub project is missing a secondary forge"))?;

        create_or_update_forge_release(
            primary_forge,
            &request.repo_root,
            &request.tag_name,
            Some(primary_url),
            &request.release_title,
            release_notes_for_gitlab.as_deref(),
            &artifact_files,
            cancel.clone(),
            &mut emit_progress,
        )
        .await?;

        create_or_update_forge_release(
            secondary_forge,
            &request.repo_root,
            &request.tag_name,
            Some(secondary_url),
            &request.release_title,
            release_notes_for_github_secondary.as_deref(),
            &artifact_files,
            cancel.clone(),
            &mut emit_progress,
        )
        .await?;
    } else {
        create_or_update_forge_release(
            forge,
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
    }

    if request.changelog_enabled || request.readme_injection_enabled {
        ensure_not_cancelled(&cancel)?;
        let repo_root_for_commit = request.repo_root.clone();
        let tag_name_for_commit = request.tag_name.clone();
        let artifact_files_for_commit = artifact_files.clone();
        let generated_commit = run_blocking_job(move || {
            create_release_now_generated_files_commit(
                &repo_root_for_commit,
                &tag_name_for_commit,
                &artifact_files_for_commit,
                request.mirror_summary_to_root_changelog,
            )
        })
        .await?;

        if let Some(generated_commit) = generated_commit {
            let remote_name = resolve_release_push_remote(
                &request.repo_root,
                request.scope.remote_spec.as_deref(),
            )?;
            let repo_root_for_branch = request.repo_root.clone();
            let cancel_for_branch = cancel.clone();
            let branch_name = run_blocking_job(move || {
                current_branch_with_cancel(&repo_root_for_branch, Some(cancel_for_branch))
            })
            .await?;

            emit_progress(vec![format!(
                "Pushing generated ReleaseNOW files to {}.",
                remote_name
            )]);
            if let Err(push_error) = run_command_with_retry_async(
                request.repo_root.clone(),
                "git",
                vec!["push".to_string(), remote_name.clone(), branch_name],
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

    if let Err(error) = clear_top_picks_edits(&request.repo_root) {
        let warning = format!(
            "Release completed, but failed to clear saved Top Picks edits: {}",
            error
        );
        emit_progress(vec![format!("Warning: {}", warning)]);
        release_notes.push(warning);
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

pub(crate) fn is_cancelled_error(message: &str) -> bool {
    message.contains("cancelled by user")
}

pub(crate) fn format_user_facing_error(message: &str) -> String {
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
    if normalized.contains("glab release") || normalized.contains("gitlab release") {
        return build_guided_error(
            "ReleaseNOW could not create the GitLab release.",
            "Check that GitLab CLI is authenticated and that the repository, tag, and release permissions are valid, then retry.",
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

fn repo_selector_from_cli_repo_args(repo_args: &[String]) -> Result<String> {
    repo_args
        .windows(2)
        .find(|window| window[0] == "-R")
        .map(|window| window[1].clone())
        .ok_or_else(|| {
            anyhow!("ReleaseNOW could not derive repository selector from forge CLI arguments")
        })
}

fn extract_relevant_error_detail(message: &str) -> Option<String> {
    let cleaned = strip_terminal_control_sequences(message);
    let detail_source = cleaned
        .split_once(": ")
        .map(|(_, rest)| rest)
        .unwrap_or(cleaned.as_str());

    let segments = detail_source
        .split(" | ")
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();

    if let Some(segment) = segments
        .iter()
        .rev()
        .find(|segment| is_error_detail_segment(segment))
    {
        return Some(truncate_error_detail(segment, 220));
    }

    segments
        .iter()
        .rev()
        .find(|segment| !is_progress_detail_segment(segment))
        .map(|segment| truncate_error_detail(segment, 220))
}

fn is_error_detail_segment(segment: &str) -> bool {
    let lower = segment.to_ascii_lowercase();
    lower.contains("error")
        || lower.contains("failed")
        || lower.contains("fatal:")
        || lower.contains("denied")
        || lower.contains("rejected")
        || lower.contains("not found")
        || lower.contains("already been taken")
        || lower.contains("validation failed")
        || lower.contains("http 4")
        || lower.contains("http 5")
}

fn is_progress_detail_segment(segment: &str) -> bool {
    let lower = segment.to_ascii_lowercase();
    lower.contains("validating tag")
        || lower.contains("creating or updating release")
        || lower.contains("uploading release assets")
        || lower.contains("uploading to release")
        || lower.contains("release updated")
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
            artifact_dirs: artifact_dirs.iter().map(|dir| (*dir).to_string()).collect(),
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

pub(crate) fn release_version_from_tag(tag_name: &str) -> String {
    tag_name
        .trim()
        .trim_start_matches(['v', 'V'])
        .trim()
        .to_string()
}

pub(crate) fn scan_artifacts_for_release_version(
    repo_root: &str,
    tag_name: &str,
    option: &ReleaseNowRunOption,
) -> Vec<ReleaseNowPlatformArtifactStatus> {
    let version = release_version_from_tag(tag_name);
    if version.is_empty() {
        return Vec::new();
    }

    option
        .scripts
        .iter()
        .map(|script| {
            let existing_files =
                discover_version_artifacts(repo_root, &script.artifact_dirs, &version);
            let ready = !script.artifact_dirs.is_empty()
                && script
                    .artifact_dirs
                    .iter()
                    .all(|dir| dir_has_version_artifact(repo_root, dir, &version));
            ReleaseNowPlatformArtifactStatus {
                label: script.label.clone(),
                script: script.clone(),
                existing_files,
                ready,
            }
        })
        .collect()
}

fn discover_version_artifacts(
    repo_root: &str,
    artifact_dirs: &[String],
    version: &str,
) -> Vec<String> {
    let mut files = Vec::new();
    for dir in artifact_dirs {
        let root = Path::new(repo_root).join("dist").join("latest").join(dir);
        if !root.exists() {
            continue;
        }
        let _ = collect_files_recursive(&root, &mut files);
    }
    files.retain(|path| file_matches_release_version(path, version));
    files.sort();
    files.dedup();
    files
}

fn dir_has_version_artifact(repo_root: &str, dir: &str, version: &str) -> bool {
    let root = Path::new(repo_root).join("dist").join("latest").join(dir);
    if !root.is_dir() {
        return false;
    }
    let mut files = Vec::new();
    collect_files_recursive(&root, &mut files).is_ok()
        && files
            .iter()
            .any(|path| file_matches_release_version(path, version))
}

fn file_matches_release_version(path: &str, version: &str) -> bool {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.contains(version))
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
async fn create_or_update_forge_release(
    forge: crate::forge::ForgeKind,
    repo_root: &str,
    tag_name: &str,
    remote_spec: Option<&str>,
    release_title: &str,
    release_notes_markdown: Option<&str>,
    artifact_files: &[String],
    cancel: GitCancellation,
    emit_progress: &mut impl FnMut(Vec<String>),
) -> Result<()> {
    let cli_name = forge.cli_name();
    let forge_label = forge.display_name();
    ensure_not_cancelled(&cancel)?;
    let repo_args = release_cli_repo_args(forge, repo_root, remote_spec)?;
    let notes_file = release_notes_markdown
        .filter(|notes| !notes.trim().is_empty())
        .map(write_release_notes_file)
        .transpose()?;

    let mut release_view_args = vec![
        "release".to_string(),
        "view".to_string(),
        tag_name.to_string(),
    ];
    release_view_args.extend(repo_args.clone());
    let release_exists = Command::new(cli_name)
        .current_dir(repo_root)
        .args(release_view_args.iter().map(String::as_str))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .with_context(|| format!("failed to check for an existing {forge_label} release"))?
        .success();

    let result = async {
        if release_exists {
            emit_progress(vec![format!(
                "Updating existing {forge_label} release '{tag_name}'."
            )]);
            match forge {
                crate::forge::ForgeKind::GitHub => {
                    let repo_root_owned = repo_root.to_string();
                    let upload_cancel = cancel.clone();

                    let mut upload_args = vec![
                        "release".to_string(),
                        "upload".to_string(),
                        tag_name.to_string(),
                    ];
                    upload_args.extend(artifact_files.iter().cloned());
                    upload_args.push("--clobber".to_string());
                    upload_args.extend(repo_args.clone());
                    #[allow(clippy::too_many_arguments)]
                    run_blocking_streaming_operation(
                        move |progress_tx| {
                            run_command_with_streaming(
                                &repo_root_owned,
                                cli_name,
                                &upload_args,
                                RELEASE_NOW_TIMEOUT,
                                &format!("{cli_name} release upload"),
                                cli_name,
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
                        ]
                        .into_iter()
                        .chain(repo_args.clone())
                        .collect::<Vec<_>>();
                        let repo_root_owned = repo_root.to_string();
                        let edit_cancel = cancel.clone();
                        run_blocking_streaming_operation(
                            move |progress_tx| {
                                run_command_with_streaming(
                                    &repo_root_owned,
                                    cli_name,
                                    &edit_args,
                                    RELEASE_NOW_TIMEOUT,
                                    &format!("{cli_name} release edit"),
                                    cli_name,
                                    &edit_cancel,
                                    &progress_tx,
                                )
                            },
                            emit_progress,
                        )
                        .await?;
                    }
                }
                crate::forge::ForgeKind::GitLab => {
                    let repo_selector = repo_selector_from_cli_repo_args(&repo_args)?;
                    let asset_labels = artifact_files
                        .iter()
                        .map(|path| release_asset_label_from_path(path))
                        .collect::<Vec<_>>();

                    let mut create_args = vec![
                        "release".to_string(),
                        "create".to_string(),
                        tag_name.to_string(),
                        "--name".to_string(),
                        release_title.to_string(),
                    ];
                    if let Some(notes_file) = &notes_file {
                        create_args.push("--notes-file".to_string());
                        create_args.push(notes_file.display().to_string());
                    }
                    create_args.extend(repo_args.clone());
                    let repo_root_owned = repo_root.to_string();
                    let create_cancel = cancel.clone();
                    run_blocking_streaming_operation(
                        move |progress_tx| {
                            run_command_with_streaming(
                                &repo_root_owned,
                                cli_name,
                                &create_args,
                                RELEASE_NOW_TIMEOUT,
                                &format!("{cli_name} release create"),
                                cli_name,
                                &create_cancel,
                                &progress_tx,
                            )
                        },
                        emit_progress,
                    )
                    .await?;

                    let tag_name_owned = tag_name.to_string();
                    let removed_assets = run_blocking_job(move || {
                        crate::glab::release::remove_conflicting_release_assets(
                            &repo_selector,
                            &tag_name_owned,
                            &asset_labels,
                        )
                    })
                    .await?;
                    for asset_name in removed_assets {
                        emit_progress(vec![format!(
                            "Removed existing GitLab release asset '{asset_name}' before re-upload."
                        )]);
                    }

                    let mut upload_args = vec![
                        "release".to_string(),
                        "upload".to_string(),
                        tag_name.to_string(),
                    ];
                    upload_args.extend(
                        artifact_files
                            .iter()
                            .map(|path| gitlab_release_asset_argument(path)),
                    );
                    upload_args.extend(repo_args.clone());
                    let repo_root_owned = repo_root.to_string();
                    let upload_cancel = cancel.clone();
                    run_blocking_streaming_operation(
                        move |progress_tx| {
                            run_command_with_streaming(
                                &repo_root_owned,
                                cli_name,
                                &upload_args,
                                RELEASE_NOW_TIMEOUT,
                                &format!("{cli_name} release upload"),
                                cli_name,
                                &upload_cancel,
                                &progress_tx,
                            )
                        },
                        emit_progress,
                    )
                    .await?;
                }
            }
        } else {
            let remote_name = resolve_release_push_remote(repo_root, remote_spec)?;
            emit_progress(vec![format!(
                "Pushing tag '{}' to {}.",
                tag_name, remote_name
            )]);

            let repo_root_owned = repo_root.to_string();
            let push_cancel = cancel.clone();
            let push_args = vec!["push".to_string(), remote_name, tag_name.to_string()];
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

            emit_progress(vec![format!(
                "Creating {forge_label} release '{tag_name}'."
            )]);

            let mut create_args = vec![
                "release".to_string(),
                "create".to_string(),
                tag_name.to_string(),
            ];
            match forge {
                crate::forge::ForgeKind::GitHub => {
                    create_args.extend(artifact_files.iter().cloned());
                }
                crate::forge::ForgeKind::GitLab => {
                    create_args.extend(
                        artifact_files
                            .iter()
                            .map(|path| gitlab_release_asset_argument(path)),
                    );
                }
            }
            create_args.push(match forge {
                crate::forge::ForgeKind::GitHub => "--title".to_string(),
                crate::forge::ForgeKind::GitLab => "--name".to_string(),
            });
            create_args.push(release_title.to_string());
            if let Some(notes_file) = &notes_file {
                create_args.push("--notes-file".to_string());
                create_args.push(notes_file.display().to_string());
            }
            create_args.extend(repo_args.clone());
            let repo_root = repo_root.to_string();
            let create_cancel = cancel.clone();
            run_blocking_streaming_operation(
                move |progress_tx| {
                    run_command_with_streaming(
                        &repo_root,
                        cli_name,
                        &create_args,
                        RELEASE_NOW_TIMEOUT,
                        &format!("{cli_name} release create"),
                        cli_name,
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
    fn extract_relevant_error_detail_prefers_glab_asset_conflict() {
        let message = "glab release create failed with exit code 1: [glab][stdout] • Validating tag v0.3.2 | [glab][stdout] ✓ Release updated | [glab][stdout] ERROR | [glab][stdout] Name has already been taken";

        let detail = extract_relevant_error_detail(message).expect("detail");

        assert!(detail.contains("already been taken") || detail.contains("ERROR"));
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

        let generated_commit =
            create_release_now_generated_files_commit(&repo_root, "v1.2.3", &[], false)
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

    #[test]
    fn create_release_now_generated_files_commit_commits_auto_injected_readme() {
        let repo_dir = create_temp_repo_dir("release-now-readme-auto-injection");
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

        fs::write(repo_dir.join("README.md"), "seed\n\ninjected\n").expect("update readme");
        run_git_checked(&repo_root, &["add", "README.md"]).expect("stage readme injection");

        let generated_commit =
            create_release_now_generated_files_commit(&repo_root, "v1.2.3", &[], false)
                .expect("create generated commit")
                .expect("readme commit should exist");

        let release_commit_subject = run_git_checked(&repo_root, &["log", "-1", "--pretty=%s"])
            .expect("read readme commit subject");
        assert_eq!(
            release_commit_subject.trim(),
            "~ReleaseNOW: Changelog Auto-injection"
        );

        rollback_release_now_generated_files_commit(&repo_root, &generated_commit)
            .expect("roll back generated commit");

        let status = run_git_checked(&repo_root, &["status", "--short"])
            .expect("read staged status after rollback");
        assert!(status.contains("M  README.md"));

        fs::remove_dir_all(&repo_dir).expect("remove temp repo dir");
    }

    #[test]
    fn create_release_now_generated_files_commit_keeps_readme_in_separate_commit() {
        let repo_dir = create_temp_repo_dir("release-now-readme-and-generated");
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

        fs::write(repo_dir.join("README.md"), "seed\n\ninjected\n").expect("update readme");
        run_git_checked(&repo_root, &["add", "README.md"]).expect("stage readme injection");

        let syncmem_dir = repo_dir.join(".comfygit").join("syncmem");
        fs::create_dir_all(&syncmem_dir).expect("create syncmem dir");
        fs::write(syncmem_dir.join("stdchlg.json"), "{}\n").expect("write syncmem file");

        create_release_now_generated_files_commit(&repo_root, "v1.2.3", &[], false)
            .expect("create generated commit")
            .expect("commits should exist");

        let subjects = run_git_checked(&repo_root, &["log", "-2", "--pretty=%s"])
            .expect("read top two commit subjects");
        let subject_lines = subjects.lines().collect::<Vec<_>>();
        assert_eq!(subject_lines.len(), 2);
        assert!(subject_lines[0].contains("ReleaseNOW! → v1.2.3 has just been released"));
        assert_eq!(
            subject_lines[1].trim(),
            "~ReleaseNOW: Changelog Auto-injection"
        );

        fs::remove_dir_all(&repo_dir).expect("remove temp repo dir");
    }

    #[test]
    fn stage_auto_injected_readme_updates_and_stages_readme() {
        let repo_dir = create_temp_repo_dir("release-now-stage-auto-readme");
        let repo_root = repo_dir.to_string_lossy().to_string();

        run_git_checked(&repo_root, &["init"]).expect("init repo");
        run_git_checked(&repo_root, &["config", "user.name", "ComfyGit Tests"])
            .expect("configure user.name");
        run_git_checked(
            &repo_root,
            &["config", "user.email", "tests@comfygit.invalid"],
        )
        .expect("configure user.email");

        fs::write(repo_dir.join("README.md"), "# crate\n\nbody\n").expect("write readme");
        run_git_checked(&repo_root, &["add", "README.md"]).expect("stage readme");
        run_git_checked(&repo_root, &["commit", "-m", "seed"]).expect("commit seed");

        stage_auto_injected_readme(
            &repo_root,
            "v1.2.3",
            "## Changelog `v1.2.3`\n\n### ♻️ Refactor\n\n* Updated docs\n",
            2,
            Some("https://github.com/comfy-home/ComfyGit"),
            false,
            crate::config::ReadmeInjectDepth::CurrentOnly,
        )
        .expect("stage injected readme");

        let status =
            run_git_checked(&repo_root, &["status", "--short"]).expect("read staged status");
        assert!(status.contains("M  README.md"));

        let readme = fs::read_to_string(repo_dir.join("README.md")).expect("read updated readme");
        assert!(readme.contains("👀 What's new in v1.2.3"));

        fs::remove_dir_all(&repo_dir).expect("remove temp repo dir");
    }

    #[test]
    fn remote_branch_head_commit_reads_pushed_branch_head() {
        let remote_dir = create_temp_repo_dir("release-now-remote-head-remote");
        let local_dir = create_temp_repo_dir("release-now-remote-head-local");
        let remote_root = remote_dir.to_string_lossy().to_string();
        let local_root = local_dir.to_string_lossy().to_string();

        run_git_checked(&remote_root, &["init", "--bare"]).expect("init bare remote");
        run_git_checked(&local_root, &["init"]).expect("init local repo");
        run_git_checked(&local_root, &["config", "user.name", "ComfyGit Tests"])
            .expect("configure user.name");
        run_git_checked(
            &local_root,
            &["config", "user.email", "tests@comfygit.invalid"],
        )
        .expect("configure user.email");

        fs::write(local_dir.join("README.md"), "seed\n").expect("write readme");
        run_git_checked(&local_root, &["add", "README.md"]).expect("stage readme");
        run_git_checked(&local_root, &["commit", "-m", "seed"]).expect("commit readme");
        run_git_checked(&local_root, &["branch", "-M", "main"]).expect("rename branch");
        run_git_checked(&local_root, &["remote", "add", "origin", &remote_root])
            .expect("add origin remote");
        run_git_checked(&local_root, &["push", "-u", "origin", "main"]).expect("push main");

        let local_head = current_head_commit(&local_root).expect("read local head");
        let remote_head =
            remote_branch_head_commit(&local_root, "origin", "main").expect("read remote head");

        assert_eq!(remote_head.as_deref(), Some(local_head.as_str()));

        fs::remove_dir_all(&local_dir).expect("remove local temp repo dir");
        fs::remove_dir_all(&remote_dir).expect("remove remote temp repo dir");
    }

    #[test]
    fn release_version_from_tag_strips_v_prefix() {
        assert_eq!(release_version_from_tag("v0.3.2"), "0.3.2");
        assert_eq!(release_version_from_tag("1.0.0"), "1.0.0");
    }

    #[test]
    fn scan_artifacts_for_release_version_detects_ready_platforms() {
        let repo_dir = create_temp_repo_dir("release-now-artifact-scan");
        let repo_root = repo_dir.to_string_lossy().to_string();
        let artifact_path = repo_dir.join("dist").join("latest").join("linux-amd64");
        fs::create_dir_all(&artifact_path).expect("create artifact dir");
        fs::write(
            artifact_path.join("snif-0.3.2-linux-amd64.tar.gz"),
            b"artifact",
        )
        .expect("write artifact");

        let option = ReleaseNowRunOption {
            label: "All configured".to_string(),
            scripts: vec![ReleaseNowScript {
                label: "Linux AMD".to_string(),
                script_path: "scripts/build.sh".to_string(),
                artifact_dirs: vec!["linux-amd64".to_string()],
            }],
            artifact_dirs: vec!["linux-amd64".to_string()],
        };

        let statuses = scan_artifacts_for_release_version(&repo_root, "v0.3.2", &option);
        assert_eq!(statuses.len(), 1);
        assert!(statuses[0].ready);
        assert_eq!(statuses[0].existing_files.len(), 1);

        fs::remove_dir_all(&repo_dir).expect("remove temp repo dir");
    }

    #[test]
    fn scripts_to_run_skips_ready_platforms_when_reusing_all() {
        let script = ReleaseNowScript {
            label: "Linux AMD".to_string(),
            script_path: "scripts/build.sh".to_string(),
            artifact_dirs: vec!["linux-amd64".to_string()],
        };
        let dialog = ReleaseNowDialog {
            project_name: "Test".to_string(),
            integration_mode: crate::config::IntegrationMode::GitHubEnabled,
            scope_label: "main".to_string(),
            scope: GitScopeContext {
                display_name: "main".to_string(),
                scope_kind: None,
                repo_root: "/tmp".to_string(),
                remote_spec: None,
                secondary_remote_spec: None,
                main_branch_name: Some("main".to_string()),
                suggested_tag_name: "v0.3.2".to_string(),
                path_filters: Vec::new(),
                hide_pr_messages: false,
                hide_bump_messages: false,
                mini_commit_hashes: false,
                changelog_wrap_detailed_if_top_picks: false,
            },
            changelog_enabled: false,
            mirror_summary_to_root_changelog: false,
            repo_root: "/tmp".to_string(),
            tag_name: "v0.3.2".to_string(),
            options: vec![ReleaseNowRunOption {
                label: "All configured".to_string(),
                scripts: vec![script.clone()],
                artifact_dirs: vec!["linux-amd64".to_string()],
            }],
            selected_option: 0,
            attach_changelog: false,
            release_notes_markdown: String::new(),
            release_notes_placeholder: String::new(),
            warning_message: None,
            mode: ReleaseNowMode::Configure,
            running: false,
            auto_follow: false,
            cancel_requested: false,
            warning_confirm_selected: false,
            platform_artifact_statuses: vec![ReleaseNowPlatformArtifactStatus {
                label: script.label.clone(),
                script: script.clone(),
                existing_files: vec![
                    "dist/latest/linux-amd64/snif-0.3.2-linux-amd64.tar.gz".to_string(),
                ],
                ready: true,
            }],
            artifact_strategy: ReleaseNowArtifactStrategy::ReuseAll,
            artifact_reuse_by_label: std::collections::HashMap::new(),
            artifacts_choice_selected: 0,
            customize_selected_platform: 0,
            scroll: 0,
            body_viewport_height: 0,
            body_viewport_width: 0,
            release_notes_display_line_count: 0,
            selection_anchor: None,
            selection_focus: None,
            summary: None,
            summary_is_warning: false,
            summary_is_error: false,
            artifact_files: Vec::new(),
            log_lines: Vec::new(),
            quick_downloads: ReleaseNowQuickDownloadsSettings::default(),
            readme_injection_enabled: false,
            readme_inject_only_top_picks: false,
            readme_inject_depth: crate::config::ReadmeInjectDepth::CurrentOnly,
            readme_inject_at_row: 0,
            release_title_template: String::new(),
            mirror_sync_report: None,
            mirror_sync_running: false,
            mirror_sync_log_lines: Vec::new(),
            started_at: None,
            frozen_elapsed: None,
        };

        assert!(dialog.scripts_to_run().is_empty());
        assert!(dialog.artifact_reuse_summary().is_some());
    }

    #[test]
    fn release_notes_preview_renders_markdown_with_cte() {
        let dialog = ReleaseNowDialog {
            project_name: "Test".to_string(),
            integration_mode: crate::config::IntegrationMode::GitHubEnabled,
            scope_label: "main".to_string(),
            scope: GitScopeContext {
                display_name: "main".to_string(),
                scope_kind: None,
                repo_root: "/tmp".to_string(),
                remote_spec: None,
                secondary_remote_spec: None,
                main_branch_name: Some("main".to_string()),
                suggested_tag_name: "v0.3.2".to_string(),
                path_filters: Vec::new(),
                hide_pr_messages: false,
                hide_bump_messages: false,
                mini_commit_hashes: false,
                changelog_wrap_detailed_if_top_picks: false,
            },
            changelog_enabled: true,
            mirror_summary_to_root_changelog: false,
            repo_root: "/tmp".to_string(),
            tag_name: "v0.3.2".to_string(),
            options: Vec::new(),
            selected_option: 0,
            attach_changelog: true,
            release_notes_markdown: "## Features\n\n* first item\n* second item\n".to_string(),
            release_notes_placeholder: String::new(),
            warning_message: None,
            mode: ReleaseNowMode::Configure,
            running: false,
            auto_follow: false,
            cancel_requested: false,
            warning_confirm_selected: false,
            platform_artifact_statuses: Vec::new(),
            artifact_strategy: ReleaseNowArtifactStrategy::Pending,
            artifact_reuse_by_label: std::collections::HashMap::new(),
            artifacts_choice_selected: 0,
            customize_selected_platform: 0,
            scroll: 0,
            body_viewport_height: 20,
            body_viewport_width: 80,
            release_notes_display_line_count: 0,
            selection_anchor: None,
            selection_focus: None,
            summary: None,
            summary_is_warning: false,
            summary_is_error: false,
            artifact_files: Vec::new(),
            log_lines: Vec::new(),
            quick_downloads: ReleaseNowQuickDownloadsSettings::default(),
            readme_injection_enabled: false,
            readme_inject_only_top_picks: false,
            readme_inject_depth: crate::config::ReadmeInjectDepth::CurrentOnly,
            readme_inject_at_row: 0,
            release_title_template: String::new(),
            mirror_sync_report: None,
            mirror_sync_running: false,
            mirror_sync_log_lines: Vec::new(),
            started_at: None,
            frozen_elapsed: None,
        };

        assert!(dialog.is_release_notes_preview());
        let rendered: String = dialog
            .rendered_body_lines(None)
            .iter()
            .flat_map(|line| line.spans.iter().map(|span| span.content.as_ref()))
            .collect();
        assert!(
            rendered.contains("Features"),
            "expected rendered heading, got:\n{rendered}"
        );
        assert!(
            rendered.contains("first item") && rendered.contains("second item"),
            "expected rendered list items, got:\n{rendered}"
        );
        assert!(
            !rendered.contains("## Features"),
            "raw markdown should be parsed, got:\n{rendered}"
        );
    }
}
