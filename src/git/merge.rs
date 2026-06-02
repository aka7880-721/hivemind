// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit License v1.2
//
// For details, see the LICENSE file in the repository root.
use std::{
    env, fs,
    io::{self, Write},
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::{
    forge::{ForgeKind, ForgePullRequest},
    git::{
        GitCancellation, current_branch_with_cancel, default_push_remote_name,
        ensure_clean_worktree_with_cancel, ensure_local_branch_published_and_in_sync_with_cancel,
        ensure_pull_request_mergeable, finish_after_pull_request_merge, run_git,
        run_git_checked_owned_with_cancel, split_output_lines, switch_to_existing_branch,
    },
};
use anyhow::{Context, Result, anyhow, bail};
use crossterm::{
    cursor::{MoveToColumn, MoveUp},
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute, queue,
    style::{Color, Print, ResetColor, SetForegroundColor},
    terminal::{Clear, ClearType, disable_raw_mode, enable_raw_mode, size},
};

const PR_LIST_LIMIT: usize = 200;
const FORGE_LINK_LABEL_GITHUB: &str = "<GitHub>";
const FORGE_LINK_LABEL_GITLAB: &str = "<GitLab>";
const VSCODE_LINK_LABEL: &str = "<VSCode>";
const CONFLICT_FIX_PREFIX: &str = "Fix: ";
const CONFLICT_LINKS_TOTAL_WIDTH: usize =
    CONFLICT_FIX_PREFIX.len() + FORGE_LINK_LABEL_GITHUB.len() + 1 + VSCODE_LINK_LABEL.len();
fn forge_link_label(forge: ForgeKind) -> &'static str {
    match forge {
        ForgeKind::GitHub => FORGE_LINK_LABEL_GITHUB,
        ForgeKind::GitLab => FORGE_LINK_LABEL_GITLAB,
    }
}

pub(crate) fn run_merge(
    repo_root: &str,
    forge: ForgeKind,
    cancel: Option<GitCancellation>,
) -> Result<()> {
    let current_branch = current_branch_with_cancel(repo_root, cancel.clone())?;
    if current_branch.starts_with("detached (") {
        bail!("cannot run cg merge from a detached HEAD");
    }

    ensure_clean_worktree_with_cancel(repo_root, "cg merge", cancel.clone())?;
    ensure_local_branch_published_and_in_sync_with_cancel(
        repo_root,
        &current_branch,
        "current branch",
        "cg merge",
        cancel.clone(),
    )?;

    let selected = prompt_pull_request_selection(repo_root, forge, cancel.clone())?;
    merge_pull_request(repo_root, forge, &selected, cancel)
}

pub(crate) fn run_merge_for_pull_request(
    repo_root: &str,
    forge: ForgeKind,
    pr_number: u64,
    cancel: Option<GitCancellation>,
) -> Result<()> {
    let current_branch = current_branch_with_cancel(repo_root, cancel.clone())?;
    if current_branch.starts_with("detached (") {
        bail!("cannot run cg merge from a detached HEAD");
    }

    ensure_clean_worktree_with_cancel(repo_root, "cg merge", cancel.clone())?;
    ensure_local_branch_published_and_in_sync_with_cancel(
        repo_root,
        &current_branch,
        "current branch",
        "cg merge",
        cancel.clone(),
    )?;

    let entries = fetch_open_pull_requests(repo_root, forge, cancel.clone())?;
    let selected = select_pull_request_by_number(&entries, pr_number)?;
    merge_pull_request(repo_root, forge, &selected, cancel)
}

fn fetch_open_pull_requests(
    repo_root: &str,
    forge: ForgeKind,
    cancel: Option<GitCancellation>,
) -> Result<Vec<PullRequestEntry>> {
    if cancel.as_ref().is_some_and(|cancel| cancel.is_cancelled()) {
        bail!("cancelled by user")
    }

    let mut entries = forge
        .list_open_pull_requests(repo_root, PR_LIST_LIMIT)?
        .into_iter()
        .map(PullRequestEntry::from_forge)
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| {
        right
            .created_at_unix
            .cmp(&left.created_at_unix)
            .then_with(|| right.number.cmp(&left.number))
    });

    if entries.is_empty() {
        let label = forge.pull_request_label();
        bail!("no open {label}s are available for this repository")
    }

    refresh_pull_request_entries(repo_root, forge, entries, cancel)
}

fn refresh_pull_request_entries(
    repo_root: &str,
    forge: ForgeKind,
    entries: Vec<PullRequestEntry>,
    cancel: Option<GitCancellation>,
) -> Result<Vec<PullRequestEntry>> {
    entries
        .into_iter()
        .map(|entry| {
            if cancel.as_ref().is_some_and(|cancel| cancel.is_cancelled()) {
                bail!("cancelled by user")
            }
            fetch_pull_request(repo_root, forge, entry.number)
        })
        .collect()
}

fn reload_pull_request_picker_entries(
    repo_root: &str,
    forge: ForgeKind,
    entries: &[PullRequestEntry],
    selected_number: u64,
    cancel: Option<GitCancellation>,
) -> Result<(Vec<PullRequestEntry>, usize)> {
    let mut reloaded_entries =
        refresh_pull_request_entries(repo_root, forge, entries.to_vec(), cancel)?;
    reloaded_entries.sort_by(|left, right| {
        right
            .created_at_unix
            .cmp(&left.created_at_unix)
            .then_with(|| right.number.cmp(&left.number))
    });
    let selected = reloaded_entries
        .iter()
        .position(|entry| entry.number == selected_number)
        .unwrap_or(0);
    Ok((reloaded_entries, selected))
}

fn fetch_pull_request(
    repo_root: &str,
    forge: ForgeKind,
    pr_number: u64,
) -> Result<PullRequestEntry> {
    Ok(PullRequestEntry::from_forge(
        forge.view_pull_request(repo_root, pr_number)?,
    ))
}

fn select_pull_request_by_number(
    entries: &[PullRequestEntry],
    pr_number: u64,
) -> Result<PullRequestEntry> {
    entries
        .iter()
        .find(|entry| entry.number == pr_number)
        .cloned()
        .ok_or_else(|| {
            anyhow!(
                "PR #{} is not currently listed as an open pull request for this repository",
                pr_number
            )
        })
}

fn prompt_pull_request_selection(
    repo_root: &str,
    forge: ForgeKind,
    cancel: Option<GitCancellation>,
) -> Result<PullRequestEntry> {
    let mut entries = fetch_open_pull_requests(repo_root, forge, cancel.clone())?;
    let mut prepared_vscode_workspace = None::<PreparedVscodeMergeWorkspace>;
    let mut selected = 0usize;
    let mut rendered_lines = 0usize;
    let mut message = None::<String>;
    let mut needs_render = true;
    let mut raw_mode = Some(MergePickerRawModeGuard::enter()?);

    loop {
        if needs_render {
            match ensure_selected_vscode_workspace(
                repo_root,
                &entries[selected],
                cancel.clone(),
                prepared_vscode_workspace.take(),
            ) {
                Ok(prepared) => {
                    prepared_vscode_workspace = prepared;
                }
                Err(error) => {
                    prepared_vscode_workspace = None;
                    if message.is_none() {
                        message = Some(format!("VS Code link unavailable: {}", error));
                    }
                }
            }
            render_pull_request_picker(
                forge,
                &entries,
                selected,
                message.as_deref(),
                prepared_vscode_workspace.as_ref(),
                &mut rendered_lines,
            )?;
            needs_render = false;
        }

        if cancel.as_ref().is_some_and(|cancel| cancel.is_cancelled()) {
            drop(raw_mode.take());
            println!();
            bail!("cancelled by user")
        }

        if !event::poll(Duration::from_millis(100)).context("failed to poll merge picker")? {
            continue;
        }

        match event::read().context("failed to read merge picker input")? {
            Event::Resize(_, _) => {
                needs_render = true;
            }
            Event::Key(key) if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) => {
                match key.code {
                    KeyCode::Esc => {
                        dismiss_prepared_vscode_workspace(prepared_vscode_workspace.take());
                        drop(raw_mode.take());
                        println!();
                        bail!("cancelled by user")
                    }
                    KeyCode::Char('c' | 'C') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        dismiss_prepared_vscode_workspace(prepared_vscode_workspace.take());
                        drop(raw_mode.take());
                        println!();
                        bail!("cancelled by user")
                    }
                    KeyCode::Up | KeyCode::BackTab => {
                        selected = selected.checked_sub(1).unwrap_or(entries.len() - 1);
                        prepared_vscode_workspace = None;
                        message = None;
                        needs_render = true;
                    }
                    KeyCode::Down | KeyCode::Tab => {
                        selected = (selected + 1) % entries.len();
                        prepared_vscode_workspace = None;
                        message = None;
                        needs_render = true;
                    }
                    KeyCode::Char('r' | 'R') => {
                        let selected_number = entries[selected].number;
                        let mut reload_note = None::<String>;

                        match reload_pull_request_picker_entries(
                            repo_root,
                            forge,
                            &entries,
                            selected_number,
                            cancel.clone(),
                        ) {
                            Ok((reloaded_entries, reloaded_selected)) => {
                                entries = reloaded_entries;
                                selected = reloaded_selected;
                            }
                            Err(error) => {
                                message = Some(format!("Reload failed: {}", error));
                                needs_render = true;
                                continue;
                            }
                        }

                        if entries[selected].is_mergeable() {
                            dismiss_prepared_vscode_workspace(prepared_vscode_workspace.take());
                            message = Some(format!(
                                "PR #{} is mergeable now. Press Enter to merge it.",
                                entries[selected].number
                            ));
                            needs_render = true;
                            continue;
                        }

                        if let Some(prepared) = prepared_vscode_workspace.clone() {
                            if prepared.pr_number != entries[selected].number {
                                dismiss_prepared_vscode_workspace(prepared_vscode_workspace.take());
                            } else {
                                match finalize_prepared_vscode_merge_workspace(
                                    &prepared,
                                    cancel.clone(),
                                ) {
                                    Ok(PreparedWorkspaceReloadOutcome::ConflictsRemaining(
                                        note,
                                    )) => {
                                        message = Some(note);
                                        needs_render = true;
                                        continue;
                                    }
                                    Ok(PreparedWorkspaceReloadOutcome::Pushed(note)) => {
                                        prepared_vscode_workspace = None;
                                        reload_note = Some(note);
                                    }
                                    Ok(PreparedWorkspaceReloadOutcome::ReadyToReload) => {
                                        prepared_vscode_workspace = None;
                                    }
                                    Err(error) => {
                                        message = Some(format!("Reload failed: {}", error));
                                        needs_render = true;
                                        continue;
                                    }
                                }
                            }
                        }

                        if reload_note.is_some() {
                            match reload_pull_request_picker_entries(
                                repo_root,
                                forge,
                                &entries,
                                selected_number,
                                cancel.clone(),
                            ) {
                                Ok((reloaded_entries, reloaded_selected)) => {
                                    entries = reloaded_entries;
                                    selected = reloaded_selected;
                                }
                                Err(error) => {
                                    message = Some(format!("Reload failed: {}", error));
                                    needs_render = true;
                                    continue;
                                }
                            }
                        }

                        if entries[selected].is_mergeable()
                            || prepared_vscode_workspace.as_ref().is_some_and(|prepared| {
                                prepared.pr_number != entries[selected].number
                            })
                        {
                            dismiss_prepared_vscode_workspace(prepared_vscode_workspace.take());
                        }

                        message = Some(
                            reload_note
                                .unwrap_or_else(|| "Pull request status reloaded.".to_string()),
                        );
                        needs_render = true;
                    }
                    KeyCode::Char('v' | 'V') => {
                        let entry = entries[selected].clone();
                        if entry.is_mergeable() {
                            message = Some(format!(
                                "PR #{} is mergeable now. Press Enter to merge it, or R to reload.",
                                entry.number
                            ));
                            needs_render = true;
                            continue;
                        }

                        clear_pull_request_picker(&mut rendered_lines)?;
                        drop(raw_mode.take());
                        println!();

                        let launch_result = match prepared_vscode_workspace.take() {
                            Some(prepared) if prepared.pr_number == entry.number => {
                                launch_prepared_vscode_merge_workspace(&prepared).map(|_| prepared)
                            }
                            _ => prepare_vscode_merge_workspace(repo_root, &entry, cancel.clone())
                                .and_then(|prepared| {
                                    launch_prepared_vscode_merge_workspace(&prepared)?;
                                    Ok(prepared)
                                }),
                        };

                        raw_mode = Some(MergePickerRawModeGuard::enter()?);
                        message = Some(match launch_result {
                            Ok(prepared) => {
                                prepared_vscode_workspace = Some(prepared.clone());
                                format!(
                                    "Opened VS Code merge workspace for PR #{} at {}. Resolve conflicts there, save, then return here and press R to commit, push, and refresh.",
                                    entry.number,
                                    prepared.worktree_root.display()
                                )
                            }
                            Err(error) => format!("VS Code merge workspace failed: {}", error),
                        });
                        needs_render = true;
                    }
                    KeyCode::Char(character) => {
                        if let Some(index) = digit_to_index(character) {
                            selected = index.min(entries.len().saturating_sub(1));
                            prepared_vscode_workspace = None;
                            message = None;
                            needs_render = true;
                        }
                    }
                    KeyCode::Enter => {
                        let entry = entries[selected].clone();
                        if !entry.is_mergeable() {
                            message = Some(format!(
                                "PR #{} cannot be merged yet. Press V to open a VS Code merge workspace, or R to reload after resolving it.",
                                entry.number
                            ));
                            needs_render = true;
                            continue;
                        }

                        dismiss_prepared_vscode_workspace(prepared_vscode_workspace.take());
                        drop(raw_mode.take());
                        println!();
                        return Ok(entry);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

fn render_pull_request_picker(
    forge: ForgeKind,
    entries: &[PullRequestEntry],
    selected: usize,
    message: Option<&str>,
    prepared_vscode_workspace: Option<&PreparedVscodeMergeWorkspace>,
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
    let layout = build_table_layout(entries, terminal_width as usize);
    queue!(
        stdout,
        MoveToColumn(0),
        Print("Choose a pull request to merge:\r\n"),
        MoveToColumn(0),
        Print(
            "Use Up/Down or Tab to select. Press Enter to merge, R to reload, V to open the VS Code merge tool for the selected conflicting PR. Esc exits.\r\n",
        ),
        MoveToColumn(0),
        Print(format_table_border(&layout)),
        Print("\r\n"),
        Print(format_table_header(&layout)),
        Print("\r\n"),
        Print(format_table_border(&layout)),
        Print("\r\n")
    )
    .context("failed to render merge picker header")?;

    for (index, entry) in entries.iter().enumerate() {
        render_pull_request_row(
            forge,
            &mut stdout,
            entry,
            index == selected,
            &layout,
            prepared_vscode_workspace.filter(|prepared| prepared.pr_number == entry.number),
        )
        .context("failed to render merge picker row")?;
    }

    queue!(
        stdout,
        MoveToColumn(0),
        Print(format_table_border(&layout)),
        Print("\r\n")
    )
    .context("failed to render merge picker footer")?;

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
    *rendered_lines = entries.len() + 6 + usize::from(message.is_some());
    Ok(())
}

fn render_pull_request_row(
    forge: ForgeKind,
    stdout: &mut io::Stdout,
    entry: &PullRequestEntry,
    selected: bool,
    layout: &PullRequestTableLayout,
    prepared_vscode_workspace: Option<&PreparedVscodeMergeWorkspace>,
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

    queue!(stdout, MoveToColumn(0), SetForegroundColor(row_color))
        .context("failed to queue merge picker row color")?;
    queue!(
        stdout,
        Print("| "),
        Print(pad_cell(&entry.number.to_string(), layout.number_width)),
        Print(" | "),
    )
    .context("failed to queue merge picker row prefix")?;
    render_pull_request_title_cell(
        forge,
        stdout,
        entry,
        row_color,
        layout.title_width,
        prepared_vscode_workspace,
    )?;
    queue!(
        stdout,
        Print(" | "),
        Print(pad_cell(
            &fit_cell(&entry.target_branch, layout.target_width),
            layout.target_width,
        )),
        Print(" | "),
        Print(pad_cell(
            &fit_cell(&entry.created_label, layout.created_width),
            layout.created_width,
        )),
        Print(" | "),
        Print(pad_cell(
            &fit_cell(&entry.author, layout.author_width),
            layout.author_width,
        )),
        Print(" | "),
        Print(pad_cell(
            &fit_cell(&entry.status, layout.status_width),
            layout.status_width,
        )),
        Print(" | ")
    )
    .context("failed to queue merge picker row body")?;
    queue!(
        stdout,
        SetForegroundColor(mergeable_color),
        Print(pad_cell(entry.mergeable_label(), layout.mergeable_width)),
        SetForegroundColor(row_color),
        Print(" |\r\n"),
        ResetColor
    )
    .context("failed to queue merge picker row mergeable state")?;
    Ok(())
}

fn render_pull_request_title_cell(
    forge: ForgeKind,
    stdout: &mut io::Stdout,
    entry: &PullRequestEntry,
    row_color: Color,
    width: usize,
    prepared_vscode_workspace: Option<&PreparedVscodeMergeWorkspace>,
) -> Result<()> {
    let Some(issue_url) = entry.issue_url.as_deref() else {
        queue!(
            stdout,
            Print(pad_cell(&fit_cell(&entry.title, width), width))
        )
        .context("failed to render merge picker plain title")?;
        return Ok(());
    };

    let label_width = CONFLICT_LINKS_TOTAL_WIDTH;
    if width <= label_width + 2 {
        queue!(
            stdout,
            Print(pad_cell(&fit_cell(&entry.title, width), width))
        )
        .context("failed to render merge picker narrow title")?;
        return Ok(());
    }

    let title_width = width - label_width - 2;
    let padded_title = pad_cell(&fit_cell(&entry.title, title_width), title_width);
    queue!(stdout, Print(padded_title), Print("  "))
        .context("failed to render merge picker title prefix")?;
    queue!(
        stdout,
        SetForegroundColor(Color::DarkGrey),
        Print(CONFLICT_FIX_PREFIX),
        SetForegroundColor(Color::Magenta),
        Print(format_terminal_hyperlink(
            issue_url,
            forge_link_label(forge)
        )),
        SetForegroundColor(Color::DarkGrey),
        Print(" "),
        SetForegroundColor(Color::Cyan),
        Print(
            prepared_vscode_workspace
                .map(|prepared| format_terminal_hyperlink(&prepared.open_uri, VSCODE_LINK_LABEL))
                .unwrap_or_else(|| VSCODE_LINK_LABEL.to_string())
        ),
        SetForegroundColor(row_color)
    )
    .context("failed to render merge picker conflict links")?;
    Ok(())
}

fn build_table_layout(
    entries: &[PullRequestEntry],
    terminal_width: usize,
) -> PullRequestTableLayout {
    let number_width = entries
        .iter()
        .map(|entry| entry.number.to_string().chars().count())
        .max()
        .unwrap_or(1)
        .max(1);
    let target_width = entries
        .iter()
        .map(|entry| entry.target_branch.chars().count())
        .max()
        .unwrap_or(6)
        .clamp(6, 14)
        .max("Target".len());
    let created_width = 16usize.max("Created".len());
    let author_width = entries
        .iter()
        .map(|entry| entry.author.chars().count())
        .max()
        .unwrap_or(6)
        .clamp(6, 14)
        .max("Author".len());
    let status_width = entries
        .iter()
        .map(|entry| entry.status.chars().count())
        .max()
        .unwrap_or(6)
        .clamp(6, 12)
        .max("Status".len());
    let mergeable_width = "Mergeable".len();
    let minimum_title_width = "PR Name".len().max(12);
    let non_title_width =
        number_width + target_width + created_width + author_width + status_width + mergeable_width;
    let separators_width = 22usize;
    let title_width = terminal_width
        .saturating_sub(non_title_width + separators_width)
        .max(minimum_title_width);

    PullRequestTableLayout {
        number_width,
        title_width,
        target_width,
        created_width,
        author_width,
        status_width,
        mergeable_width,
    }
}

fn format_table_border(layout: &PullRequestTableLayout) -> String {
    let mut line = String::from("+");
    for width in [
        layout.number_width,
        layout.title_width,
        layout.target_width,
        layout.created_width,
        layout.author_width,
        layout.status_width,
        layout.mergeable_width,
    ] {
        line.push_str(&"-".repeat(width + 2));
        line.push('+');
    }
    line
}

fn format_table_header(layout: &PullRequestTableLayout) -> String {
    format!(
        "| {} | {} | {} | {} | {} | {} | {} |",
        pad_cell("#", layout.number_width),
        pad_cell("PR Name", layout.title_width),
        pad_cell("Target", layout.target_width),
        pad_cell("Created", layout.created_width),
        pad_cell("Author", layout.author_width),
        pad_cell("Status", layout.status_width),
        pad_cell("Mergeable", layout.mergeable_width),
    )
}

fn merge_pull_request(
    repo_root: &str,
    forge: ForgeKind,
    entry: &PullRequestEntry,
    cancel: Option<GitCancellation>,
) -> Result<()> {
    let refreshed = fetch_pull_request(repo_root, forge, entry.number)?;
    ensure_pull_request_mergeable(repo_root, forge, refreshed.number)?;

    let policy = resolve_post_merge_source_branch(repo_root);
    let subject = build_merge_commit_subject(refreshed.number);
    let stdout = forge.merge_pull_request(
        repo_root,
        refreshed.number,
        &subject,
        &refreshed.source_branch,
        policy.delete_remote_on_merge(),
        policy.delete_local_after_merge(),
    )?;
    println!();
    if stdout.is_empty() {
        let label = forge.pull_request_label();
        println!("{} #{} merged.", capitalize_first(label), entry.number);
    } else {
        println!("{}", stdout);
    }

    // Sync the integration branch before deleting the source locally: `git branch -d`
    // requires the source to be merged into the current target, which is only true after
    // we fast-forward the target from the remote merge.
    finish_after_pull_request_merge(repo_root, &refreshed.target_branch, cancel.clone())?;

    if policy.delete_local_after_merge() {
        switch_off_branch_for_deletion(
            repo_root,
            &refreshed.source_branch,
            &refreshed.target_branch,
            cancel.clone(),
        )?;
        if matches!(forge, ForgeKind::GitLab) {
            delete_local_source_branch(repo_root, &refreshed.source_branch, cancel)?;
        }
    }
    cleanup_merge_workspaces_for_pr(repo_root, refreshed.number)?;

    if let Err(error) = crate::workflow::cli_sync::run_mirror_sync_after_comfygit_merge(repo_root) {
        eprintln!("Warning: mirror sync after merge failed: {error:#}. Run `cg sync` to retry.");
    }

    Ok(())
}

fn switch_off_branch_for_deletion(
    repo_root: &str,
    branch_to_delete: &str,
    fallback_branch: &str,
    cancel: Option<GitCancellation>,
) -> Result<()> {
    let current = current_branch_with_cancel(repo_root, cancel)?;
    if current != branch_to_delete {
        return Ok(());
    }

    switch_to_existing_branch(repo_root, fallback_branch).with_context(|| {
        format!(
            "failed to switch from '{branch_to_delete}' to '{fallback_branch}' before deleting the merged branch"
        )
    })?;
    Ok(())
}

fn local_branch_exists(repo_root: &str, branch: &str) -> Result<bool> {
    Ok(run_git(
        repo_root,
        &["show-ref", "--verify", &format!("refs/heads/{branch}")],
    )?
    .success)
}

fn resolve_post_merge_source_branch(repo_root: &str) -> crate::config::PostMergeSourceBranch {
    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from(repo_root));
    let config = crate::config::ConfigStore::locate()
        .and_then(|store| store.load())
        .map(|config| config.projects)
        .unwrap_or_default();
    crate::cli::post_merge_source_branch_for_repo(&config, repo_root, &cwd)
}

fn delete_local_source_branch(
    repo_root: &str,
    branch: &str,
    cancel: Option<GitCancellation>,
) -> Result<()> {
    if !local_branch_exists(repo_root, branch)? {
        return Ok(());
    }
    let delete_args = vec!["branch".to_string(), "-d".to_string(), branch.to_string()];
    if run_git_checked_owned_with_cancel(repo_root, delete_args, cancel.clone()).is_ok() {
        println!("Local branch '{branch}' deleted.");
        return Ok(());
    }
    run_git_checked_owned_with_cancel(
        repo_root,
        vec!["branch".to_string(), "-D".to_string(), branch.to_string()],
        cancel,
    )
    .with_context(|| format!("failed to delete local branch '{branch}' after merge"))?;
    eprintln!(
        "Local branch '{branch}' was not fully merged locally; deleted with -D after remote merge."
    );
    Ok(())
}

fn is_mergeable_pull_request_state(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "mergeable" | "can_be_merged" | "can be merged"
    )
}

fn capitalize_first(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn prepare_vscode_merge_workspace(
    repo_root: &str,
    entry: &PullRequestEntry,
    cancel: Option<GitCancellation>,
) -> Result<PreparedVscodeMergeWorkspace> {
    let remote_name = default_push_remote_name(repo_root)?;
    let source_refspec = format!(
        "+refs/heads/{}:refs/remotes/{}/{}",
        entry.source_branch, remote_name, entry.source_branch
    );
    let target_refspec = format!(
        "+refs/heads/{}:refs/remotes/{}/{}",
        entry.target_branch, remote_name, entry.target_branch
    );
    run_git_checked_owned_with_cancel(
        repo_root,
        vec![
            "fetch".to_string(),
            "--quiet".to_string(),
            remote_name.clone(),
            source_refspec,
            target_refspec,
        ],
        cancel.clone(),
    )?;

    let worktree_root = build_vscode_merge_workspace_root(entry.number);
    let worktree_root_string = worktree_root.to_string_lossy().to_string();
    let source_ref = format!("{}/{}", remote_name, entry.source_branch);
    let target_ref = format!("{}/{}", remote_name, entry.target_branch);
    run_git_checked_owned_with_cancel(
        repo_root,
        vec![
            "worktree".to_string(),
            "add".to_string(),
            "--detach".to_string(),
            worktree_root_string.clone(),
            source_ref,
        ],
        cancel.clone(),
    )?;

    let merge_output = Command::new("git")
        .current_dir(&worktree_root)
        .args(["merge", "--no-commit", "--no-ff", &target_ref])
        .output()
        .context("failed to prepare local merge conflict workspace")?;

    let conflicted_files = list_unmerged_files(&worktree_root_string, cancel)?;
    if conflicted_files.is_empty() {
        if merge_output.status.success() {
            let _ = run_git_checked_owned_with_cancel(
                &worktree_root_string,
                vec!["merge".to_string(), "--abort".to_string()],
                None,
            );
            bail!(
                "PR #{} no longer produces local merge conflicts. Press R to reload the picker.",
                entry.number
            )
        }

        let stderr = String::from_utf8_lossy(&merge_output.stderr)
            .trim()
            .to_string();
        let stdout = String::from_utf8_lossy(&merge_output.stdout)
            .trim()
            .to_string();
        if !stderr.is_empty() {
            bail!(stderr)
        }
        if !stdout.is_empty() {
            bail!(stdout)
        }
        bail!("failed to prepare merge conflict workspace")
    }

    let first_conflicted_file = worktree_root.join(&conflicted_files[0]);
    Ok(PreparedVscodeMergeWorkspace {
        pr_number: entry.number,
        repo_root: PathBuf::from(repo_root),
        remote_name,
        source_branch: entry.source_branch.clone(),
        target_branch: entry.target_branch.clone(),
        worktree_root,
        first_conflicted_file: first_conflicted_file.clone(),
        open_uri: build_vscode_file_uri(
            &first_conflicted_file,
            !is_running_inside_vscode_terminal(),
        ),
    })
}

fn build_vscode_merge_workspace_root(pr_number: u64) -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    env::temp_dir().join(format!("comfygit-merge-pr-{}-{}", pr_number, timestamp))
}

fn launch_prepared_vscode_merge_workspace(prepared: &PreparedVscodeMergeWorkspace) -> Result<()> {
    let vscode_executable = resolve_vscode_executable()?;
    let mut command = Command::new(vscode_executable);
    if launch_vscode_uri(&prepared.open_uri).is_ok() {
        return Ok(());
    }

    if is_running_inside_vscode_terminal() {
        command
            .arg("--reuse-window")
            .arg(&prepared.first_conflicted_file);
    } else {
        command
            .arg("-n")
            .arg(&prepared.worktree_root)
            .arg(&prepared.first_conflicted_file);
    }

    command
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("failed to launch VS Code")?;
    Ok(())
}

fn finalize_prepared_vscode_merge_workspace(
    prepared: &PreparedVscodeMergeWorkspace,
    cancel: Option<GitCancellation>,
) -> Result<PreparedWorkspaceReloadOutcome> {
    let worktree_root = prepared.worktree_root.to_string_lossy().to_string();
    let conflicted_files = list_unmerged_files(&worktree_root, cancel.clone())?;
    if !conflicted_files.is_empty() {
        return Ok(PreparedWorkspaceReloadOutcome::ConflictsRemaining(format!(
            "Conflicts still remain in {}. Resolve them in VS Code, save, then press R again.",
            prepared.worktree_root.display()
        )));
    }

    if !merge_in_progress(&prepared.worktree_root)? {
        cleanup_prepared_vscode_merge_workspace(prepared)?;
        return Ok(PreparedWorkspaceReloadOutcome::ReadyToReload);
    }

    run_git_checked_owned_with_cancel(
        &worktree_root,
        vec!["add".to_string(), "-A".to_string()],
        cancel.clone(),
    )?;
    run_git_checked_owned_with_cancel(
        &worktree_root,
        vec!["commit".to_string(), "--no-edit".to_string()],
        cancel.clone(),
    )?;
    run_git_checked_owned_with_cancel(
        &worktree_root,
        vec![
            "push".to_string(),
            prepared.remote_name.clone(),
            format!("HEAD:refs/heads/{}", prepared.source_branch),
        ],
        cancel,
    )?;
    cleanup_prepared_vscode_merge_workspace(prepared)?;

    Ok(PreparedWorkspaceReloadOutcome::Pushed(format!(
        "Resolved merge was committed and pushed to {}/{}. GitHub may need a moment; press R again if it still shows conflicting.",
        prepared.remote_name, prepared.source_branch
    )))
}

fn ensure_selected_vscode_workspace(
    repo_root: &str,
    entry: &PullRequestEntry,
    cancel: Option<GitCancellation>,
    existing: Option<PreparedVscodeMergeWorkspace>,
) -> Result<Option<PreparedVscodeMergeWorkspace>> {
    if entry.is_mergeable() || entry.issue_url.is_none() {
        return Ok(None);
    }

    if let Some(existing) = existing
        && existing.pr_number == entry.number
        && existing.first_conflicted_file.exists()
    {
        return Ok(Some(existing));
    }

    prepare_vscode_merge_workspace(repo_root, entry, cancel).map(Some)
}

fn launch_vscode_uri(uri: &str) -> Result<()> {
    let escaped_uri = uri.replace('\'', "''");
    Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            &format!("Start-Process '{}'", escaped_uri),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("failed to launch VS Code URI")?;
    Ok(())
}

fn dismiss_prepared_vscode_workspace(prepared: Option<PreparedVscodeMergeWorkspace>) {
    if let Some(prepared) = prepared {
        let _ = cleanup_prepared_vscode_merge_workspace(&prepared);
    }
}

fn cleanup_merge_workspaces_for_pr(repo_root: &str, pr_number: u64) -> Result<()> {
    let prefix = format!("comfygit-merge-pr-{pr_number}-");
    let temp = env::temp_dir();
    let Ok(read_dir) = fs::read_dir(&temp) else {
        return Ok(());
    };

    for entry in read_dir.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name() else {
            continue;
        };
        if !name.to_string_lossy().starts_with(&prefix) {
            continue;
        }

        let path_string = path.to_string_lossy().to_string();
        let _ = run_git_checked_owned_with_cancel(
            repo_root,
            vec![
                "worktree".to_string(),
                "remove".to_string(),
                "--force".to_string(),
                path_string.clone(),
            ],
            None,
        );
        if path.exists() {
            let _ = fs::remove_dir_all(&path);
        }
    }

    Ok(())
}

fn cleanup_prepared_vscode_merge_workspace(prepared: &PreparedVscodeMergeWorkspace) -> Result<()> {
    let worktree_root = prepared.worktree_root.to_string_lossy().to_string();
    let remove_result = run_git_checked_owned_with_cancel(
        &prepared.repo_root.to_string_lossy(),
        vec![
            "worktree".to_string(),
            "remove".to_string(),
            "--force".to_string(),
            worktree_root.clone(),
        ],
        None,
    );
    if remove_result.is_ok() {
        return Ok(());
    }

    if prepared.worktree_root.exists() {
        fs::remove_dir_all(&prepared.worktree_root).with_context(|| {
            format!(
                "failed to remove temporary merge workspace {}",
                prepared.worktree_root.display()
            )
        })?;
    }
    Ok(())
}

fn merge_in_progress(repo_root: &std::path::Path) -> Result<bool> {
    let status = Command::new("git")
        .current_dir(repo_root)
        .args(["rev-parse", "-q", "--verify", "MERGE_HEAD"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("failed to inspect merge state")?;
    Ok(status.success())
}

fn is_running_inside_vscode_terminal() -> bool {
    env::var("TERM_PROGRAM").is_ok_and(|value| value.eq_ignore_ascii_case("vscode"))
        || env::var_os("VSCODE_GIT_IPC_HANDLE").is_some()
}

fn resolve_vscode_executable() -> Result<PathBuf> {
    let code_command = PathBuf::from("code");
    if Command::new(&code_command)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
    {
        return Ok(code_command);
    }

    let local_app_data = env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("could not locate LOCALAPPDATA to find Code.exe"))?;
    let fallback = local_app_data
        .join("Programs")
        .join("Microsoft VS Code")
        .join("Code.exe");
    if fallback.is_file() {
        Ok(fallback)
    } else {
        bail!("could not find the VS Code CLI or Code.exe")
    }
}

fn build_vscode_file_uri(path: &std::path::Path, open_in_new_window: bool) -> String {
    let encoded_path = encode_vscode_path(path);
    if open_in_new_window {
        format!("vscode://file/{}?windowId=_blank", encoded_path)
    } else {
        format!("vscode://file/{}", encoded_path)
    }
}

fn encode_vscode_path(path: &std::path::Path) -> String {
    let normalized = path.to_string_lossy().replace('\\', "/");
    let mut encoded = String::with_capacity(normalized.len());
    for byte in normalized.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' | b':' => {
                encoded.push(*byte as char)
            }
            _ => encoded.push_str(&format!("%{:02X}", byte)),
        }
    }
    encoded
}

fn list_unmerged_files(repo_root: &str, cancel: Option<GitCancellation>) -> Result<Vec<String>> {
    let output = run_git_checked_owned_with_cancel(
        repo_root,
        vec![
            "diff".to_string(),
            "--name-only".to_string(),
            "--diff-filter=U".to_string(),
        ],
        cancel,
    )?;
    Ok(split_output_lines(&output))
}

fn build_merge_commit_subject(pr_number: u64) -> String {
    format!("Merge pull request #{} (via ComfyGit)", pr_number)
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

fn pad_cell(value: &str, width: usize) -> String {
    let value_width = value.chars().count();
    if value_width >= width {
        return value.to_string();
    }

    let mut padded = String::with_capacity(width);
    padded.push_str(value);
    padded.push_str(&" ".repeat(width - value_width));
    padded
}

fn format_terminal_hyperlink(url: &str, label: &str) -> String {
    format!("\x1b]8;;{}\x1b\\{}\x1b]8;;\x1b\\", url, label)
}

fn clear_pull_request_picker(rendered_lines: &mut usize) -> Result<()> {
    if *rendered_lines == 0 {
        return Ok(());
    }

    let mut stdout = io::stdout();
    execute!(
        stdout,
        MoveUp(*rendered_lines as u16),
        MoveToColumn(0),
        Clear(ClearType::FromCursorDown)
    )
    .context("failed to clear merge picker")?;
    *rendered_lines = 0;
    Ok(())
}

fn digit_to_index(character: char) -> Option<usize> {
    character
        .to_digit(10)
        .and_then(|digit| digit.checked_sub(1))
        .map(|digit| digit as usize)
}

struct MergePickerRawModeGuard;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PullRequestTableLayout {
    number_width: usize,
    title_width: usize,
    target_width: usize,
    created_width: usize,
    author_width: usize,
    status_width: usize,
    mergeable_width: usize,
}

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
    source_branch: String,
    created_label: String,
    created_at_unix: i64,
    author: String,
    status: String,
    mergeable_state: String,
    issue_url: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PreparedVscodeMergeWorkspace {
    pr_number: u64,
    repo_root: PathBuf,
    remote_name: String,
    source_branch: String,
    target_branch: String,
    worktree_root: PathBuf,
    first_conflicted_file: PathBuf,
    open_uri: String,
}

enum PreparedWorkspaceReloadOutcome {
    ConflictsRemaining(String),
    Pushed(String),
    ReadyToReload,
}

impl PullRequestEntry {
    fn from_forge(pr: ForgePullRequest) -> Self {
        let created_label = pr.created_label();
        let created_at_unix = pr.created_at_unix();
        Self {
            number: pr.number,
            title: pr.title,
            target_branch: pr.target_branch,
            source_branch: pr.source_branch,
            created_label,
            created_at_unix,
            author: pr.author,
            status: pr.status,
            mergeable_state: pr.mergeable_state,
            issue_url: pr.issue_url,
        }
    }

    fn is_mergeable(&self) -> bool {
        is_mergeable_pull_request_state(&self.mergeable_state)
            || is_mergeable_pull_request_state(&self.status)
    }

    fn mergeable_label(&self) -> &'static str {
        if self.is_mergeable() { "True" } else { "False" }
    }
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
    fn select_pull_request_by_number_returns_matching_open_entry() {
        let entries = vec![
            PullRequestEntry {
                number: 41,
                title: "Older PR".to_string(),
                target_branch: "main".to_string(),
                source_branch: "feature/older".to_string(),
                created_label: "2026-04-24 10:00".to_string(),
                created_at_unix: 100,
                author: "alice".to_string(),
                status: "CLEAN".to_string(),
                mergeable_state: "MERGEABLE".to_string(),
                issue_url: None,
            },
            PullRequestEntry {
                number: 67,
                title: "Target PR".to_string(),
                target_branch: "main".to_string(),
                source_branch: "feature/target".to_string(),
                created_label: "2026-04-25 10:00".to_string(),
                created_at_unix: 200,
                author: "bob".to_string(),
                status: "CLEAN".to_string(),
                mergeable_state: "MERGEABLE".to_string(),
                issue_url: None,
            },
        ];

        let selected = select_pull_request_by_number(&entries, 67).expect("select matching PR");

        assert_eq!(selected.number, 67);
        assert_eq!(selected.title, "Target PR");
    }

    #[test]
    fn select_pull_request_by_number_rejects_missing_entry() {
        let entries = vec![PullRequestEntry {
            number: 41,
            title: "Older PR".to_string(),
            target_branch: "main".to_string(),
            source_branch: "feature/older".to_string(),
            created_label: "2026-04-24 10:00".to_string(),
            created_at_unix: 100,
            author: "alice".to_string(),
            status: "CLEAN".to_string(),
            mergeable_state: "MERGEABLE".to_string(),
            issue_url: None,
        }];

        let error =
            select_pull_request_by_number(&entries, 67).expect_err("missing PR should fail");

        assert!(error.to_string().contains("PR #67"));
        assert!(error.to_string().contains("open pull request"));
    }

    #[test]
    fn pull_request_entry_treats_gitlab_mergeable_status_as_true() {
        let entry = PullRequestEntry {
            number: 3,
            title: "feature".to_string(),
            target_branch: "main".to_string(),
            source_branch: "feature/x".to_string(),
            created_label: "2026-04-25 17:06".to_string(),
            created_at_unix: 0,
            author: "alice".to_string(),
            status: "mergeable".to_string(),
            mergeable_state: "mergeable".to_string(),
            issue_url: None,
        };

        assert!(entry.is_mergeable());
    }

    #[test]
    fn pull_request_entry_treats_only_mergeable_state_as_true() {
        let entry = PullRequestEntry {
            number: 1,
            title: "PR".to_string(),
            target_branch: "main".to_string(),
            source_branch: "feature/pr".to_string(),
            created_label: "2026-04-25 17:06".to_string(),
            created_at_unix: 0,
            author: "alice".to_string(),
            status: "CLEAN".to_string(),
            mergeable_state: "MERGEABLE".to_string(),
            issue_url: None,
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
                source_branch: "feature/older".to_string(),
                created_label: "2026-04-24 10:00".to_string(),
                created_at_unix: 100,
                author: "alice".to_string(),
                status: "CLEAN".to_string(),
                mergeable_state: "MERGEABLE".to_string(),
                issue_url: None,
            },
            PullRequestEntry {
                number: 2,
                title: "newer".to_string(),
                target_branch: "main".to_string(),
                source_branch: "feature/newer".to_string(),
                created_label: "2026-04-25 10:00".to_string(),
                created_at_unix: 200,
                author: "bob".to_string(),
                status: "CLEAN".to_string(),
                mergeable_state: "MERGEABLE".to_string(),
                issue_url: None,
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

    #[test]
    fn format_table_header_uses_ascii_grid_and_aligned_columns() {
        let layout = PullRequestTableLayout {
            number_width: 2,
            title_width: 12,
            target_width: 8,
            created_width: 16,
            author_width: 10,
            status_width: 8,
            mergeable_width: 9,
        };

        assert_eq!(
            format_table_header(&layout),
            "| #  | PR Name      | Target   | Created          | Author     | Status   | Mergeable |"
        );
        assert_eq!(
            format_table_border(&layout),
            "+----+--------------+----------+------------------+------------+----------+-----------+"
        );
    }

    #[test]
    fn build_table_layout_keeps_mergeable_column_wide_enough_for_header() {
        let entries = [PullRequestEntry {
            number: 50,
            title: "demo".to_string(),
            target_branch: "0.15.x".to_string(),
            source_branch: "feature/demo".to_string(),
            created_label: "2026-04-25 17:06".to_string(),
            created_at_unix: 1,
            author: "comfy-home".to_string(),
            status: "CLEAN".to_string(),
            mergeable_state: "MERGEABLE".to_string(),
            issue_url: None,
        }];

        let layout = build_table_layout(&entries, 100);

        assert_eq!(layout.mergeable_width, "Mergeable".len());
        assert!(layout.title_width >= 12);
    }

    #[test]
    fn render_pull_request_title_cell_reserves_space_for_conflict_links() {
        let entry = PullRequestEntry {
            number: 12,
            title: "0.4.x (via ComfyGit)".to_string(),
            target_branch: "main".to_string(),
            source_branch: "0.4.x".to_string(),
            created_label: "2026-04-27 08:01".to_string(),
            created_at_unix: 1,
            author: "comfy-home".to_string(),
            status: "DIRTY".to_string(),
            mergeable_state: "CONFLICTING".to_string(),
            issue_url: Some(
                "https://github.com/comfy-home/ComfyGit-test-project/pull/12/conflicts".to_string(),
            ),
        };

        let title_width = 40usize;
        let label_width = CONFLICT_LINKS_TOTAL_WIDTH;
        let title_visible = fit_cell(&entry.title, title_width - label_width - 2);
        let rendered_width = pad_cell(&title_visible, title_width - label_width - 2)
            .chars()
            .count()
            + 2
            + label_width;

        assert_eq!(rendered_width, title_width);
        assert!(
            format_terminal_hyperlink(
                entry.issue_url.as_deref().unwrap_or_default(),
                FORGE_LINK_LABEL_GITHUB
            )
            .contains(FORGE_LINK_LABEL_GITHUB)
        );
    }

    #[test]
    fn build_vscode_merge_workspace_root_includes_pr_number() {
        let root = build_vscode_merge_workspace_root(12);
        let root = root.to_string_lossy();

        assert!(root.contains("comfygit-merge-pr-12-"));
    }

    #[test]
    fn build_vscode_file_uri_encodes_spaces_for_new_window_launches() {
        let uri =
            build_vscode_file_uri(std::path::Path::new("C:/tmp/merge space/Cargo.toml"), true);

        assert_eq!(
            uri,
            "vscode://file/C:/tmp/merge%20space/Cargo.toml?windowId=_blank"
        );
    }
}
