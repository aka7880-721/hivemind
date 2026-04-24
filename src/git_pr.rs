// Copyright © 2026 ComfyHome™
// All rights reserved.
// Licensed under the ComfyGit License v1.2
//
// For details, see the LICENSE file in the repository root.
use std::{
    collections::HashSet,
    io::{self, Write},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};

use crate::{
    changelog::pr_changelog_gen,
    git::{
        GitCancellation, current_branch_with_cancel, resolve_main_branch_name,
        run_git_checked_with_cancel, split_output_lines,
    },
};

const PR_PREVIEW_SECONDS: u64 = 5;
const ANSI_YELLOW: &str = "\x1b[33m";
const ANSI_RESET: &str = "\x1b[0m";

pub(crate) fn run_pr(
    repo_root: &str,
    force_main: bool,
    custom_main_branch: Option<&str>,
    cancel: Option<GitCancellation>,
) -> Result<()> {
    let current_branch = current_branch_with_cancel(repo_root, cancel.clone())?;
    if current_branch.starts_with("detached (") {
        bail!("cannot create a PR from a detached HEAD");
    }

    let target_branch = if force_main {
        resolve_main_branch_name(repo_root, custom_main_branch)?
    } else {
        resolve_parent_branch_name_with_cancel(
            repo_root,
            &current_branch,
            custom_main_branch,
            cancel.clone(),
        )?
    };

    if current_branch.eq_ignore_ascii_case(&target_branch) {
        bail!(
            "current branch '{}' is the same as the target branch '{}'",
            current_branch,
            target_branch
        );
    }

    let title = format!("{} (via ComfyGit)", current_branch);
    let body = build_pr_body(repo_root, &target_branch, &current_branch, cancel.clone())?;

    preview_pr(&target_branch, &current_branch, &title, &body, cancel)?;
    Ok(())
}

fn build_pr_body(
    repo_root: &str,
    target_branch: &str,
    current_branch: &str,
    cancel: Option<GitCancellation>,
) -> Result<String> {
    let range_spec = format!("{}..{}", target_branch, current_branch);
    let output = run_git_checked_with_cancel(
        repo_root,
        &["log", "--pretty=format:%h %s", &range_spec],
        cancel,
    )?;

    let lines = split_output_lines(&output)
        .into_iter()
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>();

    if lines.is_empty() {
        return Ok(format!(
            "No commits were found between `{}` and `{}`.\n\nIf this branch was just created, ensure it has commits before opening a pull request.",
            target_branch, current_branch
        ));
    }

    Ok(pr_changelog_gen(current_branch, &lines).markdown)
}

fn preview_pr(
    target_branch: &str,
    current_branch: &str,
    title: &str,
    body: &str,
    cancel: Option<GitCancellation>,
) -> Result<()> {
    println!();
    println!(
        "{}Dry-run PR preview{}\n  {}Target branch:{} {}\n  {}Source branch:{} {}\n  {}Title:{} {}\n",
        ANSI_YELLOW,
        ANSI_RESET,
        ANSI_YELLOW,
        ANSI_RESET,
        target_branch,
        ANSI_YELLOW,
        ANSI_RESET,
        current_branch,
        ANSI_YELLOW,
        ANSI_RESET,
        title
    );
    println!("{}", body);
    println!();
    println!(
        "{}Preview will complete in {} seconds. Press Ctrl+C to abort.{}",
        ANSI_YELLOW, PR_PREVIEW_SECONDS, ANSI_RESET
    );
    io::stdout()
        .flush()
        .context("failed to flush preview output")?;

    wait_for_preview(cancel, PR_PREVIEW_SECONDS)?;
    println!();
    println!("PR preview complete. No pull request was created by this command.");

    Ok(())
}

fn wait_for_preview(cancel: Option<GitCancellation>, seconds: u64) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(seconds);
    while Instant::now() < deadline {
        if cancel.as_ref().is_some_and(|cancel| cancel.is_cancelled()) {
            bail!("cancelled by user");
        }
        thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct BranchRef {
    name: String,
    refname: String,
    object_id: String,
    root_distance: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct BranchLineage {
    root: BranchRef,
    path: Vec<BranchRef>,
}

fn resolve_parent_branch_name_with_cancel(
    repo_root: &str,
    current_branch: &str,
    custom_main_branch: Option<&str>,
    cancel: Option<GitCancellation>,
) -> Result<String> {
    let lineage =
        load_branch_lineage_with_cancel(repo_root, current_branch, custom_main_branch, cancel)?
            .ok_or_else(|| anyhow::anyhow!("no local branches are available in this repository"))?;
    if lineage.root.name.eq_ignore_ascii_case(current_branch) {
        bail!("current branch is already the main branch");
    }

    let current_index = lineage
        .path
        .iter()
        .position(|branch| branch.name.eq_ignore_ascii_case(current_branch))
        .ok_or_else(|| anyhow::anyhow!("current branch is not part of the current branch tree"))?;

    let target = if current_index == 0 {
        lineage.root.name
    } else {
        lineage.path[current_index - 1].name.clone()
    };
    Ok(target)
}

fn load_branch_lineage_with_cancel(
    repo_root: &str,
    current_branch: &str,
    custom_main_branch: Option<&str>,
    cancel: Option<GitCancellation>,
) -> Result<Option<BranchLineage>> {
    let Some(tree) = build_branch_tree_data_with_cancel(
        repo_root,
        current_branch,
        custom_main_branch,
        false,
        cancel,
    )?
    else {
        return Ok(None);
    };

    Ok(Some(BranchLineage {
        root: tree.root,
        path: tree.path,
    }))
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct BranchTreeData {
    root: BranchRef,
    family: Vec<BranchRef>,
    path: Vec<BranchRef>,
}

fn build_branch_tree_data_with_cancel(
    repo_root: &str,
    current_branch: &str,
    custom_main_branch: Option<&str>,
    focus_descendant_from_root: bool,
    cancel: Option<GitCancellation>,
) -> Result<Option<BranchTreeData>> {
    let mut branches = list_local_branch_refs_with_cancel(repo_root, cancel.clone())?;
    if branches.is_empty() {
        return Ok(None);
    }

    let root_index = select_root_branch_index(&branches, current_branch, custom_main_branch);
    let root_branch = branches.remove(root_index);
    populate_root_distances_with_cancel(
        repo_root,
        &root_branch.refname,
        &mut branches,
        cancel.clone(),
    )?;

    let current_ref = if root_branch.name.eq_ignore_ascii_case(current_branch) {
        if focus_descendant_from_root {
            select_branch_diagram_focus(repo_root, &root_branch, &branches)?
                .unwrap_or_else(|| root_branch.clone())
        } else {
            root_branch.clone()
        }
    } else {
        branches
            .iter()
            .find(|branch| branch.name.eq_ignore_ascii_case(current_branch))
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("current branch is not available among local refs"))?
    };

    let first_parent_commits =
        first_parent_commit_ids_with_cancel(repo_root, &current_ref.refname, cancel.clone())?;
    let merged_into_current = local_branch_names_merged_into_with_cancel(
        repo_root,
        &current_ref.refname,
        cancel.clone(),
    )?;
    let merged_into_root =
        local_branch_names_merged_into_with_cancel(repo_root, &root_branch.refname, cancel)?;

    let family = branches
        .into_iter()
        .filter(|branch| {
            let branch_lookup = normalize_lookup(&branch.name);
            merged_into_current.contains(&branch_lookup)
                && !merged_into_root.contains(&branch_lookup)
        })
        .collect::<Vec<_>>();

    let mut path = family
        .iter()
        .filter(|branch| first_parent_commits.contains(&branch.object_id))
        .cloned()
        .collect::<Vec<_>>();
    if !root_branch.name.eq_ignore_ascii_case(current_branch)
        && path
            .iter()
            .all(|branch| !branch.name.eq_ignore_ascii_case(current_branch))
    {
        path.push(current_ref);
    }
    sort_branch_path(&mut path, current_branch);

    Ok(Some(BranchTreeData {
        root: root_branch,
        family,
        path,
    }))
}

fn list_local_branch_refs_with_cancel(
    repo_root: &str,
    cancel: Option<GitCancellation>,
) -> Result<Vec<BranchRef>> {
    let output = run_git_checked_with_cancel(
        repo_root,
        &[
            "for-each-ref",
            "--format=%(refname:short)|%(refname)|%(objectname)",
            "refs/heads",
        ],
        cancel,
    )?;
    let mut branches = split_output_lines(&output)
        .into_iter()
        .filter_map(|line| {
            let mut parts = line.split('|');
            let name = parts.next()?.trim();
            let refname = parts.next()?.trim();
            let object_id = parts.next()?.trim();
            let name = name.trim();
            if name.is_empty() || refname.is_empty() || object_id.is_empty() {
                return None;
            }

            Some(BranchRef {
                name: name.to_string(),
                refname: refname.to_string(),
                object_id: object_id.to_string(),
                root_distance: 0,
            })
        })
        .collect::<Vec<_>>();
    branches.sort_by_cached_key(|branch| normalize_lookup(&branch.name));
    branches.dedup_by(|left, right| left.name.eq_ignore_ascii_case(&right.name));
    Ok(branches)
}

fn select_root_branch_index(
    branches: &[BranchRef],
    current_branch: &str,
    custom_main_branch: Option<&str>,
) -> usize {
    branches
        .iter()
        .position(|branch| {
            custom_main_branch.is_some_and(|custom| branch.name.eq_ignore_ascii_case(custom.trim()))
        })
        .or_else(|| {
            branches
                .iter()
                .position(|branch| branch.name.eq_ignore_ascii_case("main"))
        })
        .or_else(|| {
            branches
                .iter()
                .position(|branch| branch.name.eq_ignore_ascii_case("master"))
        })
        .or_else(|| {
            branches
                .iter()
                .position(|branch| branch.name.eq_ignore_ascii_case(current_branch))
        })
        .unwrap_or(0)
}

fn populate_root_distances_with_cancel(
    repo_root: &str,
    root_ref: &str,
    branches: &mut [BranchRef],
    cancel: Option<GitCancellation>,
) -> Result<()> {
    for branch in branches.iter_mut() {
        let range = format!("{}..{}", root_ref, branch.refname);
        let output = run_git_checked_with_cancel(
            repo_root,
            &["rev-list", "--count", &range],
            cancel.clone(),
        )?;
        branch.root_distance = output
            .trim()
            .parse::<usize>()
            .with_context(|| format!("failed to parse git ancestry distance for {}", range))?;
    }

    Ok(())
}

fn select_branch_diagram_focus(
    _repo_root: &str,
    _root_branch: &BranchRef,
    branches: &[BranchRef],
) -> Result<Option<BranchRef>> {
    let mut descendants = Vec::new();
    for branch in branches {
        if branch.root_distance == 0 {
            continue;
        }
        descendants.push(branch.clone());
    }

    descendants.sort_by(|left, right| {
        right
            .root_distance
            .cmp(&left.root_distance)
            .then_with(|| normalize_lookup(&left.name).cmp(&normalize_lookup(&right.name)))
    });
    Ok(descendants.into_iter().next())
}

fn sort_branch_path(path: &mut [BranchRef], current_branch: &str) {
    path.sort_by(|left, right| {
        let left_is_current = left.name.eq_ignore_ascii_case(current_branch);
        let right_is_current = right.name.eq_ignore_ascii_case(current_branch);
        left.root_distance
            .cmp(&right.root_distance)
            .then_with(|| left_is_current.cmp(&right_is_current).reverse())
            .then_with(|| normalize_lookup(&left.name).cmp(&normalize_lookup(&right.name)))
    });
}

fn first_parent_commit_ids_with_cancel(
    repo_root: &str,
    branch_ref: &str,
    cancel: Option<GitCancellation>,
) -> Result<HashSet<String>> {
    let output = run_git_checked_with_cancel(
        repo_root,
        &["rev-list", "--first-parent", branch_ref],
        cancel,
    )?;
    Ok(split_output_lines(&output).into_iter().collect())
}

fn local_branch_names_merged_into_with_cancel(
    repo_root: &str,
    descendant_ref: &str,
    cancel: Option<GitCancellation>,
) -> Result<HashSet<String>> {
    let output = run_git_checked_with_cancel(
        repo_root,
        &[
            "for-each-ref",
            "--merged",
            descendant_ref,
            "--format=%(refname:short)",
            "refs/heads",
        ],
        cancel,
    )?;
    Ok(split_output_lines(&output)
        .into_iter()
        .map(|branch| normalize_lookup(&branch))
        .collect())
}

fn normalize_lookup(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}
