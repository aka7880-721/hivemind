// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit SA-PS License

use super::super::*;

use std::time::Duration;

use anyhow::{Result, anyhow, bail};
use arboard::Clipboard;
#[cfg(target_os = "linux")]
use arboard::{LinuxClipboardKind, SetExtLinux};
use ratatui_comfy_toaster::{ToastBuilder, ToastInteraction, ToastType};
use ratatui_explorer::Input as ExplorerInput;

use crate::{
    changelog::{
        ParsedCommit, load_top_picks_edits_with_baseline, resolve_top_picks_baseline_tag,
        save_top_picks_edits, write_temp_changelog_markdown,
    },
    cli::{prepare_commit_rename, push_branch_force_with_lease, rename_commit_with_subject},
    config::{ProjectConfig, ProjectType},
    git::{
        GitScopeContext, branches_containing_ref_with_cancel,
        collect_all_branch_git_scope_contexts, current_branch_with_cancel,
        latest_local_tag_with_cancel, run_git,
    },
    tui::OverviewTab,
    tui::{ProjectEditDialog, ProjectEditFocus},
    tui::{ProjectWizard, WizardField},
    workflow::dialogs::{BumpDialog, RecentChangesTab, TagAction, TagDialog},
    workflow::targets::{ProbeKind, TargetProbe, probe_target, write_target_version},
};

use super::super::{overview, project_settings, rls_now};
use crate::changelog::top_picks as changelog_tp;
use crate::workflow::{OverviewBumpWorkflow, git_flow};

impl App {
    pub(crate) fn handle_hit_action(&mut self, action: HitAction) -> Result<()> {
        match action {
            HitAction::SelectOverviewTab(tab) => {
                self.overview_tab = tab;
                self.dashboard_focus = DashboardPane::Overview;
            }
            HitAction::SelectProjectSettingsTab(tab) => {
                self.overview_tab = OverviewTab::ProjectSettings;
                self.project_settings_tab = tab;
                self.dashboard_focus = DashboardPane::Overview;
                project_settings::sync_project_settings_state(self);
            }
            HitAction::SelectProjectSettingsField(field) => {
                return project_settings::activate_project_settings_field(self, field);
            }
            HitAction::BrowseProjectSettingsField(field) => {
                project_settings::set_project_settings_focus(self, field);
                return project_settings::open_browser_for_project_settings_focus(self);
            }
            HitAction::SelectProject(index) => {
                self.selected_project = index.min(self.config.projects.len().saturating_sub(1));
                self.prime_selected_project_dashboard_data();
                self.dashboard_focus = DashboardPane::Projects;
            }
            HitAction::SelectOverviewScope(scope_index) => {
                return self.select_dashboard_overview_scope(scope_index);
            }
            HitAction::OpenOverviewReleaseNow(scope_index) => {
                self.dashboard_focus = DashboardPane::Overview;
                return self.open_overview_release_now(scope_index);
            }
            HitAction::BeginOverviewBump(scope_index) => {
                self.dashboard_focus = DashboardPane::Overview;
                return self.begin_overview_bump(scope_index);
            }
            HitAction::CycleOverviewTileInfo(scope_index, row) => {
                self.dashboard_focus = DashboardPane::Overview;
                return self.cycle_overview_tile_info(scope_index, row);
            }
            HitAction::SelectOverviewBumpWorkflow(index) => {
                self.select_overview_bump_workflow(index)
            }
            HitAction::ConfirmOverviewBumpWorkflow => {
                return self.request_confirm_overview_bump_workflow();
            }
            HitAction::CancelOverviewBumpWorkflow => self.cancel_overview_bump_workflow(),
            HitAction::SelectOverviewBumpKind(index) => self.select_overview_bump_kind(index),
            HitAction::ConfirmOverviewBumpKind => return self.confirm_overview_bump_kind(),
            HitAction::CancelOverviewBumpKind => self.cancel_overview_bump_kind(),
            HitAction::SelectOverviewBumpWarningChoice(index) => {
                self.select_overview_bump_warning(index)
            }
            HitAction::SelectMainBranchWarningChoice(index) => {
                self.select_main_branch_warning(index)
            }
            HitAction::SelectStdChangelogSubBranchChoice(index) => {
                self.select_std_changelog_sub_branch_warning(index)
            }
            HitAction::ConfirmChangelogPreview => return self.confirm_changelog_preview(),
            HitAction::SaveChangelogPreview => return self.save_changelog_preview(),
            HitAction::CancelChangelogPreview => self.cancel_changelog_preview(),
            HitAction::ScrollChangelogPreview(delta) => self.scroll_changelog_preview(delta),
            HitAction::AdjustOverviewVersion(scope_index, control, delta) => {
                return self.adjust_overview_pending_version(scope_index, control, delta);
            }
            HitAction::ResetOverviewPendingVersion(scope_index) => {
                return self.reset_overview_pending_version(scope_index);
            }
            HitAction::OpenOverviewTagDialog(scope_index) => {
                return self.open_overview_tag_dialog(scope_index);
            }
            HitAction::EditProjectField(field) => {
                if let Some(dialog) = &mut self.project_edit_dialog {
                    dialog.focus = field;
                }
            }
            HitAction::FocusProjectEditDialog => {}
            HitAction::ProjectEditScopeAction(action) => {
                return self.apply_project_edit_scope_action(action);
            }
            HitAction::SaveProjectEdit => return self.save_project_edit(),
            HitAction::RemoveProject => return self.remove_project(),
            HitAction::CancelProjectEdit => {
                self.project_edit_dialog = None;
                self.status = StatusMessage::info("Project edit cancelled.");
            }
            HitAction::CycleReleaseNowOption(delta) => {
                if let Some(dialog) = &mut self.release_now_dialog {
                    dialog.cycle_option(delta);
                }
            }
            HitAction::ToggleReleaseNowChangelog => {
                if let Some(dialog) = &mut self.release_now_dialog {
                    dialog.toggle_attach_changelog();
                }
            }
            HitAction::EditReleaseNowNotes => return self.open_release_now_notes_dialog(),
            HitAction::RunReleaseNow => return self.request_run_release_now(),
            HitAction::ContinueReleaseNowWarning => {
                if let Some(dialog) = &mut self.release_now_dialog {
                    dialog.proceed_past_warning();
                }
            }
            HitAction::SelectReleaseNowArtifactsChoice(choice) => {
                if let Some(dialog) = &mut self.release_now_dialog {
                    dialog.artifacts_choice_selected = choice;
                    if choice == 3 {
                        self.close_release_now_dialog();
                    } else {
                        dialog.confirm_existing_artifacts_choice();
                    }
                }
            }
            HitAction::ContinueReleaseNowArtifactsCustomize => {
                if let Some(dialog) = &mut self.release_now_dialog {
                    dialog.confirm_artifacts_customize();
                }
            }
            HitAction::BackReleaseNowArtifactsCustomize => {
                if let Some(dialog) = &mut self.release_now_dialog {
                    dialog.back_from_artifacts_customize();
                }
            }
            HitAction::ToggleReleaseNowAutoFollow => self.toggle_release_now_auto_follow(),
            HitAction::CancelReleaseNowRun => self.request_cancel_release_now(),
            HitAction::ScrollReleaseNow(delta) => self.scroll_release_now(delta),
            HitAction::SaveReleaseNowNotes => return self.save_release_now_notes(),
            HitAction::CancelReleaseNowNotes => {
                self.release_now_notes_dialog = None;
                self.status = StatusMessage::info("Release notes editor closed.");
            }
            HitAction::CloseReleaseNow => self.close_release_now_dialog(),
            HitAction::ConfirmDeleteRequest => return self.confirm_delete_request(),
            HitAction::CancelDeleteRequest => self.cancel_delete_request(),
            HitAction::ToggleTabHints => return self.toggle_tab_hints(),
            HitAction::ToggleFooter => return self.toggle_footer(),
            HitAction::CycleFooterContent(delta) => return self.cycle_footer_content(delta),
            HitAction::BrowseWizardTargetPath => {
                return self.open_browser(BrowseTarget::WizardTargetPath);
            }
            HitAction::BrowseWizardRepoRoot => {
                return self.open_browser(BrowseTarget::WizardRepoRoot);
            }
            HitAction::BrowseProjectTargetPath => {
                return self.open_browser(BrowseTarget::ProjectEditTargetPath);
            }
            HitAction::BrowseProjectRepoRoot => {
                return self.open_browser(BrowseTarget::ProjectEditRepoRoot);
            }
            HitAction::EnableWizardCustomTargetKey => {
                self.wizard.enable_custom_target_key();
                self.status = StatusMessage::info("Custom target key input enabled.");
            }
            HitAction::EnableProjectCustomTargetKey => {
                if let Some(dialog) = &mut self.project_edit_dialog {
                    dialog.enable_custom_target_key();
                    self.status = StatusMessage::info("Custom target key input enabled.");
                }
            }
            HitAction::BrowserSelect(index) => self.select_browser_index(index),
            HitAction::SelectRecentChangesTab(tab) => {
                if let Some(dialog) = &mut self.recent_changes_dialog {
                    if tab == RecentChangesTab::History && !dialog.history_loaded {
                        self.schedule_recent_changes_action(
                            "Loading tag history for the selected scope.",
                            RecentChangesLoadAction::SwitchTab(RecentChangesTab::History),
                        )?;
                    } else {
                        dialog.switch_tab(tab)?;
                    }
                }
            }
            HitAction::CycleRecentChangesScope(delta) => {
                if self.recent_changes_dialog.is_some() {
                    let message = if delta.is_negative() {
                        "Loading git history for the previous scope."
                    } else {
                        "Loading git history for the next scope."
                    };
                    self.schedule_recent_changes_action(
                        message,
                        RecentChangesLoadAction::RotateScope(delta),
                    )?;
                }
            }
            HitAction::CycleBumpScope(delta) => self.rotate_bump_scope(delta),
            HitAction::CycleBumpAction(delta) => self.rotate_bump_action(delta),
            HitAction::ApplyBump => return self.request_apply_bump(),
            HitAction::CancelBump => {
                self.bump_dialog = None;
                self.status = StatusMessage::info("Bump preview closed.");
            }
            HitAction::ConfirmOverviewBranchBump => return self.confirm_overview_branch_bump(),
            HitAction::CancelOverviewBranchBump => self.cancel_overview_branch_bump(),
            HitAction::CloseRecentChanges => {
                self.recent_changes_dialog = None;
                self.cancel_background_job_kind(BackgroundJobKind::RecentChanges);
                self.cancel_background_job_kind(BackgroundJobKind::RecentChangesPrefetch);
                self.current_recent_changes_job_id = None;
                self.status = StatusMessage::info("Git log closed.");
            }
            HitAction::ScrollRecentChanges(delta) => self.scroll_recent_changes(delta),
            HitAction::SelectRecentChangeLine(view, line_index) => {
                self.select_recent_change_line(view, line_index)
            }
            HitAction::ToggleCommitRenameForcePush => self.toggle_commit_rename_force_push(),
            HitAction::SaveCommitRename => return self.apply_commit_rename(),
            HitAction::CancelCommitRename => {
                self.commit_rename_dialog = None;
                self.status = StatusMessage::info("Commit rename cancelled.");
            }
            HitAction::ReleaseNowNotesField => {}
            HitAction::CommitRenameMessageField => {}
            HitAction::OpenTagDialog => return self.open_tag_dialog(),
            HitAction::OpenTagAnnotation => return self.open_tag_annotation_dialog(),
            HitAction::CycleTagScope(delta) => self.rotate_tag_scope(delta),
            HitAction::CycleTagAction(delta) => self.rotate_tag_action(delta),
            HitAction::CreateTag => return self.create_local_tag(),
            HitAction::SaveTagAnnotation => return self.save_tag_annotation(),
            HitAction::CancelTagAnnotation => {
                self.tag_annotation_dialog = None;
                self.status = StatusMessage::info("Tag annotation editor closed.");
            }
            HitAction::CancelTagDialog => {
                self.tag_dialog = None;
                self.tag_annotation_dialog = None;
                self.status = StatusMessage::info("Tag creation cancelled.");
            }
            HitAction::WizardField(field) => self.wizard.focus = field,
            HitAction::WizardScopeAction(action) => return self.apply_wizard_scope_action(action),
            HitAction::ValidateWizard => self.validate_wizard_target(),
            HitAction::SaveWizard => return self.save_wizard_project(),
            HitAction::CancelWizard => {
                self.screen = Screen::Dashboard;
                self.status = StatusMessage::info("Wizard cancelled.");
            }
            HitAction::SaveTopPicks => return self.save_top_picks(),
            HitAction::CancelTopPicks => {
                self.top_picks_editor_dialog = None;
                self.status = StatusMessage::info("Top Picks editor closed.");
            }
            HitAction::TopPicksEditorField => {}
        }
        Ok(())
    }

    pub(crate) fn open_recent_changes(&mut self) -> Result<()> {
        let preferred_scope = self.selected_project().ok().and_then(|project| {
            (!project.unified_versioning).then_some(self.overview_focused_scope)
        });
        self.open_recent_changes_with_scope(preferred_scope)
    }

    pub(crate) fn open_overview_tag_dialog(&mut self, scope_index: usize) -> Result<()> {
        let project = self.selected_project()?.clone();
        let preferred_scope = if project.unified_versioning {
            None
        } else {
            Some(scope_index)
        };
        self.open_tag_dialog_with_scope(preferred_scope, None)
    }

    pub(crate) fn open_overview_release_now(&mut self, scope_index: usize) -> Result<()> {
        self.open_release_now_with_scope(Some(scope_index))
    }

    pub(crate) fn open_release_now_with_scope(
        &mut self,
        preferred_scope: Option<usize>,
    ) -> Result<()> {
        let project = self.selected_project()?.clone();
        let scope_index = preferred_scope.unwrap_or_else(|| {
            if project.project_type == ProjectType::Branched {
                self.overview_focused_scope
                    .min(project.branches.len().saturating_sub(1))
            } else {
                0
            }
        });

        self.bump_dialog = None;
        self.tag_dialog = None;
        self.tag_annotation_dialog = None;
        self.release_now_dialog = None;
        self.release_now_notes_dialog = None;
        self.project_edit_dialog = None;
        self.browser_dialog = None;
        self.schedule_progress_job(
            " Validating ReleaseNOW ",
            format!("Checking ReleaseNOW prerequisites for {}.", project.name),
            BackgroundJobRequest::ValidateReleaseNow {
                project,
                scope_index,
            },
        )?;
        self.status =
            StatusMessage::info("Validating ReleaseNOW prerequisites for the selected scope.");
        Ok(())
    }

    pub(crate) fn open_recent_changes_with_scope(
        &mut self,
        preferred_scope: Option<usize>,
    ) -> Result<()> {
        let project = self.selected_project()?.clone();
        if !project.integration_mode.requires_repo() {
            bail!("git log requires a git-backed project");
        }

        self.bump_dialog = None;
        self.tag_dialog = None;
        self.project_edit_dialog = None;
        self.schedule_progress_job(
            " Loading Git Commits ",
            format!("Loading git history for {}.", project.name),
            BackgroundJobRequest::OpenRecentChanges {
                project,
                preferred_scope,
            },
        )?;
        self.status = StatusMessage::info("Loading git history for the selected project.");
        Ok(())
    }

    pub(crate) fn open_release_now_notes_dialog(&mut self) -> Result<()> {
        let dialog = self
            .release_now_dialog
            .as_ref()
            .ok_or_else(|| anyhow!("ReleaseNOW is not open"))?;
        if !dialog.attach_changelog {
            bail!("enable changelog attachment before editing release notes")
        }
        self.release_now_notes_dialog = Some(TagAnnotationDialog::with_placeholder(
            &dialog.release_notes_markdown,
            dialog.release_notes_placeholder.as_str(),
        ));
        self.status = StatusMessage::info("Editing ReleaseNOW release notes.");
        Ok(())
    }

    pub(crate) fn save_release_now_notes(&mut self) -> Result<()> {
        let notes = self
            .release_now_notes_dialog
            .as_ref()
            .ok_or_else(|| anyhow!("ReleaseNOW release notes editor is not open"))?
            .editor
            .lines()
            .join("\n");
        let dialog = self
            .release_now_dialog
            .as_mut()
            .ok_or_else(|| anyhow!("ReleaseNOW is not open"))?;
        dialog.release_notes_markdown = notes;
        self.release_now_notes_dialog = None;
        self.status = StatusMessage::success("ReleaseNOW release notes updated.");
        Ok(())
    }

    pub(crate) fn active_top_picks_scope(
        &self,
        project: &ProjectConfig,
    ) -> Result<GitScopeContext> {
        if let Some(dialog) = &self.release_now_dialog {
            return Ok(dialog.scope.clone());
        }

        let git_contexts = collect_all_branch_git_scope_contexts(project)?;
        let scope_index = if project.project_type == ProjectType::Branched {
            self.overview_focused_scope
                .min(git_contexts.len().saturating_sub(1))
        } else {
            0
        };
        git_contexts
            .get(scope_index)
            .cloned()
            .ok_or_else(|| anyhow!("no git scope available for this project"))
    }

    pub(crate) fn refresh_release_now_notes_from_current_scope(&mut self) -> Result<bool> {
        let Some((tag_name, scope)) = self
            .release_now_dialog
            .as_ref()
            .map(|dialog| (dialog.tag_name.clone(), dialog.scope.clone()))
        else {
            return Ok(false);
        };

        let markdown = build_release_notes_markdown(&tag_name, &scope)?;
        if let Some(dialog) = &mut self.release_now_dialog {
            dialog.release_notes_markdown = markdown;
        }
        Ok(true)
    }

    pub(crate) fn open_top_picks_editor(&mut self) -> Result<()> {
        let project = self.selected_project()?.clone();
        let scope = self.active_top_picks_scope(&project)?;

        // Get the repo root for the active release/dashboard scope.
        let repo_root = scope.repo_root.as_str();

        let current_baseline = resolve_top_picks_baseline_tag(repo_root);
        let (saved_baseline, memory_content) = load_top_picks_edits_with_baseline(repo_root);
        let memory_matches_baseline = saved_baseline.as_deref() == current_baseline.as_deref();
        let has_memory_edits = !memory_content.trim().is_empty() && memory_matches_baseline;

        if has_memory_edits {
            self.top_picks_editor_dialog = Some(changelog_tp::TopPicksEditorDialog::with_text(
                &memory_content,
            ));
            self.status = StatusMessage::info(
                "Top Picks editor opened with saved edits. These will be applied during release.",
            );
        } else {
            if !memory_content.trim().is_empty() && !memory_matches_baseline {
                let _ = crate::changelog::clear_top_picks_edits(repo_root);
            }
            // No memory edits, extract from commits as before
            let mut existing_picks = project.manual_top_picks.clone();

            if let Ok(picks_from_commits) = self.extract_top_picks_from_commits(&scope) {
                // Merge picks from commits with manual picks (manual takes precedence)
                let mut seen_headers: std::collections::HashSet<String> =
                    existing_picks.iter().map(|p| p.header.clone()).collect();
                for pick in picks_from_commits {
                    if !seen_headers.contains(&pick.header) {
                        seen_headers.insert(pick.header.clone());
                        existing_picks.push(pick);
                    }
                }
                // Sort by priority
                changelog_tp::sort_top_picks(&mut existing_picks);
            }

            self.top_picks_editor_dialog = Some(changelog_tp::TopPicksEditorDialog::with_picks(
                &existing_picks,
            ));
            self.status = StatusMessage::info(
                "Top Picks editor opened. Edit using the format: '1. Header' followed by '- Bullet points'",
            );
        }
        Ok(())
    }

    pub(crate) fn extract_top_picks_from_commits(
        &self,
        scope: &GitScopeContext,
    ) -> Result<Vec<changelog_tp::TopPick>> {
        let repo_root = &scope.repo_root;

        let revision_range = match resolve_top_picks_baseline_tag(repo_root) {
            Some(tag) => format!("{}..HEAD", tag),
            None => return Ok(Vec::new()),
        };

        let pathspecs = scope.git_pathspecs();
        let mut args = vec![
            "--no-pager".to_string(),
            "log".to_string(),
            "--pretty=format:%H %s%n%b---COMMIT_END---".to_string(),
            "--no-merges".to_string(),
            revision_range,
        ];
        if !pathspecs.is_empty() {
            args.push("--".to_string());
            args.extend(pathspecs);
        }
        let arg_refs = args.iter().map(String::as_str).collect::<Vec<_>>();

        // Get git log output for commits since last tag
        let output = run_git(repo_root, &arg_refs)?;

        let log_output = output.stdout;

        // Parse commits from log output
        let mut parsed_commits: Vec<ParsedCommit> = Vec::new();
        let commit_blocks: Vec<&str> = log_output.split("---COMMIT_END---").collect();
        for commit_block in commit_blocks.iter().rev() {
            let block = commit_block.trim();
            if block.is_empty() {
                continue;
            }

            let lines: Vec<&str> = block.lines().collect();
            if lines.is_empty() {
                continue;
            }

            let first_line = lines[0];
            if let Some((hash, subject)) = first_line.split_once(' ') {
                let body = if lines.len() > 1 {
                    lines[1..].join("\n")
                } else {
                    String::new()
                };

                let full_message = if body.is_empty() {
                    subject.to_string()
                } else {
                    format!("{}\n{}", subject, body)
                };

                let commits = ParsedCommit::parse_many(&full_message, hash.to_string());
                parsed_commits.extend(commits);
            }
        }

        // Extract top picks
        let refs: Vec<&ParsedCommit> = parsed_commits.iter().collect();
        let picks = changelog_tp::extract_top_picks(&refs);

        Ok(picks)
    }

    pub(crate) fn save_top_picks(&mut self) -> Result<()> {
        let dialog = self
            .top_picks_editor_dialog
            .as_ref()
            .ok_or_else(|| anyhow!("Top Picks editor is not open"))?;

        // Get the raw text content from the editor
        let text_content = dialog.editor.lines().join("\n");

        // Save to memory file in the project's repo
        let project = self.selected_project()?;
        let scope = self.active_top_picks_scope(project)?;
        let repo_root = &scope.repo_root;
        let mut warnings = Vec::new();

        // Ensure gitignore entry exists
        if let Err(error) = ensure_gitignore_entry(repo_root, ".comfygit/mem/.tp_edits.md") {
            warnings.push(format!("failed to update .gitignore: {}", error));
        }

        // Save the edits to memory file
        if let Err(error) = save_top_picks_edits(repo_root, &text_content) {
            self.status =
                StatusMessage::error(format!("Failed to save Top Picks edits: {}", error));
            return Ok(());
        }

        if let Err(error) = self.refresh_release_now_notes_from_current_scope() {
            warnings.push(format!("failed to refresh ReleaseNOW notes: {}", error));
        }

        self.status = if warnings.is_empty() {
            StatusMessage::success(
                "Top Picks edits saved. They will be applied during the next release.".to_string(),
            )
        } else {
            StatusMessage::warning(format!(
                "Top Picks edits saved, but {}",
                warnings.join(" Also, ")
            ))
        };
        self.top_picks_editor_dialog = None;
        Ok(())
    }

    pub(crate) fn request_run_release_now(&mut self) -> Result<()> {
        let request = {
            let dialog = self
                .release_now_dialog
                .as_ref()
                .ok_or_else(|| anyhow!("ReleaseNOW is not open"))?;
            if dialog.is_warning_mode() {
                bail!("confirm the recent bump warning before running ReleaseNOW")
            }
            if dialog.is_existing_artifacts_mode() {
                bail!(
                    "choose whether to reuse or rebuild existing artifacts before running ReleaseNOW"
                )
            }
            if dialog.is_running() {
                bail!("ReleaseNOW is already running")
            }
            if dialog.is_completed() {
                self.close_release_now_dialog();
                return Ok(());
            }

            rls_now::build_execution_request(dialog)
        };

        if let Some(dialog) = &mut self.release_now_dialog {
            dialog.begin_running();
        }

        self.schedule_foreground_job(BackgroundJobRequest::RunReleaseNow { request })?;
        self.status = StatusMessage::info(
            "Running ReleaseNOW for the selected scope. Live logs will stream into the dialog.",
        );
        Ok(())
    }

    pub(crate) fn close_release_now_dialog(&mut self) {
        if self
            .release_now_dialog
            .as_ref()
            .map(rls_now::ReleaseNowDialog::is_running)
            .unwrap_or(false)
        {
            self.status = StatusMessage::warning(
                "ReleaseNOW is still running. Wait for it to finish before closing the dialog.",
            );
            return;
        }

        self.release_now_notes_dialog = None;
        self.release_now_dialog = None;
        self.status = StatusMessage::info("ReleaseNOW closed.");
    }

    pub(crate) fn scroll_release_now(&mut self, delta: i16) {
        if let Some(dialog) = &mut self.release_now_dialog {
            dialog.scroll_by(delta);
        }
    }

    pub(crate) fn scroll_release_now_to_start(&mut self) {
        if let Some(dialog) = &mut self.release_now_dialog {
            dialog.scroll_to_start();
        }
    }

    pub(crate) fn scroll_release_now_to_end(&mut self) {
        if let Some(dialog) = &mut self.release_now_dialog {
            dialog.scroll_to_tail();
        }
    }

    pub(crate) fn begin_release_now_log_selection(&mut self, mouse_row: u16) -> bool {
        let Some(viewport) = self.release_now_log_viewport else {
            return false;
        };
        let Some(dialog) = &mut self.release_now_dialog else {
            return false;
        };

        dialog.begin_body_selection(mouse_row.saturating_sub(viewport.y))
    }

    pub(crate) fn update_release_now_log_selection(&mut self, mouse_row: u16) -> bool {
        let Some(viewport) = self.release_now_log_viewport else {
            return false;
        };
        let Some(dialog) = &mut self.release_now_dialog else {
            return false;
        };

        dialog.update_body_selection(mouse_row.saturating_sub(viewport.y))
    }

    pub(crate) fn copy_selected_release_now_log(&mut self, mouse_row: u16) {
        let Some(viewport) = self.release_now_log_viewport else {
            return;
        };
        let Some(dialog) = &mut self.release_now_dialog else {
            return;
        };

        if !dialog.has_body_selection() {
            let _ = dialog.begin_body_selection(mouse_row.saturating_sub(viewport.y));
        }

        if let Some(text) = dialog.selected_body_text() {
            self.copy_text_to_clipboard(&text);
        }
    }

    pub(crate) fn toggle_release_now_auto_follow(&mut self) {
        if let Some(dialog) = &mut self.release_now_dialog {
            let enabled = dialog.toggle_auto_follow();
            self.status = StatusMessage::info(if enabled {
                "ReleaseNOW auto-follow resumed."
            } else {
                "ReleaseNOW auto-follow paused."
            });
        }
    }

    pub(crate) fn request_cancel_release_now(&mut self) {
        let Some(dialog) = &mut self.release_now_dialog else {
            return;
        };
        if !dialog.is_running() {
            return;
        }
        if dialog.cancel_requested() {
            self.status = StatusMessage::warning("ReleaseNOW cancellation is already in progress.");
            return;
        }

        if let Some(cancel) = &self.current_release_now_cancel {
            cancel.cancel();
            dialog.mark_cancel_requested();
            self.status = StatusMessage::warning(
                "Cancelling ReleaseNOW. Waiting for the current step to stop.",
            );
        }
    }

    pub(crate) fn schedule_foreground_job(&mut self, request: BackgroundJobRequest) -> Result<u64> {
        if self.background_job_active {
            bail!("another background job is already running");
        }

        let request_id =
            self.schedule_background_job(BackgroundJobPriority::Foreground, request)?;
        self.background_job_active = true;
        self.active_foreground_job_id = Some(request_id);

        Ok(request_id)
    }

    pub(crate) fn schedule_progress_job(
        &mut self,
        title: impl Into<String>,
        message: impl Into<String>,
        request: BackgroundJobRequest,
    ) -> Result<()> {
        let request_id = self.schedule_foreground_job(request)?;

        self.progress_dialog = Some(ProgressDialog {
            title: title.into(),
            message: message.into(),
        });

        debug_assert_eq!(self.active_foreground_job_id, Some(request_id));

        Ok(())
    }

    pub(crate) fn try_finish_background_job(&mut self) -> Result<bool> {
        if self.background_jobs_inflight == 0 {
            return Ok(false);
        }

        let message = match self.background_result_rx.try_recv() {
            Ok(message) => message,
            Err(TryRecvError::Empty) => return Ok(false),
            Err(TryRecvError::Disconnected) => {
                self.progress_dialog = None;
                self.background_job_active = false;
                self.active_foreground_job_id = None;
                self.background_jobs_inflight = 0;
                bail!("background worker stopped unexpectedly");
            }
        };

        let terminal = matches!(message.payload, BackgroundJobMessagePayload::Finished(_));

        if terminal {
            self.background_jobs_inflight = self.background_jobs_inflight.saturating_sub(1);
            if self.active_foreground_job_id == Some(message.id) {
                self.progress_dialog = None;
                self.background_job_active = false;
                self.active_foreground_job_id = None;
            }

            if self.current_overview_activity_job_id == Some(message.id)
                && message.kind == BackgroundJobKind::OverviewActivity
            {
                self.overview_activity_job_inflight = false;
                if self.overview_activity_refresh_inflight {
                    self.overview_activity_refresh_inflight = false;
                    if self.overview_activity_refresh_pending {
                        self.overview_activity_refresh_pending = false;
                        self.schedule_refresh_overview_activity_cache()?;
                    }
                }
            }
        }

        if self.is_background_result_stale(&message) {
            return Ok(true);
        }

        match message.payload {
            BackgroundJobMessagePayload::Progress(output) => {
                self.apply_background_job_output(output)?
            }
            BackgroundJobMessagePayload::Finished(result) => match result {
                Ok(output) => self.apply_background_job_output(output)?,
                Err(error_message) => {
                    let release_now_error = if message.kind == BackgroundJobKind::ReleaseNow {
                        Some(rls_now::format_user_facing_error(&error_message))
                    } else {
                        None
                    };
                    if message.kind == BackgroundJobKind::ReleaseNow
                        && let Some(dialog) = &mut self.release_now_dialog
                    {
                        if rls_now::is_cancelled_error(&error_message) {
                            dialog.apply_cancelled(error_message.clone());
                        } else {
                            dialog.apply_failure(error_message.clone());
                        }
                    }
                    self.status = if message.kind == BackgroundJobKind::ReleaseNow
                        && rls_now::is_cancelled_error(&error_message)
                    {
                        StatusMessage::warning(error_message)
                    } else if let Some(formatted_error) = release_now_error {
                        StatusMessage::error(formatted_error)
                    } else {
                        StatusMessage::error(error_message)
                    };
                }
            },
        }

        Ok(true)
    }

    pub(crate) fn apply_background_job_output(
        &mut self,
        output: BackgroundJobOutput,
    ) -> Result<()> {
        match output {
            BackgroundJobOutput::OpenRecentChanges(dialog) => {
                self.recent_changes_dialog = Some(dialog);
                let _ = self.schedule_recent_changes_prefetch();
                self.status = StatusMessage::info("Showing git log for the selected project.");
            }
            BackgroundJobOutput::PendingBumpMainBranch {
                integration_mode,
                repos,
                pending_action,
            } => {
                if repos.is_empty() {
                    self.resume_pending_bump_action(pending_action)?;
                } else {
                    self.main_branch_warning_dialog = Some(MainBranchWarningDialog::new(
                        integration_mode,
                        repos,
                        pending_action,
                    ));
                    self.status = StatusMessage::warning(
                        "It seems like you are not on main branch. Please, choose what would you like to do...",
                    );
                }
            }
            BackgroundJobOutput::OverviewBumpWarnings {
                scope_index,
                workflow,
                warnings,
            } => {
                if warnings.is_empty() {
                    overview::continue_overview_bump_workflow_confirmation(
                        self,
                        scope_index,
                        workflow,
                    )?;
                } else {
                    self.overview_bump_warning_dialog = Some(OverviewBumpWarningDialog::new(
                        scope_index,
                        workflow,
                        warnings,
                    ));
                    self.status = StatusMessage::warning(
                        "Previously staged files were found. Review them before committing the bump.",
                    );
                }
            }
            BackgroundJobOutput::RecentChanges {
                dialog,
                status_message,
            } => {
                self.recent_changes_dialog = Some(dialog);
                let _ = self.schedule_recent_changes_prefetch();
                if let Some(message) = status_message {
                    self.status = StatusMessage::info(message);
                }
            }
            BackgroundJobOutput::RecentChangesPrefetch {
                project_name,
                next_scope_index,
                prefetched_recent_range,
                history_scope_index,
                prefetched_history_ranges,
            } => {
                if let Some(dialog) = &mut self.recent_changes_dialog
                    && dialog.project_name == project_name
                {
                    if let (Some(scope_index), Some(range)) =
                        (next_scope_index, prefetched_recent_range)
                    {
                        dialog.apply_prefetched_recent_range(scope_index, range);
                    }
                    if let (Some(scope_index), Some(ranges)) =
                        (history_scope_index, prefetched_history_ranges)
                    {
                        dialog.apply_prefetched_history_ranges(scope_index, ranges);
                    }
                }
            }
            BackgroundJobOutput::OpenChangelogPreview(dialog) => {
                self.open_changelog_preview(*dialog)
            }
            BackgroundJobOutput::OverviewActivityCache {
                project_index,
                summaries,
            } => {
                if self.selected_project == project_index {
                    self.overview_activity_summaries = summaries;
                    self.overview_activity_project = Some(project_index);
                }
            }
            BackgroundJobOutput::ReleaseNowValidated(validation) => {
                let project_name = validation.project_name.clone();
                let warning_pending = validation.warning_message.is_some();
                self.release_now_dialog =
                    Some(rls_now::ReleaseNowDialog::from_validation(validation));
                self.release_now_notes_dialog = None;
                self.status = if warning_pending {
                    StatusMessage::warning(
                        "ReleaseNOW found an older-than-expected bump. Confirm before continuing.",
                    )
                } else {
                    StatusMessage::info(format!("ReleaseNOW is ready for {}.", project_name))
                };
            }
            BackgroundJobOutput::ReleaseNowLogChunk(lines) => {
                if let Some(dialog) = &mut self.release_now_dialog {
                    dialog.append_log_lines(lines);
                }
            }
            BackgroundJobOutput::ReleaseNowCompleted(outcome) => {
                let summary = outcome.summary.clone();
                if let Some(dialog) = &mut self.release_now_dialog {
                    dialog.apply_outcome(outcome);
                }
                self.status = StatusMessage::success(summary);
            }
            BackgroundJobOutput::CreateTag {
                summary,
                replay_notices,
                replay_errors,
            } => {
                self.sync_dashboard_overview_after_repo_change();
                self.tag_dialog = None;
                self.tag_annotation_dialog = None;
                self.status = StatusMessage::success(summary);
                for notice in replay_notices {
                    self.show_transient_toast(StatusKind::Info, notice);
                }
                for error in replay_errors {
                    self.show_sticky_error_toast(error);
                }
            }
        }

        Ok(())
    }

    pub(crate) fn schedule_recent_changes_action(
        &mut self,
        message: impl Into<String>,
        action: RecentChangesLoadAction,
    ) -> Result<()> {
        let dialog = self
            .recent_changes_dialog
            .clone()
            .ok_or_else(|| anyhow!("git log is not open"))?;

        self.schedule_progress_job(
            " Loading Git Commits ",
            message,
            BackgroundJobRequest::RecentChanges { dialog, action },
        )
    }

    pub(crate) fn schedule_overview_workflow_changelog_preview(
        &mut self,
        scope_index: usize,
        workflow: OverviewBumpWorkflow,
    ) -> Result<()> {
        let project = self.selected_project()?.clone();
        self.schedule_progress_job(
            " Generating Changelog ",
            "Building changelog preview from current git history.",
            BackgroundJobRequest::OpenOverviewWorkflowChangelog {
                project,
                scope_index,
                workflow,
                pending_versions: self.overview_pending_versions.clone(),
            },
        )?;
        self.status = StatusMessage::info("Generating changelog preview from current git history.");
        Ok(())
    }

    pub(crate) fn ensure_dashboard_recent_changes(&mut self) {
        overview::ensure_dashboard_recent_changes(self);
    }

    pub(crate) fn invalidate_overview_cache(&mut self) {
        overview::invalidate_overview_cache(self);
    }

    pub(crate) fn prime_selected_project_dashboard_data(&mut self) {
        self.ensure_dashboard_recent_changes();
        let _ = self.schedule_prefetch_overview_activity_cache();
    }

    pub(crate) fn next_poll_timeout(&self) -> Duration {
        if self.background_jobs_inflight > 0 || self.toaster.has_toast() {
            ACTIVE_UI_TICK_INTERVAL
        } else {
            IDLE_UI_POLL_INTERVAL
        }
    }

    pub(crate) fn tick_ui_state(&mut self) -> bool {
        let had_toast = self.toaster.has_toast();
        self.toaster.tick();

        had_toast
            || self.toaster.has_toast() != had_toast
            || overview::tick_dashboard_tile_rotation(self)
    }

    pub(crate) fn sync_dashboard_overview_after_repo_change(&mut self) {
        self.invalidate_overview_cache();
        self.ensure_dashboard_recent_changes();
        let _ = self.schedule_refresh_overview_activity_cache();
    }

    pub(crate) fn reload_dashboard_overview_data(&mut self) -> Result<()> {
        let project = self.selected_project()?;
        if !project.integration_mode.requires_repo() {
            self.status =
                StatusMessage::info("Selected project has no git-backed dashboard data to reload.");
            return Ok(());
        }

        let preferred_scope = self
            .overview_recent_changes
            .as_ref()
            .map(|dialog| dialog.selected_scope)
            .unwrap_or(self.overview_focused_scope);

        self.invalidate_overview_cache();
        self.ensure_dashboard_recent_changes();
        if let Some(dialog) = &mut self.overview_recent_changes {
            let scope_index = preferred_scope.min(dialog.scopes.len().saturating_sub(1));
            if scope_index != dialog.selected_scope {
                dialog.select_scope(scope_index)?;
            }
        }
        self.schedule_refresh_overview_activity_cache()?;
        self.status =
            StatusMessage::info("Refreshing dashboard repo data for the selected project.");
        Ok(())
    }

    pub(crate) fn reorder_dashboard_tile_scope(&mut self, from_scope: usize, to_scope: usize) {
        overview::reorder_dashboard_tile_scope(self, from_scope, to_scope);
    }

    pub(crate) fn scroll_dashboard_tiles(&mut self, delta: isize) -> Result<()> {
        overview::scroll_dashboard_tiles(self, delta)
    }

    pub(crate) fn cycle_overview_tile_info(
        &mut self,
        scope_index: usize,
        row: OverviewTileInfoRow,
    ) -> Result<()> {
        overview::cycle_overview_tile_info(self, scope_index, row)
    }

    pub(crate) fn move_dashboard_overview_focus(&mut self, delta: isize) -> Result<()> {
        overview::move_dashboard_overview_focus(self, delta)
    }

    pub(crate) fn select_dashboard_overview_scope(&mut self, scope_index: usize) -> Result<()> {
        overview::select_dashboard_overview_scope(self, scope_index)
    }

    pub(crate) fn begin_overview_bump(&mut self, scope_index: usize) -> Result<()> {
        overview::begin_overview_bump(self, scope_index)
    }

    pub(crate) fn select_overview_bump_kind(&mut self, index: usize) {
        overview::select_overview_bump_kind(self, index);
    }

    pub(crate) fn rotate_overview_bump_kind(&mut self, delta: isize) {
        overview::rotate_overview_bump_kind(self, delta);
    }

    pub(crate) fn cancel_overview_bump_kind(&mut self) {
        overview::cancel_overview_bump_kind(self);
    }

    pub(crate) fn confirm_overview_bump_kind(&mut self) -> Result<()> {
        overview::confirm_overview_bump_kind(self)
    }

    pub(crate) fn select_overview_bump_workflow(&mut self, index: usize) {
        overview::select_overview_bump_workflow(self, index);
    }

    pub(crate) fn rotate_overview_bump_workflow(&mut self, delta: isize) {
        overview::rotate_overview_bump_workflow(self, delta);
    }

    pub(crate) fn cancel_overview_bump_workflow(&mut self) {
        overview::cancel_overview_bump_workflow(self);
    }

    pub(crate) fn select_overview_bump_warning(&mut self, index: usize) {
        overview::select_overview_bump_warning(self, index);
    }

    pub(crate) fn rotate_overview_bump_warning(&mut self, delta: isize) {
        overview::rotate_overview_bump_warning(self, delta);
    }

    pub(crate) fn cancel_overview_bump_warning(&mut self) {
        self.overview_bump_warning_dialog = None;
        self.overview_bump_workflow_dialog = None;
        self.status = bump_toast_status("Tile bump action cancelled.");
    }

    pub(crate) fn adjust_overview_pending_version(
        &mut self,
        scope_index: usize,
        control: OverviewVersionControl,
        delta: i32,
    ) -> Result<()> {
        overview::adjust_overview_pending_version(self, scope_index, control, delta)
    }

    pub(crate) fn reset_overview_pending_version(&mut self, scope_index: usize) -> Result<()> {
        overview::reset_overview_pending_version(self, scope_index)
    }

    pub(crate) fn open_dashboard_changelog_preview(
        &mut self,
        selection: Option<CustomChangelogSelection>,
    ) -> Result<()> {
        let project = self.selected_project()?.clone();
        let scope_index = if project.project_type == ProjectType::Branched {
            self.overview_focused_scope
                .min(project.branches.len().saturating_sub(1))
        } else {
            0
        };
        if !project.integration_mode.requires_repo() {
            bail!("changelog preview requires a git-backed project");
        }
        if !project.changelog_enabled_for_scope(scope_index) {
            bail!("changelog generation is disabled for the selected scope");
        }

        self.schedule_progress_job(
            " Generating Changelog ",
            "Building custom changelog preview.",
            BackgroundJobRequest::OpenDashboardChangelogPreview {
                project,
                scope_index,
                pending_versions: self.overview_pending_versions.clone(),
                selection,
            },
        )?;
        self.status = StatusMessage::info("Generating custom changelog preview.");
        Ok(())
    }

    pub(crate) fn request_confirm_overview_bump_workflow(&mut self) -> Result<()> {
        let Some(dialog) = &self.overview_bump_workflow_dialog else {
            return Ok(());
        };
        if !self.are_we_on_main(PendingBumpAction::OverviewWorkflow {
            scope_index: dialog.scope_index,
        })? {
            return Ok(());
        }
        self.confirm_overview_bump_workflow()
    }

    pub(crate) fn confirm_overview_bump_workflow(&mut self) -> Result<()> {
        overview::confirm_overview_bump_workflow(self)
    }

    pub(crate) fn confirm_overview_bump_warning(&mut self) -> Result<()> {
        overview::confirm_overview_bump_warning(self)
    }

    pub(crate) fn confirm_overview_branch_bump(&mut self) -> Result<()> {
        overview::confirm_overview_branch_bump(self)
    }

    pub(crate) fn cancel_overview_branch_bump(&mut self) {
        self.overview_branch_bump_dialog = None;
        self.status = bump_toast_status("Tile bump action cancelled.");
    }

    pub(crate) fn are_we_on_main(&mut self, pending_action: PendingBumpAction) -> Result<bool> {
        let project = self.selected_project()?.clone();
        if !project.integration_mode.requires_repo() {
            return Ok(true);
        }

        let affected_scope_indexes =
            self.affected_scope_indexes_for_pending_bump(pending_action)?;
        if affected_scope_indexes.is_empty() {
            return Ok(true);
        }

        self.schedule_progress_job(
            " Checking Branch State ",
            "Checking repositories for non-main branches before continuing.",
            BackgroundJobRequest::CheckPendingBumpMainBranch {
                project,
                affected_scope_indexes,
                pending_action,
            },
        )?;
        self.status =
            StatusMessage::info("Checking repositories for non-main branches before continuing.");
        Ok(false)
    }

    pub(crate) fn affected_scope_indexes_for_pending_bump(
        &self,
        pending_action: PendingBumpAction,
    ) -> Result<Vec<usize>> {
        let project = self.selected_project()?;
        match pending_action {
            PendingBumpAction::Standard => {
                let dialog = self
                    .bump_dialog
                    .as_ref()
                    .ok_or_else(|| anyhow!("no bump preview is in progress"))?;
                if dialog.unified_versioning {
                    Ok((0..dialog.scopes.len()).collect())
                } else {
                    Ok(vec![dialog.selected_scope])
                }
            }
            PendingBumpAction::OverviewWorkflow { scope_index } => {
                if project.unified_versioning {
                    Ok((0..project.branches.len().max(1)).collect())
                } else {
                    Ok(vec![scope_index])
                }
            }
        }
    }

    pub(crate) fn select_main_branch_warning(&mut self, index: usize) {
        if let Some(dialog) = &mut self.main_branch_warning_dialog {
            dialog.select(index);
        }
    }

    pub(crate) fn rotate_main_branch_warning(&mut self, delta: isize) {
        if let Some(dialog) = &mut self.main_branch_warning_dialog {
            dialog.rotate(delta);
        }
    }

    pub(crate) fn cancel_main_branch_warning(&mut self) {
        self.main_branch_warning_dialog = None;
        self.status = StatusMessage::info("Bump cancelled.");
    }

    pub(crate) fn confirm_main_branch_warning(&mut self) -> Result<()> {
        let Some(dialog) = self.main_branch_warning_dialog.clone() else {
            return Ok(());
        };

        match dialog.selected_choice() {
            MainBranchWarningChoice::SwitchToMain => {
                git_flow::switch_repos_to_main(&dialog.repos, dialog.integration_mode)?;
                self.main_branch_warning_dialog = None;
                self.resume_pending_bump_action(dialog.pending_action)?;
            }
            MainBranchWarningChoice::IgnoreAndContinue => {
                self.main_branch_warning_dialog = None;
                self.resume_pending_bump_action(dialog.pending_action)?;
            }
            MainBranchWarningChoice::Cancel => self.cancel_main_branch_warning(),
        }
        Ok(())
    }

    pub(crate) fn open_changelog_preview(&mut self, dialog: ChangelogPreviewDialog) {
        self.pending_changelog_write = None;
        let preview_only = dialog.workflow.is_none();
        let custom_range = dialog.custom_range.is_some();
        self.changelog_preview_dialog = Some(dialog);
        self.status = StatusMessage::info(if preview_only {
            if custom_range {
                "Showing the custom changelog preview. Use Tab to switch From/To, Left/Right to change the range, and Ctrl+S to save changelog_temp.md."
            } else {
                "Showing the generated changelog preview for the current git history."
            }
        } else {
            "Review the generated changelog, add an optional release message, then confirm the bump."
        });
    }

    pub(crate) fn cancel_changelog_preview(&mut self) {
        self.changelog_preview_dialog = None;
        self.cancel_background_job_kind(BackgroundJobKind::ChangelogPreview);
        self.current_changelog_preview_job_id = None;
        self.pending_changelog_write = None;
        self.status = StatusMessage::info("Changelog preview closed.");
    }

    pub(crate) fn save_changelog_preview(&mut self) -> Result<()> {
        let Some(dialog) = self.changelog_preview_dialog.as_ref() else {
            return Ok(());
        };

        let release_message = dialog.release_message_value();
        let written_paths = dialog
            .entries
            .iter()
            .map(|entry| {
                let markdown = entry.rendered_markdown(&release_message);
                write_temp_changelog_markdown(&entry.repo_root, &markdown)
            })
            .collect::<Result<Vec<_>>>()?;

        self.status = if written_paths.len() == 1 {
            StatusMessage::success(format!(
                "Saved changelog preview to {}.",
                written_paths[0].display()
            ))
        } else {
            StatusMessage::success(format!(
                "Saved changelog previews to changelog_temp.md in {} repositories.",
                written_paths.len()
            ))
        };
        Ok(())
    }

    pub(crate) fn scroll_changelog_preview(&mut self, delta: i16) {
        if let Some(dialog) = &mut self.changelog_preview_dialog {
            let max_scroll = dialog
                .preview_line_count()
                .saturating_sub(1)
                .min(u16::MAX as usize) as u16;
            if delta.is_negative() {
                dialog.scroll = dialog.scroll.saturating_sub(delta.unsigned_abs());
            } else {
                dialog.scroll = dialog.scroll.saturating_add(delta as u16);
            }
            dialog.scroll = dialog.scroll.min(max_scroll);
        }
    }

    pub(crate) fn scroll_changelog_preview_to_start(&mut self) {
        if let Some(dialog) = &mut self.changelog_preview_dialog {
            dialog.scroll = 0;
        }
    }

    pub(crate) fn scroll_changelog_preview_to_end(&mut self) {
        if let Some(dialog) = &mut self.changelog_preview_dialog {
            let max_scroll = dialog
                .preview_line_count()
                .saturating_sub(1)
                .min(u16::MAX as usize) as u16;
            dialog.scroll = max_scroll;
        }
    }

    pub(crate) fn confirm_changelog_preview(&mut self) -> Result<()> {
        let Some(dialog) = self.changelog_preview_dialog.take() else {
            return Ok(());
        };

        if dialog.workflow.is_none() {
            self.cancel_background_job_kind(BackgroundJobKind::ChangelogPreview);
            self.current_changelog_preview_job_id = None;
            self.status = StatusMessage::info("Changelog preview closed.");
            return Ok(());
        }

        self.pending_changelog_write = Some(dialog.prepare_pending_write());
        self.cancel_background_job_kind(BackgroundJobKind::ChangelogPreview);
        self.current_changelog_preview_job_id = None;
        let branch_name = dialog
            .workflow
            .filter(|workflow| workflow.requires_branch())
            .and_then(|_| {
                self.overview_branch_bump_dialog
                    .as_ref()
                    .map(|branch_dialog| branch_dialog.branch_name.value.trim().to_string())
            });
        overview::execute_overview_bump_workflow(
            self,
            dialog.scope_index,
            dialog
                .workflow
                .expect("workflow preview should execute a workflow"),
            branch_name.as_deref(),
        )?;
        self.overview_bump_warning_dialog = None;
        self.overview_branch_bump_dialog = None;
        self.overview_bump_workflow_dialog = None;
        Ok(())
    }

    pub(crate) fn take_matching_pending_changelog_write(
        &mut self,
        scope_index: usize,
        workflow: OverviewBumpWorkflow,
    ) -> Option<PendingChangelogWrite> {
        let matches = self
            .pending_changelog_write
            .as_ref()
            .is_some_and(|pending| {
                pending.scope_index == scope_index && pending.workflow == workflow
            });
        if matches {
            self.pending_changelog_write.take()
        } else {
            None
        }
    }

    pub(crate) fn resume_pending_bump_action(
        &mut self,
        pending_action: PendingBumpAction,
    ) -> Result<()> {
        match pending_action {
            PendingBumpAction::Standard => self.apply_bump(),
            PendingBumpAction::OverviewWorkflow { .. } => self.confirm_overview_bump_workflow(),
        }
    }

    pub(crate) fn open_tag_dialog_with_scope(
        &mut self,
        preferred_scope: Option<usize>,
        preferred_action: Option<TagAction>,
    ) -> Result<()> {
        let project = self.selected_project()?.clone();
        let dialog = TagDialog::from_project(&project, preferred_scope, preferred_action)?;
        self.bump_dialog = None;
        self.project_edit_dialog = None;
        self.browser_dialog = None;
        self.tag_annotation_dialog = None;
        self.tag_dialog = Some(dialog);
        self.status = StatusMessage::info(
            "Review the proposed tag name, add an optional annotation, then run the tag action.",
        );
        Ok(())
    }

    pub(crate) fn scroll_recent_changes(&mut self, delta: i16) {
        if let Some(dialog) = &mut self.recent_changes_dialog {
            dialog.scroll_by(delta);
        }
    }

    pub(crate) fn move_browser_selection(&mut self, delta: isize) {
        if let Some(dialog) = &mut self.browser_dialog {
            if dialog.explorer.files().is_empty() {
                return;
            }
            let len = dialog.explorer.files().len() as isize;
            let next = (dialog.explorer.selected_idx() as isize + delta).clamp(0, len - 1) as usize;
            dialog.explorer.set_selected_idx(next);
        }
    }

    pub(crate) fn scroll_project_edit_body(&mut self, delta: isize) {
        if let Some(dialog) = &mut self.project_edit_dialog {
            dialog.scroll_body(delta);
        }
    }

    pub(crate) fn scroll_wizard_body(&mut self, delta: isize) {
        self.wizard.scroll_body(delta);
    }

    pub(crate) fn scroll_project_settings(&mut self, delta: isize) {
        project_settings::scroll_project_settings(self, delta);
    }

    pub(crate) fn select_browser_index(&mut self, index: usize) {
        let mut confirm_selection = false;
        if let Some(dialog) = &mut self.browser_dialog {
            let len = dialog.explorer.files().len();
            if len == 0 || index >= len {
                return;
            }
            let already_selected = dialog.explorer.selected_idx() == index;
            dialog.explorer.set_selected_idx(index);
            if already_selected {
                confirm_selection = true;
            }
        }
        if confirm_selection {
            let _ = self.confirm_browser_selection();
        }
    }

    pub(crate) fn open_project_edit_dialog(&mut self) -> Result<()> {
        let project_index = self.selected_project;
        let project = self.selected_project()?;
        let preferred_scope =
            (project.project_type == ProjectType::Branched).then_some(self.overview_focused_scope);
        let dialog = ProjectEditDialog::from_project(project_index, project, preferred_scope)?;
        let status = if project.project_type == ProjectType::Branched {
            "Amend the selected scope settings, then save or update scopes."
        } else {
            "Amend project settings, then save or remove the project."
        };
        self.browser_dialog = None;
        self.project_edit_dialog = Some(dialog);
        self.status = StatusMessage::info(status);
        Ok(())
    }

    pub(crate) fn save_project_edit(&mut self) -> Result<()> {
        let dialog = self
            .project_edit_dialog
            .clone()
            .ok_or_else(|| anyhow!("no project edit is in progress"))?;
        let project = self
            .config
            .projects
            .get(dialog.project_index)
            .ok_or_else(|| anyhow!("selected project no longer exists"))?;
        let mut updated_project = project.clone();
        if let Err(error) = dialog.apply(&mut updated_project) {
            let error_text = error.to_string();
            if dialog.project_type == ProjectType::Branched
                && error_text.contains("target path cannot be empty")
            {
                self.status = StatusMessage::error(error_text).with_new_scope_toast_preview();
                return Ok(());
            }
            return Err(error);
        }
        ensure_project_repo_gitignore_defaults(&updated_project)?;
        self.config.projects[dialog.project_index] = updated_project;
        self.config_store.save(&self.config)?;
        self.invalidate_overview_cache();
        self.prime_selected_project_dashboard_data();
        project_settings::invalidate_project_settings_state(self);
        self.project_edit_dialog = None;
        self.status = StatusMessage::success("Project settings updated.");
        Ok(())
    }

    pub(crate) fn remove_project(&mut self) -> Result<()> {
        let dialog = self
            .project_edit_dialog
            .clone()
            .ok_or_else(|| anyhow!("no project edit is in progress"))?;
        if dialog.project_index >= self.config.projects.len() {
            bail!("selected project no longer exists");
        }

        self.request_project_deletion(dialog.project_index)
    }

    pub(crate) fn open_tag_dialog(&mut self) -> Result<()> {
        let preferred_scope = self
            .recent_changes_dialog
            .as_ref()
            .and_then(|dialog| dialog.can_select_scope().then_some(dialog.selected_scope));
        self.open_tag_dialog_with_scope(preferred_scope, None)
    }

    pub(crate) fn rotate_tag_scope(&mut self, delta: isize) {
        if let Some(dialog) = &mut self.tag_dialog {
            dialog.rotate_scope(delta);
        }
    }

    pub(crate) fn rotate_tag_action(&mut self, delta: isize) {
        if let Some(dialog) = &mut self.tag_dialog {
            dialog.rotate_action(delta);
        }
    }

    pub(crate) fn create_local_tag(&mut self) -> Result<()> {
        let Some(dialog) = self.tag_dialog.clone() else {
            return Ok(());
        };
        let changelog_enabled = self
            .selected_project()?
            .changelog_enabled_for_scope(dialog.selected_scope);

        let tag_name = dialog.tag_name.value.trim();
        if tag_name.is_empty() {
            bail!("tag name cannot be empty");
        }

        let request = PendingTagRequest {
            dialog,
            changelog_enabled,
            std_changelog_policy: StdChangelogExecutionPolicy::Auto,
        };

        if let Some(warning_dialog) = self.build_std_changelog_sub_branch_dialog(&request)? {
            self.std_changelog_sub_branch_dialog = Some(warning_dialog);
            self.status = StatusMessage::warning(
                "Standard changelog is on a non-main sub-branch. Choose whether to generate now or postpone.",
            );
            return Ok(());
        }

        self.schedule_pending_tag_request(request)
    }

    pub(crate) fn build_std_changelog_sub_branch_dialog(
        &self,
        request: &PendingTagRequest,
    ) -> Result<Option<StdChangelogSubBranchDialog>> {
        if !request.changelog_enabled {
            return Ok(None);
        }

        let repo_root = &request.dialog.active_scope().repo_root;
        let branch_name = current_branch_with_cancel(repo_root, None)?;
        let Some(previous_tag) = latest_local_tag_with_cancel(repo_root, None)? else {
            return Ok(None);
        };
        let previous_branches =
            branches_containing_ref_with_cancel(repo_root, &previous_tag, None)?;
        let head_branches = branches_containing_ref_with_cancel(repo_root, "HEAD", None)?;
        let decision = decide_std_changelog_generation(
            &previous_tag,
            &branch_name,
            &previous_branches,
            &head_branches,
            request.dialog.active_scope().main_branch_name.as_deref(),
        );

        match decision {
            StdChangelogDecision::PostponeOnSubBranch(sub_branch) => Ok(Some(
                StdChangelogSubBranchDialog::new(request.clone(), previous_tag, sub_branch),
            )),
            _ => Ok(None),
        }
    }

    pub(crate) fn schedule_pending_tag_request(
        &mut self,
        request: PendingTagRequest,
    ) -> Result<()> {
        let tag_name = request.dialog.tag_name.value.trim().to_string();
        let message = match request.dialog.selected_action() {
            TagAction::CreateLocal => format!(
                "Creating local tag '{}' and generating release notes if needed.",
                tag_name
            ),
            TagAction::CreateAndPush => format!(
                "Creating and pushing tag '{}' for the selected scope.",
                tag_name
            ),
            TagAction::CreatePushAndRelease => format!(
                "Creating, pushing, and publishing tag '{}' with generated release notes.",
                tag_name
            ),
        };
        self.schedule_progress_job(
            " Running Tag Action ",
            message.clone(),
            BackgroundJobRequest::CreateTag {
                dialog: request.dialog,
                changelog_enabled: request.changelog_enabled,
                std_changelog_policy: request.std_changelog_policy,
            },
        )?;
        self.status = StatusMessage::info(message);
        Ok(())
    }

    pub(crate) fn select_std_changelog_sub_branch_warning(&mut self, index: usize) {
        if let Some(dialog) = &mut self.std_changelog_sub_branch_dialog {
            dialog.select(index);
        }
    }

    pub(crate) fn rotate_std_changelog_sub_branch_warning(&mut self, delta: isize) {
        if let Some(dialog) = &mut self.std_changelog_sub_branch_dialog {
            dialog.rotate(delta);
        }
    }

    pub(crate) fn cancel_std_changelog_sub_branch_warning(&mut self) {
        self.std_changelog_sub_branch_dialog = None;
        self.status =
            StatusMessage::info("Standard changelog decision cancelled. Tag dialog is still open.");
    }

    pub(crate) fn confirm_std_changelog_sub_branch_warning(&mut self) -> Result<()> {
        let Some(dialog) = self.std_changelog_sub_branch_dialog.clone() else {
            return Ok(());
        };

        match dialog.selected_choice() {
            StdChangelogSubBranchChoice::GenerateNow => {
                self.std_changelog_sub_branch_dialog = None;
                let mut request = dialog.pending_request;
                request.std_changelog_policy = StdChangelogExecutionPolicy::ForceGenerate;
                self.schedule_pending_tag_request(request)?;
            }
            StdChangelogSubBranchChoice::Postpone => {
                self.std_changelog_sub_branch_dialog = None;
                let mut request = dialog.pending_request;
                request.std_changelog_policy = StdChangelogExecutionPolicy::ForcePostpone;
                self.schedule_pending_tag_request(request)?;
            }
            StdChangelogSubBranchChoice::Cancel => self.cancel_std_changelog_sub_branch_warning(),
        }

        Ok(())
    }

    pub(crate) fn open_tag_annotation_dialog(&mut self) -> Result<()> {
        let current_annotation = self
            .tag_dialog
            .as_ref()
            .map(|dialog| dialog.annotation.clone())
            .ok_or_else(|| anyhow!("no tag dialog is active"))?;

        self.tag_annotation_dialog = Some(TagAnnotationDialog::new(&current_annotation));
        self.status = StatusMessage::info("Editing tag annotation. F2 or Ctrl+S saves it.");
        Ok(())
    }

    pub(crate) fn save_tag_annotation(&mut self) -> Result<()> {
        let dialog = self
            .tag_annotation_dialog
            .take()
            .ok_or_else(|| anyhow!("no tag annotation editor is active"))?;
        let annotation = dialog.editor.lines().join("\n");

        if let Some(tag_dialog) = &mut self.tag_dialog {
            tag_dialog.annotation = annotation;
        }

        self.status = StatusMessage::success("Tag annotation saved.");
        Ok(())
    }

    pub(crate) fn selected_project(&self) -> Result<&ProjectConfig> {
        self.config
            .projects
            .get(self.selected_project)
            .ok_or_else(|| anyhow!("no project is selected"))
    }

    pub(crate) fn open_bump_dialog(&mut self) -> Result<()> {
        let project = self.selected_project()?;
        let dialog = BumpDialog::from_project(project)?;
        self.recent_changes_dialog = None;
        self.cancel_background_job_kind(BackgroundJobKind::RecentChanges);
        self.cancel_background_job_kind(BackgroundJobKind::RecentChangesPrefetch);
        self.current_recent_changes_job_id = None;
        self.tag_dialog = None;
        self.project_edit_dialog = None;
        self.browser_dialog = None;
        self.bump_dialog = Some(dialog);
        self.status = StatusMessage::info(
            "Review the preview, then press Enter to apply the bump for the active target set.",
        );
        Ok(())
    }

    pub(crate) fn rotate_bump_action(&mut self, delta: isize) {
        if let Some(dialog) = &mut self.bump_dialog {
            dialog.rotate_action(delta);
        }
    }

    pub(crate) fn rotate_bump_scope(&mut self, delta: isize) {
        if let Some(dialog) = &mut self.bump_dialog {
            dialog.rotate_scope(delta);
        }
    }

    pub(crate) fn request_apply_bump(&mut self) -> Result<()> {
        if !self.are_we_on_main(PendingBumpAction::Standard)? {
            return Ok(());
        }
        self.apply_bump()
    }

    pub(crate) fn apply_bump(&mut self) -> Result<()> {
        let Some(dialog) = &self.bump_dialog else {
            return Ok(());
        };

        let next_version = dialog.preview_next_version().map_err(anyhow::Error::msg)?;
        let targets = dialog.active_targets();
        for target in &targets {
            write_target_version(target, &next_version)?;
            git_flow::refresh_target_artifacts(target, None)?;
        }

        let target_count = targets.len();
        let scope_notice = if dialog.unified_versioning {
            String::new()
        } else {
            format!(" in scope '{}'", dialog.active_scope().display_name)
        };
        let preferred_scope = if dialog.unified_versioning {
            None
        } else {
            Some(dialog.selected_scope)
        };
        self.bump_dialog = None;
        let repo_backed = self.selected_project()?.integration_mode.requires_repo();
        self.status = StatusMessage::success(format!(
            "Updated {} target{}{} to {}.",
            target_count,
            if target_count == 1 { "" } else { "s" },
            scope_notice,
            next_version
        ));
        if repo_backed {
            self.open_tag_dialog_with_scope(preferred_scope, Some(TagAction::CreateAndPush))?;
            self.status = StatusMessage::info(
                "Version bump applied. Review the suggested tag-and-push action next.",
            );
        } else {
            self.sync_dashboard_overview_after_repo_change();
        }
        Ok(())
    }

    pub(crate) fn open_wizard(&mut self) {
        self.wizard = ProjectWizard::default();
        self.browser_dialog = None;
        self.screen = Screen::Wizard;
        self.status =
            StatusMessage::info("Configure a project and read each target file before saving.");
    }

    pub(crate) fn activate_wizard_focus(&mut self) -> Result<()> {
        match self.wizard.focus {
            WizardField::AddScope => self.apply_wizard_scope_action(ScopeAction::Add),
            WizardField::RemoveScope => self.apply_wizard_scope_action(ScopeAction::Remove),
            WizardField::MoveScopeUp => self.apply_wizard_scope_action(ScopeAction::MoveUp),
            WizardField::MoveScopeDown => self.apply_wizard_scope_action(ScopeAction::MoveDown),
            WizardField::TargetKey => {
                self.wizard.enable_custom_target_key();
                self.status = StatusMessage::info("Custom target key input enabled.");
                Ok(())
            }
            WizardField::Validate => {
                self.validate_wizard_target();
                Ok(())
            }
            WizardField::Save => self.save_wizard_project(),
            WizardField::Cancel => {
                self.screen = Screen::Dashboard;
                self.status = StatusMessage::info("Wizard cancelled.");
                Ok(())
            }
            _ => {
                self.wizard.focus_next();
                Ok(())
            }
        }
    }

    pub(crate) fn apply_wizard_scope_action(&mut self, action: ScopeAction) -> Result<()> {
        match action {
            ScopeAction::Add => {
                self.wizard.add_scope();
                self.status = StatusMessage::info("Added a new branched scope draft.");
            }
            ScopeAction::Remove => {
                self.wizard.remove_selected_scope()?;
                self.status = StatusMessage::info("Removed the selected branched scope.");
            }
            ScopeAction::MoveUp => {
                self.wizard.move_selected_scope(-1);
                self.status = StatusMessage::info("Moved the selected scope earlier.");
            }
            ScopeAction::MoveDown => {
                self.wizard.move_selected_scope(1);
                self.status = StatusMessage::info("Moved the selected scope later.");
            }
        }
        Ok(())
    }

    pub(crate) fn apply_project_edit_scope_action(&mut self, action: ScopeAction) -> Result<()> {
        match action {
            ScopeAction::Add => {
                let Some(dialog) = &mut self.project_edit_dialog else {
                    return Ok(());
                };
                dialog.add_scope();
                self.status = StatusMessage::info("Added a new branched scope draft.");
            }
            ScopeAction::Remove => {
                let Some(dialog) = &self.project_edit_dialog else {
                    return Ok(());
                };
                if dialog.scopes.len() == 1 {
                    return self
                        .request_scope_deletion(dialog.project_index, dialog.selected_scope);
                }
                let Some(dialog) = &mut self.project_edit_dialog else {
                    return Ok(());
                };
                dialog.remove_selected_scope()?;
                self.status = StatusMessage::info("Removed the selected branched scope.");
            }
            ScopeAction::MoveUp => {
                let Some(dialog) = &mut self.project_edit_dialog else {
                    return Ok(());
                };
                dialog.move_selected_scope(-1);
                self.status = StatusMessage::info("Moved the selected scope earlier.");
            }
            ScopeAction::MoveDown => {
                let Some(dialog) = &mut self.project_edit_dialog else {
                    return Ok(());
                };
                dialog.move_selected_scope(1);
                self.status = StatusMessage::info("Moved the selected scope later.");
            }
        }

        Ok(())
    }

    pub(crate) fn move_project_selection(&mut self, delta: isize) {
        if self.config.projects.is_empty() {
            return;
        }
        let len = self.config.projects.len() as isize;
        let next = (self.selected_project as isize + delta).clamp(0, len - 1);
        let next = next as usize;
        if self.selected_project != next {
            self.selected_project = next;
            self.prime_selected_project_dashboard_data();
        }
    }

    pub(crate) fn reorder_projects(&mut self, from_index: usize, to_index: usize) {
        if from_index >= self.config.projects.len() || to_index >= self.config.projects.len() {
            return;
        }
        if from_index == to_index {
            return;
        }
        let project = self.config.projects.remove(from_index);
        self.config.projects.insert(to_index, project);
        self.selected_project = to_index;
        if let Err(error) = self.config_store.save(&self.config) {
            self.status = StatusMessage::error(format!("Failed to save project order: {}", error));
        } else {
            self.status = StatusMessage::info("Project order updated.".to_string());
        }
    }

    pub(crate) fn request_dashboard_delete(&mut self) -> Result<()> {
        let Some(project) = self.config.projects.get(self.selected_project).cloned() else {
            self.status = StatusMessage::info("No project is selected.");
            return Ok(());
        };

        match project.project_type {
            ProjectType::AllInOne => self.request_project_deletion(self.selected_project),
            ProjectType::Branched => {
                if project.branches.is_empty() {
                    return self.request_project_deletion(self.selected_project);
                }

                let scope_index = self
                    .overview_focused_scope
                    .min(project.branches.len().saturating_sub(1));
                self.request_scope_deletion(self.selected_project, scope_index)
            }
        }
    }

    pub(crate) fn request_project_deletion(&mut self, project_index: usize) -> Result<()> {
        let project = self
            .config
            .projects
            .get(project_index)
            .ok_or_else(|| anyhow!("selected project no longer exists"))?;
        self.delete_confirmation_dialog = Some(DeleteConfirmationDialog::project(
            project_index,
            project.name.clone(),
        ));
        self.status =
            StatusMessage::warning(format!("Confirm deletion of project '{}'.", project.name));
        Ok(())
    }

    pub(crate) fn request_scope_deletion(
        &mut self,
        project_index: usize,
        scope_index: usize,
    ) -> Result<()> {
        let project = self
            .config
            .projects
            .get(project_index)
            .ok_or_else(|| anyhow!("selected project no longer exists"))?;
        if project.project_type != ProjectType::Branched {
            return self.request_project_deletion(project_index);
        }

        let branch = project
            .branches
            .get(scope_index)
            .ok_or_else(|| anyhow!("selected scope no longer exists"))?;
        self.delete_confirmation_dialog = Some(DeleteConfirmationDialog::scope(
            project_index,
            project.name.clone(),
            scope_index,
            branch.display_name().to_string(),
            branch.scope_kind,
            project.branches.len() == 1,
        ));
        self.status = StatusMessage::warning(format!(
            "Confirm deletion of scope '{}' from project '{}'.",
            branch.display_name(),
            project.name
        ));
        Ok(())
    }

    pub(crate) fn confirm_delete_request(&mut self) -> Result<()> {
        let Some(dialog) = self.delete_confirmation_dialog.clone() else {
            return Ok(());
        };
        self.delete_confirmation_dialog = None;

        match dialog.target {
            DeleteConfirmationTarget::Project { project_index, .. } => {
                self.delete_project_at(project_index)
            }
            DeleteConfirmationTarget::Scope {
                project_index,
                scope_index,
                removes_project,
                ..
            } => {
                if removes_project {
                    self.delete_last_scope_project(project_index, scope_index)
                } else {
                    self.delete_scope_at(project_index, scope_index)
                }
            }
        }
    }

    pub(crate) fn cancel_delete_request(&mut self) {
        self.delete_confirmation_dialog = None;
        self.status = StatusMessage::info("Deletion cancelled.");
    }

    pub(crate) fn delete_project_at(&mut self, project_index: usize) -> Result<()> {
        if project_index >= self.config.projects.len() {
            bail!("selected project no longer exists");
        }

        let removed = self.config.projects.remove(project_index);
        self.finish_delete_mutation(project_index)?;
        self.status = StatusMessage::success(format!("Removed project '{}'.", removed.name));
        Ok(())
    }

    pub(crate) fn delete_scope_at(
        &mut self,
        project_index: usize,
        scope_index: usize,
    ) -> Result<()> {
        let (project_name, scope_name, remaining_scopes) = {
            let project = self
                .config
                .projects
                .get_mut(project_index)
                .ok_or_else(|| anyhow!("selected project no longer exists"))?;
            if project.project_type != ProjectType::Branched {
                bail!("selected project does not contain removable scopes");
            }
            if scope_index >= project.branches.len() {
                bail!("selected scope no longer exists");
            }
            let removed = project.branches.remove(scope_index);
            (
                project.name.clone(),
                removed.display_name().to_string(),
                project.branches.len(),
            )
        };

        self.finish_delete_mutation(project_index)?;
        if remaining_scopes > 0 {
            self.overview_focused_scope = scope_index.min(remaining_scopes.saturating_sub(1));
        }
        self.status = StatusMessage::success(format!(
            "Removed scope '{}' from project '{}'.",
            scope_name, project_name
        ));
        Ok(())
    }

    pub(crate) fn delete_last_scope_project(
        &mut self,
        project_index: usize,
        scope_index: usize,
    ) -> Result<()> {
        let (project_name, scope_name) = {
            let project = self
                .config
                .projects
                .get(project_index)
                .ok_or_else(|| anyhow!("selected project no longer exists"))?;
            if project.project_type != ProjectType::Branched {
                bail!("selected project does not contain removable scopes");
            }
            let branch = project
                .branches
                .get(scope_index)
                .ok_or_else(|| anyhow!("selected scope no longer exists"))?;
            (project.name.clone(), branch.display_name().to_string())
        };

        self.config.projects.remove(project_index);
        self.finish_delete_mutation(project_index)?;
        self.status = StatusMessage::success(format!(
            "Removed scope '{}' and deleted project '{}' because it had no scopes left.",
            scope_name, project_name
        ));
        Ok(())
    }

    pub(crate) fn finish_delete_mutation(&mut self, selected_index_hint: usize) -> Result<()> {
        self.config_store.save(&self.config)?;
        self.project_edit_dialog = None;
        self.browser_dialog = None;
        if self.config.projects.is_empty() {
            self.selected_project = 0;
            self.overview_focused_scope = 0;
        } else {
            self.selected_project =
                selected_index_hint.min(self.config.projects.len().saturating_sub(1));
        }
        self.invalidate_overview_cache();
        self.prime_selected_project_dashboard_data();
        project_settings::invalidate_project_settings_state(self);
        Ok(())
    }

    pub(crate) fn validate_wizard_target(&mut self) {
        let (target_path, target_key) = if self.wizard.project_type == ProjectType::Branched {
            self.wizard
                .current_scope()
                .map(|scope| {
                    (
                        scope.target_path.value().trim().to_string(),
                        scope.target_key.value().trim().to_string(),
                    )
                })
                .unwrap_or_default()
        } else {
            (
                self.wizard.target_path.value.trim().to_string(),
                self.wizard.target_key.value.trim().to_string(),
            )
        };

        match probe_target(&target_path, &target_key, self.wizard.version_scheme) {
            Ok(probe) => {
                self.status = match probe.kind {
                    ProbeKind::Success => StatusMessage::success(
                        "Target file is readable and the selected key matches the chosen scheme.",
                    ),
                    ProbeKind::Warning => StatusMessage::warning(
                        "Target file is readable, but the detected version does not match the chosen scheme.",
                    ),
                    ProbeKind::Error => StatusMessage::error("Target validation failed."),
                };
                if self.wizard.project_type == ProjectType::Branched {
                    if let Some(scope) = self.wizard.current_scope_mut() {
                        scope.last_probe = Some(probe);
                    }
                } else {
                    self.wizard.last_probe = Some(probe);
                }
            }
            Err(error) => {
                self.status = StatusMessage::error(error.to_string());
                let probe = TargetProbe {
                    kind: ProbeKind::Error,
                    message: error.to_string(),
                    version: None,
                    format: None,
                };
                if self.wizard.project_type == ProjectType::Branched {
                    if let Some(scope) = self.wizard.current_scope_mut() {
                        scope.last_probe = Some(probe);
                    }
                } else {
                    self.wizard.last_probe = Some(probe);
                }
            }
        }
    }

    pub(crate) fn save_wizard_project(&mut self) -> Result<()> {
        let project = self.wizard.build_project()?;
        ensure_project_repo_gitignore_defaults(&project)?;
        self.config.projects.push(project);
        self.config_store.save(&self.config)?;
        self.selected_project = self.config.projects.len().saturating_sub(1);
        self.invalidate_overview_cache();
        self.prime_selected_project_dashboard_data();
        self.screen = Screen::Dashboard;
        self.status = StatusMessage::success("Project saved to the user config directory.");
        Ok(())
    }

    pub(crate) fn open_browser_for_wizard_focus(&mut self) -> Result<()> {
        let target = match self.wizard.focus {
            WizardField::TargetPath => BrowseTarget::WizardTargetPath,
            WizardField::RepoRoot => BrowseTarget::WizardRepoRoot,
            _ => return Ok(()),
        };
        self.open_browser(target)
    }

    pub(crate) fn open_browser_for_project_edit_focus(&mut self) -> Result<()> {
        let Some(dialog) = &self.project_edit_dialog else {
            return Ok(());
        };
        let target = match dialog.focus {
            ProjectEditFocus::TargetPath => BrowseTarget::ProjectEditTargetPath,
            ProjectEditFocus::RepoRoot => BrowseTarget::ProjectEditRepoRoot,
            _ => return Ok(()),
        };
        self.open_browser(target)
    }

    pub(crate) fn open_browser(&mut self, target: BrowseTarget) -> Result<()> {
        let dialog = FileBrowserDialog::new(target, self.initial_browser_path(target))?;
        self.browser_dialog = Some(dialog);
        self.status =
            StatusMessage::info("Browse to a file or directory, then press Enter to select it.");
        Ok(())
    }

    pub(crate) fn initial_browser_path(&self, target: BrowseTarget) -> String {
        match target {
            BrowseTarget::WizardTargetPath => self.wizard.target_path.value().to_string(),
            BrowseTarget::WizardRepoRoot => self.wizard.repo_root.value().to_string(),
            BrowseTarget::ProjectEditTargetPath => self
                .project_edit_dialog
                .as_ref()
                .map(|dialog| dialog.target_path.value().to_string())
                .unwrap_or_default(),
            BrowseTarget::ProjectEditRepoRoot => self
                .project_edit_dialog
                .as_ref()
                .map(|dialog| dialog.repo_root.value().to_string())
                .unwrap_or_default(),
            BrowseTarget::ProjectSettingsChangelogPath
            | BrowseTarget::ProjectSettingsReleaseNowGeneral
            | BrowseTarget::ProjectSettingsReleaseNowWindows
            | BrowseTarget::ProjectSettingsReleaseNowLinuxArm
            | BrowseTarget::ProjectSettingsReleaseNowLinuxAmd
            | BrowseTarget::ProjectSettingsReleaseNowMacOs
            | BrowseTarget::ProjectSettingsAliasDistPath
            | BrowseTarget::ProjectSettingsAliasUiPath
            | BrowseTarget::ProjectSettingsAliasCustomPath(_) => {
                project_settings::initial_browser_path(self, target).unwrap_or_default()
            }
        }
    }

    pub(crate) fn confirm_browser_selection(&mut self) -> Result<()> {
        let Some(dialog) = &self.browser_dialog else {
            return Ok(());
        };

        let selected = dialog.explorer.current().path.clone();
        let selected_name = dialog.explorer.current().name.clone();
        let target = dialog.target;
        let select_directories = dialog.select_directories;

        if selected.is_dir() {
            if let Some(dialog) = &mut self.browser_dialog {
                dialog.explorer.handle(ExplorerInput::Right)?;
            }
            self.status = StatusMessage::info(if selected_name == "../" {
                "Moved to the parent folder.".to_string()
            } else {
                format!("Entered folder '{}'.", selected_name)
            });
            return Ok(());
        }

        if select_directories && !selected.is_dir() {
            self.status = StatusMessage::warning(
                "Select a directory for Repo root, or press U to use the current file's folder.",
            );
            return Ok(());
        }

        if !select_directories && !selected.is_file() {
            self.status = StatusMessage::warning(
                "Select a file for Target path. Use Right to enter directories.",
            );
            return Ok(());
        }

        let selected = selected.display().to_string();
        match target {
            BrowseTarget::WizardTargetPath => self.wizard.set_target_path_from_browse(selected),
            BrowseTarget::WizardRepoRoot => self.wizard.set_repo_root_from_browse(selected),
            BrowseTarget::ProjectEditTargetPath => {
                if let Some(dialog) = &mut self.project_edit_dialog {
                    dialog.set_target_path_from_browse(selected);
                }
            }
            BrowseTarget::ProjectEditRepoRoot => {
                if let Some(dialog) = &mut self.project_edit_dialog {
                    dialog.set_repo_root_from_browse(selected);
                }
            }
            BrowseTarget::ProjectSettingsChangelogPath
            | BrowseTarget::ProjectSettingsReleaseNowGeneral
            | BrowseTarget::ProjectSettingsReleaseNowWindows
            | BrowseTarget::ProjectSettingsReleaseNowLinuxArm
            | BrowseTarget::ProjectSettingsReleaseNowLinuxAmd
            | BrowseTarget::ProjectSettingsReleaseNowMacOs
            | BrowseTarget::ProjectSettingsAliasDistPath
            | BrowseTarget::ProjectSettingsAliasUiPath
            | BrowseTarget::ProjectSettingsAliasCustomPath(_) => {
                if project_settings::apply_browser_selection(self, target, selected)? {
                    self.browser_dialog = None;
                    self.status = StatusMessage::success("Selection applied.");
                    return Ok(());
                }
            }
        }

        self.browser_dialog = None;
        self.status = StatusMessage::success("Selection applied.");
        Ok(())
    }

    pub(crate) fn confirm_browser_directory_selection(&mut self) -> Result<()> {
        let Some(dialog) = &self.browser_dialog else {
            return Ok(());
        };
        if !dialog.select_directories {
            return Ok(());
        }

        let selected = dialog.explorer.current().path.clone();
        let directory = if selected.is_dir() {
            selected
        } else if let Some(parent) = selected.parent() {
            parent.to_path_buf()
        } else {
            selected
        };

        let selected = directory.display().to_string();
        let target = dialog.target;
        match target {
            BrowseTarget::WizardRepoRoot => self.wizard.set_repo_root_from_browse(selected),
            BrowseTarget::ProjectEditRepoRoot => {
                if let Some(dialog) = &mut self.project_edit_dialog {
                    dialog.set_repo_root_from_browse(selected);
                }
            }
            BrowseTarget::ProjectSettingsAliasDistPath
            | BrowseTarget::ProjectSettingsAliasUiPath
            | BrowseTarget::ProjectSettingsAliasCustomPath(_)
                if project_settings::apply_browser_selection(self, target, selected.clone())? =>
            {
                self.browser_dialog = None;
                self.status = StatusMessage::success("Folder selection applied.");
                return Ok(());
            }
            _ => {}
        }

        self.browser_dialog = None;
        self.status = StatusMessage::success("Folder selection applied.");
        Ok(())
    }

    pub(crate) fn toggle_tab_hints(&mut self) -> Result<()> {
        self.config.ui.show_tab_hints = !self.config.ui.show_tab_hints;
        self.config_store.save(&self.config)?;
        self.status = StatusMessage::success(if self.config.ui.show_tab_hints {
            "Tab hints enabled."
        } else {
            "Tab hints hidden."
        });
        Ok(())
    }

    pub(crate) fn update_footer_visibility(&mut self, viewport_height: u16) {
        if viewport_height <= 25 {
            if !self.config.ui.hide_footer && !self.footer_manual_override {
                self.config.ui.hide_footer = true;
                self.footer_auto_hidden = true;
            }
        } else if self.footer_auto_hidden {
            self.config.ui.hide_footer = false;
            self.footer_auto_hidden = false;
        }
    }

    pub(crate) fn toggle_footer(&mut self) -> Result<()> {
        if self.footer_auto_hidden {
            self.footer_auto_hidden = false;
        }
        self.footer_manual_override = true;
        self.config.ui.hide_footer = !self.config.ui.hide_footer;
        self.config_store.save(&self.config)?;
        self.status = StatusMessage::success(if self.config.ui.hide_footer {
            "Footer hidden. Press H to show it again."
        } else {
            "Footer shown."
        });
        Ok(())
    }

    pub(crate) fn cycle_footer_content(&mut self, delta: i32) -> Result<()> {
        self.config.ui.footer_content = if delta >= 0 {
            self.config.ui.footer_content.next()
        } else {
            self.config.ui.footer_content.previous()
        };
        self.config_store.save(&self.config)?;
        self.status = StatusMessage::success(format!(
            "Footer content alignment set to {}.",
            self.config.ui.footer_content.display_name()
        ));
        Ok(())
    }

    pub(crate) fn toggle_dashboard_focus(&mut self) {
        self.dashboard_focus = match self.dashboard_focus {
            DashboardPane::Projects => DashboardPane::Overview,
            DashboardPane::Overview => DashboardPane::Projects,
        };
    }

    pub(crate) fn scroll_dashboard_recent_changes(&mut self, delta: i16) -> bool {
        if let Some(dialog) = &mut self.overview_recent_changes {
            dialog.scroll_by(delta);
            true
        } else {
            false
        }
    }

    pub(crate) fn open_commit_rename_from_view(&mut self, view: RecentChangeView) -> Result<()> {
        let (repo_root, commit_hash) = match view {
            RecentChangeView::Popup => {
                let dialog = self
                    .recent_changes_dialog
                    .as_ref()
                    .ok_or_else(|| anyhow!("the git log popup is not open"))?;
                (
                    dialog.active_scope().repo_root.clone(),
                    dialog.selected_commit_hash().ok_or_else(|| {
                        anyhow!("select a commit line before renaming its message")
                    })?,
                )
            }
            RecentChangeView::Overview => {
                let dialog = self.overview_recent_changes.as_ref().ok_or_else(|| {
                    anyhow!("recent changes are not available for the selected project")
                })?;
                (
                    dialog.active_scope().repo_root.clone(),
                    dialog.selected_commit_hash().ok_or_else(|| {
                        anyhow!("select a commit line before renaming its message")
                    })?,
                )
            }
        };

        let plan = prepare_commit_rename(&repo_root, &commit_hash)?;
        self.commit_rename_dialog = Some(CommitRenameDialog::new(view, plan));
        self.status = StatusMessage::info("Edit the commit message and press Enter to save it.");
        Ok(())
    }

    pub(crate) fn toggle_commit_rename_force_push(&mut self) {
        if let Some(dialog) = &mut self.commit_rename_dialog
            && dialog.plan.touches_pushed_history
        {
            dialog.push_after_rename = !dialog.push_after_rename;
        }
    }

    pub(crate) fn apply_commit_rename(&mut self) -> Result<()> {
        let Some(dialog) = self.commit_rename_dialog.take() else {
            return Ok(());
        };

        let new_message = dialog.message_editor.lines().join("\n");
        let outcome = match rename_commit_with_subject(&dialog.plan, &new_message) {
            Ok(outcome) => outcome,
            Err(error) => {
                self.commit_rename_dialog = Some(dialog);
                return Err(error);
            }
        };

        if dialog.push_after_rename
            && dialog.plan.touches_pushed_history
            && let Err(error) = push_branch_force_with_lease(&dialog.plan.repo_root)
        {
            self.commit_rename_dialog = Some(dialog);
            return Err(error);
        }

        self.sync_dashboard_overview_after_repo_change();
        if let Some(recent_dialog) = &mut self.recent_changes_dialog {
            let _ = recent_dialog.refresh_current_scope_cancellable(None);
        }

        let mut summary = format!(
            "Renamed {} to '{}'.",
            outcome.target_commit, outcome.new_subject
        );
        if dialog.push_after_rename && dialog.plan.touches_pushed_history {
            summary.push_str(" Force-pushed with --force-with-lease.");
        }
        self.status = StatusMessage::success(summary);
        Ok(())
    }

    pub(crate) fn select_recent_change_line(&mut self, view: RecentChangeView, line_index: usize) {
        match view {
            RecentChangeView::Popup => {
                if let Some(dialog) = &mut self.recent_changes_dialog {
                    dialog.select_line(line_index);
                }
            }
            RecentChangeView::Overview => {
                if let Some(dialog) = &mut self.overview_recent_changes {
                    dialog.select_line(line_index);
                }
            }
        }
    }

    pub(crate) fn handle_toast_interaction(&mut self, interaction: ToastInteraction) -> bool {
        match interaction {
            ToastInteraction::None => false,
            ToastInteraction::Dismissed => true,
            ToastInteraction::CopyRequested(text) => {
                self.copy_text_to_clipboard(&text);
                true
            }
        }
    }

    pub(crate) fn copy_text_to_clipboard(&mut self, text: &str) {
        self.fallback_clipboard = Some(text.to_string());

        if self.clipboard.is_none() {
            self.clipboard = Clipboard::new().ok();
        }

        let mut copied = false;
        if let Some(ref mut clipboard) = self.clipboard {
            #[cfg(target_os = "linux")]
            {
                let text_owned = text.to_string();
                let clipboard_ok = clipboard
                    .set()
                    .clipboard(LinuxClipboardKind::Clipboard)
                    .text(text_owned.clone())
                    .is_ok();
                let primary_ok = clipboard
                    .set()
                    .clipboard(LinuxClipboardKind::Primary)
                    .text(text_owned)
                    .is_ok();
                copied = clipboard_ok || primary_ok;
            }
            #[cfg(not(target_os = "linux"))]
            {
                copied = clipboard.set_text(text.to_string()).is_ok();
            }
        }

        #[cfg(target_os = "linux")]
        if !copied {
            copied = copy_text_via_linux_clipboard_cli(text);
        }

        if copied {
            self.status = StatusMessage::info("Copied to clipboard.");
        } else {
            self.status =
                StatusMessage::info("Copied in-app only (could not reach the desktop clipboard).");
        }
    }

    pub(crate) fn sync_status_toasts(&mut self) {
        if self.status.id == self.last_status_toast_id {
            return;
        }

        self.last_status_toast_id = self.status.id;
        let mut builder = ToastBuilder::new(self.status.text.clone().into());
        if let (Some(preset), Some(title)) =
            (self.status.toast_preset, self.status.toast_title.clone())
        {
            builder = builder.preset(preset, title);
        } else if let Some(title) = self.status.toast_title.clone() {
            builder = builder.title(title);
        }
        match self.status.kind {
            StatusKind::Info => self.toaster.show_toast(builder.toast_type(ToastType::Info)),
            StatusKind::Success => self
                .toaster
                .show_toast(builder.toast_type(ToastType::Success)),
            StatusKind::Warning => self
                .toaster
                .show_toast(builder.toast_type(ToastType::Warning)),
            StatusKind::Error => self
                .toaster
                .show_toast(builder.toast_type(ToastType::Error).keep_on(1)),
        }
    }

    pub(crate) fn show_transient_toast(&mut self, kind: StatusKind, text: impl Into<String>) {
        let builder = ToastBuilder::new(text.into().into());
        match kind {
            StatusKind::Info => self.toaster.show_toast(builder.toast_type(ToastType::Info)),
            StatusKind::Success => self
                .toaster
                .show_toast(builder.toast_type(ToastType::Success)),
            StatusKind::Warning => self
                .toaster
                .show_toast(builder.toast_type(ToastType::Warning)),
            StatusKind::Error => self
                .toaster
                .show_toast(builder.toast_type(ToastType::Error).keep_on(1)),
        }
    }

    pub(crate) fn show_sticky_error_toast(&mut self, text: impl Into<String>) {
        self.toaster.show_toast(
            ToastBuilder::new(text.into().into())
                .toast_type(ToastType::Error)
                .keep_on(1),
        );
    }
}
