// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit License
//
// For details, see the LICENSE file in the repository root.

use std::io::{self, Write};

use anyhow::{Context, Result, bail};
use chrono::{Datelike, NaiveDate};
use crossterm::{
    cursor::{MoveTo, MoveToColumn},
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute, queue,
    style::Print,
    terminal::{Clear, ClearType, disable_raw_mode, enable_raw_mode, size},
};

use crate::{
    git::{
        GitCancellation, is_mainline_branch_name, run_git_checked_with_cancel,
        switch_to_existing_branch,
    },
    versioning::{BumpAction, VersionScheme},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BranchNameOption {
    preview: String,
    input_mode: BranchNameInputMode,
    mode: BranchNameOptionMode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BranchNameInputMode {
    None,
    SpecificSuffix,
    Custom,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum BranchNameOptionMode {
    Fixed(String),
    SpecificSuffix { base: String },
    Custom,
}

impl BranchNameOption {
    pub(crate) fn preview(&self) -> &str {
        &self.preview
    }

    pub(crate) fn requires_input(&self) -> bool {
        self.input_mode != BranchNameInputMode::None
    }

    pub(crate) fn input_label(&self) -> &'static str {
        match self.input_mode {
            BranchNameInputMode::None => "Branch name",
            BranchNameInputMode::SpecificSuffix => "Append text",
            BranchNameInputMode::Custom => "Custom branch name",
        }
    }

    pub(crate) fn input_hint(&self) -> &'static str {
        match self.input_mode {
            BranchNameInputMode::None => {
                "Press Enter to use the selected suggestion, or use arrows to choose another option."
            }
            BranchNameInputMode::SpecificSuffix => {
                "Enter the text to append after '--'. Spaces and punctuation are converted into '-'."
            }
            BranchNameInputMode::Custom => {
                "Enter a full custom branch name. Spaces and punctuation are converted into '-'."
            }
        }
    }

    pub(crate) fn preview_with_input(&self, input: Option<&str>) -> String {
        self.resolve_name(input)
            .unwrap_or_else(|_| self.preview.clone())
    }

    pub(crate) fn resolve_name(&self, input: Option<&str>) -> Result<String> {
        match &self.mode {
            BranchNameOptionMode::Fixed(value) => Ok(value.clone()),
            BranchNameOptionMode::SpecificSuffix { base } => {
                let suffix = sanitize_branch_fragment(input.unwrap_or_default())
                    .ok_or_else(|| anyhow::anyhow!("branch suffix cannot be empty"))?;
                Ok(format!("{}--{}", base, suffix))
            }
            BranchNameOptionMode::Custom => sanitize_branch_fragment(input.unwrap_or_default())
                .ok_or_else(|| anyhow::anyhow!("branch name cannot be empty")),
        }
    }
}

pub(crate) struct BranchNameSuggestionRequest<'a> {
    pub(crate) scheme: VersionScheme,
    pub(crate) action: BumpAction,
    pub(crate) current_branch: &'a str,
    pub(crate) current_version: &'a str,
    pub(crate) next_version: &'a str,
    pub(crate) custom_main_branch: Option<&'a str>,
    pub(crate) existing_branches: &'a [String],
    pub(crate) today: NaiveDate,
}

pub(crate) fn suggest_branch_name_options(
    request: BranchNameSuggestionRequest<'_>,
) -> Result<Vec<BranchNameOption>> {
    request
        .scheme
        .validate(request.current_version)
        .map_err(anyhow::Error::msg)?;
    request
        .scheme
        .validate(request.next_version)
        .map_err(anyhow::Error::msg)?;

    if is_mainline_branch_name(request.current_branch, request.custom_main_branch) {
        return mainline_branch_name_options(
            request.scheme,
            request.action,
            request.current_version,
            request.existing_branches,
            request.today,
        );
    }

    if request.scheme == VersionScheme::SemVer && request.action == BumpAction::Patch {
        let base = next_available_non_mainline_semver_dev_branch(
            request.next_version,
            request.existing_branches,
            request.today,
        )?;
        return Ok(vec![
            fixed_branch_name_option(base.clone()),
            specific_suffix_branch_name_option(base),
            custom_branch_name_option(),
        ]);
    }

    Ok(vec![
        fixed_branch_name_option(format!("v{}-dev", request.next_version)),
        custom_branch_name_option(),
    ])
}

/// When `branch` is a semver patch dev branch `vMAJOR.MINOR.PATCH-dev`, optionally followed by
/// `--extra`, returns canonical `vMAJOR.MINOR.PATCH-dev`. Otherwise returns `branch` trimmed.
pub(crate) fn semver_dev_branch_canonical_label(branch: &str) -> String {
    let trimmed = branch.trim();
    if let Some(version) = semver_patch_dev_version_from_v_prefix(trimmed) {
        format!("v{}-dev", version)
    } else {
        trimmed.to_string()
    }
}

fn semver_patch_dev_version_from_v_prefix(branch_name: &str) -> Option<String> {
    let normalized = branch_name.trim_start_matches('v');
    let normalized = normalized
        .split_once("--")
        .map(|(base, _)| base)
        .unwrap_or(normalized);
    let release_version = normalized.strip_suffix("-dev")?;
    let mut parts = release_version.split('.');
    let major = parts.next()?.parse::<u32>().ok()?;
    let minor = parts.next()?.parse::<u32>().ok()?;
    let patch = parts.next()?.parse::<u32>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some(format!("{}.{}.{}", major, minor, patch))
}

pub(crate) fn is_release_line_branch(scheme: VersionScheme, branch_name: &str) -> bool {
    let normalized = branch_name
        .trim()
        .trim_start_matches('v')
        .split_once("--")
        .map(|(base, _)| base)
        .unwrap_or(branch_name.trim());
    if !normalized.ends_with(".x") {
        return false;
    }

    let candidate = format!("{}0", &normalized[..normalized.len().saturating_sub(1)]);
    scheme.validate(&candidate).is_ok()
}

fn mainline_branch_name_options(
    scheme: VersionScheme,
    action: BumpAction,
    current_version: &str,
    existing_branches: &[String],
    today: NaiveDate,
) -> Result<Vec<BranchNameOption>> {
    Ok(match scheme {
        VersionScheme::SemVer => {
            semver_mainline_branch_name_options(current_version, action, existing_branches)?
        }
        VersionScheme::CalVerYearMonthMicro => {
            let (year, month) = next_month_window(today);
            vec![
                fixed_branch_name_option(format!("{:04}.{:02}.x", year, month)),
                custom_branch_name_option(),
            ]
        }
        VersionScheme::CalVerShortYearMonthMicro => {
            let (year, month) = next_month_window(today);
            vec![
                fixed_branch_name_option(format!("{:02}.{:02}.x", year % 100, month)),
                custom_branch_name_option(),
            ]
        }
        VersionScheme::CalVerYearMonthDayMicro => {
            let next_day = today
                .succ_opt()
                .ok_or_else(|| anyhow::anyhow!("failed to compute the next calendar day"))?;
            vec![
                fixed_branch_name_option(format!(
                    "{:04}.{:02}.{:02}.x",
                    next_day.year(),
                    next_day.month(),
                    next_day.day()
                )),
                custom_branch_name_option(),
            ]
        }
        VersionScheme::HybridYearMinorPatch => {
            let [year, minor, _patch]: [u32; 3] = parse_numeric_parts(current_version)?
                .try_into()
                .map_err(|_| anyhow::anyhow!("expected 3 hybrid components"))?;
            let current_year = today.year() as u32;
            let next_minor = if year == current_year { minor + 1 } else { 1 };
            vec![
                fixed_branch_name_option(format!("{:04}.{}.x", current_year, next_minor)),
                custom_branch_name_option(),
            ]
        }
        VersionScheme::HybridYearPatch => {
            let [year, patch]: [u32; 2] = parse_numeric_parts(current_version)?
                .try_into()
                .map_err(|_| anyhow::anyhow!("expected 2 hybrid components"))?;
            let current_year = today.year() as u32;
            let next_patch = if year == current_year { patch + 1 } else { 1 };
            vec![
                fixed_branch_name_option(format!("{:04}.{}.x", current_year, next_patch)),
                custom_branch_name_option(),
            ]
        }
    })
}

fn semver_mainline_branch_name_options(
    current_version: &str,
    action: BumpAction,
    existing_branches: &[String],
) -> Result<Vec<BranchNameOption>> {
    let [major, minor, _patch]: [u32; 3] = parse_numeric_parts(current_version)?
        .try_into()
        .map_err(|_| anyhow::anyhow!("expected 3 semver components"))?;
    let next_minor = format!("{}.{}.x", major, minor + 1);
    let next_major = format!("{}.0.x", major + 1);

    if action == BumpAction::Major {
        let major_option = next_available_major_line(major, existing_branches);
        return Ok(vec![
            fixed_branch_name_option(major_option.clone()),
            specific_suffix_branch_name_option(major_option),
            custom_branch_name_option(),
        ]);
    }

    if action == BumpAction::Minor {
        let mut options = Vec::new();
        let minor_option = next_available_minor_line(major, minor + 1, existing_branches);
        options.push(fixed_branch_name_option(minor_option.clone()));
        options.push(specific_suffix_branch_name_option(minor_option.clone()));
        if !minor_option.eq_ignore_ascii_case(&next_minor)
            && branch_exists(existing_branches, &next_minor)
        {
            options.push(specific_suffix_branch_name_option(next_minor));
        }

        let major_option = next_available_major_line(major, existing_branches);
        options.push(fixed_branch_name_option(major_option.clone()));
        options.push(specific_suffix_branch_name_option(major_option));
        options.push(custom_branch_name_option());
        return Ok(options);
    }

    Ok(vec![
        fixed_branch_name_option(next_minor.clone()),
        specific_suffix_branch_name_option(next_minor),
        fixed_branch_name_option(next_major.clone()),
        specific_suffix_branch_name_option(next_major),
        custom_branch_name_option(),
    ])
}

fn next_month_window(today: NaiveDate) -> (u32, u32) {
    if today.month() == 12 {
        ((today.year() + 1) as u32, 1)
    } else {
        (today.year() as u32, today.month() + 1)
    }
}

fn fixed_branch_name_option(preview: String) -> BranchNameOption {
    fixed_branch_name_option_with_value(preview.clone(), preview)
}

pub(crate) fn fixed_branch_name_option_with_value(
    preview: String,
    value: String,
) -> BranchNameOption {
    BranchNameOption {
        preview,
        input_mode: BranchNameInputMode::None,
        mode: BranchNameOptionMode::Fixed(value),
    }
}

pub(crate) fn specific_suffix_branch_name_option(base: String) -> BranchNameOption {
    BranchNameOption {
        preview: format!("{}--specific", base),
        input_mode: BranchNameInputMode::SpecificSuffix,
        mode: BranchNameOptionMode::SpecificSuffix { base },
    }
}

pub(crate) fn custom_branch_name_option() -> BranchNameOption {
    custom_branch_name_option_with_preview("custom")
}

pub(crate) fn custom_branch_name_option_with_preview(preview: &str) -> BranchNameOption {
    BranchNameOption {
        preview: preview.to_string(),
        input_mode: BranchNameInputMode::Custom,
        mode: BranchNameOptionMode::Custom,
    }
}

fn parse_numeric_parts(value: &str) -> Result<Vec<u32>> {
    value
        .split('.')
        .map(|part| {
            part.parse::<u32>()
                .map_err(|_| anyhow::anyhow!("invalid numeric component '{}'", part))
        })
        .collect()
}

fn branch_exists(existing_branches: &[String], candidate: &str) -> bool {
    existing_branches
        .iter()
        .any(|branch| branch.eq_ignore_ascii_case(candidate))
}

fn next_available_minor_line(
    major: u32,
    starting_minor: u32,
    existing_branches: &[String],
) -> String {
    let mut minor = starting_minor;
    loop {
        let candidate = format!("{}.{}.x", major, minor);
        if !branch_exists(existing_branches, &candidate) {
            return candidate;
        }
        minor += 1;
    }
}

fn next_available_major_line(major: u32, existing_branches: &[String]) -> String {
    let mut next_major = major + 1;
    loop {
        let candidate = format!("{}.0.x", next_major);
        if !branch_exists(existing_branches, &candidate) {
            return candidate;
        }
        next_major += 1;
    }
}

fn next_available_non_mainline_semver_dev_branch(
    next_version: &str,
    existing_branches: &[String],
    today: NaiveDate,
) -> Result<String> {
    let mut candidate_version = next_version.to_string();
    loop {
        let exact = format!("v{}-dev", candidate_version);
        let with_suffix = format!("{}--", exact);
        let exists = existing_branches.iter().any(|branch| {
            branch.eq_ignore_ascii_case(&exact)
                || branch
                    .to_ascii_lowercase()
                    .starts_with(&with_suffix.to_ascii_lowercase())
        });
        if !exists {
            return Ok(exact);
        }

        candidate_version = VersionScheme::SemVer
            .bump(&candidate_version, BumpAction::Patch, today)
            .map_err(anyhow::Error::msg)?;
    }
}

fn sanitize_branch_fragment(value: &str) -> Option<String> {
    let mut sanitized = String::new();
    let mut last_was_separator = true;

    for character in value.trim().chars() {
        if character.is_ascii_alphanumeric() {
            sanitized.push(character);
            last_was_separator = false;
        } else if !last_was_separator {
            sanitized.push('-');
            last_was_separator = true;
        }
    }

    let sanitized = sanitized.trim_matches('-').to_string();
    (!sanitized.is_empty()).then_some(sanitized)
}

const ANSI_YELLOW: &str = "\x1b[33m";
const ANSI_RESET: &str = "\x1b[0m";

pub(crate) struct BranchPickerPrompt {
    pub(crate) header_hint: &'static str,
    pub(crate) question: &'static str,
}

pub(crate) const BRANCH_CD_PICKER_PROMPT: BranchPickerPrompt = BranchPickerPrompt {
    header_hint: "\x1b[33mPress ENTER to choose, or SPACE to load another 5 branches:\x1b[0m",
    question: "What branch would you like to cd into?",
};

pub(crate) const LOCAL_MERGE_TARGET_PICKER_PROMPT: BranchPickerPrompt = BranchPickerPrompt {
    header_hint: "\x1b[90mPress ENTER to choose, or SPACE to load another 5 branches:\x1b[0m",
    question: "What branch would you like to be the \x1b[35mTARGET\x1b[0m of this local merge?",
};

pub(crate) fn run_branch_cd(repo_root: &str, cancel: Option<GitCancellation>) -> Result<()> {
    let selected_branch = prompt_branch_selection(repo_root, &BRANCH_CD_PICKER_PROMPT, cancel)?;
    switch_to_existing_branch(repo_root, &selected_branch)?;

    println!();
    println!("switched to branch: \x1b[33m{selected_branch}\x1b[0m");
    println!();
    Ok(())
}

pub(crate) fn prompt_branch_selection(
    repo_root: &str,
    prompt: &BranchPickerPrompt,
    cancel: Option<GitCancellation>,
) -> Result<String> {
    let mut branches = Vec::new();
    let mut page = 0;
    let mut selected = 0;

    load_branches(repo_root, &mut branches, page, cancel.clone())?;

    if branches.is_empty() {
        bail!("no branches found in this repository");
    }

    let raw_mode = TerminalRawModeGuard::enter()?;

    loop {
        if cancel.as_ref().is_some_and(|cancel| cancel.is_cancelled()) {
            bail!("cancelled by user");
        }

        render_branch_selection_ui(&branches, selected, prompt)?;

        let Event::Key(key) = event::read().context("failed to read branch selection input")?
        else {
            continue;
        };

        if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
            continue;
        }

        match key.code {
            KeyCode::Up if selected > 0 => {
                selected = selected.saturating_sub(1);
            }
            KeyCode::Down if selected < branches.len().saturating_sub(1) => {
                selected += 1;
            }
            KeyCode::Enter => {
                let selected_branch = branches[selected].clone();
                drop(raw_mode);
                println!();
                return Ok(selected_branch);
            }
            KeyCode::Char(' ') => {
                page += 1;
                let start_index = branches.len();
                load_branches(repo_root, &mut branches, page, cancel.clone())?;

                if branches.len() == start_index {
                    page -= 1;
                } else {
                    selected = start_index;
                }
            }
            KeyCode::Esc => {
                drop(raw_mode);
                println!();
                bail!("Cancelled by user");
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                drop(raw_mode);
                println!();
                bail!("Cancelled by user");
            }
            _ => {}
        }
    }
}

fn load_branches(
    repo_root: &str,
    branches: &mut Vec<String>,
    page: usize,
    cancel: Option<GitCancellation>,
) -> Result<()> {
    let start_line = page * 5 + 1;

    let output =
        run_git_checked_with_cancel(repo_root, &["branch", "--sort=-committerdate"], cancel)?;

    let lines: Vec<&str> = output.lines().collect();

    for line in lines.iter().skip(start_line - 1).take(5) {
        // Remove the "* " or "  " prefix and clean up the branch name
        let branch_name = line
            .trim_start_matches("* ")
            .trim_start_matches("  ")
            .trim();
        if !branch_name.is_empty() {
            branches.push(branch_name.to_string());
        }
    }

    Ok(())
}

fn render_branch_selection_ui(
    branches: &[String],
    selected: usize,
    prompt: &BranchPickerPrompt,
) -> Result<()> {
    let mut stdout = io::stdout();
    let (terminal_width, _) = size().context("failed to read terminal size")?;

    execute!(stdout, MoveTo(0, 0), Clear(ClearType::All))
        .context("failed to render branch selection UI")?;

    queue!(
        stdout,
        MoveToColumn(0),
        Print(format!("{}\r\n\r\n", prompt.header_hint)),
        Print(format!("{}\r\n\r\n", prompt.question))
    )
    .context("failed to queue branch selection header")?;

    // Branch list
    for (i, branch) in branches.iter().enumerate() {
        let prefix = if i == selected {
            format!("{}> {}.{}{}", ANSI_YELLOW, i + 1, branch, ANSI_RESET)
        } else {
            format!("  {}.{}", i + 1, branch)
        };

        let truncated = truncate_for_terminal(&prefix, terminal_width as usize);
        queue!(stdout, MoveToColumn(0), Print(truncated), Print("\r\n"))
            .context("failed to queue branch selection item")?;
    }

    stdout
        .flush()
        .context("failed to flush branch selection UI")?;
    Ok(())
}

fn truncate_for_terminal(value: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    value.chars().take(width).collect()
}

struct TerminalRawModeGuard;

impl TerminalRawModeGuard {
    fn enter() -> Result<Self> {
        enable_raw_mode().context("failed to enable raw mode")?;
        Ok(Self)
    }
}

impl Drop for TerminalRawModeGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semver_mainline_suggestions_include_minor_major_and_custom_variants() {
        let today = NaiveDate::from_ymd_opt(2026, 4, 23).unwrap();
        let options = suggest_branch_name_options(BranchNameSuggestionRequest {
            scheme: VersionScheme::SemVer,
            action: BumpAction::Minor,
            current_branch: "main",
            current_version: "0.24.8",
            next_version: "0.25.0",
            custom_main_branch: None,
            existing_branches: &[],
            today,
        })
        .expect("semver suggestions");

        assert_eq!(
            options
                .iter()
                .map(|option| option.preview())
                .collect::<Vec<_>>(),
            vec![
                "0.25.x",
                "0.25.x--specific",
                "1.0.x",
                "1.0.x--specific",
                "custom",
            ]
        );
        assert_eq!(
            options[1]
                .resolve_name(Some("Menu Improvements"))
                .expect("specific suffix"),
            "0.25.x--Menu-Improvements"
        );
    }

    #[test]
    fn release_line_suggestions_use_next_version_dev() {
        let today = NaiveDate::from_ymd_opt(2026, 4, 23).unwrap();
        let options = suggest_branch_name_options(BranchNameSuggestionRequest {
            scheme: VersionScheme::SemVer,
            action: BumpAction::Patch,
            current_branch: "0.25.x",
            current_version: "0.25.3",
            next_version: "0.25.4",
            custom_main_branch: None,
            existing_branches: &[],
            today,
        })
        .expect("release line suggestions");

        assert_eq!(
            options
                .iter()
                .map(|option| option.preview())
                .collect::<Vec<_>>(),
            vec!["v0.25.4-dev", "v0.25.4-dev--specific", "custom"]
        );
        assert_eq!(
            options[1]
                .resolve_name(Some("menu-hotfix"))
                .expect("specific suffix"),
            "v0.25.4-dev--menu-hotfix"
        );
    }

    #[test]
    fn release_line_suggestions_skip_existing_dev_patch_branches() {
        let today = NaiveDate::from_ymd_opt(2026, 4, 23).unwrap();
        let options = suggest_branch_name_options(BranchNameSuggestionRequest {
            scheme: VersionScheme::SemVer,
            action: BumpAction::Patch,
            current_branch: "0.25.x",
            current_version: "0.25.3",
            next_version: "0.25.4",
            custom_main_branch: None,
            existing_branches: &[
                "v0.25.4-dev".to_string(),
                "v0.25.4-dev--specific".to_string(),
            ],
            today,
        })
        .expect("release line suggestions");

        assert_eq!(
            options
                .iter()
                .map(|option| option.preview())
                .collect::<Vec<_>>(),
            vec!["v0.25.5-dev", "v0.25.5-dev--specific", "custom"]
        );
    }

    #[test]
    fn calver_year_month_mainline_rolls_to_next_month() {
        let options = suggest_branch_name_options(BranchNameSuggestionRequest {
            scheme: VersionScheme::CalVerYearMonthMicro,
            action: BumpAction::Auto,
            current_branch: "main",
            current_version: "2026.09.1",
            next_version: "2026.10.0",
            custom_main_branch: None,
            existing_branches: &[],
            today: NaiveDate::from_ymd_opt(2026, 9, 5).unwrap(),
        })
        .expect("calver month suggestions");

        assert_eq!(options[0].preview(), "2026.10.x");

        let december = suggest_branch_name_options(BranchNameSuggestionRequest {
            scheme: VersionScheme::CalVerYearMonthMicro,
            action: BumpAction::Auto,
            current_branch: "main",
            current_version: "2026.12.2",
            next_version: "2027.01.0",
            custom_main_branch: None,
            existing_branches: &[],
            today: NaiveDate::from_ymd_opt(2026, 12, 5).unwrap(),
        })
        .expect("december calver month suggestions");
        assert_eq!(december[0].preview(), "2027.01.x");
    }

    #[test]
    fn hybrid_year_minor_patch_mainline_uses_next_minor_window() {
        let options = suggest_branch_name_options(BranchNameSuggestionRequest {
            scheme: VersionScheme::HybridYearMinorPatch,
            action: BumpAction::Minor,
            current_branch: "main",
            current_version: "2026.16.2",
            next_version: "2026.17.0",
            custom_main_branch: None,
            existing_branches: &[],
            today: NaiveDate::from_ymd_opt(2026, 10, 5).unwrap(),
        })
        .expect("hybrid suggestions");

        assert_eq!(options[0].preview(), "2026.17.x");

        let next_year = suggest_branch_name_options(BranchNameSuggestionRequest {
            scheme: VersionScheme::HybridYearMinorPatch,
            action: BumpAction::Minor,
            current_branch: "main",
            current_version: "2026.16.2",
            next_version: "2027.1.0",
            custom_main_branch: None,
            existing_branches: &[],
            today: NaiveDate::from_ymd_opt(2027, 1, 5).unwrap(),
        })
        .expect("hybrid next year suggestions");
        assert_eq!(next_year[0].preview(), "2027.1.x");
    }

    #[test]
    fn semver_minor_suggestions_skip_existing_release_lines() {
        let options = suggest_branch_name_options(BranchNameSuggestionRequest {
            scheme: VersionScheme::SemVer,
            action: BumpAction::Minor,
            current_branch: "main",
            current_version: "0.0.3",
            next_version: "0.1.0",
            custom_main_branch: None,
            existing_branches: &["0.1.x".to_string()],
            today: NaiveDate::from_ymd_opt(2026, 4, 23).unwrap(),
        })
        .expect("minor suggestions");

        assert_eq!(
            options
                .iter()
                .map(|option| option.preview())
                .collect::<Vec<_>>(),
            vec![
                "0.2.x",
                "0.2.x--specific",
                "0.1.x--specific",
                "1.0.x",
                "1.0.x--specific",
                "custom",
            ]
        );
    }

    #[test]
    fn semver_major_suggestions_only_offer_next_available_major_line() {
        let options = suggest_branch_name_options(BranchNameSuggestionRequest {
            scheme: VersionScheme::SemVer,
            action: BumpAction::Major,
            current_branch: "main",
            current_version: "0.0.3",
            next_version: "1.0.0",
            custom_main_branch: None,
            existing_branches: &["1.0.x".to_string()],
            today: NaiveDate::from_ymd_opt(2026, 4, 23).unwrap(),
        })
        .expect("major suggestions");

        assert_eq!(
            options
                .iter()
                .map(|option| option.preview())
                .collect::<Vec<_>>(),
            vec!["2.0.x", "2.0.x--specific", "custom"]
        );
    }

    #[test]
    fn semver_dev_branch_canonical_label_strips_specific_suffix() {
        assert_eq!(
            semver_dev_branch_canonical_label("v0.18.1-dev--menu-work"),
            "v0.18.1-dev"
        );
        assert_eq!(
            semver_dev_branch_canonical_label("0.18.1-dev--x"),
            "v0.18.1-dev"
        );
        assert_eq!(
            semver_dev_branch_canonical_label("feature/menu"),
            "feature/menu"
        );
    }

    #[test]
    fn release_line_detector_accepts_version_x_branches() {
        assert!(is_release_line_branch(VersionScheme::SemVer, "0.25.x"));
        assert!(is_release_line_branch(
            VersionScheme::CalVerYearMonthMicro,
            "2026.10.x"
        ));
        assert!(is_release_line_branch(
            VersionScheme::HybridYearMinorPatch,
            "2026.34.x"
        ));
        assert!(!is_release_line_branch(
            VersionScheme::SemVer,
            "feature/menu"
        ));
    }
}
