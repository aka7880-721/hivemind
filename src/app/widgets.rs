// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit SA-PS License
// For details, see the LICENSE file in the repository root.

use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow, bail};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use ratatui_explorer::{FileExplorer, FileExplorerBuilder};
use tui_textarea::{Input as TextAreaInput, Key as TextAreaKey, TextArea as TuiTextArea};

use crate::{
    cli::CommitRenamePlan, config::BranchScopeKind, tui::ProjectEditFocus, tui::TILE_WIDTH,
    tui::WizardField, workflow::versioning::VersionScheme,
};

use super::state::{OverviewVersionControl, RecentChangeView};

use super::{
    BROWSE_BUTTON_WIDTH, FORM_LABEL_WIDTH, GIT_BRANCH_COLORS, HitAction, SHORTCUT_HINT_COLOR,
};
pub(crate) fn sanitize_pasted_text(text: &str) -> String {
    text.chars()
        .filter(|character| *character != '\r' && *character != '\n')
        .collect()
}

pub(crate) fn sanitize_tag_fragment(text: &str) -> String {
    let sanitized = text
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    sanitized.trim_matches('-').to_string()
}

#[derive(Clone)]
pub(crate) struct DialogButton {
    pub(crate) label: String,
    pub(crate) focused: bool,
    pub(crate) action: HitAction,
    pub(crate) style: Style,
}

impl DialogButton {
    pub(crate) fn new(
        label: impl Into<String>,
        focused: bool,
        action: HitAction,
        style: Style,
    ) -> Self {
        Self {
            label: label.into(),
            focused,
            action,
            style,
        }
    }
}

#[derive(Clone)]
pub(crate) struct FormRowButton {
    pub(crate) label: &'static str,
    pub(crate) action: HitAction,
}

impl FormRowButton {
    pub(crate) fn new(label: &'static str, action: HitAction) -> Self {
        Self { label, action }
    }
}

pub(crate) struct FormRowRects {
    pub(crate) field: Rect,
    pub(crate) button: Option<Rect>,
}

pub(crate) struct TagAnnotationDialog {
    pub(crate) editor: TuiTextArea<'static>,
    pub(crate) placeholder: String,
}

impl TagAnnotationDialog {
    pub(crate) fn new(existing_annotation: &str) -> Self {
        Self::with_placeholder(existing_annotation, "Optional multi-line tag annotation")
    }

    pub(crate) fn with_placeholder(existing_annotation: &str, placeholder: &str) -> Self {
        let mut editor = if existing_annotation.trim().is_empty() {
            TuiTextArea::default()
        } else {
            TuiTextArea::from(existing_annotation.lines())
        };
        editor.set_placeholder_text(placeholder);
        editor.set_tab_length(2);
        editor.set_max_histories(100);
        Self {
            editor,
            placeholder: placeholder.to_string(),
        }
    }
}

pub(crate) struct CommitRenameDialog {
    pub(crate) view: RecentChangeView,
    pub(crate) plan: CommitRenamePlan,
    pub(crate) message_editor: TuiTextArea<'static>,
    pub(crate) push_after_rename: bool,
}

impl CommitRenameDialog {
    pub(crate) fn new(view: RecentChangeView, plan: CommitRenamePlan) -> Self {
        let mut message_editor = if plan.current_subject.trim().is_empty() {
            TuiTextArea::default()
        } else {
            TuiTextArea::from(plan.current_subject.lines())
        };
        message_editor.set_placeholder_text("Edit commit message (supports Markdown)");
        message_editor.set_tab_length(2);
        message_editor.set_max_histories(100);
        Self {
            view,
            plan,
            message_editor,
            push_after_rename: false,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum BrowseTarget {
    WizardTargetPath,
    WizardRepoRoot,
    ProjectEditTargetPath,
    ProjectEditRepoRoot,
    ProjectSettingsChangelogPath,
    ProjectSettingsReleaseNowGeneral,
    ProjectSettingsReleaseNowWindows,
    ProjectSettingsReleaseNowLinuxArm,
    ProjectSettingsReleaseNowLinuxAmd,
    ProjectSettingsReleaseNowMacOs,
    ProjectSettingsAliasDistPath,
    ProjectSettingsAliasUiPath,
    ProjectSettingsAliasCustomPath(u16),
}

pub(crate) struct FileBrowserDialog {
    pub(crate) title: &'static str,
    pub(crate) target: BrowseTarget,
    pub(crate) explorer: FileExplorer,
    pub(crate) select_directories: bool,
}

impl FileBrowserDialog {
    pub(crate) fn new(target: BrowseTarget, initial_path: String) -> Result<Self> {
        let select_directories = matches!(
            target,
            BrowseTarget::WizardRepoRoot
                | BrowseTarget::ProjectEditRepoRoot
                | BrowseTarget::ProjectSettingsAliasDistPath
                | BrowseTarget::ProjectSettingsAliasUiPath
                | BrowseTarget::ProjectSettingsAliasCustomPath(_)
        );
        let explorer = configure_file_explorer(
            FileExplorerBuilder::default(),
            &initial_path,
            select_directories,
        )?;
        let title = match target {
            BrowseTarget::WizardRepoRoot | BrowseTarget::ProjectEditRepoRoot => "Browse Repo Root",
            BrowseTarget::ProjectSettingsChangelogPath => "Browse Changelog Path",
            BrowseTarget::ProjectSettingsReleaseNowGeneral
            | BrowseTarget::ProjectSettingsReleaseNowWindows
            | BrowseTarget::ProjectSettingsReleaseNowLinuxArm
            | BrowseTarget::ProjectSettingsReleaseNowLinuxAmd
            | BrowseTarget::ProjectSettingsReleaseNowMacOs => "Browse Release Script",
            _ => "Browse Target Path",
        };

        Ok(Self {
            title,
            target,
            explorer,
            select_directories,
        })
    }
}

pub(crate) fn configure_file_explorer(
    builder: FileExplorerBuilder,
    initial_path: &str,
    select_directories: bool,
) -> Result<FileExplorer> {
    let initial = initial_path.trim();
    if initial.is_empty() {
        return builder.build().map_err(anyhow::Error::from);
    }

    let path = PathBuf::from(initial);
    if path.is_file() && !select_directories {
        return builder
            .working_file(path)
            .build()
            .map_err(anyhow::Error::from);
    }
    if path.is_dir() {
        if select_directories {
            return builder
                .working_file(path)
                .build()
                .map_err(anyhow::Error::from);
        }
        return builder
            .working_dir(path)
            .build()
            .map_err(anyhow::Error::from);
    }

    if let Some(parent) = path.parent().filter(|parent| parent.is_dir()) {
        return builder
            .working_dir(parent.to_path_buf())
            .build()
            .map_err(anyhow::Error::from);
    }

    builder.build().map_err(anyhow::Error::from)
}

pub(crate) fn visible_field_width(row_width: u16, has_browse: bool) -> usize {
    let browse_width = if has_browse {
        BROWSE_BUTTON_WIDTH + 1
    } else {
        0
    };
    row_width
        .saturating_sub(FORM_LABEL_WIDTH)
        .saturating_sub(browse_width)
        .saturating_sub(2)
        .max(1) as usize
}

pub(crate) fn derive_repo_root_from_target_path(path: &str) -> Option<String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return None;
    }

    Path::new(trimmed)
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(|parent| parent.display().to_string())
}

fn normalized_path_string(path: &str) -> String {
    path.trim().replace('\\', "/")
}

fn is_absolute_path_string(path: &str) -> bool {
    let bytes = path.as_bytes();
    path.starts_with('/')
        || path.starts_with("//")
        || path.starts_with("\\\\")
        || (bytes.len() >= 3 && bytes[1] == b':' && (bytes[2] == b'/' || bytes[2] == b'\\'))
}

/// Resolve repo-relative paths to absolute filesystem paths for browsing and I/O.
pub(crate) fn absolutize_path_for_repo_root(path: &str, repo_root: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    if is_absolute_path_string(trimmed) {
        return normalized_path_string(trimmed);
    }

    let root = repo_root.trim();
    if root.is_empty() {
        return normalized_path_string(trimmed);
    }

    std::path::Path::new(root)
        .join(trimmed.trim_start_matches('/'))
        .display()
        .to_string()
}

/// Keep non-root paths relative to repo root for storage/display.
pub(crate) fn normalize_path_for_repo_root(path: &str, repo_root: &str) -> String {
    let mut value = normalized_path_string(path);
    if value.is_empty() {
        return value;
    }

    if !is_absolute_path_string(&value) {
        return value.trim_start_matches('/').to_string();
    }

    let root = normalized_path_string(repo_root)
        .trim_end_matches('/')
        .to_string();
    if root.is_empty() {
        return value;
    }

    let value_lower = value.to_ascii_lowercase();
    let root_lower = root.to_ascii_lowercase();
    if value_lower == root_lower {
        return String::new();
    }
    if let Some(rest) = value_lower.strip_prefix(&(root_lower + "/")) {
        let keep_len = rest.len();
        value = value[value.len() - keep_len..].to_string();
        return value.trim_start_matches('/').to_string();
    }

    value
}

pub(crate) fn git_graph_base_column(lines: &[String]) -> usize {
    lines
        .iter()
        .flat_map(|line| line.char_indices())
        .filter_map(|(index, character)| matches!(character, '*' | '|').then_some(index))
        .min()
        .unwrap_or(0)
}

pub(crate) fn colorize_git_log_line(line: &str, graph_base_column: usize) -> Line<'static> {
    let Some((hash_start, hash_end)) = find_commit_hash_range(line) else {
        return Line::from(line.to_string());
    };

    let prefix = &line[..hash_start];
    let hash = &line[hash_start..hash_end];
    let suffix = &line[hash_end..];
    let hash_color = git_hash_color(prefix, graph_base_column).unwrap_or(Color::White);
    let mut spans = Vec::new();

    for (index, character) in prefix.chars().enumerate() {
        if is_git_graph_character(character) {
            spans.push(Span::styled(
                character.to_string(),
                Style::default()
                    .fg(git_branch_color(
                        index.saturating_sub(graph_base_column) / 2,
                    ))
                    .add_modifier(Modifier::BOLD),
            ));
        } else {
            spans.push(Span::raw(character.to_string()));
        }
    }

    spans.push(Span::styled(
        hash.to_string(),
        Style::default().fg(hash_color).add_modifier(Modifier::BOLD),
    ));
    if !suffix.is_empty() {
        spans.push(Span::raw(suffix.to_string()));
    }

    Line::from(spans)
}

pub(crate) fn highlight_git_log_line(line: Line<'static>) -> Line<'static> {
    let highlight = Style::default().bg(Color::Rgb(55, 80, 140));
    Line::from(
        line.spans
            .into_iter()
            .map(|span| Span::styled(span.content, span.style.patch(highlight)))
            .collect::<Vec<_>>(),
    )
}

fn git_hash_color(prefix: &str, graph_base_column: usize) -> Option<Color> {
    prefix
        .chars()
        .enumerate()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .find_map(|(index, character)| {
            is_git_graph_character(character).then_some(git_branch_color(
                index.saturating_sub(graph_base_column) / 2,
            ))
        })
}

fn git_branch_color(slot: usize) -> Color {
    GIT_BRANCH_COLORS[slot % GIT_BRANCH_COLORS.len()]
}

fn is_git_graph_character(character: char) -> bool {
    matches!(character, '|' | '\\' | ',' | '/' | '*')
}

fn find_commit_hash_range(line: &str) -> Option<(usize, usize)> {
    let indices = line.char_indices().collect::<Vec<_>>();
    for (position, (byte_index, character)) in indices.iter().enumerate() {
        if !character.is_ascii_hexdigit() {
            continue;
        }

        let previous_is_space = position == 0 || indices[position - 1].1 == ' ';
        if !previous_is_space {
            continue;
        }

        let mut end = position;
        while end < indices.len() && indices[end].1.is_ascii_hexdigit() {
            end += 1;
        }

        if end - position < 7 {
            continue;
        }

        let next_is_space = end == indices.len() || indices[end].1 == ' ';
        if !next_is_space {
            continue;
        }

        let end_byte = if end < indices.len() {
            indices[end].0
        } else {
            line.len()
        };
        return Some((*byte_index, end_byte));
    }

    None
}

/// Scroll a custom-rendered textarea by moving the cursor (viewport follows cursor).
pub(crate) fn scroll_textarea_by_lines(editor: &mut TuiTextArea<'_>, delta_lines: i16) {
    if delta_lines == 0 {
        return;
    }
    let step = if delta_lines < 0 {
        tui_textarea::CursorMove::Up
    } else {
        tui_textarea::CursorMove::Down
    };
    for _ in 0..delta_lines.unsigned_abs() {
        editor.move_cursor(step);
    }
}

pub(crate) fn convert_to_textarea_input(key: KeyEvent) -> Option<TextAreaInput> {
    let text_key = match key.code {
        KeyCode::Backspace => TextAreaKey::Backspace,
        KeyCode::Enter => TextAreaKey::Enter,
        KeyCode::Left => TextAreaKey::Left,
        KeyCode::Right => TextAreaKey::Right,
        KeyCode::Up => TextAreaKey::Up,
        KeyCode::Down => TextAreaKey::Down,
        KeyCode::Home => TextAreaKey::Home,
        KeyCode::End => TextAreaKey::End,
        KeyCode::PageUp => TextAreaKey::PageUp,
        KeyCode::PageDown => TextAreaKey::PageDown,
        KeyCode::Tab | KeyCode::BackTab => TextAreaKey::Tab,
        KeyCode::Delete => TextAreaKey::Delete,
        KeyCode::Esc => TextAreaKey::Esc,
        KeyCode::Char(character) => TextAreaKey::Char(character),
        _ => return None,
    };

    Some(TextAreaInput {
        key: text_key,
        ctrl: key.modifiers.contains(KeyModifiers::CONTROL),
        alt: key.modifiers.contains(KeyModifiers::ALT),
        shift: key.modifiers.contains(KeyModifiers::SHIFT),
    })
}

/// Maps a mouse click at `(relative_row, clicked_col)` inside the textarea content area to a
/// logical `(line_index, col)` position, accounting for line wrapping.
///
/// * `lines`        – all logical lines from `editor.lines()`
/// * `start_row`    – first visible logical line (scroll offset)
/// * `content_width`– number of visible character columns (after number gutter)
/// * `relative_row` – terminal row relative to the first content row (0-indexed)
/// * `clicked_col`  – column offset after the number gutter (0-indexed)
pub(crate) fn textarea_click_position(
    lines: &[&str],
    start_row: usize,
    content_width: usize,
    relative_row: usize,
    clicked_col: usize,
) -> (usize, usize) {
    let cw = content_width.max(1);
    let mut terminal_rows_consumed = 0usize;
    for (i, line) in lines[start_row..].iter().enumerate() {
        let logical_index = start_row + i;
        let char_count = line.chars().count();
        let rows_for_line = char_count.div_ceil(cw).max(1);
        if terminal_rows_consumed + rows_for_line > relative_row {
            // The click falls within this logical line
            let row_within_line = relative_row - terminal_rows_consumed;
            let col = (row_within_line * cw + clicked_col).min(char_count);
            return (logical_index, col);
        }
        terminal_rows_consumed += rows_for_line;
    }
    // Click is past the last line
    let last = lines.len().saturating_sub(1);
    let last_len = lines.get(last).map(|l| l.chars().count()).unwrap_or(0);
    (last, last_len)
}

pub(crate) fn render_annotation_line(
    line: &str,
    line_number: usize,
    number_width: usize,
    _content_width: usize,
    active_cursor_col: Option<usize>,
    sel_start: Option<usize>,
    sel_end: Option<usize>,
) -> Line<'static> {
    let mut spans = vec![Span::styled(
        format!("{:>width$} ", line_number, width = number_width),
        Style::default().fg(Color::DarkGray),
    )];

    let chars: Vec<_> = line.chars().collect();
    let line_len = chars.len();

    if chars.is_empty() {
        if active_cursor_col.is_some() && active_cursor_col.unwrap_or(0) == 0 {
            spans.push(Span::styled(
                " ".to_string(),
                Style::default().fg(Color::Black).bg(Color::Cyan),
            ));
        }
    } else {
        let cursor = active_cursor_col.unwrap_or(0);
        let selection_start = sel_start.unwrap_or(line_len + 1);
        let selection_end = sel_end.unwrap_or(line_len).min(line_len);

        for (index, character) in chars.iter().enumerate() {
            let in_selection =
                sel_start.is_some() && index >= selection_start && index < selection_end;
            let at_cursor = index == cursor && active_cursor_col.is_some();

            let style = if at_cursor {
                Style::default().fg(Color::Black).bg(Color::Cyan)
            } else if in_selection {
                Style::default().bg(Color::Rgb(60, 80, 120))
            } else {
                Style::default()
            };
            spans.push(Span::styled(character.to_string(), style));
        }

        // Show cursor at end of line if needed
        if cursor == line_len && active_cursor_col.is_some() {
            spans.push(Span::styled(
                " ".to_string(),
                Style::default().fg(Color::Black).bg(Color::Cyan),
            ));
        }
    }

    Line::from(spans)
}

pub(crate) fn dialog_form_row_height(viewport_height: u16) -> u16 {
    if viewport_height >= 8 {
        3
    } else if viewport_height >= 4 {
        2
    } else {
        1
    }
}

pub(crate) fn dialog_visible_rows(viewport_height: u16, row_height: u16) -> usize {
    (viewport_height / row_height.max(1)).max(1) as usize
}

pub(crate) fn clamp_dialog_scroll(
    scroll_offset: &mut usize,
    total_rows: usize,
    visible_rows: usize,
    focus_index: Option<usize>,
) {
    let visible_rows = visible_rows.max(1);
    let max_scroll = total_rows.saturating_sub(visible_rows);
    *scroll_offset = (*scroll_offset).min(max_scroll);

    if let Some(focus_index) = focus_index {
        if focus_index < *scroll_offset {
            *scroll_offset = focus_index;
        } else if focus_index >= *scroll_offset + visible_rows {
            *scroll_offset = focus_index + 1 - visible_rows;
        }
    }
}

pub(crate) fn render_vertical_overflow_indicators(
    frame: &mut Frame,
    area: Rect,
    show_above: bool,
    show_below: bool,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let indicator_width = area.width.min(5);
    if show_above {
        let top_rect = Rect {
            x: area.x + area.width.saturating_sub(indicator_width),
            y: area.y,
            width: indicator_width,
            height: 1,
        };
        frame.render_widget(
            Paragraph::new("↑↑↑").alignment(Alignment::Right).style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            top_rect,
        );
    }

    if show_below {
        let bottom_rect = Rect {
            x: area.x + area.width.saturating_sub(indicator_width),
            y: area.y + area.height.saturating_sub(1),
            width: indicator_width,
            height: 1,
        };
        frame.render_widget(
            Paragraph::new("↓↓↓").alignment(Alignment::Right).style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            bottom_rect,
        );
    }
}

pub(crate) fn rotate_scope_kind(scope_kind: BranchScopeKind, delta: i32) -> BranchScopeKind {
    if delta >= 0 {
        match scope_kind {
            BranchScopeKind::Branch => BranchScopeKind::Module,
            BranchScopeKind::Module => BranchScopeKind::Service,
            BranchScopeKind::Service => BranchScopeKind::Branch,
        }
    } else {
        match scope_kind {
            BranchScopeKind::Branch => BranchScopeKind::Service,
            BranchScopeKind::Module => BranchScopeKind::Branch,
            BranchScopeKind::Service => BranchScopeKind::Module,
        }
    }
}

pub(crate) fn target_key_presets(path: &str) -> &'static [&'static str] {
    let path_lower = path.trim().to_ascii_lowercase();
    let file_name = std::path::Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path)
        .to_ascii_lowercase();
    if crate::workflow::targets::is_plain_version_filename(path) {
        return &["", ".", "@"];
    }
    if crate::workflow::targets::is_gomod_filename(path) {
        return &["comment", "require."];
    }
    if file_name == "gemfile" {
        return &["version", "gem."];
    }
    if file_name.ends_with(".gemspec") {
        return &["version", "gem."];
    }
    if file_name.ends_with(".csproj") {
        return &[
            "PropertyGroup.Version",
            "PropertyGroup.PackageVersion",
            "Version",
        ];
    }
    if file_name == "pubspec.yaml" {
        return &["version", "appVersion"];
    }
    if file_name == "project.toml" {
        return &["project.version", "version"];
    }
    if file_name == "description" {
        return &["Version"];
    }
    if file_name == "cmakelists.txt" {
        return &["project", "VERSION", "PROJECT_VERSION"];
    }
    if file_name == "makefile" || file_name == "gnumakefile" {
        return &["VERSION", "version"];
    }
    if file_name == "build.gradle" || file_name == "build.gradle.kts" {
        return &["version", "versionName", "versionCode"];
    }
    if file_name == "project.clj" {
        return &["defproject", "version"];
    }
    if file_name.ends_with(".plist") {
        return &["CFBundleShortVersionString", "CFBundleVersion"];
    }
    if file_name == "package.swift" {
        return &["version", "packageVersion", "comment"];
    }
    if file_name == "mix.exs" {
        return &["version"];
    }
    if file_name == "build.sbt" {
        return &["version", "ThisBuild / version"];
    }
    if file_name.ends_with(".cabal") {
        return &["version", "name"];
    }
    if file_name == "configure.ac" {
        return &["AC_INIT"];
    }
    if file_name == "meson.build" {
        return &["project", "version"];
    }
    if file_name.ends_with(".nimble") {
        return &["version"];
    }
    if file_name.ends_with(".rockspec") {
        return &["version"];
    }
    if file_name == "composer.json" || file_name == "deno.json" || file_name == "meta.json" {
        return &["version", "package.version"];
    }
    if file_name == "package.yaml" || file_name == "shard.yml" {
        return &["version"];
    }
    if file_name.eq_ignore_ascii_case("makefile.pl") {
        return &["VERSION", "version"];
    }
    if file_name.eq_ignore_ascii_case("module.bazel") {
        return &["module", "version"];
    }
    let extension = std::path::Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase());
    match extension.as_deref() {
        Some("toml") if path_lower.contains("pyproject") => {
            &["project.version", "tool.poetry.version", "version"]
        }
        Some("toml") if path_lower.ends_with("project.toml") => &["project.version", "version"],
        Some("toml") => &["package.version", "workspace.package.version", "version"],
        Some("json") => &["version", "package.version", "workspace.package.version"],
        Some("yaml") | Some("yml") => &["version", "appVersion", "chart.version"],
        Some("xml") => &["project.version", "version"],
        Some("cfg") => &["metadata.version", "version"],
        _ => &["version", "package.version", "workspace.package.version"],
    }
}

pub(crate) fn default_target_key_for_path(path: &str) -> &'static str {
    target_key_presets(path)[0]
}

pub(crate) fn target_key_is_custom(path: &str, value: &str) -> bool {
    !target_key_presets(path)
        .iter()
        .any(|preset| preset == &value.trim())
}

pub(crate) fn cycle_target_key_preset(path: &str, current: &str, delta: i32) -> String {
    let presets = target_key_presets(path);
    let current_index = presets
        .iter()
        .position(|preset| *preset == current.trim())
        .unwrap_or(0) as i32;
    let next_index =
        (current_index + if delta >= 0 { 1 } else { -1 }).rem_euclid(presets.len() as i32) as usize;
    presets[next_index].to_string()
}

pub(crate) fn wizard_form_row_button(field: WizardField) -> Option<FormRowButton> {
    match field {
        WizardField::TargetPath => Some(FormRowButton::new(
            "Browse",
            HitAction::BrowseWizardTargetPath,
        )),
        WizardField::TargetKey => Some(FormRowButton::new(
            "Custom",
            HitAction::EnableWizardCustomTargetKey,
        )),
        WizardField::RepoRoot => Some(FormRowButton::new(
            "Browse",
            HitAction::BrowseWizardRepoRoot,
        )),
        _ => None,
    }
}

pub(crate) fn project_edit_form_row_button(field: ProjectEditFocus) -> Option<FormRowButton> {
    match field {
        ProjectEditFocus::TargetPath => Some(FormRowButton::new(
            "Browse",
            HitAction::BrowseProjectTargetPath,
        )),
        ProjectEditFocus::TargetKey => Some(FormRowButton::new(
            "Custom",
            HitAction::EnableProjectCustomTargetKey,
        )),
        ProjectEditFocus::RepoRoot => Some(FormRowButton::new(
            "Browse",
            HitAction::BrowseProjectRepoRoot,
        )),
        _ => None,
    }
}

pub(crate) fn dashboard_tile_columns(width: u16) -> usize {
    ((width + 1) / (TILE_WIDTH + 1)).max(1) as usize
}

pub(crate) fn rect_contains(rect: Rect, column: u16, row: u16) -> bool {
    column >= rect.x && column < rect.x + rect.width && row >= rect.y && row < rect.y + rect.height
}

pub(crate) fn digit_to_index(character: char) -> Option<usize> {
    character
        .to_digit(10)
        .and_then(|digit| digit.checked_sub(1))
        .map(|digit| digit as usize)
}

pub(crate) fn adjust_pending_version_value(
    scheme: VersionScheme,
    current: &str,
    control: OverviewVersionControl,
    delta: i32,
) -> Result<String> {
    match scheme {
        VersionScheme::SemVer => adjust_semver_overview_value(current, control, delta),
        _ => adjust_numeric_tail_overview_value(current, delta),
    }
}

fn adjust_semver_overview_value(
    current: &str,
    control: OverviewVersionControl,
    delta: i32,
) -> Result<String> {
    let mut parts = current
        .split('.')
        .map(|part| {
            part.parse::<i32>()
                .map_err(|_| anyhow!("invalid semver component '{}'", part))
        })
        .collect::<Result<Vec<_>>>()?;
    if parts.len() != 3 {
        bail!("overview semver editing requires MAJOR.MINOR.PATCH");
    }

    let index = match control {
        OverviewVersionControl::Major => 0,
        OverviewVersionControl::Minor => 1,
        OverviewVersionControl::Patch | OverviewVersionControl::Whole => 2,
    };
    parts[index] = (parts[index] + delta).max(0);
    match control {
        OverviewVersionControl::Major => {
            parts[1] = 0;
            parts[2] = 0;
        }
        OverviewVersionControl::Minor => {
            parts[2] = 0;
        }
        OverviewVersionControl::Patch | OverviewVersionControl::Whole => {}
    }
    Ok(format!("{}.{}.{}", parts[0], parts[1], parts[2]))
}

fn adjust_numeric_tail_overview_value(current: &str, delta: i32) -> Result<String> {
    let mut parts = current
        .split('.')
        .map(|part| {
            part.parse::<i32>()
                .map_err(|_| anyhow!("invalid numeric component '{}'", part))
        })
        .collect::<Result<Vec<_>>>()?;
    let last = parts
        .last_mut()
        .ok_or_else(|| anyhow!("overview version is empty"))?;
    *last = (*last + delta).max(0);
    Ok(parts
        .into_iter()
        .map(|part| part.to_string())
        .collect::<Vec<_>>()
        .join("."))
}

pub(crate) fn browser_visible_range(
    total: usize,
    selected: usize,
    height: usize,
) -> (usize, usize) {
    if total == 0 || height == 0 {
        return (0, 0);
    }

    let start = selected
        .saturating_sub(height / 2)
        .min(total.saturating_sub(height));
    let end = (start + height).min(total);
    (start, end)
}

pub(crate) fn header_height_for_viewport(_total_height: u16) -> u16 {
    if _total_height <= 18 {
        2
    } else if _total_height <= 22 {
        3
    } else if _total_height < 40 {
        7
    } else {
        9
    }
}

pub(crate) fn should_use_recent_changes_tab(area_height: u16, max_tile_height: u16) -> bool {
    area_height < max_tile_height.saturating_add(1).saturating_add(8)
}

pub(crate) fn main_screens_shortcut_spans() -> Vec<Span<'static>> {
    shortcut_key_label("1-3", " Screens")
}

pub(crate) fn ui_settings_footer_line() -> Line<'static> {
    let mut spans = main_screens_shortcut_spans();
    spans.push(Span::raw(" | "));
    spans.extend(shortcut_key_label("[", "]"));
    spans.push(Span::styled(
        " Tabs",
        Style::default().fg(SHORTCUT_HINT_COLOR),
    ));
    spans.push(Span::raw(" | "));
    spans.extend(shortcut_key_label("Space", " Toggle"));
    spans.push(Span::raw(" | "));
    spans.extend(shortcut_key_label("←", "→"));
    spans.push(Span::styled(
        " Cycle",
        Style::default().fg(SHORTCUT_HINT_COLOR),
    ));
    spans.push(Span::raw(" | "));
    spans.extend(shortcut_key_label("D", " Dashboard"));
    spans.push(Span::raw(" | "));
    spans.extend(shortcut_key_label("?", " Help"));
    spans.push(Span::raw(" | "));
    spans.extend(shortcut_key_label("Q", "uit"));
    Line::from(spans)
}

pub(crate) fn shortcut_token(token: &str) -> Vec<Span<'static>> {
    vec![Span::styled(
        token.to_string(),
        Style::default()
            .fg(SHORTCUT_HINT_COLOR)
            .add_modifier(Modifier::BOLD),
    )]
}

pub(crate) fn shortcut_key_label(key: &str, rest: &str) -> Vec<Span<'static>> {
    let mut spans = shortcut_token(key);
    spans.push(Span::raw(rest.to_string()));
    spans
}
