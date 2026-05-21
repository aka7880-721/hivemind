// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit License v1.2
//
// For details, see the LICENSE file in the repository root.

//! `cg init` — register a new project from the CLI without opening the TUI wizard.

use std::{
    env,
    ffi::OsStr,
    fs,
    io::{self, Write},
    path::Path,
    process::Command,
};

use anyhow::{Context, Result, bail};
use crossterm::{
    cursor::{MoveToColumn, MoveUp},
    event::{self, Event, KeyCode, KeyEventKind},
    execute, queue,
    style::{Color, Print, ResetColor, SetForegroundColor},
    terminal::{Clear, ClearType, disable_raw_mode, enable_raw_mode},
};

use crate::{
    app::{default_target_key_for_path, target_key_is_custom},
    cli::{best_effort_canonicalize, normalize_lookup, project_root},
    config::{AppConfig, ConfigStore, IntegrationMode, ProjectConfig, ProjectType, TargetFormat},
    git::github_owner_repo_from_remote_url,
    project_wizard::ProjectWizard,
    targets::{ProbeKind, TargetProbe, probe_target},
    versioning::VersionScheme,
};

const ANSI_CYAN: &str = "\x1b[36m";
const ANSI_YELLOW: &str = "\x1b[33m";
const ANSI_MAGENTA: &str = "\x1b[35m";
const ANSI_GREEN: &str = "\x1b[32m";
const ANSI_DARK_GREY: &str = "\x1b[90m";
const ANSI_RESET: &str = "\x1b[0m";

pub(crate) fn run_init() -> Result<()> {
    print_banner();

    let cwd =
        best_effort_canonicalize(&env::current_dir().context("failed to read current directory")?);
    if !cwd.is_dir() {
        bail!("current directory '{}' is not accessible", cwd.display());
    }

    let config_store = ConfigStore::locate()?;
    let mut config = config_store.load()?;
    ensure_not_already_registered(&config.projects, &cwd)?;

    let folder_name = folder_name_from_path(&cwd)?;
    let project_name = prompt_project_name(&folder_name)?;
    let project_alias = prompt_project_alias(&config.projects, &project_name)?;
    let setup_mode = prompt_setup_mode()?;

    if setup_mode == SetupMode::Manual {
        print_manual_setup_advice();
        return Ok(());
    }

    let detected = detect_project_layout(&cwd)?;
    let project = build_project_from_detection(&cwd, &project_name, detected)?;
    confirm_and_save_project(
        &config_store,
        &mut config,
        &project_name,
        &project_alias,
        project,
    )?;

    print_summary(&project_name, &project_alias, config_store.path());
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SetupMode {
    AutoDetect,
    Manual,
}

#[derive(Debug, Clone)]
struct DetectedManifest {
    relative_path: String,
    format: TargetFormat,
    default_key: String,
}

#[derive(Debug, Clone)]
struct ProjectDetection {
    integration_mode: IntegrationMode,
    remote_url: Option<String>,
    manifests: Vec<DetectedManifest>,
}

fn print_banner() {
    println!();
    println!("{ANSI_CYAN}ComfyGit project setup{ANSI_RESET} {ANSI_DARK_GREY}(cg init){ANSI_RESET}");
    println!();
}

fn ensure_not_already_registered(projects: &[ProjectConfig], cwd: &Path) -> Result<()> {
    let cwd = best_effort_canonicalize(cwd);
    for project in projects {
        let root = project_root(project).with_context(|| {
            format!("failed to resolve root path for project '{}'", project.name)
        })?;
        if best_effort_canonicalize(&root) == cwd {
            bail!(
                "this directory is already registered as project '{}'; open the TUI with `cg` and press E to edit it",
                project.name
            );
        }
    }
    Ok(())
}

fn prompt_project_alias(projects: &[ProjectConfig], project_name: &str) -> Result<String> {
    print_alias_intro(project_name);

    let suggested = suggest_alias_from_name(project_name);
    loop {
        let prompt = if suggested.is_empty() {
            "Project alias (optional, press Enter to skip)".to_string()
        } else {
            format!("Project alias (optional) [{ANSI_MAGENTA}{suggested}{ANSI_RESET}]")
        };
        print!("{prompt}: ");
        io::stdout()
            .flush()
            .context("failed to flush alias prompt")?;

        let mut answer = String::new();
        io::stdin()
            .read_line(&mut answer)
            .context("failed to read project alias")?;
        let trimmed = answer.trim();
        let alias = if trimmed.is_empty() {
            suggested.clone()
        } else {
            trimmed.to_string()
        };

        if alias.is_empty() {
            println!(
                "  {ANSI_DARK_GREY}No alias configured. You can add one later in the TUI (press E).{ANSI_RESET}"
            );
            return Ok(String::new());
        }

        if alias.contains('/') || alias.contains('\\') || alias.contains(' ') {
            println!("Alias cannot contain spaces or path separators.");
            continue;
        }

        if projects.iter().any(|project| {
            !project.alias.trim().is_empty()
                && normalize_lookup(&project.alias) == normalize_lookup(&alias)
        }) {
            println!("Alias '{}' is already used by another project.", alias);
            continue;
        }

        println!(
            "  Alias set to {ANSI_MAGENTA}{alias}{ANSI_RESET} — use {ANSI_MAGENTA}cg cd {alias}{ANSI_RESET} from anywhere."
        );
        return Ok(alias);
    }
}

fn print_alias_intro(project_name: &str) {
    let example_alias = suggest_alias_from_name(project_name);
    println!();
    println!("{ANSI_CYAN}Project alias{ANSI_RESET}");
    println!();
    println!(
        "{ANSI_YELLOW}With a configured alias you can cd into the repo directory from anywhere.{ANSI_RESET}"
    );
    println!("Your alias should be short and easy to remember.");
    println!();
    if example_alias.is_empty() {
        println!(
            "For example, if the project name is {ANSI_MAGENTA}My First Project{ANSI_RESET}, set the alias to {ANSI_MAGENTA}mfp{ANSI_RESET}."
        );
        println!("Then use {ANSI_MAGENTA}cg cd mfp{ANSI_RESET} to enter that directory.");
    } else {
        println!(
            "For example, with project name {ANSI_MAGENTA}{project_name}{ANSI_RESET}, you could set the alias to {ANSI_MAGENTA}{example_alias}{ANSI_RESET}."
        );
        println!(
            "Then use {ANSI_MAGENTA}cg cd {example_alias}{ANSI_RESET} to enter that directory."
        );
    }
    println!();
}

fn suggest_alias_from_name(project_name: &str) -> String {
    project_name
        .split(|character: char| !character.is_alphanumeric())
        .filter(|segment| !segment.is_empty())
        .filter_map(|segment| segment.chars().next())
        .collect::<String>()
        .to_ascii_lowercase()
}

fn folder_name_from_path(path: &Path) -> Result<String> {
    path.file_name()
        .and_then(OsStr::to_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            anyhow::anyhow!("could not determine a project name from {}", path.display())
        })
}

fn prompt_project_name(folder_name: &str) -> Result<String> {
    println!(
        "{ANSI_CYAN}Project name{ANSI_RESET} {ANSI_DARK_GREY}(from current folder){ANSI_RESET}"
    );
    println!();

    let options = [
        format!("Use folder name: {ANSI_MAGENTA}{folder_name}{ANSI_RESET}"),
        "Set a CUSTOM project name".to_string(),
    ];
    let labels: Vec<&str> = options.iter().map(String::as_str).collect();

    match prompt_option_picker("Choose how to name this project:", &labels)? {
        0 => Ok(folder_name.to_string()),
        1 => prompt_custom_project_name(folder_name),
        _ => unreachable!(),
    }
}

fn prompt_custom_project_name(folder_name: &str) -> Result<String> {
    loop {
        print!("Custom project name [{folder_name}]: ");
        io::stdout().flush().context("failed to flush prompt")?;
        let mut answer = String::new();
        io::stdin()
            .read_line(&mut answer)
            .context("failed to read custom project name")?;
        let trimmed = answer.trim();
        if trimmed.is_empty() {
            return Ok(folder_name.to_string());
        }
        if trimmed.contains('/') || trimmed.contains('\\') {
            println!("Project name cannot contain path separators.");
            continue;
        }
        return Ok(trimmed.to_string());
    }
}

fn prompt_setup_mode() -> Result<SetupMode> {
    println!();
    println!("{ANSI_CYAN}Setup mode{ANSI_RESET}");
    println!();

    let options = [
        "Auto-Detect — scan this folder and build the project config",
        "Add details manually in the TUI (press N after running `cg`)",
    ];

    match prompt_option_picker(
        "Should ComfyGit auto-detect the project configuration?",
        &options,
    )? {
        0 => Ok(SetupMode::AutoDetect),
        1 => Ok(SetupMode::Manual),
        _ => unreachable!(),
    }
}

fn print_manual_setup_advice() {
    println!();
    println!("{ANSI_YELLOW}Manual setup is easier in the TUI.{ANSI_RESET}");
    println!();
    println!("  1. Run {ANSI_MAGENTA}cg{ANSI_RESET}");
    println!("  2. Press {ANSI_MAGENTA}N{ANSI_RESET} to open the project wizard");
    println!(
        "  3. Use {ANSI_MAGENTA}Browse{ANSI_RESET} and the full form to configure targets, scopes, and git settings"
    );
    println!();
    println!(
        "{ANSI_DARK_GREY}No project was saved. Run `cg init` again when you want auto-detection.{ANSI_RESET}"
    );
    println!();
}

fn detect_project_layout(cwd: &Path) -> Result<ProjectDetection> {
    println!();
    println!("{ANSI_CYAN}Running pre-flight checks...{ANSI_RESET}");
    println!();

    let integration = detect_integration_mode(cwd)?;
    println!(
        "  Integration: {ANSI_YELLOW}{}{ANSI_RESET}",
        integration.integration_mode.display_name()
    );
    if let Some(remote) = integration.remote_url.as_deref() {
        println!("  Remote URL:  {ANSI_MAGENTA}{remote}{ANSI_RESET}");
    } else if integration.integration_mode.requires_repo() {
        println!("  Remote URL:  {ANSI_DARK_GREY}(none configured){ANSI_RESET}");
    }

    let manifests = detect_manifests(cwd)?;
    if manifests.is_empty() {
        bail!(
            "no Cargo.toml or package.json was found in {}; add a version manifest first or use manual setup in the TUI",
            cwd.display()
        );
    }

    println!();
    println!("{ANSI_CYAN}Detected version manifests:{ANSI_RESET}");
    for manifest in &manifests {
        println!(
            "  - {ANSI_MAGENTA}{}{ANSI_RESET} ({})",
            manifest.relative_path,
            manifest.format.display_name()
        );
    }
    println!();

    Ok(ProjectDetection {
        integration_mode: integration.integration_mode,
        remote_url: integration.remote_url,
        manifests,
    })
}

#[derive(Debug, Clone)]
struct IntegrationDetection {
    integration_mode: IntegrationMode,
    remote_url: Option<String>,
}

fn detect_integration_mode(cwd: &Path) -> Result<IntegrationDetection> {
    if !is_git_repository(cwd) {
        println!(
            "  {ANSI_DARK_GREY}Git repository: not detected — using Local-only mode{ANSI_RESET}"
        );
        return Ok(IntegrationDetection {
            integration_mode: IntegrationMode::LocalOnly,
            remote_url: None,
        });
    }

    println!("  {ANSI_GREEN}Git repository: detected{ANSI_RESET}");

    let remote_url = read_git_remote_url(cwd);
    if let Some(remote_url) = remote_url {
        if github_owner_repo_from_remote_url(&remote_url).is_some() {
            return Ok(IntegrationDetection {
                integration_mode: IntegrationMode::GitHubEnabled,
                remote_url: Some(remote_url),
            });
        }

        return Ok(IntegrationDetection {
            integration_mode: IntegrationMode::GitLocalOnly,
            remote_url: Some(remote_url),
        });
    }

    let mode = prompt_git_backed_mode()?;
    Ok(IntegrationDetection {
        integration_mode: mode,
        remote_url: None,
    })
}

fn prompt_git_backed_mode() -> Result<IntegrationMode> {
    println!();
    println!("{ANSI_YELLOW}No git remote was found in `git remote -v`.{ANSI_RESET}");
    println!();

    let options = [
        "GitLocal-only — local git repo without GitHub integration",
        "Local-only — filesystem project without git requirements",
    ];

    match prompt_option_picker("How should ComfyGit treat this project?", &options)? {
        0 => Ok(IntegrationMode::GitLocalOnly),
        1 => Ok(IntegrationMode::LocalOnly),
        _ => unreachable!(),
    }
}

fn is_git_repository(cwd: &Path) -> bool {
    Command::new("git")
        .current_dir(cwd)
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn read_git_remote_url(cwd: &Path) -> Option<String> {
    let output = Command::new("git")
        .current_dir(cwd)
        .args(["remote", "-v"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let mut origin_fetch = None;
    let mut first_fetch = None;

    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let remote = parts.next()?;
        let url = parts.next()?;
        let mode = parts.next()?;
        if mode != "(fetch)" {
            continue;
        }
        if first_fetch.is_none() {
            first_fetch = Some(url.to_string());
        }
        if remote == "origin" && origin_fetch.is_none() {
            origin_fetch = Some(url.to_string());
        }
    }

    origin_fetch.or(first_fetch)
}

fn detect_manifests(cwd: &Path) -> Result<Vec<DetectedManifest>> {
    let mut manifests = Vec::new();
    for file_name in ["Cargo.toml", "package.json"] {
        let path = cwd.join(file_name);
        if path.is_file() {
            manifests.push(DetectedManifest {
                relative_path: file_name.to_string(),
                format: if file_name.ends_with(".toml") {
                    TargetFormat::Toml
                } else {
                    TargetFormat::Json
                },
                default_key: default_target_key_for_path(file_name).to_string(),
            });
        }
    }
    Ok(manifests)
}

fn build_project_from_detection(
    cwd: &Path,
    project_name: &str,
    detection: ProjectDetection,
) -> Result<ProjectConfig> {
    let selected_manifest = prompt_manifest_selection(&detection.manifests)?;
    let target_path = prompt_target_path_confirmation(cwd, &selected_manifest)?;
    let target_key = prompt_target_key_confirmation(&target_path, &selected_manifest.default_key)?;

    let probe = probe_target(&target_path, &target_key, VersionScheme::SemVer)
        .with_context(|| format!("failed to read version from {}", target_path))?;
    print_probe_result(&probe);

    if !matches!(probe.kind, ProbeKind::Success | ProbeKind::Warning) {
        bail!("target validation failed for {}", target_path);
    }

    if !prompt_confirm_default_yes("Use the detected target settings above?", true)? {
        bail!("Cancelled by user");
    }

    let repo_root = cwd.display().to_string();
    let remote_url = if detection.integration_mode.requires_remote() {
        let default = detection.remote_url.unwrap_or_default();
        Some(prompt_remote_url(&default)?)
    } else {
        detection.remote_url
    };

    let mut wizard = ProjectWizard::default();
    wizard.name.set_value(project_name);
    wizard.project_type = ProjectType::AllInOne;
    wizard.integration_mode = detection.integration_mode;
    wizard.version_scheme = VersionScheme::SemVer;
    wizard.target_path.set_value(&target_path);
    wizard.target_key.set_value(&target_key);
    wizard.target_key_custom = target_key_is_custom(&target_path, &target_key);
    wizard.repo_root.set_value(&repo_root);
    if let Some(remote_url) = remote_url.as_deref() {
        wizard.remote_url.set_value(remote_url);
    }
    wizard.last_probe = Some(probe);

    wizard.build_project()
}

fn prompt_manifest_selection(manifests: &[DetectedManifest]) -> Result<DetectedManifest> {
    if manifests.len() == 1 {
        return Ok(manifests[0].clone());
    }

    let options = manifests
        .iter()
        .map(|manifest| {
            format!(
                "{} ({}, key: {})",
                manifest.relative_path,
                manifest.format.display_name(),
                manifest.default_key
            )
        })
        .collect::<Vec<_>>();
    let labels: Vec<&str> = options.iter().map(String::as_str).collect();
    let picked = prompt_option_picker(
        "Multiple version manifests were found. Which one should ComfyGit use?",
        &labels,
    )?;
    Ok(manifests[picked].clone())
}

fn prompt_target_path_confirmation(cwd: &Path, manifest: &DetectedManifest) -> Result<String> {
    let absolute = cwd.join(&manifest.relative_path);
    let absolute = absolute.display().to_string();
    let default_relative = manifest.relative_path.clone();

    loop {
        print!("Version manifest path [{ANSI_MAGENTA}{default_relative}{ANSI_RESET}]: ");
        io::stdout().flush().context("failed to flush prompt")?;
        let mut answer = String::new();
        io::stdin()
            .read_line(&mut answer)
            .context("failed to read target path")?;
        let trimmed = answer.trim();
        let path = if trimmed.is_empty() {
            absolute.clone()
        } else {
            let candidate = Path::new(trimmed);
            if candidate.is_absolute() {
                trimmed.to_string()
            } else {
                cwd.join(trimmed).display().to_string()
            }
        };

        if Path::new(&path).is_file() {
            return Ok(path);
        }
        println!("File does not exist: {path}");
    }
}

fn prompt_target_key_confirmation(_target_path: &str, default_key: &str) -> Result<String> {
    loop {
        print!("Version key [{ANSI_MAGENTA}{default_key}{ANSI_RESET}]: ");
        io::stdout().flush().context("failed to flush prompt")?;
        let mut answer = String::new();
        io::stdin()
            .read_line(&mut answer)
            .context("failed to read target key")?;
        let trimmed = answer.trim();
        if trimmed.is_empty() {
            return Ok(default_key.to_string());
        }
        if !trimmed.contains(' ') {
            return Ok(trimmed.to_string());
        }
        println!("Version key cannot contain spaces.");
    }
}

fn prompt_remote_url(default: &str) -> Result<String> {
    loop {
        print!("Git remote URL [{ANSI_MAGENTA}{default}{ANSI_RESET}]: ");
        io::stdout().flush().context("failed to flush prompt")?;
        let mut answer = String::new();
        io::stdin()
            .read_line(&mut answer)
            .context("failed to read remote URL")?;
        let trimmed = answer.trim();
        let value = if trimmed.is_empty() {
            default.to_string()
        } else {
            trimmed.to_string()
        };
        if value.is_empty() {
            println!("Remote URL is required for GitHub-enabled projects.");
            continue;
        }
        return Ok(value);
    }
}

fn print_probe_result(probe: &TargetProbe) {
    let (label, color) = match probe.kind {
        ProbeKind::Success => ("OK", ANSI_GREEN),
        ProbeKind::Warning => ("Warning", ANSI_YELLOW),
        ProbeKind::Error => ("Error", ANSI_YELLOW),
    };
    println!(
        "  Target read [{color}{label}{ANSI_RESET}]: {}",
        probe.message
    );
    if let Some(version) = probe.version.as_deref() {
        println!("  Detected version: {ANSI_MAGENTA}{version}{ANSI_RESET}");
    }
}

fn confirm_and_save_project(
    config_store: &ConfigStore,
    config: &mut AppConfig,
    project_name: &str,
    project_alias: &str,
    mut project: ProjectConfig,
) -> Result<()> {
    project.name = project_name.to_string();
    project.alias = project_alias.trim().to_string();

    if !prompt_confirm_default_yes("Save this project to your ComfyGit configuration?", true)? {
        bail!("Cancelled by user");
    }

    if let Some(repo) = project.repo.as_ref() {
        ensure_gitignore_entry(&repo.local_root, ".comfygit/syncmem/stdchlg-local.json")?;
    }

    config.projects.push(project);
    config_store.save(config)?;

    println!();
    println!("{ANSI_GREEN}Project '{project_name}' saved successfully.{ANSI_RESET}");
    Ok(())
}

fn print_summary(project_name: &str, project_alias: &str, config_path: &Path) {
    println!();
    println!("{ANSI_CYAN}Summary{ANSI_RESET}");
    println!();
    println!("  Project: {ANSI_MAGENTA}{project_name}{ANSI_RESET}");
    if project_alias.trim().is_empty() {
        println!("  Alias:   {ANSI_DARK_GREY}(not set){ANSI_RESET}");
    } else {
        println!(
            "  Alias:   {ANSI_MAGENTA}{}{ANSI_RESET}",
            project_alias.trim()
        );
        println!(
            "  Quick:   {ANSI_MAGENTA}cg cd {}{ANSI_RESET}",
            project_alias.trim()
        );
    }
    println!(
        "  Config:  {ANSI_DARK_GREY}{}{ANSI_RESET}",
        config_path.display()
    );
    println!();
    println!(
        "{ANSI_YELLOW}To change any setting later:{ANSI_RESET} run {ANSI_MAGENTA}cg{ANSI_RESET}, select this project, and press {ANSI_MAGENTA}E{ANSI_RESET} to edit it."
    );
    println!();
    println!(
        "{ANSI_YELLOW}For changelog generation, release scripts, and other advanced options:{ANSI_RESET}"
    );
    println!(
        "  open the TUI → select this project → visit the {ANSI_MAGENTA}Project Settings{ANSI_RESET} tab"
    );
    println!();
}

fn prompt_option_picker(title: &str, options: &[&str]) -> Result<usize> {
    if options.is_empty() {
        bail!("no options are available");
    }

    let mut selected = 0usize;
    let mut rendered_lines = 0usize;
    let raw_mode = RawModeGuard::enter()?;

    loop {
        render_option_picker(title, options, selected, &mut rendered_lines)?;

        let Event::Key(key) = event::read().context("failed to read key event")? else {
            continue;
        };
        if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
            continue;
        }

        match key.code {
            KeyCode::Esc => {
                drop(raw_mode);
                println!();
                bail!("Cancelled by user");
            }
            KeyCode::Up | KeyCode::BackTab => {
                selected = selected.checked_sub(1).unwrap_or(options.len() - 1);
            }
            KeyCode::Down | KeyCode::Tab => {
                selected = (selected + 1) % options.len();
            }
            KeyCode::Char(c) if c.is_ascii_digit() => {
                if let Some(index) = c.to_digit(10).and_then(|d| d.checked_sub(1)) {
                    let index = index as usize;
                    if index < options.len() {
                        selected = index;
                    }
                }
            }
            KeyCode::Enter => {
                drop(raw_mode);
                println!();
                return Ok(selected);
            }
            _ => {}
        }
    }
}

fn render_option_picker(
    title: &str,
    options: &[&str],
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
        .context("failed to redraw option picker")?;
    }

    queue!(
        stdout,
        MoveToColumn(0),
        Print("\r\n"),
        SetForegroundColor(Color::DarkGrey),
        Print("Use Up/Down or Tab to select, then press Enter.\r\n"),
        ResetColor,
        Print("\r\n"),
        SetForegroundColor(Color::Cyan),
        Print(format!("{title}\r\n")),
        ResetColor,
        Print("\r\n")
    )
    .context("failed to render option picker title")?;

    for (index, label) in options.iter().enumerate() {
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
            Print(format!("{} {}. {}\r\n", marker, index + 1, label)),
            ResetColor
        )
        .context("failed to render option picker row")?;
    }

    queue!(stdout, MoveToColumn(0), Print("\r\n"))?;
    stdout.flush().context("failed to flush option picker")?;
    *rendered_lines = 5 + options.len();
    Ok(())
}

fn prompt_confirm_default_yes(prompt: &str, default_yes: bool) -> Result<bool> {
    let suffix = if default_yes { "Y/n" } else { "y/N" };
    loop {
        print!("{prompt} [{ANSI_YELLOW}{suffix}{ANSI_RESET}]: ");
        io::stdout().flush().context("failed to flush prompt")?;

        let mut answer = String::new();
        io::stdin()
            .read_line(&mut answer)
            .context("failed to read response")?;

        match answer.trim().to_lowercase().as_str() {
            "" if default_yes => return Ok(true),
            "" if !default_yes => return Ok(false),
            "y" | "yes" => return Ok(true),
            "n" | "no" => return Ok(false),
            _ => println!("Please answer Y or N."),
        }
    }
}

fn ensure_gitignore_entry(repo_root: &str, entry: &str) -> Result<()> {
    let gitignore_path = Path::new(repo_root).join(".gitignore");
    let mut lines = if gitignore_path.exists() {
        fs::read_to_string(&gitignore_path)
            .with_context(|| format!("failed to read .gitignore in '{}'", repo_root))?
            .lines()
            .map(str::to_string)
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    let normalized_entry = entry.trim();
    if lines.iter().any(|line| line.trim() == normalized_entry) {
        return Ok(());
    }

    if !lines.is_empty() && !lines.last().unwrap().is_empty() {
        lines.push(String::new());
    }
    lines.push(normalized_entry.to_string());

    fs::write(&gitignore_path, lines.join("\n") + "\n")
        .with_context(|| format!("failed to update .gitignore in '{}'", repo_root))?;

    Ok(())
}

struct RawModeGuard;

impl RawModeGuard {
    fn enter() -> Result<Self> {
        enable_raw_mode().context("failed to enable raw terminal mode")?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn suggest_alias_from_name_uses_first_letters_of_words() {
        assert_eq!(suggest_alias_from_name("My First Project"), "mfp");
        assert_eq!(suggest_alias_from_name("test-project"), "tp");
    }

    #[test]
    fn ensure_not_already_registered_allows_subdirectory_of_existing_project() {
        let parent =
            std::env::temp_dir().join(format!("comfygit-init-parent-{}", std::process::id()));
        let child = parent.join("nested-app");
        let _ = fs::remove_dir_all(&parent);
        fs::create_dir_all(&child).expect("create nested dir");

        let projects = vec![ProjectConfig {
            name: "Parent".to_string(),
            repo: Some(crate::config::RepoConfig {
                local_root: parent.display().to_string(),
                ..Default::default()
            }),
            targets: vec![crate::config::TargetSpec {
                label: "Version".to_string(),
                path: parent.join("Cargo.toml").display().to_string(),
                key_path: "package.version".to_string(),
                format: TargetFormat::Toml,
            }],
            ..Default::default()
        }];
        fs::write(
            parent.join("Cargo.toml"),
            "[package]\nversion = \"0.1.0\"\n",
        )
        .expect("write manifest");

        ensure_not_already_registered(&projects, &child).expect("child should be allowed");

        ensure_not_already_registered(&projects, &parent)
            .expect_err("exact parent root should be blocked");

        let _ = fs::remove_dir_all(&parent);
    }

    #[test]
    fn folder_name_from_path_uses_final_segment() {
        let path = PathBuf::from("/tmp/my-cool-app");
        assert_eq!(
            folder_name_from_path(&path).expect("folder name"),
            "my-cool-app"
        );
    }

    #[test]
    fn read_git_remote_url_prefers_origin_fetch_entry() {
        let dir = std::env::temp_dir().join(format!("comfygit-init-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        Command::new("git")
            .current_dir(&dir)
            .args(["init"])
            .status()
            .expect("git init");
        Command::new("git")
            .current_dir(&dir)
            .args([
                "remote",
                "add",
                "origin",
                "git@github.com:comfy-home/ComfyGit.git",
            ])
            .status()
            .expect("git remote add");

        let remote = read_git_remote_url(&dir).expect("remote url");
        assert_eq!(remote, "git@github.com:comfy-home/ComfyGit.git");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn detect_manifests_finds_cargo_and_package_json() {
        let dir =
            std::env::temp_dir().join(format!("comfygit-init-manifests-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        fs::write(dir.join("Cargo.toml"), "[package]\nversion = \"0.1.0\"\n").expect("write cargo");
        fs::write(dir.join("package.json"), "{\"version\":\"0.1.0\"}").expect("write package");

        let manifests = detect_manifests(&dir).expect("detect manifests");
        assert_eq!(manifests.len(), 2);
        assert!(manifests.iter().any(|m| m.relative_path == "Cargo.toml"));
        assert!(manifests.iter().any(|m| m.relative_path == "package.json"));

        let _ = fs::remove_dir_all(&dir);
    }
}
