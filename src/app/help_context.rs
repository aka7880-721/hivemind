// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit License v1.2

use crate::tui::{HelpContext, OverviewTab};

use super::{
    App, DashboardPane, Screen,
    project_settings::{self, ProjectSettingsTab},
};

impl App {
    pub(crate) fn resolve_help_context(&self) -> HelpContext {
        if self.snif_dialog.is_some() {
            return HelpContext::ModalSnif;
        }
        if self.browser_dialog.is_some() {
            return HelpContext::ModalBrowser;
        }
        if self.release_now_notes_dialog.is_some() {
            return HelpContext::ModalReleaseNowNotes;
        }
        if self.top_picks_editor_dialog.is_some() {
            return HelpContext::ModalTopPicksEditor;
        }
        if let Some(dialog) = &self.release_now_dialog {
            if dialog.is_running() {
                return HelpContext::ModalReleaseNowRunning;
            }
            if dialog.is_completed() {
                return HelpContext::ModalReleaseNowCompleted;
            }
            if dialog.is_warning_mode() {
                return HelpContext::ModalReleaseNowWarning;
            }
            return HelpContext::ModalReleaseNowConfigure;
        }
        if self.delete_confirmation_dialog.is_some() {
            return HelpContext::ModalDeleteConfirm;
        }
        if self.commit_rename_dialog.is_some() {
            return HelpContext::ModalCommitRename;
        }
        if self.project_edit_dialog.is_some() {
            return HelpContext::ModalProjectEdit;
        }
        if self.tag_annotation_dialog.is_some() {
            return HelpContext::ModalTagAnnotation;
        }
        if self.main_branch_warning_dialog.is_some() {
            return HelpContext::ModalMainBranchWarning;
        }
        if self.std_changelog_sub_branch_dialog.is_some() {
            return HelpContext::ModalStdChangelogSubBranch;
        }
        if self.tag_dialog.is_some() {
            return HelpContext::ModalTag;
        }
        if let Some(dialog) = &self.changelog_preview_dialog {
            if dialog.workflow.is_some() {
                return HelpContext::ModalChangelogPreviewWorkflow;
            }
            if dialog.custom_range.is_some() {
                return HelpContext::ModalChangelogPreviewCustomRange;
            }
            return HelpContext::ModalChangelogPreviewReadonly;
        }
        if self.recent_changes_dialog.is_some() {
            return HelpContext::ModalRecentChanges;
        }
        if self.overview_bump_warning_dialog.is_some() {
            return HelpContext::ModalOverviewBumpWarning;
        }
        if self.overview_bump_kind_dialog.is_some() {
            return HelpContext::ModalOverviewBumpKind;
        }
        if self.overview_branch_bump_dialog.is_some() {
            return HelpContext::ModalOverviewBranchBump;
        }
        if self.overview_bump_workflow_dialog.is_some() {
            return HelpContext::ModalOverviewBumpWorkflow;
        }
        if self.bump_dialog.is_some() {
            return HelpContext::ModalBump;
        }
        if self.progress_dialog.is_some() {
            return HelpContext::ModalProgress;
        }

        match self.screen {
            Screen::Wizard => HelpContext::ScreenWizard,
            Screen::UiSettings => HelpContext::ScreenUiSettings,
            Screen::Dashboard => match self.dashboard_focus {
                DashboardPane::Projects => HelpContext::DashboardProjects,
                DashboardPane::Overview => self.dashboard_overview_help_context(),
            },
        }
    }

    fn dashboard_overview_help_context(&self) -> HelpContext {
        match self.overview_tab {
            OverviewTab::Overview => HelpContext::OverviewTiles,
            OverviewTab::RecentChanges => HelpContext::OverviewRecentEmbedded,
            OverviewTab::ProjectDetail => HelpContext::OverviewProjectDetail,
            OverviewTab::ProjectSettings => match self.project_settings_tab {
                ProjectSettingsTab::General => HelpContext::ProjectSettingsGeneral,
                ProjectSettingsTab::Changelogs => HelpContext::ProjectSettingsChangelogs,
                ProjectSettingsTab::Distro => HelpContext::ProjectSettingsDistro,
                ProjectSettingsTab::RlsQd => HelpContext::ProjectSettingsRlsQd,
            },
        }
    }

    /// When true, `?` is passed through to the active editor instead of opening help.
    pub(crate) fn help_blocked_by_text_input(&mut self) -> bool {
        if self.commit_rename_dialog.is_some() {
            return true;
        }
        if self.tag_annotation_dialog.is_some() {
            return true;
        }
        if self.release_now_notes_dialog.is_some() {
            return true;
        }
        if self.top_picks_editor_dialog.is_some() {
            return true;
        }
        if self
            .changelog_preview_dialog
            .as_ref()
            .is_some_and(|dialog| dialog.workflow.is_some())
        {
            return true;
        }
        if self.snif_dialog.is_some() {
            return true;
        }
        if self.tag_dialog.is_some() {
            return true;
        }
        if self
            .overview_branch_bump_dialog
            .as_ref()
            .is_some_and(|dialog| dialog.input_enabled())
        {
            return true;
        }
        if matches!(self.screen, Screen::Wizard) && self.wizard.focus_accepts_text() {
            return true;
        }
        if self
            .project_edit_dialog
            .as_ref()
            .is_some_and(|dialog| dialog.focus_accepts_text())
        {
            return true;
        }
        if self.screen == Screen::Dashboard
            && self.overview_tab == OverviewTab::ProjectSettings
            && project_settings::captures_text_input(self)
        {
            return true;
        }
        false
    }
}
