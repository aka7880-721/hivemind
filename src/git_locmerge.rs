// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit License v1.2
//
// For details, see the LICENSE file in the repository root.

//! Local merge workflow — `cg merge local` and `cg br end local` (no PR).

use std::io::{self, Write};

use anyhow::{Context, Result, bail};

use crate::git::{
    GitCancellation, current_branch_with_cancel, ensure_clean_worktree_with_cancel,
    run_git_checked_with_cancel, switch_to_existing_branch,
};
use crate::git_br::{LOCAL_MERGE_TARGET_PICKER_PROMPT, prompt_branch_selection};

const ANSI_CYAN: &str = "\x1b[36m";
const ANSI_YELLOW: &str = "\x1b[33m";
const ANSI_MAGENTA: &str = "\x1b[35m";
const ANSI_RESET: &str = "\x1b[0m";

pub(crate) fn run_local_merge(
    repo_root: &str,
    project_name: &str,
    cancel: Option<GitCancellation>,
) -> Result<()> {
    let source_branch = current_branch_with_cancel(repo_root, cancel.clone())?;
    if source_branch.starts_with("detached (") {
        bail!("cannot run a local merge from a detached HEAD");
    }

    if !prompt_local_merge_position(project_name, &source_branch)? {
        bail!("Cancelled by user");
    }

    let target_branch =
        prompt_branch_selection(repo_root, &LOCAL_MERGE_TARGET_PICKER_PROMPT, cancel.clone())?;

    if source_branch.eq_ignore_ascii_case(&target_branch) {
        bail!(
            "source branch '{}' cannot be merged into itself",
            source_branch
        );
    }

    if !prompt_local_merge_confirmation(&source_branch, &target_branch)? {
        bail!("Cancelled by user");
    }

    ensure_clean_worktree_with_cancel(repo_root, "cg merge local", cancel.clone())?;
    switch_to_existing_branch(repo_root, &target_branch)?;
    ensure_clean_worktree_with_cancel(repo_root, "cg merge local", cancel.clone())?;
    run_git_checked_with_cancel(repo_root, &["merge", &source_branch], cancel.clone())?;

    println!();
    println!(
        "local merge complete: merged \x1b[33m{source_branch}\x1b[0m into \x1b[33m{target_branch}\x1b[0m"
    );
    println!();
    Ok(())
}

fn prompt_local_merge_position(project_name: &str, current_branch: &str) -> Result<bool> {
    println!();
    println!("{ANSI_CYAN}You are here:{ANSI_RESET}");
    println!("  {project_name} -> {ANSI_MAGENTA}{current_branch}{ANSI_RESET}");
    println!();
    println!(
        "{ANSI_YELLOW}This is the branch that will be merged into a branch you select on the next screen...{ANSI_RESET}"
    );
    println!();
    prompt_confirm_default_yes(&format!(
        "{ANSI_YELLOW}Press ENTER or Y to continue; N to cancel:{ANSI_RESET} "
    ))
}

fn prompt_local_merge_confirmation(source_branch: &str, target_branch: &str) -> Result<bool> {
    println!();
    println!(
        "   {ANSI_YELLOW}{source_branch}{ANSI_RESET} {ANSI_MAGENTA}--->{ANSI_RESET} {ANSI_YELLOW}{target_branch}{ANSI_RESET}"
    );
    println!();
    println!("Summary:");
    println!(
        "This action will merge {ANSI_YELLOW}{source_branch}{ANSI_RESET} into {ANSI_YELLOW}{target_branch}{ANSI_RESET}"
    );
    println!();
    prompt_confirm_default_yes(&format!(
        "{ANSI_YELLOW}Press ENTER or Y to continue; N to cancel:{ANSI_RESET} "
    ))
}

fn prompt_confirm_default_yes(prompt: &str) -> Result<bool> {
    loop {
        print!("{prompt}");
        io::stdout().flush().context("failed to flush prompt")?;

        let mut answer = String::new();
        io::stdin()
            .read_line(&mut answer)
            .context("failed to read response")?;

        match answer.trim().to_lowercase().as_str() {
            "" | "y" => return Ok(true),
            "n" => return Ok(false),
            _ => println!("Please answer Y or N."),
        }
    }
}
