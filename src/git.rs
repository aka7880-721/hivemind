// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyVersionBumper License v1.2
//
// For details, see the LICENSE file in the repository root.

use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};

use crate::{config::ProjectConfig, targets::collect_bump_targets};

pub(crate) struct GitOutput {
    pub(crate) success: bool,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
}

pub(crate) fn project_repo_root(project: &ProjectConfig) -> Result<String> {
    let repo = project
        .repo
        .as_ref()
        .ok_or_else(|| anyhow!("this project is local-only and has no git repository configured"))?;
    Ok(repo.local_root.clone())
}

pub(crate) fn suggested_tag_name(project: &ProjectConfig) -> String {
    if let Ok(targets) = collect_bump_targets(project) {
        if let Some(first) = targets.first() {
            if targets.iter().all(|target| target.current_version == first.current_version) {
                return format!("v{}", first.current_version);
            }
        }
    }

    let fallback = project
        .name
        .chars()
        .map(|character| if character.is_ascii_alphanumeric() { character.to_ascii_lowercase() } else { '-' })
        .collect::<String>();
    fallback.trim_matches('-').to_string()
}

pub(crate) fn ensure_git_repo(repo_root: &str) -> Result<()> {
    let output = run_git_checked(repo_root, &["rev-parse", "--is-inside-work-tree"])?;
    if output.trim() == "true" {
        Ok(())
    } else {
        bail!("{} is not a git working tree", repo_root)
    }
}

pub(crate) fn ensure_local_tag(repo_root: &str, tag_name: &str) -> Result<bool> {
    let existing = run_git_checked(repo_root, &["tag", "--list", tag_name])?;
    if existing.lines().any(|line| line.trim() == tag_name) {
        Ok(false)
    } else {
        run_git_checked(repo_root, &["tag", tag_name])?;
        Ok(true)
    }
}

pub(crate) fn ensure_gh_available() -> Result<()> {
    let output = Command::new("gh")
        .arg("--version")
        .output()
        .context("failed to invoke gh; install GitHub CLI to create releases")?;
    if output.status.success() {
        Ok(())
    } else {
        bail!("gh is not available or not functioning; install GitHub CLI to create releases")
    }
}

pub(crate) fn run_git(repo_root: &str, args: &[&str]) -> Result<GitOutput> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(args)
        .output()
        .with_context(|| format!("failed to run git in {}", repo_root))?;

    Ok(GitOutput {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

pub(crate) fn run_git_checked(repo_root: &str, args: &[&str]) -> Result<String> {
    let output = run_git(repo_root, args)?;
    if output.success {
        Ok(output.stdout)
    } else {
        let details = output.stderr.trim();
        if details.is_empty() {
            bail!("git {:?} failed in {}", args, repo_root)
        } else {
            bail!("git {:?} failed in {}: {}", args, repo_root, details)
        }
    }
}

pub(crate) fn run_gh_checked(repo_root: &str, args: &[&str]) -> Result<String> {
    let output = Command::new("gh")
        .current_dir(repo_root)
        .args(args)
        .output()
        .with_context(|| format!("failed to run gh in {}", repo_root))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        let details = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if details.is_empty() {
            bail!("gh {:?} failed in {}", args, repo_root)
        } else {
            bail!("gh {:?} failed in {}: {}", args, repo_root, details)
        }
    }
}

pub(crate) fn split_output_lines(output: &str) -> Vec<String> {
    output
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}