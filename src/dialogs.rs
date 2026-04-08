// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyVersionBumper License v1.2
//
// For details, see the LICENSE file in the repository root.

use anyhow::{Result, bail};
use chrono::Local;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::{
    config::{IntegrationMode, ProjectConfig},
    git::{ensure_git_repo, project_repo_root, run_git, run_git_checked, split_output_lines, suggested_tag_name},
    targets::{BumpTarget, collect_bump_targets},
    versioning::{BumpAction, VersionScheme},
};

#[derive(Clone)]
pub(crate) struct RecentChangesDialog {
    pub(crate) project_name: String,
    pub(crate) repo_root: String,
    pub(crate) recent_range: ChangeRange,
    pub(crate) history_ranges: Vec<ChangeRange>,
    pub(crate) active_tab: RecentChangesTab,
    pub(crate) history_index: usize,
    pub(crate) scroll: u16,
}

#[derive(Clone)]
pub(crate) struct ChangeRange {
    pub(crate) label: String,
    pub(crate) lines: Vec<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum RecentChangesTab {
    Recent,
    History,
}

impl RecentChangesDialog {
    pub(crate) fn from_project(project: &ProjectConfig) -> Result<Self> {
        let repo_root = project_repo_root(project)?;
        ensure_git_repo(&repo_root)?;

        let describe = run_git(&repo_root, &["describe", "--tags", "--abbrev=0"])?;
        let recent_range = if describe.success {
            let tag = describe.stdout.trim().to_string();
            let range = format!("{}..HEAD", tag);
            let output = run_git_checked(&repo_root, &["log", "--oneline", "--graph", &range])?;
            let lines = split_output_lines(&output);
            ChangeRange { label: range, lines }
        } else {
            let output = run_git_checked(&repo_root, &["log", "--oneline", "--graph", "-n", "60"])?;
            ChangeRange {
                label: "no tags found; showing the latest 60 commits".to_string(),
                lines: split_output_lines(&output),
            }
        };

        let tags = split_output_lines(&run_git_checked(&repo_root, &["tag", "--sort=-creatordate"])?);
        let mut history_ranges = Vec::new();
        for window in tags.windows(2) {
            let newer = &window[0];
            let older = &window[1];
            let range = format!("{}..{}", older, newer);
            let output = run_git_checked(&repo_root, &["log", "--oneline", "--graph", &range])?;
            history_ranges.push(ChangeRange {
                label: range,
                lines: split_output_lines(&output),
            });
        }

        Ok(Self {
            project_name: project.name.clone(),
            repo_root,
            recent_range,
            history_ranges,
            active_tab: RecentChangesTab::Recent,
            history_index: 0,
            scroll: 0,
        })
    }

    pub(crate) fn current_range(&self) -> &ChangeRange {
        match self.active_tab {
            RecentChangesTab::Recent => &self.recent_range,
            RecentChangesTab::History => self
                .history_ranges
                .get(self.history_index)
                .unwrap_or(&self.recent_range),
        }
    }

    pub(crate) fn switch_tab(&mut self, tab: RecentChangesTab) {
        self.active_tab = tab;
        self.scroll = 0;
    }

    pub(crate) fn cycle_tab(&mut self, delta: isize) {
        let tabs = [RecentChangesTab::Recent, RecentChangesTab::History];
        let current = match self.active_tab {
            RecentChangesTab::Recent => 0,
            RecentChangesTab::History => 1,
        } as isize;
        let next = (current + delta).rem_euclid(tabs.len() as isize) as usize;
        self.switch_tab(tabs[next]);
    }

    pub(crate) fn navigate_history(&mut self, delta: isize) {
        if self.history_ranges.is_empty() {
            return;
        }

        let next = (self.history_index as isize + delta)
            .clamp(0, self.history_ranges.len().saturating_sub(1) as isize) as usize;
        if next != self.history_index {
            self.history_index = next;
            self.scroll = 0;
        }
    }
}

#[derive(Clone)]
pub(crate) struct TagDialog {
    pub(crate) project_name: String,
    pub(crate) repo_root: String,
    pub(crate) remote_spec: Option<String>,
    pub(crate) tag_name: TextInput,
    pub(crate) annotation: String,
    pub(crate) actions: Vec<TagAction>,
    pub(crate) action_index: usize,
}

impl TagDialog {
    pub(crate) fn from_project(project: &ProjectConfig) -> Result<Self> {
        let repo_root = project_repo_root(project)?;
        ensure_git_repo(&repo_root)?;
        let default_tag = suggested_tag_name(project);
        let remote_spec = project.repo.as_ref().and_then(|repo| repo.remote_url.clone());
        let actions = match project.integration_mode {
            IntegrationMode::LocalOnly => bail!("local-only projects do not support git tags"),
            IntegrationMode::GitLocalOnly => vec![TagAction::CreateLocal],
            IntegrationMode::GitHubEnabled => vec![
                TagAction::CreateLocal,
                TagAction::CreateAndPush,
                TagAction::CreatePushAndRelease,
            ],
        };
        Ok(Self {
            project_name: project.name.clone(),
            repo_root,
            remote_spec,
            tag_name: TextInput::with_value(default_tag),
            annotation: String::new(),
            actions,
            action_index: 0,
        })
    }

    pub(crate) fn selected_action(&self) -> TagAction {
        self.actions[self.action_index]
    }

    pub(crate) fn rotate_action(&mut self, delta: isize) {
        if self.actions.len() <= 1 {
            self.action_index = 0;
            return;
        }
        let len = self.actions.len() as isize;
        self.action_index = (self.action_index as isize + delta).rem_euclid(len) as usize;
    }
}

#[derive(Clone, Copy)]
pub(crate) enum TagAction {
    CreateLocal,
    CreateAndPush,
    CreatePushAndRelease,
}

impl TagAction {
    pub(crate) fn display_name(self) -> &'static str {
        match self {
            TagAction::CreateLocal => "Local Tag",
            TagAction::CreateAndPush => "Tag + Push",
            TagAction::CreatePushAndRelease => "Tag + Push + Release",
        }
    }
}

#[derive(Clone)]
pub(crate) struct BumpDialog {
    pub(crate) project_name: String,
    pub(crate) scheme: VersionScheme,
    pub(crate) current_version: String,
    pub(crate) targets: Vec<BumpTarget>,
    pub(crate) action_index: usize,
}

impl BumpDialog {
    pub(crate) fn from_project(project: &ProjectConfig) -> Result<Self> {
        let targets = collect_bump_targets(project)?;
        if targets.is_empty() {
            bail!("selected project does not contain any configured targets");
        }

        let scheme = targets[0].scheme;
        if targets.iter().any(|target| target.scheme != scheme) {
            bail!("projects with mixed version schemes are not supported in the bump preview yet");
        }

        let current_version = targets[0].current_version.clone();
        if targets.iter().any(|target| target.current_version != current_version) {
            bail!("configured targets do not currently share the same version value");
        }

        Ok(Self {
            project_name: project.name.clone(),
            scheme,
            current_version,
            targets,
            action_index: 0,
        })
    }

    pub(crate) fn actions(&self) -> &'static [BumpAction] {
        self.scheme.supported_actions()
    }

    pub(crate) fn selected_action(&self) -> BumpAction {
        self.actions()[self.action_index]
    }

    pub(crate) fn rotate_action(&mut self, delta: isize) {
        let actions = self.actions();
        if actions.len() <= 1 {
            self.action_index = 0;
            return;
        }
        let len = actions.len() as isize;
        let next = (self.action_index as isize + delta).rem_euclid(len);
        self.action_index = next as usize;
    }

    pub(crate) fn preview_next_version(&self) -> Result<String> {
        let today = Local::now().date_naive();
        self.scheme
            .bump(&self.current_version, self.selected_action(), today)
            .map_err(anyhow::Error::msg)
    }
}

#[derive(Clone)]
pub(crate) struct TextInput {
    pub(crate) value: String,
    cursor: usize,
}

impl TextInput {
    pub(crate) fn with_value(value: impl Into<String>) -> Self {
        let value = value.into();
        let cursor = value.len();
        Self { value, cursor }
    }

    pub(crate) fn set_value(&mut self, value: impl Into<String>) {
        self.value = value.into();
        self.cursor = self.value.len();
    }

    pub(crate) fn insert(&mut self, character: char) {
        self.value.insert(self.cursor, character);
        self.cursor += character.len_utf8();
    }

    pub(crate) fn insert_str(&mut self, text: &str) {
        self.value.insert_str(self.cursor, text);
        self.cursor = (self.cursor + text.len()).min(self.value.len());
    }

    pub(crate) fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let previous = previous_char_boundary(&self.value, self.cursor);
        self.value.drain(previous..self.cursor);
        self.cursor = previous;
    }

    pub(crate) fn delete(&mut self) {
        if self.cursor >= self.value.len() {
            return;
        }
        let next = next_char_boundary(&self.value, self.cursor);
        self.value.drain(self.cursor..next);
    }

    pub(crate) fn move_left(&mut self) {
        self.cursor = previous_char_boundary(&self.value, self.cursor);
    }

    pub(crate) fn move_right(&mut self) {
        self.cursor = next_char_boundary(&self.value, self.cursor);
    }

    pub(crate) fn home(&mut self) {
        self.cursor = 0;
    }

    pub(crate) fn end(&mut self) {
        self.cursor = self.value.len();
    }

    pub(crate) fn handle_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char(character) if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT => self.insert(character),
            KeyCode::Backspace => self.backspace(),
            KeyCode::Delete => self.delete(),
            KeyCode::Left => self.move_left(),
            KeyCode::Right => self.move_right(),
            KeyCode::Home => self.home(),
            KeyCode::End => self.end(),
            _ => {}
        }
    }

    pub(crate) fn display_value(&self, focused: bool) -> String {
        self.display_value_with_width(focused, usize::MAX)
    }

    pub(crate) fn display_value_with_width(&self, focused: bool, max_width: usize) -> String {
        if max_width == 0 {
            return String::new();
        }

        let cursor = self.cursor.min(self.value.len());
        let mut rendered: Vec<char> = self.value.chars().collect();
        if focused {
            let cursor_index = self.value[..cursor].chars().count();
            rendered.insert(cursor_index, '|');
        }

        if rendered.len() <= max_width {
            return rendered.into_iter().collect();
        }

        if !focused {
            let start = rendered.len().saturating_sub(max_width);
            return rendered[start..].iter().collect();
        }

        let cursor_index = self.value[..cursor].chars().count();
        let visible_cursor = cursor_index.min(rendered.len().saturating_sub(1));
        let mut start = visible_cursor.saturating_sub(max_width.saturating_sub(1));
        let end = (start + max_width).min(rendered.len());
        if end - start < max_width {
            start = end.saturating_sub(max_width);
        }
        rendered[start..end].iter().collect()
    }

    pub(crate) fn value(&self) -> &str {
        &self.value
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.value.trim().is_empty()
    }
}

fn previous_char_boundary(value: &str, index: usize) -> usize {
    if index == 0 {
        return 0;
    }

    let mut previous = 0;
    for (offset, _) in value.char_indices() {
        if offset >= index {
            break;
        }
        previous = offset;
    }
    previous
}

fn next_char_boundary(value: &str, index: usize) -> usize {
    if index >= value.len() {
        return value.len();
    }

    value[index..]
        .char_indices()
        .nth(1)
        .map(|(offset, _)| index + offset)
        .unwrap_or(value.len())
}

#[cfg(test)]
mod tests {
    use super::TextInput;

    #[test]
    fn display_value_with_width_keeps_cursor_visible() {
        let mut input = TextInput::with_value("C:/very/long/path/to/a/file.json");
        input.home();
        for _ in 0..20 {
            input.move_right();
        }

        let rendered = input.display_value_with_width(true, 12);
        assert!(rendered.contains('|'));
        assert!(rendered.len() <= 12);
    }
}