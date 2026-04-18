// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit License v1.2
//
// For details, see the LICENSE file in the repository root.

/// Git-related workflow operations for applying version bumps across repositories, managing staged changes, and ensuring tag consistency.
use super::*;
use crate::git::{
    GitCancellation, current_branch_with_cancel, run_git_checked_with_cancel, switch_to_main_branch,
};

#[derive(Clone)]
pub(super) struct RepoBranchState {
    pub(super) repo_root: String,
    pub(super) current_branch: String,
    pub(super) remote_spec: Option<String>,
}

pub(super) fn collect_repo_bump_operations(
    _project: &ProjectConfig,
    scopes: &[BumpScope],
    git_contexts: &[crate::git::GitScopeContext],
    affected_scope_indexes: &[usize],
) -> Result<Vec<RepoBumpOperation>> {
    let mut operations = Vec::<RepoBumpOperation>::new();

    for scope_index in affected_scope_indexes {
        let scope = scopes
            .get(*scope_index)
            .ok_or_else(|| anyhow!("the selected scope does not exist"))?;
        let context = git_contexts
            .get(*scope_index)
            .or_else(|| git_contexts.first())
            .ok_or_else(|| {
                anyhow!("git scope metadata is unavailable for the selected bump targets")
            })?;
        let stage_paths = collect_stage_paths_for_targets(&context.repo_root, &scope.targets);

        if let Some(existing) = operations
            .iter_mut()
            .find(|operation| operation.repo_root == context.repo_root)
        {
            for path in stage_paths {
                if !existing
                    .stage_paths
                    .iter()
                    .any(|candidate| candidate == &path)
                {
                    existing.stage_paths.push(path);
                }
            }
        } else {
            operations.push(RepoBumpOperation {
                repo_root: context.repo_root.clone(),
                remote_spec: context.remote_spec.clone(),
                stage_paths,
            });
        }
    }

    Ok(operations)
}

pub(super) fn apply_repo_bump_workflow(
    operations: &[RepoBumpOperation],
    next_version: &str,
    workflow: OverviewBumpWorkflow,
) -> Result<()> {
    let commit_message = format!("bump: CG version bump to {}", next_version);

    for operation in operations {
        if !operation.stage_paths.is_empty() {
            let mut add_args = vec!["add".to_string(), "--".to_string()];
            add_args.extend(operation.stage_paths.iter().cloned());
            run_git_checked_owned(&operation.repo_root, add_args)?;
        }

        if has_staged_changes(&operation.repo_root)? {
            run_git_checked(&operation.repo_root, &["commit", "-m", &commit_message])?;
        }

        if workflow.requires_tag() {
            ensure_local_tag(&operation.repo_root, next_version, None)?;
        }

        if workflow.requires_push() {
            let remote_spec = operation
                .remote_spec
                .as_deref()
                .ok_or_else(|| anyhow!("no remote is configured for this project"))?;
            run_git_checked(&operation.repo_root, &["push", remote_spec])?;
            if workflow.requires_tag() {
                run_git_checked(&operation.repo_root, &["push", remote_spec, next_version])?;
            }
        }
    }

    Ok(())
}

pub(super) fn collect_non_main_repo_states_with_cancel(
    project: &ProjectConfig,
    scopes: &[BumpScope],
    git_contexts: &[crate::git::GitScopeContext],
    affected_scope_indexes: &[usize],
    cancel: Option<GitCancellation>,
) -> Result<Vec<RepoBranchState>> {
    let operations =
        collect_repo_bump_operations(project, scopes, git_contexts, affected_scope_indexes)?;
    let mut repo_states = Vec::new();

    for operation in operations {
        let current_branch = current_branch_with_cancel(&operation.repo_root, cancel.clone())?;
        if current_branch != "main" {
            repo_states.push(RepoBranchState {
                repo_root: operation.repo_root,
                current_branch,
                remote_spec: operation.remote_spec,
            });
        }
    }

    Ok(repo_states)
}

pub(super) fn switch_repos_to_main(
    repos: &[RepoBranchState],
    integration_mode: IntegrationMode,
) -> Result<()> {
    let sync_remote = integration_mode == IntegrationMode::GitHubEnabled;
    for repo in repos {
        switch_to_main_branch(&repo.repo_root, repo.remote_spec.as_deref(), sync_remote)?;
    }
    Ok(())
}

fn has_staged_changes(repo_root: &str) -> Result<bool> {
    Ok(!run_git(repo_root, &["diff", "--cached", "--quiet", "--exit-code"])?.success)
}

fn staged_paths_with_cancel(
    repo_root: &str,
    cancel: Option<GitCancellation>,
) -> Result<Vec<String>> {
    Ok(split_output_lines(&run_git_checked_with_cancel(
        repo_root,
        &["diff", "--cached", "--name-only", "--diff-filter=ACMR"],
        cancel,
    )?))
}

pub(super) fn collect_unexpected_staged_paths_with_cancel(
    operations: &[RepoBumpOperation],
    cancel: Option<GitCancellation>,
) -> Result<Vec<UnexpectedStagedRepo>> {
    let mut warnings = Vec::new();

    for operation in operations {
        let expected = operation
            .stage_paths
            .iter()
            .map(|path| comparable_git_path(path))
            .collect::<HashSet<_>>();
        let extra_paths = staged_paths_with_cancel(&operation.repo_root, cancel.clone())?
            .into_iter()
            .filter(|path| !expected.contains(&comparable_git_path(path)))
            .collect::<Vec<_>>();
        if !extra_paths.is_empty() {
            warnings.push(UnexpectedStagedRepo {
                repo_root: operation.repo_root.clone(),
                extra_paths,
            });
        }
    }

    Ok(warnings)
}

pub(super) fn unstage_paths(repo_root: &str, paths: &[String]) -> Result<()> {
    if paths.is_empty() {
        return Ok(());
    }

    let mut args = vec![
        "restore".to_string(),
        "--staged".to_string(),
        "--".to_string(),
    ];
    args.extend(paths.iter().cloned());
    run_git_checked_owned(repo_root, args)?;
    Ok(())
}

pub(super) fn collect_stage_paths_for_targets(
    repo_root: &str,
    targets: &[BumpTarget],
) -> Vec<String> {
    let mut paths = Vec::new();

    for target in targets {
        push_stage_path(&mut paths, repo_root, &target.path);
        if target.format == TargetFormat::Toml {
            let target_path = resolve_repo_path(repo_root, &target.path);
            let is_cargo_manifest = target_path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.eq_ignore_ascii_case("Cargo.toml"));
            if is_cargo_manifest {
                let lock_path = target_path.with_file_name("Cargo.lock");
                if lock_path.is_file() {
                    push_stage_path(&mut paths, repo_root, &lock_path.display().to_string());
                }
            }
        }
    }

    paths
}

pub(super) fn append_repo_stage_paths(
    operations: &mut [RepoBumpOperation],
    repo_root: &str,
    paths: &[String],
) {
    if let Some(operation) = operations
        .iter_mut()
        .find(|operation| operation.repo_root == repo_root)
    {
        for path in paths {
            if !operation
                .stage_paths
                .iter()
                .any(|existing| existing == path)
            {
                operation.stage_paths.push(path.clone());
            }
        }
    }
}

pub(super) fn stage_path_for_file(repo_root: &str, path: &str) -> String {
    normalize_repo_stage_path(repo_root, path)
}
pub(super) fn refresh_target_artifacts(target: &BumpTarget, repo_root: Option<&str>) -> Result<()> {
    if target.format != TargetFormat::Toml {
        return Ok(());
    }

    let target_path = resolve_target_path(repo_root, &target.path);
    let is_cargo_manifest = target_path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("Cargo.toml"));
    if !is_cargo_manifest {
        return Ok(());
    }

    let lock_path = target_path.with_file_name("Cargo.lock");
    if !lock_path.is_file() {
        return Ok(());
    }

    let output = Command::new("cargo")
        .arg("generate-lockfile")
        .arg("--manifest-path")
        .arg(&target_path)
        .output()
        .with_context(|| {
            format!(
                "failed to refresh {} after updating {}",
                lock_path.display(),
                target.path
            )
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let detail = if stderr.is_empty() { stdout } else { stderr };
        bail!(
            "failed to refresh {} after updating {}: {}",
            lock_path.display(),
            target.path,
            detail
        );
    }

    Ok(())
}

fn push_stage_path(paths: &mut Vec<String>, repo_root: &str, path: &str) {
    let candidate = normalize_repo_stage_path(repo_root, path);
    if !candidate.is_empty() && !paths.iter().any(|existing| existing == &candidate) {
        paths.push(candidate);
    }
}

fn normalize_repo_stage_path(repo_root: &str, path: &str) -> String {
    let candidate = Path::new(path);
    if candidate.is_absolute() {
        candidate
            .strip_prefix(repo_root)
            .map(|relative| relative.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|_| path.replace('\\', "/"))
    } else {
        path.replace('\\', "/")
    }
}

fn resolve_repo_path(repo_root: &str, path: &str) -> PathBuf {
    let candidate = Path::new(path);
    if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        Path::new(repo_root).join(candidate)
    }
}

fn resolve_target_path(repo_root: Option<&str>, path: &str) -> PathBuf {
    let candidate = Path::new(path);
    if candidate.is_absolute() {
        candidate.to_path_buf()
    } else if let Some(repo_root) = repo_root {
        Path::new(repo_root).join(candidate)
    } else {
        candidate.to_path_buf()
    }
}

fn comparable_git_path(path: &str) -> String {
    path.replace('\\', "/").to_ascii_lowercase()
}

fn run_git_checked_owned(repo_root: &str, args: Vec<String>) -> Result<String> {
    let arg_refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    run_git_checked(repo_root, &arg_refs)
}
