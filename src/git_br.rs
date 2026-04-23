// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit License v1.2
//
// For details, see the LICENSE file in the repository root.

use anyhow::Result;
use chrono::{Datelike, NaiveDate};

use crate::{git::is_mainline_branch_name, versioning::VersionScheme};

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

pub(crate) fn suggest_branch_name_options(
    scheme: VersionScheme,
    current_branch: &str,
    current_version: &str,
    next_version: &str,
    custom_main_branch: Option<&str>,
    today: NaiveDate,
) -> Result<Vec<BranchNameOption>> {
    scheme
        .validate(current_version)
        .map_err(anyhow::Error::msg)?;
    scheme.validate(next_version).map_err(anyhow::Error::msg)?;

    if is_mainline_branch_name(current_branch, custom_main_branch) {
        return mainline_branch_name_options(scheme, current_version, today);
    }

    Ok(vec![
        fixed_branch_name_option(format!("v{}-dev", next_version)),
        custom_branch_name_option(),
    ])
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
    current_version: &str,
    today: NaiveDate,
) -> Result<Vec<BranchNameOption>> {
    Ok(match scheme {
        VersionScheme::SemVer => semver_mainline_branch_name_options(current_version)?,
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

fn semver_mainline_branch_name_options(current_version: &str) -> Result<Vec<BranchNameOption>> {
    let [major, minor, _patch]: [u32; 3] = parse_numeric_parts(current_version)?
        .try_into()
        .map_err(|_| anyhow::anyhow!("expected 3 semver components"))?;
    let next_minor = format!("{}.{}.x", major, minor + 1);
    let next_major = format!("{}.0.x", major + 1);

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
    BranchNameOption {
        preview: preview.clone(),
        input_mode: BranchNameInputMode::None,
        mode: BranchNameOptionMode::Fixed(preview),
    }
}

fn specific_suffix_branch_name_option(base: String) -> BranchNameOption {
    BranchNameOption {
        preview: format!("{}--specific", base),
        input_mode: BranchNameInputMode::SpecificSuffix,
        mode: BranchNameOptionMode::SpecificSuffix { base },
    }
}

fn custom_branch_name_option() -> BranchNameOption {
    BranchNameOption {
        preview: "custom".to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semver_mainline_suggestions_include_minor_major_and_custom_variants() {
        let today = NaiveDate::from_ymd_opt(2026, 4, 23).unwrap();
        let options = suggest_branch_name_options(
            VersionScheme::SemVer,
            "main",
            "0.24.8",
            "0.25.0",
            None,
            today,
        )
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
        let options = suggest_branch_name_options(
            VersionScheme::SemVer,
            "0.25.x",
            "0.25.3",
            "0.25.4",
            None,
            today,
        )
        .expect("release line suggestions");

        assert_eq!(
            options
                .iter()
                .map(|option| option.preview())
                .collect::<Vec<_>>(),
            vec!["v0.25.4-dev", "custom"]
        );
    }

    #[test]
    fn calver_year_month_mainline_rolls_to_next_month() {
        let options = suggest_branch_name_options(
            VersionScheme::CalVerYearMonthMicro,
            "main",
            "2026.09.1",
            "2026.10.0",
            None,
            NaiveDate::from_ymd_opt(2026, 9, 5).unwrap(),
        )
        .expect("calver month suggestions");

        assert_eq!(options[0].preview(), "2026.10.x");

        let december = suggest_branch_name_options(
            VersionScheme::CalVerYearMonthMicro,
            "main",
            "2026.12.2",
            "2027.01.0",
            None,
            NaiveDate::from_ymd_opt(2026, 12, 5).unwrap(),
        )
        .expect("december calver month suggestions");
        assert_eq!(december[0].preview(), "2027.01.x");
    }

    #[test]
    fn hybrid_year_minor_patch_mainline_uses_next_minor_window() {
        let options = suggest_branch_name_options(
            VersionScheme::HybridYearMinorPatch,
            "main",
            "2026.16.2",
            "2026.17.0",
            None,
            NaiveDate::from_ymd_opt(2026, 10, 5).unwrap(),
        )
        .expect("hybrid suggestions");

        assert_eq!(options[0].preview(), "2026.17.x");

        let next_year = suggest_branch_name_options(
            VersionScheme::HybridYearMinorPatch,
            "main",
            "2026.16.2",
            "2027.1.0",
            None,
            NaiveDate::from_ymd_opt(2027, 1, 5).unwrap(),
        )
        .expect("hybrid next year suggestions");
        assert_eq!(next_year[0].preview(), "2027.1.x");
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
