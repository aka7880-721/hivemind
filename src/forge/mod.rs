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

    pub fn repository_web_url(self, repo_root: &str) -> Option<String> {
        match self {
            ForgeKind::GitHub => crate::ghub::repository_web_url(repo_root),
            ForgeKind::GitLab => crate::glab::repository_web_url(repo_root),
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

    pub fn fetch_mergeability(
        self,
        repo_root: &str,
        number: u64,
    ) -> Result<ForgeMergeability> {
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
    ) -> Result<String> {
        match self {
            ForgeKind::GitHub => crate::ghub::pr::merge_pull_request(repo_root, number, subject),
            ForgeKind::GitLab => crate::glab::mr::merge_merge_request(repo_root, number, subject),
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
            ForgeKind::GitHub => {
                crate::ghub::pr::lookup_created_pull_request(repo_root, current_branch, target_branch)
            }
            ForgeKind::GitLab => {
                crate::glab::mr::lookup_created_merge_request(repo_root, current_branch, target_branch)
            }
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

    pub fn release_exists(self, repo_root: &str, tag_name: &str) -> Result<bool> {
        match self {
            ForgeKind::GitHub => crate::ghub::release::release_exists(repo_root, tag_name),
            ForgeKind::GitLab => crate::glab::release::release_exists(repo_root, tag_name),
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
    pub fn is_mergeable(&self) -> bool {
        self.mergeable_state.eq_ignore_ascii_case("MERGEABLE")
            || self.mergeable_state.eq_ignore_ascii_case("can_be_merged")
    }

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

    pub fn mergeable_label(&self) -> &'static str {
        if self.is_mergeable() { "True" } else { "False" }
    }
}

#[derive(Debug, Clone)]
pub struct ForgeMergeability {
    pub mergeable: String,
    pub merge_state_status: String,
}

impl ForgeMergeability {
    pub fn is_unknown(&self) -> bool {
        self.mergeable.eq_ignore_ascii_case("UNKNOWN")
            || self.merge_state_status.eq_ignore_ascii_case("UNKNOWN")
    }

    pub fn is_mergeable(&self) -> bool {
        self.mergeable.eq_ignore_ascii_case("MERGEABLE")
            || self.mergeable.eq_ignore_ascii_case("can_be_merged")
    }
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
