// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit License v1.2
//
// For details, see the LICENSE file in the repository root.

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style, Stylize},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph},
};
use tui_checkbox::Checkbox;
use tui_tabs::TabNav;

use super::{
    App, BROWSE_BUTTON_WIDTH, BrowseTarget, FORM_LABEL_WIDTH, FormRowButton, HitAction, HitTarget,
    visible_field_width,
};
use crate::{
    config::{DEFAULT_CHANGELOG_PATH, ProjectConfig, ProjectType},
    dialogs::TextInput,
    ui::center_vertically,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProjectSettingsTab {
    General,
    Distro,
    RlsQd,
}

impl ProjectSettingsTab {
    fn tab_strip(release_now_enabled: bool) -> &'static [ProjectSettingsTab] {
        if release_now_enabled {
            &[Self::General, Self::Distro, Self::RlsQd]
        } else {
            &[Self::General, Self::Distro]
        }
    }

    pub(crate) fn step(self, delta: isize, release_now_enabled: bool) -> Self {
        let tabs = Self::tab_strip(release_now_enabled);
        let index = tabs.iter().position(|tab| *tab == self).unwrap_or(0) as isize;
        tabs[(index + delta).rem_euclid(tabs.len() as isize) as usize]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProjectSettingsFocus {
    CustomMainBranchEnabled,
    CustomMainBranchName,
    Alias,
    ChangelogEnabled,
    ChangelogPath,
    ChangelogHidePrMessages,
    ChangelogHideBumpMessages,
    ChangelogMiniCommitHashes,
    ChangelogWrapDetailedIfTopPicks,
    ReleaseNowEnabled,
    ReleaseNowWindows,
    ReleaseNowLinuxArm,
    ReleaseNowLinuxAmd,
    ReleaseNowMacOs,
    QuickDownloadsEnabled,
    QuickDownloadsPosition,
    QuickDownloadsFooter,
}

#[derive(Clone)]
pub(crate) struct ProjectSettingsState {
    pub(crate) binding: Option<(usize, usize)>,
    pub(crate) focus: ProjectSettingsFocus,
    pub(crate) scroll: u16,
    pub(crate) viewport_height: u16,
    pub(crate) follow_focus: bool,
    pub(crate) custom_main_branch_name: TextInput,
    pub(crate) alias: TextInput,
    pub(crate) changelog_path: TextInput,
    pub(crate) changelog_hide_pr_messages: bool,
    pub(crate) changelog_hide_bump_messages: bool,
    pub(crate) changelog_mini_commit_hashes: bool,
    pub(crate) changelog_wrap_detailed_if_top_picks: bool,
    pub(crate) release_now_windows: TextInput,
    pub(crate) release_now_linux_arm: TextInput,
    pub(crate) release_now_linux_amd: TextInput,
    pub(crate) release_now_macos: TextInput,
    pub(crate) quick_downloads_position: TextInput,
    pub(crate) quick_downloads_footer: TextInput,
}

impl Default for ProjectSettingsState {
    fn default() -> Self {
        Self {
            binding: None,
            focus: ProjectSettingsFocus::CustomMainBranchEnabled,
            scroll: 0,
            viewport_height: 0,
            follow_focus: true,
            custom_main_branch_name: TextInput::with_value(""),
            alias: TextInput::with_value(""),
            changelog_path: TextInput::with_value(DEFAULT_CHANGELOG_PATH),
            changelog_hide_pr_messages: false,
            changelog_hide_bump_messages: false,
            changelog_mini_commit_hashes: false,
            changelog_wrap_detailed_if_top_picks: false,
            release_now_windows: TextInput::with_value(""),
            release_now_linux_arm: TextInput::with_value(""),
            release_now_linux_amd: TextInput::with_value(""),
            release_now_macos: TextInput::with_value(""),
            quick_downloads_position: TextInput::with_value(""),
            quick_downloads_footer: TextInput::with_value(""),
        }
    }
}

impl ProjectSettingsState {
    fn sync_from_project(
        &mut self,
        project_index: usize,
        tab: ProjectSettingsTab,
        project: &ProjectConfig,
        scope_index: usize,
    ) {
        if self.binding == Some((project_index, scope_index)) {
            return;
        }

        let release_now = project.release_now_for_scope(scope_index);
        self.binding = Some((project_index, scope_index));
        self.scroll = 0;
        self.follow_focus = true;
        self.custom_main_branch_name
            .set_value(project.repo_custom_main_branch_value_for_scope(scope_index));
        self.alias.set_value(project.alias.clone());
        self.changelog_path
            .set_value(project.changelog_path_for_scope(scope_index).to_string());
        self.changelog_hide_pr_messages = project.changelog_hide_pr_messages_for_scope(scope_index);
        self.changelog_hide_bump_messages =
            project.changelog_hide_bump_messages_for_scope(scope_index);
        self.changelog_mini_commit_hashes =
            project.changelog_mini_commit_hashes_for_scope(scope_index);
        self.changelog_wrap_detailed_if_top_picks =
            project.changelog_wrap_detailed_if_top_picks_for_scope(scope_index);
        self.release_now_windows
            .set_value(release_now.windows_script.clone());
        self.release_now_linux_arm
            .set_value(release_now.linux_arm_script.clone());
        self.release_now_linux_amd
            .set_value(release_now.linux_amd_script.clone());
        self.release_now_macos
            .set_value(release_now.macos_script.clone());
        let qd = &release_now.quick_downloads;
        self.quick_downloads_position
            .set_value(qd.position.display_name().to_string());
        self.quick_downloads_footer
            .set_value(qd.footer_message.clone());
        self.ensure_focus_visible(tab, project, scope_index);
    }

    fn visible_fields(
        &self,
        tab: ProjectSettingsTab,
        project: &ProjectConfig,
        scope_index: usize,
    ) -> Vec<ProjectSettingsFocus> {
        match tab {
            ProjectSettingsTab::General => {
                let mut fields = Vec::new();
                if project.integration_mode.requires_repo() {
                    fields.push(ProjectSettingsFocus::CustomMainBranchEnabled);
                    if project.repo_has_custom_main_branch_for_scope(scope_index) {
                        fields.push(ProjectSettingsFocus::CustomMainBranchName);
                    }
                }
                fields.push(ProjectSettingsFocus::Alias);
                fields.push(ProjectSettingsFocus::ChangelogEnabled);
                if project.changelog_enabled_for_scope(scope_index) {
                    fields.push(ProjectSettingsFocus::ChangelogPath);
                    fields.push(ProjectSettingsFocus::ChangelogHidePrMessages);
                    fields.push(ProjectSettingsFocus::ChangelogHideBumpMessages);
                    fields.push(ProjectSettingsFocus::ChangelogWrapDetailedIfTopPicks);
                    fields.push(ProjectSettingsFocus::ChangelogMiniCommitHashes);
                }
                fields
            }
            ProjectSettingsTab::Distro => {
                let mut fields = vec![ProjectSettingsFocus::ReleaseNowEnabled];
                if project.release_now_for_scope(scope_index).enabled {
                    fields.extend([
                        ProjectSettingsFocus::ReleaseNowWindows,
                        ProjectSettingsFocus::ReleaseNowLinuxArm,
                        ProjectSettingsFocus::ReleaseNowLinuxAmd,
                        ProjectSettingsFocus::ReleaseNowMacOs,
                    ]);
                }
                fields
            }
            ProjectSettingsTab::RlsQd => {
                let mut fields = vec![ProjectSettingsFocus::QuickDownloadsEnabled];
                if project
                    .release_now_for_scope(scope_index)
                    .quick_downloads
                    .enabled
                {
                    fields.push(ProjectSettingsFocus::QuickDownloadsPosition);
                    fields.push(ProjectSettingsFocus::QuickDownloadsFooter);
                }
                fields
            }
        }
    }

    fn ensure_focus_visible(
        &mut self,
        tab: ProjectSettingsTab,
        project: &ProjectConfig,
        scope_index: usize,
    ) {
        let fields = self.visible_fields(tab, project, scope_index);
        if !fields.contains(&self.focus) {
            self.focus = *fields.first().unwrap_or(&ProjectSettingsFocus::Alias);
            self.follow_focus = true;
        }
    }

    fn focus_next(&mut self, tab: ProjectSettingsTab, project: &ProjectConfig, scope_index: usize) {
        let fields = self.visible_fields(tab, project, scope_index);
        let index = fields
            .iter()
            .position(|field| *field == self.focus)
            .unwrap_or(0);
        self.focus = fields[(index + 1) % fields.len()];
        self.follow_focus = true;
    }

    fn focus_previous(
        &mut self,
        tab: ProjectSettingsTab,
        project: &ProjectConfig,
        scope_index: usize,
    ) {
        let fields = self.visible_fields(tab, project, scope_index);
        let index = fields
            .iter()
            .position(|field| *field == self.focus)
            .unwrap_or(0);
        self.focus = fields[(index + fields.len() - 1) % fields.len()];
        self.follow_focus = true;
    }

    fn focus_accepts_text(
        &self,
        tab: ProjectSettingsTab,
        project: &ProjectConfig,
        scope_index: usize,
    ) -> bool {
        self.visible_fields(tab, project, scope_index)
            .contains(&self.focus)
            && matches!(
                self.focus,
                ProjectSettingsFocus::CustomMainBranchName
                    | ProjectSettingsFocus::Alias
                    | ProjectSettingsFocus::ChangelogPath
                    | ProjectSettingsFocus::ReleaseNowWindows
                    | ProjectSettingsFocus::ReleaseNowLinuxArm
                    | ProjectSettingsFocus::ReleaseNowLinuxAmd
                    | ProjectSettingsFocus::ReleaseNowMacOs
                    | ProjectSettingsFocus::QuickDownloadsFooter
            )
    }

    pub(crate) fn active_input_mut(&mut self) -> Option<&mut TextInput> {
        match self.focus {
            ProjectSettingsFocus::CustomMainBranchName => Some(&mut self.custom_main_branch_name),
            ProjectSettingsFocus::Alias => Some(&mut self.alias),
            ProjectSettingsFocus::ChangelogPath => Some(&mut self.changelog_path),
            ProjectSettingsFocus::ReleaseNowWindows => Some(&mut self.release_now_windows),
            ProjectSettingsFocus::ReleaseNowLinuxArm => Some(&mut self.release_now_linux_arm),
            ProjectSettingsFocus::ReleaseNowLinuxAmd => Some(&mut self.release_now_linux_amd),
            ProjectSettingsFocus::ReleaseNowMacOs => Some(&mut self.release_now_macos),
            ProjectSettingsFocus::QuickDownloadsFooter => Some(&mut self.quick_downloads_footer),
            _ => None,
        }
    }

    fn handle_text_input(&mut self, key: KeyEvent) {
        if let Some(input) = self.active_input_mut() {
            input.handle_key(key);
        }
    }

    fn insert_text(&mut self, text: &str) -> bool {
        if let Some(input) = self.active_input_mut() {
            input.insert_str(text);
            return true;
        }
        false
    }

    fn display_value_for_field(
        &self,
        field: ProjectSettingsFocus,
        focused: bool,
        max_width: usize,
    ) -> Line<'static> {
        match field {
            ProjectSettingsFocus::CustomMainBranchName => self
                .custom_main_branch_name
                .display_line_with_width(focused, max_width),
            ProjectSettingsFocus::Alias => self.alias.display_line_with_width(focused, max_width),
            ProjectSettingsFocus::ChangelogPath => self
                .changelog_path
                .display_line_with_width(focused, max_width),
            ProjectSettingsFocus::ReleaseNowWindows => self
                .release_now_windows
                .display_line_with_width(focused, max_width),
            ProjectSettingsFocus::ReleaseNowLinuxArm => self
                .release_now_linux_arm
                .display_line_with_width(focused, max_width),
            ProjectSettingsFocus::ReleaseNowLinuxAmd => self
                .release_now_linux_amd
                .display_line_with_width(focused, max_width),
            ProjectSettingsFocus::ReleaseNowMacOs => self
                .release_now_macos
                .display_line_with_width(focused, max_width),
            ProjectSettingsFocus::QuickDownloadsPosition => Line::from(format!(
                "< {} >",
                self.quick_downloads_position.value().trim()
            )),
            ProjectSettingsFocus::QuickDownloadsFooter => self
                .quick_downloads_footer
                .display_line_with_width(focused, max_width),
            _ => Line::from(String::new()),
        }
    }

    fn set_value_from_browse(&mut self, field: ProjectSettingsFocus, value: String) {
        match field {
            ProjectSettingsFocus::ChangelogPath => self.changelog_path.set_value(value),
            ProjectSettingsFocus::ReleaseNowWindows => self.release_now_windows.set_value(value),
            ProjectSettingsFocus::ReleaseNowLinuxArm => self.release_now_linux_arm.set_value(value),
            ProjectSettingsFocus::ReleaseNowLinuxAmd => self.release_now_linux_amd.set_value(value),
            ProjectSettingsFocus::ReleaseNowMacOs => self.release_now_macos.set_value(value),
            ProjectSettingsFocus::QuickDownloadsFooter => {
                self.quick_downloads_footer.set_value(value)
            }
            _ => {}
        }
    }

    fn clamp_scroll(&mut self, total_height: u16, viewport_height: u16) {
        let max_scroll = total_height.saturating_sub(viewport_height);
        self.scroll = self.scroll.min(max_scroll);
    }

    fn ensure_row_visible(
        &mut self,
        top: u16,
        height: u16,
        total_height: u16,
        viewport_height: u16,
    ) {
        self.clamp_scroll(total_height, viewport_height);
        if viewport_height == 0 {
            self.scroll = 0;
            return;
        }
        if top < self.scroll {
            self.scroll = top;
        } else {
            let bottom = top.saturating_add(height);
            let viewport_bottom = self.scroll.saturating_add(viewport_height);
            if bottom > viewport_bottom {
                self.scroll = bottom.saturating_sub(viewport_height);
            }
        }
        self.clamp_scroll(total_height, viewport_height);
    }

    fn scroll_by(&mut self, delta: isize, total_height: u16, viewport_height: u16) {
        self.follow_focus = false;
        self.clamp_scroll(total_height, viewport_height);
        let max_scroll = total_height.saturating_sub(viewport_height);
        let next = if delta.is_negative() {
            self.scroll.saturating_sub(delta.unsigned_abs() as u16)
        } else {
            self.scroll.saturating_add(delta as u16).min(max_scroll)
        };
        self.scroll = next;
    }
}

pub(crate) fn render_project_settings(app: &mut App, frame: &mut Frame, area: Rect) {
    sync_project_settings_state(app);

    let Some(project) = app.config.projects.get(app.selected_project).cloned() else {
        frame.render_widget(
            Paragraph::new("Select a project to manage per-scope settings."),
            area,
        );
        return;
    };

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(8)])
        .split(area);

    render_project_settings_tabs(app, frame, sections[0]);

    let scope_index = active_scope_index(&project, app.overview_focused_scope);
    match app.project_settings_tab {
        ProjectSettingsTab::General => {
            render_general_settings(app, frame, sections[1], &project, scope_index)
        }
        ProjectSettingsTab::Distro => {
            render_distro_settings(app, frame, sections[1], &project, scope_index)
        }
        ProjectSettingsTab::RlsQd => {
            render_rls_qd_settings(app, frame, sections[1], &project, scope_index)
        }
    }
}

pub(crate) fn sync_project_settings_state(app: &mut App) {
    let Some(project) = app.config.projects.get(app.selected_project).cloned() else {
        return;
    };
    let scope_index = active_scope_index(&project, app.overview_focused_scope);
    if app.project_settings_tab == ProjectSettingsTab::RlsQd
        && !project.release_now_for_scope(scope_index).enabled
    {
        app.project_settings_tab = ProjectSettingsTab::Distro;
    }
    app.project_settings_state.sync_from_project(
        app.selected_project,
        app.project_settings_tab,
        &project,
        scope_index,
    );
}

pub(crate) fn invalidate_project_settings_state(app: &mut App) {
    app.project_settings_state.binding = None;
}

pub(crate) fn step_project_settings_tab(app: &mut App, delta: isize) {
    let Some(project) = app.config.projects.get(app.selected_project).cloned() else {
        return;
    };
    let scope_index = active_scope_index(&project, app.overview_focused_scope);
    let release_now_enabled = project.release_now_for_scope(scope_index).enabled;
    app.project_settings_tab = app.project_settings_tab.step(delta, release_now_enabled);
    app.project_settings_state.scroll = 0;
    app.project_settings_state.follow_focus = true;
    sync_project_settings_state(app);
    if let Some(project) = app.config.projects.get(app.selected_project).cloned() {
        let scope_index = active_scope_index(&project, app.overview_focused_scope);
        app.project_settings_state.ensure_focus_visible(
            app.project_settings_tab,
            &project,
            scope_index,
        );
    }
}

pub(crate) fn captures_text_input(app: &mut App) -> bool {
    if app.dashboard_focus != super::DashboardPane::Overview
        || app.overview_tab != super::OverviewTab::ProjectSettings
    {
        return false;
    }
    sync_project_settings_state(app);
    let Some(project) = app.config.projects.get(app.selected_project).cloned() else {
        return false;
    };
    let scope_index = active_scope_index(&project, app.overview_focused_scope);
    app.project_settings_state
        .focus_accepts_text(app.project_settings_tab, &project, scope_index)
}

pub(crate) fn try_handle_project_settings_key(app: &mut App, key: KeyEvent) -> Result<bool> {
    if app.dashboard_focus != super::DashboardPane::Overview
        || app.overview_tab != super::OverviewTab::ProjectSettings
    {
        return Ok(false);
    }

    sync_project_settings_state(app);
    let Some(project) = app.config.projects.get(app.selected_project).cloned() else {
        return Ok(false);
    };
    let scope_index = active_scope_index(&project, app.overview_focused_scope);

    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('o') {
        return open_browser_for_project_settings_focus(app).map(|_| true);
    }

    if matches!(key.code, KeyCode::Char('[') | KeyCode::Char(']')) {
        step_project_settings_tab(
            app,
            if matches!(key.code, KeyCode::Char('[')) {
                -1
            } else {
                1
            },
        );
        return Ok(true);
    }

    if app.project_settings_state.focus_accepts_text(
        app.project_settings_tab,
        &project,
        scope_index,
    ) {
        match key.code {
            KeyCode::Tab | KeyCode::Down => {
                app.project_settings_state.focus_next(
                    app.project_settings_tab,
                    &project,
                    scope_index,
                );
                return Ok(true);
            }
            KeyCode::BackTab | KeyCode::Up => {
                app.project_settings_state.focus_previous(
                    app.project_settings_tab,
                    &project,
                    scope_index,
                );
                return Ok(true);
            }
            KeyCode::Enter => {
                app.project_settings_state.focus_next(
                    app.project_settings_tab,
                    &project,
                    scope_index,
                );
                return Ok(true);
            }
            _ => {
                app.project_settings_state.handle_text_input(key);
                persist_project_settings_inputs(app)?;
                return Ok(true);
            }
        }
    }

    match key.code {
        KeyCode::Tab | KeyCode::Down => {
            app.project_settings_state
                .focus_next(app.project_settings_tab, &project, scope_index);
            Ok(true)
        }
        KeyCode::BackTab | KeyCode::Up => {
            app.project_settings_state.focus_previous(
                app.project_settings_tab,
                &project,
                scope_index,
            );
            Ok(true)
        }
        KeyCode::Left | KeyCode::Right
            if app.project_settings_state.focus == ProjectSettingsFocus::QuickDownloadsPosition =>
        {
            toggle_focused_project_settings_control(app)?;
            Ok(true)
        }
        KeyCode::Enter | KeyCode::Char(' ')
            if app.project_settings_state.focus != ProjectSettingsFocus::QuickDownloadsPosition =>
        {
            toggle_focused_project_settings_control(app)?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

pub(crate) fn insert_project_settings_text(app: &mut App, text: &str) -> bool {
    sync_project_settings_state(app);
    let Some(project) = app.config.projects.get(app.selected_project).cloned() else {
        return false;
    };
    let scope_index = active_scope_index(&project, app.overview_focused_scope);
    if !app.project_settings_state.focus_accepts_text(
        app.project_settings_tab,
        &project,
        scope_index,
    ) {
        return false;
    }
    let inserted = app.project_settings_state.insert_text(text);
    if inserted {
        let _ = persist_project_settings_inputs(app);
    }
    inserted
}

pub(crate) fn set_project_settings_focus(app: &mut App, focus: ProjectSettingsFocus) {
    sync_project_settings_state(app);
    app.project_settings_state.focus = focus;
    app.project_settings_state.follow_focus = true;
}

pub(crate) fn activate_project_settings_field(
    app: &mut App,
    focus: ProjectSettingsFocus,
) -> Result<()> {
    sync_project_settings_state(app);
    app.project_settings_state.focus = focus;
    app.project_settings_state.follow_focus = true;
    if is_checkbox_field(focus) {
        return toggle_focused_project_settings_control(app);
    }
    Ok(())
}

pub(crate) fn scroll_project_settings(app: &mut App, delta: isize) {
    sync_project_settings_state(app);
    let Some(project) = app.config.projects.get(app.selected_project).cloned() else {
        return;
    };
    let scope_index = active_scope_index(&project, app.overview_focused_scope);
    let rows = build_rows(app.project_settings_tab, &project, scope_index);
    let total_height = total_rows_height(&rows);
    let viewport_height = app.project_settings_state.viewport_height;
    app.project_settings_state
        .scroll_by(delta, total_height, viewport_height);
}

#[derive(Clone)]
enum ProjectSettingsRow {
    Text(Line<'static>),
    Spacer(u16),
    Checkbox(ProjectSettingsFocus),
    DualCheckbox(ProjectSettingsFocus, ProjectSettingsFocus),
    Path(ProjectSettingsFocus),
}

impl ProjectSettingsRow {
    fn height(&self) -> u16 {
        match self {
            Self::Text(_) => 1,
            Self::Spacer(height) => *height,
            Self::Checkbox(_) => 2,
            Self::DualCheckbox(_, _) => 2,
            Self::Path(_) => 3,
        }
    }

    fn focus(&self) -> Option<ProjectSettingsFocus> {
        match self {
            Self::Checkbox(field) | Self::Path(field) => Some(*field),
            Self::DualCheckbox(left, _) => Some(*left),
            _ => None,
        }
    }
}

pub(crate) fn open_browser_for_project_settings_focus(app: &mut App) -> Result<()> {
    sync_project_settings_state(app);
    let target = match app.project_settings_state.focus {
        ProjectSettingsFocus::ChangelogPath => BrowseTarget::ProjectSettingsChangelogPath,
        ProjectSettingsFocus::ReleaseNowWindows => BrowseTarget::ProjectSettingsReleaseNowWindows,
        ProjectSettingsFocus::ReleaseNowLinuxArm => BrowseTarget::ProjectSettingsReleaseNowLinuxArm,
        ProjectSettingsFocus::ReleaseNowLinuxAmd => BrowseTarget::ProjectSettingsReleaseNowLinuxAmd,
        ProjectSettingsFocus::ReleaseNowMacOs => BrowseTarget::ProjectSettingsReleaseNowMacOs,
        _ => return Ok(()),
    };
    app.open_browser(target)
}

pub(crate) fn initial_browser_path(app: &App, target: BrowseTarget) -> Option<String> {
    match target {
        BrowseTarget::ProjectSettingsChangelogPath => Some(
            app.project_settings_state
                .changelog_path
                .value()
                .to_string(),
        ),
        BrowseTarget::ProjectSettingsReleaseNowWindows => Some(
            app.project_settings_state
                .release_now_windows
                .value()
                .to_string(),
        ),
        BrowseTarget::ProjectSettingsReleaseNowLinuxArm => Some(
            app.project_settings_state
                .release_now_linux_arm
                .value()
                .to_string(),
        ),
        BrowseTarget::ProjectSettingsReleaseNowLinuxAmd => Some(
            app.project_settings_state
                .release_now_linux_amd
                .value()
                .to_string(),
        ),
        BrowseTarget::ProjectSettingsReleaseNowMacOs => Some(
            app.project_settings_state
                .release_now_macos
                .value()
                .to_string(),
        ),
        _ => None,
    }
}

pub(crate) fn apply_browser_selection(
    app: &mut App,
    target: BrowseTarget,
    value: String,
) -> Result<bool> {
    let field = match target {
        BrowseTarget::ProjectSettingsChangelogPath => ProjectSettingsFocus::ChangelogPath,
        BrowseTarget::ProjectSettingsReleaseNowWindows => ProjectSettingsFocus::ReleaseNowWindows,
        BrowseTarget::ProjectSettingsReleaseNowLinuxArm => ProjectSettingsFocus::ReleaseNowLinuxArm,
        BrowseTarget::ProjectSettingsReleaseNowLinuxAmd => ProjectSettingsFocus::ReleaseNowLinuxAmd,
        BrowseTarget::ProjectSettingsReleaseNowMacOs => ProjectSettingsFocus::ReleaseNowMacOs,
        _ => return Ok(false),
    };
    sync_project_settings_state(app);
    app.project_settings_state.focus = field;
    app.project_settings_state
        .set_value_from_browse(field, value);
    persist_project_settings_inputs(app)?;
    Ok(true)
}

fn render_project_settings_tabs(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(project) = app.config.projects.get(app.selected_project).cloned() else {
        return;
    };
    let scope_index = active_scope_index(&project, app.overview_focused_scope);
    let strip = ProjectSettingsTab::tab_strip(project.release_now_for_scope(scope_index).enabled);
    let labels: Vec<&str> = strip
        .iter()
        .map(|t| match t {
            ProjectSettingsTab::General => "General",
            ProjectSettingsTab::Distro => "Distro",
            ProjectSettingsTab::RlsQd => "RLS-QD",
        })
        .collect();
    let active_index = strip
        .iter()
        .position(|tab| *tab == app.project_settings_tab)
        .unwrap_or(0);
    let tabs = TabNav::new(&labels, active_index)
        .highlight_style(Style::default().fg(Color::Cyan))
        .border_style(Style::default().fg(Color::DarkGray))
        .style(Style::default().fg(Color::White))
        .indicator(None);
    frame.render_widget(tabs, area);

    let constraints: Vec<Constraint> = strip
        .iter()
        .map(|tab| {
            Constraint::Length(match tab {
                ProjectSettingsTab::General => 16,
                ProjectSettingsTab::Distro => 16,
                ProjectSettingsTab::RlsQd => 18,
            })
        })
        .collect();
    let rects = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area);
    for (idx, tab) in strip.iter().enumerate() {
        if let Some(rect) = rects.get(idx) {
            app.hit_targets.push(HitTarget::new(
                *rect,
                HitAction::SelectProjectSettingsTab(*tab),
            ));
        }
    }
}

fn render_general_settings(
    app: &mut App,
    frame: &mut Frame,
    area: Rect,
    project: &ProjectConfig,
    scope_index: usize,
) {
    render_scrollable_rows(
        app,
        frame,
        area,
        project,
        scope_index,
        &build_general_rows(project, scope_index),
    );
}

fn render_distro_settings(
    app: &mut App,
    frame: &mut Frame,
    area: Rect,
    project: &ProjectConfig,
    scope_index: usize,
) {
    render_scrollable_rows(
        app,
        frame,
        area,
        project,
        scope_index,
        &build_distro_rows(project, scope_index),
    );
}

fn render_scrollable_rows(
    app: &mut App,
    frame: &mut Frame,
    area: Rect,
    project: &ProjectConfig,
    scope_index: usize,
    rows: &[ProjectSettingsRow],
) {
    let gutter_width = if area.width > 3 { 1 } else { 0 };
    let content_area = Rect {
        x: area.x,
        y: area.y,
        width: area.width.saturating_sub(gutter_width),
        height: area.height,
    };
    let total_height = total_rows_height(rows);
    app.project_settings_state.viewport_height = content_area.height;
    if app.project_settings_state.follow_focus {
        if let Some((top, height)) = focused_row_bounds(rows, app.project_settings_state.focus) {
            app.project_settings_state.ensure_row_visible(
                top,
                height,
                total_height,
                content_area.height,
            );
        } else {
            app.project_settings_state
                .clamp_scroll(total_height, content_area.height);
        }
    } else if let Some((top, height)) = focused_row_bounds(rows, app.project_settings_state.focus) {
        let viewport_top = app.project_settings_state.scroll;
        let viewport_bottom = viewport_top.saturating_add(content_area.height);
        if top >= viewport_top && top.saturating_add(height) <= viewport_bottom {
            app.project_settings_state.follow_focus = true;
        }
        app.project_settings_state
            .clamp_scroll(total_height, content_area.height);
    } else {
        app.project_settings_state
            .clamp_scroll(total_height, content_area.height);
    }

    let mut cursor_y = 0u16;
    let scroll = app.project_settings_state.scroll;
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
            ProjectSettingsRow::Text(line) => {
                frame.render_widget(Paragraph::new(line.clone()), row_area);
            }
            ProjectSettingsRow::Spacer(_) => {}
            ProjectSettingsRow::Checkbox(field) if row_area.height >= 2 => {
                let focused = *field == app.project_settings_state.focus;
                render_checkbox_row(app, frame, row_area, *field, project, scope_index, focused);
            }
            ProjectSettingsRow::DualCheckbox(left, right) if row_area.height >= 2 => {
                render_dual_checkbox_row(app, frame, row_area, *left, *right, project, scope_index);
            }
            ProjectSettingsRow::Path(field) if row_area.height >= 3 => {
                let focused = *field == app.project_settings_state.focus;
                render_path_row(app, frame, row_area, *field, focused);
            }
            _ => {}
        }

        cursor_y = row_bottom;
    }

    if gutter_width == 1 && total_height > content_area.height {
        let indicator_x = area.x + area.width - 1;
        if app.project_settings_state.scroll > 0 {
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
            .project_settings_state
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

fn build_rows(
    tab: ProjectSettingsTab,
    project: &ProjectConfig,
    scope_index: usize,
) -> Vec<ProjectSettingsRow> {
    match tab {
        ProjectSettingsTab::General => build_general_rows(project, scope_index),
        ProjectSettingsTab::Distro => build_distro_rows(project, scope_index),
        ProjectSettingsTab::RlsQd => build_rls_qd_rows(project, scope_index),
    }
}

fn build_general_rows(project: &ProjectConfig, scope_index: usize) -> Vec<ProjectSettingsRow> {
    let mut rows = Vec::new();
    if project.integration_mode.requires_repo() {
        if project.repo_has_custom_main_branch_for_scope(scope_index) {
            rows.push(ProjectSettingsRow::Path(
                ProjectSettingsFocus::CustomMainBranchName,
            ));
        }
        rows.push(ProjectSettingsRow::Spacer(1));
        rows.push(ProjectSettingsRow::Checkbox(
            ProjectSettingsFocus::CustomMainBranchEnabled,
        ));
    }
    rows.extend([
        ProjectSettingsRow::Path(ProjectSettingsFocus::Alias),
        ProjectSettingsRow::Spacer(1),
        ProjectSettingsRow::Checkbox(ProjectSettingsFocus::ChangelogEnabled),
    ]);
    if project.changelog_enabled_for_scope(scope_index) {
        rows.push(ProjectSettingsRow::Path(
            ProjectSettingsFocus::ChangelogPath,
        ));
        rows.push(ProjectSettingsRow::Spacer(1));
        rows.push(ProjectSettingsRow::DualCheckbox(
            ProjectSettingsFocus::ChangelogHidePrMessages,
            ProjectSettingsFocus::ChangelogHideBumpMessages,
        ));
        rows.push(ProjectSettingsRow::Checkbox(
            ProjectSettingsFocus::ChangelogWrapDetailedIfTopPicks,
        ));
        rows.push(ProjectSettingsRow::Checkbox(
            ProjectSettingsFocus::ChangelogMiniCommitHashes,
        ));
        rows.push(ProjectSettingsRow::Spacer(1));
    }
    rows.extend([
		ProjectSettingsRow::Text(Line::from("This toggle now lives at the scope level.".yellow())),
		ProjectSettingsRow::Text(Line::from(if project.project_type == ProjectType::Branched {
			"Use the focused overview tile or click another tile to switch scopes."
		} else {
			"All-in-one projects apply this setting to the single project scope."
		})),
		ProjectSettingsRow::Text(Line::from("Press Space or Enter to toggle the selected checkbox. Ctrl+O opens Browse on path fields.")),
		ProjectSettingsRow::Text(Line::from("Up/Down or Tab/Shift+Tab moves between fields. Mouse wheel scrolls when content overflows.")),
	]);
    rows
}

fn build_distro_rows(project: &ProjectConfig, scope_index: usize) -> Vec<ProjectSettingsRow> {
    let mut rows = vec![
        ProjectSettingsRow::Text(
            Line::from(format!(
                "Scope: {}",
                active_scope_name(project, scope_index)
            ))
            .bold(),
        ),
        ProjectSettingsRow::Text(Line::from(format!(
            "Scope type: {}",
            active_scope_kind(project, scope_index)
        ))),
        ProjectSettingsRow::Text(Line::from(
            "Configure release-now script paths per scope. The feature is not wired into release execution yet.",
        )),
        ProjectSettingsRow::Spacer(1),
        ProjectSettingsRow::Checkbox(ProjectSettingsFocus::ReleaseNowEnabled),
    ];
    if project.release_now_for_scope(scope_index).enabled {
        rows.extend([
            ProjectSettingsRow::Path(ProjectSettingsFocus::ReleaseNowWindows),
            ProjectSettingsRow::Path(ProjectSettingsFocus::ReleaseNowLinuxArm),
            ProjectSettingsRow::Path(ProjectSettingsFocus::ReleaseNowLinuxAmd),
            ProjectSettingsRow::Path(ProjectSettingsFocus::ReleaseNowMacOs),
        ]);
    }
    rows.extend([
        ProjectSettingsRow::Spacer(1),
        ProjectSettingsRow::Text(Line::from(
            "When enabled, each platform path can point to a script or command wrapper.".yellow(),
        )),
        ProjectSettingsRow::Text(Line::from(
            "Browse selects a file path only; no validation or execution is performed yet.",
        )),
    ]);
    rows
}

fn render_rls_qd_settings(
    app: &mut App,
    frame: &mut Frame,
    area: Rect,
    project: &ProjectConfig,
    scope_index: usize,
) {
    render_scrollable_rows(
        app,
        frame,
        area,
        project,
        scope_index,
        &build_rls_qd_rows(project, scope_index),
    );
}

fn build_rls_qd_rows(project: &ProjectConfig, scope_index: usize) -> Vec<ProjectSettingsRow> {
    let mut rows = vec![
        ProjectSettingsRow::Text(
            Line::from(format!(
                "Scope: {}",
                active_scope_name(project, scope_index)
            ))
            .bold(),
        ),
        ProjectSettingsRow::Text(Line::from(
            "Quick-Downloads: HTML table injected into GitHub release notes during ReleaseNOW."
                .yellow(),
        )),
        ProjectSettingsRow::Spacer(1),
        ProjectSettingsRow::Checkbox(ProjectSettingsFocus::QuickDownloadsEnabled),
    ];
    if project
        .release_now_for_scope(scope_index)
        .quick_downloads
        .enabled
    {
        rows.push(ProjectSettingsRow::Path(
            ProjectSettingsFocus::QuickDownloadsPosition,
        ));
        rows.push(ProjectSettingsRow::Path(
            ProjectSettingsFocus::QuickDownloadsFooter,
        ));
    }
    rows.extend([
        ProjectSettingsRow::Spacer(1),
        ProjectSettingsRow::Text(Line::from(
            "Top: table is a prefix before your notes. Bottom: table is an appendix after notes.",
        )),
        ProjectSettingsRow::Text(Line::from(
            "Uses the scope Remote URL (GitHub SSH or HTTPS). Missing artifacts become non-linked cells.",
        )),
    ]);
    rows
}

fn total_rows_height(rows: &[ProjectSettingsRow]) -> u16 {
    rows.iter().map(ProjectSettingsRow::height).sum()
}

fn focused_row_bounds(
    rows: &[ProjectSettingsRow],
    focus: ProjectSettingsFocus,
) -> Option<(u16, u16)> {
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

fn render_checkbox_row(
    app: &mut App,
    frame: &mut Frame,
    area: Rect,
    field: ProjectSettingsFocus,
    project: &ProjectConfig,
    scope_index: usize,
    focused: bool,
) {
    let inset = control_inset(area);
    let enabled = match field {
        ProjectSettingsFocus::CustomMainBranchEnabled => {
            project.repo_has_custom_main_branch_for_scope(scope_index)
        }
        ProjectSettingsFocus::ChangelogEnabled => project.changelog_enabled_for_scope(scope_index),
        ProjectSettingsFocus::ReleaseNowEnabled => {
            project.release_now_for_scope(scope_index).enabled
        }
        ProjectSettingsFocus::QuickDownloadsEnabled => {
            project
                .release_now_for_scope(scope_index)
                .quick_downloads
                .enabled
        }
        ProjectSettingsFocus::ChangelogWrapDetailedIfTopPicks => {
            project.changelog_wrap_detailed_if_top_picks_for_scope(scope_index)
        }
        ProjectSettingsFocus::ChangelogMiniCommitHashes => {
            project.changelog_mini_commit_hashes_for_scope(scope_index)
        }
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
        HitAction::SelectProjectSettingsField(field),
    ));
}

fn render_dual_checkbox_row(
    app: &mut App,
    frame: &mut Frame,
    area: Rect,
    left_field: ProjectSettingsFocus,
    right_field: ProjectSettingsFocus,
    _project: &ProjectConfig,
    _scope_index: usize,
) {
    let inset = control_inset(area);
    // Split the area into two equal parts with space between them
    let halves = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(inset);

    // Get state for each checkbox from the app state
    let left_enabled = match left_field {
        ProjectSettingsFocus::ChangelogHidePrMessages => {
            app.project_settings_state.changelog_hide_pr_messages
        }
        _ => false,
    };
    let right_enabled = match right_field {
        ProjectSettingsFocus::ChangelogHideBumpMessages => {
            app.project_settings_state.changelog_hide_bump_messages
        }
        _ => false,
    };
    // Note: ChangelogMiniCommitHashes is rendered as a standalone Checkbox, not DualCheckbox

    let left_focused = left_field == app.project_settings_state.focus;
    let right_focused = right_field == app.project_settings_state.focus;

    // Render left checkbox
    let left_checkbox = Checkbox::new(checkbox_label(left_field), left_enabled)
        .checked_symbol("✅ ")
        .unchecked_symbol("❌ ")
        .style(if left_focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::White)
        })
        .checkbox_style(Style::default().fg(if left_enabled {
            Color::Green
        } else {
            Color::Red
        }))
        .label_style(if left_focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::White)
        });
    frame.render_widget(left_checkbox, halves[0]);
    app.hit_targets.push(HitTarget::new(
        halves[0],
        HitAction::SelectProjectSettingsField(left_field),
    ));

    // Render right checkbox
    let right_checkbox = Checkbox::new(checkbox_label(right_field), right_enabled)
        .checked_symbol("✅ ")
        .unchecked_symbol("❌ ")
        .style(if right_focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::White)
        })
        .checkbox_style(Style::default().fg(if right_enabled {
            Color::Green
        } else {
            Color::Red
        }))
        .label_style(if right_focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::White)
        });
    frame.render_widget(right_checkbox, halves[1]);
    app.hit_targets.push(HitTarget::new(
        halves[1],
        HitAction::SelectProjectSettingsField(right_field),
    ));
}

fn render_path_form_row(
    frame: &mut Frame,
    area: Rect,
    label: &'static str,
    value: Line,
    focused: bool,
    side_button: Option<FormRowButton>,
) -> Option<Rect> {
    let label_area = center_vertically(
        Rect {
            x: area.x,
            y: area.y,
            width: FORM_LABEL_WIDTH,
            height: area.height,
        },
        1,
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            label,
            Style::default().fg(Color::Rgb(220, 220, 220)),
        ))),
        label_area,
    );

    let row = if side_button.is_some() {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(FORM_LABEL_WIDTH),
                Constraint::Min(10),
                Constraint::Length(1),
                Constraint::Length(BROWSE_BUTTON_WIDTH),
            ])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(FORM_LABEL_WIDTH), Constraint::Min(10)])
            .split(area)
    };

    let field_area = center_vertically(row[1], area.height.min(3));
    let block = if focused {
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
    } else {
        Block::default().borders(Borders::ALL)
    };
    frame.render_widget(Paragraph::new(Text::from(value)).block(block), field_area);

    if let Some(button) = side_button {
        let button_area = center_vertically(row[3], area.height.min(3));
        frame.render_widget(
            Paragraph::new(button.label)
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::Black).bg(Color::Cyan))
                .block(Block::default().borders(Borders::ALL)),
            button_area,
        );
        Some(button_area)
    } else {
        None
    }
}

fn render_path_row(
    app: &mut App,
    frame: &mut Frame,
    area: Rect,
    field: ProjectSettingsFocus,
    focused: bool,
) {
    let inset = control_inset(area);
    let side_button = match field {
        ProjectSettingsFocus::Alias | ProjectSettingsFocus::CustomMainBranchName => None,
        ProjectSettingsFocus::QuickDownloadsPosition
        | ProjectSettingsFocus::QuickDownloadsFooter => None,
        _ => Some(FormRowButton::new(
            "Browse",
            HitAction::BrowseProjectSettingsField(field),
        )),
    };
    let value = app.project_settings_state.display_value_for_field(
        field,
        focused,
        visible_field_width(inset.width, side_button.is_some()),
    );
    let button_rect = render_path_form_row(
        frame,
        inset,
        field_label(field),
        value,
        focused,
        side_button.clone(),
    );
    app.hit_targets.push(HitTarget::new(
        inset,
        HitAction::SelectProjectSettingsField(field),
    ));
    if let (Some(rect), Some(button)) = (button_rect, side_button) {
        app.hit_targets.push(HitTarget::new(rect, button.action));
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

fn checkbox_label(field: ProjectSettingsFocus) -> &'static str {
    match field {
        ProjectSettingsFocus::CustomMainBranchEnabled => {
            "This repo has a custom named main branch."
        }
        ProjectSettingsFocus::ChangelogEnabled => "Changelog Generation",
        ProjectSettingsFocus::ChangelogHidePrMessages => "Hide PR messages",
        ProjectSettingsFocus::ChangelogHideBumpMessages => "Hide bump messages",
        ProjectSettingsFocus::ChangelogWrapDetailedIfTopPicks => {
            "Wrap detailed changelog if TopPicks present"
        }
        ProjectSettingsFocus::ChangelogMiniCommitHashes => "Mini commit hashes",
        ProjectSettingsFocus::ReleaseNowEnabled => {
            "Enable Release-NOW capabilities for this project/scope"
        }
        ProjectSettingsFocus::QuickDownloadsEnabled => "Quick-Downloads Enabled",
        _ => "",
    }
}

fn field_label(field: ProjectSettingsFocus) -> &'static str {
    match field {
        ProjectSettingsFocus::CustomMainBranchName => "Custom main branch",
        ProjectSettingsFocus::Alias => "Alias",
        ProjectSettingsFocus::ChangelogPath => "Changelog path",
        ProjectSettingsFocus::ChangelogHidePrMessages => "Hide PR messages",
        ProjectSettingsFocus::ChangelogHideBumpMessages => "Hide bump messages",
        ProjectSettingsFocus::ReleaseNowWindows => "Windows",
        ProjectSettingsFocus::ReleaseNowLinuxArm => "Linux ARM",
        ProjectSettingsFocus::ReleaseNowLinuxAmd => "Linux AMD",
        ProjectSettingsFocus::ReleaseNowMacOs => "MacOS",
        ProjectSettingsFocus::QuickDownloadsPosition => "Position (←/→)",
        ProjectSettingsFocus::QuickDownloadsFooter => "Footer",
        _ => "",
    }
}

fn is_checkbox_field(field: ProjectSettingsFocus) -> bool {
    matches!(
        field,
        ProjectSettingsFocus::CustomMainBranchEnabled
            | ProjectSettingsFocus::ChangelogEnabled
            | ProjectSettingsFocus::ChangelogHidePrMessages
            | ProjectSettingsFocus::ChangelogHideBumpMessages
            | ProjectSettingsFocus::ChangelogWrapDetailedIfTopPicks
            | ProjectSettingsFocus::ChangelogMiniCommitHashes
            | ProjectSettingsFocus::ReleaseNowEnabled
            | ProjectSettingsFocus::QuickDownloadsEnabled
    )
}

fn toggle_focused_project_settings_control(app: &mut App) -> Result<()> {
    let Some(project) = app.config.projects.get(app.selected_project).cloned() else {
        return Ok(());
    };
    let scope_index = active_scope_index(&project, app.overview_focused_scope);
    let scope_name = active_scope_name(&project, scope_index);
    let active_project = app
        .config
        .projects
        .get_mut(app.selected_project)
        .expect("selected project checked above");

    match app.project_settings_state.focus {
        ProjectSettingsFocus::CustomMainBranchEnabled => {
            let next_enabled = !active_project.repo_has_custom_main_branch_for_scope(scope_index);
            let custom_main_branch = app
                .project_settings_state
                .custom_main_branch_name
                .value()
                .to_string();
            active_project.set_repo_custom_main_branch_for_scope(
                scope_index,
                next_enabled,
                custom_main_branch,
            )?;
            app.status = super::StatusMessage::success(format!(
                "Custom main branch {} for {}.",
                if next_enabled { "enabled" } else { "disabled" },
                scope_name
            ));
        }
        ProjectSettingsFocus::ChangelogEnabled => {
            let next_enabled = !active_project.changelog_enabled_for_scope(scope_index);
            active_project.set_changelog_enabled_for_scope(scope_index, next_enabled);
            app.status = super::StatusMessage::success(format!(
                "Changelog generation {} for {}.",
                if next_enabled { "enabled" } else { "disabled" },
                scope_name
            ));
        }
        ProjectSettingsFocus::ReleaseNowEnabled => {
            let settings = active_project.release_now_for_scope_mut(scope_index);
            settings.enabled = !settings.enabled;
            if !settings.enabled && app.project_settings_tab == ProjectSettingsTab::RlsQd {
                app.project_settings_tab = ProjectSettingsTab::Distro;
            }
            app.status = super::StatusMessage::success(format!(
                "Release-NOW capabilities {} for {}.",
                if settings.enabled {
                    "enabled"
                } else {
                    "disabled"
                },
                scope_name
            ));
        }
        ProjectSettingsFocus::QuickDownloadsEnabled => {
            let qd = &mut active_project
                .release_now_for_scope_mut(scope_index)
                .quick_downloads;
            qd.enabled = !qd.enabled;
            app.status = super::StatusMessage::success(format!(
                "Quick-Downloads {} for {}.",
                if qd.enabled { "enabled" } else { "disabled" },
                scope_name
            ));
        }
        ProjectSettingsFocus::QuickDownloadsPosition => {
            let settings_mut = active_project.release_now_for_scope_mut(scope_index);
            let next = settings_mut.quick_downloads.position.toggle();
            settings_mut.quick_downloads.position = next;
            app.project_settings_state
                .quick_downloads_position
                .set_value(next.display_name().to_string());
            app.status = super::StatusMessage::success(format!(
                "Quick-Downloads position set to {} for {}.",
                next.display_name(),
                scope_name
            ));
        }
        ProjectSettingsFocus::ChangelogHidePrMessages => {
            let next = !app.project_settings_state.changelog_hide_pr_messages;
            app.project_settings_state.changelog_hide_pr_messages = next;
            active_project.set_changelog_hide_pr_messages_for_scope(scope_index, next);
            app.status = super::StatusMessage::success(format!(
                "PR messages {} for {}.",
                if next { "hidden" } else { "shown" },
                scope_name
            ));
        }
        ProjectSettingsFocus::ChangelogHideBumpMessages => {
            let next = !app.project_settings_state.changelog_hide_bump_messages;
            app.project_settings_state.changelog_hide_bump_messages = next;
            active_project.set_changelog_hide_bump_messages_for_scope(scope_index, next);
            app.status = super::StatusMessage::success(format!(
                "Bump messages {} for {}.",
                if next { "hidden" } else { "shown" },
                scope_name
            ));
        }
        ProjectSettingsFocus::ChangelogWrapDetailedIfTopPicks => {
            let next = !app
                .project_settings_state
                .changelog_wrap_detailed_if_top_picks;
            app.project_settings_state
                .changelog_wrap_detailed_if_top_picks = next;
            active_project.set_changelog_wrap_detailed_if_top_picks_for_scope(scope_index, next);
            app.status = super::StatusMessage::success(format!(
                "Wrap detailed changelog {} for {}.",
                if next { "enabled" } else { "disabled" },
                scope_name
            ));
        }
        ProjectSettingsFocus::ChangelogMiniCommitHashes => {
            let next = !app.project_settings_state.changelog_mini_commit_hashes;
            app.project_settings_state.changelog_mini_commit_hashes = next;
            active_project.set_changelog_mini_commit_hashes_for_scope(scope_index, next);
            app.status = super::StatusMessage::success(format!(
                "Mini commit hashes {} for {}.",
                if next { "enabled" } else { "disabled" },
                scope_name
            ));
        }
        _ => return Ok(()),
    }

    app.config_store.save(&app.config)?;
    let updated_project = app
        .config
        .projects
        .get(app.selected_project)
        .cloned()
        .expect("selected project present");
    app.project_settings_state.ensure_focus_visible(
        app.project_settings_tab,
        &updated_project,
        scope_index,
    );
    Ok(())
}

fn persist_project_settings_inputs(app: &mut App) -> Result<()> {
    let Some(project) = app.config.projects.get(app.selected_project).cloned() else {
        return Ok(());
    };
    let scope_index = active_scope_index(&project, app.overview_focused_scope);
    let custom_main_branch = app
        .project_settings_state
        .custom_main_branch_name
        .value()
        .to_string();
    let alias = app.project_settings_state.alias.value().trim().to_string();
    let changelog_path = app
        .project_settings_state
        .changelog_path
        .value()
        .to_string();
    let windows_script = app
        .project_settings_state
        .release_now_windows
        .value()
        .to_string();
    let linux_arm_script = app
        .project_settings_state
        .release_now_linux_arm
        .value()
        .to_string();
    let linux_amd_script = app
        .project_settings_state
        .release_now_linux_amd
        .value()
        .to_string();
    let macos_script = app
        .project_settings_state
        .release_now_macos
        .value()
        .to_string();
    let qd_footer = app
        .project_settings_state
        .quick_downloads_footer
        .value()
        .to_string();

    let active_project = app
        .config
        .projects
        .get_mut(app.selected_project)
        .expect("selected project checked above");
    let custom_main_branch_enabled =
        active_project.repo_has_custom_main_branch_for_scope(scope_index);
    if active_project.integration_mode.requires_repo()
        && (custom_main_branch_enabled
            || active_project.repo_config_for_scope(scope_index).is_some())
    {
        active_project.set_repo_custom_main_branch_for_scope(
            scope_index,
            custom_main_branch_enabled,
            custom_main_branch,
        )?;
    }
    active_project.alias = alias;
    active_project.set_changelog_path_for_scope(scope_index, changelog_path);
    let release_now = active_project.release_now_for_scope_mut(scope_index);
    release_now.windows_script = windows_script;
    release_now.linux_arm_script = linux_arm_script;
    release_now.linux_amd_script = linux_amd_script;
    release_now.macos_script = macos_script;
    release_now.quick_downloads.footer_message = qd_footer;
    app.config_store.save(&app.config)?;
    Ok(())
}

fn active_scope_index(project: &ProjectConfig, focused_scope: usize) -> usize {
    if project.project_type == ProjectType::Branched {
        focused_scope.min(project.branches.len().saturating_sub(1))
    } else {
        0
    }
}

fn active_scope_name(project: &ProjectConfig, scope_index: usize) -> String {
    if project.project_type == ProjectType::Branched {
        project
            .branches
            .get(scope_index)
            .map(|branch| branch.display_name().to_string())
            .unwrap_or_else(|| "Selected scope".to_string())
    } else {
        project.name.clone()
    }
}

fn active_scope_kind(project: &ProjectConfig, scope_index: usize) -> &'static str {
    if project.project_type == ProjectType::Branched {
        project
            .branches
            .get(scope_index)
            .map(|branch| branch.scope_kind.display_name())
            .unwrap_or("Scope")
    } else {
        "Project"
    }
}
