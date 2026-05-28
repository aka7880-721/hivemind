// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit SA-PS License
//
// For details, see the LICENSE file in the repository root.

//! Advanced alias settings UI (Project Settings → General).

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::{
    config::{AdvancedAliasSettings, CustomAliasEntry, ProjectConfig},
    dialogs::TextInput,
};

use super::{
    App, BROWSE_BUTTON_WIDTH, FORM_LABEL_WIDTH, HitAction, HitTarget,
    project_settings::{ProjectSettingsFocus, ProjectSettingsRow, ProjectSettingsState},
    visible_field_width,
};

const ALIAS_ADD_BUTTON_WIDTH: u16 = 22;
const ALIAS_ADD_BUTTON_LABEL: &str = "Add Custom Sub-ALIAS";

#[derive(Clone)]
pub(crate) struct AliasCustomEntryState {
    pub name: String,
    pub path: TextInput,
}

pub(crate) fn sync_alias_state_from_project(
    state: &mut ProjectSettingsState,
    project: &ProjectConfig,
    scope_index: usize,
) {
    let settings = project.advanced_alias_for_scope(scope_index);
    state.advanced_alias_enabled = settings.enabled;
    state.alias_dist_path.set_value(settings.dist_path.clone());
    state.alias_ui_path.set_value(settings.ui_path.clone());
    state.alias_custom = settings
        .custom
        .iter()
        .map(|entry| AliasCustomEntryState {
            name: entry.name.clone(),
            path: TextInput::with_value(entry.path.clone()),
        })
        .collect();
    state.alias_custom_draft_active = false;
    state.alias_custom_draft_name.set_value(String::new());
}

pub(crate) fn persist_alias_state_to_project(
    project: &mut ProjectConfig,
    scope_index: usize,
    state: &ProjectSettingsState,
) {
    let settings = project.advanced_alias_for_scope_mut(scope_index);
    settings.enabled = state.advanced_alias_enabled;
    settings.dist_path = state.alias_dist_path.value().trim().to_string();
    settings.ui_path = state.alias_ui_path.value().trim().to_string();
    settings.custom = state
        .alias_custom
        .iter()
        .map(|entry| CustomAliasEntry {
            name: entry.name.clone(),
            path: entry.path.value().trim().to_string(),
        })
        .collect();
}

pub(crate) fn append_general_alias_rows(
    rows: &mut Vec<ProjectSettingsRow>,
    project: &ProjectConfig,
    scope_index: usize,
    state: &ProjectSettingsState,
) {
    if !project.supports_advanced_alias_for_scope(scope_index) {
        return;
    }

    rows.push(ProjectSettingsRow::Spacer(1));
    rows.push(ProjectSettingsRow::Checkbox(
        ProjectSettingsFocus::AdvancedAliasEnabled,
    ));

    if !state.advanced_alias_enabled {
        return;
    }

    rows.push(ProjectSettingsRow::Path(
        ProjectSettingsFocus::AliasDistPath,
    ));
    rows.push(ProjectSettingsRow::Path(ProjectSettingsFocus::AliasUiPath));

    for index in 0..state.alias_custom.len() {
        let index = index as u16;
        rows.push(ProjectSettingsRow::AliasCustom { index });
    }

    if state.alias_custom_draft_active {
        rows.push(ProjectSettingsRow::AliasCustomDraft);
    }

    rows.push(ProjectSettingsRow::AliasAddButton);
}

pub(crate) fn append_alias_visible_fields(
    fields: &mut Vec<ProjectSettingsFocus>,
    project: &ProjectConfig,
    scope_index: usize,
    state: &ProjectSettingsState,
) {
    if !project.supports_advanced_alias_for_scope(scope_index) {
        return;
    }

    fields.push(ProjectSettingsFocus::AdvancedAliasEnabled);
    if !state.advanced_alias_enabled {
        return;
    }

    fields.push(ProjectSettingsFocus::AliasDistPath);
    fields.push(ProjectSettingsFocus::AliasUiPath);
    for index in 0..state.alias_custom.len() {
        fields.push(ProjectSettingsFocus::AliasCustomPath(index as u16));
        fields.push(ProjectSettingsFocus::AliasCustomDelete(index as u16));
    }
    if state.alias_custom_draft_active {
        fields.push(ProjectSettingsFocus::AliasCustomDraftName);
        fields.push(ProjectSettingsFocus::AliasCustomDraftConfirm);
    }
    fields.push(ProjectSettingsFocus::AliasCustomAdd);
}

pub(crate) fn render_alias_row(
    app: &mut App,
    frame: &mut Frame,
    row_area: Rect,
    row: &ProjectSettingsRow,
    focused_field: ProjectSettingsFocus,
) {
    match row {
        ProjectSettingsRow::AliasCustom { index } => {
            render_alias_custom_row(app, frame, row_area, *index, focused_field);
        }
        ProjectSettingsRow::AliasCustomDraft => {
            render_alias_custom_draft_row(app, frame, row_area, focused_field);
        }
        ProjectSettingsRow::AliasAddButton => {
            render_alias_add_button(app, frame, row_area, focused_field);
        }
        _ => {}
    }
}

fn render_alias_custom_row(
    app: &mut App,
    frame: &mut Frame,
    area: Rect,
    index: u16,
    focused_field: ProjectSettingsFocus,
) {
    let Some(entry) = app.project_settings_state.alias_custom.get(index as usize) else {
        return;
    };
    let path_focus = ProjectSettingsFocus::AliasCustomPath(index);
    let delete_focus = ProjectSettingsFocus::AliasCustomDelete(index);
    let path_focused = focused_field == path_focus;
    let delete_focused = focused_field == delete_focus;

    let inset = control_inset(area);
    let row = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(FORM_LABEL_WIDTH),
            Constraint::Min(10),
            Constraint::Length(1),
            Constraint::Length(BROWSE_BUTTON_WIDTH),
            Constraint::Length(1),
            Constraint::Length(BROWSE_BUTTON_WIDTH),
        ])
        .split(inset);

    let label_area = center_vertically(
        Rect {
            x: row[0].x,
            y: row[0].y,
            width: row[0].width,
            height: area.height,
        },
        1,
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            entry.name.as_str(),
            Style::default().fg(Color::Rgb(220, 220, 220)),
        ))),
        label_area,
    );

    let field_area = center_vertically(row[1], area.height.min(3));
    let block = if path_focused {
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
    } else {
        Block::default().borders(Borders::ALL)
    };
    let value = entry
        .path
        .display_line_with_width(path_focused, visible_field_width(row[1].width, true));
    frame.render_widget(Paragraph::new(value).block(block), field_area);

    let browse_area = center_vertically(row[3], area.height.min(3));
    let browse_style = if path_focused {
        Style::default().fg(Color::Black).bg(Color::Cyan)
    } else {
        Style::default().fg(Color::Black).bg(Color::DarkGray)
    };
    frame.render_widget(
        Paragraph::new("Browse")
            .alignment(Alignment::Center)
            .style(browse_style)
            .block(Block::default().borders(Borders::ALL)),
        browse_area,
    );

    let delete_area = center_vertically(row[5], area.height.min(3));
    let delete_style = if delete_focused {
        Style::default().fg(Color::White).bg(Color::Red)
    } else {
        Style::default()
            .fg(Color::Rgb(255, 180, 180))
            .bg(Color::Rgb(80, 20, 20))
    };
    frame.render_widget(
        Paragraph::new("Delete")
            .alignment(Alignment::Center)
            .style(delete_style)
            .block(Block::default().borders(Borders::ALL)),
        delete_area,
    );

    app.hit_targets.push(HitTarget::new(
        field_area,
        HitAction::SelectProjectSettingsField(path_focus),
    ));
    app.hit_targets.push(HitTarget::new(
        browse_area,
        HitAction::BrowseProjectSettingsField(path_focus),
    ));
    app.hit_targets.push(HitTarget::new(
        delete_area,
        HitAction::SelectProjectSettingsField(delete_focus),
    ));
}

fn render_alias_custom_draft_row(
    app: &mut App,
    frame: &mut Frame,
    area: Rect,
    focused_field: ProjectSettingsFocus,
) {
    let name_focused = focused_field == ProjectSettingsFocus::AliasCustomDraftName;
    let confirm_focused = focused_field == ProjectSettingsFocus::AliasCustomDraftConfirm;
    let inset = control_inset(area);
    let row = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(FORM_LABEL_WIDTH),
            Constraint::Min(10),
            Constraint::Length(1),
            Constraint::Length(BROWSE_BUTTON_WIDTH),
        ])
        .split(inset);

    let label_area = center_vertically(
        Rect {
            x: row[0].x,
            y: row[0].y,
            width: row[0].width,
            height: area.height,
        },
        1,
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "Name",
            Style::default().fg(Color::Rgb(220, 220, 220)),
        ))),
        label_area,
    );

    let field_area = center_vertically(row[1], area.height.min(3));
    let block = if name_focused {
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
    } else {
        Block::default().borders(Borders::ALL)
    };
    let value = app
        .project_settings_state
        .alias_custom_draft_name
        .display_line_with_width(name_focused, visible_field_width(row[1].width, true));
    frame.render_widget(Paragraph::new(value).block(block), field_area);

    let confirm_area = center_vertically(row[3], area.height.min(3));
    let confirm_style = if confirm_focused {
        Style::default().fg(Color::Black).bg(Color::Green)
    } else {
        Style::default()
            .fg(Color::Rgb(180, 255, 180))
            .bg(Color::Rgb(20, 80, 20))
    };
    frame.render_widget(
        Paragraph::new("OK")
            .alignment(Alignment::Center)
            .style(confirm_style)
            .block(Block::default().borders(Borders::ALL)),
        confirm_area,
    );

    app.hit_targets.push(HitTarget::new(
        field_area,
        HitAction::SelectProjectSettingsField(ProjectSettingsFocus::AliasCustomDraftName),
    ));
    app.hit_targets.push(HitTarget::new(
        confirm_area,
        HitAction::SelectProjectSettingsField(ProjectSettingsFocus::AliasCustomDraftConfirm),
    ));
}

fn render_alias_add_button(
    app: &mut App,
    frame: &mut Frame,
    area: Rect,
    focused_field: ProjectSettingsFocus,
) {
    let focused = focused_field == ProjectSettingsFocus::AliasCustomAdd;
    let inset = control_inset(area);
    let row = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(ALIAS_ADD_BUTTON_WIDTH),
            Constraint::Min(0),
        ])
        .split(inset);
    let button_area = center_vertically(row[0], area.height.min(3));
    let (fg, bg) = if focused {
        (Color::Black, Color::Green)
    } else {
        (Color::Rgb(180, 255, 180), Color::Rgb(20, 80, 20))
    };
    let fill = Style::default().fg(fg).bg(bg);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(fill)
        .style(fill);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(ALIAS_ADD_BUTTON_LABEL, fill)))
            .alignment(Alignment::Center)
            .block(block),
        button_area,
    );
    app.hit_targets.push(HitTarget::new(
        button_area,
        HitAction::SelectProjectSettingsField(ProjectSettingsFocus::AliasCustomAdd),
    ));
}

pub(crate) fn confirm_alias_custom_draft(app: &mut App) -> Option<String> {
    let name = app
        .project_settings_state
        .alias_custom_draft_name
        .value()
        .trim()
        .to_string();
    if name.is_empty() {
        return Some("Custom alias name cannot be empty.".to_string());
    }
    if AdvancedAliasSettings::is_reserved_name(&name) {
        return Some(format!(
            "Name '{}' is reserved. Use dist/ui for built-in paths.",
            name.trim()
        ));
    }
    if app
        .project_settings_state
        .alias_custom
        .iter()
        .any(|entry| entry.name.eq_ignore_ascii_case(&name))
    {
        return Some(format!("Custom alias '{}' already exists.", name));
    }

    app.project_settings_state
        .alias_custom
        .push(AliasCustomEntryState {
            name: name.clone(),
            path: TextInput::with_value(""),
        });
    app.project_settings_state.alias_custom_draft_active = false;
    app.project_settings_state
        .alias_custom_draft_name
        .set_value(String::new());
    app.project_settings_state.focus = ProjectSettingsFocus::AliasCustomPath(
        (app.project_settings_state.alias_custom.len() - 1) as u16,
    );
    app.project_settings_state.follow_focus = true;
    None
}

pub(crate) fn delete_alias_custom(app: &mut App, index: u16) {
    let idx = index as usize;
    if idx < app.project_settings_state.alias_custom.len() {
        app.project_settings_state.alias_custom.remove(idx);
        app.project_settings_state.follow_focus = true;
    }
}

pub(crate) fn set_alias_path_from_browse(
    state: &mut ProjectSettingsState,
    field: ProjectSettingsFocus,
    value: String,
) {
    match field {
        ProjectSettingsFocus::AliasDistPath => state.alias_dist_path.set_value(value),
        ProjectSettingsFocus::AliasUiPath => state.alias_ui_path.set_value(value),
        ProjectSettingsFocus::AliasCustomPath(index) => {
            if let Some(entry) = state.alias_custom.get_mut(index as usize) {
                entry.path.set_value(value);
            }
        }
        _ => {}
    }
}

fn control_inset(area: Rect) -> Rect {
    Rect {
        x: area.x.saturating_add(2),
        y: area.y,
        width: area.width.saturating_sub(2),
        height: area.height,
    }
}

fn center_vertically(area: Rect, content_height: u16) -> Rect {
    crate::ui::center_vertically(area, content_height)
}
