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
    sync::{Mutex, OnceLock},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, anyhow, bail};
use chrono::Local;
use crossterm::{
    cursor::{MoveToColumn, MoveUp},
    event::{self, Event, KeyCode, KeyEventKind},
    execute, queue,
    style::{Color, Print, ResetColor, SetForegroundColor},
    terminal::{Clear, ClearType, disable_raw_mode, enable_raw_mode},
};
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
        GitCancellation, collect_all_branch_git_scope_contexts, current_branch_with_cancel,
        last_bump_time, latest_local_tag_with_cancel, resolve_main_branch_name, run_git,
        run_git_checked, run_git_checked_owned_with_cancel, run_git_checked_with_cancel,
        split_output_lines, switch_to_existing_branch, switch_to_main_branch,
    },
    git_br::{BranchNameOption, is_release_line_branch, suggest_branch_name_options},
    git_pr::run_pr,
    targets::{BumpTarget, collect_bump_scopes, shared_bump_version, write_target_version},
    versioning::{BumpAction, VersionScheme},
};

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
const GITHUB_LATEST_RELEASE_URL: &str =
    "https://api.github.com/repos/comfy-home/ComfyGit/releases/latest";

static CLI_GIT_CANCELLATION_SLOT: OnceLock<Mutex<Option<GitCancellation>>> = OnceLock::new();
static CLI_CTRL_C_HANDLER_RESULT: OnceLock<std::result::Result<(), String>> = OnceLock::new();

fn cli_git_cancellation_slot() -> &'static Mutex<Option<GitCancellation>> {
    CLI_GIT_CANCELLATION_SLOT.get_or_init(|| Mutex::new(None))
}

fn ensure_cli_ctrl_c_handler() -> Result<()> {
    let slot = cli_git_cancellation_slot();
    let install_result = CLI_CTRL_C_HANDLER_RESULT.get_or_init(|| {
        ctrlc::set_handler(move || {
            if let Ok(active) = slot.lock()
                && let Some(cancel) = active.as_ref()
            {
                cancel.cancel();
            }
        })
        .map_err(|error| error.to_string())
    });

    if let Err(message) = install_result {
        bail!("failed to install Ctrl+C handler: {}", message)
    }

    Ok(())
}

fn with_cli_git_cancellation<T>(
    action: impl FnOnce(Option<GitCancellation>) -> Result<T>,
) -> Result<T> {
    ensure_cli_ctrl_c_handler()?;

    let cancel = GitCancellation::new();
    {
        let mut active = cli_git_cancellation_slot()
            .lock()
            .map_err(|_| anyhow!("failed to acquire Ctrl+C cancellation state"))?;
        *active = Some(cancel.clone());
    }

    let result = action(Some(cancel.clone()));

    if let Ok(mut active) = cli_git_cancellation_slot().lock() {
        *active = None;
    }

    match result {
        Err(_error) if cancel.is_cancelled() => bail!("cancelled by user"),
        other => other,
    }
}

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
        [command, action] if is_branch_command(command) && is_branch_up_action(action) => {
            run_branch_up()?;
            Ok(StartupMode::Handled)
        }
        [command, action] if is_branch_command(command) && is_branch_main_action(action) => {
            run_branch_main()?;
            Ok(StartupMode::Handled)
        }
        [command] if is_pr_command(command) => {
            let cwd = env::current_dir().context("failed to read current directory")?;
            let repo_root = current_git_repo_root(&cwd)?;
            let custom_main_branch = find_repo_custom_main_branch(&repo_root);
            with_cli_git_cancellation(|cancel| {
                run_pr(&repo_root, false, custom_main_branch.as_deref(), cancel)
            })?;
            Ok(StartupMode::Handled)
        }
        [command, option] if is_pr_command(command) && is_pr_main_option(option) => {
            let cwd = env::current_dir().context("failed to read current directory")?;
            let repo_root = current_git_repo_root(&cwd)?;
            let custom_main_branch = find_repo_custom_main_branch(&repo_root);
            with_cli_git_cancellation(|cancel| {
                run_pr(&repo_root, true, custom_main_branch.as_deref(), cancel)
            })?;
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
        [command, action, commit_hash]
            if is_commit_command(command) && is_commit_delete_action(action) =>
        {
            run_commit_delete(commit_hash)?;
            Ok(StartupMode::Handled)
        }
        [command, action, commit_target]
            if is_commit_command(command) && is_commit_rename_action(action) =>
        {
            run_commit_rename(commit_target)?;
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

fn is_commit_command(value: &str) -> bool {
    matches!(value, "commit" | "cmt" | "com" | "ct")
}

fn is_commit_delete_action(value: &str) -> bool {
    matches!(value, "del" | "rm" | "rem" | "delete" | "drop" | "erase")
}

fn is_commit_rename_action(value: &str) -> bool {
    matches!(value, "rename" | "rn" | "rnm" | "reword" | "rwrd" | "rwd")
}

fn is_branch_command(value: &str) -> bool {
    matches!(value, "branch" | "br" | "brn" | "brnch")
}

fn is_branch_up_action(value: &str) -> bool {
    matches!(value, "up" | "..")
}

fn is_branch_main_action(value: &str) -> bool {
    matches!(value, "main" | "~")
}

fn is_pr_command(value: &str) -> bool {
    matches!(value, "pr")
}

fn is_pr_main_option(value: &str) -> bool {
    matches!(value, "--main" | "-main")
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
    println!("  cg v <alias>               Show project version, last bump, and last release");
    println!("  cg commit del <hash>       Safely remove a published commit by reverting it");
    println!(
        "                             on the current branch and pushing the revert if an upstream exists"
    );
    println!("  cg commit rename <target>  Rename a commit message on the current branch");
    println!(
        "                             <target> may be a commit hash or a HEAD offset (0 = HEAD, 1 = HEAD~1)"
    );
    println!("          synonyms:");
    println!("            commit: cmt | com | ct");
    println!("            del: del | rm | rem | delete | drop | erase");
    println!("            rename: rename | rn | rnm | reword | rwrd | rwd");
    println!(" ");
    println!("  BRANCHING COMMANDS:");
    println!(" ");
    println!("  cg branch                  Show the current branch and a compact branch tree");
    println!("  cg branch up | ..          Switch to the parent branch in the current tree");
    println!("  cg branch main | ~         Switch to main/master/custom main for the project");
    println!(
        "  cg pr                      Generate a pull request title/body for the current branch"
    );
    println!(
        "  cg pr --main | -main       Generate a pull request title/body against main/master/custom main"
    );
    println!("          synonyms:");
    println!("            branch: br | brn | brnch");
    println!("            up: up | ..");
    println!("            main: main | ~");
    println!(" ");
    println!("  BUMPING COMMANDS:");
    println!(" ");
    println!(
        "  cg bmp <action>            Performs a simple version bump for the project in the current working directory"
    );
    println!("          actions: major | minor | Patch | Auto | Cal ");
    println!("          synonyms:");
    println!("            bmp: bump | bp | bum");
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
    println!("             4 → Branch & Bump & Commit (will prompt for branch name, local only)");
    println!("             5 → Branch & Bump & Commit & Push (will prompt for branch name)");
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
    let context = load_active_branch_cli_context()?;

    println!();
    println!("current branch: \x1b[33m{}\x1b[0m", context.current_branch);
    println!("---------------");
    println!();
    println!("Generating the tree...");
    println!("\x1b[90m(ctrl+c to cancel)\x1b[0m");
    println!();
    io::stdout()
        .flush()
        .context("failed to flush branch status output")?;

    let diagram = with_cli_git_cancellation(|cancel| {
        load_branch_diagram_with_cancel(
            &context.repo_root,
            &context.current_branch,
            context.main_branch_name.as_deref(),
            cancel,
        )
    })?;

    println!("{}", render_branch_tree(diagram.as_ref()));
    println!();
    Ok(())
}

fn run_branch_up() -> Result<()> {
    let context = load_active_branch_cli_context()?;
    let target_branch = with_cli_git_cancellation(|cancel| {
        resolve_parent_branch_name_with_cancel(
            &context.repo_root,
            &context.current_branch,
            context.main_branch_name.as_deref(),
            cancel,
        )
    })?;
    switch_to_existing_branch(&context.repo_root, &target_branch)?;

    println!();
    println!(
        "switched current branch: \x1b[33m{}\x1b[0m → \x1b[33m{}\x1b[0m",
        context.current_branch, target_branch
    );
    println!();
    Ok(())
}

fn run_branch_main() -> Result<()> {
    let context = load_active_branch_cli_context()?;
    let target_branch =
        resolve_main_branch_target(&context.repo_root, context.main_branch_name.as_deref())?;
    switch_to_main_branch(
        &context.repo_root,
        None,
        false,
        context.main_branch_name.as_deref(),
    )?;

    println!();
    println!(
        "switched current branch: \x1b[33m{}\x1b[0m → \x1b[33m{}\x1b[0m",
        context.current_branch, target_branch
    );
    println!();
    Ok(())
}

struct ActiveBranchCliContext {
    repo_root: String,
    main_branch_name: Option<String>,
    current_branch: String,
}

fn load_active_branch_cli_context() -> Result<ActiveBranchCliContext> {
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

    Ok(ActiveBranchCliContext {
        repo_root: context.repo_root.clone(),
        main_branch_name: context.main_branch_name.clone(),
        current_branch,
    })
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

    let mut git_contexts = Vec::new();
    let mut non_main_repo_states = Vec::new();
    let mut repo_operations = Vec::new();
    if workflow != OverviewBumpWorkflow::JustBump {
        if !project.integration_mode.requires_repo() {
            bail!("selected bump option requires a git-backed project")
        }

        git_contexts = collect_all_branch_git_scope_contexts(&resolved_project)?;
        repo_operations = collect_repo_bump_operations(
            &resolved_project,
            &scopes,
            &git_contexts,
            &affected_indexes,
        )?;

        if workflow.requires_branch() {
            non_main_repo_states = collect_non_main_repo_states(
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
        let branch_prompt_source = resolve_branch_prompt_source(
            &repo_operations,
            &git_contexts,
            &non_main_repo_states,
            scheme,
        )?;
        let branch_name_options = suggest_branch_name_options(
            scheme,
            &branch_prompt_source.current_branch,
            &current_version,
            &next_version,
            branch_prompt_source.custom_main_branch.as_deref(),
            Local::now().date_naive(),
        )?;
        Some(prompt_branch_name(&branch_name_options)?)
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

#[derive(Debug, PartialEq, Eq)]
struct CommitDeleteOutcome {
    repo_root: String,
    branch_name: String,
    reverted_commit: String,
    reverted_subject: String,
    revert_commit: String,
    pushed: bool,
    upstream_ref: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CommitRenameStrategy {
    AmendHead,
    RewordAncestor,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CommitRenamePlan {
    pub(crate) repo_root: String,
    pub(crate) branch_name: String,
    pub(crate) target_commit: String,
    pub(crate) target_short: String,
    pub(crate) current_subject: String,
    pub(crate) current_message: String,
    pub(crate) distance_from_head: usize,
    pub(crate) strategy: CommitRenameStrategy,
    pub(crate) upstream_ref: Option<String>,
    pub(crate) touches_pushed_history: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CommitRenameOutcome {
    pub(crate) repo_root: String,
    pub(crate) branch_name: String,
    pub(crate) target_commit: String,
    pub(crate) previous_subject: String,
    pub(crate) new_subject: String,
    pub(crate) head_commit: String,
    pub(crate) strategy: CommitRenameStrategy,
}

struct RenameEditorScripts {
    temp_dir: PathBuf,
    sequence_editor_command: String,
    message_editor_command: String,
}

fn run_commit_delete(commit_hash: &str) -> Result<()> {
    let cwd = env::current_dir().context("failed to read current directory")?;
    let repo_root = current_git_repo_root(&cwd)?;
    let outcome = revert_commit_safely(&repo_root, commit_hash)?;

    println!();
    println!(
        "Safely reverted {} on branch {}.",
        outcome.reverted_commit, outcome.branch_name
    );
    println!("Original commit: {}", outcome.reverted_subject);
    println!("New revert commit: {}", outcome.revert_commit);
    if outcome.pushed {
        println!(
            "Pushed the revert to {}.",
            outcome
                .upstream_ref
                .as_deref()
                .unwrap_or("the upstream branch")
        );
    } else {
        println!("No upstream branch is configured, so the revert remains local.");
    }
    println!();

    Ok(())
}

fn run_commit_rename(commit_target: &str) -> Result<()> {
    let cwd = env::current_dir().context("failed to read current directory")?;
    let repo_root = current_git_repo_root(&cwd)?;
    let plan = prepare_commit_rename(&repo_root, commit_target)?;

    if plan.touches_pushed_history {
        println!();
        println!(
            "Warning: {} is already reachable from {}.",
            plan.target_short,
            plan.upstream_ref
                .as_deref()
                .unwrap_or("the upstream branch")
        );
        println!(
            "Renaming it will rewrite published history on {}.\nDo this only if:\n  A) This is a solo project\n  B) This is a very recent pushed commit\n  C) You are 100% sure no one else is basing work on it\n  D) You coordinate with everyone who is basing work on it to update their branches after you push the rewrite",
            plan.branch_name
        );
        if !prompt_confirm_default_no("Continue with the local rename? [y/N]: ")? {
            bail!("cancelled by user")
        }
    }

    let new_subject = prompt_commit_subject(&plan.current_subject)?;
    let outcome = rename_commit_with_subject(&plan, &new_subject)?;

    let mut force_pushed = false;
    if plan.touches_pushed_history && plan.upstream_ref.is_some() {
        force_pushed = prompt_confirm_default_no(&format!(
            "Push the rewritten branch to {} with --force-with-lease? [y/N]: ",
            plan.upstream_ref
                .as_deref()
                .unwrap_or("the upstream branch")
        ))?;
        if force_pushed {
            push_branch_force_with_lease(&plan.repo_root)?;
        }
    }

    println!();
    println!(
        "Renamed {} on branch {}.",
        outcome.target_commit, outcome.branch_name
    );
    println!("Previous subject: {}", outcome.previous_subject);
    println!("New subject: {}", outcome.new_subject);
    println!("Current HEAD: {}", outcome.head_commit);
    if force_pushed {
        println!(
            "Force-pushed the rewritten branch to {} with --force-with-lease.",
            plan.upstream_ref
                .as_deref()
                .unwrap_or("the upstream branch")
        );
    } else if plan.touches_pushed_history {
        println!("The history rewrite is local only until you force-push it.");
    }
    println!();

    Ok(())
}

pub(crate) fn prepare_commit_rename(
    repo_root: &str,
    commit_target: &str,
) -> Result<CommitRenamePlan> {
    let commit_target = commit_target.trim();
    if commit_target.is_empty() {
        bail!("commit target cannot be empty")
    }

    ensure_clean_worktree(repo_root)?;

    let branch_name = current_branch_with_cancel(repo_root, None)?;
    if branch_name.starts_with("detached (") {
        bail!("cg commit rename requires a checked-out branch; detached HEAD is not supported")
    }

    let verified_commit = resolve_commit_target(repo_root, commit_target)?;
    if !is_commit_ancestor_of_head(repo_root, &verified_commit)? {
        bail!(
            "commit {} is not reachable from the current branch {}; switch to the branch that contains it first",
            verified_commit,
            branch_name
        )
    }

    let distance_from_head = commit_distance_from_head(repo_root, &verified_commit)?;
    let strategy = if distance_from_head == 0 {
        CommitRenameStrategy::AmendHead
    } else {
        CommitRenameStrategy::RewordAncestor
    };
    let current_subject = commit_subject(repo_root, &verified_commit)?;
    let current_message = commit_message(repo_root, &verified_commit)?;
    let upstream_ref = current_upstream_ref(repo_root)?;
    let touches_pushed_history = upstream_ref.as_deref().is_some_and(|upstream| {
        is_commit_ancestor_of_ref(repo_root, &verified_commit, upstream).unwrap_or(false)
    });

    Ok(CommitRenamePlan {
        repo_root: repo_root.to_string(),
        branch_name,
        target_short: short_commit_hash(repo_root, &verified_commit)?,
        target_commit: verified_commit,
        current_subject,
        current_message,
        distance_from_head,
        strategy,
        upstream_ref,
        touches_pushed_history,
    })
}

pub(crate) fn rename_commit_with_subject(
    plan: &CommitRenamePlan,
    new_subject: &str,
) -> Result<CommitRenameOutcome> {
    let new_subject = new_subject.trim();
    if new_subject.is_empty() {
        bail!("new commit message cannot be empty")
    }
    if new_subject == plan.current_subject {
        bail!("new commit message matches the current subject")
    }

    let updated_message = replace_commit_subject(&plan.current_message, new_subject);
    match plan.strategy {
        CommitRenameStrategy::AmendHead => {
            amend_head_commit_message(&plan.repo_root, &updated_message)?
        }
        CommitRenameStrategy::RewordAncestor => {
            reword_ancestor_commit_message(
                &plan.repo_root,
                &plan.target_commit,
                &plan.target_short,
                &updated_message,
            )?;
        }
    }

    Ok(CommitRenameOutcome {
        repo_root: plan.repo_root.clone(),
        branch_name: plan.branch_name.clone(),
        target_commit: plan.target_short.clone(),
        previous_subject: plan.current_subject.clone(),
        new_subject: new_subject.to_string(),
        head_commit: short_commit_hash(&plan.repo_root, "HEAD")?,
        strategy: plan.strategy.clone(),
    })
}

pub(crate) fn push_branch_force_with_lease(repo_root: &str) -> Result<()> {
    run_git_checked(repo_root, &["push", "--force-with-lease"])
        .context("failed to push the rewritten branch with --force-with-lease")?;
    Ok(())
}

fn current_git_repo_root(cwd: &Path) -> Result<String> {
    let cwd_display = cwd.display().to_string();
    Ok(
        run_git_checked(&cwd_display, &["rev-parse", "--show-toplevel"])
            .context("the current directory is not inside a git repository")?
            .trim()
            .to_string(),
    )
}

fn revert_commit_safely(repo_root: &str, commit_hash: &str) -> Result<CommitDeleteOutcome> {
    let commit_hash = commit_hash.trim();
    if commit_hash.is_empty() {
        bail!("commit hash cannot be empty")
    }

    ensure_clean_worktree(repo_root)?;

    let branch_name = current_branch_with_cancel(repo_root, None)?;
    if branch_name.starts_with("detached (") {
        bail!("cg commit del requires a checked-out branch; detached HEAD is not supported")
    }

    let verified_commit = verify_commit_hash(repo_root, commit_hash)?;
    if !is_commit_ancestor_of_head(repo_root, &verified_commit)? {
        bail!(
            "commit {} is not reachable from the current branch {}; switch to the branch that contains it first",
            verified_commit,
            branch_name
        )
    }

    let use_merge_mainline = revert_uses_mainline_parent(repo_root, &verified_commit)?;

    let reverted_subject = commit_subject(repo_root, &verified_commit)?;
    let mut revert_args = vec!["revert", "--no-edit"];
    if use_merge_mainline {
        revert_args.extend(["-m", "1"]);
    }
    revert_args.push(verified_commit.as_str());
    run_git_checked(repo_root, &revert_args).with_context(|| {
        format!(
            "failed to revert commit {}; if git reported conflicts, resolve them and run 'git revert --continue' or abort with 'git revert --abort'",
            verified_commit
        )
    })?;

    let revert_commit = run_git_checked(repo_root, &["rev-parse", "--short", "HEAD"])?
        .trim()
        .to_string();
    let upstream_ref = current_upstream_ref(repo_root)?;
    let pushed = if upstream_ref.is_some() {
        if let Err(error) = run_git_checked(repo_root, &["push"]) {
            bail!(
                "reverted commit {} locally as {}, but failed to push it: {}",
                verified_commit,
                revert_commit,
                error
            )
        }
        true
    } else {
        false
    };

    Ok(CommitDeleteOutcome {
        repo_root: repo_root.to_string(),
        branch_name,
        reverted_commit: verified_commit,
        reverted_subject,
        revert_commit,
        pushed,
        upstream_ref,
    })
}

fn ensure_clean_worktree(repo_root: &str) -> Result<()> {
    let status = run_git_checked(repo_root, &["status", "--porcelain"])?;
    if status.trim().is_empty() {
        Ok(())
    } else {
        bail!(
            "the git working tree has uncommitted changes; commit, stash, or discard them before running cg commit del"
        )
    }
}

fn verify_commit_hash(repo_root: &str, commit_hash: &str) -> Result<String> {
    let revision = format!("{}^{{commit}}", commit_hash);
    Ok(
        run_git_checked(repo_root, &["rev-parse", "--verify", &revision])
            .with_context(|| format!("commit '{}' was not found", commit_hash))?
            .trim()
            .to_string(),
    )
}

fn is_commit_ancestor_of_head(repo_root: &str, commit_hash: &str) -> Result<bool> {
    let output = run_git(
        repo_root,
        &["merge-base", "--is-ancestor", commit_hash, "HEAD"],
    )?;
    Ok(output.success)
}

fn is_commit_ancestor_of_ref(repo_root: &str, commit_hash: &str, reference: &str) -> Result<bool> {
    let output = run_git(
        repo_root,
        &["merge-base", "--is-ancestor", commit_hash, reference],
    )?;
    Ok(output.success)
}

fn resolve_commit_target(repo_root: &str, commit_target: &str) -> Result<String> {
    if commit_target
        .chars()
        .all(|character| character.is_ascii_digit())
    {
        let offset = commit_target
            .parse::<usize>()
            .with_context(|| format!("'{}' is not a valid HEAD offset", commit_target))?;
        let revision = if offset == 0 {
            "HEAD".to_string()
        } else {
            format!("HEAD~{}", offset)
        };
        return verify_commit_hash(repo_root, &revision).with_context(|| {
            format!(
                "HEAD offset {} is not available on the current branch",
                offset
            )
        });
    }

    verify_commit_hash(repo_root, commit_target)
}

fn commit_distance_from_head(repo_root: &str, commit_hash: &str) -> Result<usize> {
    let count = run_git_checked(
        repo_root,
        &["rev-list", "--count", &format!("{}..HEAD", commit_hash)],
    )?;
    count.trim().parse::<usize>().with_context(|| {
        format!(
            "failed to measure the distance from {} to HEAD",
            commit_hash
        )
    })
}

fn revert_uses_mainline_parent(repo_root: &str, commit_hash: &str) -> Result<bool> {
    let output = run_git_checked(
        repo_root,
        &["rev-list", "--parents", "-n", "1", commit_hash],
    )?;
    let parent_count = output.split_whitespace().count().saturating_sub(1);
    match parent_count {
        0 | 1 => Ok(false),
        2 => Ok(true),
        _ => bail!(
            "commit {} is an octopus merge with {} parents; revert it manually with 'git revert -m <parent-number> {}'",
            commit_hash,
            parent_count,
            commit_hash
        ),
    }
}

fn commit_subject(repo_root: &str, commit_hash: &str) -> Result<String> {
    Ok(run_git_checked(
        repo_root,
        &["show", "--no-patch", "--format=%s", commit_hash],
    )?
    .trim()
    .to_string())
}

fn commit_message(repo_root: &str, commit_hash: &str) -> Result<String> {
    run_git_checked(
        repo_root,
        &["show", "--no-patch", "--format=%B", commit_hash],
    )
}

fn short_commit_hash(repo_root: &str, commit_hash: &str) -> Result<String> {
    Ok(
        run_git_checked(repo_root, &["rev-parse", "--short", commit_hash])?
            .trim()
            .to_string(),
    )
}

fn current_upstream_ref(repo_root: &str) -> Result<Option<String>> {
    let output = run_git(
        repo_root,
        &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
    )?;
    if !output.success {
        return Ok(None);
    }

    let upstream = output.stdout.trim();
    if upstream.is_empty() {
        Ok(None)
    } else {
        Ok(Some(upstream.to_string()))
    }
}

fn replace_commit_subject(current_message: &str, new_subject: &str) -> String {
    if let Some((_, remainder)) = current_message.split_once('\n') {
        format!("{}\n{}", new_subject, remainder)
    } else {
        format!("{}\n", new_subject)
    }
}

fn amend_head_commit_message(repo_root: &str, message: &str) -> Result<()> {
    let temp_file = write_temp_commit_message_file(message)?;
    let temp_file_arg = temp_file.to_string_lossy().to_string();
    let result = run_git_checked(
        repo_root,
        &["commit", "--amend", "-F", temp_file_arg.as_str()],
    );
    let _ = fs::remove_file(&temp_file);
    result.context("failed to amend the current HEAD commit message")?;
    Ok(())
}

fn reword_ancestor_commit_message(
    repo_root: &str,
    target_commit: &str,
    target_short: &str,
    message: &str,
) -> Result<()> {
    let scripts = create_rename_editor_scripts(message, target_commit, target_short)?;
    let rebase_args = if commit_has_parents(repo_root, target_commit)? {
        vec![
            "rebase".to_string(),
            "-i".to_string(),
            "--rebase-merges".to_string(),
            format!("{}^", target_commit),
        ]
    } else {
        vec![
            "rebase".to_string(),
            "-i".to_string(),
            "--rebase-merges".to_string(),
            "--root".to_string(),
        ]
    };

    let output = run_git_command_with_env(
        repo_root,
        &rebase_args,
        &[
            (
                "GIT_SEQUENCE_EDITOR",
                scripts.sequence_editor_command.as_str(),
            ),
            ("GIT_EDITOR", scripts.message_editor_command.as_str()),
        ],
    )
    .context("failed to start the commit reword rebase")?;

    let _ = fs::remove_dir_all(&scripts.temp_dir);

    if !output.status.success() {
        let _ = run_git(repo_root, &["rebase", "--abort"]);
        bail!(
            "failed to rename commit {}; git reported: {}",
            target_short,
            format_git_command_error(&output)
        )
    }

    Ok(())
}

fn commit_has_parents(repo_root: &str, commit_hash: &str) -> Result<bool> {
    let output = run_git_checked(
        repo_root,
        &["rev-list", "--parents", "-n", "1", commit_hash],
    )?;
    Ok(output.split_whitespace().count().saturating_sub(1) > 0)
}

fn prompt_commit_subject(current_subject: &str) -> Result<String> {
    println!();
    println!("Current commit message: {}", current_subject);
    print!("New commit message: ");
    io::stdout()
        .flush()
        .context("failed to flush commit message prompt")?;

    let mut answer = String::new();
    io::stdin()
        .read_line(&mut answer)
        .context("failed to read the new commit message")?;

    let answer = answer.trim().to_string();
    if answer.is_empty() {
        bail!("new commit message cannot be empty")
    }

    Ok(answer)
}

fn prompt_confirm_default_no(prompt: &str) -> Result<bool> {
    loop {
        print!("{}", prompt);
        io::stdout().flush().context("failed to flush prompt")?;

        let mut answer = String::new();
        io::stdin()
            .read_line(&mut answer)
            .context("failed to read response")?;

        match answer.trim().to_lowercase().as_str() {
            "y" | "yes" => return Ok(true),
            "" | "n" | "no" => return Ok(false),
            other => println!("Please answer Y or N. Received: {}", other),
        }
    }
}

fn write_temp_commit_message_file(message: &str) -> Result<PathBuf> {
    let temp_file = env::temp_dir().join(format!(
        "comfygit-rename-message-{}-{}.txt",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::write(&temp_file, message).with_context(|| {
        format!(
            "failed to write the temporary commit message file at {}",
            temp_file.display()
        )
    })?;
    Ok(temp_file)
}

fn create_rename_editor_scripts(
    message: &str,
    target_commit: &str,
    target_short: &str,
) -> Result<RenameEditorScripts> {
    let temp_dir = env::temp_dir().join(format!(
        "comfygit-rename-scripts-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::create_dir_all(&temp_dir).with_context(|| {
        format!(
            "failed to create the temporary rename script directory at {}",
            temp_dir.display()
        )
    })?;

    let message_file = temp_dir.join("commit-message.txt");
    fs::write(&message_file, message).with_context(|| {
        format!(
            "failed to write the temporary commit message file at {}",
            message_file.display()
        )
    })?;

    if cfg!(windows) {
        let sequence_script = temp_dir.join("sequence-editor.ps1");
        fs::write(
            &sequence_script,
            format!(
                r#"$todoPath = $args[0]
$targetFull = '{target_full}'
$targetShort = '{target_short}'
$lines = Get-Content -LiteralPath $todoPath
$updated = $false
$rewritten = foreach ($line in $lines) {{
    if (-not $updated -and $line -match "^\s*pick\s+({target_full}|{target_short})(\s|$)") {{
        $updated = $true
        $line -replace '^\s*pick\s+', 'reword '
    }} elseif (-not $updated -and $line -match "^\s*merge\s+-C\s+({target_full}|{target_short})(\s|$)") {{
        $updated = $true
        $line -replace '^\s*merge\s+-C\s+', 'merge -c '
    }} else {{
        $line
    }}
}}
if (-not $updated) {{
    Write-Error 'Target commit was not found in the rebase todo list.'
    exit 1
}}
Set-Content -LiteralPath $todoPath -Value $rewritten
"#,
                target_full = target_commit,
                target_short = target_short,
            ),
        )
        .context("failed to write the PowerShell sequence editor script")?;
        let sequence_wrapper = temp_dir.join("sequence-editor.cmd");
        fs::write(
            &sequence_wrapper,
            format!(
                "@echo off\r\npowershell.exe -NoProfile -ExecutionPolicy Bypass -File \"{}\" %*\r\n",
                sequence_script.display()
            ),
        )
        .context("failed to write the sequence editor wrapper")?;

        let message_script = temp_dir.join("message-editor.ps1");
        fs::write(
            &message_script,
            format!(
                "$destinationPath = $args[0]\nCopy-Item -LiteralPath '{}' -Destination $destinationPath -Force\n",
                powershell_literal(&message_file)
            ),
        )
        .context("failed to write the PowerShell message editor script")?;
        let message_wrapper = temp_dir.join("message-editor.cmd");
        fs::write(
            &message_wrapper,
            format!(
                "@echo off\r\npowershell.exe -NoProfile -ExecutionPolicy Bypass -File \"{}\" %*\r\n",
                message_script.display()
            ),
        )
        .context("failed to write the message editor wrapper")?;

        return Ok(RenameEditorScripts {
            temp_dir,
            sequence_editor_command: format!("\"{}\"", sequence_wrapper.display()),
            message_editor_command: format!("\"{}\"", message_wrapper.display()),
        });
    }

    let sequence_script = temp_dir.join("sequence-editor.sh");
    fs::write(
        &sequence_script,
        format!(
            "#!/bin/sh\nset -eu\ntodo_path=\"$1\"\nawk 'BEGIN {{ updated = 0 }}\n{{\n  if (!updated && $1 == \"pick\" && ($2 == \"{target_full}\" || $2 == \"{target_short}\")) {{ $1 = \"reword\"; updated = 1 }}\n  else if (!updated && $1 == \"merge\" && $2 == \"-C\" && ($3 == \"{target_full}\" || $3 == \"{target_short}\")) {{ $2 = \"-c\"; updated = 1 }}\n  print\n}}\nEND {{ if (!updated) exit 9 }}' \"$todo_path\" > \"$todo_path.tmp\"\nmv \"$todo_path.tmp\" \"$todo_path\"\n",
            target_full = target_commit,
            target_short = target_short,
        ),
    )
    .context("failed to write the shell sequence editor script")?;
    let message_script = temp_dir.join("message-editor.sh");
    fs::write(
        &message_script,
        format!(
            "#!/bin/sh\nset -eu\ncat {} > \"$1\"\n",
            shell_literal(&message_file)
        ),
    )
    .context("failed to write the shell message editor script")?;
    set_script_executable(&sequence_script)?;
    set_script_executable(&message_script)?;

    Ok(RenameEditorScripts {
        temp_dir,
        sequence_editor_command: shell_literal(&sequence_script),
        message_editor_command: shell_literal(&message_script),
    })
}

fn powershell_literal(path: &Path) -> String {
    path.display().to_string().replace('\'', "''")
}

fn shell_literal(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', "'\\''"))
}

#[cfg(unix)]
fn set_script_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)
        .with_context(|| format!("failed to read script permissions for {}", path.display()))?
        .permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(path, permissions)
        .with_context(|| format!("failed to make {} executable", path.display()))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_script_executable(_path: &Path) -> Result<()> {
    Ok(())
}

struct GitCommandCapture {
    status: std::process::ExitStatus,
    stdout: String,
    stderr: String,
}

fn run_git_command_with_env(
    repo_root: &str,
    args: &[String],
    envs: &[(&str, &str)],
) -> Result<GitCommandCapture> {
    let mut command = Command::new("git");
    command.current_dir(repo_root);
    command.args(args.iter().map(String::as_str));
    for (key, value) in envs {
        command.env(key, value);
    }
    let output = command.output().context("failed to run git command")?;
    Ok(GitCommandCapture {
        status: output.status,
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

fn format_git_command_error(output: &GitCommandCapture) -> String {
    let stderr = output.stderr.trim();
    if !stderr.is_empty() {
        return stderr.to_string();
    }
    let stdout = output.stdout.trim();
    if !stdout.is_empty() {
        return stdout.to_string();
    }
    match output.status.code() {
        Some(code) => format!("git exited with status {}", code),
        None => "git terminated before it returned a status code".to_string(),
    }
}

fn parse_cli_bump_option(value: Option<&str>) -> Result<OverviewBumpWorkflow> {
    match value.map(str::trim).filter(|value| !value.is_empty()) {
        None => Ok(OverviewBumpWorkflow::JustBump),
        Some("1") => Ok(OverviewBumpWorkflow::JustBump),
        Some("2") => Ok(OverviewBumpWorkflow::Commit),
        Some("3") => Ok(OverviewBumpWorkflow::CommitAndPush),
        Some("4") => Ok(OverviewBumpWorkflow::BranchCommit),
        Some("5") => Ok(OverviewBumpWorkflow::BranchCommitAndPush),
        Some(other) => bail!("unsupported bump option '{}'; expected 1-5", other),
    }
}

#[derive(Clone)]
struct BranchPromptSource {
    current_branch: String,
    custom_main_branch: Option<String>,
}

struct CliRawModeGuard;

impl CliRawModeGuard {
    fn enter() -> Result<Self> {
        enable_raw_mode().context("failed to enable raw mode for branch name selection")?;
        Ok(Self)
    }
}

impl Drop for CliRawModeGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
    }
}

fn resolve_branch_prompt_source(
    repo_operations: &[crate::app::RepoBumpOperation],
    git_contexts: &[crate::git::GitScopeContext],
    non_main_repo_states: &[crate::app::git_flow::RepoBranchState],
    scheme: VersionScheme,
) -> Result<BranchPromptSource> {
    let preferred_repo_root = non_main_repo_states
        .iter()
        .find(|state| is_release_line_branch(scheme, &state.current_branch))
        .or_else(|| non_main_repo_states.first())
        .map(|state| state.repo_root.as_str())
        .or_else(|| {
            repo_operations
                .first()
                .map(|operation| operation.repo_root.as_str())
        })
        .ok_or_else(|| anyhow!("the selected workflow requires a git-backed repository"))?;

    let context = git_contexts
        .iter()
        .find(|context| context.repo_root == preferred_repo_root)
        .ok_or_else(|| anyhow!("git scope metadata is unavailable for the selected repository"))?;
    let current_branch = non_main_repo_states
        .iter()
        .find(|state| state.repo_root == preferred_repo_root)
        .map(|state| state.current_branch.clone())
        .unwrap_or(current_branch_with_cancel(preferred_repo_root, None)?);

    Ok(BranchPromptSource {
        current_branch,
        custom_main_branch: context.main_branch_name.clone(),
    })
}

fn prompt_branch_name(options: &[BranchNameOption]) -> Result<String> {
    if options.is_empty() {
        bail!("branch name options are unavailable")
    }

    let mut selected = 0usize;
    let raw_mode = CliRawModeGuard::enter()?;
    let mut rendered_lines = 0usize;

    loop {
        render_cli_branch_name_picker(options, selected, &mut rendered_lines)?;

        let Event::Key(key) = event::read().context("failed to read branch name selection")? else {
            continue;
        };
        if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
            continue;
        }

        match key.code {
            KeyCode::Esc => {
                drop(raw_mode);
                println!();
                bail!("Cancelled by user")
            }
            KeyCode::Up | KeyCode::BackTab => {
                selected = selected.checked_sub(1).unwrap_or(options.len() - 1);
            }
            KeyCode::Down | KeyCode::Tab => {
                selected = (selected + 1) % options.len();
            }
            KeyCode::Char(character) => {
                if let Some(index) = digit_to_index(character) {
                    selected = index.min(options.len().saturating_sub(1));
                }
            }
            KeyCode::Enter | KeyCode::F(2) => {
                let option = options[selected].clone();
                drop(raw_mode);
                println!();
                let input = if option.requires_input() {
                    Some(prompt_branch_name_input(option.input_label())?)
                } else {
                    None
                };
                return option.resolve_name(input.as_deref());
            }
            _ => {}
        }
    }
}

fn render_cli_branch_name_picker(
    options: &[BranchNameOption],
    selected: usize,
    rendered_lines: &mut usize,
) -> Result<()> {
    let mut stdout = io::stdout();
    if *rendered_lines > 0 {
        execute!(
            stdout,
            MoveUp(*rendered_lines as u16),
            MoveToColumn(0),
            Clear(ClearType::FromCursorDown)
        )
        .context("failed to redraw branch name picker")?;
    }

    queue!(
        stdout,
        MoveToColumn(0),
        Print("Choose the new branch name:\r\n"),
        MoveToColumn(0),
        Print("Use Up/Down or Tab to select, then press Enter.\r\n")
    )
    .context("failed to render branch name picker")?;

    for (index, option) in options.iter().enumerate() {
        let marker = if index == selected { ">" } else { " " };
        let color = if index == selected {
            Color::Yellow
        } else {
            Color::DarkGrey
        };
        queue!(
            stdout,
            MoveToColumn(0),
            SetForegroundColor(color),
            Print(format!(
                "{} {}. {}\r\n",
                marker,
                index + 1,
                option.preview()
            )),
            ResetColor
        )
        .context("failed to render branch name picker option")?;
    }
    stdout
        .flush()
        .context("failed to flush branch name picker")?;
    *rendered_lines = options.len() + 2;
    Ok(())
}

fn prompt_branch_name_input(label: &str) -> Result<String> {
    print!("{}: ", label);
    io::stdout()
        .flush()
        .context("failed to flush branch name input prompt")?;

    let mut branch_name = String::new();
    io::stdin()
        .read_line(&mut branch_name)
        .context("failed to read branch name input")?;

    Ok(branch_name.trim().to_string())
}

fn digit_to_index(character: char) -> Option<usize> {
    character
        .to_digit(10)
        .and_then(|digit| digit.checked_sub(1))
        .map(|digit| digit as usize)
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

#[derive(Clone, Debug, PartialEq, Eq)]
struct BranchLineage {
    root: BranchRef,
    path: Vec<BranchRef>,
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

#[derive(Clone, Debug, PartialEq, Eq)]
struct BranchTreeData {
    root: BranchRef,
    family: Vec<BranchRef>,
    path: Vec<BranchRef>,
}

#[cfg_attr(not(test), allow(dead_code))]
fn load_branch_diagram(
    repo_root: &str,
    current_branch: &str,
    custom_main_branch: Option<&str>,
) -> Result<Option<BranchDiagram>> {
    load_branch_diagram_with_cancel(repo_root, current_branch, custom_main_branch, None)
}

fn load_branch_diagram_with_cancel(
    repo_root: &str,
    current_branch: &str,
    custom_main_branch: Option<&str>,
    cancel: Option<GitCancellation>,
) -> Result<Option<BranchDiagram>> {
    let Some(tree) = build_branch_tree_data_with_cancel(
        repo_root,
        current_branch,
        custom_main_branch,
        true,
        cancel.clone(),
    )?
    else {
        return Ok(None);
    };

    let mut path = tree
        .path
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

    let merged_sets = tree
        .path
        .iter()
        .map(|branch| {
            local_branch_names_merged_into_with_cancel(repo_root, &branch.refname, cancel.clone())
        })
        .collect::<Result<Vec<_>>>()?;

    for branch in tree.family {
        if tree
            .path
            .iter()
            .any(|path_branch| path_branch.object_id == branch.object_id)
        {
            continue;
        }

        let branch_lookup = normalize_lookup(&branch.name);
        if let Some(index) = merged_sets
            .iter()
            .position(|merged| merged.contains(&branch_lookup))
        {
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
            name: display_branch_name(&tree.root.name),
            state: BranchDiagramState::Main,
        },
        path,
    }))
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
            .ok_or_else(|| anyhow!("current branch is not available among local refs"))?
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
        let output = run_git_checked_owned_with_cancel(
            repo_root,
            vec!["rev-list".to_string(), "--count".to_string(), range.clone()],
            cancel.clone(),
        )?;
        branch.root_distance = output
            .trim()
            .parse::<usize>()
            .with_context(|| format!("failed to parse git ancestry distance for {}", range))?;
    }

    Ok(())
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

#[cfg_attr(not(test), allow(dead_code))]
fn resolve_parent_branch_name(
    repo_root: &str,
    current_branch: &str,
    custom_main_branch: Option<&str>,
) -> Result<String> {
    resolve_parent_branch_name_with_cancel(repo_root, current_branch, custom_main_branch, None)
}

fn resolve_parent_branch_name_with_cancel(
    repo_root: &str,
    current_branch: &str,
    custom_main_branch: Option<&str>,
    cancel: Option<GitCancellation>,
) -> Result<String> {
    let lineage =
        load_branch_lineage_with_cancel(repo_root, current_branch, custom_main_branch, cancel)?
            .ok_or_else(|| anyhow!("no local branches are available in this repository"))?;
    if lineage.root.name.eq_ignore_ascii_case(current_branch) {
        bail!("current branch is already the main branch")
    }

    let current_index = lineage
        .path
        .iter()
        .position(|branch| branch.name.eq_ignore_ascii_case(current_branch))
        .ok_or_else(|| anyhow!("current branch is not part of the current branch tree"))?;

    let target = if current_index == 0 {
        lineage.root.name
    } else {
        lineage.path[current_index - 1].name.clone()
    };
    Ok(target)
}

fn resolve_main_branch_target(repo_root: &str, custom_main_branch: Option<&str>) -> Result<String> {
    resolve_main_branch_name(repo_root, custom_main_branch)
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

fn find_repo_custom_main_branch(repo_root: &str) -> Option<String> {
    let canonical_repo_root = best_effort_canonicalize(Path::new(repo_root));
    let config = load_config().ok()?;

    for project in &config.projects {
        if let Some(repo) = project.repo.as_ref()
            && best_effort_canonicalize(&repo_root_path(repo)) == canonical_repo_root
            && let Some(branch) = repo.custom_main_branch_name()
        {
            return Some(branch.to_string());
        }

        for branch in &project.branches {
            if let Some(repo) = branch.repo.as_ref()
                && best_effort_canonicalize(&repo_root_path(repo)) == canonical_repo_root
                && let Some(branch_name) = repo.custom_main_branch_name()
            {
                return Some(branch_name.to_string());
            }
        }
    }

    None
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
    use std::{
        env, fs,
        time::{SystemTime, UNIX_EPOCH},
    };

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
            tile_info: crate::config::TileInfoSettings::default(),
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
            OverviewBumpWorkflow::BranchCommit
        );
        assert_eq!(
            parse_cli_bump_option(Some("5")).expect("option 5 should parse"),
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
    fn is_commit_command_accepts_requested_synonyms() {
        assert!(is_commit_command("commit"));
        assert!(is_commit_command("cmt"));
        assert!(is_commit_command("com"));
        assert!(is_commit_command("ct"));
        assert!(!is_commit_command("cmd"));
    }

    #[test]
    fn is_commit_delete_action_accepts_requested_synonyms() {
        assert!(is_commit_delete_action("del"));
        assert!(is_commit_delete_action("rm"));
        assert!(is_commit_delete_action("rem"));
        assert!(is_commit_delete_action("delete"));
        assert!(is_commit_delete_action("drop"));
        assert!(is_commit_delete_action("erase"));
        assert!(!is_commit_delete_action("remove"));
    }

    #[test]
    fn is_commit_rename_action_accepts_requested_synonyms() {
        assert!(is_commit_rename_action("rename"));
        assert!(is_commit_rename_action("rn"));
        assert!(is_commit_rename_action("rnm"));
        assert!(is_commit_rename_action("reword"));
        assert!(is_commit_rename_action("rwrd"));
        assert!(is_commit_rename_action("rwd"));
        assert!(!is_commit_rename_action("rew"));
    }

    #[test]
    fn replace_commit_subject_preserves_body() {
        assert_eq!(
            replace_commit_subject("old subject\n\nbody line\n", "new subject"),
            "new subject\n\nbody line\n"
        );
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
    fn is_branch_up_action_accepts_requested_synonyms() {
        assert!(is_branch_up_action("up"));
        assert!(is_branch_up_action(".."));
        assert!(!is_branch_up_action("parent"));
    }

    #[test]
    fn is_branch_main_action_accepts_requested_synonyms() {
        assert!(is_branch_main_action("main"));
        assert!(is_branch_main_action("~"));
        assert!(!is_branch_main_action("root"));
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

    fn create_temp_repo_dir(test_name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir = env::temp_dir().join(format!(
            "comfygit-cli-{}-{}-{}",
            test_name,
            std::process::id(),
            unique
        ));
        fs::create_dir_all(&dir).expect("create temp repo dir");
        dir
    }

    fn init_temp_git_repo(repo_root: &str) {
        run_git_checked(repo_root, &["init"]).expect("init repo");
        run_git_checked(repo_root, &["config", "user.name", "ComfyGit Tests"])
            .expect("configure user.name");
        run_git_checked(
            repo_root,
            &["config", "user.email", "tests@comfygit.invalid"],
        )
        .expect("configure user.email");
        let switch_main = run_git(repo_root, &["switch", "-c", "main"]).expect("switch main");
        if !switch_main.success {
            run_git_checked(repo_root, &["checkout", "-b", "main"]).expect("checkout main");
        }
    }

    #[test]
    fn rename_commit_with_subject_amends_head_commit() {
        let repo_dir = create_temp_repo_dir("commit-rename-head");
        let repo_root = repo_dir.to_string_lossy().to_string();

        init_temp_git_repo(&repo_root);

        let tracked_file = repo_dir.join("tracked.txt");
        fs::write(&tracked_file, "base\n").expect("write base file");
        run_git_checked(&repo_root, &["add", "tracked.txt"]).expect("stage base file");
        run_git_checked(&repo_root, &["commit", "-m", "base"]).expect("commit base file");

        fs::write(&tracked_file, "base\nhead\n").expect("write head file");
        run_git_checked(&repo_root, &["add", "tracked.txt"]).expect("stage head file");
        run_git_checked(&repo_root, &["commit", "-m", "old head subject"])
            .expect("commit head change");

        let plan = prepare_commit_rename(&repo_root, "0").expect("prepare head rename plan");
        assert_eq!(plan.strategy, CommitRenameStrategy::AmendHead);

        let outcome =
            rename_commit_with_subject(&plan, "new head subject").expect("rename head commit");

        assert_eq!(outcome.previous_subject, "old head subject");
        assert_eq!(outcome.new_subject, "new head subject");
        assert_eq!(
            run_git_checked(&repo_root, &["show", "--no-patch", "--format=%s", "HEAD"])
                .expect("read amended subject")
                .trim(),
            "new head subject"
        );

        fs::remove_dir_all(&repo_dir).expect("remove temp repo dir");
    }

    #[test]
    fn rename_commit_with_subject_rewords_ancestor_commit() {
        let repo_dir = create_temp_repo_dir("commit-rename-ancestor");
        let repo_root = repo_dir.to_string_lossy().to_string();

        init_temp_git_repo(&repo_root);

        let tracked_file = repo_dir.join("tracked.txt");
        fs::write(&tracked_file, "base\n").expect("write base file");
        run_git_checked(&repo_root, &["add", "tracked.txt"]).expect("stage base file");
        run_git_checked(&repo_root, &["commit", "-m", "base"]).expect("commit base file");

        fs::write(&tracked_file, "base\nsecond\n").expect("write second file");
        run_git_checked(&repo_root, &["add", "tracked.txt"]).expect("stage second file");
        run_git_checked(&repo_root, &["commit", "-m", "old middle subject"])
            .expect("commit second change");
        let target_commit = run_git_checked(&repo_root, &["rev-parse", "HEAD"])
            .expect("read target commit")
            .trim()
            .to_string();

        fs::write(&tracked_file, "base\nsecond\nthird\n").expect("write third file");
        run_git_checked(&repo_root, &["add", "tracked.txt"]).expect("stage third file");
        run_git_checked(&repo_root, &["commit", "-m", "top subject"]).expect("commit top change");

        let plan = prepare_commit_rename(&repo_root, &target_commit)
            .expect("prepare ancestor rename plan");
        assert_eq!(plan.strategy, CommitRenameStrategy::RewordAncestor);

        let outcome = rename_commit_with_subject(&plan, "new middle subject")
            .expect("rename ancestor commit");

        assert_eq!(outcome.previous_subject, "old middle subject");
        assert_eq!(outcome.new_subject, "new middle subject");

        let subjects = split_output_lines(
            &run_git_checked(&repo_root, &["log", "--format=%s", "-n", "3"])
                .expect("read commit subjects"),
        );
        assert_eq!(
            subjects,
            vec![
                "top subject".to_string(),
                "new middle subject".to_string(),
                "base".to_string(),
            ]
        );

        fs::remove_dir_all(&repo_dir).expect("remove temp repo dir");
    }

    #[test]
    fn revert_commit_safely_creates_local_revert_commit() {
        let repo_dir = create_temp_repo_dir("commit-del");
        let repo_root = repo_dir.to_string_lossy().to_string();

        run_git_checked(&repo_root, &["init"]).expect("init repo");
        run_git_checked(&repo_root, &["config", "user.name", "ComfyGit Tests"])
            .expect("configure user.name");
        run_git_checked(
            &repo_root,
            &["config", "user.email", "tests@comfygit.invalid"],
        )
        .expect("configure user.email");
        let switch_main = run_git(&repo_root, &["switch", "-c", "main"]).expect("switch main");
        if !switch_main.success {
            run_git_checked(&repo_root, &["checkout", "-b", "main"]).expect("checkout main");
        }

        let tracked_file = repo_dir.join("tracked.txt");
        fs::write(&tracked_file, "base\n").expect("write base file");
        run_git_checked(&repo_root, &["add", "tracked.txt"]).expect("stage base file");
        run_git_checked(&repo_root, &["commit", "-m", "base"]).expect("commit base file");

        fs::write(&tracked_file, "changed\n").expect("write changed file");
        run_git_checked(&repo_root, &["add", "tracked.txt"]).expect("stage changed file");
        run_git_checked(&repo_root, &["commit", "-m", "change tracked file"])
            .expect("commit changed file");

        let target_commit = run_git_checked(&repo_root, &["rev-parse", "HEAD"])
            .expect("read target commit")
            .trim()
            .to_string();

        let outcome =
            revert_commit_safely(&repo_root, &target_commit).expect("safe revert should succeed");

        assert_eq!(outcome.reverted_commit, target_commit);
        assert_eq!(outcome.reverted_subject, "change tracked file");
        assert!(!outcome.revert_commit.is_empty());
        assert!(!outcome.pushed);
        assert!(outcome.upstream_ref.is_none());
        assert_eq!(
            fs::read_to_string(&tracked_file)
                .expect("read reverted file")
                .replace("\r\n", "\n"),
            "base\n"
        );

        let head_subject =
            run_git_checked(&repo_root, &["show", "--no-patch", "--format=%s", "HEAD"])
                .expect("read revert subject");
        assert!(head_subject.contains("Revert \"change tracked file\""));

        fs::remove_dir_all(&repo_dir).expect("remove temp repo dir");
    }

    #[test]
    fn revert_commit_safely_reverts_standard_merge_commit() {
        let repo_dir = create_temp_repo_dir("commit-del-merge");
        let repo_root = repo_dir.to_string_lossy().to_string();

        run_git_checked(&repo_root, &["init"]).expect("init repo");
        run_git_checked(&repo_root, &["config", "user.name", "ComfyGit Tests"])
            .expect("configure user.name");
        run_git_checked(
            &repo_root,
            &["config", "user.email", "tests@comfygit.invalid"],
        )
        .expect("configure user.email");
        let switch_main = run_git(&repo_root, &["switch", "-c", "main"]).expect("switch main");
        if !switch_main.success {
            run_git_checked(&repo_root, &["checkout", "-b", "main"]).expect("checkout main");
        }

        let tracked_file = repo_dir.join("tracked.txt");
        fs::write(&tracked_file, "base\n").expect("write base file");
        run_git_checked(&repo_root, &["add", "tracked.txt"]).expect("stage base file");
        run_git_checked(&repo_root, &["commit", "-m", "base"]).expect("commit base file");

        let switch_feature = run_git(&repo_root, &["switch", "-c", "feature/remove-me"])
            .expect("switch feature branch");
        if !switch_feature.success {
            run_git_checked(&repo_root, &["checkout", "-b", "feature/remove-me"])
                .expect("checkout feature branch");
        }

        fs::write(&tracked_file, "base\nfeature\n").expect("write feature file");
        run_git_checked(&repo_root, &["add", "tracked.txt"]).expect("stage feature file");
        run_git_checked(&repo_root, &["commit", "-m", "feature change"])
            .expect("commit feature change");

        let switch_back = run_git(&repo_root, &["switch", "main"]).expect("switch back to main");
        if !switch_back.success {
            run_git_checked(&repo_root, &["checkout", "main"]).expect("checkout main");
        }
        run_git_checked(
            &repo_root,
            &[
                "merge",
                "--no-ff",
                "feature/remove-me",
                "-m",
                "Merge feature/remove-me",
            ],
        )
        .expect("merge feature branch");

        let merge_commit = run_git_checked(&repo_root, &["rev-parse", "HEAD"])
            .expect("read merge commit")
            .trim()
            .to_string();

        let outcome = revert_commit_safely(&repo_root, &merge_commit)
            .expect("safe revert of merge commit should succeed");

        assert_eq!(outcome.reverted_commit, merge_commit);
        assert!(outcome.reverted_subject.contains("Merge feature/remove-me"));
        assert!(!outcome.pushed);
        assert!(outcome.upstream_ref.is_none());
        assert_eq!(
            fs::read_to_string(&tracked_file)
                .expect("read reverted file")
                .replace("\r\n", "\n"),
            "base\n"
        );

        let head_subject =
            run_git_checked(&repo_root, &["show", "--no-patch", "--format=%s", "HEAD"])
                .expect("read revert subject");
        assert!(head_subject.contains("Revert \"Merge feature/remove-me\""));

        fs::remove_dir_all(&repo_dir).expect("remove temp repo dir");
    }

    #[test]
    fn load_branch_diagram_uses_deepest_open_descendant_when_current_is_main() {
        let repo_dir = create_temp_repo_dir("branch-diagram-main-focus");
        let repo_root = repo_dir.to_string_lossy().to_string();

        run_git_checked(&repo_root, &["init"]).expect("init repo");
        run_git_checked(&repo_root, &["config", "user.name", "ComfyGit Tests"])
            .expect("configure user.name");
        run_git_checked(
            &repo_root,
            &["config", "user.email", "tests@comfygit.invalid"],
        )
        .expect("configure user.email");

        let switch_main = run_git(&repo_root, &["switch", "-c", "main"]).expect("switch main");
        if !switch_main.success {
            run_git_checked(&repo_root, &["checkout", "-b", "main"]).expect("checkout main");
        }

        fs::write(repo_dir.join("tracked.txt"), "base\n").expect("write base file");
        run_git_checked(&repo_root, &["add", "tracked.txt"]).expect("stage base file");
        run_git_checked(&repo_root, &["commit", "-m", "base"]).expect("commit base file");

        let switch_v039 = run_git(&repo_root, &["switch", "-c", "v0.3.9"]).expect("switch v0.3.9");
        if !switch_v039.success {
            run_git_checked(&repo_root, &["checkout", "-b", "v0.3.9"]).expect("checkout v0.3.9");
        }
        fs::write(repo_dir.join("tracked.txt"), "base\nv0.3.9\n").expect("write v0.3.9 file");
        run_git_checked(&repo_root, &["add", "tracked.txt"]).expect("stage v0.3.9 file");
        run_git_checked(&repo_root, &["commit", "-m", "v0.3.9 change"])
            .expect("commit v0.3.9 change");

        let switch_v011x =
            run_git(&repo_root, &["switch", "-c", "v0.11.x"]).expect("switch v0.11.x");
        if !switch_v011x.success {
            run_git_checked(&repo_root, &["checkout", "-b", "v0.11.x"]).expect("checkout v0.11.x");
        }
        fs::write(repo_dir.join("tracked.txt"), "base\nv0.3.9\nv0.11.x\n")
            .expect("write v0.11.x file");
        run_git_checked(&repo_root, &["add", "tracked.txt"]).expect("stage v0.11.x file");
        run_git_checked(&repo_root, &["commit", "-m", "v0.11.x change"])
            .expect("commit v0.11.x change");

        let switch_main_back =
            run_git(&repo_root, &["switch", "main"]).expect("switch back to main");
        if !switch_main_back.success {
            run_git_checked(&repo_root, &["checkout", "main"]).expect("checkout main");
        }

        let diagram = load_branch_diagram(&repo_root, "main", None)
            .expect("load branch diagram")
            .expect("diagram should exist");

        let path_names = diagram
            .path
            .iter()
            .map(|segment| segment.branch.name.clone())
            .collect::<Vec<_>>();
        assert_eq!(
            path_names,
            vec!["v0.3.9".to_string(), "v0.11.x".to_string()]
        );
        assert!(
            diagram
                .path
                .iter()
                .all(|segment| segment.branch.state == BranchDiagramState::Open)
        );

        fs::remove_dir_all(&repo_dir).expect("remove temp repo dir");
    }

    #[test]
    fn resolve_parent_branch_name_uses_previous_level_in_branch_tree() {
        let repo_dir = create_temp_repo_dir("branch-parent-target");
        let repo_root = repo_dir.to_string_lossy().to_string();

        init_temp_git_repo(&repo_root);

        fs::write(repo_dir.join("tracked.txt"), "base\n").expect("write base file");
        run_git_checked(&repo_root, &["add", "tracked.txt"]).expect("stage base file");
        run_git_checked(&repo_root, &["commit", "-m", "base"]).expect("commit base file");

        let switch_dev =
            run_git(&repo_root, &["switch", "-c", "v0.12.x-dev"]).expect("switch dev branch");
        if !switch_dev.success {
            run_git_checked(&repo_root, &["checkout", "-b", "v0.12.x-dev"])
                .expect("checkout dev branch");
        }
        fs::write(repo_dir.join("tracked.txt"), "base\ndev\n").expect("write dev file");
        run_git_checked(&repo_root, &["add", "tracked.txt"]).expect("stage dev file");
        run_git_checked(&repo_root, &["commit", "-m", "dev"]).expect("commit dev file");

        let switch_patch =
            run_git(&repo_root, &["switch", "-c", "v0.12.2"]).expect("switch patch branch");
        if !switch_patch.success {
            run_git_checked(&repo_root, &["checkout", "-b", "v0.12.2"])
                .expect("checkout patch branch");
        }
        fs::write(repo_dir.join("tracked.txt"), "base\ndev\npatch\n").expect("write patch file");
        run_git_checked(&repo_root, &["add", "tracked.txt"]).expect("stage patch file");
        run_git_checked(&repo_root, &["commit", "-m", "patch"]).expect("commit patch file");

        assert_eq!(
            resolve_parent_branch_name(&repo_root, "v0.12.2", None).expect("resolve parent branch"),
            "v0.12.x-dev"
        );

        fs::remove_dir_all(&repo_dir).expect("remove temp repo dir");
    }

    #[test]
    fn resolve_main_branch_target_prefers_custom_main_branch() {
        let repo_dir = create_temp_repo_dir("branch-main-target");
        let repo_root = repo_dir.to_string_lossy().to_string();

        run_git_checked(&repo_root, &["init"]).expect("init repo");
        run_git_checked(&repo_root, &["config", "user.name", "ComfyGit Tests"])
            .expect("configure user.name");
        run_git_checked(
            &repo_root,
            &["config", "user.email", "tests@comfygit.invalid"],
        )
        .expect("configure user.email");
        let switch_trunk =
            run_git(&repo_root, &["switch", "-c", "trunk"]).expect("switch trunk branch");
        if !switch_trunk.success {
            run_git_checked(&repo_root, &["checkout", "-b", "trunk"])
                .expect("checkout trunk branch");
        }

        fs::write(repo_dir.join("tracked.txt"), "base\n").expect("write base file");
        run_git_checked(&repo_root, &["add", "tracked.txt"]).expect("stage base file");
        run_git_checked(&repo_root, &["commit", "-m", "base"]).expect("commit base file");

        assert_eq!(
            resolve_main_branch_target(&repo_root, Some("trunk"))
                .expect("resolve main branch target"),
            "trunk"
        );

        fs::remove_dir_all(&repo_dir).expect("remove temp repo dir");
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
