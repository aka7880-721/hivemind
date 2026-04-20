// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit License v1.2
//
// For details, see the LICENSE file in the repository root.

use std::{
    cmp::Ordering,
    collections::HashSet,
    env, fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use anyhow::{Context, Result, anyhow, bail};
use chrono::Local;
use reqwest::blocking::Client;
use serde::Deserialize;

use crate::{
    app::{
        OverviewBumpWorkflow,
        git_flow::{apply_repo_bump_workflow, collect_repo_bump_operations},
    },
    config::{
        AppConfig, BranchConfig, ConfigStore, ProjectConfig, ProjectType, RepoConfig, TargetFormat,
        TargetSpec,
    },
    git::collect_all_branch_git_scope_contexts,
    targets::{BumpTarget, collect_bump_scopes, shared_bump_version, write_target_version},
    versioning::{BumpAction, VersionScheme},
};

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
const GITHUB_LATEST_RELEASE_URL: &str =
    "https://api.github.com/repos/comfy-home/ComfyGit/releases/latest";

pub(crate) enum StartupMode {
    Handled,
    LaunchTui,
}

pub(crate) fn dispatch() -> Result<StartupMode> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    dispatch_args(&args)
}

fn dispatch_args(args: &[String]) -> Result<StartupMode> {
    match args {
        [] => Ok(StartupMode::LaunchTui),
        [command] if is_help(command) => {
            print_usage();
            Ok(StartupMode::Handled)
        }
        [command] if is_version(command) => {
            print_version_status();
            Ok(StartupMode::Handled)
        }
        [command] if is_tui(command) => Ok(StartupMode::LaunchTui),
        [command, option] if command == "pwd" && matches!(option.as_str(), "-all" | "all") => {
            let config = load_config()?;
            for root in all_configured_repo_roots(&config.projects) {
                println!("{}", root.display());
            }
            Ok(StartupMode::Handled)
        }
        [command, lookup] if command == "pwd" => {
            let config = load_config()?;
            let project = find_project_by_lookup(&config.projects, lookup)?;
            println!("{}", project_root(project)?.display());
            Ok(StartupMode::Handled)
        }
        [command, action] if is_bump_command(command) => {
            run_bump(action, None)?;
            Ok(StartupMode::Handled)
        }
        [command, action, option] if is_bump_command(command) => {
            run_bump(action, Some(option))?;
            Ok(StartupMode::Handled)
        }
        _ => {
            print_usage();
            bail!("unknown command")
        }
    }
}

fn is_help(value: &str) -> bool {
    matches!(value, "help" | "-h" | "--help")
}

fn is_version(value: &str) -> bool {
    matches!(value, "-v" | "--version")
}

fn is_tui(value: &str) -> bool {
    matches!(value, "tui" | "ui")
}

fn is_bump_command(value: &str) -> bool {
    matches!(value, "bmp" | "bump" | "bp" | "bum")
}

fn print_usage() {
    println!(" ");
    println!(
        "ComfyGit {} | © 2026 ComfyHome™ | support@comfyhome.io",
        APP_VERSION
    );
    println!(" ");
    println!("Available:");
    println!(" ");
    println!("  ↓ Command / CATEGORY ↓     ↓ Description ↓");
    println!(" ");
    println!("  GENERAL COMMANDS:");
    println!(" ");
    println!("  cg | comfygit              Launch the interactive TUI");
    println!("  cg -v | --version          Show version and GitHub update status");
    println!("  ---------------            --------------------------------------------------");
    println!("  <alias>                    Set in TUI: ");
    println!("                                 Projects → Project Settings → General → Alias");
    println!("  ---------------            --------------------------------------------------");
    println!("  cg pwd <alias>             Print the configured project root path");
    println!("  cg pwd -all | all          Print all configured repo root directories");
    println!("  ---------------            --------------------------------------------------");
    println!(
        "  cg cd <alias>              Change the current directory to the configured project root path from anywhere!"
    );
    println!(" ");
    println!("  BUMPING COMMANDS:");
    println!(" ");
    println!(
        "  cg bmp <action>            Performs a simple version bump for the project in the current working directory"
    );
    println!("          actions: major | minor | Patch | Auto | Cal ");
    println!("          synonyms:");
    println!("            major: maj | mj | mjr | big | .");
    println!("            minor: min | mnr | mr | mn | small | sml | ..");
    println!("            patch: pat | ptch | ph | pth | mini | ...");
    println!(" ");
    println!(
        "  cg bmp <action> <option>   Bump the project in the current working directory as per options available in TUI"
    );
    println!("          options:");
    println!("             1 → Just bump the version");
    println!("             2 → Bump & Commit (locally)");
    println!("             3 → Bump & Commit & Push");
    println!("             4 → Branch & Bump & Commit & Push (will prompt for branch name)");
    println!(" ");
}

fn print_version_status() {
    println!(" ");
    println!(
        "ComfyGit {} | © 2026 ComfyHome™ | support@comfyhome.io",
        APP_VERSION
    );
    println!(" ");
    match github_release_status() {
        ReleaseStatus::UpToDate => println!("GitHub status: Up to date!"),
        ReleaseStatus::UpdateAvailable(version) => {
            println!("GitHub status: Update available! Latest: {}", version)
        }
        ReleaseStatus::Unavailable => println!("GitHub status: Version check unavailable."),
    }
    println!(" ");
}

fn run_bump(action_name: &str, option_name: Option<&str>) -> Result<()> {
    let config = load_config()?;
    let cwd =
        best_effort_canonicalize(&env::current_dir().context("failed to read current directory")?);
    let project = find_project_for_cwd(&config.projects, &cwd)?;
    let resolved_project = resolve_project_target_paths(project)?;
    let scopes = collect_bump_scopes(&resolved_project)?;
    if scopes.is_empty() {
        bail!("the matched project does not contain any bump targets")
    }

    let action = parse_bump_action(action_name)?;
    let workflow = parse_cli_bump_option(option_name)?;
    let affected_indexes = affected_scope_indexes(project, &resolved_project, &cwd, scopes.len())?;
    let scope_index = *affected_indexes
        .first()
        .ok_or_else(|| anyhow!("no scope was selected for bumping"))?;
    let scheme = scopes
        .get(scope_index)
        .map(|scope| scope.scheme)
        .ok_or_else(|| anyhow!("the selected scope no longer exists"))?;
    ensure_action_supported(scheme, action)?;

    let current_version =
        if project.project_type == ProjectType::AllInOne || project.unified_versioning {
            shared_bump_version(&scopes).ok_or_else(|| {
                anyhow!("the project has mixed target versions; unify them before running cg bmp")
            })?
        } else {
            scopes[scope_index]
                .current_version
                .clone()
                .ok_or_else(|| anyhow!("the selected scope has mixed target versions"))?
        };

    let next_version = scheme
        .bump(&current_version, action, Local::now().date_naive())
        .map_err(anyhow::Error::msg)?;

    let mut updated_targets = 0usize;
    for index in &affected_indexes {
        let scope = scopes
            .get(*index)
            .ok_or_else(|| anyhow!("scope index {} is out of range", index))?;
        for target in &scope.targets {
            write_target_version(target, &next_version)?;
            refresh_target_artifacts(target)?;
            updated_targets += 1;
        }
    }

    if workflow != OverviewBumpWorkflow::JustBump {
        if !project.integration_mode.requires_repo() {
            bail!("selected bump option requires a git-backed project")
        }

        let git_contexts = collect_all_branch_git_scope_contexts(&resolved_project)?;
        let repo_operations = collect_repo_bump_operations(
            &resolved_project,
            &scopes,
            &git_contexts,
            &affected_indexes,
        )?;
        let branch_name = if workflow.requires_branch() {
            Some(prompt_branch_name()?)
        } else {
            None
        };
        apply_repo_bump_workflow(
            &repo_operations,
            &next_version,
            workflow,
            branch_name.as_deref(),
        )?;
    }

    if project.project_type == ProjectType::AllInOne || project.unified_versioning {
        println!(
            "Updated {} target{} in {} from {} to {}.",
            updated_targets,
            if updated_targets == 1 { "" } else { "s" },
            project.name,
            current_version,
            next_version
        );
    } else {
        println!(
            "Updated {} target{} in {} / {} from {} to {}.",
            updated_targets,
            if updated_targets == 1 { "" } else { "s" },
            project.name,
            scopes[scope_index].display_name,
            current_version,
            next_version
        );
    }

    Ok(())
}

fn parse_cli_bump_option(value: Option<&str>) -> Result<OverviewBumpWorkflow> {
    match value.map(str::trim).filter(|value| !value.is_empty()) {
        None => Ok(OverviewBumpWorkflow::JustBump),
        Some("1") => Ok(OverviewBumpWorkflow::JustBump),
        Some("2") => Ok(OverviewBumpWorkflow::Commit),
        Some("3") => Ok(OverviewBumpWorkflow::CommitAndPush),
        Some("4") => Ok(OverviewBumpWorkflow::BranchCommitAndPush),
        Some(other) => bail!("unsupported bump option '{}'; expected 1-4", other),
    }
}

fn prompt_branch_name() -> Result<String> {
    print!("Enter branch name: ");
    io::stdout()
        .flush()
        .context("failed to flush branch name prompt")?;

    let mut branch_name = String::new();
    io::stdin()
        .read_line(&mut branch_name)
        .context("failed to read branch name")?;

    let branch_name = branch_name.trim().to_string();
    if branch_name.is_empty() {
        bail!("branch name cannot be empty")
    }

    Ok(branch_name)
}

fn load_config() -> Result<AppConfig> {
    ConfigStore::locate()?.load()
}

fn all_configured_repo_roots(projects: &[ProjectConfig]) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    let mut roots = Vec::new();

    for project in projects {
        if let Some(repo) = project.repo.as_ref() {
            let path = best_effort_canonicalize(&repo_root_path(repo));
            if seen.insert(path.clone()) {
                roots.push(path);
            }
        }

        if project.project_type == ProjectType::Branched {
            for branch in &project.branches {
                if let Some(repo) = branch.repo.as_ref() {
                    let path = best_effort_canonicalize(&repo_root_path(repo));
                    if seen.insert(path.clone()) {
                        roots.push(path);
                    }
                }
            }
        }
    }

    roots
}

fn find_project_by_lookup<'a>(
    projects: &'a [ProjectConfig],
    lookup: &str,
) -> Result<&'a ProjectConfig> {
    let normalized_lookup = normalize_lookup(lookup);
    if normalized_lookup.is_empty() {
        bail!("project lookup cannot be empty")
    }

    let alias_matches = projects
        .iter()
        .filter(|project| normalize_lookup(&project.alias) == normalized_lookup)
        .collect::<Vec<_>>();
    if alias_matches.len() == 1 {
        return Ok(alias_matches[0]);
    }
    if alias_matches.len() > 1 {
        bail!("multiple projects use the alias '{}'", lookup)
    }

    let name_matches = projects
        .iter()
        .filter(|project| normalize_lookup(&project.name) == normalized_lookup)
        .collect::<Vec<_>>();
    match name_matches.as_slice() {
        [project] => Ok(*project),
        [] => bail!("no configured project matched '{}'", lookup),
        _ => bail!("multiple projects matched '{}'", lookup),
    }
}

fn find_project_for_cwd<'a>(
    projects: &'a [ProjectConfig],
    cwd: &Path,
) -> Result<&'a ProjectConfig> {
    let mut matches = projects
        .iter()
        .filter_map(|project| {
            project_root(project)
                .ok()
                .map(|root| (project, best_effort_canonicalize(&root)))
        })
        .filter(|(_, root)| cwd.starts_with(root))
        .collect::<Vec<_>>();

    if matches.is_empty() {
        bail!(
            "no configured project matches the current directory {}",
            cwd.display()
        )
    }

    matches.sort_by_key(|(_, right)| std::cmp::Reverse(path_depth(right)));
    if matches.len() > 1 && path_depth(&matches[0].1) == path_depth(&matches[1].1) {
        bail!("multiple configured projects match the current directory")
    }

    Ok(matches[0].0)
}

fn affected_scope_indexes(
    project: &ProjectConfig,
    resolved_project: &ProjectConfig,
    cwd: &Path,
    scope_count: usize,
) -> Result<Vec<usize>> {
    if project.project_type == ProjectType::AllInOne {
        return Ok(vec![0]);
    }

    if project.unified_versioning {
        return Ok((0..scope_count).collect());
    }

    Ok(vec![find_scope_for_cwd(project, resolved_project, cwd)?])
}

fn find_scope_for_cwd(
    project: &ProjectConfig,
    resolved_project: &ProjectConfig,
    cwd: &Path,
) -> Result<usize> {
    let mut matches = resolved_project
        .branches
        .iter()
        .enumerate()
        .filter_map(|(index, branch)| {
            scope_root(project, branch).map(|root| (index, best_effort_canonicalize(&root)))
        })
        .filter(|(_, root)| cwd.starts_with(root))
        .collect::<Vec<_>>();

    if matches.is_empty() {
        if resolved_project.branches.len() == 1 {
            return Ok(0);
        }
        bail!(
            "no configured scope matches {}; run cg bmp from a scope directory or enable unified versioning",
            cwd.display()
        )
    }

    matches.sort_by_key(|(_, right)| std::cmp::Reverse(path_depth(right)));
    if matches.len() > 1 && path_depth(&matches[0].1) == path_depth(&matches[1].1) {
        bail!(
            "multiple scopes match the current directory; move deeper into a scope before bumping"
        )
    }

    Ok(matches[0].0)
}

fn scope_root(project: &ProjectConfig, branch: &BranchConfig) -> Option<PathBuf> {
    branch.repo.as_ref().map(repo_root_path).or_else(|| {
        let project_root = project.repo.as_ref().map(repo_root_path);
        target_root_from_specs(&branch.targets, project_root.as_deref())
    })
}

fn project_root(project: &ProjectConfig) -> Result<PathBuf> {
    if let Some(repo) = project.repo.as_ref() {
        return Ok(repo_root_path(repo));
    }

    let roots = if project.project_type == ProjectType::AllInOne {
        target_root_from_specs(&project.targets, None)
            .into_iter()
            .collect::<Vec<_>>()
    } else {
        project
            .branches
            .iter()
            .filter_map(|branch| scope_root(project, branch))
            .collect::<Vec<_>>()
    };

    common_ancestor(roots).ok_or_else(|| {
        anyhow!(
            "project '{}' does not have a resolvable root path",
            project.name
        )
    })
}

fn repo_root_path(repo: &RepoConfig) -> PathBuf {
    PathBuf::from(repo.local_root.trim())
}

fn target_root_from_specs(specs: &[TargetSpec], base_root: Option<&Path>) -> Option<PathBuf> {
    specs
        .iter()
        .find_map(|target| target_parent_path(&target.path, base_root))
}

fn target_parent_path(path: &str, base_root: Option<&Path>) -> Option<PathBuf> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return None;
    }

    let candidate = Path::new(trimmed);
    let resolved = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        base_root?.join(candidate)
    };

    resolved.parent().map(Path::to_path_buf)
}

fn common_ancestor(paths: Vec<PathBuf>) -> Option<PathBuf> {
    let mut iter = paths.into_iter();
    let first = best_effort_canonicalize(&iter.next()?);
    let mut ancestor = first.components().collect::<Vec<_>>();

    for path in iter {
        let canonical = best_effort_canonicalize(&path);
        let components = canonical.components().collect::<Vec<_>>();
        let shared_len = ancestor
            .iter()
            .zip(components.iter())
            .take_while(|(left, right)| left == right)
            .count();
        ancestor.truncate(shared_len);
        if ancestor.is_empty() {
            return None;
        }
    }

    let mut root = PathBuf::new();
    for component in ancestor {
        root.push(component.as_os_str());
    }
    Some(root)
}

fn resolve_project_target_paths(project: &ProjectConfig) -> Result<ProjectConfig> {
    let mut resolved = project.clone();

    if resolved.project_type == ProjectType::AllInOne {
        let project_root = resolved.project_root_base();
        absolutize_targets(&mut resolved.targets, project_root.as_deref());
        return Ok(resolved);
    }

    let project_root = resolved.project_root_base();
    for branch in &mut resolved.branches {
        let branch_root = branch
            .repo
            .as_ref()
            .map(repo_root_path)
            .or_else(|| project_root.clone());
        absolutize_targets(&mut branch.targets, branch_root.as_deref());
    }

    Ok(resolved)
}

fn absolutize_targets(targets: &mut [TargetSpec], base_root: Option<&Path>) {
    for target in targets {
        let trimmed = target.path.trim();
        if trimmed.is_empty() {
            continue;
        }

        let path = Path::new(trimmed);
        if path.is_absolute() {
            continue;
        }

        if let Some(root) = base_root {
            target.path = root.join(path).display().to_string();
        }
    }
}

fn parse_bump_action(value: &str) -> Result<BumpAction> {
    match value.trim().to_ascii_lowercase().as_str() {
        "maj" | "major" | "mj" | "mjr" | "big" | "." => Ok(BumpAction::Major),
        "min" | "minor" | "mnr" | "mr" | "mn" | "small" | "sml" | ".." => Ok(BumpAction::Minor),
        "pat" | "patch" | "ptch" | "ph" | "pth" | "mini" | "..." => Ok(BumpAction::Patch),
        "auto" | "cal" => Ok(BumpAction::Auto),
        _ => bail!("unsupported bump action '{}'", value),
    }
}

fn ensure_action_supported(scheme: VersionScheme, action: BumpAction) -> Result<()> {
    if scheme.supported_actions().contains(&action) {
        Ok(())
    } else {
        let supported = scheme
            .supported_actions()
            .iter()
            .map(|candidate| candidate.display_name().to_ascii_lowercase())
            .collect::<Vec<_>>()
            .join(", ");
        bail!(
            "{} does not support {}; try one of: {}",
            scheme.display_name(),
            action.display_name().to_ascii_lowercase(),
            supported
        )
    }
}

fn normalize_lookup(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn path_depth(path: &Path) -> usize {
    path.components().count()
}

fn best_effort_canonicalize(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn refresh_target_artifacts(target: &BumpTarget) -> Result<()> {
    if target.format != TargetFormat::Toml {
        return Ok(());
    }

    let target_path = Path::new(&target.path);
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
        .arg(target_path)
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

enum ReleaseStatus {
    UpToDate,
    UpdateAvailable(String),
    Unavailable,
}

#[derive(Deserialize)]
struct GitHubRelease {
    tag_name: String,
}

fn github_release_status() -> ReleaseStatus {
    let client = match Client::builder().timeout(Duration::from_secs(3)).build() {
        Ok(client) => client,
        Err(_) => return ReleaseStatus::Unavailable,
    };

    let release = match client
        .get(GITHUB_LATEST_RELEASE_URL)
        .header("User-Agent", format!("cg/{}", APP_VERSION))
        .header("Accept", "application/vnd.github+json")
        .send()
        .and_then(|response| response.error_for_status())
        .and_then(|response| response.json::<GitHubRelease>())
    {
        Ok(release) => release,
        Err(_) => return ReleaseStatus::Unavailable,
    };

    let latest = normalize_release_version(&release.tag_name).to_string();
    match compare_release_versions(APP_VERSION, &latest) {
        Ordering::Less => ReleaseStatus::UpdateAvailable(latest),
        Ordering::Equal | Ordering::Greater => ReleaseStatus::UpToDate,
    }
}

fn normalize_release_version(value: &str) -> &str {
    value.trim().trim_start_matches('v')
}

fn compare_release_versions(current: &str, latest: &str) -> Ordering {
    let current_parts = parse_release_version(current);
    let latest_parts = parse_release_version(latest);
    match (current_parts, latest_parts) {
        (Some(current_parts), Some(latest_parts)) => {
            let width = current_parts.len().max(latest_parts.len());
            for index in 0..width {
                let current_part = *current_parts.get(index).unwrap_or(&0);
                let latest_part = *latest_parts.get(index).unwrap_or(&0);
                match current_part.cmp(&latest_part) {
                    Ordering::Equal => continue,
                    ordering => return ordering,
                }
            }
            Ordering::Equal
        }
        _ => normalize_release_version(current).cmp(normalize_release_version(latest)),
    }
}

fn parse_release_version(value: &str) -> Option<Vec<u64>> {
    normalize_release_version(value)
        .split('.')
        .map(|part| part.parse::<u64>().ok())
        .collect()
}

trait ProjectRootBase {
    fn project_root_base(&self) -> Option<PathBuf>;
}

impl ProjectRootBase for ProjectConfig {
    fn project_root_base(&self) -> Option<PathBuf> {
        self.repo.as_ref().map(repo_root_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{BranchScopeKind, ChangelogSettings, IntegrationMode, ReleaseNowSettings};

    fn sample_project(name: &str, alias: &str) -> ProjectConfig {
        ProjectConfig {
            name: name.to_string(),
            alias: alias.to_string(),
            project_type: ProjectType::AllInOne,
            integration_mode: IntegrationMode::GitLocalOnly,
            unified_versioning: true,
            version_scheme: VersionScheme::SemVer,
            changelog: ChangelogSettings::default(),
            release_now: ReleaseNowSettings::default(),
            targets: vec![TargetSpec {
                label: "Version".to_string(),
                path: "C:/repo/Cargo.toml".to_string(),
                key_path: "package.version".to_string(),
                format: TargetFormat::Toml,
            }],
            branches: Vec::new(),
            repo: Some(RepoConfig {
                local_root: "C:/repo".to_string(),
                remote_url: None,
            }),
        }
    }

    #[test]
    fn parse_bump_action_accepts_short_and_long_forms() {
        assert_eq!(
            parse_bump_action("maj").expect("maj should parse"),
            BumpAction::Major
        );
        assert_eq!(
            parse_bump_action("minor").expect("minor should parse"),
            BumpAction::Minor
        );
        assert_eq!(
            parse_bump_action("pat").expect("pat should parse"),
            BumpAction::Patch
        );
        assert_eq!(
            parse_bump_action("cal").expect("cal should parse"),
            BumpAction::Auto
        );
    }
    #[test]
    fn parse_bump_action_accepts_synonyms() {
        assert_eq!(
            parse_bump_action("mj").expect("mj should parse"),
            BumpAction::Major
        );
        assert_eq!(
            parse_bump_action("mjr").expect("mjr should parse"),
            BumpAction::Major
        );
        assert_eq!(
            parse_bump_action("big").expect("big should parse"),
            BumpAction::Major
        );
        assert_eq!(
            parse_bump_action(".").expect(". should parse"),
            BumpAction::Major
        );

        assert_eq!(
            parse_bump_action("mnr").expect("mnr should parse"),
            BumpAction::Minor
        );
        assert_eq!(
            parse_bump_action("mr").expect("mr should parse"),
            BumpAction::Minor
        );
        assert_eq!(
            parse_bump_action("mn").expect("mn should parse"),
            BumpAction::Minor
        );
        assert_eq!(
            parse_bump_action("small").expect("small should parse"),
            BumpAction::Minor
        );
        assert_eq!(
            parse_bump_action("sml").expect("sml should parse"),
            BumpAction::Minor
        );
        assert_eq!(
            parse_bump_action("..").expect(".. should parse"),
            BumpAction::Minor
        );

        assert_eq!(
            parse_bump_action("ptch").expect("ptch should parse"),
            BumpAction::Patch
        );
        assert_eq!(
            parse_bump_action("ph").expect("ph should parse"),
            BumpAction::Patch
        );
        assert_eq!(
            parse_bump_action("pth").expect("pth should parse"),
            BumpAction::Patch
        );
        assert_eq!(
            parse_bump_action("mini").expect("mini should parse"),
            BumpAction::Patch
        );
        assert_eq!(
            parse_bump_action("...").expect("... should parse"),
            BumpAction::Patch
        );
    }
    #[test]
    fn parse_cli_bump_option_maps_supported_workflows() {
        assert_eq!(
            parse_cli_bump_option(None).expect("default option should parse"),
            OverviewBumpWorkflow::JustBump
        );
        assert_eq!(
            parse_cli_bump_option(Some("1")).expect("option 1 should parse"),
            OverviewBumpWorkflow::JustBump
        );
        assert_eq!(
            parse_cli_bump_option(Some("2")).expect("option 2 should parse"),
            OverviewBumpWorkflow::Commit
        );
        assert_eq!(
            parse_cli_bump_option(Some("3")).expect("option 3 should parse"),
            OverviewBumpWorkflow::CommitAndPush
        );
        assert_eq!(
            parse_cli_bump_option(Some("4")).expect("option 4 should parse"),
            OverviewBumpWorkflow::BranchCommitAndPush
        );
    }

    #[test]
    fn is_bump_command_accepts_synonyms() {
        assert!(is_bump_command("bmp"));
        assert!(is_bump_command("bump"));
        assert!(is_bump_command("bp"));
        assert!(is_bump_command("bum"));
        assert!(!is_bump_command("bom"));
    }

    #[test]
    fn lookup_prefers_alias_before_name() {
        let projects = vec![
            sample_project("alpha", "core"),
            sample_project("core", "ops"),
        ];

        let matched = find_project_by_lookup(&projects, "core").expect("alias should match");

        assert_eq!(matched.name, "alpha");
    }

    #[test]
    fn all_configured_repo_roots_returns_distinct_repo_roots() {
        let mut project1 = sample_project("alpha", "core");
        project1.project_type = ProjectType::Branched;
        project1.unified_versioning = false;
        project1.targets.clear();
        project1.branches = vec![BranchConfig {
            name: "svc".to_string(),
            label: "Service".to_string(),
            scope_kind: BranchScopeKind::Service,
            repo: Some(RepoConfig {
                local_root: "C:/repo/service".to_string(),
                remote_url: None,
            }),
            changelog_enabled: false,
            changelog_path: None,
            release_now: ReleaseNowSettings::default(),
            version_scheme: VersionScheme::SemVer,
            targets: vec![TargetSpec {
                label: "Version".to_string(),
                path: "C:/repo/service/Cargo.toml".to_string(),
                key_path: "package.version".to_string(),
                format: TargetFormat::Toml,
            }],
        }];

        let mut project2 = sample_project("beta", "beta");
        project2.repo = Some(RepoConfig {
            local_root: "C:/repo/beta".to_string(),
            remote_url: None,
        });

        let roots = all_configured_repo_roots(&[project1, project2]);

        assert_eq!(roots.len(), 3);
        assert!(roots.contains(&PathBuf::from("C:/repo")));
        assert!(roots.contains(&PathBuf::from("C:/repo/service")));
        assert!(roots.contains(&PathBuf::from("C:/repo/beta")));
    }

    #[test]
    fn compare_release_versions_handles_prefixes_and_ordering() {
        assert_eq!(compare_release_versions("0.9.2", "v0.9.2"), Ordering::Equal);
        assert_eq!(compare_release_versions("0.9.2", "0.10.0"), Ordering::Less);
        assert_eq!(
            compare_release_versions("0.10.0", "0.9.2"),
            Ordering::Greater
        );
    }

    #[test]
    fn common_ancestor_uses_shared_prefix() {
        let ancestor = common_ancestor(vec![
            PathBuf::from("C:/repo/core"),
            PathBuf::from("C:/repo/service"),
        ])
        .expect("common root should resolve");

        assert!(ancestor.ends_with(Path::new("repo")));
    }

    #[test]
    fn scope_root_prefers_branch_repo_override() {
        let mut project = sample_project("alpha", "core");
        project.project_type = ProjectType::Branched;
        project.unified_versioning = false;
        project.targets.clear();
        project.branches = vec![BranchConfig {
            name: "svc".to_string(),
            label: "Service".to_string(),
            scope_kind: BranchScopeKind::Service,
            repo: Some(RepoConfig {
                local_root: "C:/repo/service".to_string(),
                remote_url: None,
            }),
            changelog_enabled: false,
            changelog_path: None,
            release_now: ReleaseNowSettings::default(),
            version_scheme: VersionScheme::SemVer,
            targets: vec![TargetSpec {
                label: "Version".to_string(),
                path: "Cargo.toml".to_string(),
                key_path: "package.version".to_string(),
                format: TargetFormat::Toml,
            }],
        }];

        let root = scope_root(&project, &project.branches[0]).expect("scope root should resolve");

        assert_eq!(root, PathBuf::from("C:/repo/service"));
    }
}
