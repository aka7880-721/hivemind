// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit SA-PS License
//
// For details, see the LICENSE file in the repository root.

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style, Stylize},
    text::Line,
    widgets::{Paragraph, StatefulWidget},
};
use tui_checkbox::Checkbox;

use super::{App, DashboardPane, FORM_LABEL_WIDTH, HitAction, HitTarget, Screen, StatusMessage};
use crate::{
    config::{cycle_tab_selection_flash_color, tab_selection_flash_color_label},
    tui::{apply_tab_selection_flash, comfy_tab_nav, sync_tab_nav_flash_state},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UiSettingsTab {
    General,
    Tabs,
}

impl UiSettingsTab {
    const ALL: [Self; 2] = [Self::General, Self::Tabs];

    fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Tabs => "Tabs",
        }
    }

    fn step(self, delta: isize) -> Self {
        let index = Self::ALL.iter().position(|tab| *tab == self).unwrap_or(0) as isize;
        Self::ALL[(index + delta).rem_euclid(Self::ALL.len() as isize) as usize]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UiSettingsFocus {
    ShowTabHints,
    HideFooter,
    FooterContent,
    TabSelectionFlashEnabled,
    TabSelectionFlashColor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UiSettingsRow {
    Text,
    Spacer(u16),
    Checkbox(UiSettingsFocus),
    Cycle(UiSettingsFocus),
}

impl UiSettingsRow {
    fn height(self) -> u16 {
        match self {
            Self::Text => 1,
            Self::Spacer(lines) => lines,
            Self::Checkbox(_) | Self::Cycle(_) => 2,
        }
    }

    fn focus(self) -> Option<UiSettingsFocus> {
        match self {
            Self::Checkbox(field) | Self::Cycle(field) => Some(field),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct UiSettingsState {
    pub(crate) tab: UiSettingsTab,
    pub(crate) focus: UiSettingsFocus,
    pub(crate) scroll: u16,
    pub(crate) viewport_height: u16,
    pub(crate) follow_focus: bool,
}

impl Default for UiSettingsState {
    fn default() -> Self {
        Self {
            tab: UiSettingsTab::General,
            focus: UiSettingsFocus::ShowTabHints,
            scroll: 0,
            viewport_height: 0,
            follow_focus: true,
        }
    }
}

impl UiSettingsState {
    pub(crate) fn visible_fields(&self, tab: UiSettingsTab) -> Vec<UiSettingsFocus> {
        match tab {
            UiSettingsTab::General => vec![
                UiSettingsFocus::ShowTabHints,
                UiSettingsFocus::HideFooter,
                UiSettingsFocus::FooterContent,
            ],
            UiSettingsTab::Tabs => vec![
                UiSettingsFocus::TabSelectionFlashEnabled,
                UiSettingsFocus::TabSelectionFlashColor,
            ],
        }
    }

    fn focus_next(&mut self) {
        let fields = self.visible_fields(self.tab);
        let index = fields
            .iter()
            .position(|field| *field == self.focus)
            .unwrap_or(0);
        self.focus = fields[(index + 1) % fields.len()];
        self.follow_focus = true;
    }

    fn focus_previous(&mut self) {
        let fields = self.visible_fields(self.tab);
        let index = fields
            .iter()
            .position(|field| *field == self.focus)
            .unwrap_or(0);
        let len = fields.len();
        self.focus = fields[(index + len - 1) % len];
        self.follow_focus = true;
    }

    fn clamp_scroll(&mut self, total_height: u16, viewport_height: u16) {
        let max_scroll = total_height.saturating_sub(viewport_height);
        self.scroll = self.scroll.min(max_scroll);
    }

    fn scroll_by(&mut self, delta: isize, total_height: u16, viewport_height: u16) {
        self.follow_focus = false;
        self.clamp_scroll(total_height, viewport_height);
        let max_scroll = total_height.saturating_sub(viewport_height);
        self.scroll = if delta.is_negative() {
            self.scroll.saturating_sub(delta.unsigned_abs() as u16)
        } else {
            self.scroll.saturating_add(delta as u16).min(max_scroll)
        };
    }
}

pub(crate) fn render_ui_settings(app: &mut App, frame: &mut Frame, area: Rect) {
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(8)])
        .split(area);

    render_ui_settings_tabs(app, frame, sections[0]);

    let rows = build_rows(app.ui_settings_state.tab);
    render_ui_settings_rows(app, frame, sections[1], &rows);
}

fn render_ui_settings_tabs(app: &mut App, frame: &mut Frame, area: Rect) {
    let labels: Vec<&str> = UiSettingsTab::ALL.iter().map(|tab| tab.label()).collect();
    let active_index = UiSettingsTab::ALL
        .iter()
        .position(|tab| *tab == app.ui_settings_state.tab)
        .unwrap_or(0);
    app.ui_settings_tab_nav_state.selected = active_index;
    sync_tab_nav_flash_state(&mut app.ui_settings_tab_nav_state, &app.config.ui);
    app.ui_settings_tab_strip_area = Some(area);
    let nav = apply_tab_selection_flash(comfy_tab_nav(&labels, active_index), &app.config.ui);
    let tab_rects = nav.tab_rects(area);
    StatefulWidget::render(
        nav,
        area,
        frame.buffer_mut(),
        &mut app.ui_settings_tab_nav_state,
    );

    for (idx, tab) in UiSettingsTab::ALL.iter().enumerate() {
        if let Some(rect) = tab_rects.get(idx) {
            app.hit_targets
                .push(HitTarget::new(*rect, HitAction::SelectUiSettingsTab(*tab)));
        }
    }
}

fn build_rows(tab: UiSettingsTab) -> Vec<UiSettingsRow> {
    match tab {
        UiSettingsTab::General => vec![
            UiSettingsRow::Checkbox(UiSettingsFocus::ShowTabHints),
            UiSettingsRow::Checkbox(UiSettingsFocus::HideFooter),
            UiSettingsRow::Cycle(UiSettingsFocus::FooterContent),
            UiSettingsRow::Spacer(1),
            UiSettingsRow::Text,
        ],
        UiSettingsTab::Tabs => vec![
            UiSettingsRow::Checkbox(UiSettingsFocus::TabSelectionFlashEnabled),
            UiSettingsRow::Cycle(UiSettingsFocus::TabSelectionFlashColor),
            UiSettingsRow::Spacer(1),
            UiSettingsRow::Text,
        ],
    }
}

fn render_ui_settings_rows(app: &mut App, frame: &mut Frame, area: Rect, rows: &[UiSettingsRow]) {
    let gutter_width = if area.width > 2 { 1u16 } else { 0 };
    let content_area = Rect {
        width: area.width.saturating_sub(gutter_width),
        ..area
    };
    app.ui_settings_state.viewport_height = content_area.height;
    let total_height = rows.iter().map(|row| row.height()).sum();
    if app.ui_settings_state.follow_focus
        && let Some((top, height)) = focused_row_bounds(rows, app.ui_settings_state.focus)
    {
        let viewport_top = app.ui_settings_state.scroll;
        let viewport_bottom = viewport_top.saturating_add(content_area.height);
        if top < viewport_top {
            app.ui_settings_state.scroll = top;
        } else if top.saturating_add(height) > viewport_bottom {
            app.ui_settings_state.scroll = top
                .saturating_add(height)
                .saturating_sub(content_area.height);
        }
    }
    app.ui_settings_state
        .clamp_scroll(total_height, content_area.height);

    let mut cursor_y = 0u16;
    let scroll = app.ui_settings_state.scroll;
    for row in rows {
        let row_height = row.height();
        let row_bottom = cursor_y.saturating_add(row_height);
        if row_bottom <= scroll {
            cursor_y = row_bottom;
            continue;
        }

        let screen_y = content_area
            .y
            .saturating_add(cursor_y.saturating_sub(scroll));
        if screen_y >= content_area.y.saturating_add(content_area.height) {
            break;
        }
        let remaining_height = content_area
            .height
            .saturating_sub(screen_y.saturating_sub(content_area.y));
        if remaining_height == 0 {
            break;
        }
        let row_area = Rect {
            x: content_area.x,
            y: screen_y,
            width: content_area.width,
            height: row_height.min(remaining_height),
        };

        match row {
            UiSettingsRow::Text => {
                let line = help_line_for_tab(app.ui_settings_state.tab);
                frame.render_widget(Paragraph::new(line), row_area);
            }
            UiSettingsRow::Spacer(_) => {}
            UiSettingsRow::Checkbox(field) if row_area.height >= 2 => {
                let focused = *field == app.ui_settings_state.focus;
                render_checkbox_row(app, frame, row_area, *field, focused);
            }
            UiSettingsRow::Cycle(field) if row_area.height >= 2 => {
                let focused = *field == app.ui_settings_state.focus;
                render_cycle_row(app, frame, row_area, *field, focused);
            }
            _ => {}
        }

        cursor_y = row_bottom;
    }

    if gutter_width == 1 && total_height > content_area.height {
        let indicator_x = area.x + area.width - 1;
        if app.ui_settings_state.scroll > 0 {
            frame.render_widget(
                Paragraph::new("▲").alignment(Alignment::Center),
                Rect {
                    x: indicator_x,
                    y: area.y,
                    width: 1,
                    height: 1,
                },
            );
        }
        if app
            .ui_settings_state
            .scroll
            .saturating_add(content_area.height)
            < total_height
        {
            frame.render_widget(
                Paragraph::new("▼").alignment(Alignment::Center),
                Rect {
                    x: indicator_x,
                    y: area.y + area.height.saturating_sub(1),
                    width: 1,
                    height: 1,
                },
            );
        }
    }
}

fn help_line_for_tab(tab: UiSettingsTab) -> Line<'static> {
    match tab {
        UiSettingsTab::General => Line::from(
            "Footer and tab-hint options apply across the dashboard. Space toggles checkboxes; ←/→ cycles alignment."
                .dim(),
        ),
        UiSettingsTab::Tabs => Line::from(
            "Tab selection flash highlights the border of a newly selected tab twice. Space toggles; ←/→ picks color."
                .dim(),
        ),
    }
}

fn focused_row_bounds(rows: &[UiSettingsRow], focus: UiSettingsFocus) -> Option<(u16, u16)> {
    let mut top = 0u16;
    for row in rows {
        let height = row.height();
        if row.focus() == Some(focus) {
            return Some((top, height));
        }
        top = top.saturating_add(height);
    }
    None
}

fn control_inset(area: Rect) -> Rect {
    Rect {
        x: area.x.saturating_add(1),
        y: area.y,
        width: area.width.saturating_sub(2),
        height: area.height,
    }
}

fn checkbox_label(field: UiSettingsFocus) -> &'static str {
    match field {
        UiSettingsFocus::ShowTabHints => "Show tab hints in footer",
        UiSettingsFocus::HideFooter => "Hide footer",
        UiSettingsFocus::FooterContent => "Footer content alignment",
        UiSettingsFocus::TabSelectionFlashEnabled => "Tab selection flash",
        UiSettingsFocus::TabSelectionFlashColor => "Flash color",
    }
}

fn render_checkbox_row(
    app: &mut App,
    frame: &mut Frame,
    area: Rect,
    field: UiSettingsFocus,
    focused: bool,
) {
    let inset = control_inset(area);
    let enabled = match field {
        UiSettingsFocus::ShowTabHints => app.config.ui.show_tab_hints,
        UiSettingsFocus::HideFooter => app.config.ui.hide_footer,
        UiSettingsFocus::TabSelectionFlashEnabled => app.config.ui.tab_selection_flash_enabled,
        _ => false,
    };
    let checkbox = Checkbox::new(checkbox_label(field), enabled)
        .checked_symbol("✅ ")
        .unchecked_symbol("❌ ")
        .style(if focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::White)
        })
        .checkbox_style(Style::default().fg(if enabled { Color::Green } else { Color::Red }))
        .label_style(if focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::White)
        });
    frame.render_widget(checkbox, inset);
    app.hit_targets.push(HitTarget::new(
        inset,
        HitAction::SelectUiSettingsField(field),
    ));
}

fn render_cycle_row(
    app: &mut App,
    frame: &mut Frame,
    area: Rect,
    field: UiSettingsFocus,
    focused: bool,
) {
    let inset = control_inset(area);
    let row = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(FORM_LABEL_WIDTH), Constraint::Min(10)])
        .split(inset);

    let label_style = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::White)
    };
    let value = match field {
        UiSettingsFocus::FooterContent => app.config.ui.footer_content.display_name().to_string(),
        UiSettingsFocus::TabSelectionFlashColor => app.config.ui.tab_selection_flash_color_label(),
        _ => String::new(),
    };
    frame.render_widget(
        Paragraph::new(checkbox_label(field)).style(label_style),
        Rect {
            height: 1,
            ..row[0]
        },
    );
    frame.render_widget(
        Paragraph::new(format!("< {value} >")).style(label_style),
        Rect {
            height: 1,
            ..row[1]
        },
    );
    app.hit_targets.push(HitTarget::new(
        inset,
        HitAction::SelectUiSettingsField(field),
    ));
}

pub(crate) fn step_ui_settings_tab(app: &mut App, delta: isize) {
    app.ui_settings_state.tab = app.ui_settings_state.tab.step(delta);
    app.ui_settings_state.scroll = 0;
    app.ui_settings_state.follow_focus = true;
    let fields = app
        .ui_settings_state
        .visible_fields(app.ui_settings_state.tab);
    if let Some(first) = fields.first() {
        app.ui_settings_state.focus = *first;
    }
    flash_ui_settings_tab_selection(app);
}

pub(crate) fn flash_ui_settings_tab_selection(app: &mut App) {
    let index = UiSettingsTab::ALL
        .iter()
        .position(|tab| *tab == app.ui_settings_state.tab)
        .unwrap_or(0);
    app.ui_settings_tab_nav_state.flash_selection(index);
}

pub(crate) fn sync_ui_settings_tab_nav(app: &mut App) {
    sync_tab_nav_flash_state(&mut app.ui_settings_tab_nav_state, &app.config.ui);
    sync_tab_nav_flash_state(&mut app.project_settings_tab_nav_state, &app.config.ui);
    sync_tab_nav_flash_state(&mut app.overview_tab_nav_state, &app.config.ui);
}

pub(crate) fn try_handle_ui_settings_key(app: &mut App, key: KeyEvent) -> Result<bool> {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s') {
        app.screen = Screen::Dashboard;
        app.dashboard_focus = DashboardPane::Projects;
        return Ok(true);
    }

    if matches!(key.code, KeyCode::Char('[') | KeyCode::Char(']')) {
        step_ui_settings_tab(
            app,
            if matches!(key.code, KeyCode::Char('[')) {
                -1
            } else {
                1
            },
        );
        return Ok(true);
    }

    let rows = build_rows(app.ui_settings_state.tab);
    let total_height = rows.iter().map(|row| row.height()).sum();
    let viewport_height = app.ui_settings_state.viewport_height.max(1);

    match key.code {
        KeyCode::Esc | KeyCode::Char('d') | KeyCode::Char('D') => {
            app.screen = Screen::Dashboard;
            app.dashboard_focus = DashboardPane::Projects;
            return Ok(true);
        }
        KeyCode::Char('n') => {
            app.open_wizard();
            return Ok(true);
        }
        KeyCode::PageUp => {
            app.ui_settings_state
                .scroll_by(-3, total_height, viewport_height);
            return Ok(true);
        }
        KeyCode::PageDown => {
            app.ui_settings_state
                .scroll_by(3, total_height, viewport_height);
            return Ok(true);
        }
        KeyCode::Tab | KeyCode::Down => {
            app.ui_settings_state.focus_next();
            return Ok(true);
        }
        KeyCode::BackTab | KeyCode::Up => {
            app.ui_settings_state.focus_previous();
            return Ok(true);
        }
        KeyCode::Left => {
            if matches!(
                app.ui_settings_state.focus,
                UiSettingsFocus::FooterContent | UiSettingsFocus::TabSelectionFlashColor
            ) {
                toggle_focused_ui_settings_control(app, -1)?;
            }
            return Ok(true);
        }
        KeyCode::Right => {
            if matches!(
                app.ui_settings_state.focus,
                UiSettingsFocus::FooterContent | UiSettingsFocus::TabSelectionFlashColor
            ) {
                toggle_focused_ui_settings_control(app, 1)?;
            }
            return Ok(true);
        }
        KeyCode::Enter | KeyCode::Char(' ') => {
            toggle_focused_ui_settings_control(app, 1)?;
            return Ok(true);
        }
        _ => {}
    }

    Ok(false)
}

pub(crate) fn activate_ui_settings_field(app: &mut App, focus: UiSettingsFocus) -> Result<()> {
    app.ui_settings_state.focus = focus;
    app.ui_settings_state.follow_focus = true;
    toggle_focused_ui_settings_control(app, 1)
}

fn toggle_focused_ui_settings_control(app: &mut App, delta: i32) -> Result<()> {
    match app.ui_settings_state.focus {
        UiSettingsFocus::ShowTabHints => app.toggle_tab_hints()?,
        UiSettingsFocus::HideFooter => app.toggle_footer()?,
        UiSettingsFocus::FooterContent => app.cycle_footer_content(delta)?,
        UiSettingsFocus::TabSelectionFlashEnabled => {
            app.config.ui.tab_selection_flash_enabled = !app.config.ui.tab_selection_flash_enabled;
            app.config_store.save(&app.config)?;
            sync_ui_settings_tab_nav(app);
            app.status = StatusMessage::success(if app.config.ui.tab_selection_flash_enabled {
                "Tab selection flash enabled."
            } else {
                "Tab selection flash disabled."
            });
        }
        UiSettingsFocus::TabSelectionFlashColor => {
            app.config.ui.tab_selection_flash_color =
                cycle_tab_selection_flash_color(app.config.ui.tab_selection_flash_color, delta);
            app.config_store.save(&app.config)?;
            sync_ui_settings_tab_nav(app);
            app.status = StatusMessage::success(format!(
                "Tab flash color: {}.",
                tab_selection_flash_color_label(app.config.ui.tab_selection_flash_color)
            ));
        }
    }
    Ok(())
}

pub(crate) fn flash_overview_tab_selection(app: &mut App, include_recent_changes: bool) {
    if !app.config.ui.tab_selection_flash_enabled {
        return;
    }
    sync_tab_nav_flash_state(&mut app.overview_tab_nav_state, &app.config.ui);
    let index = crate::tui::overview_tab_index(app.overview_tab, include_recent_changes);
    app.overview_tab_nav_state.flash_selection(index);
}

pub(crate) fn flash_project_settings_tab_strip(app: &mut App) {
    if !app.config.ui.tab_selection_flash_enabled {
        return;
    }
    sync_tab_nav_flash_state(&mut app.project_settings_tab_nav_state, &app.config.ui);
    app.flash_project_settings_tab_selection();
}
