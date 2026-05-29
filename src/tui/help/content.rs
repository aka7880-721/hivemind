// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit SA-PS License
// For details, see the LICENSE file in the repository root.

use super::context::HelpContext;

pub(crate) fn markdown_for(context: HelpContext) -> &'static str {
    match context {
        HelpContext::DashboardProjects => {
            include_str!("pages/dashboard/projects.md")
        }
        HelpContext::OverviewTiles => include_str!("pages/overview/tiles.md"),
        HelpContext::OverviewRecentEmbedded => {
            include_str!("pages/overview/recent_embedded.md")
        }
        HelpContext::OverviewProjectDetail => {
            include_str!("pages/overview/project_detail.md")
        }
        HelpContext::ProjectSettingsGeneral => {
            include_str!("pages/project_settings/general.md")
        }
        HelpContext::ProjectSettingsChangelogs => {
            include_str!("pages/project_settings/changelogs.md")
        }
        HelpContext::ProjectSettingsDistro => {
            include_str!("pages/project_settings/distro.md")
        }
        HelpContext::ProjectSettingsRlsQd => {
            include_str!("pages/project_settings/rls_qd.md")
        }
        HelpContext::ScreenWizard => include_str!("pages/screens/wizard.md"),
        HelpContext::ScreenUiSettings => include_str!("pages/screens/ui_settings.md"),
        HelpContext::ModalSnif => include_str!("pages/modals/snif.md"),
        HelpContext::ModalBrowser => include_str!("pages/modals/browser.md"),
        HelpContext::ModalReleaseNowNotes => {
            include_str!("pages/modals/release_now_notes.md")
        }
        HelpContext::ModalTopPicksEditor => include_str!("pages/modals/top_picks_editor.md"),
        HelpContext::ModalReleaseNowWarning => {
            include_str!("pages/modals/release_now_warning.md")
        }
        HelpContext::ModalReleaseNowConfigure => {
            include_str!("pages/modals/release_now_configure.md")
        }
        HelpContext::ModalReleaseNowRunning => {
            include_str!("pages/modals/release_now_running.md")
        }
        HelpContext::ModalReleaseNowCompleted => {
            include_str!("pages/modals/release_now_completed.md")
        }
        HelpContext::ModalDeleteConfirm => include_str!("pages/modals/delete_confirm.md"),
        HelpContext::ModalCommitRename => include_str!("pages/modals/commit_rename.md"),
        HelpContext::ModalProjectEdit => include_str!("pages/modals/project_edit.md"),
        HelpContext::ModalTagAnnotation => include_str!("pages/modals/tag_annotation.md"),
        HelpContext::ModalMainBranchWarning => {
            include_str!("pages/modals/main_branch_warning.md")
        }
        HelpContext::ModalStdChangelogSubBranch => {
            include_str!("pages/modals/std_changelog_sub_branch.md")
        }
        HelpContext::ModalTag => include_str!("pages/modals/tag.md"),
        HelpContext::ModalChangelogPreviewWorkflow => {
            include_str!("pages/modals/changelog_preview_workflow.md")
        }
        HelpContext::ModalChangelogPreviewCustomRange => {
            include_str!("pages/modals/changelog_preview_custom_range.md")
        }
        HelpContext::ModalChangelogPreviewReadonly => {
            include_str!("pages/modals/changelog_preview_readonly.md")
        }
        HelpContext::ModalRecentChanges => include_str!("pages/modals/recent_changes.md"),
        HelpContext::ModalOverviewBumpWarning => {
            include_str!("pages/modals/overview_bump_warning.md")
        }
        HelpContext::ModalOverviewBumpKind => include_str!("pages/modals/overview_bump_kind.md"),
        HelpContext::ModalOverviewBranchBump => {
            include_str!("pages/modals/overview_branch_bump.md")
        }
        HelpContext::ModalOverviewBumpWorkflow => {
            include_str!("pages/modals/overview_bump_workflow.md")
        }
        HelpContext::ModalBump => include_str!("pages/modals/bump.md"),
        HelpContext::ModalProgress => include_str!("pages/modals/progress.md"),
    }
}
