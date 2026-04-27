// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit License v1.2
//
// For details, see the LICENSE file in the repository root.
use std::{
    io::{self, Write},
    process::Command,
    thread,
    time::Duration,
};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::{
    git::{
        GitCancellation, github_pull_conflicts_url, publish_branch_with_upstream,
        run_git_checked_with_cancel, switch_to_existing_branch,
    },
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
    let created_pr = loop {
        match run_pr_and_capture(repo_root, false, custom_main_branch, cancel.clone()) {
            Ok(created_pr) => break created_pr,
            Err(error) => {
                let Some(unpublished_branch) = unpublished_branch_name_from_error(&error) else {
                    return Err(error);
                };

                if !prompt_publish_target_branch(&unpublished_branch)? {
                    bail!("Cancelled by user")
                }

                let _ = publish_branch_with_upstream(
                    repo_root,
                    &unpublished_branch,
                    None,
                    cancel.clone(),
                )?;
            }
        }
    };
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

fn unpublished_branch_name_from_error(error: &anyhow::Error) -> Option<String> {
    let message = error.to_string();
    for prefix in ["current branch '", "target branch '"] {
        let suffix = "' is not published to a tracked remote branch; push it with upstream tracking before running cg pr";
        let Some(start) = message.find(prefix).map(|index| index + prefix.len()) else {
            continue;
        };
        let remainder = &message[start..];
        let Some(end) = remainder.find(suffix) else {
            continue;
        };
        return Some(remainder[..end].to_string());
    }

    None
}

fn prompt_publish_target_branch(branch_name: &str) -> Result<bool> {
    println!();
    println!(
        "target branch '{}' is not published to a tracked remote branch; push it with upstream tracking before running cg pr",
        branch_name
    );
    println!("1. Publish branch {} and continue", branch_name);
    println!("2. Abort");

    loop {
        print!("Choose 1 or 2: ");
        io::stdout()
            .flush()
            .context("failed to flush publish prompt")?;

        let mut answer = String::new();
        io::stdin()
            .read_line(&mut answer)
            .context("failed to read publish prompt response")?;

        match answer.trim() {
            "1" => return Ok(true),
            "2" => return Ok(false),
            _ => println!("Please enter 1 or 2."),
        }
    }
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
                "{}",
                format_non_mergeable_pull_request_error(
                    repo_root,
                    pr_number,
                    &status.mergeable,
                    &status.merge_state_status,
                )
            )
        }
        if attempt_index + 1 == MERGEABILITY_RETRY_DELAYS_SECONDS.len() {
            bail!(MERGEABILITY_UNKNOWN_MESSAGE)
        }
    }

    bail!(MERGEABILITY_UNKNOWN_MESSAGE)
}

fn format_non_mergeable_pull_request_error(
    repo_root: &str,
    pr_number: u64,
    mergeable: &str,
    status: &str,
) -> String {
    let mut message = format!(
        "PR #{} is not mergeable yet (mergeable: \x1b[31m{}\x1b[0m, status: \x1b[31m{}\x1b[0m)",
        pr_number, mergeable, status
    );
    if let Some(conflicts_url) = github_pull_conflicts_url(repo_root, pr_number) {
        message.push_str("\n\nTo see the issues, please visit:\n\n");
        message.push_str(&format!("\x1b[33m{}\x1b[0m\n", conflicts_url));
    }
    message
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

    #[test]
    fn unpublished_branch_error_parser_extracts_target_branch_name() {
        let error = anyhow::anyhow!(
            "target branch '0.1.x' is not published to a tracked remote branch; push it with upstream tracking before running cg pr"
        );

        assert_eq!(
            unpublished_branch_name_from_error(&error).as_deref(),
            Some("0.1.x")
        );
    }

    #[test]
    fn unpublished_branch_error_parser_extracts_current_branch_name() {
        let error = anyhow::anyhow!(
            "current branch 'v0.1.2-dev' is not published to a tracked remote branch; push it with upstream tracking before running cg pr"
        );

        assert_eq!(
            unpublished_branch_name_from_error(&error).as_deref(),
            Some("v0.1.2-dev")
        );
    }

    #[test]
    fn format_non_mergeable_pull_request_error_colors_status_values() {
        let message = format_non_mergeable_pull_request_error("C:/repo", 9, "CONFLICTING", "DIRTY");

        assert!(message.contains("\x1b[31mCONFLICTING\x1b[0m"));
        assert!(message.contains("\x1b[31mDIRTY\x1b[0m"));
    }
}
