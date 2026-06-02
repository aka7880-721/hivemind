// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit License v1.2

use std::{
    env,
    io::{self, Write},
    path::Path,
};

use anyhow::{Context, Result, bail};

use crate::{
    cli::{
        best_effort_canonicalize, find_project_for_cwd, mirror_sync_after_merge_for_repo,
        project_root,
    },
    config::{ConfigStore, ProjectConfig},
    git::{check_mirror_sync, push_mirror_sync},
};

const ANSI_CYAN: &str = "\x1b[36m";
const ANSI_GREEN: &str = "\x1b[32m";
const ANSI_YELLOW: &str = "\x1b[33m";
const ANSI_DARK_GREY: &str = "\x1b[90m";
const ANSI_RESET: &str = "\x1b[0m";

pub(crate) fn run_sync() -> Result<()> {
    run_sync_with_options(false)
}

pub(crate) fn run_sync_with_options(skip_confirm: bool) -> Result<()> {
    let cwd =
        best_effort_canonicalize(&env::current_dir().context("failed to read current directory")?);
    let config = ConfigStore::locate()?.load()?;
    let project = find_project_for_cwd(&config.projects, &cwd).map_err(|_| {
        anyhow::anyhow!(
            "no ComfyGit project covers the current directory; run 'cg init' or open the TUI from a registered project root"
        )
    })?;

    if !project.integration_mode.is_dual_forge() {
        bail!(
            "cg sync requires a GitLab+GitHub project (current mode: {})",
            project.integration_mode.display_name()
        );
    }

    let repo_root = project_root(project)?;
    run_mirror_sync_for_project(project, &repo_root, skip_confirm)
}

/// After a ComfyGit MR/PR merge, push to both remotes when the project opts in.
pub(crate) fn run_mirror_sync_after_comfygit_merge(repo_root: &str) -> Result<()> {
    let cwd =
        best_effort_canonicalize(&env::current_dir().context("failed to read current directory")?);
    let config = ConfigStore::locate()?.load()?;
    let policy = mirror_sync_after_merge_for_repo(&config.projects, repo_root, &cwd);
    if !policy.runs_automatically_after_merge() {
        return Ok(());
    }

    let canonical_repo_root = best_effort_canonicalize(Path::new(repo_root));
    let project = find_project_for_cwd(&config.projects, &cwd)
        .or_else(|_| find_project_for_cwd(&config.projects, &canonical_repo_root))?;
    if !project.integration_mode.is_dual_forge() {
        return Ok(());
    }

    run_mirror_sync_for_project(project, &canonical_repo_root, true)
}

fn run_mirror_sync_for_project(
    project: &ProjectConfig,
    repo_root: &Path,
    skip_confirm: bool,
) -> Result<()> {
    let (gitlab_remote, github_remote) = configured_dual_remotes(project, repo_root)?;
    let repo_root = repo_root
        .to_str()
        .context("repo root path is not valid UTF-8")?;

    if skip_confirm {
        return run_mirror_sync_push(repo_root, gitlab_remote, github_remote, skip_confirm);
    }

    println!();
    println!("{ANSI_CYAN}ComfyGit mirror sync{ANSI_RESET} {ANSI_DARK_GREY}(cg sync){ANSI_RESET}");
    println!();
    println!("  Project: {ANSI_CYAN}{}{ANSI_RESET}", project.name);
    println!("  Repo:    {ANSI_DARK_GREY}{}{ANSI_RESET}", repo_root);

    run_mirror_sync_push(repo_root, gitlab_remote, github_remote, skip_confirm)
}

fn run_mirror_sync_push(
    repo_root: &str,
    gitlab_remote: Option<String>,
    github_remote: Option<String>,
    skip_confirm: bool,
) -> Result<()> {
    let report = check_mirror_sync(
        repo_root,
        gitlab_remote.as_deref(),
        github_remote.as_deref(),
    )?;
    for line in report.summary_lines() {
        println!("  {line}");
    }
    println!();

    if report.in_sync() {
        println!("{ANSI_GREEN}GitLab and GitHub remotes are already in sync.{ANSI_RESET}");
        return Ok(());
    }

    if !skip_confirm && !prompt_yes_no("Push the current branch to both remotes now?", false)? {
        println!("Sync skipped.");
        return Ok(());
    }

    let lines = push_mirror_sync(
        repo_root,
        gitlab_remote.as_deref(),
        github_remote.as_deref(),
    )?;
    for line in lines {
        println!("  {ANSI_GREEN}{line}{ANSI_RESET}");
    }

    let follow_up = check_mirror_sync(
        repo_root,
        gitlab_remote.as_deref(),
        github_remote.as_deref(),
    )?;
    println!();
    if follow_up.in_sync() {
        println!("{ANSI_GREEN}GitLab and GitHub remotes are now in sync.{ANSI_RESET}");
    } else {
        println!("{ANSI_YELLOW}Sync finished, but remotes still appear out of sync.{ANSI_RESET}");
        for line in follow_up.summary_lines() {
            println!("  {line}");
        }
    }
    Ok(())
}

fn configured_dual_remotes(
    project: &crate::config::ProjectConfig,
    repo_root: &Path,
) -> Result<(Option<String>, Option<String>)> {
    if project.project_type == crate::config::ProjectType::AllInOne {
        let repo = project.repo.as_ref().ok_or_else(|| {
            anyhow::anyhow!("GitLab+GitHub project is missing repo configuration")
        })?;
        return Ok((repo.remote_url.clone(), repo.secondary_remote_url.clone()));
    }

    if let Some(branch) = project.branches.first() {
        let repo = branch
            .repo
            .as_ref()
            .or(project.repo.as_ref())
            .ok_or_else(|| {
                anyhow::anyhow!("GitLab+GitHub project is missing repo configuration")
            })?;
        return Ok((repo.remote_url.clone(), repo.secondary_remote_url.clone()));
    }

    let _ = repo_root;
    bail!("GitLab+GitHub project is missing repo configuration")
}

fn prompt_yes_no(prompt: &str, default_yes: bool) -> Result<bool> {
    let suffix = if default_yes { "[Y/n]" } else { "[y/N]" };
    print!("{prompt} {suffix} ");
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    let answer = line.trim().to_ascii_lowercase();
    if answer.is_empty() {
        return Ok(default_yes);
    }
    Ok(matches!(answer.as_str(), "y" | "yes"))
}

#[cfg(test)]
mod tests {
    use crate::config::{IntegrationMode, MirrorSyncAfterMerge};

    #[test]
    fn dual_forge_mode_requires_sync_command() {
        assert!(IntegrationMode::GitLabGitHubEnabled.is_dual_forge());
        assert!(!IntegrationMode::GitLabEnabled.is_dual_forge());
    }

    #[test]
    fn mirror_sync_policy_default_runs_after_merge() {
        assert!(MirrorSyncAfterMerge::default().runs_automatically_after_merge());
    }
}
