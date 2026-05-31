// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit License v1.2
//
// For details, see the LICENSE file in the repository root.

use std::{thread, time::Duration};

use crate::{
    forge::ForgeKind,
    git::{GitCancellation, run_git_checked_with_cancel, switch_to_existing_branch},
};
use anyhow::{Context, Result, bail};

pub(crate) const MERGEABILITY_PENDING_RETRY_SECONDS: u64 = 5;
pub(crate) const MERGEABILITY_PENDING_MAX_RETRIES: u32 = 3;

/// Waits for forge merge checks only while the remote reports a pending state (e.g. GitLab `checking`).
/// Any other non-mergeable result fails immediately (conflicts, blocked, etc.).
pub(crate) fn ensure_pull_request_mergeable(
    repo_root: &str,
    forge: ForgeKind,
    pr_number: u64,
) -> Result<()> {
    for attempt in 0..=MERGEABILITY_PENDING_MAX_RETRIES {
        if attempt > 0 {
            eprintln!(
                "Remote merge checks still in progress; retrying in {} seconds (attempt {}/{})...",
                MERGEABILITY_PENDING_RETRY_SECONDS,
                attempt + 1,
                MERGEABILITY_PENDING_MAX_RETRIES + 1
            );
            thread::sleep(Duration::from_secs(MERGEABILITY_PENDING_RETRY_SECONDS));
        }

        let status = forge.fetch_mergeability(repo_root, pr_number)?;
        if status.is_mergeable() {
            return Ok(());
        }
        if !status.is_pending() {
            bail!(
                "{}",
                format_non_mergeable_pull_request_error(
                    forge,
                    repo_root,
                    pr_number,
                    &status.mergeable,
                    &status.merge_state_status,
                )
            );
        }
        if attempt == MERGEABILITY_PENDING_MAX_RETRIES {
            bail!(
                "{}",
                format_non_mergeable_pull_request_error(
                    forge,
                    repo_root,
                    pr_number,
                    &status.mergeable,
                    &status.merge_state_status,
                )
            );
        }
    }

    unreachable!("mergeability retry loop always returns or bails")
}

/// After a successful MR/PR merge: check out the integration target and fast-forward from remote.
pub(crate) fn finish_after_pull_request_merge(
    repo_root: &str,
    target_branch: &str,
    cancel: Option<GitCancellation>,
) -> Result<()> {
    switch_to_existing_branch(repo_root, target_branch).with_context(|| {
        format!("failed to switch to target branch '{target_branch}' after merge")
    })?;
    sync_current_branch(repo_root, cancel)?;
    Ok(())
}

pub(crate) fn sync_current_branch(repo_root: &str, cancel: Option<GitCancellation>) -> Result<()> {
    let output = run_git_checked_with_cancel(repo_root, &["pull", "--ff-only"], cancel)?;
    let output = output.trim();
    if !output.is_empty() {
        println!("{}", output);
    }
    Ok(())
}

pub(crate) fn format_non_mergeable_pull_request_error(
    forge: ForgeKind,
    repo_root: &str,
    pr_number: u64,
    mergeable: &str,
    status: &str,
) -> String {
    let label = forge.pull_request_label();
    let mut message = format!(
        "{} #{} is not mergeable yet (mergeable: \x1b[31m{}\x1b[0m, status: \x1b[31m{}\x1b[0m)",
        capitalize_first(label),
        pr_number,
        mergeable,
        status
    );
    if let Some(conflicts_url) = forge.pull_conflicts_url(repo_root, pr_number) {
        message.push_str("\n\nTo see the issues in your browser, please visit:\n\n");
        message.push_str(&format!("\x1b[33m{}\x1b[0m", conflicts_url));
        message.push_str(&format!(
            "\n\nYou can also run `cg mg` / `cg merge` now, the conflict resolving will be done in your IDE of choice (e.g. VSCode), and the conflict tool should be opened automatically.\n If not, just select PR #{} from the list, and press V (or click the button) to open a disposable IDE merge workspace. Press R there afterwards to refresh the status.\n\n REMEMBER: Up to date IDE's have its conflict resolving tool based on creating a disposable worktree/workspace, steps are across IDE's usually the same, or very similar:\n\n 1. Select conflicting file marked with `!` in newly created worktree/workspace in Source Control tab.\n 2. Resolve problems directly in editor, or click the button to open the conflict resolving tool.\n 3. Accept 'Current Changes' or 'Incoming Changes' (or combine) as needed, and save the file(s).\n 4. It is usually enough to stage changed/resolved files. Commit is not needed.",
            pr_number
        ));
    }
    message
}

fn capitalize_first(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forge::ForgeMergeability;

    #[test]
    fn mergeability_pending_detection_includes_gitlab_checking() {
        let checking = ForgeMergeability {
            mergeable: "checking".to_string(),
            merge_state_status: "checking".to_string(),
        };
        assert!(checking.is_pending());
        assert!(!checking.is_mergeable());
        assert!(!checking.is_definitively_not_mergeable());
    }

    #[test]
    fn mergeability_pending_detection_includes_unknown() {
        let unknown = ForgeMergeability {
            mergeable: "UNKNOWN".to_string(),
            merge_state_status: "UNKNOWN".to_string(),
        };
        assert!(unknown.is_pending());
        assert!(!unknown.is_definitively_not_mergeable());
    }

    #[test]
    fn mergeability_blocked_detection_does_not_retry_conflicts() {
        let blocked = ForgeMergeability {
            mergeable: "CONFLICTING".to_string(),
            merge_state_status: "cannot_be_merged".to_string(),
        };
        assert!(!blocked.is_pending());
        assert!(blocked.is_definitively_not_mergeable());
    }

    #[test]
    fn gitlab_conflict_status_is_not_pending() {
        let conflict = ForgeMergeability {
            mergeable: "conflict".to_string(),
            merge_state_status: "conflict".to_string(),
        };
        assert!(!conflict.is_mergeable());
        assert!(!conflict.is_pending());
        assert!(conflict.is_definitively_not_mergeable());
    }

    #[test]
    fn mergeability_retry_policy_uses_three_five_second_retries() {
        assert_eq!(MERGEABILITY_PENDING_RETRY_SECONDS, 5);
        assert_eq!(MERGEABILITY_PENDING_MAX_RETRIES, 3);
    }

    #[test]
    fn format_non_mergeable_pull_request_error_colors_status_values() {
        let message = format_non_mergeable_pull_request_error(
            ForgeKind::GitHub,
            "C:/repo",
            9,
            "CONFLICTING",
            "DIRTY",
        );

        assert!(message.contains("\x1b[31mCONFLICTING\x1b[0m"));
        assert!(message.contains("\x1b[31mDIRTY\x1b[0m"));
    }
}
