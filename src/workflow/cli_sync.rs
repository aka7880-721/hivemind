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
        best_effort_canonicalize, find_project_for_cwd, find_scope_for_cwd,
        mirror_sync_after_merge_for_repo, project_root, scope_root,
    },
    config::{ConfigStore, ProjectConfig, ProjectType},
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

    let repo_root = sync_repo_root_for_cwd(project, &cwd)?;
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

fn sync_repo_root_for_cwd(project: &ProjectConfig, cwd: &Path) -> Result<std::path::PathBuf> {
    if project.project_type == ProjectType::AllInOne {
        return project_root(project);
    }

    let scope_index = find_scope_for_cwd(project, project, cwd)?;
    let branch = project
        .branches
        .get(scope_index)
        .with_context(|| format!("scope index {} is out of range", scope_index))?;
    scope_root(project, branch).with_context(|| {
        format!(
            "scope '{}' does not have a resolvable repository root",
            branch.display_name()
        )
    })
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
    if project.project_type == ProjectType::Branched
        && let Some(scope_name) = scope_display_name_for_repo_root(project, Path::new(repo_root))
    {
        println!("  Scope:   {ANSI_CYAN}{scope_name}{ANSI_RESET}");
    }
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

fn scope_display_name_for_repo_root(project: &ProjectConfig, repo_root: &Path) -> Option<String> {
    let canonical = best_effort_canonicalize(repo_root);
    project.branches.iter().find_map(|branch| {
        scope_root(project, branch).and_then(|root| {
            (best_effort_canonicalize(&root) == canonical)
                .then(|| branch.display_name().to_string())
        })
    })
}

fn configured_dual_remotes(
    project: &ProjectConfig,
    repo_root: &Path,
) -> Result<(Option<String>, Option<String>)> {
    if project.project_type == ProjectType::AllInOne {
        let repo = project.repo.as_ref().ok_or_else(|| {
            anyhow::anyhow!("GitLab+GitHub project is missing repo configuration")
        })?;
        return Ok((repo.remote_url.clone(), repo.secondary_remote_url.clone()));
    }

    let canonical = best_effort_canonicalize(repo_root);
    for branch in &project.branches {
        let Some(scope_root) = scope_root(project, branch) else {
            continue;
        };
        if best_effort_canonicalize(&scope_root) != canonical {
            continue;
        }
        let repo = branch
            .repo
            .as_ref()
            .or(project.repo.as_ref())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "GitLab+GitHub scope '{}' is missing repo configuration",
                    branch.display_name()
                )
            })?;
        return Ok((repo.remote_url.clone(), repo.secondary_remote_url.clone()));
    }

    if let Some(repo) = project.repo.as_ref()
        && best_effort_canonicalize(Path::new(repo.local_root.trim())) == canonical
    {
        return Ok((repo.remote_url.clone(), repo.secondary_remote_url.clone()));
    }

    bail!(
        "GitLab+GitHub project has no scope configured for repository '{}'",
        repo_root.display()
    )
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
    use std::path::{Path, PathBuf};

    use crate::config::{
        BranchConfig, BranchScopeKind, IntegrationMode, MirrorSyncAfterMerge, ProjectConfig,
        ProjectType, RepoConfig, TargetFormat, TargetSpec,
    };
    use crate::workflow::versioning::VersionScheme;

    #[test]
    fn dual_forge_mode_requires_sync_command() {
        assert!(IntegrationMode::GitLabGitHubEnabled.is_dual_forge());
        assert!(!IntegrationMode::GitLabEnabled.is_dual_forge());
    }

    #[test]
    fn mirror_sync_policy_default_runs_after_merge() {
        assert!(MirrorSyncAfterMerge::default().runs_automatically_after_merge());
    }

    fn dual_scope_project() -> ProjectConfig {
        ProjectConfig {
            name: "demo".to_string(),
            alias: String::new(),
            project_type: ProjectType::Branched,
            integration_mode: IntegrationMode::GitLabGitHubEnabled,
            unified_versioning: false,
            version_scheme: VersionScheme::SemVer,
            targets: Vec::new(),
            branches: vec![
                BranchConfig {
                    name: "core".to_string(),
                    label: "Core".to_string(),
                    scope_kind: BranchScopeKind::Branch,
                    repo: Some(RepoConfig {
                        local_root: "/tmp/demo/core".to_string(),
                        remote_url: Some("git@gitlab.com:org/core.git".to_string()),
                        secondary_remote_url: Some("git@github.com:org/core.git".to_string()),
                        ..RepoConfig::default()
                    }),
                    changelog_enabled: false,
                    changelog_path: None,
                    changelog_hide_pr_messages: false,
                    changelog_hide_bump_messages: false,
                    changelog_mini_commit_hashes: false,
                    changelog_mirror_summary_to_root_changelog: false,
                    changelog_wrap_detailed_if_top_picks: false,
                    release_now: crate::config::ReleaseNowSettings::default(),
                    version_scheme: VersionScheme::SemVer,
                    targets: vec![TargetSpec {
                        label: "Version".to_string(),
                        path: "Cargo.toml".to_string(),
                        key_path: "package.version".to_string(),
                        format: TargetFormat::Toml,
                    }],
                    advanced_alias: Default::default(),
                },
                BranchConfig {
                    name: "api".to_string(),
                    label: "API".to_string(),
                    scope_kind: BranchScopeKind::Service,
                    repo: Some(RepoConfig {
                        local_root: "/tmp/demo/api".to_string(),
                        remote_url: Some("git@gitlab.com:org/api.git".to_string()),
                        secondary_remote_url: Some("git@github.com:org/api.git".to_string()),
                        ..RepoConfig::default()
                    }),
                    changelog_enabled: false,
                    changelog_path: None,
                    changelog_hide_pr_messages: false,
                    changelog_hide_bump_messages: false,
                    changelog_mini_commit_hashes: false,
                    changelog_mirror_summary_to_root_changelog: false,
                    changelog_wrap_detailed_if_top_picks: false,
                    release_now: crate::config::ReleaseNowSettings::default(),
                    version_scheme: VersionScheme::SemVer,
                    targets: vec![TargetSpec {
                        label: "Version".to_string(),
                        path: "Cargo.toml".to_string(),
                        key_path: "package.version".to_string(),
                        format: TargetFormat::Toml,
                    }],
                    advanced_alias: Default::default(),
                },
            ],
            repo: None,
            ..Default::default()
        }
    }

    #[test]
    fn sync_repo_root_for_cwd_uses_matching_scope_not_core() {
        let project = dual_scope_project();
        let cwd = PathBuf::from("/tmp/demo/api/src");
        let repo_root =
            super::sync_repo_root_for_cwd(&project, &cwd).expect("api scope root should resolve");
        assert_eq!(repo_root, PathBuf::from("/tmp/demo/api"));
    }

    #[test]
    fn configured_dual_remotes_selects_scope_matching_repo_root() {
        let project = dual_scope_project();
        let (gitlab, github) = super::configured_dual_remotes(&project, Path::new("/tmp/demo/api"))
            .expect("api remotes should resolve");
        assert_eq!(gitlab.as_deref(), Some("git@gitlab.com:org/api.git"));
        assert_eq!(github.as_deref(), Some("git@github.com:org/api.git"));
    }
}
