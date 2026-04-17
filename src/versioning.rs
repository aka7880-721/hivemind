// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit License v1.2
//
// For details, see the LICENSE file in the repository root.

use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BumpAction {
    Auto,
    Major,
    Minor,
    Patch,
}

impl BumpAction {
    pub fn display_name(self) -> &'static str {
        match self {
            BumpAction::Auto => "Auto",
            BumpAction::Major => "Major",
            BumpAction::Minor => "Minor",
            BumpAction::Patch => "Patch",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum VersionScheme {
    #[default]
    SemVer,
    CalVerYearMonthMicro,
    CalVerShortYearMonthMicro,
    CalVerYearMonthDayMicro,
    HybridYearMinorPatch,
    HybridYearPatch,
}

impl VersionScheme {
    pub const SEMVER_ACTIONS: [BumpAction; 3] = [BumpAction::Major, BumpAction::Minor, BumpAction::Patch];
    pub const CALVER_ACTIONS: [BumpAction; 1] = [BumpAction::Auto];
    pub const HYBRID_MINOR_PATCH_ACTIONS: [BumpAction; 2] = [BumpAction::Minor, BumpAction::Patch];
    pub const HYBRID_PATCH_ACTIONS: [BumpAction; 1] = [BumpAction::Patch];

    pub const ALL: [VersionScheme; 6] = [
        VersionScheme::SemVer,
        VersionScheme::CalVerYearMonthMicro,
        VersionScheme::CalVerShortYearMonthMicro,
        VersionScheme::CalVerYearMonthDayMicro,
        VersionScheme::HybridYearMinorPatch,
        VersionScheme::HybridYearPatch,
    ];

    pub fn display_name(self) -> &'static str {
        match self {
            VersionScheme::SemVer => "SemVer",
            VersionScheme::CalVerYearMonthMicro => "CalVer YYYY.MM.Micro",
            VersionScheme::CalVerShortYearMonthMicro => "CalVer YY.MM.Micro",
            VersionScheme::CalVerYearMonthDayMicro => "CalVer YYYY.MM.DD.Micro",
            VersionScheme::HybridYearMinorPatch => "Hybrid YYYY.MINOR.PATCH",
            VersionScheme::HybridYearPatch => "Hybrid YYYY.PATCH",
        }
    }

    pub fn example(self) -> &'static str {
        match self {
            VersionScheme::SemVer => "1.2.3",
            VersionScheme::CalVerYearMonthMicro => "2026.04.7",
            VersionScheme::CalVerShortYearMonthMicro => "26.04.7",
            VersionScheme::CalVerYearMonthDayMicro => "2026.04.06.2",
            VersionScheme::HybridYearMinorPatch => "2026.4.12",
            VersionScheme::HybridYearPatch => "2026.12",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            VersionScheme::SemVer => "MAJOR.MINOR.PATCH",
            VersionScheme::CalVerYearMonthMicro => "Year, month, then micro increment",
            VersionScheme::CalVerShortYearMonthMicro => "Two-digit year, month, then micro increment",
            VersionScheme::CalVerYearMonthDayMicro => "Year, month, day, then micro increment",
            VersionScheme::HybridYearMinorPatch => "Year followed by minor and patch counters",
            VersionScheme::HybridYearPatch => "Year followed by a single patch counter",
        }
    }

    pub fn next(self) -> Self {
        let index = Self::ALL.iter().position(|candidate| *candidate == self).unwrap_or(0);
        Self::ALL[(index + 1) % Self::ALL.len()]
    }

    pub fn previous(self) -> Self {
        let index = Self::ALL.iter().position(|candidate| *candidate == self).unwrap_or(0);
        Self::ALL[(index + Self::ALL.len() - 1) % Self::ALL.len()]
    }

    pub fn validate(self, value: &str) -> Result<(), String> {
        match self {
            VersionScheme::SemVer => validate_parts(value, &[PartRule::Any, PartRule::Any, PartRule::Any]),
            VersionScheme::CalVerYearMonthMicro => {
                validate_parts(value, &[PartRule::Digits(4), PartRule::Month, PartRule::Any])
            }
            VersionScheme::CalVerShortYearMonthMicro => {
                validate_parts(value, &[PartRule::Digits(2), PartRule::Month, PartRule::Any])
            }
            VersionScheme::CalVerYearMonthDayMicro => validate_parts(
                value,
                &[PartRule::Digits(4), PartRule::Month, PartRule::Day, PartRule::Any],
            ),
            VersionScheme::HybridYearMinorPatch => {
                validate_parts(value, &[PartRule::Digits(4), PartRule::Any, PartRule::Any])
            }
            VersionScheme::HybridYearPatch => validate_parts(value, &[PartRule::Digits(4), PartRule::Any]),
        }
    }

    pub fn supported_actions(self) -> &'static [BumpAction] {
        match self {
            VersionScheme::SemVer => &Self::SEMVER_ACTIONS,
            VersionScheme::CalVerYearMonthMicro
            | VersionScheme::CalVerShortYearMonthMicro
            | VersionScheme::CalVerYearMonthDayMicro => &Self::CALVER_ACTIONS,
            VersionScheme::HybridYearMinorPatch => &Self::HYBRID_MINOR_PATCH_ACTIONS,
            VersionScheme::HybridYearPatch => &Self::HYBRID_PATCH_ACTIONS,
        }
    }

    pub fn bump(self, value: &str, action: BumpAction, today: NaiveDate) -> Result<String, String> {
        self.validate(value)?;
        match self {
            VersionScheme::SemVer => bump_semver(value, action),
            VersionScheme::CalVerYearMonthMicro => bump_calver_year_month_micro(value, action, today),
            VersionScheme::CalVerShortYearMonthMicro => bump_calver_short_year_month_micro(value, action, today),
            VersionScheme::CalVerYearMonthDayMicro => bump_calver_year_month_day_micro(value, action, today),
            VersionScheme::HybridYearMinorPatch => bump_hybrid_year_minor_patch(value, action, today),
            VersionScheme::HybridYearPatch => bump_hybrid_year_patch(value, action, today),
        }
    }
}

#[derive(Clone, Copy)]
enum PartRule {
    Any,
    Digits(usize),
    Month,
    Day,
}

fn validate_parts(value: &str, rules: &[PartRule]) -> Result<(), String> {
    let parts = value.split('.').collect::<Vec<_>>();
    if parts.len() != rules.len() {
        return Err(format!("expected {} dot-separated parts", rules.len()));
    }

    for (part, rule) in parts.into_iter().zip(rules.iter().copied()) {
        if part.is_empty() || !part.chars().all(|character| character.is_ascii_digit()) {
            return Err("version parts must be numeric".to_string());
        }

        match rule {
            PartRule::Any => {}
            PartRule::Digits(width) => {
                if part.len() != width {
                    return Err(format!("expected a {}-digit component", width));
                }
            }
            PartRule::Month => {
                let month = part.parse::<u32>().map_err(|_| "invalid month value".to_string())?;
                if !(1..=12).contains(&month) {
                    return Err("month must be between 1 and 12".to_string());
                }
            }
            PartRule::Day => {
                let day = part.parse::<u32>().map_err(|_| "invalid day value".to_string())?;
                if !(1..=31).contains(&day) {
                    return Err("day must be between 1 and 31".to_string());
                }
            }
        }
    }

    Ok(())
}

fn bump_semver(value: &str, action: BumpAction) -> Result<String, String> {
    let parts = parse_numeric_parts(value)?;
    let [major, minor, patch]: [u32; 3] = parts
        .try_into()
        .map_err(|_| "expected 3 semver components".to_string())?;

    let bumped = match action {
        BumpAction::Major => [major + 1, 0, 0],
        BumpAction::Minor => [major, minor + 1, 0],
        BumpAction::Patch => [major, minor, patch + 1],
        BumpAction::Auto => return Err("auto bump is not supported for SemVer".to_string()),
    };

    Ok(format!("{}.{}.{}", bumped[0], bumped[1], bumped[2]))
}

fn bump_calver_year_month_micro(value: &str, action: BumpAction, today: NaiveDate) -> Result<String, String> {
    require_action(action, &[BumpAction::Auto])?;
    let parts = parse_numeric_parts(value)?;
    let [year, month, micro]: [u32; 3] = parts
        .try_into()
        .map_err(|_| "expected 3 calver components".to_string())?;
    let current_year = today.year() as u32;
    let current_month = today.month();
    let next_micro = if year == current_year && month == current_month { micro + 1 } else { 0 };
    Ok(format!("{:04}.{:02}.{}", current_year, current_month, next_micro))
}

fn bump_calver_short_year_month_micro(value: &str, action: BumpAction, today: NaiveDate) -> Result<String, String> {
    require_action(action, &[BumpAction::Auto])?;
    let parts = parse_numeric_parts(value)?;
    let [year, month, micro]: [u32; 3] = parts
        .try_into()
        .map_err(|_| "expected 3 calver components".to_string())?;
    let current_year = (today.year() % 100) as u32;
    let current_month = today.month();
    let next_micro = if year == current_year && month == current_month { micro + 1 } else { 0 };
    Ok(format!("{:02}.{:02}.{}", current_year, current_month, next_micro))
}

fn bump_calver_year_month_day_micro(value: &str, action: BumpAction, today: NaiveDate) -> Result<String, String> {
    require_action(action, &[BumpAction::Auto])?;
    let parts = parse_numeric_parts(value)?;
    let [year, month, day, micro]: [u32; 4] = parts
        .try_into()
        .map_err(|_| "expected 4 calver components".to_string())?;
    let current_year = today.year() as u32;
    let current_month = today.month();
    let current_day = today.day();
    let next_micro = if year == current_year && month == current_month && day == current_day {
        micro + 1
    } else {
        0
    };
    Ok(format!("{:04}.{:02}.{:02}.{}", current_year, current_month, current_day, next_micro))
}

fn bump_hybrid_year_minor_patch(value: &str, action: BumpAction, today: NaiveDate) -> Result<String, String> {
    require_action(action, &[BumpAction::Minor, BumpAction::Patch])?;
    let parts = parse_numeric_parts(value)?;
    let [year, minor, patch]: [u32; 3] = parts
        .try_into()
        .map_err(|_| "expected 3 hybrid components".to_string())?;
    let current_year = today.year() as u32;

    let (next_minor, next_patch) = if year != current_year {
        match action {
            BumpAction::Minor => (1, 0),
            BumpAction::Patch => (0, 1),
            _ => unreachable!(),
        }
    } else {
        match action {
            BumpAction::Minor => (minor + 1, 0),
            BumpAction::Patch => (minor, patch + 1),
            _ => unreachable!(),
        }
    };

    Ok(format!("{:04}.{}.{}", current_year, next_minor, next_patch))
}

fn bump_hybrid_year_patch(value: &str, action: BumpAction, today: NaiveDate) -> Result<String, String> {
    require_action(action, &[BumpAction::Patch])?;
    let parts = parse_numeric_parts(value)?;
    let [year, patch]: [u32; 2] = parts
        .try_into()
        .map_err(|_| "expected 2 hybrid components".to_string())?;
    let current_year = today.year() as u32;
    let next_patch = if year == current_year { patch + 1 } else { 1 };
    Ok(format!("{:04}.{}", current_year, next_patch))
}

fn require_action(action: BumpAction, allowed: &[BumpAction]) -> Result<(), String> {
    if allowed.contains(&action) {
        Ok(())
    } else {
        Err(format!("{} bump is not supported for this version scheme", action.display_name()))
    }
}

fn parse_numeric_parts(value: &str) -> Result<Vec<u32>, String> {
    value
        .split('.')
        .map(|part| part.parse::<u32>().map_err(|_| format!("invalid numeric component '{}'", part)))
        .collect()
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::{BumpAction, VersionScheme};

    #[test]
    fn semver_accepts_three_numeric_segments() {
        assert!(VersionScheme::SemVer.validate("1.2.3").is_ok());
        assert!(VersionScheme::SemVer.validate("1.2").is_err());
    }

    #[test]
    fn calver_requires_month_range() {
        assert!(VersionScheme::CalVerYearMonthMicro.validate("2026.04.1").is_ok());
        assert!(VersionScheme::CalVerYearMonthMicro.validate("2026.13.1").is_err());
    }

    #[test]
    fn hybrid_year_patch_requires_four_digit_year() {
        assert!(VersionScheme::HybridYearPatch.validate("2026.12").is_ok());
        assert!(VersionScheme::HybridYearPatch.validate("26.12").is_err());
    }

    #[test]
    fn year_month_day_calver_requires_four_parts() {
        assert!(VersionScheme::CalVerYearMonthDayMicro.validate("2026.04.06.2").is_ok());
        assert!(VersionScheme::CalVerYearMonthDayMicro.validate("2026.04.2").is_err());
    }

    #[test]
    fn semver_patch_bump_increments_patch() {
        let today = NaiveDate::from_ymd_opt(2026, 4, 6).unwrap();
        let bumped = VersionScheme::SemVer.bump("1.2.3", BumpAction::Patch, today).unwrap();
        assert_eq!(bumped, "1.2.4");
    }

    #[test]
    fn calver_auto_rolls_to_current_month_and_resets_micro() {
        let today = NaiveDate::from_ymd_opt(2026, 4, 6).unwrap();
        let bumped = VersionScheme::CalVerYearMonthMicro.bump("2026.03.8", BumpAction::Auto, today).unwrap();
        assert_eq!(bumped, "2026.04.0");
    }

    #[test]
    fn hybrid_minor_patch_rolls_year_forward() {
        let today = NaiveDate::from_ymd_opt(2026, 4, 6).unwrap();
        let bumped = VersionScheme::HybridYearMinorPatch.bump("2025.7.4", BumpAction::Patch, today).unwrap();
        assert_eq!(bumped, "2026.0.1");
    }
}