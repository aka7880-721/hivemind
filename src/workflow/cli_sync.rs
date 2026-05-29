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
    cli::{best_effort_canonicalize, find_project_for_cwd, project_root},
    config::ConfigStore,
    git::{check_mirror_sync, push_mirror_sync},
};

const ANSI_CYAN: &str = "\x1b[36m";
const ANSI_GREEN: &str = "\x1b[32m";
const ANSI_YELLOW: &str = "\x1b[33m";
const ANSI_DARK_GREY: &str = "\x1b[90m";
const ANSI_RESET: &str = "\x1b[0m";

pub(crate) fn run_sync() -> Result<()> {
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
    let (gitlab_remote, github_remote) = configured_dual_remotes(project, &repo_root)?;
    println!();
    println!("{ANSI_CYAN}ComfyGit mirror sync{ANSI_RESET} {ANSI_DARK_GREY}(cg sync){ANSI_RESET}");
    println!();
    println!("  Project: {ANSI_CYAN}{}{ANSI_RESET}", project.name);
    println!(
        "  Repo:    {ANSI_DARK_GREY}{}{ANSI_RESET}",
        repo_root.display()
    );

    let report = check_mirror_sync(
        repo_root
            .to_str()
            .context("repo root path is not valid UTF-8")?,
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

    if !prompt_yes_no("Push the current branch to both remotes now?", false)? {
        println!("Sync skipped.");
        return Ok(());
    }

    let lines = push_mirror_sync(
        repo_root
            .to_str()
            .context("repo root path is not valid UTF-8")?,
        gitlab_remote.as_deref(),
        github_remote.as_deref(),
    )?;
    for line in lines {
        println!("  {ANSI_GREEN}{line}{ANSI_RESET}");
    }

    let follow_up = check_mirror_sync(
        repo_root
            .to_str()
            .context("repo root path is not valid UTF-8")?,
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
    #[test]
    fn dual_forge_mode_requires_sync_command() {
        assert!(crate::config::IntegrationMode::GitLabGitHubEnabled.is_dual_forge());
        assert!(!crate::config::IntegrationMode::GitLabEnabled.is_dual_forge());
    }
}
