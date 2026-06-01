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
    layout::{Alignment, Constraint, Direction, Flex, Layout, Rect},
    style::{Color, Style, Stylize},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, StatefulWidget},
};
use tui_checkbox::Checkbox;

use super::{
    App, BROWSE_BUTTON_WIDTH, BrowseTarget, FORM_LABEL_WIDTH, FormRowButton, HitAction, HitTarget,
    visible_field_width,
};
use crate::{
    config::{
        DEFAULT_CHANGELOG_PATH, MirrorSyncAfterMerge, PostMergeSourceBranch, ProjectConfig,
        ProjectType, ReadmeInjectDepth,
    },
    tui::{center_vertically, project_settings_tab_nav},
    workflow::dialogs::TextInput,
};

use super::ps_alias::{
    append_alias_visible_fields, append_general_alias_rows, confirm_alias_custom_draft,
    delete_alias_custom, persist_alias_state_to_project, render_alias_row,
    set_alias_path_from_browse, sync_alias_state_from_project,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProjectSettingsTab {
    General,
    Git,
    Changelogs,
    Distro,
    RlsQd,
}

pub(crate) fn project_settings_tab_strip(
    project: &ProjectConfig,
    release_now_enabled: bool,
) -> Vec<ProjectSettingsTab> {
    let mut tabs = vec![ProjectSettingsTab::General];
    if project.integration_mode.requires_repo() {
        tabs.push(ProjectSettingsTab::Git);
    }
    tabs.push(ProjectSettingsTab::Changelogs);
    tabs.push(ProjectSettingsTab::Distro);
    if release_now_enabled {
        tabs.push(ProjectSettingsTab::RlsQd);
    }
    tabs
}

/// Merges a saved tab order with the default strip (drops removed tabs, appends new ones).
pub(crate) fn merge_project_settings_tab_order(
    default: &[ProjectSettingsTab],
    saved: &[ProjectSettingsTab],
) -> Vec<ProjectSettingsTab> {
    if saved.is_empty() {
        return default.to_vec();
    }
    let mut order: Vec<_> = saved
        .iter()
        .copied()
        .filter(|tab| default.contains(tab))
        .collect();
    for tab in default {
        if !order.contains(tab) {
            order.push(*tab);
        }
    }
    order
}

pub(crate) fn project_settings_tab_pins(strip: &[ProjectSettingsTab]) -> Vec<bool> {
    strip
        .iter()
        .map(|tab| *tab == ProjectSettingsTab::General)
        .collect()
}

impl ProjectSettingsTab {
    pub(crate) fn step(
        self,
        delta: isize,
        project: &ProjectConfig,
        release_now_enabled: bool,
    ) -> Self {
        let tabs = project_settings_tab_strip(project, release_now_enabled);
        let index = tabs.iter().position(|tab| *tab == self).unwrap_or(0) as isize;
        tabs[(index + delta).rem_euclid(tabs.len() as isize) as usize]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProjectSettingsFocus {
    ComfyGitFlowEnabled,
    CustomMainBranchEnabled,
    CustomMainBranchName,
    Alias,
    AdvancedAliasEnabled,
    AliasDistPath,
    AliasUiPath,
    AliasCustomAdd,
    AliasCustomDraftName,
    AliasCustomDraftConfirm,
    AliasCustomPath(u16),
    AliasCustomDelete(u16),
    ChangelogEnabled,
    ChangelogPath,
    ChangelogHidePrMessages,
    ChangelogHideBumpMessages,
    ChangelogMiniCommitHashes,
    ChangelogMirrorSummaryToRootChangelog,
    ChangelogWrapDetailedIfTopPicks,
    ReleaseNowEnabled,
    ReleaseNowGeneral,
    ReleaseNowWindows,
    ReleaseNowLinuxArm,
    ReleaseNowLinuxAmd,
    ReleaseNowMacOs,
    QuickDownloadsEnabled,
    QuickDownloadsPosition,
    PostMergeSourceBranch,
    MirrorSyncAfterMerge,
    QuickDownloadsFooter,
    ReadmeInjectionEnabled,
    ReadmeInjectOnlyTopPicks,
    ReadmeInjectAtRow,
    ReadmeInjectDepthCurrentOnly,
    ReadmeInjectDepthLast3,
    ReadmeInjectDepthLast5,
    ReadmeInjectDepthLast10,
    ReleaseTitleTemplate,
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
    pub(crate) advanced_alias_enabled: bool,
    pub(crate) alias_dist_path: TextInput,
    pub(crate) alias_ui_path: TextInput,
    pub(crate) alias_custom: Vec<super::ps_alias::AliasCustomEntryState>,
    pub(crate) alias_custom_draft_active: bool,
    pub(crate) alias_custom_draft_name: TextInput,
    pub(crate) changelog_path: TextInput,
    pub(crate) changelog_hide_pr_messages: bool,
    pub(crate) changelog_hide_bump_messages: bool,
    pub(crate) changelog_mini_commit_hashes: bool,
    pub(crate) changelog_mirror_summary_to_root_changelog: bool,
    pub(crate) changelog_wrap_detailed_if_top_picks: bool,
    pub(crate) release_now_general: TextInput,
    pub(crate) release_now_windows: TextInput,
    pub(crate) release_now_linux_arm: TextInput,
    pub(crate) release_now_linux_amd: TextInput,
    pub(crate) release_now_macos: TextInput,
    pub(crate) quick_downloads_position: TextInput,
    pub(crate) post_merge_source_branch: TextInput,
    pub(crate) mirror_sync_after_merge: TextInput,
    pub(crate) quick_downloads_footer: TextInput,
    pub(crate) readme_inject_at_row: TextInput,
    pub(crate) release_title_template: TextInput,
}

impl Default for ProjectSettingsState {
    fn default() -> Self {
        Self {
            binding: None,
            focus: ProjectSettingsFocus::ComfyGitFlowEnabled,
            scroll: 0,
            viewport_height: 0,
            follow_focus: true,
            custom_main_branch_name: TextInput::with_value(""),
            alias: TextInput::with_value(""),
            advanced_alias_enabled: false,
            alias_dist_path: TextInput::with_value(""),
            alias_ui_path: TextInput::with_value(""),
            alias_custom: Vec::new(),
            alias_custom_draft_active: false,
            alias_custom_draft_name: TextInput::with_value(""),
            changelog_path: TextInput::with_value(DEFAULT_CHANGELOG_PATH),
            changelog_hide_pr_messages: false,
            changelog_hide_bump_messages: false,
            changelog_mini_commit_hashes: false,
            changelog_mirror_summary_to_root_changelog: false,
            changelog_wrap_detailed_if_top_picks: false,
            release_now_general: TextInput::with_value(""),
            release_now_windows: TextInput::with_value(""),
            release_now_linux_arm: TextInput::with_value(""),
            release_now_linux_amd: TextInput::with_value(""),
            release_now_macos: TextInput::with_value(""),
            quick_downloads_position: TextInput::with_value(""),
            post_merge_source_branch: TextInput::with_value(""),
            mirror_sync_after_merge: TextInput::with_value(""),
            quick_downloads_footer: TextInput::with_value(""),
            readme_inject_at_row: TextInput::with_value(""),
            release_title_template: TextInput::with_value(""),
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
        sync_alias_state_from_project(self, project, scope_index);
        self.changelog_path
            .set_value(project.changelog_path_for_scope(scope_index).to_string());
        self.changelog_hide_pr_messages = project.changelog_hide_pr_messages_for_scope(scope_index);
        self.changelog_hide_bump_messages =
            project.changelog_hide_bump_messages_for_scope(scope_index);
        self.changelog_mini_commit_hashes =
            project.changelog_mini_commit_hashes_for_scope(scope_index);
        self.changelog_mirror_summary_to_root_changelog =
            project.changelog_mirror_summary_to_root_changelog_for_scope(scope_index);
        self.changelog_wrap_detailed_if_top_picks =
            project.changelog_wrap_detailed_if_top_picks_for_scope(scope_index);
        self.release_now_general
            .set_value(release_now.general_script.clone());
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
        self.post_merge_source_branch.set_value(
            project
                .post_merge_source_branch_for_scope(scope_index)
                .display_name()
                .to_string(),
        );
        self.mirror_sync_after_merge.set_value(
            project
                .mirror_sync_after_merge_for_scope(scope_index)
                .display_name()
                .to_string(),
        );
        self.quick_downloads_footer
            .set_value(qd.footer_message.clone());
        let rls = project.release_now_for_scope(scope_index);
        self.readme_inject_at_row
            .set_value(if rls.readme_injection_enabled {
                rls.readme_inject_at_row.to_string()
            } else {
                String::new()
            });
        self.release_title_template
            .set_value(rls.release_title_template.clone());
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
                let mut fields = vec![ProjectSettingsFocus::ComfyGitFlowEnabled];
                if project.integration_mode.requires_repo() {
                    fields.push(ProjectSettingsFocus::CustomMainBranchEnabled);
                    if project.repo_has_custom_main_branch_for_scope(scope_index) {
                        fields.push(ProjectSettingsFocus::CustomMainBranchName);
                    }
                }
                fields.push(ProjectSettingsFocus::Alias);
                append_alias_visible_fields(&mut fields, project, scope_index, self);
                fields
            }
            ProjectSettingsTab::Git => git_visible_fields(project, scope_index),
            ProjectSettingsTab::Changelogs => changelog_visible_fields(project, scope_index),
            ProjectSettingsTab::Distro => {
                let mut fields = vec![ProjectSettingsFocus::ReleaseNowEnabled];
                if project.release_now_for_scope(scope_index).enabled {
                    fields.extend([
                        ProjectSettingsFocus::ReleaseNowGeneral,
                        ProjectSettingsFocus::ReleaseNowWindows,
                        ProjectSettingsFocus::ReleaseNowLinuxArm,
                        ProjectSettingsFocus::ReleaseNowLinuxAmd,
                        ProjectSettingsFocus::ReleaseNowMacOs,
                        ProjectSettingsFocus::ReleaseTitleTemplate,
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
            self.focus = *fields
                .first()
                .unwrap_or(&ProjectSettingsFocus::ComfyGitFlowEnabled);
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
                    | ProjectSettingsFocus::AliasDistPath
                    | ProjectSettingsFocus::AliasUiPath
                    | ProjectSettingsFocus::AliasCustomPath(_)
                    | ProjectSettingsFocus::AliasCustomDraftName
                    | ProjectSettingsFocus::ChangelogPath
                    | ProjectSettingsFocus::ReleaseNowGeneral
                    | ProjectSettingsFocus::ReleaseNowWindows
                    | ProjectSettingsFocus::ReleaseNowLinuxArm
                    | ProjectSettingsFocus::ReleaseNowLinuxAmd
                    | ProjectSettingsFocus::ReleaseNowMacOs
                    | ProjectSettingsFocus::QuickDownloadsFooter
                    | ProjectSettingsFocus::ReadmeInjectAtRow
                    | ProjectSettingsFocus::ReleaseTitleTemplate
            )
    }

    pub(crate) fn active_input_mut(&mut self) -> Option<&mut TextInput> {
        match self.focus {
            ProjectSettingsFocus::CustomMainBranchName => Some(&mut self.custom_main_branch_name),
            ProjectSettingsFocus::Alias => Some(&mut self.alias),
            ProjectSettingsFocus::AliasDistPath => Some(&mut self.alias_dist_path),
            ProjectSettingsFocus::AliasUiPath => Some(&mut self.alias_ui_path),
            ProjectSettingsFocus::AliasCustomPath(index) => self
                .alias_custom
                .get_mut(index as usize)
                .map(|entry| &mut entry.path),
            ProjectSettingsFocus::AliasCustomDraftName => Some(&mut self.alias_custom_draft_name),
            ProjectSettingsFocus::ChangelogPath => Some(&mut self.changelog_path),
            ProjectSettingsFocus::ReleaseNowGeneral => Some(&mut self.release_now_general),
            ProjectSettingsFocus::ReleaseNowWindows => Some(&mut self.release_now_windows),
            ProjectSettingsFocus::ReleaseNowLinuxArm => Some(&mut self.release_now_linux_arm),
            ProjectSettingsFocus::ReleaseNowLinuxAmd => Some(&mut self.release_now_linux_amd),
            ProjectSettingsFocus::ReleaseNowMacOs => Some(&mut self.release_now_macos),
            ProjectSettingsFocus::QuickDownloadsFooter => Some(&mut self.quick_downloads_footer),
            ProjectSettingsFocus::ReadmeInjectAtRow => Some(&mut self.readme_inject_at_row),
            ProjectSettingsFocus::ReleaseTitleTemplate => Some(&mut self.release_title_template),
            _ => None,
        }
    }

    fn handle_text_input(&mut self, key: KeyEvent) {
        if self.focus == ProjectSettingsFocus::ReadmeInjectAtRow
            && let crossterm::event::KeyCode::Char(c) = key.code
            && !c.is_ascii_digit()
        {
            return;
        }
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
            ProjectSettingsFocus::AliasDistPath => self
                .alias_dist_path
                .display_line_with_width(focused, max_width),
            ProjectSettingsFocus::AliasUiPath => self
                .alias_ui_path
                .display_line_with_width(focused, max_width),
            ProjectSettingsFocus::AliasCustomPath(index) => self
                .alias_custom
                .get(index as usize)
                .map(|entry| entry.path.display_line_with_width(focused, max_width))
                .unwrap_or_else(|| Line::from(String::new())),
            ProjectSettingsFocus::AliasCustomDraftName => self
                .alias_custom_draft_name
                .display_line_with_width(focused, max_width),
            ProjectSettingsFocus::ChangelogPath => self
                .changelog_path
                .display_line_with_width(focused, max_width),
            ProjectSettingsFocus::ReleaseNowGeneral => self
                .release_now_general
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
            ProjectSettingsFocus::PostMergeSourceBranch => Line::from(format!(
                "< {} >",
                self.post_merge_source_branch.value().trim()
            )),
            ProjectSettingsFocus::MirrorSyncAfterMerge => Line::from(format!(
                "< {} >",
                self.mirror_sync_after_merge.value().trim()
            )),
            ProjectSettingsFocus::QuickDownloadsFooter => self
                .quick_downloads_footer
                .display_line_with_width(focused, max_width),
            ProjectSettingsFocus::ReadmeInjectAtRow => self
                .readme_inject_at_row
                .display_line_with_width(focused, max_width),
            ProjectSettingsFocus::ReleaseTitleTemplate => self
                .release_title_template
                .display_line_with_width(focused, max_width),
            _ => Line::from(String::new()),
        }
    }

    fn set_value_from_browse(&mut self, field: ProjectSettingsFocus, value: String) {
        match field {
            ProjectSettingsFocus::ChangelogPath => self.changelog_path.set_value(value),
            ProjectSettingsFocus::ReleaseNowGeneral => self.release_now_general.set_value(value),
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
        ProjectSettingsTab::Git => {
            render_git_settings(app, frame, sections[1], &project, scope_index)
        }
        ProjectSettingsTab::Changelogs => {
            render_changelogs_settings(app, frame, sections[1], &project, scope_index)
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
    if app.project_settings_tab == ProjectSettingsTab::Git
        && !project.integration_mode.requires_repo()
    {
        app.project_settings_tab = ProjectSettingsTab::General;
    }
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
    app.project_settings_tab = app
        .project_settings_tab
        .step(delta, &project, release_now_enabled);
    app.flash_project_settings_tab_selection();
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
        KeyCode::Left
            if app.project_settings_state.focus == ProjectSettingsFocus::PostMergeSourceBranch =>
        {
            adjust_post_merge_source_branch(app, PostMergeSourceBranch::previous)?;
            Ok(true)
        }
        KeyCode::Right
            if app.project_settings_state.focus == ProjectSettingsFocus::PostMergeSourceBranch =>
        {
            adjust_post_merge_source_branch(app, PostMergeSourceBranch::next)?;
            Ok(true)
        }
        KeyCode::Left
            if app.project_settings_state.focus == ProjectSettingsFocus::MirrorSyncAfterMerge =>
        {
            adjust_mirror_sync_after_merge(app, MirrorSyncAfterMerge::previous)?;
            Ok(true)
        }
        KeyCode::Right
            if app.project_settings_state.focus == ProjectSettingsFocus::MirrorSyncAfterMerge =>
        {
            adjust_mirror_sync_after_merge(app, MirrorSyncAfterMerge::next)?;
            Ok(true)
        }
        KeyCode::Left | KeyCode::Right
            if app.project_settings_state.focus == ProjectSettingsFocus::QuickDownloadsPosition =>
        {
            toggle_focused_project_settings_control(app)?;
            Ok(true)
        }
        KeyCode::Enter | KeyCode::Char(' ')
            if matches!(
                app.project_settings_state.focus,
                ProjectSettingsFocus::AliasCustomAdd
                    | ProjectSettingsFocus::AliasCustomDraftConfirm
            ) =>
        {
            activate_project_settings_field(app, app.project_settings_state.focus)?;
            Ok(true)
        }
        KeyCode::Enter | KeyCode::Char(' ')
            if !matches!(
                app.project_settings_state.focus,
                ProjectSettingsFocus::QuickDownloadsPosition
                    | ProjectSettingsFocus::PostMergeSourceBranch
                    | ProjectSettingsFocus::MirrorSyncAfterMerge
            ) =>
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
    if is_checkbox_field(focus) || is_readme_inject_depth_field(focus) {
        return toggle_focused_project_settings_control(app);
    }
    if focus == ProjectSettingsFocus::AliasCustomAdd {
        app.project_settings_state.alias_custom_draft_active = true;
        app.project_settings_state.focus = ProjectSettingsFocus::AliasCustomDraftName;
        app.project_settings_state
            .alias_custom_draft_name
            .clear_selection();
        app.project_settings_state.follow_focus = true;
        return Ok(());
    }
    if focus == ProjectSettingsFocus::AliasCustomDraftName {
        app.project_settings_state
            .alias_custom_draft_name
            .clear_selection();
    }
    if focus == ProjectSettingsFocus::AliasCustomDraftConfirm {
        if let Some(message) = confirm_alias_custom_draft(app) {
            app.status = super::StatusMessage::error(message);
        } else {
            let _ = persist_project_settings_inputs(app);
        }
        return Ok(());
    }
    if let ProjectSettingsFocus::AliasCustomDelete(index) = focus {
        delete_alias_custom(app, index);
        return persist_project_settings_inputs(app);
    }
    Ok(())
}

pub(crate) fn scroll_project_settings(app: &mut App, delta: isize) {
    sync_project_settings_state(app);
    let Some(project) = app.config.projects.get(app.selected_project).cloned() else {
        return;
    };
    let scope_index = active_scope_index(&project, app.overview_focused_scope);
    let rows = build_rows(
        app.project_settings_tab,
        &project,
        scope_index,
        &app.project_settings_state,
    );
    let total_height = total_rows_height(&rows);
    let viewport_height = app.project_settings_state.viewport_height;
    app.project_settings_state
        .scroll_by(delta, total_height, viewport_height);
}

#[derive(Clone)]
pub(crate) enum ProjectSettingsRow {
    Text(Line<'static>),
    Spacer(u16),
    Checkbox(ProjectSettingsFocus),
    DualCheckbox(ProjectSettingsFocus, ProjectSettingsFocus),
    Path(ProjectSettingsFocus),
    InjectDepth,
    AliasCustom { index: u16 },
    AliasCustomDraft,
    AliasAddButton,
}

impl ProjectSettingsRow {
    fn height(&self) -> u16 {
        match self {
            Self::Text(_) => 1,
            Self::Spacer(height) => *height,
            Self::Checkbox(_) => 2,
            Self::DualCheckbox(_, _) => 2,
            Self::Path(_) => 3,
            Self::InjectDepth => 2,
            Self::AliasCustom { .. } | Self::AliasCustomDraft => 3,
            Self::AliasAddButton => 3,
        }
    }

    fn focus(&self) -> Option<ProjectSettingsFocus> {
        match self {
            Self::Checkbox(field) | Self::Path(field) => Some(*field),
            Self::DualCheckbox(left, _) => Some(*left),
            Self::InjectDepth => Some(ProjectSettingsFocus::ReadmeInjectDepthCurrentOnly),
            Self::AliasCustom { index } => Some(ProjectSettingsFocus::AliasCustomPath(*index)),
            Self::AliasCustomDraft => Some(ProjectSettingsFocus::AliasCustomDraftName),
            Self::AliasAddButton => Some(ProjectSettingsFocus::AliasCustomAdd),
            _ => None,
        }
    }
}

fn readme_inject_depth_from_focus(focus: ProjectSettingsFocus) -> Option<ReadmeInjectDepth> {
    match focus {
        ProjectSettingsFocus::ReadmeInjectDepthCurrentOnly => Some(ReadmeInjectDepth::CurrentOnly),
        ProjectSettingsFocus::ReadmeInjectDepthLast3 => Some(ReadmeInjectDepth::Last3),
        ProjectSettingsFocus::ReadmeInjectDepthLast5 => Some(ReadmeInjectDepth::Last5),
        ProjectSettingsFocus::ReadmeInjectDepthLast10 => Some(ReadmeInjectDepth::Last10),
        _ => None,
    }
}

fn focus_for_readme_inject_depth(depth: ReadmeInjectDepth) -> ProjectSettingsFocus {
    match depth {
        ReadmeInjectDepth::CurrentOnly => ProjectSettingsFocus::ReadmeInjectDepthCurrentOnly,
        ReadmeInjectDepth::Last3 => ProjectSettingsFocus::ReadmeInjectDepthLast3,
        ReadmeInjectDepth::Last5 => ProjectSettingsFocus::ReadmeInjectDepthLast5,
        ReadmeInjectDepth::Last10 => ProjectSettingsFocus::ReadmeInjectDepthLast10,
    }
}

fn is_readme_inject_depth_field(field: ProjectSettingsFocus) -> bool {
    readme_inject_depth_from_focus(field).is_some()
}

pub(crate) fn open_browser_for_project_settings_focus(app: &mut App) -> Result<()> {
    sync_project_settings_state(app);
    let target = match app.project_settings_state.focus {
        ProjectSettingsFocus::ChangelogPath => BrowseTarget::ProjectSettingsChangelogPath,
        ProjectSettingsFocus::ReleaseNowGeneral => BrowseTarget::ProjectSettingsReleaseNowGeneral,
        ProjectSettingsFocus::ReleaseNowWindows => BrowseTarget::ProjectSettingsReleaseNowWindows,
        ProjectSettingsFocus::ReleaseNowLinuxArm => BrowseTarget::ProjectSettingsReleaseNowLinuxArm,
        ProjectSettingsFocus::ReleaseNowLinuxAmd => BrowseTarget::ProjectSettingsReleaseNowLinuxAmd,
        ProjectSettingsFocus::ReleaseNowMacOs => BrowseTarget::ProjectSettingsReleaseNowMacOs,
        ProjectSettingsFocus::AliasDistPath => BrowseTarget::ProjectSettingsAliasDistPath,
        ProjectSettingsFocus::AliasUiPath => BrowseTarget::ProjectSettingsAliasUiPath,
        ProjectSettingsFocus::AliasCustomPath(index) => {
            BrowseTarget::ProjectSettingsAliasCustomPath(index)
        }
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
        BrowseTarget::ProjectSettingsReleaseNowGeneral => Some(
            app.project_settings_state
                .release_now_general
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
        BrowseTarget::ProjectSettingsAliasDistPath => Some(
            app.project_settings_state
                .alias_dist_path
                .value()
                .to_string(),
        ),
        BrowseTarget::ProjectSettingsAliasUiPath => {
            Some(app.project_settings_state.alias_ui_path.value().to_string())
        }
        BrowseTarget::ProjectSettingsAliasCustomPath(index) => app
            .project_settings_state
            .alias_custom
            .get(index as usize)
            .map(|entry| entry.path.value().to_string()),
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
        BrowseTarget::ProjectSettingsReleaseNowGeneral => ProjectSettingsFocus::ReleaseNowGeneral,
        BrowseTarget::ProjectSettingsReleaseNowWindows => ProjectSettingsFocus::ReleaseNowWindows,
        BrowseTarget::ProjectSettingsReleaseNowLinuxArm => ProjectSettingsFocus::ReleaseNowLinuxArm,
        BrowseTarget::ProjectSettingsReleaseNowLinuxAmd => ProjectSettingsFocus::ReleaseNowLinuxAmd,
        BrowseTarget::ProjectSettingsReleaseNowMacOs => ProjectSettingsFocus::ReleaseNowMacOs,
        BrowseTarget::ProjectSettingsAliasDistPath => ProjectSettingsFocus::AliasDistPath,
        BrowseTarget::ProjectSettingsAliasUiPath => ProjectSettingsFocus::AliasUiPath,
        BrowseTarget::ProjectSettingsAliasCustomPath(index) => {
            ProjectSettingsFocus::AliasCustomPath(index)
        }
        _ => return Ok(false),
    };
    app.project_settings_state.focus = field;
    if matches!(
        field,
        ProjectSettingsFocus::AliasDistPath
            | ProjectSettingsFocus::AliasUiPath
            | ProjectSettingsFocus::AliasCustomPath(_)
    ) {
        set_alias_path_from_browse(&mut app.project_settings_state, field, value);
    } else {
        app.project_settings_state
            .set_value_from_browse(field, value);
    }
    persist_project_settings_inputs(app)?;
    Ok(true)
}

pub(crate) fn project_settings_tab_label(tab: ProjectSettingsTab) -> &'static str {
    match tab {
        ProjectSettingsTab::General => "General",
        ProjectSettingsTab::Git => "Git",
        ProjectSettingsTab::Changelogs => "Changelogs",
        ProjectSettingsTab::Distro => "Distro",
        ProjectSettingsTab::RlsQd => "RLS-QD",
    }
}

fn render_project_settings_tabs(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(project) = app.config.projects.get(app.selected_project).cloned() else {
        return;
    };
    let scope_index = active_scope_index(&project, app.overview_focused_scope);
    let default_strip = project_settings_tab_strip(
        &project,
        project.release_now_for_scope(scope_index).enabled,
    );
    let strip = merge_project_settings_tab_order(
        &default_strip,
        &app.project_settings_tab_order,
    );
    let labels: Vec<&str> = strip
        .iter()
        .map(|tab| project_settings_tab_label(*tab))
        .collect();
    let active_index = strip
        .iter()
        .position(|tab| *tab == app.project_settings_tab)
        .unwrap_or(0);
    app.project_settings_tab_nav_state.selected = active_index;
    let tab_pins = project_settings_tab_pins(&strip);
    app.project_settings_tab_strip_area = Some(area);
    let nav = project_settings_tab_nav(&labels, active_index, &tab_pins, &app.config.ui);
    let tab_rects = nav.tab_rects(area);
    StatefulWidget::render(nav, area, frame.buffer_mut(), &mut app.project_settings_tab_nav_state);

    for (idx, tab) in strip.iter().enumerate() {
        if let Some(rect) = tab_rects.get(idx) {
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
        &build_general_rows(project, scope_index, &app.project_settings_state),
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
            ProjectSettingsRow::InjectDepth if row_area.height >= 2 => {
                render_inject_depth_row(app, frame, row_area, project, scope_index);
            }
            row @ (ProjectSettingsRow::AliasCustom { .. }
            | ProjectSettingsRow::AliasCustomDraft) => {
                render_alias_row(app, frame, row_area, row, app.project_settings_state.focus);
            }
            ProjectSettingsRow::AliasAddButton if row_area.height >= 3 => {
                render_alias_row(
                    app,
                    frame,
                    row_area,
                    &ProjectSettingsRow::AliasAddButton,
                    app.project_settings_state.focus,
                );
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
    state: &ProjectSettingsState,
) -> Vec<ProjectSettingsRow> {
    match tab {
        ProjectSettingsTab::General => build_general_rows(project, scope_index, state),
        ProjectSettingsTab::Git => build_git_rows(project, scope_index),
        ProjectSettingsTab::Changelogs => build_changelogs_rows(project, scope_index),
        ProjectSettingsTab::Distro => build_distro_rows(project, scope_index),
        ProjectSettingsTab::RlsQd => build_rls_qd_rows(project, scope_index),
    }
}

fn changelog_visible_fields(
    project: &ProjectConfig,
    scope_index: usize,
) -> Vec<ProjectSettingsFocus> {
    let mut fields = vec![ProjectSettingsFocus::ChangelogEnabled];
    if project.changelog_enabled_for_scope(scope_index) {
        fields.push(ProjectSettingsFocus::ChangelogPath);
        fields.push(ProjectSettingsFocus::ChangelogHidePrMessages);
        fields.push(ProjectSettingsFocus::ChangelogHideBumpMessages);
        fields.push(ProjectSettingsFocus::ChangelogWrapDetailedIfTopPicks);
        fields.push(ProjectSettingsFocus::ChangelogMiniCommitHashes);
        fields.push(ProjectSettingsFocus::ChangelogMirrorSummaryToRootChangelog);
        fields.push(ProjectSettingsFocus::ReadmeInjectionEnabled);
        if project
            .release_now_for_scope(scope_index)
            .readme_injection_enabled
        {
            fields.push(ProjectSettingsFocus::ReadmeInjectOnlyTopPicks);
            fields.push(ProjectSettingsFocus::ReadmeInjectAtRow);
            fields.extend([
                ProjectSettingsFocus::ReadmeInjectDepthCurrentOnly,
                ProjectSettingsFocus::ReadmeInjectDepthLast3,
                ProjectSettingsFocus::ReadmeInjectDepthLast5,
                ProjectSettingsFocus::ReadmeInjectDepthLast10,
            ]);
        }
    }
    fields
}

fn build_general_rows(
    project: &ProjectConfig,
    scope_index: usize,
    state: &ProjectSettingsState,
) -> Vec<ProjectSettingsRow> {
    let mut rows = vec![ProjectSettingsRow::Checkbox(
        ProjectSettingsFocus::ComfyGitFlowEnabled,
    )];
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
    rows.push(ProjectSettingsRow::Path(ProjectSettingsFocus::Alias));
    append_general_alias_rows(&mut rows, project, scope_index, state);
    rows.extend([
        ProjectSettingsRow::Spacer(1),
        ProjectSettingsRow::Text(Line::from(
            "Press Space or Enter to toggle the selected checkbox. Ctrl+O opens Browse on path fields.",
        )),
        ProjectSettingsRow::Text(Line::from(
            "Up/Down or Tab/Shift+Tab moves between fields. Mouse wheel scrolls when content overflows.",
        )),
    ]);
    rows
}

fn git_visible_fields(project: &ProjectConfig, _scope_index: usize) -> Vec<ProjectSettingsFocus> {
    let mut fields = vec![ProjectSettingsFocus::PostMergeSourceBranch];
    if project.integration_mode.is_dual_forge() {
        fields.push(ProjectSettingsFocus::MirrorSyncAfterMerge);
    }
    fields
}

fn build_git_rows(project: &ProjectConfig, scope_index: usize) -> Vec<ProjectSettingsRow> {
    let scope_label = format!("Project/Scope: {}", active_scope_name(project, scope_index));
    let mut rows = vec![
        ProjectSettingsRow::Text(Line::from(scope_label).bold()),
        ProjectSettingsRow::Spacer(1),
        ProjectSettingsRow::Text(Line::from(
            "After successful MR/PR merge, the source branch should always be:",
        )),
        ProjectSettingsRow::Path(ProjectSettingsFocus::PostMergeSourceBranch),
    ];
    if project.integration_mode.is_dual_forge() {
        rows.push(ProjectSettingsRow::Spacer(1));
        rows.push(ProjectSettingsRow::Text(Line::from(
            "This project should be kept in sync:",
        )));
        rows.push(ProjectSettingsRow::Path(
            ProjectSettingsFocus::MirrorSyncAfterMerge,
        ));
    }
    if !project.comfygitflow_enabled {
        rows.push(ProjectSettingsRow::Spacer(1));
        rows.push(ProjectSettingsRow::Text(
            Line::from("More options are available for ComfyGitFlow-enabled projects!")
                .style(Style::default().fg(Color::DarkGray)),
        ));
        rows.push(ProjectSettingsRow::Text(
            Line::from("See line 1 in `Project Settings/General`...")
                .style(Style::default().fg(Color::DarkGray)),
        ));
    }
    rows
}

fn render_git_settings(
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
        &build_git_rows(project, scope_index),
    );
}

fn build_changelogs_rows(project: &ProjectConfig, scope_index: usize) -> Vec<ProjectSettingsRow> {
    let mut rows = vec![
        ProjectSettingsRow::Text(
            Line::from(format!(
                "Scope: {}",
                active_scope_name(project, scope_index)
            ))
            .bold(),
        ),
        ProjectSettingsRow::Spacer(1),
        ProjectSettingsRow::Checkbox(ProjectSettingsFocus::ChangelogEnabled),
    ];
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
        rows.push(ProjectSettingsRow::Checkbox(
            ProjectSettingsFocus::ChangelogMirrorSummaryToRootChangelog,
        ));
        rows.push(ProjectSettingsRow::Checkbox(
            ProjectSettingsFocus::ReadmeInjectionEnabled,
        ));
        if project
            .release_now_for_scope(scope_index)
            .readme_injection_enabled
        {
            rows.push(ProjectSettingsRow::Checkbox(
                ProjectSettingsFocus::ReadmeInjectOnlyTopPicks,
            ));
            rows.push(ProjectSettingsRow::Path(
                ProjectSettingsFocus::ReadmeInjectAtRow,
            ));
            rows.push(ProjectSettingsRow::InjectDepth);
        }
        rows.push(ProjectSettingsRow::Spacer(1));
    }
    rows.extend([
        ProjectSettingsRow::Text(Line::from(if project.project_type == ProjectType::Branched {
            "Use the focused overview tile or click another tile to switch scopes."
        } else {
            "All-in-one projects apply changelog settings to the single project scope."
        })),
        ProjectSettingsRow::Text(Line::from(
            "Press Space or Enter to toggle the selected checkbox. Ctrl+O opens Browse on path fields.",
        )),
        ProjectSettingsRow::Text(Line::from(
            "Up/Down or Tab/Shift+Tab moves between fields. Mouse wheel scrolls when content overflows.",
        )),
    ]);
    rows
}

fn render_changelogs_settings(
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
        &build_changelogs_rows(project, scope_index),
    );
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
            ProjectSettingsRow::Path(ProjectSettingsFocus::ReleaseNowGeneral),
            ProjectSettingsRow::Path(ProjectSettingsFocus::ReleaseNowWindows),
            ProjectSettingsRow::Path(ProjectSettingsFocus::ReleaseNowLinuxArm),
            ProjectSettingsRow::Path(ProjectSettingsFocus::ReleaseNowLinuxAmd),
            ProjectSettingsRow::Path(ProjectSettingsFocus::ReleaseNowMacOs),
            ProjectSettingsRow::Path(ProjectSettingsFocus::ReleaseTitleTemplate),
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
        let row_focused = row.focus() == Some(focus)
            || (matches!(row, ProjectSettingsRow::InjectDepth)
                && is_readme_inject_depth_field(focus))
            || matches!(
                (row, focus),
                (
                    ProjectSettingsRow::AliasCustom { index },
                    ProjectSettingsFocus::AliasCustomPath(path_index)
                ) if *index == path_index
            )
            || matches!(
                (row, focus),
                (
                    ProjectSettingsRow::AliasCustom { index },
                    ProjectSettingsFocus::AliasCustomDelete(delete_index)
                ) if *index == delete_index
            )
            || (matches!(row, ProjectSettingsRow::AliasCustomDraft)
                && matches!(
                    focus,
                    ProjectSettingsFocus::AliasCustomDraftName
                        | ProjectSettingsFocus::AliasCustomDraftConfirm
                ));
        if row_focused {
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
        ProjectSettingsFocus::ComfyGitFlowEnabled => project.comfygitflow_enabled,
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
        ProjectSettingsFocus::ChangelogMirrorSummaryToRootChangelog => {
            project.changelog_mirror_summary_to_root_changelog_for_scope(scope_index)
        }
        ProjectSettingsFocus::ReadmeInjectionEnabled => {
            project
                .release_now_for_scope(scope_index)
                .readme_injection_enabled
        }
        ProjectSettingsFocus::AdvancedAliasEnabled => {
            app.project_settings_state.advanced_alias_enabled
        }
        ProjectSettingsFocus::ReadmeInjectOnlyTopPicks => {
            project
                .release_now_for_scope(scope_index)
                .readme_inject_only_top_picks
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

fn render_inject_depth_row(
    app: &mut App,
    frame: &mut Frame,
    area: Rect,
    project: &ProjectConfig,
    scope_index: usize,
) {
    let inset = control_inset(area);
    let selected = project
        .release_now_for_scope(scope_index)
        .readme_inject_depth;
    let row = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(FORM_LABEL_WIDTH), Constraint::Min(10)])
        .split(inset);

    let label_style = if is_readme_inject_depth_field(app.project_settings_state.focus) {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::White)
    };
    frame.render_widget(
        Paragraph::new("Inject:").style(label_style),
        Rect {
            height: 1,
            ..row[0]
        },
    );

    let options = ReadmeInjectDepth::all();
    let option_areas = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(vec![Constraint::Ratio(1, 1); options.len()])
        .flex(Flex::SpaceEvenly)
        .split(row[1]);

    for (depth, option_area) in options.iter().zip(option_areas.iter()) {
        let focus = focus_for_readme_inject_depth(*depth);
        let is_selected = *depth == selected;
        let is_focused = app.project_settings_state.focus == focus;
        let checkbox = Checkbox::new(depth.display_name(), is_selected)
            .checked_symbol("✅ ")
            .unchecked_symbol("❌ ")
            .style(if is_focused {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default().fg(Color::White)
            })
            .checkbox_style(Style::default().fg(if is_selected {
                Color::Green
            } else {
                Color::Red
            }))
            .label_style(if is_focused {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default().fg(Color::White)
            });
        frame.render_widget(checkbox, *option_area);
        app.hit_targets.push(HitTarget::new(
            *option_area,
            HitAction::SelectProjectSettingsField(focus),
        ));
    }
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
        ProjectSettingsFocus::Alias
        | ProjectSettingsFocus::CustomMainBranchName
        | ProjectSettingsFocus::QuickDownloadsPosition
        | ProjectSettingsFocus::PostMergeSourceBranch
        | ProjectSettingsFocus::MirrorSyncAfterMerge
        | ProjectSettingsFocus::QuickDownloadsFooter
        | ProjectSettingsFocus::ReadmeInjectAtRow
        | ProjectSettingsFocus::ReleaseTitleTemplate => None,
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
        ProjectSettingsFocus::ComfyGitFlowEnabled => {
            "This project follows ComfyGitFlow guides & rules."
        }
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
        ProjectSettingsFocus::ChangelogMirrorSummaryToRootChangelog => {
            "Mirror summary changelog to repo_root/CHANGELOG.md"
        }
        ProjectSettingsFocus::ReadmeInjectionEnabled => "👀 What's new README injection enabled",
        ProjectSettingsFocus::ReadmeInjectOnlyTopPicks => "Inject only Top Picks",
        ProjectSettingsFocus::ReleaseNowEnabled => {
            "Enable Release-NOW capabilities for this project/scope"
        }
        ProjectSettingsFocus::QuickDownloadsEnabled => "Quick-Downloads Enabled",
        ProjectSettingsFocus::AdvancedAliasEnabled => "Advanced Alias functionality",
        _ => "",
    }
}

fn field_label(field: ProjectSettingsFocus) -> &'static str {
    match field {
        ProjectSettingsFocus::CustomMainBranchName => "Custom main branch",
        ProjectSettingsFocus::Alias => "Alias",
        ProjectSettingsFocus::AliasDistPath => "Dist path",
        ProjectSettingsFocus::AliasUiPath => "UI path",
        ProjectSettingsFocus::ChangelogPath => "Changelog path",
        ProjectSettingsFocus::ChangelogHidePrMessages => "Hide PR messages",
        ProjectSettingsFocus::ChangelogHideBumpMessages => "Hide bump messages",
        ProjectSettingsFocus::ReleaseNowGeneral => "General",
        ProjectSettingsFocus::ReleaseNowWindows => "Windows",
        ProjectSettingsFocus::ReleaseNowLinuxArm => "Linux ARM",
        ProjectSettingsFocus::ReleaseNowLinuxAmd => "Linux AMD",
        ProjectSettingsFocus::ReleaseNowMacOs => "MacOS",
        ProjectSettingsFocus::QuickDownloadsPosition => "Position (←/→)",
        ProjectSettingsFocus::PostMergeSourceBranch => "Policy (←/→)",
        ProjectSettingsFocus::MirrorSyncAfterMerge => "Policy (←/→)",
        ProjectSettingsFocus::QuickDownloadsFooter => "Footer",
        ProjectSettingsFocus::ReadmeInjectAtRow => "Inject at row:",
        ProjectSettingsFocus::ReleaseTitleTemplate => "Release title:",
        _ => "",
    }
}

fn is_checkbox_field(field: ProjectSettingsFocus) -> bool {
    matches!(
        field,
        ProjectSettingsFocus::ComfyGitFlowEnabled
            | ProjectSettingsFocus::CustomMainBranchEnabled
            | ProjectSettingsFocus::ChangelogEnabled
            | ProjectSettingsFocus::ChangelogHidePrMessages
            | ProjectSettingsFocus::ChangelogHideBumpMessages
            | ProjectSettingsFocus::ChangelogWrapDetailedIfTopPicks
            | ProjectSettingsFocus::ChangelogMiniCommitHashes
            | ProjectSettingsFocus::ChangelogMirrorSummaryToRootChangelog
            | ProjectSettingsFocus::ReadmeInjectionEnabled
            | ProjectSettingsFocus::ReadmeInjectOnlyTopPicks
            | ProjectSettingsFocus::AdvancedAliasEnabled
            | ProjectSettingsFocus::ReleaseNowEnabled
            | ProjectSettingsFocus::QuickDownloadsEnabled
    )
}

fn adjust_mirror_sync_after_merge(
    app: &mut App,
    step: fn(MirrorSyncAfterMerge) -> MirrorSyncAfterMerge,
) -> Result<()> {
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
    let current = active_project.mirror_sync_after_merge_for_scope(scope_index);
    let next = step(current);
    active_project.set_mirror_sync_after_merge_for_scope(scope_index, next)?;
    app.project_settings_state
        .mirror_sync_after_merge
        .set_value(next.display_name().to_string());
    app.status = super::StatusMessage::success(format!(
        "Mirror sync policy set to \"{}\" for {}.",
        next.display_name(),
        scope_name
    ));
    app.config_store.save(&app.config)?;
    Ok(())
}

fn adjust_post_merge_source_branch(
    app: &mut App,
    step: fn(PostMergeSourceBranch) -> PostMergeSourceBranch,
) -> Result<()> {
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
    let current = active_project.post_merge_source_branch_for_scope(scope_index);
    let next = step(current);
    active_project.set_post_merge_source_branch_for_scope(scope_index, next)?;
    app.project_settings_state
        .post_merge_source_branch
        .set_value(next.display_name().to_string());
    app.status = super::StatusMessage::success(format!(
        "After-merge source branch policy set to \"{}\" for {}.",
        next.display_name(),
        scope_name
    ));
    app.config_store.save(&app.config)?;
    Ok(())
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
        ProjectSettingsFocus::ComfyGitFlowEnabled => {
            active_project.comfygitflow_enabled = !active_project.comfygitflow_enabled;
            let enabled = active_project.comfygitflow_enabled;
            app.status = super::StatusMessage::success(format!(
                "ComfyGitFlow {} for {}.",
                if enabled { "enabled" } else { "disabled" },
                active_project.name
            ));
        }
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
        ProjectSettingsFocus::PostMergeSourceBranch => {
            adjust_post_merge_source_branch(app, PostMergeSourceBranch::next)?;
        }
        ProjectSettingsFocus::MirrorSyncAfterMerge => {
            adjust_mirror_sync_after_merge(app, MirrorSyncAfterMerge::next)?;
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
        ProjectSettingsFocus::ChangelogMirrorSummaryToRootChangelog => {
            let next = !app
                .project_settings_state
                .changelog_mirror_summary_to_root_changelog;
            app.project_settings_state
                .changelog_mirror_summary_to_root_changelog = next;
            active_project
                .set_changelog_mirror_summary_to_root_changelog_for_scope(scope_index, next);
            app.status = super::StatusMessage::success(format!(
                "Summary changelog mirroring {} for {}.",
                if next { "enabled" } else { "disabled" },
                scope_name
            ));
        }
        ProjectSettingsFocus::ReadmeInjectionEnabled => {
            let rls = active_project.release_now_for_scope_mut(scope_index);
            rls.readme_injection_enabled = !rls.readme_injection_enabled;
            let enabled = rls.readme_injection_enabled;
            app.status = super::StatusMessage::success(format!(
                "README injection {} for {}.",
                if enabled { "enabled" } else { "disabled" },
                scope_name
            ));
        }
        ProjectSettingsFocus::AdvancedAliasEnabled => {
            let next = !app.project_settings_state.advanced_alias_enabled;
            app.project_settings_state.advanced_alias_enabled = next;
            if !next {
                app.project_settings_state.alias_custom_draft_active = false;
            }
            active_project
                .advanced_alias_for_scope_mut(scope_index)
                .enabled = next;
            app.status = super::StatusMessage::success(format!(
                "Advanced alias functionality {} for {}.",
                if next { "enabled" } else { "disabled" },
                scope_name
            ));
        }
        ProjectSettingsFocus::ReadmeInjectOnlyTopPicks => {
            let rls = active_project.release_now_for_scope_mut(scope_index);
            rls.readme_inject_only_top_picks = !rls.readme_inject_only_top_picks;
            let enabled = rls.readme_inject_only_top_picks;
            app.status = super::StatusMessage::success(format!(
                "Inject only Top Picks {} for {}.",
                if enabled { "enabled" } else { "disabled" },
                scope_name
            ));
        }
        focus if readme_inject_depth_from_focus(focus).is_some() => {
            let depth = readme_inject_depth_from_focus(focus).expect("depth focus checked");
            let rls = active_project.release_now_for_scope_mut(scope_index);
            rls.readme_inject_depth = depth;
            app.project_settings_state.focus = focus_for_readme_inject_depth(depth);
            app.status = super::StatusMessage::success(format!(
                "README inject depth set to {} for {}.",
                depth.display_name(),
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
    let general_script = app
        .project_settings_state
        .release_now_general
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
    persist_alias_state_to_project(active_project, scope_index, &app.project_settings_state);
    active_project.set_changelog_path_for_scope(scope_index, changelog_path);
    let release_now = active_project.release_now_for_scope_mut(scope_index);
    release_now.general_script = general_script;
    release_now.windows_script = windows_script;
    release_now.linux_arm_script = linux_arm_script;
    release_now.linux_amd_script = linux_amd_script;
    release_now.macos_script = macos_script;
    release_now.release_title_template = app
        .project_settings_state
        .release_title_template
        .value()
        .to_string();
    release_now.quick_downloads.footer_message = qd_footer;
    let readme_inject_at_row_str = app
        .project_settings_state
        .readme_inject_at_row
        .value()
        .trim()
        .to_string();
    if release_now.readme_injection_enabled {
        release_now.readme_inject_at_row = readme_inject_at_row_str
            .parse::<u16>()
            .unwrap_or(release_now.readme_inject_at_row);
    }
    app.config_store.save(&app.config)?;
    Ok(())
}

pub(crate) fn active_scope_index(project: &ProjectConfig, focused_scope: usize) -> usize {
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
