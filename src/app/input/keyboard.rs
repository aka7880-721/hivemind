// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit SA-PS License

use super::super::*;

use anyhow::Result;
use arboard::Clipboard;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui_comfy_toaster::ToastShortcut;

use crate::{
    config::ProjectType,
    tui::ProjectEditFocus,
    tui::{OverviewTab, overview_tabs},
    workflow::dialogs::RecentChangesTab,
};

use super::super::{project_settings, rls_now};

impl App {
    pub(crate) fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('v') {
            self.paste_from_clipboard();
            return Ok(());
        }

        if self.try_handle_toast_shortcut(key) {
            return Ok(());
        }

        if self.help_modal.is_some() && self.handle_help_key(key) {
            return Ok(());
        }

        if self.progress_dialog.is_some() {
            return Ok(());
        }

        if self.snif_dialog.is_some() {
            return self.handle_snif_key(key);
        }

        if self.browser_dialog.is_some() {
            return self.handle_browser_key(key);
        }

        if self.release_now_notes_dialog.is_some() {
            return self.handle_release_now_notes_key(key);
        }

        if self.top_picks_editor_dialog.is_some() {
            return self.handle_top_picks_editor_key(key);
        }

        if self.release_now_dialog.is_some() {
            return self.handle_release_now_key(key);
        }

        if self.delete_confirmation_dialog.is_some() {
            return self.handle_delete_confirmation_key(key);
        }

        if self.commit_rename_dialog.is_some() {
            return self.handle_commit_rename_key(key);
        }

        if self.project_edit_dialog.is_some() {
            return self.handle_project_edit_key(key);
        }

        if self.tag_annotation_dialog.is_some() {
            return self.handle_tag_annotation_key(key);
        }

        if self.main_branch_warning_dialog.is_some() {
            return self.handle_main_branch_warning_key(key);
        }

        if self.std_changelog_sub_branch_dialog.is_some() {
            return self.handle_std_changelog_sub_branch_key(key);
        }

        if self.tag_dialog.is_some() {
            return self.handle_tag_key(key);
        }

        if self.changelog_preview_dialog.is_some() {
            return self.handle_changelog_preview_key(key);
        }

        if self.recent_changes_dialog.is_some() {
            return self.handle_recent_changes_key(key);
        }

        if self.overview_bump_warning_dialog.is_some() {
            return self.handle_overview_bump_warning_key(key);
        }

        if self.overview_bump_kind_dialog.is_some() {
            return self.handle_overview_bump_kind_key(key);
        }

        if self.overview_branch_bump_dialog.is_some() {
            return self.handle_overview_branch_bump_key(key);
        }

        if self.overview_bump_workflow_dialog.is_some() {
            return self.handle_overview_bump_workflow_key(key);
        }

        if self.bump_dialog.is_some() {
            return self.handle_bump_key(key);
        }

        if self.screen == Screen::Dashboard
            && self.overview_tab == OverviewTab::ProjectSettings
            && project_settings::captures_text_input(self)
        {
            return self.handle_dashboard_key(key);
        }

        if self.handle_tab_shortcut(key) {
            return Ok(());
        }

        if self.try_handle_help_shortcut(key)? {
            return Ok(());
        }

        if self.try_handle_ui_shortcut(key)? {
            return Ok(());
        }

        if key.code == KeyCode::Char('q')
            && key.modifiers.is_empty()
            && !(matches!(self.screen, Screen::Wizard) && self.wizard.focus_accepts_text())
            && !self
                .project_edit_dialog
                .as_ref()
                .map(|dialog| dialog.focus_accepts_text())
                .unwrap_or(false)
            && !project_settings::captures_text_input(self)
        {
            self.should_quit = true;
            return Ok(());
        }

        match self.screen {
            Screen::Dashboard => self.handle_dashboard_key(key),
            Screen::UiSettings => self.handle_ui_settings_key(key),
            Screen::Wizard => self.handle_wizard_key(key),
        }
    }

    pub(crate) fn handle_dashboard_key(&mut self, key: KeyEvent) -> Result<()> {
        if project_settings::try_handle_project_settings_key(self, key)? {
            return Ok(());
        }

        match key.code {
            KeyCode::Char('r') | KeyCode::Char('R')
                if key.modifiers.contains(KeyModifiers::CONTROL)
                    && self.dashboard_focus == DashboardPane::Overview
                    && self.overview_recent_changes.is_some() =>
            {
                return self.open_commit_rename_from_view(RecentChangeView::Overview);
            }
            KeyCode::Char('n') => self.open_wizard(),
            KeyCode::Char('e') => self.open_project_edit_dialog()?,
            KeyCode::Char('d') | KeyCode::Char('D') => self.request_dashboard_delete()?,
            KeyCode::Char('l') | KeyCode::Char('L') => self.open_release_now_with_scope(None)?,
            KeyCode::Char('p') | KeyCode::Char('P') => self.open_top_picks_editor()?,
            KeyCode::Char('b') => self.open_bump_dialog()?,
            KeyCode::Char('g') => self.open_recent_changes()?,
            KeyCode::Char('c') | KeyCode::Char('C') => {
                self.open_dashboard_changelog_preview(None)?
            }
            KeyCode::Char('t') => self.open_tag_dialog()?,
            KeyCode::Char('f') | KeyCode::Char('F') => self.open_snif_dialog()?,
            KeyCode::Char('r') | KeyCode::Char('R') => self.reload_dashboard_overview_data()?,
            KeyCode::Tab | KeyCode::BackTab => self.toggle_dashboard_focus(),
            KeyCode::Up => {
                if self.dashboard_focus == DashboardPane::Overview {
                    if !self.scroll_dashboard_recent_changes(-1) {
                        let _ = self.scroll_dashboard_tiles(-1);
                    }
                } else {
                    self.move_project_selection(-1);
                }
            }
            KeyCode::Down => {
                if self.dashboard_focus == DashboardPane::Overview {
                    if !self.scroll_dashboard_recent_changes(1) {
                        let _ = self.scroll_dashboard_tiles(1);
                    }
                } else {
                    self.move_project_selection(1);
                }
            }
            KeyCode::Left if self.dashboard_focus == DashboardPane::Overview => {
                self.move_dashboard_overview_focus(-1)?;
            }
            KeyCode::Right if self.dashboard_focus == DashboardPane::Overview => {
                self.move_dashboard_overview_focus(1)?;
            }
            KeyCode::PageUp
                if self.dashboard_focus == DashboardPane::Overview
                    && !self.scroll_dashboard_recent_changes(-6) =>
            {
                let _ = self.scroll_dashboard_tiles(-1);
            }
            KeyCode::PageDown
                if self.dashboard_focus == DashboardPane::Overview
                    && !self.scroll_dashboard_recent_changes(6) =>
            {
                let _ = self.scroll_dashboard_tiles(1);
            }
            _ => {}
        }
        Ok(())
    }

    pub(crate) fn handle_delete_confirmation_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Left | KeyCode::Right | KeyCode::Tab | KeyCode::BackTab => {
                if let Some(dialog) = &mut self.delete_confirmation_dialog {
                    dialog.toggle_selection();
                }
            }
            KeyCode::Enter => {
                if self
                    .delete_confirmation_dialog
                    .as_ref()
                    .map(|dialog| dialog.confirm_selected)
                    .unwrap_or(false)
                {
                    return self.confirm_delete_request();
                }
                self.cancel_delete_request();
            }
            KeyCode::Char('y') | KeyCode::Char('Y') => return self.confirm_delete_request(),
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => self.cancel_delete_request(),
            _ => {}
        }
        Ok(())
    }

    pub(crate) fn handle_release_now_key(&mut self, key: KeyEvent) -> Result<()> {
        let warning_mode = self
            .release_now_dialog
            .as_ref()
            .map(rls_now::ReleaseNowDialog::is_warning_mode)
            .unwrap_or(false);
        let mirror_sync_mode = self
            .release_now_dialog
            .as_ref()
            .map(rls_now::ReleaseNowDialog::is_mirror_sync_mode)
            .unwrap_or(false);
        let existing_artifacts_mode = self
            .release_now_dialog
            .as_ref()
            .map(rls_now::ReleaseNowDialog::is_existing_artifacts_mode)
            .unwrap_or(false);
        let customize_mode = self
            .release_now_dialog
            .as_ref()
            .map(rls_now::ReleaseNowDialog::is_artifacts_customize_mode)
            .unwrap_or(false);
        let running = self
            .release_now_dialog
            .as_ref()
            .map(rls_now::ReleaseNowDialog::is_running)
            .unwrap_or(false);
        let completed = self
            .release_now_dialog
            .as_ref()
            .map(rls_now::ReleaseNowDialog::is_completed)
            .unwrap_or(false);

        if mirror_sync_mode {
            let sync_running = self
                .release_now_dialog
                .as_ref()
                .map(|dialog| dialog.mirror_sync_running)
                .unwrap_or(false);
            match key.code {
                KeyCode::Enter if !sync_running => {
                    return self.request_release_now_mirror_sync(true);
                }
                KeyCode::Char('r') | KeyCode::Char('R') if !sync_running => {
                    return self.request_release_now_mirror_sync(false);
                }
                KeyCode::Esc if !sync_running => self.close_release_now_dialog(),
                KeyCode::Up => self.scroll_release_now(-1),
                KeyCode::Down => self.scroll_release_now(1),
                KeyCode::PageUp => self.scroll_release_now(-6),
                KeyCode::PageDown => self.scroll_release_now(6),
                KeyCode::Home => self.scroll_release_now_to_start(),
                KeyCode::End => self.scroll_release_now_to_end(),
                _ => {}
            }
            return Ok(());
        }

        if warning_mode {
            match key.code {
                KeyCode::Left | KeyCode::Right | KeyCode::Tab | KeyCode::BackTab => {
                    if let Some(dialog) = &mut self.release_now_dialog {
                        dialog.toggle_warning_selection();
                    }
                }
                KeyCode::Enter => {
                    let proceed = self
                        .release_now_dialog
                        .as_ref()
                        .map(|dialog| dialog.warning_confirm_selected)
                        .unwrap_or(false);
                    if proceed {
                        if let Some(dialog) = &mut self.release_now_dialog {
                            dialog.proceed_past_warning();
                        }
                    } else {
                        self.close_release_now_dialog();
                    }
                }
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    if let Some(dialog) = &mut self.release_now_dialog {
                        dialog.proceed_past_warning();
                    }
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                    self.close_release_now_dialog()
                }
                _ => {}
            }
            return Ok(());
        }

        if customize_mode {
            match key.code {
                KeyCode::Up => {
                    if let Some(dialog) = &mut self.release_now_dialog {
                        dialog.cycle_customize_platform(-1);
                    }
                }
                KeyCode::Down => {
                    if let Some(dialog) = &mut self.release_now_dialog {
                        dialog.cycle_customize_platform(1);
                    }
                }
                KeyCode::Char(' ') => {
                    if let Some(dialog) = &mut self.release_now_dialog {
                        dialog.toggle_customize_platform_reuse();
                    }
                }
                KeyCode::Enter => {
                    if let Some(dialog) = &mut self.release_now_dialog {
                        dialog.confirm_artifacts_customize();
                    }
                }
                KeyCode::Esc => {
                    if let Some(dialog) = &mut self.release_now_dialog {
                        dialog.back_from_artifacts_customize();
                    }
                }
                KeyCode::PageUp => self.scroll_release_now(-6),
                KeyCode::PageDown => self.scroll_release_now(6),
                KeyCode::Home => self.scroll_release_now_to_start(),
                KeyCode::End => self.scroll_release_now_to_end(),
                _ => {}
            }
            return Ok(());
        }

        if existing_artifacts_mode {
            match key.code {
                KeyCode::Left | KeyCode::Right | KeyCode::Tab | KeyCode::BackTab => {
                    if let Some(dialog) = &mut self.release_now_dialog {
                        let delta = if matches!(key.code, KeyCode::Left | KeyCode::BackTab) {
                            -1
                        } else {
                            1
                        };
                        dialog.cycle_artifacts_choice(delta);
                    }
                }
                KeyCode::Enter => {
                    let choice = self
                        .release_now_dialog
                        .as_ref()
                        .map(|dialog| dialog.artifacts_choice_selected)
                        .unwrap_or(0);
                    if choice == 3 {
                        self.close_release_now_dialog();
                    } else if let Some(dialog) = &mut self.release_now_dialog {
                        dialog.confirm_existing_artifacts_choice();
                    }
                }
                KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                    self.close_release_now_dialog()
                }
                KeyCode::Up => self.scroll_release_now(-1),
                KeyCode::Down => self.scroll_release_now(1),
                KeyCode::PageUp => self.scroll_release_now(-6),
                KeyCode::PageDown => self.scroll_release_now(6),
                KeyCode::Home => self.scroll_release_now_to_start(),
                KeyCode::End => self.scroll_release_now_to_end(),
                _ => {}
            }
            return Ok(());
        }

        if running {
            match key.code {
                KeyCode::Char('f') | KeyCode::Char('F') => self.toggle_release_now_auto_follow(),
                KeyCode::Char('x') | KeyCode::Char('X') => self.request_cancel_release_now(),
                KeyCode::Up => self.scroll_release_now(-1),
                KeyCode::Down => self.scroll_release_now(1),
                KeyCode::PageUp => self.scroll_release_now(-6),
                KeyCode::PageDown => self.scroll_release_now(6),
                KeyCode::Home => self.scroll_release_now_to_start(),
                KeyCode::End => self.scroll_release_now_to_end(),
                KeyCode::Esc => {
                    self.status = StatusMessage::warning(
                        "ReleaseNOW is still running. Wait for it to finish before closing the dialog.",
                    );
                }
                _ => {}
            }
            return Ok(());
        }

        if completed {
            match key.code {
                KeyCode::Esc | KeyCode::Enter => self.close_release_now_dialog(),
                KeyCode::Up => self.scroll_release_now(-1),
                KeyCode::Down => self.scroll_release_now(1),
                KeyCode::PageUp => self.scroll_release_now(-6),
                KeyCode::PageDown => self.scroll_release_now(6),
                KeyCode::Home => self.scroll_release_now_to_start(),
                KeyCode::End => self.scroll_release_now_to_end(),
                _ => {}
            }
            return Ok(());
        }

        match key.code {
            KeyCode::Esc => self.close_release_now_dialog(),
            KeyCode::Left => {
                if let Some(dialog) = &mut self.release_now_dialog {
                    dialog.cycle_option(-1);
                }
            }
            KeyCode::Right => {
                if let Some(dialog) = &mut self.release_now_dialog {
                    dialog.cycle_option(1);
                }
            }
            KeyCode::Char('c') | KeyCode::Char('C') => {
                if let Some(dialog) = &mut self.release_now_dialog {
                    dialog.toggle_attach_changelog();
                }
            }
            KeyCode::Char('e') | KeyCode::Char('E') => self.open_release_now_notes_dialog()?,
            KeyCode::Char('p') | KeyCode::Char('P') => self.open_top_picks_editor()?,
            KeyCode::Enter | KeyCode::F(2) => return self.request_run_release_now(),
            KeyCode::Up => self.scroll_release_now(-1),
            KeyCode::Down => self.scroll_release_now(1),
            KeyCode::PageUp => self.scroll_release_now(-6),
            KeyCode::PageDown => self.scroll_release_now(6),
            KeyCode::Home => self.scroll_release_now_to_start(),
            KeyCode::End => self.scroll_release_now_to_end(),
            _ => {}
        }
        Ok(())
    }

    pub(crate) fn handle_release_now_notes_key(&mut self, key: KeyEvent) -> Result<()> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s') {
            return self.save_release_now_notes();
        }

        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('C'))
            && let Some(dialog) = &mut self.release_now_notes_dialog
        {
            dialog.editor.copy();
            let text = dialog.editor.yank_text();
            if !text.is_empty() {
                self.copy_text_to_clipboard(&text);
                return Ok(());
            }
        }

        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('x') | KeyCode::Char('X'))
            && let Some(dialog) = &mut self.release_now_notes_dialog
        {
            dialog.editor.cut();
            let text = dialog.editor.yank_text();
            if !text.is_empty() {
                self.copy_text_to_clipboard(&text);
                return Ok(());
            }
        }

        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('a') | KeyCode::Char('A'))
            && let Some(dialog) = &mut self.release_now_notes_dialog
        {
            dialog.editor.select_all();
            return Ok(());
        }

        match key.code {
            KeyCode::Esc => {
                self.release_now_notes_dialog = None;
                self.status = StatusMessage::info("Release notes editor closed.");
            }
            KeyCode::F(2) => return self.save_release_now_notes(),
            _ => {
                if let Some(dialog) = &mut self.release_now_notes_dialog
                    && let Some(input) = convert_to_textarea_input(key)
                {
                    dialog.editor.input(input);
                }
            }
        }

        Ok(())
    }

    pub(crate) fn handle_top_picks_editor_key(&mut self, key: KeyEvent) -> Result<()> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s') {
            return self.save_top_picks();
        }

        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('C'))
            && let Some(dialog) = &mut self.top_picks_editor_dialog
        {
            dialog.editor.copy();
            let text = dialog.editor.yank_text();
            if !text.is_empty() {
                self.copy_text_to_clipboard(&text);
                return Ok(());
            }
        }

        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('x') | KeyCode::Char('X'))
            && let Some(dialog) = &mut self.top_picks_editor_dialog
        {
            dialog.editor.cut();
            let text = dialog.editor.yank_text();
            if !text.is_empty() {
                self.copy_text_to_clipboard(&text);
                return Ok(());
            }
        }

        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('a') | KeyCode::Char('A'))
            && let Some(dialog) = &mut self.top_picks_editor_dialog
        {
            dialog.editor.select_all();
            return Ok(());
        }

        match key.code {
            KeyCode::Esc => {
                self.top_picks_editor_dialog = None;
                self.status = StatusMessage::info("Top Picks editor closed.");
            }
            KeyCode::F(2) => return self.save_top_picks(),
            _ => {
                if let Some(dialog) = &mut self.top_picks_editor_dialog
                    && let Some(input) = convert_to_textarea_input(key)
                {
                    dialog.editor.input(input);
                }
            }
        }

        Ok(())
    }

    pub(crate) fn handle_ui_settings_key(&mut self, key: KeyEvent) -> Result<()> {
        if crate::app::ui_settings::try_handle_ui_settings_key(self, key)? {
            return Ok(());
        }
        Ok(())
    }

    pub(crate) fn handle_wizard_key(&mut self, key: KeyEvent) -> Result<()> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s') {
            return self.save_wizard_project();
        }

        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('o') {
            return self.open_browser_for_wizard_focus();
        }

        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('v') {
            self.paste_from_clipboard();
            return Ok(());
        }

        if self.wizard.focus_accepts_text() {
            match key.code {
                KeyCode::Esc => {
                    self.screen = Screen::Dashboard;
                    self.status = StatusMessage::info("Wizard cancelled.");
                }
                KeyCode::Tab | KeyCode::Down => self.wizard.focus_next(),
                KeyCode::BackTab | KeyCode::Up => self.wizard.focus_previous(),
                KeyCode::PageUp => self.scroll_wizard_body(-3),
                KeyCode::PageDown => self.scroll_wizard_body(3),
                KeyCode::F(5) => self.validate_wizard_target(),
                KeyCode::F(2) => return self.save_wizard_project(),
                KeyCode::Enter => {
                    self.wizard.focus_next();
                }
                _ => self.wizard.handle_text_input(key),
            }
            return Ok(());
        }

        match key.code {
            KeyCode::Esc => {
                self.screen = Screen::Dashboard;
                self.status = StatusMessage::info("Wizard cancelled.");
            }
            KeyCode::Tab | KeyCode::Down => self.wizard.focus_next(),
            KeyCode::BackTab | KeyCode::Up => self.wizard.focus_previous(),
            KeyCode::PageUp => self.scroll_wizard_body(-3),
            KeyCode::PageDown => self.scroll_wizard_body(3),
            KeyCode::F(5) => self.validate_wizard_target(),
            KeyCode::F(2) => return self.save_wizard_project(),
            KeyCode::Enter => return self.activate_wizard_focus(),
            KeyCode::Left => self.wizard.adjust_current_enum(-1),
            KeyCode::Right => self.wizard.adjust_current_enum(1),
            _ => self.wizard.handle_text_input(key),
        }
        Ok(())
    }

    pub(crate) fn handle_bump_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => {
                self.bump_dialog = None;
                self.status = StatusMessage::info("Bump preview closed.");
            }
            KeyCode::Up | KeyCode::BackTab => self.rotate_bump_scope(-1),
            KeyCode::Down | KeyCode::Tab => self.rotate_bump_scope(1),
            KeyCode::Left => self.rotate_bump_action(-1),
            KeyCode::Right => self.rotate_bump_action(1),
            KeyCode::Enter | KeyCode::F(2) => self.request_apply_bump()?,
            _ => {}
        }
        Ok(())
    }

    pub(crate) fn handle_overview_bump_workflow_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => self.cancel_overview_bump_workflow(),
            KeyCode::Up | KeyCode::BackTab => self.rotate_overview_bump_workflow(-1),
            KeyCode::Down | KeyCode::Tab => self.rotate_overview_bump_workflow(1),
            KeyCode::Char(character) => {
                if let Some(index) = digit_to_index(character) {
                    self.select_overview_bump_workflow(index);
                }
            }
            KeyCode::Enter | KeyCode::F(2) => return self.request_confirm_overview_bump_workflow(),
            _ => {}
        }
        Ok(())
    }

    pub(crate) fn handle_overview_bump_kind_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => self.cancel_overview_bump_kind(),
            KeyCode::Up | KeyCode::BackTab => self.rotate_overview_bump_kind(-1),
            KeyCode::Down | KeyCode::Tab => self.rotate_overview_bump_kind(1),
            KeyCode::Char(character) => {
                if let Some(index) = digit_to_index(character) {
                    self.select_overview_bump_kind(index);
                }
            }
            KeyCode::Enter | KeyCode::F(2) => return self.confirm_overview_bump_kind(),
            _ => {}
        }
        Ok(())
    }

    pub(crate) fn handle_overview_branch_bump_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => self.cancel_overview_branch_bump(),
            KeyCode::Up | KeyCode::BackTab => {
                if let Some(dialog) = &mut self.overview_branch_bump_dialog {
                    dialog.rotate(-1);
                }
            }
            KeyCode::Down | KeyCode::Tab => {
                if let Some(dialog) = &mut self.overview_branch_bump_dialog {
                    dialog.rotate(1);
                }
            }
            KeyCode::Char(character) if key.modifiers.is_empty() => {
                if let Some(index) = digit_to_index(character) {
                    if let Some(dialog) = &mut self.overview_branch_bump_dialog {
                        dialog.select(index);
                    }
                    return Ok(());
                }
                if let Some(dialog) = &mut self.overview_branch_bump_dialog
                    && dialog.input_enabled()
                {
                    dialog.branch_name.insert(character);
                }
            }
            KeyCode::Char(character) if key.modifiers == KeyModifiers::SHIFT => {
                if let Some(dialog) = &mut self.overview_branch_bump_dialog
                    && dialog.input_enabled()
                {
                    dialog.branch_name.insert(character);
                }
            }
            KeyCode::Enter | KeyCode::F(2) => return self.confirm_overview_branch_bump(),
            KeyCode::Backspace => {
                if let Some(dialog) = &mut self.overview_branch_bump_dialog
                    && dialog.input_enabled()
                {
                    dialog.branch_name.backspace();
                }
            }
            KeyCode::Delete => {
                if let Some(dialog) = &mut self.overview_branch_bump_dialog
                    && dialog.input_enabled()
                {
                    dialog.branch_name.delete();
                }
            }
            KeyCode::Left => {
                if let Some(dialog) = &mut self.overview_branch_bump_dialog
                    && dialog.input_enabled()
                {
                    dialog.branch_name.move_left();
                }
            }
            KeyCode::Right => {
                if let Some(dialog) = &mut self.overview_branch_bump_dialog
                    && dialog.input_enabled()
                {
                    dialog.branch_name.move_right();
                }
            }
            KeyCode::Home => {
                if let Some(dialog) = &mut self.overview_branch_bump_dialog
                    && dialog.input_enabled()
                {
                    dialog.branch_name.home();
                }
            }
            KeyCode::End => {
                if let Some(dialog) = &mut self.overview_branch_bump_dialog
                    && dialog.input_enabled()
                {
                    dialog.branch_name.end();
                }
            }
            KeyCode::PageUp => {
                if let Some(dialog) = &mut self.overview_branch_bump_dialog {
                    dialog.scroll_by(-3);
                }
            }
            KeyCode::PageDown => {
                if let Some(dialog) = &mut self.overview_branch_bump_dialog {
                    dialog.scroll_by(3);
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub(crate) fn handle_overview_bump_warning_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => self.cancel_overview_bump_warning(),
            KeyCode::Up | KeyCode::BackTab => self.rotate_overview_bump_warning(-1),
            KeyCode::Down | KeyCode::Tab => self.rotate_overview_bump_warning(1),
            KeyCode::Char('1') => self.select_overview_bump_warning(0),
            KeyCode::Char('2') => self.select_overview_bump_warning(1),
            KeyCode::Char('3') => self.select_overview_bump_warning(2),
            KeyCode::Enter | KeyCode::F(2) => return self.confirm_overview_bump_warning(),
            _ => {}
        }
        Ok(())
    }

    pub(crate) fn handle_main_branch_warning_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => self.cancel_main_branch_warning(),
            KeyCode::Up | KeyCode::BackTab => self.rotate_main_branch_warning(-1),
            KeyCode::Down | KeyCode::Tab => self.rotate_main_branch_warning(1),
            KeyCode::Char('1') => self.select_main_branch_warning(0),
            KeyCode::Char('2') => self.select_main_branch_warning(1),
            KeyCode::Char('3') => self.select_main_branch_warning(2),
            KeyCode::Enter | KeyCode::F(2) => return self.confirm_main_branch_warning(),
            _ => {}
        }
        Ok(())
    }

    pub(crate) fn handle_std_changelog_sub_branch_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => self.cancel_std_changelog_sub_branch_warning(),
            KeyCode::Up | KeyCode::BackTab => self.rotate_std_changelog_sub_branch_warning(-1),
            KeyCode::Down | KeyCode::Tab => self.rotate_std_changelog_sub_branch_warning(1),
            KeyCode::Char('1') => self.select_std_changelog_sub_branch_warning(0),
            KeyCode::Char('2') => self.select_std_changelog_sub_branch_warning(1),
            KeyCode::Char('3') => self.select_std_changelog_sub_branch_warning(2),
            KeyCode::Enter | KeyCode::F(2) => {
                return self.confirm_std_changelog_sub_branch_warning();
            }
            _ => {}
        }
        Ok(())
    }

    pub(crate) fn handle_changelog_preview_key(&mut self, key: KeyEvent) -> Result<()> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s') {
            return self.save_changelog_preview();
        }

        let mut refresh_selection = None;

        match key.code {
            KeyCode::Esc => self.cancel_changelog_preview(),
            KeyCode::F(2) => return self.confirm_changelog_preview(),
            KeyCode::Enter
                if self
                    .changelog_preview_dialog
                    .as_ref()
                    .is_some_and(|dialog| dialog.workflow.is_none()) =>
            {
                return self.confirm_changelog_preview();
            }
            KeyCode::Tab
                if self
                    .changelog_preview_dialog
                    .as_ref()
                    .and_then(|dialog| dialog.custom_range.as_ref())
                    .is_some() =>
            {
                if let Some(custom_range) = self
                    .changelog_preview_dialog
                    .as_mut()
                    .and_then(|dialog| dialog.custom_range.as_mut())
                {
                    custom_range.cycle_focus(1);
                }
            }
            KeyCode::BackTab
                if self
                    .changelog_preview_dialog
                    .as_ref()
                    .and_then(|dialog| dialog.custom_range.as_ref())
                    .is_some() =>
            {
                if let Some(custom_range) = self
                    .changelog_preview_dialog
                    .as_mut()
                    .and_then(|dialog| dialog.custom_range.as_mut())
                {
                    custom_range.cycle_focus(-1);
                }
            }
            KeyCode::Char('1')
                if self
                    .changelog_preview_dialog
                    .as_ref()
                    .and_then(|dialog| dialog.custom_range.as_ref())
                    .is_some() =>
            {
                if let Some(custom_range) = self
                    .changelog_preview_dialog
                    .as_mut()
                    .and_then(|dialog| dialog.custom_range.as_mut())
                {
                    custom_range.select_focus(CustomChangelogRangeFocus::From);
                }
            }
            KeyCode::Char('2')
                if self
                    .changelog_preview_dialog
                    .as_ref()
                    .and_then(|dialog| dialog.custom_range.as_ref())
                    .is_some() =>
            {
                if let Some(custom_range) = self
                    .changelog_preview_dialog
                    .as_mut()
                    .and_then(|dialog| dialog.custom_range.as_mut())
                {
                    custom_range.select_focus(CustomChangelogRangeFocus::To);
                }
            }
            KeyCode::Left
                if self
                    .changelog_preview_dialog
                    .as_ref()
                    .and_then(|dialog| dialog.custom_range.as_ref())
                    .is_some() =>
            {
                if let Some(custom_range) = self
                    .changelog_preview_dialog
                    .as_mut()
                    .and_then(|dialog| dialog.custom_range.as_mut())
                    && custom_range.adjust_focused_selection(-1)
                {
                    refresh_selection = custom_range.selection();
                }
            }
            KeyCode::Right
                if self
                    .changelog_preview_dialog
                    .as_ref()
                    .and_then(|dialog| dialog.custom_range.as_ref())
                    .is_some() =>
            {
                if let Some(custom_range) = self
                    .changelog_preview_dialog
                    .as_mut()
                    .and_then(|dialog| dialog.custom_range.as_mut())
                    && custom_range.adjust_focused_selection(1)
                {
                    refresh_selection = custom_range.selection();
                }
            }
            KeyCode::Char('r') | KeyCode::Char('R')
                if self
                    .changelog_preview_dialog
                    .as_ref()
                    .and_then(|dialog| dialog.custom_range.as_ref())
                    .is_some() =>
            {
                refresh_selection = self
                    .changelog_preview_dialog
                    .as_ref()
                    .and_then(|dialog| dialog.custom_range.as_ref())
                    .and_then(CustomChangelogRangeState::selection);
            }
            KeyCode::PageUp => self.scroll_changelog_preview(-8),
            KeyCode::PageDown => self.scroll_changelog_preview(8),
            KeyCode::Home => self.scroll_changelog_preview_to_start(),
            KeyCode::End => self.scroll_changelog_preview_to_end(),
            _ => {
                if let Some(dialog) = &mut self.changelog_preview_dialog {
                    if dialog.workflow.is_none() {
                        return Ok(());
                    }
                    if let Some(input) = convert_to_textarea_input(key) {
                        dialog.release_message.input(input);
                    }
                }
            }
        }

        if let Some(selection) = refresh_selection {
            return self.open_dashboard_changelog_preview(Some(selection));
        }

        Ok(())
    }

    pub(crate) fn handle_recent_changes_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => {
                self.recent_changes_dialog = None;
                self.cancel_background_job_kind(BackgroundJobKind::RecentChanges);
                self.cancel_background_job_kind(BackgroundJobKind::RecentChangesPrefetch);
                self.current_recent_changes_job_id = None;
                self.status = StatusMessage::info("Git log closed.");
            }
            KeyCode::Char('r') | KeyCode::Char('R')
                if key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                return self.open_commit_rename_from_view(RecentChangeView::Popup);
            }
            KeyCode::Up => self.scroll_recent_changes(-1),
            KeyCode::Down => self.scroll_recent_changes(1),
            KeyCode::PageUp => self.scroll_recent_changes(-8),
            KeyCode::PageDown => self.scroll_recent_changes(8),
            KeyCode::Tab => {
                if let Some(dialog) = &mut self.recent_changes_dialog {
                    if dialog.active_tab == RecentChangesTab::Recent && !dialog.history_loaded {
                        self.schedule_recent_changes_action(
                            "Loading tag history for the selected scope.",
                            RecentChangesLoadAction::SwitchTab(RecentChangesTab::History),
                        )?;
                    } else {
                        dialog.cycle_tab(1)?;
                    }
                }
            }
            KeyCode::BackTab => {
                if let Some(dialog) = &mut self.recent_changes_dialog {
                    dialog.cycle_tab(-1)?;
                }
            }
            KeyCode::Char('1') => {
                if let Some(dialog) = &mut self.recent_changes_dialog {
                    dialog.switch_tab(RecentChangesTab::Recent)?;
                }
            }
            KeyCode::Char('2') => {
                if let Some(dialog) = &mut self.recent_changes_dialog {
                    if dialog.history_loaded {
                        dialog.switch_tab(RecentChangesTab::History)?;
                    } else {
                        self.schedule_recent_changes_action(
                            "Loading tag history for the selected scope.",
                            RecentChangesLoadAction::SwitchTab(RecentChangesTab::History),
                        )?;
                    }
                }
            }
            KeyCode::Char('[') if self.recent_changes_dialog.is_some() => {
                self.schedule_recent_changes_action(
                    "Loading git history for the previous scope.",
                    RecentChangesLoadAction::RotateScope(-1),
                )?;
            }
            KeyCode::Char(']') if self.recent_changes_dialog.is_some() => {
                self.schedule_recent_changes_action(
                    "Loading git history for the next scope.",
                    RecentChangesLoadAction::RotateScope(1),
                )?;
            }
            KeyCode::Char('r') | KeyCode::Char('R') if self.recent_changes_dialog.is_some() => {
                self.schedule_recent_changes_action(
                    "Refreshing git history for the current scope.",
                    RecentChangesLoadAction::RefreshCurrentScope,
                )?;
            }
            KeyCode::Left => {
                if let Some(dialog) = &mut self.recent_changes_dialog {
                    if dialog.active_tab == RecentChangesTab::Recent && dialog.can_select_scope() {
                        self.schedule_recent_changes_action(
                            "Loading git history for the previous scope.",
                            RecentChangesLoadAction::RotateScope(-1),
                        )?;
                    } else if dialog.active_tab == RecentChangesTab::History {
                        dialog.navigate_history(1);
                    }
                }
            }
            KeyCode::Right => {
                if let Some(dialog) = &mut self.recent_changes_dialog {
                    if dialog.active_tab == RecentChangesTab::Recent && dialog.can_select_scope() {
                        self.schedule_recent_changes_action(
                            "Loading git history for the next scope.",
                            RecentChangesLoadAction::RotateScope(1),
                        )?;
                    } else if dialog.active_tab == RecentChangesTab::History {
                        dialog.navigate_history(-1);
                    }
                }
            }
            KeyCode::Char('t') => self.open_tag_dialog()?,
            _ => {}
        }
        Ok(())
    }

    pub(crate) fn handle_commit_rename_key(&mut self, key: KeyEvent) -> Result<()> {
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('p') | KeyCode::Char('P'))
        {
            self.toggle_commit_rename_force_push();
            return Ok(());
        }

        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('C'))
        {
            self.status = StatusMessage::info("Ctrl+C pressed".to_string());
            // Try textarea first (commit rename dialog)
            if let Some(dialog) = &mut self.commit_rename_dialog {
                let was_selecting = dialog.message_editor.is_selecting();
                self.status = StatusMessage::info(format!("Ctrl+C: selecting={}", was_selecting));
                dialog.message_editor.copy();
                let text: String = dialog.message_editor.yank_text();
                self.status = StatusMessage::info(format!("Ctrl+C: yank text len={}", text.len()));
                if !text.is_empty() {
                    self.copy_text_to_clipboard(&text);
                    self.status = StatusMessage::info("Ctrl+C: copied to clipboard".to_string());
                    return Ok(());
                }
            }
            // Fall back to regular text input
            let selected_text = self
                .active_text_input_mut()
                .and_then(|input| input.selected_text().map(str::to_string));
            if let Some(text) = selected_text {
                self.copy_text_to_clipboard(&text);
                return Ok(());
            }
        }

        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('x') | KeyCode::Char('X'))
            && let Some(dialog) = &mut self.commit_rename_dialog
        {
            self.status = StatusMessage::info("Ctrl+X pressed".to_string());
            dialog.message_editor.cut();
            let text: String = dialog.message_editor.yank_text();
            if !text.is_empty() {
                self.copy_text_to_clipboard(&text);
                return Ok(());
            }
        }

        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('a') | KeyCode::Char('A'))
            && let Some(dialog) = &mut self.commit_rename_dialog
        {
            dialog.message_editor.select_all();
            return Ok(());
        }

        match key.code {
            KeyCode::Esc => {
                self.commit_rename_dialog = None;
                self.status = StatusMessage::info("Commit rename cancelled.");
            }
            KeyCode::Enter => {
                if key.modifiers.contains(KeyModifiers::ALT) {
                    // Alt+Enter inserts a newline
                    if let Some(dialog) = &mut self.commit_rename_dialog {
                        dialog.message_editor.insert_newline();
                    }
                } else {
                    return self.apply_commit_rename();
                }
            }
            KeyCode::F(2) => return self.apply_commit_rename(),
            KeyCode::Tab => {
                if self
                    .commit_rename_dialog
                    .as_ref()
                    .map(|dialog| dialog.plan.touches_pushed_history)
                    .unwrap_or(false)
                {
                    self.toggle_commit_rename_force_push();
                }
            }
            KeyCode::Up => {
                if let Some(dialog) = &mut self.commit_rename_dialog {
                    dialog
                        .message_editor
                        .move_cursor(tui_textarea::CursorMove::Up);
                }
            }
            KeyCode::Down => {
                if let Some(dialog) = &mut self.commit_rename_dialog {
                    dialog
                        .message_editor
                        .move_cursor(tui_textarea::CursorMove::Down);
                }
            }
            _ => {
                if let Some(dialog) = &mut self.commit_rename_dialog
                    && let Some(input) = convert_to_textarea_input(key)
                {
                    dialog.message_editor.input(input);
                }
            }
        }

        Ok(())
    }

    pub(crate) fn handle_tag_key(&mut self, key: KeyEvent) -> Result<()> {
        let focus_accepts_text = self
            .tag_dialog
            .as_ref()
            .is_some_and(|dialog| dialog.focus_accepts_text());

        if focus_accepts_text {
            match key.code {
                KeyCode::Esc => {
                    self.tag_dialog = None;
                    self.tag_annotation_dialog = None;
                    self.status = StatusMessage::info("Tag creation cancelled.");
                }
                KeyCode::Tab => {
                    if let Some(dialog) = &mut self.tag_dialog {
                        dialog.focus = crate::workflow::dialogs::TagDialogFocus::Controls;
                    }
                }
                KeyCode::Enter | KeyCode::F(2) => return self.create_local_tag(),
                _ => {
                    if let Some(dialog) = &mut self.tag_dialog {
                        dialog.tag_name.handle_key(key);
                    }
                }
            }
            return Ok(());
        }

        match key.code {
            KeyCode::Esc => {
                self.tag_dialog = None;
                self.tag_annotation_dialog = None;
                self.status = StatusMessage::info("Tag creation cancelled.");
            }
            KeyCode::Tab | KeyCode::BackTab => {
                if let Some(dialog) = &mut self.tag_dialog {
                    dialog.focus = crate::workflow::dialogs::TagDialogFocus::TagName;
                }
            }
            KeyCode::Char('[') => self.rotate_tag_scope(-1),
            KeyCode::Char(']') => self.rotate_tag_scope(1),
            KeyCode::Char('a') | KeyCode::Char('A') => self.open_tag_annotation_dialog()?,
            KeyCode::Left => self.rotate_tag_action(-1),
            KeyCode::Right => self.rotate_tag_action(1),
            KeyCode::Enter | KeyCode::F(2) => return self.create_local_tag(),
            _ => {}
        }
        Ok(())
    }

    pub(crate) fn handle_tag_annotation_key(&mut self, key: KeyEvent) -> Result<()> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s') {
            return self.save_tag_annotation();
        }

        match key.code {
            KeyCode::Esc => {
                self.tag_annotation_dialog = None;
                self.status = StatusMessage::info("Tag annotation editor closed.");
            }
            KeyCode::F(2) => return self.save_tag_annotation(),
            _ => {
                if let Some(dialog) = &mut self.tag_annotation_dialog
                    && let Some(input) = convert_to_textarea_input(key)
                {
                    dialog.editor.input(input);
                }
            }
        }

        Ok(())
    }

    pub(crate) fn handle_project_edit_key(&mut self, key: KeyEvent) -> Result<()> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('o') {
            return self.open_browser_for_project_edit_focus();
        }

        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('v') {
            self.paste_from_clipboard();
            return Ok(());
        }

        let focus_accepts_text = self
            .project_edit_dialog
            .as_ref()
            .map(|dialog| dialog.focus_accepts_text())
            .unwrap_or(false);

        if focus_accepts_text {
            match key.code {
                KeyCode::Esc => {
                    self.project_edit_dialog = None;
                    self.status = StatusMessage::info("Project edit cancelled.");
                }
                KeyCode::Tab | KeyCode::Down => {
                    if let Some(dialog) = &mut self.project_edit_dialog {
                        dialog.focus_next();
                    }
                }
                KeyCode::BackTab | KeyCode::Up => {
                    if let Some(dialog) = &mut self.project_edit_dialog {
                        dialog.focus_previous();
                    }
                }
                KeyCode::PageUp => self.scroll_project_edit_body(-3),
                KeyCode::PageDown => self.scroll_project_edit_body(3),
                KeyCode::F(2) => return self.save_project_edit(),
                KeyCode::Enter => {
                    if let Some(dialog) = &mut self.project_edit_dialog {
                        dialog.focus_next();
                    }
                }
                _ => {
                    if let Some(dialog) = &mut self.project_edit_dialog {
                        dialog.handle_text_input(key);
                    }
                }
            }
            return Ok(());
        }

        match key.code {
            KeyCode::Esc => {
                self.project_edit_dialog = None;
                self.status = StatusMessage::info("Project edit cancelled.");
            }
            KeyCode::Tab | KeyCode::Down => {
                if let Some(dialog) = &mut self.project_edit_dialog {
                    dialog.focus_next();
                }
            }
            KeyCode::BackTab | KeyCode::Up => {
                if let Some(dialog) = &mut self.project_edit_dialog {
                    dialog.focus_previous();
                }
            }
            KeyCode::PageUp => self.scroll_project_edit_body(-3),
            KeyCode::PageDown => self.scroll_project_edit_body(3),
            KeyCode::Enter => {
                if let Some(dialog) = &self.project_edit_dialog {
                    if dialog.is_save_focused() {
                        return self.save_project_edit();
                    }
                    if dialog.is_add_scope_focused() {
                        return self.apply_project_edit_scope_action(ScopeAction::Add);
                    }
                    if dialog.is_remove_scope_focused() {
                        return self.apply_project_edit_scope_action(ScopeAction::Remove);
                    }
                    if dialog.focus == ProjectEditFocus::TargetKey {
                        if let Some(dialog) = &mut self.project_edit_dialog {
                            dialog.enable_custom_target_key();
                        }
                        self.status = StatusMessage::info("Custom target key input enabled.");
                        return Ok(());
                    }
                    if dialog.is_remove_focused() {
                        return self.remove_project();
                    }
                    if dialog.is_cancel_focused() {
                        self.project_edit_dialog = None;
                        self.status = StatusMessage::info("Project edit cancelled.");
                        return Ok(());
                    }
                }
            }
            KeyCode::F(2) => return self.save_project_edit(),
            KeyCode::Delete if key.modifiers.is_empty() => {
                let remove_scope = self
                    .project_edit_dialog
                    .as_ref()
                    .is_some_and(|dialog| dialog.project_type == ProjectType::Branched);
                if remove_scope {
                    return self.apply_project_edit_scope_action(ScopeAction::Remove);
                }
                return self.remove_project();
            }
            KeyCode::Left => {
                if let Some(dialog) = &mut self.project_edit_dialog {
                    dialog.adjust_current_enum(-1);
                }
            }
            KeyCode::Right => {
                if let Some(dialog) = &mut self.project_edit_dialog {
                    dialog.adjust_current_enum(1);
                }
            }
            _ => {
                if let Some(dialog) = &mut self.project_edit_dialog {
                    dialog.handle_text_input(key);
                }
            }
        }
        Ok(())
    }

    pub(crate) fn handle_browser_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => {
                self.browser_dialog = None;
                self.status = StatusMessage::info("Browse cancelled.");
            }
            KeyCode::Enter | KeyCode::F(2) => return self.confirm_browser_selection(),
            KeyCode::Char('u') | KeyCode::Char('U') => {
                return self.confirm_browser_directory_selection();
            }
            _ => {
                if let Some(dialog) = &mut self.browser_dialog {
                    let event = Event::Key(key);
                    dialog.explorer.handle(&event)?;
                }
            }
        }
        Ok(())
    }

    pub(crate) fn handle_paste(&mut self, text: String) {
        if let Some(dialog) = &mut self.tag_annotation_dialog {
            dialog.editor.insert_str(text);
            self.status = StatusMessage::info("Pasted into the tag annotation.");
            return;
        }

        if let Some(dialog) = &mut self.changelog_preview_dialog
            && dialog.workflow.is_some()
        {
            dialog.release_message.insert_str(text);
            self.status = StatusMessage::info("Pasted into the release notes.");
            return;
        }

        if let Some(dialog) = &mut self.release_now_notes_dialog {
            for line in text.lines() {
                dialog.editor.insert_str(line);
                dialog.editor.insert_newline();
            }
            self.status = StatusMessage::info("Pasted into the release notes.");
            return;
        }

        if let Some(dialog) = &mut self.commit_rename_dialog {
            for line in text.lines() {
                dialog.message_editor.insert_str(line);
                dialog.message_editor.insert_newline();
            }
            self.status = StatusMessage::info("Pasted into the commit message.");
            return;
        }

        if let Some(dialog) = &mut self.top_picks_editor_dialog {
            for line in text.lines() {
                dialog.editor.insert_str(line);
                dialog.editor.insert_newline();
            }
            self.status = StatusMessage::info("Pasted into the Top Picks editor.");
            return;
        }

        let sanitized = sanitize_pasted_text(&text);
        if self.insert_text(&sanitized) {
            self.status = StatusMessage::info("Pasted into the active field.");
        }
    }

    pub(crate) fn paste_from_clipboard(&mut self) {
        self.status = StatusMessage::info("Paste triggered".to_string());
        let clipboard = if let Some(ref mut clipboard) = self.clipboard {
            clipboard
        } else {
            self.status = StatusMessage::info("Creating new clipboard".to_string());
            self.clipboard = Clipboard::new().ok();
            if let Some(ref mut clipboard) = self.clipboard {
                clipboard
            } else {
                self.status = StatusMessage::warning("Clipboard creation failed".to_string());
                #[cfg(target_os = "linux")]
                if let Some(text) = paste_from_linux_clipboard_cli() {
                    self.handle_paste(text);
                    return;
                }
                if let Some(text) = self.fallback_clipboard.clone() {
                    self.handle_paste(text);
                } else {
                    self.status = StatusMessage::warning("No clipboard content is available.");
                }
                return;
            }
        };

        self.status = StatusMessage::info("Getting clipboard text...".to_string());
        match clipboard.get_text() {
            Ok(text) => {
                self.status =
                    StatusMessage::info(format!("Got clipboard text: {} chars", text.len()));
                if let Some(dialog) = &mut self.tag_annotation_dialog {
                    dialog.editor.insert_str(text);
                    self.status = StatusMessage::info("Pasted into the tag annotation.");
                    return;
                }

                if let Some(dialog) = &mut self.changelog_preview_dialog
                    && dialog.workflow.is_some()
                {
                    dialog.release_message.insert_str(text);
                    self.status = StatusMessage::info("Pasted into the release notes.");
                    return;
                }

                if let Some(dialog) = &mut self.release_now_notes_dialog {
                    for line in text.lines() {
                        dialog.editor.insert_str(line);
                        dialog.editor.insert_newline();
                    }
                    self.status = StatusMessage::info("Pasted into the release notes.");
                    return;
                }

                if let Some(dialog) = &mut self.commit_rename_dialog {
                    for line in text.lines() {
                        dialog.message_editor.insert_str(line);
                        dialog.message_editor.insert_newline();
                    }
                    self.status = StatusMessage::info("Pasted into the commit message.");
                    return;
                }

                if let Some(dialog) = &mut self.top_picks_editor_dialog {
                    for line in text.lines() {
                        dialog.editor.insert_str(line);
                        dialog.editor.insert_newline();
                    }
                    self.status = StatusMessage::info("Pasted into the Top Picks editor.");
                    return;
                }

                let sanitized = sanitize_pasted_text(&text);
                if self.insert_text(&sanitized) {
                    self.status = StatusMessage::info("Pasted into the active field.");
                } else {
                    self.status = StatusMessage::warning("No editable field is focused.");
                }
            }
            Err(_) => {
                #[cfg(target_os = "linux")]
                if let Some(text) = paste_from_linux_clipboard_cli() {
                    self.handle_paste(text);
                    return;
                }
                if let Some(text) = self.fallback_clipboard.clone() {
                    self.handle_paste(text);
                } else {
                    self.status = StatusMessage::warning("Clipboard paste failed.");
                }
            }
        }
    }

    pub(crate) fn insert_text(&mut self, text: &str) -> bool {
        if let Some(dialog) = &mut self.project_edit_dialog
            && dialog.insert_text(text)
        {
            return true;
        }

        if let Some(dialog) = &mut self.tag_dialog {
            dialog.tag_name.insert_str(text);
            return true;
        }

        if self.screen == Screen::Dashboard
            && self.overview_tab == OverviewTab::ProjectSettings
            && project_settings::insert_project_settings_text(self, text)
        {
            return true;
        }

        if self.screen == Screen::Wizard && self.wizard.insert_text(text) {
            return true;
        }

        if let Some(dialog) = &mut self.commit_rename_dialog {
            for line in text.lines() {
                dialog.message_editor.insert_str(line);
                dialog.message_editor.insert_newline();
            }
            return true;
        }

        false
    }

    pub(crate) fn handle_tab_shortcut(&mut self, key: KeyEvent) -> bool {
        if !key.modifiers.is_empty() {
            return false;
        }

        if matches!(self.screen, Screen::Wizard) && self.wizard.focus_accepts_text() {
            return false;
        }

        if self.screen == Screen::Dashboard && self.dashboard_focus == DashboardPane::Overview {
            let target = if let KeyCode::Char(digit @ '1'..='4') = key.code {
                let index = (digit as u8 - b'1') as usize;
                overview_tabs(self.overview_show_recent_tab)
                    .get(index)
                    .copied()
            } else {
                None
            };
            if let Some(target) = target {
                if self.overview_tab != target {
                    self.overview_tab = target;
                    crate::app::ui_settings::flash_overview_tab_selection(
                        self,
                        self.overview_show_recent_tab,
                    );
                }
                return true;
            }
        }

        let target = match key.code {
            KeyCode::Char('1') => Some(Screen::Dashboard),
            KeyCode::Char('2') => Some(Screen::Wizard),
            KeyCode::Char('3') | KeyCode::Char('s') | KeyCode::Char('S') => {
                Some(Screen::UiSettings)
            }
            _ => None,
        };

        let Some(target) = target else {
            return false;
        };

        match target {
            Screen::Wizard => self.open_wizard(),
            Screen::Dashboard => {
                self.screen = Screen::Dashboard;
                self.dashboard_focus = DashboardPane::Projects;
            }
            _ => self.screen = target,
        }
        true
    }

    pub(crate) fn try_handle_help_shortcut(&mut self, key: KeyEvent) -> Result<bool> {
        if key.modifiers.is_empty() && key.code == KeyCode::Char('?') {
            if self.help_blocked_by_text_input() {
                return Ok(false);
            }
            let context = self.resolve_help_context();
            self.help_modal = Some(crate::tui::HelpModal::new(context));
            return Ok(true);
        }
        Ok(false)
    }

    pub(crate) fn handle_help_key(&mut self, key: KeyEvent) -> bool {
        let Some(modal) = &mut self.help_modal else {
            return false;
        };
        if modal.handle_key(key) {
            self.help_modal = None;
            return true;
        }
        true
    }

    pub(crate) fn try_handle_ui_shortcut(&mut self, key: KeyEvent) -> Result<bool> {
        if key.modifiers.is_empty() && matches!(key.code, KeyCode::Char('h') | KeyCode::Char('H')) {
            if matches!(self.screen, Screen::Wizard) && self.wizard.focus_accepts_text() {
                return Ok(false);
            }
            if self
                .project_edit_dialog
                .as_ref()
                .map(|dialog| dialog.focus_accepts_text())
                .unwrap_or(false)
            {
                return Ok(false);
            }
            if project_settings::captures_text_input(self) {
                return Ok(false);
            }
            self.toggle_footer()?;
            return Ok(true);
        }
        Ok(false)
    }

    pub(crate) fn try_handle_toast_shortcut(&mut self, key: KeyEvent) -> bool {
        if key.modifiers.contains(KeyModifiers::ALT)
            && matches!(key.code, KeyCode::Char('x') | KeyCode::Char('X'))
        {
            let interaction = self.toaster.handle_shortcut(ToastShortcut::Dismiss);
            return self.handle_toast_interaction(interaction);
        }

        if key.code == KeyCode::F(5) && self.screen != Screen::Wizard {
            let interaction = self.toaster.handle_shortcut(ToastShortcut::Copy);
            return self.handle_toast_interaction(interaction);
        }

        false
    }
}
