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
use crossterm::{
    cursor::MoveToColumn,
    event::{self, Event, KeyCode, KeyEventKind},
    execute, queue,
    style::Print,
    terminal::{Clear, ClearType, disable_raw_mode, enable_raw_mode},
};
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

const ANSI_CYAN: &str = "\x1b[36m";
const ANSI_YELLOW: &str = "\x1b[33m";
const ANSI_RESET: &str = "\x1b[0m";

pub(crate) fn run_branch_done(
    repo_root: &str,
    custom_main_branch: Option<&str>,
    cancel: Option<GitCancellation>,
) -> Result<()> {
    let created_pr = loop {
        match run_pr_and_capture(repo_root, false, custom_main_branch, cancel.clone()) {
            Ok(created_pr) => break created_pr,
            Err(error) => {
                // Check for uncommitted changes error first
                if is_uncommitted_changes_error(&error) {
                    if let Err(e) = handle_uncommitted_changes(repo_root, cancel.clone()) {
                        // If the user cancelled or there was an error, bail out
                        if e.to_string().contains("Cancelled by user") {
                            bail!("Cancelled by user");
                        }
                        return Err(e);
                    }
                    // Try again after handling uncommitted changes
                    continue;
                }

                // Check for "ahead" errors (branch is ahead of remote)
                if let Some(ahead_branch) = is_ahead_error(&error) {
                    if !prompt_publish_target_branch(&ahead_branch)? {
                        bail!("Cancelled by user")
                    }

                    let _ = publish_branch_with_upstream(
                        repo_root,
                        &ahead_branch,
                        None,
                        cancel.clone(),
                    )?;
                    // Try again after pushing
                    continue;
                }

                // Check for unpublished branch error
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

fn is_uncommitted_changes_error(error: &anyhow::Error) -> bool {
    let message = error.to_string();
    message.contains("the git working tree has uncommitted changes")
}

fn is_ahead_error(error: &anyhow::Error) -> Option<String> {
    let message = error.to_string();

    // Look for pattern: "current branch 'branch-name' is ahead of 'origin/branch-name' by X commit(s)"
    for branch_prefix in ["current branch '", "target branch '"] {
        if let Some(start) = message.find(branch_prefix) {
            let start = start + branch_prefix.len();
            if let Some(end) = message[start..].find("' is ahead of '") {
                let branch_name = &message[start..start + end];
                return Some(branch_name.to_string());
            }
        }
    }

    None
}

#[derive(Debug, PartialEq, Eq)]
enum UncommittedChangesAction {
    Commit,
    Stash,
    Cancel,
}

fn prompt_uncommitted_changes() -> Result<UncommittedChangesAction> {
    let mut selected = 0; // 0 = Commit, 1 = Stash, 2 = Cancel

    let raw_mode = TerminalRawModeGuard::enter()?;

    loop {
        render_uncommitted_changes_menu(selected)?;

        let Event::Key(key) = event::read().context("failed to read uncommitted changes input")?
        else {
            continue;
        };

        if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
            continue;
        }

        match key.code {
            KeyCode::Up if selected > 0 => {
                selected = selected.saturating_sub(1);
            }
            KeyCode::Down if selected < 2 => {
                selected += 1;
            }
            KeyCode::Enter => {
                drop(raw_mode);
                return Ok(match selected {
                    0 => UncommittedChangesAction::Commit,
                    1 => UncommittedChangesAction::Stash,
                    2 => UncommittedChangesAction::Cancel,
                    _ => UncommittedChangesAction::Cancel,
                });
            }
            KeyCode::Esc => {
                drop(raw_mode);
                return Ok(UncommittedChangesAction::Cancel);
            }
            KeyCode::Char('c')
                if key
                    .modifiers
                    .contains(crossterm::event::KeyModifiers::CONTROL) =>
            {
                drop(raw_mode);
                return Ok(UncommittedChangesAction::Cancel);
            }
            _ => {}
        }
    }
}

fn handle_uncommitted_changes(repo_root: &str, cancel: Option<GitCancellation>) -> Result<()> {
    let action = prompt_uncommitted_changes()?;

    match action {
        UncommittedChangesAction::Commit => {
            // Add all changes and ask for commit message
            run_git_checked_with_cancel(repo_root, &["add", "."], cancel.clone())?;

            // Ask for commit message
            print!("Enter commit message: ");
            io::stdout()
                .flush()
                .context("failed to flush commit message prompt")?;

            let mut commit_message = String::new();
            io::stdin()
                .read_line(&mut commit_message)
                .context("failed to read commit message")?;

            let commit_message = commit_message.trim();
            if commit_message.is_empty() {
                bail!("Commit message cannot be empty");
            }

            run_git_checked_with_cancel(repo_root, &["commit", "-m", commit_message], cancel)?;
        }
        UncommittedChangesAction::Stash => {
            // Stash changes with a default message
            run_git_checked_with_cancel(
                repo_root,
                &["stash", "push", "-m", "Auto-stash before branch merge"],
                cancel,
            )?;
        }
        UncommittedChangesAction::Cancel => {
            bail!("Cancelled by user");
        }
    }

    Ok(())
}

fn render_uncommitted_changes_menu(selected: usize) -> Result<()> {
    let mut stdout = io::stdout();

    execute!(stdout, Clear(ClearType::All))
        .context("failed to clear screen for uncommitted changes menu")?;

    // Menu content
    queue!(
        stdout,
        MoveToColumn(0),
        Print("\r\n"),
        Print("We can't conclude this branch now because you have uncommitted changes...\r\n\r\n"),
        Print(format!(
            "{}What would you like to do with your changes?{}\r\n\r\n",
            ANSI_CYAN, ANSI_RESET
        ))
    )
    .context("failed to queue uncommitted changes header")?;

    // Options
    let options = [
        "Commit changes and continue",
        "Stash changes and continue",
        "Cancel the process",
    ];

    for (i, option) in options.iter().enumerate() {
        let display_line = if i == selected {
            format!("{}> {}{}{}", ANSI_YELLOW, option, ANSI_RESET, "")
        } else {
            format!("  {}", option)
        };

        queue!(stdout, MoveToColumn(0), Print(display_line), Print("\r\n"))
            .context("failed to queue uncommitted changes option")?;
    }

    stdout
        .flush()
        .context("failed to flush uncommitted changes menu")?;
    Ok(())
}

fn prompt_publish_target_branch(branch_name: &str) -> Result<bool> {
    let mut selected = 0; // 0 = Yes, 1 = No

    let raw_mode = TerminalRawModeGuard::enter()?;

    loop {
        render_push_confirmation_menu(branch_name, selected)?;

        let Event::Key(key) = event::read().context("failed to read push confirmation input")?
        else {
            continue;
        };

        if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
            continue;
        }

        match key.code {
            KeyCode::Up if selected > 0 => {
                selected = selected.saturating_sub(1);
            }
            KeyCode::Down if selected < 1 => {
                selected += 1;
            }
            KeyCode::Enter => {
                drop(raw_mode);
                return Ok(selected == 0);
            }
            KeyCode::Esc => {
                drop(raw_mode);
                return Ok(false);
            }
            KeyCode::Char('c')
                if key
                    .modifiers
                    .contains(crossterm::event::KeyModifiers::CONTROL) =>
            {
                drop(raw_mode);
                return Ok(false);
            }
            _ => {}
        }
    }
}

fn render_push_confirmation_menu(_branch_name: &str, selected: usize) -> Result<()> {
    let mut stdout = io::stdout();

    execute!(stdout, Clear(ClearType::All))
        .context("failed to clear screen for push confirmation menu")?;

    // Menu content
    queue!(
        stdout,
        MoveToColumn(0),
        Print("\r\n"),
        Print("We can't conclude this branch now because changes have not been pushed yet to remote...\r\n\r\n"),
        Print(format!(
            "{}Would you like to push them now and continue with the branch merge?{}\r\n\r\n",
            ANSI_CYAN, ANSI_RESET
        ))
    )
    .context("failed to queue push confirmation header")?;

    // Options
    let options = ["Yes.", "No, cancel the process."];

    for (i, option) in options.iter().enumerate() {
        let display_line = if i == selected {
            format!("{}> {}{}{}", ANSI_YELLOW, option, ANSI_RESET, "")
        } else {
            format!("  {}", option)
        };

        queue!(stdout, MoveToColumn(0), Print(display_line), Print("\r\n"))
            .context("failed to queue push confirmation option")?;
    }

    stdout
        .flush()
        .context("failed to flush push confirmation menu")?;
    Ok(())
}

struct TerminalRawModeGuard;

impl TerminalRawModeGuard {
    fn enter() -> Result<Self> {
        enable_raw_mode().context("failed to enable raw mode")?;
        Ok(Self)
    }
}

impl Drop for TerminalRawModeGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
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
        message.push_str(&format!("\x1b[33m{}\x1b[0m", conflicts_url));
        message.push_str(&format!(
            "\n\nThen run cg merge, select PR #{}, and press V to open a disposable VS Code merge workspace. Press R there afterwards to refresh the status.\n",
            pr_number
        ));
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
