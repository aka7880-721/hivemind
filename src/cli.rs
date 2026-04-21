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
        git_flow::{
            apply_repo_bump_workflow, collect_non_main_repo_states, collect_repo_bump_operations,
        },
    },
    config::{
        AppConfig, BranchConfig, ConfigStore, ProjectConfig, ProjectType, RepoConfig, TargetFormat,
        TargetSpec,
    },
    git::{
        collect_all_branch_git_scope_contexts, current_branch_with_cancel, last_bump_time,
        latest_local_tag_with_cancel, run_git, run_git_checked, split_output_lines,
    },
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
        [command] if is_branch_command(command) => {
            print_branch_status()?;
            Ok(StartupMode::Handled)
        }
        [command, lookup] if is_project_version_command(command) => {
            print_project_version(lookup)?;
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

fn is_branch_command(value: &str) -> bool {
    matches!(value, "branch" | "br" | "brn" | "brnch")
}

fn is_project_version_command(value: &str) -> bool {
    value == "v"
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
    println!("  cg branch                  Show the current branch and a compact branch tree");
    println!("  cg v <alias>               Show project version, last bump, and last release");
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

fn print_branch_status() -> Result<()> {
    let config = load_config()?;
    let cwd =
        best_effort_canonicalize(&env::current_dir().context("failed to read current directory")?);
    let project = find_project_for_cwd(&config.projects, &cwd)?;
    let resolved_project = resolve_project_target_paths(project)?;
    let scope_index = if project.project_type == ProjectType::AllInOne || project.unified_versioning
    {
        0
    } else {
        find_scope_for_cwd(project, &resolved_project, &cwd)?
    };
    let git_contexts = collect_all_branch_git_scope_contexts(&resolved_project)?;
    let context = git_contexts
        .get(scope_index)
        .or_else(|| git_contexts.first())
        .ok_or_else(|| anyhow!("git scope metadata is unavailable for the current branch view"))?;
    let current_branch = current_branch_with_cancel(&context.repo_root, None)?;

    println!();
    println!("current branch: \x1b[33m{}\x1b[0m", current_branch);
    println!("---------------");
    println!();
    println!("Generating the tree...");
    println!("\x1b[90m(ctrl+c to cancel)\x1b[0m");
    println!();
    io::stdout()
        .flush()
        .context("failed to flush branch status output")?;

    let diagram = load_branch_diagram(
        &context.repo_root,
        &current_branch,
        context.main_branch_name.as_deref(),
    )?;

    println!("{}", render_branch_tree(diagram.as_ref()));
    println!();
    Ok(())
}

fn print_project_version(lookup: &str) -> Result<()> {
    let config = load_config()?;
    let project = find_project_by_lookup(&config.projects, lookup)?;
    let resolved_project = resolve_project_target_paths(project)?;
    let scopes = collect_bump_scopes(&resolved_project)?;
    if scopes.is_empty() {
        bail!("the matched project does not contain any bump targets")
    }

    let current_version = shared_bump_version(&scopes)
        .or_else(|| {
            scopes
                .first()
                .and_then(|scope| scope.current_version.clone())
        })
        .unwrap_or_else(|| "mixed values".to_string());

    let git_context = collect_all_branch_git_scope_contexts(&resolved_project)
        .ok()
        .and_then(|contexts| contexts.into_iter().next());
    let (last_bump, last_release) = if let Some(context) = git_context {
        let last_bump = last_bump_time(&context.repo_root, &context.git_pathspecs(), None)
            .ok()
            .flatten()
            .and_then(|timestamp| {
                crate::git_stt::format_relative_git_timestamp(&timestamp.to_string())
            })
            .unwrap_or_else(|| "n/a".to_string());
        let last_release = latest_public_release_tag_for_repo(&context.repo_root)
            .or_else(|| {
                latest_local_tag_with_cancel(&context.repo_root, None)
                    .ok()
                    .flatten()
            })
            .unwrap_or_else(|| "n/a".to_string());
        (last_bump, last_release)
    } else {
        ("n/a".to_string(), "n/a".to_string())
    };

    println!();
    println!("Project Name: {}", project.name);
    println!("Current Version: {}", current_version);
    println!("Last Bump: {}", last_bump);
    println!("Last Release: {}", last_release);
    println!();
    Ok(())
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

    let mut repo_operations = Vec::new();
    if workflow != OverviewBumpWorkflow::JustBump {
        if !project.integration_mode.requires_repo() {
            bail!("selected bump option requires a git-backed project")
        }

        let git_contexts = collect_all_branch_git_scope_contexts(&resolved_project)?;
        repo_operations = collect_repo_bump_operations(
            &resolved_project,
            &scopes,
            &git_contexts,
            &affected_indexes,
        )?;

        if workflow.requires_branch() {
            let non_main_repo_states = collect_non_main_repo_states(
                &resolved_project,
                &scopes,
                &git_contexts,
                &affected_indexes,
            )?;
            if !non_main_repo_states.is_empty() {
                println!("Just to check: Are you aware you are currently on a NON-MAIN branch?");
                println!("You are here:");
                for state in &non_main_repo_states {
                    let repo_name = Path::new(&state.repo_root)
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or(&state.repo_root);
                    println!("  {} -> {}", repo_name, state.current_branch);
                }

                if !prompt_confirm_default_yes(
                    "Press ENTER or Y to ignore and continue; N to cancel: ",
                )? {
                    bail!("Cancelled by user");
                }
            }
        }
    }

    let branch_name = if workflow.requires_branch() {
        Some(prompt_branch_name()?)
    } else {
        None
    };

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

fn prompt_confirm_default_yes(prompt: &str) -> Result<bool> {
    loop {
        print!("{}", prompt);
        io::stdout().flush().context("failed to flush prompt")?;

        let mut answer = String::new();
        io::stdin()
            .read_line(&mut answer)
            .context("failed to read response")?;

        match answer.trim().to_lowercase().as_str() {
            "" | "y" => return Ok(true),
            "n" => return Ok(false),
            other => {
                println!("Please answer Y or N. Received: {}", other);
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct BranchRef {
    name: String,
    refname: String,
    object_id: String,
    root_distance: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum BranchDiagramState {
    Main,
    Current,
    Open,
    Merged,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct BranchDiagramNode {
    name: String,
    state: BranchDiagramState,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct BranchDiagramSegment {
    branch: BranchDiagramNode,
    merged: Vec<BranchDiagramNode>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct BranchDiagram {
    root: BranchDiagramNode,
    path: Vec<BranchDiagramSegment>,
}

fn list_local_branch_refs(repo_root: &str) -> Result<Vec<BranchRef>> {
    let output = run_git_checked(
        repo_root,
        &[
            "for-each-ref",
            "--format=%(refname:short)|%(refname)|%(objectname)",
            "refs/heads",
        ],
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

fn load_branch_diagram(
    repo_root: &str,
    current_branch: &str,
    custom_main_branch: Option<&str>,
) -> Result<Option<BranchDiagram>> {
    let mut branches = list_local_branch_refs(repo_root)?;
    if branches.is_empty() {
        return Ok(None);
    }

    let root_index = branches
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
        .unwrap_or(0);
    let root_branch = branches.remove(root_index);

    for branch in &mut branches {
        branch.root_distance =
            branch_unique_commit_count(repo_root, &root_branch.refname, &branch.refname)?;
    }

    let current_ref = if root_branch.name.eq_ignore_ascii_case(current_branch) {
        root_branch.clone()
    } else {
        branches
            .iter()
            .find(|branch| branch.name.eq_ignore_ascii_case(current_branch))
            .cloned()
            .ok_or_else(|| anyhow!("current branch is not available among local refs"))?
    };

    let first_parent_commits = first_parent_commit_ids(repo_root, &current_ref.refname)?;
    let mut family_branches = Vec::new();
    for branch in branches {
        if !is_branch_ancestor(repo_root, &branch.refname, &current_ref.refname)? {
            continue;
        }

        if is_branch_ancestor(repo_root, &branch.refname, &root_branch.refname)? {
            continue;
        }

        family_branches.push(branch);
    }

    let mut path_branches = family_branches
        .iter()
        .filter(|branch| first_parent_commits.contains(&branch.object_id))
        .cloned()
        .collect::<Vec<_>>();
    if !root_branch.name.eq_ignore_ascii_case(current_branch)
        && path_branches
            .iter()
            .all(|branch| !branch.name.eq_ignore_ascii_case(current_branch))
    {
        path_branches.push(current_ref.clone());
    }

    path_branches.sort_by(|left, right| {
        let left_is_current = left.name.eq_ignore_ascii_case(current_branch);
        let right_is_current = right.name.eq_ignore_ascii_case(current_branch);
        left.root_distance
            .cmp(&right.root_distance)
            .then_with(|| left_is_current.cmp(&right_is_current).reverse())
            .then_with(|| normalize_lookup(&left.name).cmp(&normalize_lookup(&right.name)))
    });

    let mut path = path_branches
        .iter()
        .map(|branch| BranchDiagramSegment {
            branch: BranchDiagramNode {
                name: display_branch_name(&branch.name),
                state: if branch.name.eq_ignore_ascii_case(current_branch) {
                    BranchDiagramState::Current
                } else {
                    BranchDiagramState::Open
                },
            },
            merged: Vec::new(),
        })
        .collect::<Vec<_>>();

    for branch in family_branches {
        if path_branches
            .iter()
            .any(|path_branch| path_branch.object_id == branch.object_id)
        {
            continue;
        }

        let mut best_attachment = None;
        for (index, path_branch) in path_branches.iter().enumerate() {
            if !is_branch_ancestor(repo_root, &branch.refname, &path_branch.refname)? {
                continue;
            }

            let distance = commit_distance(repo_root, &branch.refname, &path_branch.refname)?;
            if distance == 0 {
                continue;
            }

            match best_attachment {
                Some((_, best_distance)) if distance >= best_distance => {}
                _ => best_attachment = Some((index, distance)),
            }
        }

        if let Some((index, _)) = best_attachment {
            path[index].merged.push(BranchDiagramNode {
                name: display_branch_name(&branch.name),
                state: BranchDiagramState::Merged,
            });
        }
    }

    for segment in &mut path {
        segment.merged.sort_by(|left, right| {
            normalize_lookup(&left.name).cmp(&normalize_lookup(&right.name))
        });
    }

    Ok(Some(BranchDiagram {
        root: BranchDiagramNode {
            name: display_branch_name(&root_branch.name),
            state: BranchDiagramState::Main,
        },
        path,
    }))
}

fn first_parent_commit_ids(repo_root: &str, branch_ref: &str) -> Result<HashSet<String>> {
    let output = run_git_checked(repo_root, &["rev-list", "--first-parent", branch_ref])?;
    Ok(split_output_lines(&output).into_iter().collect())
}

fn is_branch_ancestor(repo_root: &str, ancestor_ref: &str, descendant_ref: &str) -> Result<bool> {
    let output = run_git(
        repo_root,
        &["merge-base", "--is-ancestor", ancestor_ref, descendant_ref],
    )?;
    Ok(output.success)
}

fn branch_unique_commit_count(repo_root: &str, base_ref: &str, branch_ref: &str) -> Result<usize> {
    commit_distance(repo_root, base_ref, branch_ref)
}

fn commit_distance(repo_root: &str, base_ref: &str, branch_ref: &str) -> Result<usize> {
    let range = format!("{}..{}", base_ref, branch_ref);
    let output = run_git_checked(repo_root, &["rev-list", "--count", &range])?;
    output
        .trim()
        .parse::<usize>()
        .with_context(|| format!("failed to parse git ancestry distance for {}", range))
}

fn display_branch_name(branch: &str) -> String {
    branch.strip_prefix("heads/").unwrap_or(branch).to_string()
}

fn render_branch_tree(diagram: Option<&BranchDiagram>) -> String {
    let Some(diagram) = diagram else {
        return "(no local branches)".to_string();
    };

    let (arrow_column, segment_layouts) = compute_branch_diagram_layout(diagram);
    let mut lines = vec![render_aligned_arrow_line(
        "",
        &format_branch_label(&diagram.root),
        plain_branch_label(&diagram.root).len() + 1,
        arrow_column,
    )];
    render_branch_segments(&mut lines, &diagram.path, &segment_layouts, arrow_column);

    lines.join("\n")
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct BranchDiagramSegmentLayout {
    anchor_column: usize,
    merge_column: Option<usize>,
    continuation_column: Option<usize>,
}

fn compute_branch_diagram_layout(
    diagram: &BranchDiagram,
) -> (usize, Vec<BranchDiagramSegmentLayout>) {
    const MIN_RIGHT_TAIL: usize = 17;
    const MIN_JUNCTION_GAP: usize = 12;

    let root_start = plain_branch_label(&diagram.root).len() + 1;
    let mut arrow_column = root_start + MIN_RIGHT_TAIL;
    let mut layouts = Vec::with_capacity(diagram.path.len());
    let mut anchor_column = 0usize;

    for (index, segment) in diagram.path.iter().enumerate() {
        let path_start = anchor_column + 3 + plain_branch_label(&segment.branch).len() + 1;
        let has_more_path = index + 1 < diagram.path.len();
        let has_merged = !segment.merged.is_empty();
        let merge_column = has_merged.then(|| {
            let merged_start = segment
                .merged
                .iter()
                .map(|merged| anchor_column + 6 + plain_branch_label(merged).len() + 1)
                .max()
                .unwrap_or(path_start);
            path_start.max(merged_start) + MIN_JUNCTION_GAP
        });
        let continuation_column = if has_more_path {
            Some(merge_column.unwrap_or(path_start + MIN_JUNCTION_GAP) + usize::from(has_merged))
        } else {
            None
        };

        let line_arrow_column = continuation_column
            .map(|column| column + MIN_RIGHT_TAIL + 1)
            .or_else(|| merge_column.map(|column| column + MIN_RIGHT_TAIL + 1))
            .unwrap_or(path_start + MIN_RIGHT_TAIL);
        arrow_column = arrow_column.max(line_arrow_column);
        layouts.push(BranchDiagramSegmentLayout {
            anchor_column,
            merge_column,
            continuation_column,
        });

        if let Some(next_anchor_column) = continuation_column {
            anchor_column = next_anchor_column;
        }
    }

    (arrow_column, layouts)
}

fn render_branch_segments(
    lines: &mut Vec<String>,
    segments: &[BranchDiagramSegment],
    layouts: &[BranchDiagramSegmentLayout],
    arrow_column: usize,
) {
    for (index, segment) in segments.iter().enumerate() {
        let layout = &layouts[index];
        let path_prefix = " ".repeat(layout.anchor_column);
        let has_more_path = index + 1 < segments.len();
        let has_merged = !segment.merged.is_empty();
        let path_label = format_branch_label(&segment.branch);
        let path_start = layout.anchor_column + 3 + plain_branch_label(&segment.branch).len() + 1;

        match (layout.merge_column, layout.continuation_column) {
            (Some(merge_column), Some(continuation_column)) => {
                lines.push(format!(
                    "{}└─ {} {}┬┬{}>",
                    path_prefix,
                    path_label,
                    branch_diagram_fill(path_start, merge_column),
                    branch_diagram_fill(continuation_column + 1, arrow_column)
                ));
            }
            (Some(merge_column), None) => {
                debug_assert!(!has_more_path);
                lines.push(format!(
                    "{}└─ {} {}┬{}>",
                    path_prefix,
                    path_label,
                    branch_diagram_fill(path_start, merge_column),
                    branch_diagram_fill(merge_column + 1, arrow_column)
                ));
            }
            (None, Some(continuation_column)) => {
                debug_assert!(has_more_path);
                lines.push(format!(
                    "{}└─ {} {}┬{}>",
                    path_prefix,
                    path_label,
                    branch_diagram_fill(path_start, continuation_column),
                    branch_diagram_fill(continuation_column + 1, arrow_column)
                ));
            }
            (None, None) => {
                debug_assert!(!has_more_path && !has_merged);
                lines.push(format!(
                    "{}└─ {} {}>",
                    path_prefix,
                    path_label,
                    branch_diagram_fill(path_start, arrow_column)
                ));
            }
        }

        for (merged_index, merged) in segment.merged.iter().enumerate() {
            let merged_prefix = format!("{}   ", path_prefix);
            let merged_start = layout.anchor_column + 6 + plain_branch_label(merged).len() + 1;
            let merge_column = layout
                .merge_column
                .expect("merged segments require a merge column");
            let starter = if merged_index + 1 == segment.merged.len() {
                "└─ "
            } else {
                "├─ "
            };
            let close = if merged_index + 1 == segment.merged.len() {
                '┘'
            } else {
                '┤'
            };

            if layout.continuation_column.is_some() {
                lines.push(format!(
                    "{}{}{} {}{}│",
                    merged_prefix,
                    starter,
                    format_branch_label(merged),
                    branch_diagram_fill(merged_start, merge_column),
                    close
                ));
            } else {
                lines.push(format!(
                    "{}{}{} {}{}",
                    merged_prefix,
                    starter,
                    format_branch_label(merged),
                    branch_diagram_fill(merged_start, merge_column),
                    close
                ));
            }
        }
    }
}

fn render_aligned_arrow_line(
    prefix: &str,
    label: &str,
    start_column: usize,
    arrow_column: usize,
) -> String {
    format!(
        "{}{} {}>",
        prefix,
        label,
        branch_diagram_fill(start_column, arrow_column)
    )
}

fn branch_diagram_fill(start_column: usize, target_column: usize) -> String {
    let count = target_column.saturating_sub(start_column);
    "─".repeat(count)
}

fn plain_branch_label(branch: &BranchDiagramNode) -> String {
    branch.name.clone()
}

fn format_branch_label(branch: &BranchDiagramNode) -> String {
    let plain = plain_branch_label(branch);

    match branch.state {
        BranchDiagramState::Main => format!("\x1b[32m{}\x1b[0m", plain),
        BranchDiagramState::Current => format!("\x1b[33m{}\x1b[0m", plain),
        BranchDiagramState::Open => format!("\x1b[92m{}\x1b[0m", plain),
        BranchDiagramState::Merged => format!("\x1b[94m{}\x1b[0m", plain),
    }
}

fn latest_public_release_tag_for_repo(repo_root: &str) -> Option<String> {
    let output = Command::new("gh")
        .current_dir(repo_root)
        .args([
            "release",
            "list",
            "--limit",
            "1",
            "--json",
            "tagName",
            "--jq",
            ".[].tagName",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let tag = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!tag.is_empty()).then_some(tag)
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
                ..RepoConfig::default()
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
    fn is_branch_command_accepts_requested_synonyms() {
        assert!(is_branch_command("branch"));
        assert!(is_branch_command("br"));
        assert!(is_branch_command("brn"));
        assert!(is_branch_command("brnch"));
        assert!(!is_branch_command("bran"));
    }

    #[test]
    fn is_project_version_command_accepts_short_form() {
        assert!(is_project_version_command("v"));
        assert!(!is_project_version_command("version"));
    }

    #[test]
    fn render_branch_tree_highlights_main_and_current_branch() {
        let tree = render_branch_tree(Some(&BranchDiagram {
            root: BranchDiagramNode {
                name: "main".to_string(),
                state: BranchDiagramState::Main,
            },
            path: vec![
                BranchDiagramSegment {
                    branch: BranchDiagramNode {
                        name: "feature/base".to_string(),
                        state: BranchDiagramState::Open,
                    },
                    merged: vec![BranchDiagramNode {
                        name: "release/0.10".to_string(),
                        state: BranchDiagramState::Merged,
                    }],
                },
                BranchDiagramSegment {
                    branch: BranchDiagramNode {
                        name: "feature/payments".to_string(),
                        state: BranchDiagramState::Current,
                    },
                    merged: Vec::new(),
                },
            ],
        }));

        assert!(tree.contains("\x1b[32mmain\x1b[0m"));
        assert!(tree.contains("\x1b[33mfeature/payments\x1b[0m"));
        assert!(tree.contains("\x1b[94mrelease/0.10\x1b[0m"));
    }

    #[test]
    fn render_branch_tree_highlights_custom_main_branch() {
        let tree = render_branch_tree(Some(&BranchDiagram {
            root: BranchDiagramNode {
                name: "trunk".to_string(),
                state: BranchDiagramState::Main,
            },
            path: vec![BranchDiagramSegment {
                branch: BranchDiagramNode {
                    name: "feature/payments".to_string(),
                    state: BranchDiagramState::Current,
                },
                merged: Vec::new(),
            }],
        }));

        assert!(tree.contains("\x1b[32mtrunk\x1b[0m"));
    }

    #[test]
    fn render_branch_tree_aligns_arrowheads_and_junctions() {
        let tree = render_branch_tree(Some(&BranchDiagram {
            root: BranchDiagramNode {
                name: "main".to_string(),
                state: BranchDiagramState::Main,
            },
            path: vec![
                BranchDiagramSegment {
                    branch: BranchDiagramNode {
                        name: "0.10.8+".to_string(),
                        state: BranchDiagramState::Open,
                    },
                    merged: vec![BranchDiagramNode {
                        name: "0.10.9".to_string(),
                        state: BranchDiagramState::Merged,
                    }],
                },
                BranchDiagramSegment {
                    branch: BranchDiagramNode {
                        name: "0.10.10".to_string(),
                        state: BranchDiagramState::Current,
                    },
                    merged: Vec::new(),
                },
            ],
        }));
        let lines = strip_ansi_for_test(&tree)
            .lines()
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();

        let arrow_column = char_position(&lines[0], '>').expect("root line should contain arrow");
        assert_eq!(
            char_position(&lines[1], '>').expect("open branch should contain arrow"),
            arrow_column
        );
        assert_eq!(
            char_position(&lines[3], '>').expect("current branch should contain arrow"),
            arrow_column
        );
        assert_eq!(
            char_position(&lines[1], '┬').expect("junction should exist"),
            char_position(&lines[2], '┘').expect("merged return should exist")
        );
    }

    #[test]
    fn render_branch_tree_keeps_future_branch_after_merge_timeline() {
        let tree = render_branch_tree(Some(&BranchDiagram {
            root: BranchDiagramNode {
                name: "main".to_string(),
                state: BranchDiagramState::Main,
            },
            path: vec![
                BranchDiagramSegment {
                    branch: BranchDiagramNode {
                        name: "0.10.8+".to_string(),
                        state: BranchDiagramState::Open,
                    },
                    merged: vec![BranchDiagramNode {
                        name: "0.10.9".to_string(),
                        state: BranchDiagramState::Merged,
                    }],
                },
                BranchDiagramSegment {
                    branch: BranchDiagramNode {
                        name: "0.10.10".to_string(),
                        state: BranchDiagramState::Current,
                    },
                    merged: Vec::new(),
                },
            ],
        }));

        assert_eq!(
            strip_ansi_for_test(&tree),
            [
                "main ─────────────────────────────────────────────────>",
                "└─ 0.10.8+ ──────────────┬┬───────────────────────────>",
                "   └─ 0.10.9 ────────────┘│",
                "                          └─ 0.10.10 ─────────────────>",
            ]
            .join("\n")
        );
    }

    #[test]
    fn display_branch_name_omits_heads_prefix() {
        assert_eq!(display_branch_name("heads/0.10.9"), "0.10.9");
        assert_eq!(display_branch_name("release/1.0.0"), "release/1.0.0");
    }

    fn strip_ansi_for_test(text: &str) -> String {
        let mut plain = String::new();
        let mut chars = text.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '\u{1b}' && chars.peek() == Some(&'[') {
                chars.next();
                for seq_ch in chars.by_ref() {
                    if seq_ch.is_ascii_alphabetic() {
                        break;
                    }
                }
                continue;
            }

            plain.push(ch);
        }

        plain
    }

    fn char_position(text: &str, target: char) -> Option<usize> {
        text.chars().position(|ch| ch == target)
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
                ..RepoConfig::default()
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
            ..RepoConfig::default()
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
                ..RepoConfig::default()
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
