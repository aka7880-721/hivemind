// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit License v1.2
//
// For details, see the LICENSE file in the repository root.
use std::{process::Command, thread, time::Duration};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::{
    git::{GitCancellation, run_git_checked_with_cancel, switch_to_existing_branch},
    git_mg::run_merge_for_pull_request,
    git_pr::run_pr_and_capture,
};

const MERGEABILITY_RETRY_DELAYS_SECONDS: [u64; 3] = [2, 5, 15];
const MERGEABILITY_UNKNOWN_MESSAGE: &str =
    "Ooops, something's not right. Check this PR on GitHub for more info...";

pub(crate) fn run_branch_done(
    repo_root: &str,
    custom_main_branch: Option<&str>,
    cancel: Option<GitCancellation>,
) -> Result<()> {
    let created_pr = run_pr_and_capture(repo_root, false, custom_main_branch, cancel.clone())?;
    ensure_pull_request_mergeable(repo_root, created_pr.number)?;
    run_merge_for_pull_request(repo_root, created_pr.number, cancel.clone())?;
    switch_to_existing_branch(repo_root, &created_pr.target_branch)?;
    sync_current_branch(repo_root, cancel)?;

    println!();
    println!(
        "branch done complete: PR #{} merged, switched to \x1b[33m{}\x1b[0m, and synced with remote",
        created_pr.number, created_pr.target_branch
    );
    println!();
    Ok(())
}

fn ensure_pull_request_mergeable(repo_root: &str, pr_number: u64) -> Result<()> {
    for (attempt_index, delay_seconds) in MERGEABILITY_RETRY_DELAYS_SECONDS.iter().enumerate() {
        thread::sleep(Duration::from_secs(*delay_seconds));
        let status = fetch_pull_request_mergeability(repo_root, pr_number)?;
        if status.mergeable.eq_ignore_ascii_case("MERGEABLE") {
            return Ok(());
        }
        if !status.is_unknown() {
            bail!(
                "PR #{} is not mergeable yet (mergeable: {}, status: {})",
                pr_number,
                status.mergeable,
                status.merge_state_status
            )
        }
        if attempt_index + 1 == MERGEABILITY_RETRY_DELAYS_SECONDS.len() {
            bail!(MERGEABILITY_UNKNOWN_MESSAGE)
        }
    }

    bail!(MERGEABILITY_UNKNOWN_MESSAGE)
}

fn fetch_pull_request_mergeability(
    repo_root: &str,
    pr_number: u64,
) -> Result<PullRequestMergeability> {
    let output = Command::new("gh")
        .current_dir(repo_root)
        .args(build_pull_request_mergeability_args(pr_number))
        .output()
        .context("failed to execute gh pr view for mergeability verification")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !stderr.is_empty() {
            bail!("gh pr view failed: {}", stderr)
        }
        if !stdout.is_empty() {
            bail!("gh pr view failed: {}", stdout)
        }
        bail!(
            "gh pr view failed with exit code {:?}",
            output.status.code()
        )
    }

    serde_json::from_slice::<PullRequestMergeability>(&output.stdout)
        .context("failed to parse gh pr view mergeability output")
}

fn build_pull_request_mergeability_args(pr_number: u64) -> Vec<String> {
    vec![
        "pr".to_string(),
        "view".to_string(),
        pr_number.to_string(),
        "--json".to_string(),
        "mergeable,mergeStateStatus".to_string(),
    ]
}

fn sync_current_branch(repo_root: &str, cancel: Option<GitCancellation>) -> Result<()> {
    let output = run_git_checked_with_cancel(repo_root, &["pull", "--ff-only"], cancel)?;
    let output = output.trim();
    if !output.is_empty() {
        println!("{}", output);
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PullRequestMergeability {
    mergeable: String,
    merge_state_status: String,
}

impl PullRequestMergeability {
    fn is_unknown(&self) -> bool {
        self.mergeable.eq_ignore_ascii_case("UNKNOWN")
            || self.merge_state_status.eq_ignore_ascii_case("UNKNOWN")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_pull_request_mergeability_args_uses_requested_fields() {
        assert_eq!(
            build_pull_request_mergeability_args(67),
            vec!["pr", "view", "67", "--json", "mergeable,mergeStateStatus",]
                .into_iter()
                .map(str::to_string)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn pull_request_mergeability_unknown_detection_matches_retry_policy() {
        let unknown = PullRequestMergeability {
            mergeable: "UNKNOWN".to_string(),
            merge_state_status: "UNKNOWN".to_string(),
        };
        let clean = PullRequestMergeability {
            mergeable: "MERGEABLE".to_string(),
            merge_state_status: "CLEAN".to_string(),
        };
        let blocked = PullRequestMergeability {
            mergeable: "CONFLICTING".to_string(),
            merge_state_status: "DIRTY".to_string(),
        };

        assert!(unknown.is_unknown());
        assert!(!clean.is_unknown());
        assert!(!blocked.is_unknown());
    }

    #[test]
    fn mergeability_retry_delays_match_requested_backoff() {
        assert_eq!(MERGEABILITY_RETRY_DELAYS_SECONDS, [2, 5, 15]);
        assert_eq!(
            MERGEABILITY_UNKNOWN_MESSAGE,
            "Ooops, something's not right. Check this PR on GitHub for more info..."
        );
    }
}
