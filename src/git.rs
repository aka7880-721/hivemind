// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyVersionBumper License v1.2
//
pub(crate) fn current_branch(repo_root: &str) -> Result<String> {
    let branch = run_git_checked(repo_root, &["branch", "--show-current"])?;
    let branch = branch.trim();
    if !branch.is_empty() {
        return Ok(branch.to_string());
    }

    let head = run_git_checked(repo_root, &["rev-parse", "--short", "HEAD"])?;
    Ok(format!("detached ({})", head.trim()))
}

pub(crate) fn switch_to_main_branch(repo_root: &str, remote_spec: Option<&str>, sync_remote: bool) -> Result<()> {
    let switch_output = run_git(repo_root, &["switch", "main"])?;
    if !switch_output.success {
        run_git_checked(repo_root, &["checkout", "main"])?;
    }

    if sync_remote {
        let remote_spec = remote_spec.ok_or_else(|| anyhow!("no remote is configured for this project"))?;
        run_git_checked(repo_root, &["pull", "--ff-only", remote_spec, "main"])?;
    }

    Ok(())
}
// For details, see the LICENSE file in the repository root.

/// Git-related utilities for interacting with repositories, collecting activity summaries, and managing tags.

use std::{path::Path, process::Command};

use anyhow::{Context, Result, anyhow, bail};
use chrono::{Local, TimeZone};

use crate::{
    config::{BranchScopeKind, ProjectConfig, ProjectType, TargetSpec},
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
    pub(crate) path_filters: Vec<String>,
}

impl GitScopeContext {
    pub(crate) fn git_pathspecs(&self) -> Vec<String> {
        let repo_root = Path::new(&self.repo_root);
        self.path_filters
            .iter()
            .filter_map(|path| normalize_pathspec(repo_root, path))
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RepoActivitySummary {
    pub(crate) commits_since_tag_label: String,
    pub(crate) last_bump_label: String,
    pub(crate) last_commit_label: String,
}

fn build_git_args(base: &[&str], pathspecs: &[String]) -> Vec<String> {
    let mut args = base.iter().map(|arg| (*arg).to_string()).collect::<Vec<_>>();
    if !pathspecs.is_empty() {
        args.push("--".to_string());
        args.extend(pathspecs.iter().cloned());
    }
    args
}

fn run_git_checked_owned(repo_root: &str, args: Vec<String>) -> Result<String> {
    let arg_refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    run_git_checked(repo_root, &arg_refs)
}

fn derive_repo_root_from_targets(specs: &[TargetSpec]) -> Option<String> {
    specs.iter()
        .find_map(|target| {
            let trimmed = target.path.trim();
            if trimmed.is_empty() {
                return None;
            }

            Path::new(trimmed)
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .map(|parent| parent.display().to_string())
        })
}

fn resolve_scope_repo_root(
    project_repo: Option<&crate::config::RepoConfig>,
    branch_repo: Option<&crate::config::RepoConfig>,
    targets: &[TargetSpec],
) -> Result<String> {
    if let Some(repo) = branch_repo {
        return Ok(repo.local_root.clone());
    }
    if let Some(repo_root) = derive_repo_root_from_targets(targets) {
        return Ok(repo_root);
    }
    if let Some(repo) = project_repo {
        return Ok(repo.local_root.clone());
    }
    bail!("scope does not have a git repository configured and no repo root could be derived from its target paths")
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
    if project.project_type == ProjectType::AllInOne {
        let repo_root = project_repo_root(project)?;
        let remote_spec = project.repo.as_ref().and_then(|repo| repo.remote_url.clone());
        return Ok(vec![GitScopeContext {
            display_name: project.name.clone(),
            scope_kind: None,
            repo_root,
            remote_spec,
            suggested_tag_name: suggested_tag_name(project),
            path_filters: project_scope_target_paths(project),
        }]);
    }

    if project.unified_versioning || project.branches.len() <= 1 {
        let branch = project
            .branches
            .first()
            .ok_or_else(|| anyhow!("branched project does not contain any scopes"))?;
        let repo_root = resolve_scope_repo_root(project.repo.as_ref(), branch.repo.as_ref(), &branch.targets)?;
        let remote_spec = branch
            .repo
            .as_ref()
            .or(project.repo.as_ref())
            .and_then(|repo| repo.remote_url.clone());
        return Ok(vec![GitScopeContext {
            display_name: project.name.clone(),
            scope_kind: None,
            repo_root,
            remote_spec,
            suggested_tag_name: suggested_tag_name(project),
            path_filters: project_scope_target_paths(project),
        }]);
    }

    project
        .branches
        .iter()
        .enumerate()
        .map(|(index, branch)| {
            let repo_root = resolve_scope_repo_root(project.repo.as_ref(), branch.repo.as_ref(), &branch.targets)?;
            let repo = branch.repo.as_ref().or(project.repo.as_ref());
            Ok(GitScopeContext {
                display_name: branch.display_name().to_string(),
                scope_kind: Some(branch.scope_kind),
                repo_root,
                remote_spec: repo.and_then(|repo| repo.remote_url.clone()),
                suggested_tag_name: suggested_tag_name_for_scope(project, Some(index)),
                path_filters: collect_target_paths(&branch.targets),
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
            let repo_root = resolve_scope_repo_root(project.repo.as_ref(), branch.repo.as_ref(), &branch.targets)?;
            let repo = branch.repo.as_ref().or(project.repo.as_ref());
            Ok(GitScopeContext {
                display_name: branch.display_name().to_string(),
                scope_kind: Some(branch.scope_kind),
                repo_root,
                remote_spec: repo.and_then(|repo| repo.remote_url.clone()),
                suggested_tag_name: suggested_tag_name_for_scope(project, Some(index)),
                path_filters: collect_target_paths(&branch.targets),
            })
        })
        .collect()
}

fn project_scope_target_paths(project: &ProjectConfig) -> Vec<String> {
    if project.project_type == ProjectType::AllInOne {
        collect_target_paths(&project.targets)
    } else {
        project
            .branches
            .iter()
            .flat_map(|branch| collect_target_paths(&branch.targets))
            .collect()
    }
}

fn collect_target_paths(specs: &[TargetSpec]) -> Vec<String> {
    let mut paths = Vec::new();
    for target in specs {
        let Some(filter) = activity_filter_for_target(&target.path) else {
            continue;
        };
        if !paths.iter().any(|existing| existing == &filter) {
            paths.push(filter);
        }
    }
    paths
}

fn activity_filter_for_target(path: &str) -> Option<String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return None;
    }

    let parent = Path::new(trimmed).parent()?;
    if parent.as_os_str().is_empty() || parent == Path::new(".") {
        None
    } else {
        Some(parent.display().to_string())
    }
}

fn normalize_pathspec(repo_root: &Path, path: &str) -> Option<String> {
    let candidate = Path::new(path);
    let relative = if candidate.is_absolute() {
        candidate.strip_prefix(repo_root).ok()?
    } else {
        candidate
    };

    let rendered = relative.to_string_lossy().replace('\\', "/");
    (!rendered.is_empty()).then_some(rendered)
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

pub(crate) fn split_output_lines(output: &str) -> Vec<String> {
    output
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

pub(crate) fn load_scope_activity_summary(scope: &GitScopeContext) -> Result<RepoActivitySummary> {
    let repo_root = &scope.repo_root;
    let pathspecs = scope.git_pathspecs();
    ensure_git_repo(repo_root)?;

    let describe = run_git(repo_root, &["describe", "--tags", "--abbrev=0"])?;
    let (commits_since_tag_label, last_bump_label) = if describe.success {
        let tag = describe.stdout.trim().to_string();
        let range = format!("{}..HEAD", tag);
        let count = run_git_checked_owned(
            repo_root,
            build_git_args(&["rev-list", "--count", range.as_str()], &pathspecs),
        )?
        .trim()
        .to_string();
        let tag_timestamp = run_git_checked(repo_root, &["log", "-1", "--format=%ct", &tag])?;
        (
            format!("{}c ahd", count),
            format_relative_git_timestamp(tag_timestamp.trim()).unwrap_or_else(|| "n/a".to_string()),
        )
    } else {
        ("no tags".to_string(), "n/a".to_string())
    };

    let last_commit_timestamp = run_git_checked_owned(
        repo_root,
        build_git_args(&["log", "-1", "--format=%ct", "HEAD"], &pathspecs),
    )?;
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
        config::{BranchConfig, ChangelogSettings, IntegrationMode, RepoConfig, TargetFormat, TargetSpec},
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
            changelog: ChangelogSettings::default(),
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
        assert!(scopes[0].path_filters.is_empty());
        assert_eq!(scopes[1].repo_root, "C:/repo/project");
        assert_eq!(scopes[1].remote_spec.as_deref(), Some("origin-project"));
        assert_eq!(scopes[1].suggested_tag_name, "api");
        assert!(scopes[1].path_filters.is_empty());
    }

    #[test]
    fn collect_all_branch_git_scope_contexts_keeps_scopes_for_unified_projects() {
        let project = ProjectConfig {
            name: "demo".to_string(),
            project_type: ProjectType::Branched,
            integration_mode: IntegrationMode::GitLocalOnly,
            unified_versioning: true,
            version_scheme: VersionScheme::SemVer,
            changelog: ChangelogSettings::default(),
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
        assert_eq!(scopes[0].path_filters, vec!["core"]);
        assert_eq!(scopes[1].path_filters, vec!["api"]);
    }

    #[test]
    fn git_pathspecs_normalize_inside_repo_paths() {
        let scope = GitScopeContext {
            display_name: "Core".to_string(),
            scope_kind: Some(BranchScopeKind::Module),
            repo_root: "C:/repo".to_string(),
            remote_spec: None,
            suggested_tag_name: "core-v1.2.3".to_string(),
            path_filters: vec!["C:/repo/core".to_string(), "core\\nested".to_string()],
        };

        assert_eq!(scope.git_pathspecs(), vec!["core", "core/nested"]);
    }

    #[test]
    fn collect_scope_context_derives_repo_root_from_target_path() {
        let project = ProjectConfig {
            name: "demo".to_string(),
            project_type: ProjectType::Branched,
            integration_mode: IntegrationMode::GitLocalOnly,
            unified_versioning: false,
            version_scheme: VersionScheme::SemVer,
            changelog: ChangelogSettings::default(),
            targets: Vec::new(),
            branches: vec![BranchConfig {
                name: "core".to_string(),
                label: "Core".to_string(),
                scope_kind: BranchScopeKind::Branch,
                repo: None,
                version_scheme: VersionScheme::SemVer,
                targets: vec![TargetSpec {
                    label: "Version".to_string(),
                    path: "C:/repo/core/Cargo.toml".to_string(),
                    key_path: "package.version".to_string(),
                    format: TargetFormat::Toml,
                }],
            }],
            repo: Some(RepoConfig {
                local_root: "C:/repo".to_string(),
                remote_url: Some("origin".to_string()),
            }),
        };

        let scopes = collect_all_branch_git_scope_contexts(&project).expect("scope contexts");

        assert_eq!(scopes[0].repo_root, "C:/repo/core");
    }

    #[test]
    fn relative_git_timestamps_are_compacted() {
        let now = Local::now().timestamp();
        let two_days_ago = (now - 60 * 60 * 24 * 2).to_string();

        let formatted = format_relative_git_timestamp(&two_days_ago).expect("timestamp should format");

        assert_eq!(formatted, "2d ago");
    }
}