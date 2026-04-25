// Copyright © 2026 ComfyHome™
// All rights reserved.
// Licensed under the ComfyGit License v1.2
//
// For details, see the LICENSE file in the repository root.
use std::{
    collections::HashSet,
    io::{self, Write},
    path::Path,
    process::Command,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use crossterm::{
    cursor::{MoveTo, MoveToColumn},
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute, queue,
    style::Print,
    terminal::{Clear, ClearType, disable_raw_mode, enable_raw_mode},
};

use crate::{
    changelog::{pr_changelog_gen, write_temp_changelog_markdown},
    git::{
        GitCancellation, current_branch_with_cancel, resolve_main_branch_name,
        run_git_checked_with_cancel, split_output_lines,
    },
};

const PR_PREVIEW_SECONDS: u64 = 30;
const ANSI_YELLOW: &str = "\x1b[33m";
const ANSI_RESET: &str = "\x1b[0m";

pub(crate) fn run_pr(
    repo_root: &str,
    force_main: bool,
    custom_main_branch: Option<&str>,
    cancel: Option<GitCancellation>,
) -> Result<()> {
    let current_branch = current_branch_with_cancel(repo_root, cancel.clone())?;
    if current_branch.starts_with("detached (") {
        bail!("cannot create a PR from a detached HEAD");
    }

    let target_branch = if force_main {
        resolve_main_branch_name(repo_root, custom_main_branch)?
    } else {
        resolve_parent_branch_name_with_cancel(
            repo_root,
            &current_branch,
            custom_main_branch,
            cancel.clone(),
        )?
    };

    if current_branch.eq_ignore_ascii_case(&target_branch) {
        bail!(
            "current branch '{}' is the same as the target branch '{}'",
            current_branch,
            target_branch
        );
    }

    let title = format!("{} (via ComfyGit)", current_branch);
    let body = build_pr_body(repo_root, &target_branch, &current_branch, cancel.clone())?;
    let body = preview_pr(
        &target_branch,
        &current_branch,
        &title,
        &body,
        cancel.clone(),
    )?;
    let body_path = write_temp_changelog_markdown(repo_root, &body)?;
    let args = build_pr_create_args(&target_branch, &current_branch, &title, &body_path);
    create_pr(repo_root, &args)?;
    Ok(())
}

fn build_pr_body(
    repo_root: &str,
    target_branch: &str,
    current_branch: &str,
    cancel: Option<GitCancellation>,
) -> Result<String> {
    let range_spec = format!("{}..{}", target_branch, current_branch);
    let output = run_git_checked_with_cancel(
        repo_root,
        &["log", "--pretty=format:%h %s", &range_spec],
        cancel,
    )?;

    let lines = split_output_lines(&output)
        .into_iter()
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>();

    if lines.is_empty() {
        return Ok(format!(
            "No commits were found between `{}` and `{}`.\n\nIf this branch was just created, ensure it has commits before opening a pull request.",
            target_branch, current_branch
        ));
    }

    Ok(pr_changelog_gen(current_branch, &lines).markdown)
}

fn preview_pr(
    target_branch: &str,
    current_branch: &str,
    title: &str,
    body: &str,
    cancel: Option<GitCancellation>,
) -> Result<String> {
    let mut body = body.to_string();

    loop {
        let raw_mode = TerminalRawModeGuard::enter()?;
        render_preview_screen(target_branch, current_branch, title, &body)?;

        match wait_for_preview_action(cancel.clone(), PR_PREVIEW_SECONDS)? {
            PreviewAction::Create => return Ok(body),
            PreviewAction::Edit => {
                drop(raw_mode);
                body = edit_pr_body(&body, cancel.clone())?;
            }
            PreviewAction::Cancel => bail!("cancelled by user"),
        }
    }
}

fn wait_for_preview_action(cancel: Option<GitCancellation>, seconds: u64) -> Result<PreviewAction> {
    let deadline = Instant::now() + Duration::from_secs(seconds);
    while Instant::now() < deadline {
        if cancel.as_ref().is_some_and(|cancel| cancel.is_cancelled()) {
            bail!("cancelled by user");
        }

        if event::poll(Duration::from_millis(100)).context("failed to poll preview input")? {
            let Event::Key(key) = event::read().context("failed to read preview input")? else {
                continue;
            };

            match classify_preview_key(key) {
                Some(PreviewAction::Create) => return Ok(PreviewAction::Create),
                Some(PreviewAction::Edit) => return Ok(PreviewAction::Edit),
                Some(PreviewAction::Cancel) => bail!("cancelled by user"),
                None => {}
            }
        }
    }

    Ok(PreviewAction::Create)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PreviewAction {
    Create,
    Edit,
    Cancel,
}

fn classify_preview_key(key: KeyEvent) -> Option<PreviewAction> {
    if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
        return None;
    }

    match key.code {
        KeyCode::Enter => Some(PreviewAction::Create),
        KeyCode::Char('e' | 'E') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(PreviewAction::Edit)
        }
        KeyCode::Char('c' | 'C') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(PreviewAction::Cancel)
        }
        _ => None,
    }
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

fn render_preview_screen(
    target_branch: &str,
    current_branch: &str,
    title: &str,
    body: &str,
) -> Result<()> {
    let mut stdout = io::stdout();
    execute!(stdout, MoveTo(0, 0), Clear(ClearType::All)).context("failed to render PR preview")?;

    queue!(
        stdout,
        MoveToColumn(0),
        Print(format!(
            "{}Dry-run PR preview{}\r\n  {}Target branch:{} {}\r\n  {}Source branch:{} {}\r\n  {}Title:{} {}\r\n\r\n",
            ANSI_YELLOW,
            ANSI_RESET,
            ANSI_YELLOW,
            ANSI_RESET,
            target_branch,
            ANSI_YELLOW,
            ANSI_RESET,
            current_branch,
            ANSI_YELLOW,
            ANSI_RESET,
            title
        ))
    )
    .context("failed to queue PR preview header")?;

    for line in body.lines() {
        queue!(stdout, MoveToColumn(0), Print(line), Print("\r\n"))
            .context("failed to queue PR preview body")?;
    }

    queue!(
        stdout,
        MoveToColumn(0),
        Print("\r\n"),
        Print(format!(
            "{}Preview ends in {} seconds. Press Enter to create now, E to edit, or Ctrl+C to abort.{}\r\n",
            ANSI_YELLOW, PR_PREVIEW_SECONDS, ANSI_RESET
        ))
    )
    .context("failed to queue PR preview footer")?;

    stdout.flush().context("failed to flush preview output")?;
    Ok(())
}

fn edit_pr_body(body: &str, cancel: Option<GitCancellation>) -> Result<String> {
    let raw_mode = TerminalRawModeGuard::enter()?;
    let mut editor = PrBodyEditor::new(body);

    loop {
        render_editor_screen(&editor)?;

        if cancel.as_ref().is_some_and(|cancel| cancel.is_cancelled()) {
            bail!("cancelled by user");
        }

        let Event::Key(key) = event::read().context("failed to read PR body edit input")? else {
            continue;
        };
        if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
            continue;
        }

        match key.code {
            KeyCode::Char('e' | 'E') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                drop(raw_mode);
                return Ok(editor.into_string());
            }
            KeyCode::Char('c' | 'C') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                bail!("cancelled by user");
            }
            _ => editor.handle_key(key),
        }
    }
}

fn render_editor_screen(editor: &PrBodyEditor) -> Result<()> {
    let mut stdout = io::stdout();
    execute!(stdout, MoveTo(0, 0), Clear(ClearType::All))
        .context("failed to render PR body editor")?;

    queue!(
        stdout,
        MoveToColumn(0),
        Print(format!(
            "{}Edit PR body{}\r\nPress Ctrl+E to finish editing and return to preview. Press Ctrl+C to abort.\r\n\r\n",
            ANSI_YELLOW, ANSI_RESET
        ))
    )
    .context("failed to queue PR body editor header")?;

    for line in &editor.lines {
        queue!(stdout, MoveToColumn(0), Print(line), Print("\r\n"))
            .context("failed to queue PR body editor content")?;
    }

    let cursor_x = editor.cursor_col.min(u16::MAX as usize) as u16;
    let cursor_y = (editor.cursor_row + 3).min(u16::MAX as usize) as u16;
    execute!(stdout, MoveTo(cursor_x, cursor_y)).context("failed to position PR editor cursor")?;
    stdout.flush().context("failed to flush PR body editor")?;
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PrBodyEditor {
    lines: Vec<String>,
    cursor_row: usize,
    cursor_col: usize,
}

impl PrBodyEditor {
    fn new(body: &str) -> Self {
        let mut lines = body.split('\n').map(ToOwned::to_owned).collect::<Vec<_>>();
        if lines.is_empty() {
            lines.push(String::new());
        }

        let cursor_row = lines.len().saturating_sub(1);
        let cursor_col = line_char_len(&lines[cursor_row]);
        Self {
            lines,
            cursor_row,
            cursor_col,
        }
    }

    fn into_string(self) -> String {
        self.lines.join("\n")
    }

    fn handle_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char(character)
                if !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT) =>
            {
                self.insert_char(character);
            }
            KeyCode::Enter => self.insert_newline(),
            KeyCode::Tab => self.insert_str("  "),
            KeyCode::Backspace => self.backspace(),
            KeyCode::Delete => self.delete(),
            KeyCode::Left => self.move_left(),
            KeyCode::Right => self.move_right(),
            KeyCode::Up => self.move_up(),
            KeyCode::Down => self.move_down(),
            KeyCode::Home => self.cursor_col = 0,
            KeyCode::End => self.cursor_col = line_char_len(&self.lines[self.cursor_row]),
            _ => {}
        }
    }

    fn insert_char(&mut self, character: char) {
        let line = &mut self.lines[self.cursor_row];
        let index = char_to_byte_index(line, self.cursor_col);
        line.insert(index, character);
        self.cursor_col += 1;
    }

    fn insert_str(&mut self, value: &str) {
        for character in value.chars() {
            self.insert_char(character);
        }
    }

    fn insert_newline(&mut self) {
        let line = &mut self.lines[self.cursor_row];
        let index = char_to_byte_index(line, self.cursor_col);
        let trailing = line.split_off(index);
        self.cursor_row += 1;
        self.cursor_col = 0;
        self.lines.insert(self.cursor_row, trailing);
    }

    fn backspace(&mut self) {
        if self.cursor_col > 0 {
            let line = &mut self.lines[self.cursor_row];
            let end = char_to_byte_index(line, self.cursor_col);
            let start = char_to_byte_index(line, self.cursor_col - 1);
            line.replace_range(start..end, "");
            self.cursor_col -= 1;
            return;
        }

        if self.cursor_row == 0 {
            return;
        }

        let current = self.lines.remove(self.cursor_row);
        self.cursor_row -= 1;
        self.cursor_col = line_char_len(&self.lines[self.cursor_row]);
        self.lines[self.cursor_row].push_str(&current);
    }

    fn delete(&mut self) {
        let line_len = line_char_len(&self.lines[self.cursor_row]);
        if self.cursor_col < line_len {
            let line = &mut self.lines[self.cursor_row];
            let start = char_to_byte_index(line, self.cursor_col);
            let end = char_to_byte_index(line, self.cursor_col + 1);
            line.replace_range(start..end, "");
            return;
        }

        if self.cursor_row + 1 >= self.lines.len() {
            return;
        }

        let next = self.lines.remove(self.cursor_row + 1);
        self.lines[self.cursor_row].push_str(&next);
    }

    fn move_left(&mut self) {
        if self.cursor_col > 0 {
            self.cursor_col -= 1;
        } else if self.cursor_row > 0 {
            self.cursor_row -= 1;
            self.cursor_col = line_char_len(&self.lines[self.cursor_row]);
        }
    }

    fn move_right(&mut self) {
        let line_len = line_char_len(&self.lines[self.cursor_row]);
        if self.cursor_col < line_len {
            self.cursor_col += 1;
        } else if self.cursor_row + 1 < self.lines.len() {
            self.cursor_row += 1;
            self.cursor_col = 0;
        }
    }

    fn move_up(&mut self) {
        if self.cursor_row == 0 {
            return;
        }

        self.cursor_row -= 1;
        self.cursor_col = self
            .cursor_col
            .min(line_char_len(&self.lines[self.cursor_row]));
    }

    fn move_down(&mut self) {
        if self.cursor_row + 1 >= self.lines.len() {
            return;
        }

        self.cursor_row += 1;
        self.cursor_col = self
            .cursor_col
            .min(line_char_len(&self.lines[self.cursor_row]));
    }
}

fn char_to_byte_index(value: &str, char_index: usize) -> usize {
    value
        .char_indices()
        .nth(char_index)
        .map(|(index, _)| index)
        .unwrap_or(value.len())
}

fn line_char_len(value: &str) -> usize {
    value.chars().count()
}

fn build_pr_create_args(
    target_branch: &str,
    current_branch: &str,
    title: &str,
    body_path: &Path,
) -> Vec<String> {
    vec![
        "pr".to_string(),
        "create".to_string(),
        "--base".to_string(),
        target_branch.to_string(),
        "--head".to_string(),
        current_branch.to_string(),
        "--title".to_string(),
        title.to_string(),
        "--body-file".to_string(),
        body_path.display().to_string(),
    ]
}

fn create_pr(repo_root: &str, args: &[String]) -> Result<()> {
    let output = Command::new("gh")
        .current_dir(repo_root)
        .args(args)
        .output()
        .context("failed to execute gh pr create")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !stderr.is_empty() {
            bail!("gh pr create failed: {}", stderr);
        }
        if !stdout.is_empty() {
            bail!("gh pr create failed: {}", stdout);
        }
        bail!(
            "gh pr create failed with exit code {:?}",
            output.status.code()
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    println!();
    if stdout.is_empty() {
        println!("Pull request created.");
    } else {
        println!("{}", stdout);
    }

    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct BranchRef {
    name: String,
    refname: String,
    object_id: String,
    root_distance: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct BranchLineage {
    root: BranchRef,
    path: Vec<BranchRef>,
}

fn resolve_parent_branch_name_with_cancel(
    repo_root: &str,
    current_branch: &str,
    custom_main_branch: Option<&str>,
    cancel: Option<GitCancellation>,
) -> Result<String> {
    let lineage =
        load_branch_lineage_with_cancel(repo_root, current_branch, custom_main_branch, cancel)?
            .ok_or_else(|| anyhow::anyhow!("no local branches are available in this repository"))?;
    if lineage.root.name.eq_ignore_ascii_case(current_branch) {
        bail!("current branch is already the main branch");
    }

    let current_index = lineage
        .path
        .iter()
        .position(|branch| branch.name.eq_ignore_ascii_case(current_branch))
        .ok_or_else(|| anyhow::anyhow!("current branch is not part of the current branch tree"))?;

    let target = if current_index == 0 {
        lineage.root.name
    } else {
        lineage.path[current_index - 1].name.clone()
    };
    Ok(target)
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

#[derive(Clone, Debug, PartialEq, Eq)]
struct BranchTreeData {
    root: BranchRef,
    family: Vec<BranchRef>,
    path: Vec<BranchRef>,
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
            .ok_or_else(|| anyhow::anyhow!("current branch is not available among local refs"))?
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
        let output = run_git_checked_with_cancel(
            repo_root,
            &["rev-list", "--count", &range],
            cancel.clone(),
        )?;
        branch.root_distance = output
            .trim()
            .parse::<usize>()
            .with_context(|| format!("failed to parse git ancestry distance for {}", range))?;
    }

    Ok(())
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

fn normalize_lookup(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_pr_create_args_uses_non_interactive_flags() {
        let args = build_pr_create_args(
            "main",
            "feature/demo",
            "feature/demo (via ComfyGit)",
            Path::new("changelog_temp.md"),
        );

        assert_eq!(
            args,
            vec![
                "pr",
                "create",
                "--base",
                "main",
                "--head",
                "feature/demo",
                "--title",
                "feature/demo (via ComfyGit)",
                "--body-file",
                "changelog_temp.md",
            ]
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>()
        );
    }

    #[test]
    fn classify_preview_key_maps_requested_controls() {
        assert_eq!(
            classify_preview_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            Some(PreviewAction::Create)
        );
        assert_eq!(
            classify_preview_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE)),
            Some(PreviewAction::Edit)
        );
        assert_eq!(
            classify_preview_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            Some(PreviewAction::Cancel)
        );
    }

    #[test]
    fn pr_body_editor_supports_multiline_edits() {
        let mut editor = PrBodyEditor::new("first\nsecond");
        editor.move_up();
        editor.cursor_col = line_char_len(&editor.lines[editor.cursor_row]);
        editor.insert_newline();
        editor.insert_str("middle");

        assert_eq!(editor.into_string(), "first\nmiddle\nsecond");
    }
}
