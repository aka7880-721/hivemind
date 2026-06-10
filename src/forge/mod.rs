// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit License v1.2

use std::path::Path;

use anyhow::{Result, bail};
use chrono::{DateTime, Local};

use crate::config::IntegrationMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForgeKind {
    GitHub,
    GitLab,
}

impl ForgeKind {
    pub fn cli_name(self) -> &'static str {
        match self {
            ForgeKind::GitHub => crate::ghub::CLI_NAME,
            ForgeKind::GitLab => crate::glab::CLI_NAME,
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            ForgeKind::GitHub => "GitHub",
            ForgeKind::GitLab => "GitLab",
        }
    }

    pub fn pull_request_label(self) -> &'static str {
        match self {
            ForgeKind::GitHub => "pull request",
            ForgeKind::GitLab => "merge request",
        }
    }

    pub fn ensure_available(self) -> Result<()> {
        match self {
            ForgeKind::GitHub => crate::ghub::ensure_available(),
            ForgeKind::GitLab => crate::glab::ensure_available(),
        }
    }

    pub fn ensure_authenticated(self) -> Result<()> {
        match self {
            ForgeKind::GitHub => crate::ghub::ensure_authenticated(),
            ForgeKind::GitLab => crate::glab::ensure_authenticated(),
        }
    }

    pub fn owner_repo_from_remote_url(self, remote_url: &str) -> Option<(String, String)> {
        match self {
            ForgeKind::GitHub => crate::ghub::owner_repo_from_remote_url(remote_url),
            ForgeKind::GitLab => crate::glab::owner_repo_from_remote_url(remote_url),
        }
    }

    pub fn pull_conflicts_url(self, repo_root: &str, number: u64) -> Option<String> {
        match self {
            ForgeKind::GitHub => crate::ghub::pull_conflicts_url(repo_root, number),
            ForgeKind::GitLab => crate::glab::merge_request_conflicts_url(repo_root, number),
        }
    }

    pub fn release_page_url(self, remote_url: &str, tag: &str) -> Option<String> {
        match self {
            ForgeKind::GitHub => crate::ghub::release_page_url(remote_url, tag),
            ForgeKind::GitLab => crate::glab::release_page_url(remote_url, tag),
        }
    }

    pub fn release_download_url(
        self,
        owner: &str,
        repo: &str,
        tag: &str,
        file_name: &str,
    ) -> String {
        match self {
            ForgeKind::GitHub => crate::ghub::release_download_url(owner, repo, tag, file_name),
            ForgeKind::GitLab => crate::glab::release_download_url(owner, repo, tag, file_name),
        }
    }

    pub fn list_open_pull_requests(
        self,
        repo_root: &str,
        limit: usize,
    ) -> Result<Vec<ForgePullRequest>> {
        match self {
            ForgeKind::GitHub => crate::ghub::pr::list_open_pull_requests(repo_root, limit),
            ForgeKind::GitLab => crate::glab::mr::list_open_merge_requests(repo_root, limit),
        }
    }

    pub fn view_pull_request(self, repo_root: &str, number: u64) -> Result<ForgePullRequest> {
        match self {
            ForgeKind::GitHub => crate::ghub::pr::view_pull_request(repo_root, number),
            ForgeKind::GitLab => crate::glab::mr::view_merge_request(repo_root, number),
        }
    }

    pub fn fetch_mergeability(self, repo_root: &str, number: u64) -> Result<ForgeMergeability> {
        match self {
            ForgeKind::GitHub => {
                let status = crate::ghub::pr::fetch_mergeability(repo_root, number)?;
                Ok(ForgeMergeability {
                    mergeable: status.mergeable,
                    merge_state_status: status.merge_state_status,
                })
            }
            ForgeKind::GitLab => {
                let status = crate::glab::mr::fetch_mergeability(repo_root, number)?;
                Ok(ForgeMergeability {
                    mergeable: status.mergeable,
                    merge_state_status: status.merge_state_status,
                })
            }
        }
    }

    pub fn merge_pull_request(
        self,
        repo_root: &str,
        number: u64,
        subject: &str,
        source_branch: &str,
        delete_remote: bool,
        delete_local: bool,
    ) -> Result<String> {
        match self {
            ForgeKind::GitHub => crate::ghub::pr::merge_pull_request(
                repo_root,
                number,
                subject,
                source_branch,
                delete_remote,
                delete_local,
            ),
            ForgeKind::GitLab => {
                crate::glab::mr::merge_merge_request(repo_root, number, subject, delete_remote)
            }
        }
    }

    pub fn create_pull_request(
        self,
        repo_root: &str,
        target_branch: &str,
        current_branch: &str,
        title: &str,
        body_path: &Path,
    ) -> Result<String> {
        match self {
            ForgeKind::GitHub => crate::ghub::pr::create_pull_request(
                repo_root,
                target_branch,
                current_branch,
                title,
                body_path,
            ),
            ForgeKind::GitLab => crate::glab::mr::create_merge_request(
                repo_root,
                target_branch,
                current_branch,
                title,
                body_path,
            ),
        }
    }

    pub fn lookup_created_pull_request(
        self,
        repo_root: &str,
        current_branch: &str,
        target_branch: &str,
    ) -> Result<(u64, String)> {
        match self {
            ForgeKind::GitHub => crate::ghub::pr::lookup_created_pull_request(
                repo_root,
                current_branch,
                target_branch,
            ),
            ForgeKind::GitLab => crate::glab::mr::lookup_created_merge_request(
                repo_root,
                current_branch,
                target_branch,
            ),
        }
    }

    pub fn last_release_published_at(self, repo_root: &str) -> Result<Option<String>> {
        match self {
            ForgeKind::GitHub => crate::ghub::release::last_release_published_at(repo_root),
            ForgeKind::GitLab => crate::glab::release::last_release_published_at(repo_root),
        }
    }

    pub fn last_release_tag(self, repo_root: &str) -> Result<Option<String>> {
        match self {
            ForgeKind::GitHub => crate::ghub::release::last_release_tag(repo_root),
            ForgeKind::GitLab => crate::glab::release::last_release_tag(repo_root),
        }
    }

    pub fn latest_public_release_tag(self, repo_root: &str) -> Option<String> {
        match self {
            ForgeKind::GitHub => crate::ghub::release::latest_public_release_tag(repo_root),
            ForgeKind::GitLab => crate::glab::release::latest_public_release_tag(repo_root),
        }
    }

    pub fn delete_release(self, repo_root: &str, tag_name: &str) -> Result<()> {
        match self {
            ForgeKind::GitHub => crate::ghub::release::delete_release(repo_root, tag_name),
            ForgeKind::GitLab => crate::glab::release::delete_release(repo_root, tag_name),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ForgePullRequest {
    pub number: u64,
    pub title: String,
    pub target_branch: String,
    pub source_branch: String,
    pub created_at: String,
    pub author: String,
    pub status: String,
    pub mergeable_state: String,
    pub issue_url: Option<String>,
}

impl ForgePullRequest {
    pub fn created_label(&self) -> String {
        DateTime::parse_from_rfc3339(&self.created_at)
            .map(|timestamp| {
                timestamp
                    .with_timezone(&Local)
                    .format("%Y-%m-%d %H:%M")
                    .to_string()
            })
            .unwrap_or_else(|_| self.created_at.clone())
    }

    pub fn created_at_unix(&self) -> i64 {
        DateTime::parse_from_rfc3339(&self.created_at)
            .ok()
            .map(|timestamp| timestamp.timestamp())
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone)]
pub struct ForgeMergeability {
    pub mergeable: String,
    pub merge_state_status: String,
}

impl ForgeMergeability {
    #[allow(dead_code)]
    pub fn is_unknown(&self) -> bool {
        self.mergeable.eq_ignore_ascii_case("UNKNOWN")
            || self.merge_state_status.eq_ignore_ascii_case("UNKNOWN")
    }

    pub fn is_mergeable(&self) -> bool {
        self.mergeable.eq_ignore_ascii_case("MERGEABLE")
            || self.mergeable.eq_ignore_ascii_case("can_be_merged")
    }

    /// Remote mergeability is still being computed (GitLab `checking`, GitHub `UNKNOWN`, etc.).
    pub fn is_pending(&self) -> bool {
        if self.is_mergeable() {
            return false;
        }
        if self.is_definitively_not_mergeable() {
            return false;
        }
        is_pending_mergeability_token(&self.mergeable)
            || is_pending_mergeability_token(&self.merge_state_status)
    }

    /// A definitive failure state — do not wait/retry.
    pub fn is_definitively_not_mergeable(&self) -> bool {
        if self.is_mergeable() {
            return false;
        }
        is_blocked_mergeability_token(&self.mergeable)
            || is_blocked_mergeability_token(&self.merge_state_status)
    }
}

fn is_pending_mergeability_token(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "checking"
            | "unchecked"
            | "preparing"
            | "unknown"
            | "not_ready"
            | "not ready"
            | "not_started"
            | "not started"
            | "in_progress"
            | "in progress"
            | "processing"
    )
}

fn is_blocked_mergeability_token(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "conflict"
            | "conflicting"
            | "cannot_be_merged"
            | "cannot be merged"
            | "not_mergeable"
            | "not mergeable"
            | "blocked"
            | "dirty"
            | "failed"
            | "broken"
    )
}

pub fn detect_forge_for_repo(repo_root: &str) -> Option<ForgeKind> {
    let remote_name = crate::git::default_push_remote_name(repo_root).ok()?;
    let remote_url =
        crate::git::run_git_checked(repo_root, &["remote", "get-url", &remote_name]).ok()?;
    detect_forge_from_remote_url(remote_url.trim())
}

pub fn detect_forge_from_remote_url(remote_url: &str) -> Option<ForgeKind> {
    if crate::ghub::owner_repo_from_remote_url(remote_url).is_some() {
        Some(ForgeKind::GitHub)
    } else if crate::glab::owner_repo_from_remote_url(remote_url).is_some() {
        Some(ForgeKind::GitLab)
    } else {
        None
    }
}

pub fn integration_mode_for_remote_url(remote_url: &str) -> Option<IntegrationMode> {
    detect_forge_from_remote_url(remote_url).map(|kind| match kind {
        ForgeKind::GitHub => IntegrationMode::GitHubEnabled,
        ForgeKind::GitLab => IntegrationMode::GitLabEnabled,
    })
}

pub fn integration_mode_for_dual_remotes(
    gitlab_remote_url: &str,
    github_remote_url: &str,
) -> Option<IntegrationMode> {
    let gitlab = detect_forge_from_remote_url(gitlab_remote_url)?;
    let github = detect_forge_from_remote_url(github_remote_url)?;
    if gitlab == ForgeKind::GitLab && github == ForgeKind::GitHub {
        Some(IntegrationMode::GitLabGitHubEnabled)
    } else {
        None
    }
}

pub fn integration_mode_for_repo_config(repo: &crate::config::RepoConfig) -> IntegrationMode {
    if let (Some(gitlab_remote), Some(github_remote)) = (
        repo.remote_url.as_deref(),
        repo.secondary_remote_url.as_deref(),
    ) && let Some(mode) = integration_mode_for_dual_remotes(gitlab_remote, github_remote)
    {
        return mode;
    }

    if let Some(remote_url) = repo.remote_url.as_deref() {
        return integration_mode_for_remote_url(remote_url)
            .unwrap_or(crate::config::IntegrationMode::GitLocalOnly);
    }

    crate::config::IntegrationMode::GitLocalOnly
}

pub fn ensure_forge_clis(integration_mode: IntegrationMode) -> Result<Vec<ForgeKind>> {
    match integration_mode {
        IntegrationMode::GitLabGitHubEnabled => {
            ForgeKind::GitLab.ensure_available()?;
            ForgeKind::GitHub.ensure_available()?;
            Ok(vec![ForgeKind::GitLab, ForgeKind::GitHub])
        }
        _ => Ok(vec![require_forge_cli(integration_mode)?]),
    }
}

pub fn ensure_forge_authenticated(integration_mode: IntegrationMode) -> Result<()> {
    for forge in ensure_forge_clis(integration_mode)? {
        forge.ensure_authenticated()?;
    }
    Ok(())
}

pub fn github_repo_slug_from_remote_url(remote_url: &str) -> Option<String> {
    let (owner, repo) = ForgeKind::GitHub.owner_repo_from_remote_url(remote_url)?;
    Some(format!("{owner}/{repo}"))
}

pub fn resolve_github_repo_slug_for_actions(
    repo_root: &str,
    configured_secondary_remote: Option<&str>,
) -> Result<String> {
    if let Some(remote_url) = configured_secondary_remote
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if let Some(slug) = github_repo_slug_from_remote_url(remote_url) {
            return Ok(slug);
        }
        if let Ok(remote_name) = crate::git::resolve_push_remote_name(repo_root, remote_url)
            && let Ok(url) =
                crate::git::run_git_checked(repo_root, &["remote", "get-url", &remote_name])
            && let Some(slug) = github_repo_slug_from_remote_url(url.trim())
        {
            return Ok(slug);
        }
    }

    for remote in crate::git::git_remote_names(repo_root)? {
        let url = crate::git::run_git_checked(repo_root, &["remote", "get-url", &remote])?;
        if let Some(slug) = github_repo_slug_from_remote_url(url.trim()) {
            return Ok(slug);
        }
    }

    bail!(
        "could not resolve a GitHub repository for macOS CI; configure a GitHub remote URL in the project or add an origin remote pointing at github.com"
    )
}

pub fn resolve_forge(integration_mode: IntegrationMode) -> Result<ForgeKind> {
    integration_mode.forge_kind().ok_or_else(|| {
        anyhow::anyhow!(
            "this command requires a GitHub- or GitLab-enabled project (current mode: {})",
            integration_mode.display_name()
        )
    })
}

pub fn require_forge_cli(integration_mode: IntegrationMode) -> Result<ForgeKind> {
    let forge = resolve_forge(integration_mode)?;
    forge.ensure_available()?;
    Ok(forge)
}

pub fn require_forge_for_repo(repo_root: &str) -> Result<ForgeKind> {
    let forge = detect_forge_for_repo(repo_root).ok_or_else(|| {
        anyhow::anyhow!(
            "could not detect GitHub or GitLab from the repository remote; configure origin to github.com or gitlab.com"
        )
    })?;
    forge.ensure_available()?;
    Ok(forge)
}
