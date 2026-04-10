// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyVersionBumper License v1.2
//
// For details, see the LICENSE file in the repository root.

use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};
use chrono::{Local, TimeZone};

use crate::{
    config::{BranchScopeKind, ProjectConfig, ProjectType},
    targets::{collect_bump_scopes, shared_bump_version},
};

pub(crate) struct GitOutput {
    pub(crate) success: bool,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
}

#[derive(Clone)]
pub(crate) struct GitScopeContext {
    pub(crate) display_name: String,
    pub(crate) scope_kind: Option<BranchScopeKind>,
    pub(crate) repo_root: String,
    pub(crate) remote_spec: Option<String>,
    pub(crate) suggested_tag_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RepoActivitySummary {
    pub(crate) commits_since_tag_label: String,
    pub(crate) last_bump_label: String,
    pub(crate) last_commit_label: String,
}

pub(crate) fn project_repo_root(project: &ProjectConfig) -> Result<String> {
    let repo = project
        .repo
        .as_ref()
        .ok_or_else(|| anyhow!("this project is local-only and has no git repository configured"))?;
    Ok(repo.local_root.clone())
}

pub(crate) fn suggested_tag_name(project: &ProjectConfig) -> String {
    suggested_tag_name_for_scope(project, None)
}

pub(crate) fn suggested_tag_name_for_scope(project: &ProjectConfig, scope_index: Option<usize>) -> String {
    if project.project_type == ProjectType::AllInOne || project.unified_versioning || scope_index.is_none() {
        if let Ok(scopes) = collect_bump_scopes(project) {
            if let Some(version) = shared_bump_version(&scopes) {
                return format!("v{}", version);
            }
        }
        return slugify(&project.name);
    }

    let Some(scope_index) = scope_index else {
        return slugify(&project.name);
    };

    let scope_slug = project
        .branches
        .get(scope_index)
        .map(|branch| slugify(branch.display_name()))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| slugify(&project.name));

    if let Ok(scopes) = collect_bump_scopes(project) {
        if let Some(scope) = scopes.get(scope_index) {
            if let Some(version) = &scope.current_version {
                return format!("{}-v{}", scope_slug, version);
            }
        }
    }

    scope_slug
}

pub(crate) fn collect_git_scope_contexts(project: &ProjectConfig) -> Result<Vec<GitScopeContext>> {
    if project.project_type == ProjectType::AllInOne || project.unified_versioning || project.branches.len() <= 1 {
        let repo_root = project_repo_root(project)?;
        let remote_spec = project.repo.as_ref().and_then(|repo| repo.remote_url.clone());
        return Ok(vec![GitScopeContext {
            display_name: project.name.clone(),
            scope_kind: None,
            repo_root,
            remote_spec,
            suggested_tag_name: suggested_tag_name(project),
        }]);
    }

    project
        .branches
        .iter()
        .enumerate()
        .map(|(index, branch)| {
            let repo = branch
                .repo
                .as_ref()
                .or(project.repo.as_ref())
                .ok_or_else(|| anyhow!("branch '{}' does not have a git repository configured", branch.display_name()))?;
            Ok(GitScopeContext {
                display_name: branch.display_name().to_string(),
                scope_kind: Some(branch.scope_kind),
                repo_root: repo.local_root.clone(),
                remote_spec: repo.remote_url.clone(),
                suggested_tag_name: suggested_tag_name_for_scope(project, Some(index)),
            })
        })
        .collect()
}

pub(crate) fn collect_all_branch_git_scope_contexts(project: &ProjectConfig) -> Result<Vec<GitScopeContext>> {
    if project.project_type == ProjectType::AllInOne || project.branches.len() <= 1 {
        return collect_git_scope_contexts(project);
    }

    project
        .branches
        .iter()
        .enumerate()
        .map(|(index, branch)| {
            let repo = branch
                .repo
                .as_ref()
                .or(project.repo.as_ref())
                .ok_or_else(|| anyhow!("branch '{}' does not have a git repository configured", branch.display_name()))?;
            Ok(GitScopeContext {
                display_name: branch.display_name().to_string(),
                scope_kind: Some(branch.scope_kind),
                repo_root: repo.local_root.clone(),
                remote_spec: repo.remote_url.clone(),
                suggested_tag_name: suggested_tag_name_for_scope(project, Some(index)),
            })
        })
        .collect()
}

fn slugify(value: &str) -> String {
    value
        .chars()
        .map(|character| if character.is_ascii_alphanumeric() { character.to_ascii_lowercase() } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

pub(crate) fn ensure_git_repo(repo_root: &str) -> Result<()> {
    let output = run_git_checked(repo_root, &["rev-parse", "--is-inside-work-tree"])?;
    if output.trim() == "true" {
        Ok(())
    } else {
        bail!("{} is not a git working tree", repo_root)
    }
}

pub(crate) fn ensure_local_tag(repo_root: &str, tag_name: &str, annotation: Option<&str>) -> Result<bool> {
    let existing = run_git_checked(repo_root, &["tag", "--list", tag_name])?;
    if existing.lines().any(|line| line.trim() == tag_name) {
        Ok(false)
    } else {
        if let Some(annotation) = annotation.filter(|annotation| !annotation.trim().is_empty()) {
            run_git_checked(repo_root, &["tag", "-a", tag_name, "-m", annotation])?;
        } else {
            run_git_checked(repo_root, &["tag", tag_name])?;
        }
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

pub(crate) fn load_repo_activity_summary(repo_root: &str) -> Result<RepoActivitySummary> {
    ensure_git_repo(repo_root)?;

    let describe = run_git(repo_root, &["describe", "--tags", "--abbrev=0"])?;
    let (commits_since_tag_label, last_bump_label) = if describe.success {
        let tag = describe.stdout.trim().to_string();
        let count = run_git_checked(repo_root, &["rev-list", "--count", &format!("{}..HEAD", tag)])?
            .trim()
            .to_string();
        let tag_timestamp = run_git_checked(repo_root, &["log", "-1", "--format=%ct", &tag])?;
        (
            format!("{}c ahd", count),
            format_relative_git_timestamp(tag_timestamp.trim()).unwrap_or_else(|| "n/a".to_string()),
        )
    } else {
        (
            "no tags".to_string(),
            "n/a".to_string(),
        )
    };

    let last_commit_timestamp = run_git_checked(repo_root, &["log", "-1", "--format=%ct", "HEAD"])?;
    let last_commit_label = format_relative_git_timestamp(last_commit_timestamp.trim())
        .unwrap_or_else(|| "n/a".to_string());

    Ok(RepoActivitySummary {
        commits_since_tag_label,
        last_bump_label,
        last_commit_label,
    })
}

fn format_relative_git_timestamp(timestamp: &str) -> Option<String> {
    let seconds = timestamp.parse::<i64>().ok()?;
    let then = Local.timestamp_opt(seconds, 0).single()?;
    let now = Local::now();
    let delta = now.signed_duration_since(then);
    let minutes = delta.num_minutes().max(0);

    let label = if minutes < 60 {
        format!("{}m ago", minutes.max(1))
    } else if minutes < 60 * 24 {
        format!("{}h ago", (minutes / 60).max(1))
    } else if minutes < 60 * 24 * 7 {
        format!("{}d ago", (minutes / (60 * 24)).max(1))
    } else if minutes < 60 * 24 * 365 {
        format!("{}w ago", (minutes / (60 * 24 * 7)).max(1))
    } else {
        format!("{}y ago", (minutes / (60 * 24 * 365)).max(1))
    };

    Some(label)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::{BranchConfig, IntegrationMode, RepoConfig, TargetFormat, TargetSpec},
        versioning::VersionScheme,
    };

    #[test]
    fn collect_git_scope_contexts_prefers_branch_repo_overrides() {
        let project = ProjectConfig {
            name: "demo".to_string(),
            project_type: ProjectType::Branched,
            integration_mode: IntegrationMode::GitHubEnabled,
            unified_versioning: false,
            version_scheme: VersionScheme::SemVer,
            targets: Vec::new(),
            branches: vec![
                BranchConfig {
                    name: "core".to_string(),
                    label: "Core".to_string(),
                    scope_kind: BranchScopeKind::Module,
                    repo: Some(RepoConfig {
                        local_root: "C:/repo/core".to_string(),
                        remote_url: Some("origin-core".to_string()),
                    }),
                    version_scheme: VersionScheme::SemVer,
                    targets: vec![TargetSpec {
                        label: "Version".to_string(),
                        path: "missing-core.toml".to_string(),
                        key_path: "package.version".to_string(),
                        format: TargetFormat::Toml,
                    }],
                },
                BranchConfig {
                    name: "api".to_string(),
                    label: "API".to_string(),
                    scope_kind: BranchScopeKind::Service,
                    repo: None,
                    version_scheme: VersionScheme::SemVer,
                    targets: vec![TargetSpec {
                        label: "Version".to_string(),
                        path: "missing-api.json".to_string(),
                        key_path: "version".to_string(),
                        format: TargetFormat::Json,
                    }],
                },
            ],
            repo: Some(RepoConfig {
                local_root: "C:/repo/project".to_string(),
                remote_url: Some("origin-project".to_string()),
            }),
        };

        let scopes = collect_git_scope_contexts(&project).expect("scoped git contexts should resolve");

        assert_eq!(scopes.len(), 2);
        assert_eq!(scopes[0].repo_root, "C:/repo/core");
        assert_eq!(scopes[0].remote_spec.as_deref(), Some("origin-core"));
        assert_eq!(scopes[1].repo_root, "C:/repo/project");
        assert_eq!(scopes[1].remote_spec.as_deref(), Some("origin-project"));
        assert_eq!(scopes[1].suggested_tag_name, "api");
    }

    #[test]
    fn collect_all_branch_git_scope_contexts_keeps_scopes_for_unified_projects() {
        let project = ProjectConfig {
            name: "demo".to_string(),
            project_type: ProjectType::Branched,
            integration_mode: IntegrationMode::GitLocalOnly,
            unified_versioning: true,
            version_scheme: VersionScheme::SemVer,
            targets: Vec::new(),
            branches: vec![
                BranchConfig {
                    name: "core".to_string(),
                    label: "Core".to_string(),
                    scope_kind: BranchScopeKind::Branch,
                    repo: Some(RepoConfig {
                        local_root: "C:/repo/core".to_string(),
                        remote_url: None,
                    }),
                    version_scheme: VersionScheme::SemVer,
                    targets: vec![TargetSpec {
                        label: "Version".to_string(),
                        path: "core/Cargo.toml".to_string(),
                        key_path: "package.version".to_string(),
                        format: TargetFormat::Toml,
                    }],
                },
                BranchConfig {
                    name: "api".to_string(),
                    label: "API".to_string(),
                    scope_kind: BranchScopeKind::Service,
                    repo: Some(RepoConfig {
                        local_root: "C:/repo/api".to_string(),
                        remote_url: None,
                    }),
                    version_scheme: VersionScheme::SemVer,
                    targets: vec![TargetSpec {
                        label: "Version".to_string(),
                        path: "api/package.json".to_string(),
                        key_path: "version".to_string(),
                        format: TargetFormat::Json,
                    }],
                },
            ],
            repo: None,
        };

        let scopes = collect_all_branch_git_scope_contexts(&project).expect("all branch scopes should resolve");

        assert_eq!(scopes.len(), 2);
        assert_eq!(scopes[0].display_name, "Core");
        assert_eq!(scopes[1].display_name, "API");
    }

    #[test]
    fn relative_git_timestamps_are_compacted() {
        let now = Local::now().timestamp();
        let two_days_ago = (now - 60 * 60 * 24 * 2).to_string();

        let formatted = format_relative_git_timestamp(&two_days_ago).expect("timestamp should format");

        assert_eq!(formatted, "2d ago");
    }
}