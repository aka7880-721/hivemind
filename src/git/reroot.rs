// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit License v1.2
//
// For details, see the LICENSE file in the repository root.

use std::io::{self, Write};

use anyhow::{Context, Result, bail};

use crate::git::{
    GitCancellation, current_branch_with_cancel, ensure_clean_worktree_with_cancel,
    is_mainline_branch_name, resolve_main_branch_name, run_git_checked_with_cancel,
    split_output_lines,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RerootMode {
    Merge,
    Rebase,
}

impl RerootMode {
    fn verb(self) -> &'static str {
        match self {
            Self::Merge => "merge",
            Self::Rebase => "rebase",
        }
    }

    fn command(self) -> &'static str {
        match self {
            Self::Merge => "merge",
            Self::Rebase => "rebase",
        }
    }
}

pub(crate) fn run_reroot(
    repo_root: &str,
    custom_main_branch: Option<&str>,
    mode: RerootMode,
    cancel: Option<GitCancellation>,
) -> Result<()> {
    ensure_clean_worktree_with_cancel(repo_root, "cg reroot", cancel.clone())?;

    let current_branch = current_branch_with_cancel(repo_root, cancel.clone())?;
    if current_branch.starts_with("detached (") {
        bail!("cg reroot requires a checked-out branch; detached HEAD is not supported")
    }
    if is_mainline_branch_name(&current_branch, custom_main_branch) {
        bail!(
            "cg reroot is intended for non-main branches; you are currently on '{}'",
            current_branch
        )
    }

    let source_branches = derive_source_branch_candidates(
        &current_branch,
        &resolve_main_branch_name(repo_root, custom_main_branch)?,
        &list_local_branch_names(repo_root, cancel.clone())?,
    );
    if source_branches.is_empty() {
        bail!(
            "could not determine a source branch to {} into '{}'",
            mode.verb(),
            current_branch
        )
    }

    let source_branch = prompt_source_branch(&current_branch, &source_branches, mode)?;
    let upstream_ref = current_branch_upstream_ref(repo_root, &current_branch, cancel.clone())?;
    if mode == RerootMode::Rebase
        && let Some(upstream_ref) = upstream_ref.as_deref()
        && !prompt_rebase_warning(&current_branch, upstream_ref, &source_branch)?
    {
        bail!("cancelled by user")
    }

    let args = match mode {
        RerootMode::Merge => vec!["merge", "--no-ff", "--no-edit", source_branch.as_str()],
        RerootMode::Rebase => vec!["rebase", source_branch.as_str()],
    };
    let output = run_git_checked_with_cancel(repo_root, &args, cancel)?;

    println!();
    if output.trim().is_empty() {
        println!(
            "{} completed: {} ← {}",
            mode.command(),
            current_branch,
            source_branch
        );
    } else {
        println!("{}", output.trim());
    }
    if mode == RerootMode::Rebase && upstream_ref.is_some() {
        println!();
        println!(
            "warning: '{}' tracks a remote branch; if you want to update that remote after the rebase, use a force push with lease.",
            current_branch
        );
    }
    println!();

    Ok(())
}

fn list_local_branch_names(
    repo_root: &str,
    cancel: Option<GitCancellation>,
) -> Result<Vec<String>> {
    let output = run_git_checked_with_cancel(
        repo_root,
        &["for-each-ref", "--format=%(refname:short)", "refs/heads"],
        cancel,
    )?;
    let mut branches = split_output_lines(&output);
    branches.sort_by_cached_key(|branch| branch.to_ascii_lowercase());
    branches.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    Ok(branches)
}

fn current_branch_upstream_ref(
    repo_root: &str,
    branch_name: &str,
    cancel: Option<GitCancellation>,
) -> Result<Option<String>> {
    let branch_ref = format!("refs/heads/{}", branch_name);
    let output = run_git_checked_with_cancel(
        repo_root,
        &["for-each-ref", "--format=%(upstream:short)", &branch_ref],
        cancel,
    )?;
    Ok(split_output_lines(&output).into_iter().next())
}

fn derive_source_branch_candidates(
    current_branch: &str,
    main_branch: &str,
    local_branches: &[String],
) -> Vec<String> {
    let mut candidates = Vec::new();
    if !current_branch.eq_ignore_ascii_case(main_branch) {
        candidates.push(main_branch.to_string());
    }

    if let Some(release_line_branch) = semver_release_line_branch_from_dev_branch(current_branch)
        && !release_line_branch.eq_ignore_ascii_case(main_branch)
        && !release_line_branch.eq_ignore_ascii_case(current_branch)
        && local_branches
            .iter()
            .any(|branch| branch.eq_ignore_ascii_case(&release_line_branch))
        && !candidates
            .iter()
            .any(|branch| branch.eq_ignore_ascii_case(&release_line_branch))
    {
        candidates.push(release_line_branch);
    }

    candidates
}

fn semver_release_line_branch_from_dev_branch(branch_name: &str) -> Option<String> {
    let normalized = branch_name.trim().trim_start_matches('v');
    let normalized = normalized
        .split_once("--")
        .map(|(base, _)| base)
        .unwrap_or(normalized);
    let release_version = normalized.strip_suffix("-dev")?;
    let mut parts = release_version.split('.');
    let major = parts.next()?.parse::<u32>().ok()?;
    let minor = parts.next()?.parse::<u32>().ok()?;
    let _patch = parts.next()?.parse::<u32>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some(format!("{}.{}.x", major, minor))
}

fn prompt_source_branch(
    current_branch: &str,
    source_branches: &[String],
    mode: RerootMode,
) -> Result<String> {
    println!();
    println!("Current branch: {}", current_branch);
    println!("Choose the source branch to {} from:", mode.verb());
    for (index, branch) in source_branches.iter().enumerate() {
        println!("{}. {}", index + 1, branch);
    }

    loop {
        print!("Select 1-{}: ", source_branches.len());
        io::stdout()
            .flush()
            .context("failed to flush reroot source branch prompt")?;

        let mut answer = String::new();
        io::stdin()
            .read_line(&mut answer)
            .context("failed to read reroot source branch response")?;
        let trimmed = answer.trim();
        let selected = trimmed
            .parse::<usize>()
            .ok()
            .and_then(|value| value.checked_sub(1));
        if let Some(index) = selected.filter(|index| *index < source_branches.len()) {
            return Ok(source_branches[index].clone());
        }

        println!(
            "Please enter a number between 1 and {}.",
            source_branches.len()
        );
    }
}

fn prompt_rebase_warning(
    current_branch: &str,
    upstream_ref: &str,
    source_branch: &str,
) -> Result<bool> {
    println!();
    println!("SERIOUS WARNING");
    println!(
        "'{}' is already published to remote as '{}'.",
        current_branch, upstream_ref
    );
    println!(
        "Rebasing it onto '{}' will rewrite branch history and can disrupt collaborators.",
        source_branch
    );
    println!("1. Abort");
    println!("2. Continue with rebase");

    loop {
        print!("Choose 1 or 2: ");
        io::stdout()
            .flush()
            .context("failed to flush reroot rebase warning prompt")?;

        let mut answer = String::new();
        io::stdin()
            .read_line(&mut answer)
            .context("failed to read reroot rebase warning response")?;
        match answer.trim() {
            "1" => return Ok(false),
            "2" => return Ok(true),
            _ => println!("Please enter 1 or 2."),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_source_branch_candidates_for_dev_branch_prefers_main_and_release_line() {
        let candidates = derive_source_branch_candidates(
            "v0.2.0-dev",
            "main",
            &[
                "main".to_string(),
                "0.2.x".to_string(),
                "v0.2.0-dev".to_string(),
            ],
        );

        assert_eq!(candidates, vec!["main", "0.2.x"]);
    }

    #[test]
    fn derive_source_branch_candidates_for_release_line_uses_main_only() {
        let candidates = derive_source_branch_candidates(
            "0.2.x",
            "main",
            &["main".to_string(), "0.2.x".to_string()],
        );

        assert_eq!(candidates, vec!["main"]);
    }

    #[test]
    fn semver_release_line_branch_from_dev_branch_handles_specific_suffix() {
        assert_eq!(
            semver_release_line_branch_from_dev_branch("v0.2.4-dev--specific"),
            Some("0.2.x".to_string())
        );
    }
} // Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the latest ComfyGit License
//
// For details, see the LICENSE file in the repository root.
