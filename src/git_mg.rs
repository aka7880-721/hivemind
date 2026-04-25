// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit License v1.2
//
// For details, see the LICENSE file in the repository root.
use std::{
    io::{self, Write},
    process::Command,
    time::Duration,
};

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Local};
use crossterm::{
    cursor::{MoveToColumn, MoveUp},
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute, queue,
    style::{Color, Print, ResetColor, SetForegroundColor},
    terminal::{Clear, ClearType, disable_raw_mode, enable_raw_mode, size},
};
use serde::Deserialize;

use crate::git::GitCancellation;

const PR_LIST_LIMIT: usize = 200;

pub(crate) fn run_merge(repo_root: &str, cancel: Option<GitCancellation>) -> Result<()> {
    let entries = fetch_open_pull_requests(repo_root, cancel.clone())?;
    let selected = prompt_pull_request_selection(&entries, cancel)?;
    merge_pull_request(repo_root, &selected)
}

fn fetch_open_pull_requests(
    repo_root: &str,
    cancel: Option<GitCancellation>,
) -> Result<Vec<PullRequestEntry>> {
    if cancel.as_ref().is_some_and(|cancel| cancel.is_cancelled()) {
        bail!("cancelled by user")
    }

    let limit = PR_LIST_LIMIT.to_string();
    let args = [
        "pr".to_string(),
        "list".to_string(),
        "--state".to_string(),
        "open".to_string(),
        "--limit".to_string(),
        limit,
        "--json".to_string(),
        "number,title,baseRefName,createdAt,author,mergeable,mergeStateStatus".to_string(),
    ];
    let output = Command::new("gh")
        .current_dir(repo_root)
        .args(args.iter().map(String::as_str))
        .output()
        .context("failed to execute gh pr list")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !stderr.is_empty() {
            bail!("gh pr list failed: {}", stderr)
        }
        if !stdout.is_empty() {
            bail!("gh pr list failed: {}", stdout)
        }
        bail!(
            "gh pr list failed with exit code {:?}",
            output.status.code()
        )
    }

    let listed = serde_json::from_slice::<Vec<GhPullRequest>>(&output.stdout)
        .context("failed to parse gh pr list output")?;
    let mut entries = listed
        .into_iter()
        .map(PullRequestEntry::from_gh)
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| {
        right
            .created_at_unix
            .cmp(&left.created_at_unix)
            .then_with(|| right.number.cmp(&left.number))
    });

    if entries.is_empty() {
        bail!("no open pull requests are available for this repository")
    }

    Ok(entries)
}

fn prompt_pull_request_selection(
    entries: &[PullRequestEntry],
    cancel: Option<GitCancellation>,
) -> Result<PullRequestEntry> {
    let mut selected = 0usize;
    let mut rendered_lines = 0usize;
    let mut message = None::<String>;
    let raw_mode = MergePickerRawModeGuard::enter()?;

    loop {
        render_pull_request_picker(entries, selected, message.as_deref(), &mut rendered_lines)?;

        if cancel.as_ref().is_some_and(|cancel| cancel.is_cancelled()) {
            drop(raw_mode);
            println!();
            bail!("cancelled by user")
        }

        if !event::poll(Duration::from_millis(100)).context("failed to poll merge picker")? {
            continue;
        }

        let Event::Key(key) = event::read().context("failed to read merge picker input")? else {
            continue;
        };
        if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
            continue;
        }

        match key.code {
            KeyCode::Esc => {
                drop(raw_mode);
                println!();
                bail!("cancelled by user")
            }
            KeyCode::Char('c' | 'C') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                drop(raw_mode);
                println!();
                bail!("cancelled by user")
            }
            KeyCode::Up | KeyCode::BackTab => {
                selected = selected.checked_sub(1).unwrap_or(entries.len() - 1);
                message = None;
            }
            KeyCode::Down | KeyCode::Tab => {
                selected = (selected + 1) % entries.len();
                message = None;
            }
            KeyCode::Char(character) => {
                if let Some(index) = digit_to_index(character) {
                    selected = index.min(entries.len().saturating_sub(1));
                    message = None;
                }
            }
            KeyCode::Enter => {
                let entry = entries[selected].clone();
                if !entry.is_mergeable() {
                    message = Some(format!(
                        "PR #{} cannot be merged yet. Select a row where Mergeable is True.",
                        entry.number
                    ));
                    continue;
                }

                drop(raw_mode);
                println!();
                return Ok(entry);
            }
            _ => {}
        }
    }
}

fn render_pull_request_picker(
    entries: &[PullRequestEntry],
    selected: usize,
    message: Option<&str>,
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
        .context("failed to redraw merge picker")?;
    }

    let (terminal_width, _) = size().context("failed to read terminal size")?;
    queue!(
        stdout,
        MoveToColumn(0),
        Print("Choose a pull request to merge:\r\n"),
        MoveToColumn(0),
        Print("Use Up/Down or Tab to select. Press Enter to merge. Esc exits.\r\n"),
        MoveToColumn(0),
        Print("|#|PR Name|Target|Created|Author|Status|Mergeable|\r\n")
    )
    .context("failed to render merge picker header")?;

    for (index, entry) in entries.iter().enumerate() {
        render_pull_request_row(
            &mut stdout,
            entry,
            index == selected,
            terminal_width as usize,
        )
        .context("failed to render merge picker row")?;
    }

    if let Some(message) = message {
        queue!(
            stdout,
            MoveToColumn(0),
            SetForegroundColor(Color::Red),
            Print(message),
            Print("\r\n"),
            ResetColor
        )
        .context("failed to render merge picker message")?;
    }

    stdout.flush().context("failed to flush merge picker")?;
    *rendered_lines = entries.len() + 3 + usize::from(message.is_some());
    Ok(())
}

fn render_pull_request_row(
    stdout: &mut io::Stdout,
    entry: &PullRequestEntry,
    selected: bool,
    terminal_width: usize,
) -> Result<()> {
    let row_color = if selected {
        Color::Yellow
    } else {
        Color::DarkGrey
    };
    let mergeable_color = if entry.is_mergeable() {
        Color::Green
    } else {
        Color::Red
    };
    let created_width = 16usize;
    let target_width = 14usize;
    let author_width = 14usize;
    let status_width = 12usize;
    let mergeable_width = 5usize;
    let title_width = terminal_width
        .saturating_sub(
            5 + created_width + target_width + author_width + status_width + mergeable_width + 14,
        )
        .max(12);

    queue!(stdout, MoveToColumn(0), SetForegroundColor(row_color))
        .context("failed to queue merge picker row color")?;
    queue!(
        stdout,
        Print("|"),
        Print(entry.number.to_string()),
        Print("|"),
        Print(fit_cell(&entry.title, title_width)),
        Print("|"),
        Print(fit_cell(&entry.target_branch, target_width)),
        Print("|"),
        Print(fit_cell(&entry.created_label, created_width)),
        Print("|"),
        Print(fit_cell(&entry.author, author_width)),
        Print("|"),
        Print(fit_cell(&entry.status, status_width)),
        Print("|")
    )
    .context("failed to queue merge picker row body")?;
    queue!(
        stdout,
        SetForegroundColor(mergeable_color),
        Print(fit_cell(entry.mergeable_label(), mergeable_width)),
        SetForegroundColor(row_color),
        Print("|\r\n"),
        ResetColor
    )
    .context("failed to queue merge picker row mergeable state")?;
    Ok(())
}

fn merge_pull_request(repo_root: &str, entry: &PullRequestEntry) -> Result<()> {
    let subject = build_merge_commit_subject(entry.number);
    let args = build_merge_args(entry.number, &subject);
    let output = Command::new("gh")
        .current_dir(repo_root)
        .args(args.iter().map(String::as_str))
        .output()
        .context("failed to execute gh pr merge")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !stderr.is_empty() {
            bail!("gh pr merge failed: {}", stderr)
        }
        if !stdout.is_empty() {
            bail!("gh pr merge failed: {}", stdout)
        }
        bail!(
            "gh pr merge failed with exit code {:?}",
            output.status.code()
        )
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    println!();
    if stdout.is_empty() {
        println!("Pull request #{} merged.", entry.number);
    } else {
        println!("{}", stdout);
    }

    Ok(())
}

fn build_merge_commit_subject(pr_number: u64) -> String {
    format!("Merge pull request #{} (via ComfyGit)", pr_number)
}

fn build_merge_args(pr_number: u64, subject: &str) -> Vec<String> {
    vec![
        "pr".to_string(),
        "merge".to_string(),
        pr_number.to_string(),
        "--merge".to_string(),
        "--subject".to_string(),
        subject.to_string(),
    ]
}

fn fit_cell(value: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }

    let char_count = value.chars().count();
    if char_count <= width {
        return value.to_string();
    }
    if width <= 3 {
        return value.chars().take(width).collect();
    }

    let mut truncated = value.chars().take(width - 3).collect::<String>();
    truncated.push_str("...");
    truncated
}

fn digit_to_index(character: char) -> Option<usize> {
    character
        .to_digit(10)
        .and_then(|digit| digit.checked_sub(1))
        .map(|digit| digit as usize)
}

struct MergePickerRawModeGuard;

impl MergePickerRawModeGuard {
    fn enter() -> Result<Self> {
        enable_raw_mode().context("failed to enable raw mode")?;
        Ok(Self)
    }
}

impl Drop for MergePickerRawModeGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PullRequestEntry {
    number: u64,
    title: String,
    target_branch: String,
    created_label: String,
    created_at_unix: i64,
    author: String,
    status: String,
    mergeable_state: String,
}

impl PullRequestEntry {
    fn from_gh(pr: GhPullRequest) -> Self {
        let author = pr
            .author
            .and_then(|author| {
                let login = author.login.trim().to_string();
                if login.is_empty() {
                    author.name.filter(|name| !name.trim().is_empty())
                } else {
                    Some(login)
                }
            })
            .unwrap_or_else(|| "-".to_string());
        let created_at_unix = parse_created_at_unix(&pr.created_at).unwrap_or_default();
        Self {
            number: pr.number,
            title: pr.title,
            target_branch: pr.base_ref_name,
            created_label: format_created_at_label(&pr.created_at),
            created_at_unix,
            author,
            status: pr.merge_state_status,
            mergeable_state: pr.mergeable,
        }
    }

    fn is_mergeable(&self) -> bool {
        self.mergeable_state.eq_ignore_ascii_case("MERGEABLE")
    }

    fn mergeable_label(&self) -> &'static str {
        if self.is_mergeable() { "True" } else { "False" }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhPullRequest {
    number: u64,
    title: String,
    base_ref_name: String,
    created_at: String,
    author: Option<GhPullRequestAuthor>,
    mergeable: String,
    merge_state_status: String,
}

#[derive(Deserialize)]
struct GhPullRequestAuthor {
    login: String,
    name: Option<String>,
}

fn parse_created_at_unix(value: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|timestamp| timestamp.timestamp())
}

fn format_created_at_label(value: &str) -> String {
    DateTime::parse_from_rfc3339(value)
        .map(|timestamp| {
            timestamp
                .with_timezone(&Local)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or_else(|_| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_merge_commit_subject_matches_requested_format() {
        assert_eq!(
            build_merge_commit_subject(42),
            "Merge pull request #42 (via ComfyGit)"
        );
    }

    #[test]
    fn build_merge_args_uses_merge_strategy_and_subject() {
        let args = build_merge_args(42, "Merge pull request #42 (via ComfyGit)");

        assert_eq!(
            args,
            vec![
                "pr",
                "merge",
                "42",
                "--merge",
                "--subject",
                "Merge pull request #42 (via ComfyGit)",
            ]
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>()
        );
    }

    #[test]
    fn pull_request_entry_treats_only_mergeable_state_as_true() {
        let entry = PullRequestEntry {
            number: 1,
            title: "PR".to_string(),
            target_branch: "main".to_string(),
            created_label: "2026-04-25 17:06".to_string(),
            created_at_unix: 0,
            author: "alice".to_string(),
            status: "CLEAN".to_string(),
            mergeable_state: "MERGEABLE".to_string(),
        };
        let not_ready = PullRequestEntry {
            mergeable_state: "CONFLICTING".to_string(),
            ..entry.clone()
        };

        assert!(entry.is_mergeable());
        assert_eq!(entry.mergeable_label(), "True");
        assert!(!not_ready.is_mergeable());
        assert_eq!(not_ready.mergeable_label(), "False");
    }

    #[test]
    fn fit_cell_truncates_long_values_with_ascii_ellipsis() {
        assert_eq!(fit_cell("very-long-title", 8), "very-...");
        assert_eq!(fit_cell("short", 8), "short");
    }

    #[test]
    fn pull_request_entries_sort_newest_first() {
        let mut entries = [
            PullRequestEntry {
                number: 1,
                title: "older".to_string(),
                target_branch: "main".to_string(),
                created_label: "2026-04-24 10:00".to_string(),
                created_at_unix: 100,
                author: "alice".to_string(),
                status: "CLEAN".to_string(),
                mergeable_state: "MERGEABLE".to_string(),
            },
            PullRequestEntry {
                number: 2,
                title: "newer".to_string(),
                target_branch: "main".to_string(),
                created_label: "2026-04-25 10:00".to_string(),
                created_at_unix: 200,
                author: "bob".to_string(),
                status: "CLEAN".to_string(),
                mergeable_state: "MERGEABLE".to_string(),
            },
        ];

        entries.sort_by(|left, right| {
            right
                .created_at_unix
                .cmp(&left.created_at_unix)
                .then_with(|| right.number.cmp(&left.number))
        });

        assert_eq!(entries[0].number, 2);
        assert_eq!(entries[1].number, 1);
    }

    #[test]
    fn pr_list_limit_matches_requested_capacity() {
        assert_eq!(PR_LIST_LIMIT, 200);
    }

    #[test]
    fn picker_accepts_digit_selection_indexes() {
        assert_eq!(digit_to_index('1'), Some(0));
        assert_eq!(digit_to_index('3'), Some(2));
        assert_eq!(digit_to_index('0'), None);
    }
}
