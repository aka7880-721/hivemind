// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit License v1.2
//
// For details, see the LICENSE file in the repository root.

//! Alternative (`alt`) development branches — `cg new alt`.
//!
//! Numeric alts (`v0.1.5-dev-alt1`) are created from a dev branch.
//! Letter alts (`v0.1.5-dev-alt2A`) are created from a numeric alt branch.

use std::{
    env,
    io::{self, Write},
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
    cli::{best_effort_canonicalize, current_git_repo_root, find_project_for_cwd},
    config::{ConfigStore, IntegrationMode},
    git::{
        collect_all_branch_git_scope_contexts, create_branch_and_switch,
        current_branch_with_cancel, publish_branch_with_upstream, run_git_checked_with_cancel,
    },
    git_br::{
        BranchNameOption, custom_branch_name_option_with_preview,
        fixed_branch_name_option_with_value, specific_suffix_branch_name_option,
    },
};

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

pub(crate) fn run_new_alt(option_name: Option<&str>) -> Result<()> {
    let config = ConfigStore::locate()?.load()?;
    let cwd =
        best_effort_canonicalize(&env::current_dir().context("failed to read current directory")?);
    let project = find_project_for_cwd(&config.projects, &cwd)?;
    let repo_root = current_git_repo_root(&cwd)?;
    let current_branch = current_branch_with_cancel(&repo_root, None)?;

    validate_alt_creation_source(&current_branch)?;

    let synced_work = match option_name.map(str::trim) {
        None => prompt_alt_work_type_selection(&current_branch)?,
        Some("1") => true,
        Some("2") => false,
        Some(other) => {
            bail!(
                "cg new alt option must be 1 (Synced Work) or 2 (Local Work); got '{}'",
                other
            )
        }
    };

    if synced_work && project.integration_mode != IntegrationMode::GitHubEnabled {
        bail!(
            "cg new alt 1 (Synced Work) is only available for GitHub-enabled projects; \
             use option 2 for local-only branches"
        );
    }

    if !prompt_alt_position_continue(&project.name, &current_branch)? {
        bail!("Cancelled by user");
    }

    let existing_branches = list_local_branch_names(&repo_root)?;
    let branch_options = suggest_alt_branch_name_options(&current_branch, &existing_branches)?;
    let branch_name = prompt_alt_branch_name(&branch_options)?;

    create_branch_and_switch(&repo_root, &branch_name)?;

    if synced_work {
        let remote_spec = resolve_remote_spec_for_repo(project, &cwd, &repo_root)?;
        publish_branch_with_upstream(&repo_root, &branch_name, Some(&remote_spec), None)?;
        println!(
            "Created, switched to, and published branch '{}' to remote.",
            branch_name
        );
    } else {
        println!("Created and switched to branch '{}'.", branch_name);
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Merge parent resolution (used by cg br end / cg pr)
// ---------------------------------------------------------------------------

/// When `current_branch` is an alt branch, returns the branch it should merge into.
///
/// When `existing_branches` is provided, prefers the first candidate that already exists
/// locally (e.g. falls back from `v0.9.1-dev--or-this-way` to `v0.9.1-dev` when only the
/// latter exists). When empty, returns the preferred (first) candidate.
pub(crate) fn alt_merge_parent_branch(
    current_branch: &str,
    existing_branches: &[String],
) -> Option<String> {
    let candidates = alt_merge_parent_candidates(current_branch)?;
    pick_existing_branch_candidate(&candidates, existing_branches)
}

/// Dev branch that this alt branch is an alternative for (`v0.1.5-dev-alt2` → `v0.1.5-dev`).
pub(crate) fn alt_lineage_dev_base(branch: &str) -> Option<String> {
    let (base, _) = split_specific_suffix(branch);
    if let Some(dev_parent) = parse_numeric_alt_dev_parent(&base) {
        return Some(dev_parent);
    }

    let (numeric_base, _) = parse_letter_alt_base(&base)?;
    parse_numeric_alt_dev_parent(&numeric_base)
}

pub(crate) fn is_alt_branch(branch: &str) -> bool {
    alt_lineage_dev_base(branch).is_some()
}

/// Other local alt branches exploring alternatives on the same dev branch.
pub(crate) fn alt_sibling_branch_names(
    current_branch: &str,
    existing_branches: &[String],
) -> Vec<String> {
    let Some(dev_base) = alt_lineage_dev_base(current_branch) else {
        return Vec::new();
    };

    let mut siblings = existing_branches
        .iter()
        .filter(|branch| {
            !branch.eq_ignore_ascii_case(current_branch)
                && is_alt_branch(branch)
                && alt_lineage_dev_base(branch).as_deref() == Some(dev_base.as_str())
        })
        .cloned()
        .collect::<Vec<_>>();
    siblings.sort_by_cached_key(|branch| branch.to_ascii_lowercase());
    siblings
}

fn alt_merge_parent_candidates(current_branch: &str) -> Option<Vec<String>> {
    let (base, specific_suffix) = split_specific_suffix(current_branch);
    let mut candidates = Vec::new();

    if let Some((numeric_base, _letter)) = parse_letter_alt_base(&base) {
        candidates.push(join_with_specific_suffix(
            &numeric_base,
            specific_suffix.as_deref(),
        ));
        candidates.push(numeric_base.clone());
        if let Some(dev_parent) = parse_numeric_alt_dev_parent(&numeric_base) {
            candidates.push(join_with_specific_suffix(
                &dev_parent,
                specific_suffix.as_deref(),
            ));
            candidates.push(dev_parent);
        }
    } else if let Some(dev_parent) = parse_numeric_alt_dev_parent(&base) {
        candidates.push(join_with_specific_suffix(
            &dev_parent,
            specific_suffix.as_deref(),
        ));
        candidates.push(dev_parent);
    } else {
        return None;
    }

    candidates.dedup();
    Some(candidates)
}

fn pick_existing_branch_candidate(
    candidates: &[String],
    existing_branches: &[String],
) -> Option<String> {
    if existing_branches.is_empty() {
        return candidates.first().cloned();
    }

    candidates
        .iter()
        .find(|candidate| branch_exists(existing_branches, candidate))
        .cloned()
        .or_else(|| candidates.last().cloned())
}

fn branch_exists(existing_branches: &[String], candidate: &str) -> bool {
    existing_branches
        .iter()
        .any(|branch| branch.eq_ignore_ascii_case(candidate))
}

// ---------------------------------------------------------------------------
// Branch naming
// ---------------------------------------------------------------------------

fn validate_alt_creation_source(current_branch: &str) -> Result<()> {
    match classify_alt_source(current_branch)? {
        AltSourceKind::DevBranch | AltSourceKind::NumericAlt | AltSourceKind::LetterAlt => Ok(()),
    }
}

fn suggest_alt_branch_name_options(
    current_branch: &str,
    existing_branches: &[String],
) -> Result<Vec<BranchNameOption>> {
    let (base, specific_suffix) = split_specific_suffix(current_branch);
    let next_base = match classify_alt_source(current_branch)? {
        AltSourceKind::DevBranch => {
            let dev_base = base;
            let next_number = next_numeric_alt_number(&dev_base, existing_branches);
            format!("{}-alt{}", dev_base, next_number)
        }
        AltSourceKind::NumericAlt => {
            let numeric_base = base;
            let next_letter = next_letter_alt_suffix(&numeric_base, existing_branches)?;
            format!("{}{}", numeric_base, next_letter)
        }
        AltSourceKind::LetterAlt => {
            let (numeric_base, _) = parse_letter_alt_base(&base)
                .ok_or_else(|| anyhow::anyhow!("invalid letter alt branch '{}'", current_branch))?;
            let next_letter = next_letter_alt_suffix(&numeric_base, existing_branches)?;
            format!("{}{}", numeric_base, next_letter)
        }
    };

    let preview = join_with_specific_suffix(&next_base, specific_suffix.as_deref());
    Ok(vec![
        fixed_branch_name_option_with_value(preview.clone(), preview.clone()),
        specific_suffix_branch_name_option(next_base),
        custom_branch_name_option_with_preview("custom (not recommended)"),
    ])
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AltSourceKind {
    DevBranch,
    NumericAlt,
    LetterAlt,
}

fn classify_alt_source(current_branch: &str) -> Result<AltSourceKind> {
    let (base, _) = split_specific_suffix(current_branch);

    if parse_letter_alt_base(&base).is_some() {
        return Ok(AltSourceKind::LetterAlt);
    }

    if parse_numeric_alt_dev_parent(&base).is_some() {
        return Ok(AltSourceKind::NumericAlt);
    }

    if is_dev_branch_without_alt(&base) {
        return Ok(AltSourceKind::DevBranch);
    }

    bail!(
        "cg new alt can only be run from a dev branch or an existing alt branch; \
         current branch is '{}'",
        current_branch
    )
}

fn is_dev_branch_without_alt(base: &str) -> bool {
    base.contains("-dev") && !base.contains("-alt")
}

fn parse_numeric_alt_dev_parent(base: &str) -> Option<String> {
    let alt_index = base.rfind("-alt")?;
    let dev_parent = base[..alt_index].trim();
    if !is_dev_branch_without_alt(dev_parent) {
        return None;
    }

    let suffix = &base[alt_index + 4..];
    if suffix.is_empty() || !suffix.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }

    Some(dev_parent.to_string())
}

fn parse_letter_alt_base(base: &str) -> Option<(String, char)> {
    let alt_index = base.rfind("-alt")?;
    let dev_parent = base[..alt_index].trim();
    if !dev_parent.contains("-dev") {
        return None;
    }

    let suffix = &base[alt_index + 4..];
    let digit_len = suffix.chars().take_while(|ch| ch.is_ascii_digit()).count();
    if digit_len == 0 {
        return None;
    }

    let letters = &suffix[digit_len..];
    if letters.is_empty() || !letters.chars().all(|ch| ch.is_ascii_uppercase()) {
        return None;
    }

    let numeric_base = base[..alt_index + 4 + digit_len].to_string();
    let letter = letters.chars().last()?;
    Some((numeric_base, letter))
}

fn next_numeric_alt_number(dev_base: &str, existing_branches: &[String]) -> u32 {
    let max_number = existing_branches
        .iter()
        .filter_map(|branch| numeric_alt_number_for_dev(branch, dev_base))
        .max()
        .unwrap_or(0);
    max_number + 1
}

fn numeric_alt_number_for_dev(branch: &str, dev_base: &str) -> Option<u32> {
    let (base, _) = split_specific_suffix(branch);
    let suffix = base.strip_prefix(&format!("{dev_base}-alt"))?;
    if suffix.is_empty() {
        return None;
    }

    let digit_len = suffix.chars().take_while(|ch| ch.is_ascii_digit()).count();
    if digit_len == 0 {
        return None;
    }

    let rest = &suffix[digit_len..];
    if !rest.is_empty() {
        return None;
    }

    suffix[..digit_len].parse().ok()
}

fn next_letter_alt_suffix(numeric_base: &str, existing_branches: &[String]) -> Result<char> {
    let max_letter = existing_branches
        .iter()
        .filter_map(|branch| letter_alt_suffix_for_numeric(branch, numeric_base))
        .max();

    match max_letter {
        None => Ok('A'),
        Some(letter) if letter < 'Z' => Ok(((letter as u8) + 1) as char),
        Some('Z') => bail!(
            "all letter alt branches A-Z already exist for '{}'",
            numeric_base
        ),
        Some(_) => bail!("invalid letter alt suffix state for '{}'", numeric_base),
    }
}

fn letter_alt_suffix_for_numeric(branch: &str, numeric_base: &str) -> Option<char> {
    let (base, _) = split_specific_suffix(branch);
    if !base.starts_with(numeric_base) {
        return None;
    }

    let letters = base.strip_prefix(numeric_base)?;
    if letters.is_empty() || !letters.chars().all(|ch| ch.is_ascii_uppercase()) {
        return None;
    }

    letters.chars().last()
}

fn split_specific_suffix(branch: &str) -> (String, Option<String>) {
    if let Some((base, suffix)) = branch.split_once("--") {
        (base.to_string(), Some(suffix.to_string()))
    } else {
        (branch.to_string(), None)
    }
}

fn join_with_specific_suffix(base: &str, specific_suffix: Option<&str>) -> String {
    match specific_suffix.filter(|suffix| !suffix.is_empty()) {
        Some(suffix) => format!("{base}--{suffix}"),
        None => base.to_string(),
    }
}

fn list_local_branch_names(repo_root: &str) -> Result<Vec<String>> {
    let output = run_git_checked_with_cancel(
        repo_root,
        &["for-each-ref", "--format=%(refname:short)", "refs/heads"],
        None,
    )?;
    let mut branches = output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    branches.sort_by_cached_key(|branch| branch.to_ascii_lowercase());
    branches.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    Ok(branches)
}

fn resolve_remote_spec_for_repo(
    project: &crate::config::ProjectConfig,
    cwd: &std::path::Path,
    repo_root: &str,
) -> Result<String> {
    let contexts = collect_all_branch_git_scope_contexts(project)?;
    let context = contexts
        .iter()
        .find(|context| {
            cwd.starts_with(&context.repo_root) || context.repo_root.starts_with(repo_root)
        })
        .or_else(|| contexts.first());

    context
        .and_then(|context| context.remote_spec.clone())
        .filter(|remote| !remote.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("no remote is configured for this project"))
}

// ---------------------------------------------------------------------------
// Wizards
// ---------------------------------------------------------------------------

const ALT_WORK_OPTIONS: [(&str, &str); 2] = [
    ("1", "Synced Work"),
    ("2", "Local Work (will not push to remote now)"),
];

fn prompt_alt_work_type_selection(current_branch: &str) -> Result<bool> {
    let mut selected = 0usize;
    let mut rendered_lines = 0usize;
    let raw_mode = RawModeGuard::enter()?;

    loop {
        render_alt_work_type_picker(current_branch, selected, &mut rendered_lines)?;

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
                bail!("Cancelled by user")
            }
            KeyCode::Up | KeyCode::BackTab => {
                selected = selected
                    .checked_sub(1)
                    .unwrap_or(ALT_WORK_OPTIONS.len() - 1);
            }
            KeyCode::Down | KeyCode::Tab => {
                selected = (selected + 1) % ALT_WORK_OPTIONS.len();
            }
            KeyCode::Char(c) if c.is_ascii_digit() => {
                if let Some(index) = c.to_digit(10).and_then(|d| d.checked_sub(1)) {
                    let index = index as usize;
                    if index < ALT_WORK_OPTIONS.len() {
                        selected = index;
                    }
                }
            }
            KeyCode::Enter => {
                let synced = ALT_WORK_OPTIONS[selected].0 == "1";
                drop(raw_mode);
                println!();
                return Ok(synced);
            }
            _ => {}
        }
    }
}

fn render_alt_work_type_picker(
    current_branch: &str,
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
        .context("failed to redraw alt work type picker")?;
    }

    queue!(stdout, MoveToColumn(0), Print("\r\n")).context("render alt work type: blank 1")?;
    queue!(
        stdout,
        MoveToColumn(0),
        SetForegroundColor(Color::DarkGrey),
        Print("Use Up/Down or Tab to select, then press Enter.\r\n"),
        ResetColor
    )
    .context("render alt work type: hint")?;
    queue!(stdout, MoveToColumn(0), Print("\r\n")).context("render alt work type: blank 2")?;
    queue!(
        stdout,
        MoveToColumn(0),
        Print("Your current active branch: "),
        SetForegroundColor(Color::Yellow),
        Print(current_branch),
        ResetColor,
        Print("\r\n")
    )
    .context("render alt work type: branch")?;
    queue!(stdout, MoveToColumn(0), Print("\r\n")).context("render alt work type: blank 3")?;
    queue!(
        stdout,
        MoveToColumn(0),
        SetForegroundColor(Color::Cyan),
        Print("What would you like to do?\r\n"),
        ResetColor
    )
    .context("render alt work type: question")?;
    queue!(stdout, MoveToColumn(0), Print("\r\n")).context("render alt work type: blank 4")?;

    for (index, (_, label)) in ALT_WORK_OPTIONS.iter().enumerate() {
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
        .context("render alt work type: option")?;
    }
    queue!(stdout, MoveToColumn(0), Print("\r\n"))
        .context("render alt work type: trailing blank")?;
    stdout
        .flush()
        .context("failed to flush alt work type picker")?;
    *rendered_lines = 8 + ALT_WORK_OPTIONS.len();
    Ok(())
}

fn prompt_alt_position_continue(project_name: &str, current_branch: &str) -> Result<bool> {
    println!();
    println!("You are here:");
    println!("  {} -> {}", project_name, current_branch);
    println!();
    prompt_confirm_default_yes("Press ENTER or Y to continue; N to cancel: ")
}

fn prompt_confirm_default_yes(prompt: &str) -> Result<bool> {
    loop {
        print!("{prompt}");
        io::stdout().flush().context("failed to flush prompt")?;

        let mut answer = String::new();
        io::stdin()
            .read_line(&mut answer)
            .context("failed to read response")?;

        match answer.trim().to_lowercase().as_str() {
            "" | "y" => return Ok(true),
            "n" => return Ok(false),
            _ => println!("Please answer Y or N."),
        }
    }
}

fn prompt_alt_branch_name(options: &[BranchNameOption]) -> Result<String> {
    if options.is_empty() {
        bail!("alt branch name options are unavailable")
    }

    let mut selected = 0usize;
    let mut rendered_lines = 0usize;
    let raw_mode = RawModeGuard::enter()?;

    loop {
        render_alt_branch_name_picker(options, selected, &mut rendered_lines)?;

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
            KeyCode::Enter => {
                let option = options[selected].clone();
                drop(raw_mode);
                println!();
                let input = if option.requires_input() {
                    Some(prompt_alt_branch_name_input(option.input_label())?)
                } else {
                    None
                };
                return option.resolve_name(input.as_deref());
            }
            _ => {}
        }
    }
}

fn render_alt_branch_name_picker(
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
        .context("failed to redraw alt branch name picker")?;
    }

    queue!(stdout, MoveToColumn(0), Print("\r\n")).context("render alt name: blank 1")?;
    queue!(
        stdout,
        MoveToColumn(0),
        Print("----------------------------------------\r\n")
    )
    .context("render alt name: separator")?;
    queue!(stdout, MoveToColumn(0), Print("\r\n")).context("render alt name: blank 2")?;
    queue!(
        stdout,
        MoveToColumn(0),
        SetForegroundColor(Color::Cyan),
        Print("Please choose a name for the alt branch:\r\n"),
        ResetColor
    )
    .context("render alt name: question")?;
    queue!(stdout, MoveToColumn(0), Print("\r\n")).context("render alt name: blank 3")?;
    queue!(
        stdout,
        MoveToColumn(0),
        SetForegroundColor(Color::DarkGrey),
        Print("Use Up/Down or Tab to select, then press Enter.\r\n"),
        ResetColor
    )
    .context("render alt name: hint")?;
    queue!(stdout, MoveToColumn(0), Print("\r\n")).context("render alt name: blank 4")?;

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
        .context("render alt name: option")?;
    }
    queue!(stdout, MoveToColumn(0), Print("\r\n")).context("render alt name: trailing blank")?;
    stdout
        .flush()
        .context("failed to flush alt branch name picker")?;
    *rendered_lines = 6 + options.len();
    Ok(())
}

fn prompt_alt_branch_name_input(label: &str) -> Result<String> {
    print!("{label}: ");
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alt_merge_parent_for_numeric_alt_returns_dev_branch() {
        let existing = vec!["v0.1.5-dev".to_string()];
        assert_eq!(
            alt_merge_parent_branch("v0.1.5-dev-alt2", &existing).as_deref(),
            Some("v0.1.5-dev")
        );
        assert_eq!(
            alt_merge_parent_branch("v0.1.5-dev-alt2--menu", &existing).as_deref(),
            Some("v0.1.5-dev")
        );
    }

    #[test]
    fn alt_merge_parent_prefers_matching_dev_specific_branch_when_present() {
        let existing = vec!["v0.1.5-dev".to_string(), "v0.1.5-dev--menu".to_string()];
        assert_eq!(
            alt_merge_parent_branch("v0.1.5-dev-alt2--menu", &existing).as_deref(),
            Some("v0.1.5-dev--menu")
        );
    }

    #[test]
    fn alt_merge_parent_for_letter_alt_returns_numeric_alt() {
        let existing = vec!["v0.1.5-dev-alt2".to_string(), "v0.1.5-dev".to_string()];
        assert_eq!(
            alt_merge_parent_branch("v0.1.5-dev-alt2B", &existing).as_deref(),
            Some("v0.1.5-dev-alt2")
        );
        assert_eq!(
            alt_merge_parent_branch("v0.1.5-dev-alt2B--menu", &existing).as_deref(),
            Some("v0.1.5-dev-alt2")
        );
    }

    #[test]
    fn alt_merge_parent_returns_none_for_non_alt_branch() {
        assert_eq!(alt_merge_parent_branch("v0.1.5-dev", &[]), None);
        assert_eq!(alt_merge_parent_branch("main", &[]), None);
    }

    #[test]
    fn alt_sibling_branch_names_lists_other_alts_on_same_dev_branch() {
        let existing = vec![
            "v0.9.1-dev".to_string(),
            "v0.9.1-dev-alt1".to_string(),
            "v0.9.1-dev-alt2--or-this-way".to_string(),
            "v0.9.1-dev-alt3".to_string(),
        ];
        let siblings = alt_sibling_branch_names("v0.9.1-dev-alt2--or-this-way", &existing);
        assert_eq!(
            siblings,
            vec!["v0.9.1-dev-alt1".to_string(), "v0.9.1-dev-alt3".to_string(),]
        );
    }

    #[test]
    fn next_numeric_alt_number_skips_existing_branches() {
        let existing = vec![
            "v0.1.5-dev-alt1".to_string(),
            "v0.1.5-dev-alt2".to_string(),
            "v0.1.5-dev-alt2A".to_string(),
        ];
        assert_eq!(next_numeric_alt_number("v0.1.5-dev", &existing), 3);
    }

    #[test]
    fn next_letter_alt_suffix_starts_at_a_then_increments() {
        let existing = vec!["v0.1.5-dev-alt2A".to_string()];
        assert_eq!(
            next_letter_alt_suffix("v0.1.5-dev-alt2", &existing).expect("next letter"),
            'B'
        );
    }

    #[test]
    fn suggest_alt_branch_name_options_from_dev_branch() {
        let options = suggest_alt_branch_name_options("v0.1.5-dev", &[]).expect("suggest options");
        assert_eq!(options.len(), 3);
        assert_eq!(options[0].preview(), "v0.1.5-dev-alt1");
    }

    #[test]
    fn suggest_alt_branch_name_options_from_numeric_alt() {
        let options =
            suggest_alt_branch_name_options("v0.1.5-dev-alt2", &[]).expect("suggest options");
        assert_eq!(options[0].preview(), "v0.1.5-dev-alt2A");
    }
}
