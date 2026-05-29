// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit SA-PS License
// For details, see the LICENSE file in the repository root.

use ratatui::layout::Rect;
use tui_textarea::TextArea as TuiTextArea;

use crate::{
    changelog::ChangelogDocument, config::BranchScopeKind, tui::OverviewTab, tui::ProjectEditFocus,
    tui::WizardField, workflow::OverviewBumpWorkflow, workflow::dialogs::RecentChangesTab,
};

use super::project_settings::{ProjectSettingsFocus, ProjectSettingsTab};
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Screen {
    Dashboard,
    Wizard,
    UiSettings,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum DashboardPane {
    Projects,
    Overview,
}

#[derive(Clone)]
pub(crate) struct HitTarget {
    pub(crate) rect: Rect,
    pub(crate) action: HitAction,
    pub(crate) right_action: Option<HitAction>,
}

impl HitTarget {
    pub(crate) fn new(rect: Rect, action: HitAction) -> Self {
        Self {
            rect,
            action,
            right_action: None,
        }
    }

    pub(crate) fn with_right_action(
        rect: Rect,
        action: HitAction,
        right_action: HitAction,
    ) -> Self {
        Self {
            rect,
            action,
            right_action: Some(right_action),
        }
    }

    pub(crate) fn contains(&self, column: u16, row: u16) -> bool {
        column >= self.rect.x
            && column < self.rect.x + self.rect.width
            && row >= self.rect.y
            && row < self.rect.y + self.rect.height
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum TextInputClickTarget {
    Wizard(WizardField),
    ProjectEdit(ProjectEditFocus),
    ProjectSettings(ProjectSettingsFocus),
    CommitRenameMessage,
}

impl TextInputClickTarget {
    pub(crate) fn same_field_action(&self, action: &HitAction) -> bool {
        match (self, action) {
            (&TextInputClickTarget::Wizard(a), &HitAction::WizardField(b)) => a == b,
            (&TextInputClickTarget::ProjectEdit(a), &HitAction::EditProjectField(b)) => a == b,
            (
                &TextInputClickTarget::ProjectSettings(a),
                &HitAction::SelectProjectSettingsField(b),
            ) => a == b,
            (&TextInputClickTarget::CommitRenameMessage, &HitAction::CommitRenameMessageField) => {
                true
            }
            _ => false,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum RecentChangeView {
    Overview,
    Popup,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct RecentChangeClickTarget {
    pub(crate) view: RecentChangeView,
    pub(crate) line_index: usize,
}

#[derive(Clone)]
pub(crate) enum HitAction {
    SelectOverviewTab(OverviewTab),
    SelectProjectSettingsTab(ProjectSettingsTab),
    SelectProjectSettingsField(ProjectSettingsFocus),
    SelectProject(usize),
    SelectOverviewScope(usize),
    BrowseProjectSettingsField(ProjectSettingsFocus),
    OpenOverviewReleaseNow(usize),
    BeginOverviewBump(usize),
    CycleOverviewTileInfo(usize, OverviewTileInfoRow),
    SelectOverviewBumpKind(usize),
    ConfirmOverviewBumpKind,
    CancelOverviewBumpKind,
    SelectOverviewBumpWorkflow(usize),
    ConfirmOverviewBumpWorkflow,
    CancelOverviewBumpWorkflow,
    SelectOverviewBumpWarningChoice(usize),
    SelectMainBranchWarningChoice(usize),
    SelectStdChangelogSubBranchChoice(usize),
    ConfirmChangelogPreview,
    SaveChangelogPreview,
    CancelChangelogPreview,
    ScrollChangelogPreview(i16),
    AdjustOverviewVersion(usize, OverviewVersionControl, i32),
    ResetOverviewPendingVersion(usize),
    OpenOverviewTagDialog(usize),
    EditProjectField(ProjectEditFocus),
    FocusProjectEditDialog,
    ProjectEditScopeAction(ScopeAction),
    SaveProjectEdit,
    RemoveProject,
    CancelProjectEdit,
    CycleReleaseNowOption(isize),
    ToggleReleaseNowChangelog,
    EditReleaseNowNotes,
    RunReleaseNow,
    ContinueReleaseNowWarning,
    ToggleReleaseNowAutoFollow,
    CancelReleaseNowRun,
    ScrollReleaseNow(i16),
    SaveReleaseNowNotes,
    CancelReleaseNowNotes,
    CloseReleaseNow,
    ConfirmDeleteRequest,
    CancelDeleteRequest,
    ToggleTabHints,
    ToggleFooter,
    CycleFooterContent(i32),
    BrowseWizardTargetPath,
    BrowseWizardRepoRoot,
    BrowseProjectTargetPath,
    BrowseProjectRepoRoot,
    EnableWizardCustomTargetKey,
    EnableProjectCustomTargetKey,
    BrowserSelect(usize),
    SelectRecentChangesTab(RecentChangesTab),
    CycleRecentChangesScope(isize),
    CloseRecentChanges,
    ScrollRecentChanges(i16),
    SelectRecentChangeLine(RecentChangeView, usize),
    ToggleCommitRenameForcePush,
    SaveCommitRename,
    CancelCommitRename,
    ReleaseNowNotesField,
    CommitRenameMessageField,
    OpenTagDialog,
    OpenTagAnnotation,
    CycleTagScope(isize),
    CycleTagAction(isize),
    CycleBumpAction(isize),
    CycleBumpScope(isize),
    ApplyBump,
    CancelBump,
    ConfirmOverviewBranchBump,
    CancelOverviewBranchBump,
    CreateTag,
    SaveTagAnnotation,
    CancelTagAnnotation,
    CancelTagDialog,
    WizardField(WizardField),
    WizardScopeAction(ScopeAction),
    ValidateWizard,
    SaveWizard,
    CancelWizard,
    SaveTopPicks,
    CancelTopPicks,
    TopPicksEditorField,
}

#[derive(Clone, Copy)]
pub(crate) enum ScopeAction {
    Add,
    Remove,
    MoveUp,
    MoveDown,
}

#[derive(Clone, Copy)]
pub(crate) enum OverviewVersionControl {
    Major,
    Minor,
    Patch,
    Whole,
}

#[derive(Clone, Copy)]
pub(crate) enum OverviewTileInfoRow {
    Dev,
    Release,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum CustomChangelogRangeFocus {
    From,
    To,
}

#[derive(Clone)]
pub(crate) struct CustomChangelogSelection {
    pub(crate) from_ref: String,
    pub(crate) to_ref: Option<String>,
}

#[derive(Clone)]
pub(crate) struct CustomChangelogRangeState {
    pub(crate) scope_name: String,
    pub(crate) tags: Vec<String>,
    pub(crate) from_index: usize,
    pub(crate) to_index: Option<usize>,
    pub(crate) focus: CustomChangelogRangeFocus,
}

impl CustomChangelogRangeState {
    pub(crate) fn new(
        scope_name: String,
        tags: Vec<String>,
        selection: Option<CustomChangelogSelection>,
    ) -> Self {
        let mut state = Self {
            scope_name,
            tags,
            from_index: 0,
            to_index: None,
            focus: CustomChangelogRangeFocus::From,
        };

        if let Some(selection) = selection {
            if let Some(from_index) = state.tags.iter().position(|tag| tag == &selection.from_ref) {
                state.from_index = from_index;
            }
            state.to_index = selection
                .to_ref
                .as_ref()
                .and_then(|to_ref| state.tags.iter().position(|tag| tag == to_ref))
                .filter(|to_index| *to_index < state.from_index);
        }

        state.ensure_valid_to_index();
        state
    }

    pub(crate) fn has_tags(&self) -> bool {
        !self.tags.is_empty()
    }

    pub(crate) fn focus_label(&self, focus: CustomChangelogRangeFocus) -> &'static str {
        if self.focus == focus { ">" } else { " " }
    }

    pub(crate) fn current_from_ref(&self) -> Option<&str> {
        self.tags.get(self.from_index).map(String::as_str)
    }

    pub(crate) fn current_to_ref(&self) -> &str {
        self.to_index
            .and_then(|index| self.tags.get(index))
            .map(String::as_str)
            .unwrap_or("HEAD")
    }

    pub(crate) fn range_label(&self) -> String {
        self.current_from_ref()
            .map(|from_ref| format!("{}..{}", from_ref, self.current_to_ref()))
            .unwrap_or_else(|| "no tags found; showing the latest 60 commits".to_string())
    }

    pub(crate) fn selection(&self) -> Option<CustomChangelogSelection> {
        Some(CustomChangelogSelection {
            from_ref: self.current_from_ref()?.to_string(),
            to_ref: self
                .to_index
                .and_then(|index| self.tags.get(index))
                .cloned(),
        })
    }

    pub(crate) fn cycle_focus(&mut self, delta: isize) {
        let focuses = [
            CustomChangelogRangeFocus::From,
            CustomChangelogRangeFocus::To,
        ];
        let current = match self.focus {
            CustomChangelogRangeFocus::From => 0,
            CustomChangelogRangeFocus::To => 1,
        } as isize;
        let next = (current + delta).rem_euclid(focuses.len() as isize) as usize;
        self.focus = focuses[next];
    }

    pub(crate) fn select_focus(&mut self, focus: CustomChangelogRangeFocus) {
        self.focus = focus;
    }

    pub(crate) fn adjust_focused_selection(&mut self, delta: isize) -> bool {
        if !self.has_tags() || delta == 0 {
            return false;
        }

        match self.focus {
            CustomChangelogRangeFocus::From => self.adjust_from(delta),
            CustomChangelogRangeFocus::To => self.adjust_to(delta),
        }
    }

    pub(crate) fn display_from(&self) -> String {
        self.current_from_ref().unwrap_or("<no tags>").to_string()
    }

    pub(crate) fn display_to(&self) -> String {
        self.current_to_ref().to_string()
    }

    pub(crate) fn ensure_valid_to_index(&mut self) {
        if self.tags.is_empty() {
            self.from_index = 0;
            self.to_index = None;
            return;
        }

        self.from_index = self.from_index.min(self.tags.len().saturating_sub(1));
        if self.from_index == 0 {
            self.to_index = None;
        } else if self
            .to_index
            .is_some_and(|to_index| to_index >= self.from_index)
        {
            self.to_index = Some(self.from_index - 1);
        }
    }

    pub(crate) fn adjust_from(&mut self, delta: isize) -> bool {
        let len = self.tags.len();
        if len == 0 {
            return false;
        }

        let next =
            (self.from_index as isize + delta).clamp(0, len.saturating_sub(1) as isize) as usize;
        if next == self.from_index {
            return false;
        }

        self.from_index = next;
        self.ensure_valid_to_index();
        true
    }

    pub(crate) fn adjust_to(&mut self, delta: isize) -> bool {
        let max_position = self.from_index;
        let current_position = self.to_index.map(|to_index| to_index + 1).unwrap_or(0);
        let next_position =
            (current_position as isize + delta).clamp(0, max_position as isize) as usize;
        if next_position == current_position {
            return false;
        }

        self.to_index = if next_position == 0 {
            None
        } else {
            Some(next_position - 1)
        };
        true
    }
}

pub(crate) struct ChangelogPreviewDialog {
    pub(crate) project_name: String,
    pub(crate) next_version: String,
    pub(crate) scope_index: usize,
    pub(crate) workflow: Option<OverviewBumpWorkflow>,
    pub(crate) custom_range: Option<CustomChangelogRangeState>,
    pub(crate) entries: Vec<ChangelogPreviewEntry>,
    pub(crate) release_message: TuiTextArea<'static>,
    pub(crate) release_message_placeholder: String,
    pub(crate) scroll: u16,
    /// Layout width of the preview body, set when the dialog is drawn.
    pub(crate) preview_render_width: u16,
}

#[derive(Clone)]
pub(crate) struct DeleteConfirmationDialog {
    pub(crate) target: DeleteConfirmationTarget,
    pub(crate) confirm_selected: bool,
}

impl DeleteConfirmationDialog {
    pub(crate) fn project(project_index: usize, project_name: String) -> Self {
        Self {
            target: DeleteConfirmationTarget::Project {
                project_index,
                project_name,
            },
            confirm_selected: false,
        }
    }

    pub(crate) fn scope(
        project_index: usize,
        project_name: String,
        scope_index: usize,
        scope_name: String,
        scope_kind: BranchScopeKind,
        removes_project: bool,
    ) -> Self {
        Self {
            target: DeleteConfirmationTarget::Scope {
                project_index,
                project_name,
                scope_index,
                scope_name,
                scope_kind,
                removes_project,
            },
            confirm_selected: false,
        }
    }

    pub(crate) fn toggle_selection(&mut self) {
        self.confirm_selected = !self.confirm_selected;
    }
}

#[derive(Clone)]
pub(crate) enum DeleteConfirmationTarget {
    Project {
        project_index: usize,
        project_name: String,
    },
    Scope {
        project_index: usize,
        project_name: String,
        scope_index: usize,
        scope_name: String,
        scope_kind: BranchScopeKind,
        removes_project: bool,
    },
}

impl ChangelogPreviewDialog {
    pub(crate) fn new(
        project_name: String,
        next_version: String,
        scope_index: usize,
        workflow: OverviewBumpWorkflow,
        entries: Vec<ChangelogPreviewEntry>,
    ) -> Self {
        Self {
            project_name,
            next_version,
            scope_index,
            workflow: Some(workflow),
            custom_range: None,
            entries,
            release_message: new_release_message_editor(""),
            release_message_placeholder: "Optional multi-line release notes in Markdown"
                .to_string(),
            scroll: 0,
            preview_render_width: 80,
        }
    }

    pub(crate) fn preview_only(
        project_name: String,
        next_version: String,
        scope_index: usize,
        custom_range: Option<CustomChangelogRangeState>,
        entries: Vec<ChangelogPreviewEntry>,
    ) -> Self {
        Self {
            project_name,
            next_version,
            scope_index,
            workflow: None,
            custom_range,
            entries,
            release_message: new_release_message_editor(""),
            release_message_placeholder: "Optional multi-line release notes in Markdown"
                .to_string(),
            scroll: 0,
            preview_render_width: 80,
        }
    }

    pub(crate) fn release_message_value(&self) -> String {
        let release_message = self.release_message.lines().join("\n");
        if release_message.trim().is_empty() {
            String::new()
        } else {
            release_message
        }
    }

    pub(crate) fn combined_preview_markdown(&self) -> String {
        let release_message = self.release_message_value();
        let mut lines = Vec::new();
        for (index, entry) in self.entries.iter().enumerate() {
            if self.entries.len() > 1 {
                lines.push(format!("### Repo: {}", entry.repo_root));
                lines.push(format!("Path: `{}`", entry.changelog_path));
                lines.push(String::new());
            }

            let rendered = entry.rendered_markdown(&release_message);
            lines.extend(rendered.lines().map(ToOwned::to_owned));
            if index + 1 < self.entries.len() {
                lines.push(String::new());
            }
        }
        lines.join("\n")
    }

    pub(crate) fn preview_line_count(&self) -> usize {
        crate::tui::markdown_line_count(
            &self.combined_preview_markdown(),
            self.preview_render_width,
        )
    }

    pub(crate) fn prepare_pending_write(&self) -> PendingChangelogWrite {
        let release_message = self.release_message_value();
        PendingChangelogWrite {
            scope_index: self.scope_index,
            workflow: self
                .workflow
                .expect("workflow preview required to prepare changelog write"),
            entries: self
                .entries
                .iter()
                .map(|entry| PreparedChangelogEntry {
                    repo_root: entry.repo_root.clone(),
                    changelog_path: entry.changelog_path.clone(),
                    stage_path: entry.stage_path.clone(),
                    markdown: entry.rendered_markdown(&release_message),
                })
                .collect(),
        }
    }
}

pub(crate) fn new_release_message_editor(existing_release_message: &str) -> TuiTextArea<'static> {
    let mut editor = if existing_release_message.trim().is_empty() {
        TuiTextArea::default()
    } else {
        TuiTextArea::from(existing_release_message.lines())
    };
    editor.set_placeholder_text("Optional multi-line release notes in Markdown");
    editor.set_tab_length(2);
    editor.set_max_histories(100);
    editor
}

#[derive(Clone)]
pub(crate) struct ChangelogPreviewEntry {
    pub(crate) repo_root: String,
    pub(crate) changelog_path: String,
    pub(crate) stage_path: String,
    pub(crate) document: ChangelogDocument,
}

impl ChangelogPreviewEntry {
    pub(crate) fn rendered_markdown(&self, release_message: &str) -> String {
        let document = if release_message.trim().is_empty() {
            self.document.clone()
        } else {
            self.document
                .clone()
                .with_release_message(release_message.to_string())
        };
        document.render_markdown().markdown
    }
}

#[derive(Clone)]
pub(crate) struct PreparedChangelogEntry {
    pub(crate) repo_root: String,
    pub(crate) changelog_path: String,
    pub(crate) stage_path: String,
    pub(crate) markdown: String,
}

#[derive(Clone)]
pub(crate) struct PendingChangelogWrite {
    pub(crate) scope_index: usize,
    pub(crate) workflow: OverviewBumpWorkflow,
    pub(crate) entries: Vec<PreparedChangelogEntry>,
}

pub(crate) struct ProgressDialog {
    pub(crate) title: String,
    pub(crate) message: String,
}
