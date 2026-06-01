// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit License v1.2

use anyhow::{Context, Result, bail};
use std::process::Command;

use crate::forge::{ForgeKind, detect_forge_from_remote_url};

use super::{
    current_branch_with_cancel, git_remote_names, resolve_push_remote_name, run_git_checked,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirrorSyncReport {
    pub branch: String,
    pub head_line: String,
    pub gitlab_remote: String,
    pub github_remote: String,
    pub gitlab_tracking: bool,
    pub github_tracking: bool,
}

const ANSI_STATUS_IN_SYNC: &str = "\x1b[38;5;46m";
const ANSI_STATUS_OUT_OF_SYNC: &str = "\x1b[38;5;196m";
const ANSI_RESET: &str = "\x1b[0m";

impl MirrorSyncReport {
    pub fn in_sync(&self) -> bool {
        self.gitlab_tracking && self.github_tracking
    }

    pub fn status_colored(&self) -> String {
        if self.in_sync() {
            format!("{ANSI_STATUS_IN_SYNC}in sync{ANSI_RESET}")
        } else {
            format!("{ANSI_STATUS_OUT_OF_SYNC}out of sync{ANSI_RESET}")
        }
    }

    pub fn summary_lines(&self) -> Vec<String> {
        let mut lines = vec![
            format!("Branch: {}", self.branch),
            format!("Status: {}", self.status_colored()),
            format!(
                "GitLab remote '{}': {}",
                self.gitlab_remote,
                if self.gitlab_tracking {
                    "tracking"
                } else {
                    "missing"
                }
            ),
            format!(
                "GitHub remote '{}': {}",
                self.github_remote,
                if self.github_tracking {
                    "tracking"
                } else {
                    "missing"
                }
            ),
            format!("git log --oneline -1: {}", self.head_line),
        ];
        if !self.in_sync() {
            lines.push(
                "Run 'cg sync' to push the current branch to both remotes when you choose to sync."
                    .to_string(),
            );
        }
        lines
    }
}

pub fn resolve_dual_remotes(
    repo_root: &str,
    configured_gitlab: Option<&str>,
    configured_github: Option<&str>,
) -> Result<(String, String)> {
    if let (Some(gitlab), Some(github)) = (configured_gitlab, configured_github) {
        let gitlab_name = resolve_push_remote_name(repo_root, gitlab.trim())?;
        let github_name = resolve_push_remote_name(repo_root, github.trim())?;
        return Ok((gitlab_name, github_name));
    }

    let mut gitlab_remote = None;
    let mut github_remote = None;
    for remote in git_remote_names(repo_root)? {
        let url = run_git_checked(repo_root, &["remote", "get-url", &remote])?;
        match detect_forge_from_remote_url(url.trim()) {
            Some(ForgeKind::GitLab) if gitlab_remote.is_none() || remote == "gitlab" => {
                gitlab_remote = Some(remote);
            }
            Some(ForgeKind::GitHub) if github_remote.is_none() || remote == "origin" => {
                github_remote = Some(remote);
            }
            _ => {}
        }
    }

    match (gitlab_remote, github_remote) {
        (Some(gitlab), Some(github)) => Ok((gitlab, github)),
        _ => bail!(
            "could not resolve GitLab and GitHub remotes; configure both remote URLs in the project or add 'gitlab' and 'origin' remotes"
        ),
    }
}

pub fn check_mirror_sync(
    repo_root: &str,
    configured_gitlab: Option<&str>,
    configured_github: Option<&str>,
) -> Result<MirrorSyncReport> {
    let (gitlab_remote, github_remote) =
        resolve_dual_remotes(repo_root, configured_gitlab, configured_github)?;
    let branch = current_branch_with_cancel(repo_root, None)?;
    if branch.starts_with("detached") {
        bail!("mirror sync check requires a checked-out branch, not a detached HEAD");
    }

    let output = Command::new("git")
        .current_dir(repo_root)
        .args(["log", "--oneline", "-1", "--decorate"])
        .output()
        .context("failed to read latest commit decoration")?;
    if !output.status.success() {
        bail!("failed to read latest commit for mirror sync check");
    }
    let head_line = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let gitlab_ref = format!("{gitlab_remote}/{branch}");
    let github_ref = format!("{github_remote}/{branch}");

    let gitlab_tracking = head_line.contains(&gitlab_ref);
    let github_tracking = head_line.contains(&github_ref);

    Ok(MirrorSyncReport {
        branch,
        head_line,
        gitlab_remote,
        github_remote,
        gitlab_tracking,
        github_tracking,
    })
}

pub fn push_mirror_sync(
    repo_root: &str,
    configured_gitlab: Option<&str>,
    configured_github: Option<&str>,
) -> Result<Vec<String>> {
    let (gitlab_remote, github_remote) =
        resolve_dual_remotes(repo_root, configured_gitlab, configured_github)?;
    let branch = current_branch_with_cancel(repo_root, None)?;
    if branch.starts_with("detached") {
        bail!("mirror sync requires a checked-out branch, not a detached HEAD");
    }

    let mut lines = Vec::new();
    for remote in [&gitlab_remote, &github_remote] {
        let output = Command::new("git")
            .current_dir(repo_root)
            .args(["push", remote, &branch])
            .output()
            .with_context(|| format!("failed to push branch '{branch}' to remote '{remote}'"))?;
        if !output.status.success() {
            let combined = format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            bail!(
                "failed to push branch '{branch}' to remote '{remote}': {}",
                combined.trim()
            );
        }
        lines.push(format!("Pushed '{branch}' to remote '{remote}'."));
    }
    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mirror_sync_report_in_sync_when_both_remotes_track() {
        let report = MirrorSyncReport {
            branch: "main".to_string(),
            head_line: "abc1234 (HEAD -> main, origin/main, gitlab/main) init".to_string(),
            gitlab_remote: "gitlab".to_string(),
            github_remote: "origin".to_string(),
            gitlab_tracking: true,
            github_tracking: true,
        };
        assert!(report.in_sync());
    }

    #[test]
    fn mirror_sync_report_out_of_sync_when_github_missing() {
        let report = MirrorSyncReport {
            branch: "main".to_string(),
            head_line: "abc1234 (HEAD -> main, gitlab/main) init".to_string(),
            gitlab_remote: "gitlab".to_string(),
            github_remote: "origin".to_string(),
            gitlab_tracking: true,
            github_tracking: false,
        };
        assert!(!report.in_sync());
    }

    #[test]
    fn mirror_sync_status_colored_uses_palette_codes() {
        let in_sync = MirrorSyncReport {
            branch: "main".to_string(),
            head_line: String::new(),
            gitlab_remote: "gitlab".to_string(),
            github_remote: "origin".to_string(),
            gitlab_tracking: true,
            github_tracking: true,
        };
        assert!(in_sync.status_colored().contains("38;5;46m"));
        assert!(in_sync.status_colored().contains("in sync"));

        let out_of_sync = MirrorSyncReport {
            github_tracking: false,
            ..in_sync
        };
        assert!(out_of_sync.status_colored().contains("38;5;196m"));
        assert!(out_of_sync.status_colored().contains("out of sync"));
    }
}
