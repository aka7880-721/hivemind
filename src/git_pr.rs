// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit License v1.2
//
// For details, see the LICENSE file in the repository root.

use std::{
    collections::HashSet,
    env, fs,
    io::{self, Write},
    path::Path,
    process::Command,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
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

#[derive(Clone, Debug, PartialEq, Eq)]
struct PullRequestPlan {
    target_branch: String,
    current_branch: String,
    title: String,
    body: String,
}

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

    let plan = PullRequestPlan {
        title: format!("{} (via ComfyGit)", current_branch),
        body: build_pr_body(repo_root, &target_branch, &current_branch, cancel.clone())?,
        target_branch,
        current_branch,
    };

    preview_pr(&plan, cancel)?;
    let pr_url = create_pull_request(repo_root, &plan)?;

    println!();
    println!(
        "Created pull request: {}{}{}",
        ANSI_YELLOW, pr_url, ANSI_RESET
    );

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

fn preview_pr(plan: &PullRequestPlan, cancel: Option<GitCancellation>) -> Result<()> {
    println!();
    println!("{}", render_preview_summary(plan));
    println!();
    println!("{}", plan.body);
    println!();
    println!(
        "Flow proceeds automatically in {}{}{} seconds. Press Ctrl+C to abort.",
        ANSI_YELLOW, PR_PREVIEW_SECONDS, ANSI_RESET
    );
    io::stdout()
        .flush()
        .context("failed to flush preview output")?;

    wait_for_preview(cancel, PR_PREVIEW_SECONDS)?;
    println!();
    println!("Creating pull request...");

    Ok(())
}

fn render_preview_summary(plan: &PullRequestPlan) -> String {
    format!(
        "ComfyGit is about to create PR {}{}{} from the branch {}{}{} that will request merge into {}{}{}. Press Ctrl+C within {}{}{} seconds to abort.",
        ANSI_YELLOW,
        plan.title,
        ANSI_RESET,
        ANSI_YELLOW,
        plan.current_branch,
        ANSI_RESET,
        ANSI_YELLOW,
        plan.target_branch,
        ANSI_RESET,
        ANSI_YELLOW,
        PR_PREVIEW_SECONDS,
        ANSI_RESET,
    )
}

fn create_pull_request(repo_root: &str, plan: &PullRequestPlan) -> Result<String> {
    let body_file = write_pr_body_file(&plan.body)?;
    let args = build_pr_create_args(plan, &body_file);
    let output = Command::new("gh")
        .current_dir(repo_root)
        .args(&args)
        .output()
        .context(
            "failed to execute `gh pr create`; ensure GitHub CLI is installed and authenticated",
        )?;
    let _ = fs::remove_file(&body_file);

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let detail = if stderr.is_empty() { stdout } else { stderr };
        bail!("`gh pr create` failed: {}", detail);
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let pr_url = stdout
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .map(str::trim)
        .unwrap_or_default()
        .to_string();
    if pr_url.is_empty() {
        bail!("`gh pr create` succeeded but did not return a pull request URL");
    }

    Ok(pr_url)
}

fn write_pr_body_file(body: &str) -> Result<std::path::PathBuf> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before UNIX_EPOCH")?
        .as_nanos();
    let path = env::temp_dir().join(format!(
        "comfygit-pr-body-{}-{}.md",
        std::process::id(),
        timestamp
    ));
    fs::write(&path, body.trim_end())
        .with_context(|| format!("failed to write temporary PR body file {}", path.display()))?;
    Ok(path)
}

fn build_pr_create_args(plan: &PullRequestPlan, body_file: &Path) -> Vec<String> {
    vec![
        "pr".to_string(),
        "create".to_string(),
        "--base".to_string(),
        plan.target_branch.clone(),
        "--head".to_string(),
        plan.current_branch.clone(),
        "--title".to_string(),
        plan.title.clone(),
        "--body-file".to_string(),
        body_file.display().to_string(),
    ]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_summary_highlights_dynamic_values() {
        let plan = PullRequestPlan {
            target_branch: "v0.13.x".to_string(),
            current_branch: "v0.13.1-dev".to_string(),
            title: "v0.13.1-dev (via ComfyGit)".to_string(),
            body: "body".to_string(),
        };

        let summary = render_preview_summary(&plan);

        assert!(summary.contains(&format!("{}{}{}", ANSI_YELLOW, plan.title, ANSI_RESET)));
        assert!(summary.contains(&format!(
            "{}{}{}",
            ANSI_YELLOW, plan.current_branch, ANSI_RESET
        )));
        assert!(summary.contains(&format!(
            "{}{}{}",
            ANSI_YELLOW, plan.target_branch, ANSI_RESET
        )));
    }

    #[test]
    fn build_pr_create_args_uses_non_interactive_flags() {
        let plan = PullRequestPlan {
            target_branch: "main".to_string(),
            current_branch: "feature/demo".to_string(),
            title: "feature/demo (via ComfyGit)".to_string(),
            body: "body".to_string(),
        };
        let body_file = Path::new("pr_temp.md");

        let args = build_pr_create_args(&plan, body_file);

        assert_eq!(
            args,
            vec![
                "pr",
                "create",
                "--base",
                "main",
                "--head",
                "feature/demo",
                "--title",
                "feature/demo (via ComfyGit)",
                "--body-file",
                "pr_temp.md",
            ]
        );
    }
}
