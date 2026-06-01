// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit SA-PS License

use super::super::*;

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use ratatui_comfy_toaster::ToastMouseButton;

use crate::workflow::dialogs::TextInput;

impl App {
    pub(crate) fn handle_mouse(&mut self, mouse: MouseEvent) {
        self.update_mouse_position(&mouse);

        if self.snif_dialog.is_some() {
            return;
        }
        if self.handle_toast_mouse(mouse) {
            return;
        }

        if self.help_modal.is_some() {
            match mouse.kind {
                MouseEventKind::ScrollUp => {
                    if let Some(help) = &mut self.help_modal {
                        help.scroll_wheel(-3);
                    }
                }
                MouseEventKind::ScrollDown => {
                    if let Some(help) = &mut self.help_modal {
                        help.scroll_wheel(3);
                    }
                }
                _ => {}
            }
            return;
        }

        if self.progress_dialog.is_some() {
            return;
        }

        if self.release_now_notes_dialog.is_some() {
            match mouse.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    if let Some((action, rect)) =
                        self.resolve_hit_target(mouse.column, mouse.row, false)
                    {
                        if matches!(action, HitAction::ReleaseNowNotesField) {
                            if let Some(dialog) = &mut self.release_now_notes_dialog {
                                let inner = Rect {
                                    x: rect.x + 1,
                                    y: rect.y + 1,
                                    width: rect.width.saturating_sub(2),
                                    height: rect.height.saturating_sub(2),
                                };
                                let lines = dialog.editor.lines();
                                let (cursor_row, _) = dialog.editor.cursor();
                                let visible_height = inner.height.max(1) as usize;
                                let start_row = cursor_row
                                    .saturating_sub(visible_height / 2)
                                    .min(lines.len().saturating_sub(visible_height));
                                let end_row = (start_row + visible_height).min(lines.len());
                                let number_width = end_row.max(1).to_string().len().max(2) as u16;
                                let content_width =
                                    inner.width.saturating_sub(number_width + 1).max(1) as usize;
                                let relative_row = mouse.row.saturating_sub(inner.y) as usize;
                                let clicked_col =
                                    mouse.column.saturating_sub(inner.x + number_width + 1)
                                        as usize;
                                let lines_ref: Vec<&str> =
                                    lines.iter().map(|s| s.as_str()).collect();
                                let (target_row, target_col) = textarea_click_position(
                                    &lines_ref,
                                    start_row,
                                    content_width,
                                    relative_row,
                                    clicked_col,
                                );
                                let now = Instant::now();
                                let is_double_click = self
                                    .release_now_notes_textarea_click_at
                                    .map(|prev| {
                                        now.duration_since(prev) <= Duration::from_millis(400)
                                    })
                                    .unwrap_or(false);
                                dialog.editor.cancel_selection();
                                dialog.editor.move_cursor(tui_textarea::CursorMove::Jump(
                                    target_row as u16,
                                    target_col as u16,
                                ));
                                if is_double_click {
                                    dialog
                                        .editor
                                        .move_cursor(tui_textarea::CursorMove::WordBack);
                                    dialog.editor.start_selection();
                                    dialog
                                        .editor
                                        .move_cursor(tui_textarea::CursorMove::WordForward);
                                    dialog.editor.move_cursor(tui_textarea::CursorMove::Back);
                                }
                                self.release_now_notes_textarea_click_at = Some(now);
                            }
                        } else if let Err(error) = self.handle_hit_action(action) {
                            self.status = StatusMessage::error(error.to_string());
                        }
                    }
                    return;
                }
                MouseEventKind::Drag(MouseButton::Left) => {
                    if let Some((action, rect)) =
                        self.resolve_hit_target(mouse.column, mouse.row, false)
                        && matches!(action, HitAction::ReleaseNowNotesField)
                        && let Some(dialog) = &mut self.release_now_notes_dialog
                    {
                        let inner = Rect {
                            x: rect.x + 1,
                            y: rect.y + 1,
                            width: rect.width.saturating_sub(2),
                            height: rect.height.saturating_sub(2),
                        };
                        let lines = dialog.editor.lines();
                        let (cursor_row, _) = dialog.editor.cursor();
                        let visible_height = inner.height.max(1) as usize;
                        let start_row = cursor_row
                            .saturating_sub(visible_height / 2)
                            .min(lines.len().saturating_sub(visible_height));
                        let end_row = (start_row + visible_height).min(lines.len());
                        let number_width = end_row.max(1).to_string().len().max(2) as u16;
                        let content_width =
                            inner.width.saturating_sub(number_width + 1).max(1) as usize;
                        let relative_row = mouse.row.saturating_sub(inner.y) as usize;
                        let clicked_col =
                            mouse.column.saturating_sub(inner.x + number_width + 1) as usize;
                        let lines_ref: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
                        let (target_row, target_col) = textarea_click_position(
                            &lines_ref,
                            start_row,
                            content_width,
                            relative_row,
                            clicked_col,
                        );
                        if !dialog.editor.is_selecting() {
                            dialog.editor.start_selection();
                        }
                        dialog.editor.move_cursor(tui_textarea::CursorMove::Jump(
                            target_row as u16,
                            target_col as u16,
                        ));
                    }
                    return;
                }
                MouseEventKind::Down(MouseButton::Right) => {
                    if let Some(dialog) = &mut self.release_now_notes_dialog
                        && dialog.editor.is_selecting()
                    {
                        dialog.editor.copy();
                        let text = dialog.editor.yank_text();
                        if !text.is_empty() {
                            self.copy_text_to_clipboard(&text);
                            return;
                        }
                    }
                    self.paste_from_clipboard();
                    return;
                }
                _ => return,
            }
        }

        if self.top_picks_editor_dialog.is_some() {
            match mouse.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    if let Some((action, rect)) =
                        self.resolve_hit_target(mouse.column, mouse.row, false)
                    {
                        if matches!(action, HitAction::TopPicksEditorField) {
                            if let Some(dialog) = &mut self.top_picks_editor_dialog {
                                let inner = Rect {
                                    x: rect.x + 1,
                                    y: rect.y + 1,
                                    width: rect.width.saturating_sub(2),
                                    height: rect.height.saturating_sub(2),
                                };
                                let lines = dialog.editor.lines();
                                let (cursor_row, _) = dialog.editor.cursor();
                                let visible_height = inner.height.max(1) as usize;
                                let start_row = cursor_row
                                    .saturating_sub(visible_height / 2)
                                    .min(lines.len().saturating_sub(visible_height));
                                let end_row = (start_row + visible_height).min(lines.len());
                                let number_width = end_row.max(1).to_string().len().max(2) as u16;
                                let content_width =
                                    inner.width.saturating_sub(number_width + 1).max(1) as usize;
                                let relative_row = mouse.row.saturating_sub(inner.y) as usize;
                                let clicked_col =
                                    mouse.column.saturating_sub(inner.x + number_width + 1)
                                        as usize;
                                let lines_ref: Vec<&str> =
                                    lines.iter().map(|s| s.as_str()).collect();
                                let (target_row, target_col) = textarea_click_position(
                                    &lines_ref,
                                    start_row,
                                    content_width,
                                    relative_row,
                                    clicked_col,
                                );
                                // Check for double-click (word selection)
                                let now = Instant::now();
                                let is_double_click = self
                                    .top_picks_editor_click_at
                                    .map(|prev| {
                                        now.duration_since(prev) <= Duration::from_millis(400)
                                    })
                                    .unwrap_or(false);
                                dialog.editor.cancel_selection();
                                dialog.editor.move_cursor(tui_textarea::CursorMove::Jump(
                                    target_row as u16,
                                    target_col as u16,
                                ));
                                if is_double_click {
                                    dialog
                                        .editor
                                        .move_cursor(tui_textarea::CursorMove::WordBack);
                                    dialog.editor.start_selection();
                                    dialog
                                        .editor
                                        .move_cursor(tui_textarea::CursorMove::WordForward);
                                    dialog.editor.move_cursor(tui_textarea::CursorMove::Back);
                                }
                                self.top_picks_editor_click_at = Some(now);
                                self.top_picks_editor_rect = Some(rect);
                            }
                        } else if let Err(error) = self.handle_hit_action(action) {
                            self.status = StatusMessage::error(error.to_string());
                        }
                    }
                    return;
                }
                MouseEventKind::Drag(MouseButton::Left) => {
                    if let Some((action, rect)) =
                        self.resolve_hit_target(mouse.column, mouse.row, false)
                        && matches!(action, HitAction::TopPicksEditorField)
                        && let Some(dialog) = &mut self.top_picks_editor_dialog
                    {
                        let inner = Rect {
                            x: rect.x + 1,
                            y: rect.y + 1,
                            width: rect.width.saturating_sub(2),
                            height: rect.height.saturating_sub(2),
                        };
                        let lines = dialog.editor.lines();
                        let (cursor_row, _) = dialog.editor.cursor();
                        let visible_height = inner.height.max(1) as usize;
                        let start_row = cursor_row
                            .saturating_sub(visible_height / 2)
                            .min(lines.len().saturating_sub(visible_height));
                        let end_row = (start_row + visible_height).min(lines.len());
                        let number_width = end_row.max(1).to_string().len().max(2) as u16;
                        let content_width =
                            inner.width.saturating_sub(number_width + 1).max(1) as usize;
                        let relative_row = mouse.row.saturating_sub(inner.y) as usize;
                        let clicked_col =
                            mouse.column.saturating_sub(inner.x + number_width + 1) as usize;
                        let lines_ref: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
                        let (target_row, target_col) = textarea_click_position(
                            &lines_ref,
                            start_row,
                            content_width,
                            relative_row,
                            clicked_col,
                        );
                        if !dialog.editor.is_selecting() {
                            dialog.editor.start_selection();
                        }
                        dialog.editor.move_cursor(tui_textarea::CursorMove::Jump(
                            target_row as u16,
                            target_col as u16,
                        ));
                    }
                    return;
                }
                MouseEventKind::Down(MouseButton::Right) => {
                    if let Some(dialog) = &mut self.top_picks_editor_dialog
                        && dialog.editor.is_selecting()
                    {
                        dialog.editor.copy();
                        let text = dialog.editor.yank_text();
                        if !text.is_empty() {
                            self.copy_text_to_clipboard(&text);
                            return;
                        }
                    }
                    self.paste_from_clipboard();
                    return;
                }
                MouseEventKind::ScrollUp => {
                    if let Some(dialog) = &mut self.top_picks_editor_dialog {
                        scroll_textarea_by_lines(&mut dialog.editor, -3);
                    }
                    return;
                }
                MouseEventKind::ScrollDown => {
                    if let Some(dialog) = &mut self.top_picks_editor_dialog {
                        scroll_textarea_by_lines(&mut dialog.editor, 3);
                    }
                    return;
                }
                _ => return,
            }
        }

        if self.release_now_dialog.is_some() {
            let in_log_viewport = self
                .release_now_log_viewport
                .map(|viewport| rect_contains(viewport, mouse.column, mouse.row))
                .unwrap_or(false);
            match mouse.kind {
                MouseEventKind::ScrollUp => {
                    self.scroll_release_now(-2);
                    return;
                }
                MouseEventKind::ScrollDown => {
                    self.scroll_release_now(2);
                    return;
                }
                MouseEventKind::Down(MouseButton::Left) => {
                    if in_log_viewport && self.begin_release_now_log_selection(mouse.row) {
                        return;
                    }
                    if let Some(action) = self.resolve_hit_action(mouse.column, mouse.row, false)
                        && let Err(error) = self.handle_hit_action(action)
                    {
                        self.status = StatusMessage::error(error.to_string());
                    }
                    return;
                }
                MouseEventKind::Drag(MouseButton::Left) => {
                    if in_log_viewport && self.update_release_now_log_selection(mouse.row) {
                        return;
                    }
                    return;
                }
                MouseEventKind::Down(MouseButton::Right) => {
                    if in_log_viewport {
                        self.copy_selected_release_now_log(mouse.row);
                    }
                    return;
                }
                MouseEventKind::Up(MouseButton::Left) => return,
                _ => return,
            }
        }

        if self.delete_confirmation_dialog.is_some() {
            match mouse.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    if let Some(action) = self.resolve_hit_action(mouse.column, mouse.row, false)
                        && let Err(error) = self.handle_hit_action(action)
                    {
                        self.status = StatusMessage::error(error.to_string());
                    }
                    return;
                }
                MouseEventKind::ScrollUp
                | MouseEventKind::ScrollDown
                | MouseEventKind::Down(MouseButton::Right)
                | MouseEventKind::Drag(MouseButton::Left)
                | MouseEventKind::Up(MouseButton::Left) => return,
                _ => return,
            }
        }

        if self.commit_rename_dialog.is_some() {
            match mouse.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    if let Some((action, rect)) =
                        self.resolve_hit_target(mouse.column, mouse.row, false)
                    {
                        let maybe_click_target = self.text_input_click_target(&action);
                        let mut select_all = false;
                        if let Some(target) = maybe_click_target {
                            let now = Instant::now();
                            if self.last_text_input_click_target == Some(target)
                                && self
                                    .last_text_input_click_at
                                    .map(|previous| {
                                        now.duration_since(previous) <= Duration::from_millis(400)
                                    })
                                    .unwrap_or(false)
                            {
                                select_all = true;
                            }
                            self.last_text_input_click_target = Some(target);
                            self.last_text_input_click_at = Some(now);
                        } else {
                            self.last_text_input_click_target = None;
                            self.last_text_input_click_at = None;
                        }
                        let is_commit_rename_field =
                            matches!(action, HitAction::CommitRenameMessageField);
                        if let Err(error) = self.handle_hit_action(action) {
                            self.status = StatusMessage::error(error.to_string());
                        }
                        if maybe_click_target.is_some() {
                            if select_all {
                                if let Some(input) = self.active_text_input_mut() {
                                    input.select_all();
                                }
                            } else {
                                self.set_text_input_cursor_from_mouse(rect, mouse.column);
                            }
                        }
                        // Handle textarea cursor positioning for commit rename dialog
                        if is_commit_rename_field
                            && let Some(dialog) = &mut self.commit_rename_dialog
                        {
                            let inner = Rect {
                                x: rect.x + 1,
                                y: rect.y + 1,
                                width: rect.width.saturating_sub(2),
                                height: rect.height.saturating_sub(2),
                            };
                            let lines = dialog.message_editor.lines();
                            let (cursor_row, _) = dialog.message_editor.cursor();
                            let visible_height = inner.height.max(1) as usize;
                            let start_row = cursor_row
                                .saturating_sub(visible_height / 2)
                                .min(lines.len().saturating_sub(visible_height));
                            let end_row = (start_row + visible_height).min(lines.len());
                            let number_width = end_row.max(1).to_string().len().max(2) as u16;
                            let content_width =
                                inner.width.saturating_sub(number_width + 1).max(1) as usize;
                            let relative_row = mouse.row.saturating_sub(inner.y) as usize;
                            let clicked_col =
                                mouse.column.saturating_sub(inner.x + number_width + 1) as usize;
                            let lines_ref: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
                            let (target_row, target_col) = textarea_click_position(
                                &lines_ref,
                                start_row,
                                content_width,
                                relative_row,
                                clicked_col,
                            );
                            // Check for double-click (word selection)
                            let now = Instant::now();
                            let is_double_click = self
                                .commit_rename_textarea_click_at
                                .map(|prev| now.duration_since(prev) <= Duration::from_millis(400))
                                .unwrap_or(false);
                            // Always position cursor first
                            dialog.message_editor.cancel_selection();
                            dialog
                                .message_editor
                                .move_cursor(tui_textarea::CursorMove::Jump(
                                    target_row as u16,
                                    target_col as u16,
                                ));
                            if is_double_click {
                                // Double-click: select word at cursor position
                                // Move to word start, then select to word end
                                dialog
                                    .message_editor
                                    .move_cursor(tui_textarea::CursorMove::WordBack);
                                dialog.message_editor.start_selection();
                                dialog
                                    .message_editor
                                    .move_cursor(tui_textarea::CursorMove::WordForward);
                                // WordForward goes to start of next word, so go back one char
                                dialog
                                    .message_editor
                                    .move_cursor(tui_textarea::CursorMove::Back);
                            }
                            self.commit_rename_textarea_click_at = Some(now);
                            self.commit_rename_textarea_rect = Some(rect);
                        }
                    }
                    return;
                }
                MouseEventKind::Drag(MouseButton::Left) => {
                    if let Some((action, rect)) =
                        self.resolve_hit_target(mouse.column, mouse.row, false)
                    {
                        if matches!(action, HitAction::CommitRenameMessageField) {
                            // Handle drag selection for textarea
                            if let Some(dialog) = &mut self.commit_rename_dialog {
                                let inner = Rect {
                                    x: rect.x + 1,
                                    y: rect.y + 1,
                                    width: rect.width.saturating_sub(2),
                                    height: rect.height.saturating_sub(2),
                                };
                                let lines = dialog.message_editor.lines();
                                let (cursor_row, _) = dialog.message_editor.cursor();
                                let visible_height = inner.height.max(1) as usize;
                                let start_row = cursor_row
                                    .saturating_sub(visible_height / 2)
                                    .min(lines.len().saturating_sub(visible_height));
                                let end_row = (start_row + visible_height).min(lines.len());
                                let number_width = end_row.max(1).to_string().len().max(2) as u16;
                                let content_width =
                                    inner.width.saturating_sub(number_width + 1).max(1) as usize;
                                let relative_row = mouse.row.saturating_sub(inner.y) as usize;
                                let clicked_col =
                                    mouse.column.saturating_sub(inner.x + number_width + 1)
                                        as usize;
                                let lines_ref: Vec<&str> =
                                    lines.iter().map(|s| s.as_str()).collect();
                                let (target_row, target_col) = textarea_click_position(
                                    &lines_ref,
                                    start_row,
                                    content_width,
                                    relative_row,
                                    clicked_col,
                                );
                                // Ensure selection is active and extend to new position
                                if !dialog.message_editor.is_selecting() {
                                    dialog.message_editor.start_selection();
                                }
                                dialog
                                    .message_editor
                                    .move_cursor(tui_textarea::CursorMove::Jump(
                                        target_row as u16,
                                        target_col as u16,
                                    ));
                            }
                        } else if let Some(last_target) = self.last_text_input_click_target
                            && last_target.same_field_action(&action)
                        {
                            self.update_text_input_drag_selection(rect, mouse.column);
                        }
                    }
                    return;
                }
                MouseEventKind::Down(MouseButton::Right) => {
                    let selected_text = self
                        .active_text_input_mut()
                        .and_then(|input| input.selected_text().map(str::to_string));
                    if let Some(selection) = selected_text {
                        self.copy_text_to_clipboard(&selection);
                        return;
                    }
                    let action = self.resolve_hit_action(mouse.column, mouse.row, true);
                    if action.is_none() && self.active_text_input_mut().is_some() {
                        self.paste_from_clipboard();
                        return;
                    }
                    if let Some(action) = action {
                        if let Err(error) = self.handle_hit_action(action) {
                            self.status = StatusMessage::error(error.to_string());
                        }
                    } else {
                        self.paste_from_clipboard();
                    }
                    return;
                }
                MouseEventKind::ScrollUp
                | MouseEventKind::ScrollDown
                | MouseEventKind::Up(MouseButton::Left) => return,
                _ => return,
            }
        }

        if self.overview_bump_kind_dialog.is_some() {
            match mouse.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    if let Some(action) = self.resolve_hit_action(mouse.column, mouse.row, false)
                        && let Err(error) = self.handle_hit_action(action)
                    {
                        self.status = StatusMessage::error(error.to_string());
                    }
                    return;
                }
                MouseEventKind::ScrollUp => {
                    self.rotate_overview_bump_kind(-1);
                    return;
                }
                MouseEventKind::ScrollDown => {
                    self.rotate_overview_bump_kind(1);
                    return;
                }
                MouseEventKind::Down(MouseButton::Right)
                | MouseEventKind::Drag(MouseButton::Left)
                | MouseEventKind::Up(MouseButton::Left) => return,
                _ => return,
            }
        }

        if self.recent_changes_dialog.is_some()
            && self.commit_rename_dialog.is_none()
            && self.tag_dialog.is_none()
            && self.tag_annotation_dialog.is_none()
        {
            match mouse.kind {
                MouseEventKind::ScrollUp
                | MouseEventKind::ScrollDown
                | MouseEventKind::ScrollLeft
                | MouseEventKind::ScrollRight => {
                    if self.try_handle_tab_wheel(&mouse) {
                        return;
                    }
                    match mouse.kind {
                        MouseEventKind::ScrollUp | MouseEventKind::ScrollLeft => {
                            self.scroll_recent_changes(-2);
                        }
                        MouseEventKind::ScrollDown | MouseEventKind::ScrollRight => {
                            self.scroll_recent_changes(2);
                        }
                        _ => {}
                    }
                    return;
                }
                MouseEventKind::Down(MouseButton::Left) => {
                    if let Some(action) = self.resolve_hit_action(mouse.column, mouse.row, false)
                        && let Err(error) = self.handle_hit_action(action)
                    {
                        self.status = StatusMessage::error(error.to_string());
                    }
                    return;
                }
                MouseEventKind::Down(MouseButton::Right)
                | MouseEventKind::Drag(MouseButton::Left)
                | MouseEventKind::Up(MouseButton::Left) => return,
                _ => return,
            }
        }

        if self.browser_dialog.is_some() {
            match mouse.kind {
                MouseEventKind::ScrollUp => {
                    self.move_browser_selection(-1);
                    return;
                }
                MouseEventKind::ScrollDown => {
                    self.move_browser_selection(1);
                    return;
                }
                MouseEventKind::Down(MouseButton::Left) => {
                    if let Some(action) = self.resolve_hit_action(mouse.column, mouse.row, false)
                        && let Err(error) = self.handle_hit_action(action)
                    {
                        self.status = StatusMessage::error(error.to_string());
                    }
                    return;
                }
                MouseEventKind::Down(MouseButton::Right)
                | MouseEventKind::Drag(MouseButton::Left)
                | MouseEventKind::Up(MouseButton::Left) => return,
                _ => {}
            }
        }

        if self.project_edit_dialog.is_some() {
            match mouse.kind {
                MouseEventKind::ScrollUp => {
                    self.scroll_project_edit_body(-1);
                    return;
                }
                MouseEventKind::ScrollDown => {
                    self.scroll_project_edit_body(1);
                    return;
                }
                MouseEventKind::Down(MouseButton::Left) => {
                    if let Some((action, rect)) =
                        self.resolve_hit_target(mouse.column, mouse.row, false)
                    {
                        let maybe_click_target = self.text_input_click_target(&action);
                        let mut select_all = false;
                        if let Some(target) = maybe_click_target {
                            let now = Instant::now();
                            if self.last_text_input_click_target == Some(target)
                                && self
                                    .last_text_input_click_at
                                    .map(|previous| {
                                        now.duration_since(previous) <= Duration::from_millis(400)
                                    })
                                    .unwrap_or(false)
                            {
                                select_all = true;
                            }
                            self.last_text_input_click_target = Some(target);
                            self.last_text_input_click_at = Some(now);
                        } else {
                            self.last_text_input_click_target = None;
                            self.last_text_input_click_at = None;
                        }

                        if let Err(error) = self.handle_hit_action(action) {
                            self.status = StatusMessage::error(error.to_string());
                        }

                        if select_all {
                            if let Some(input) = self.active_text_input_mut() {
                                input.select_all();
                            }
                        } else if maybe_click_target.is_some() && rect.width > FORM_LABEL_WIDTH + 2
                        {
                            self.set_text_input_cursor_from_mouse(rect, mouse.column);
                        }
                    }
                    return;
                }
                MouseEventKind::Drag(MouseButton::Left) => {
                    if let Some((action, rect)) =
                        self.resolve_hit_target(mouse.column, mouse.row, false)
                        && let Some(last_target) = self.last_text_input_click_target
                        && last_target.same_field_action(&action)
                    {
                        self.update_text_input_drag_selection(rect, mouse.column);
                    }
                    return;
                }
                MouseEventKind::Down(MouseButton::Right) => {
                    let selected_text = self
                        .active_text_input_mut()
                        .and_then(|input| input.selected_text().map(str::to_string));
                    if let Some(selection) = selected_text {
                        self.copy_text_to_clipboard(&selection);
                    }
                    return;
                }
                MouseEventKind::Up(MouseButton::Left) => return,
                _ => return,
            }
        }

        match mouse.kind {
            MouseEventKind::ScrollUp
            | MouseEventKind::ScrollDown
            | MouseEventKind::ScrollLeft
            | MouseEventKind::ScrollRight => {
                if self.try_handle_tab_wheel(&mouse) {
                    return;
                }
                let scroll_up = matches!(
                    mouse.kind,
                    MouseEventKind::ScrollUp | MouseEventKind::ScrollLeft
                );
                if scroll_up {
                    if self.project_edit_dialog.is_some() {
                        self.scroll_project_edit_body(-1);
                    } else if self.changelog_preview_dialog.is_some() {
                        self.scroll_changelog_preview(-2);
                    } else if self.overview_bump_workflow_dialog.is_some()
                        || self.tag_dialog.is_some()
                    {
                    } else if self.recent_changes_dialog.is_some() {
                        self.scroll_recent_changes(-2);
                    } else if self.bump_dialog.is_some() {
                        self.rotate_bump_action(-1);
                    } else if self.screen == Screen::Wizard {
                        self.scroll_wizard_body(-1);
                    } else if self.screen == Screen::Dashboard
                        && self.overview_tab == OverviewTab::ProjectSettings
                    {
                        self.scroll_project_settings(-1);
                    } else if self.screen == Screen::Dashboard
                        && self.overview_tab == OverviewTab::Overview
                    {
                        if self
                            .overview_recent_viewport
                            .map(|viewport| rect_contains(viewport, mouse.column, mouse.row))
                            .unwrap_or(false)
                        {
                            if let Some(dialog) = &mut self.overview_recent_changes {
                                dialog.scroll_by(-2);
                            } else {
                                self.move_project_selection(-1);
                            }
                        } else if self
                            .overview_tile_viewport
                            .map(|viewport| rect_contains(viewport, mouse.column, mouse.row))
                            .unwrap_or(false)
                        {
                            if let Err(error) = self.scroll_dashboard_tiles(-1) {
                                self.status = StatusMessage::error(error.to_string());
                            }
                        } else if let Some(dialog) = &mut self.overview_recent_changes {
                            dialog.scroll_by(-2);
                        } else {
                            self.move_project_selection(-1);
                        }
                    } else if self.screen == Screen::Dashboard
                        && self.overview_tab == OverviewTab::RecentChanges
                    {
                        if let Some(dialog) = &mut self.overview_recent_changes {
                            dialog.scroll_by(-2);
                        }
                    } else if self.screen == Screen::Dashboard {
                        self.move_project_selection(-1);
                    }
                } else if self.project_edit_dialog.is_some() {
                    self.scroll_project_edit_body(1);
                } else if self.changelog_preview_dialog.is_some() {
                    self.scroll_changelog_preview(2);
                } else if self.overview_bump_workflow_dialog.is_some() || self.tag_dialog.is_some()
                {
                } else if self.recent_changes_dialog.is_some() {
                    self.scroll_recent_changes(2);
                } else if self.bump_dialog.is_some() {
                    self.rotate_bump_action(1);
                } else if self.screen == Screen::Wizard {
                    self.scroll_wizard_body(1);
                } else if self.screen == Screen::Dashboard
                    && self.overview_tab == OverviewTab::ProjectSettings
                {
                    self.scroll_project_settings(1);
                } else if self.screen == Screen::Dashboard
                    && self.overview_tab == OverviewTab::Overview
                {
                    if self
                        .overview_recent_viewport
                        .map(|viewport| rect_contains(viewport, mouse.column, mouse.row))
                        .unwrap_or(false)
                    {
                        if let Some(dialog) = &mut self.overview_recent_changes {
                            dialog.scroll_by(2);
                        } else {
                            self.move_project_selection(1);
                        }
                    } else if self
                        .overview_tile_viewport
                        .map(|viewport| rect_contains(viewport, mouse.column, mouse.row))
                        .unwrap_or(false)
                    {
                        if let Err(error) = self.scroll_dashboard_tiles(1) {
                            self.status = StatusMessage::error(error.to_string());
                        }
                    } else if let Some(dialog) = &mut self.overview_recent_changes {
                        dialog.scroll_by(2);
                    } else {
                        self.move_project_selection(1);
                    }
                } else if self.screen == Screen::Dashboard
                    && self.overview_tab == OverviewTab::RecentChanges
                {
                    if let Some(dialog) = &mut self.overview_recent_changes {
                        dialog.scroll_by(2);
                    }
                } else if self.screen == Screen::Dashboard {
                    self.move_project_selection(1);
                }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                if self.screen == Screen::Dashboard
                    && self.overview_tab == OverviewTab::ProjectSettings
                    && self.try_start_project_settings_tab_drag(&mouse)
                {
                    return;
                }

                if self.overview_bump_workflow_dialog.is_none()
                    && self.screen == Screen::Dashboard
                    && self.overview_tab == OverviewTab::Overview
                {
                    self.overview_drag_scope =
                        self.overview_tile_rects
                            .iter()
                            .rev()
                            .find_map(|(rect, scope)| {
                                if mouse.column >= rect.x
                                    && mouse.column < rect.x + rect.width
                                    && mouse.row >= rect.y
                                    && mouse.row < rect.y + rect.height
                                {
                                    Some(*scope)
                                } else {
                                    None
                                }
                            });
                    if let Some(scope_index) = self.overview_drag_scope
                        && let Err(error) = self.select_dashboard_overview_scope(scope_index)
                    {
                        self.status = StatusMessage::error(error.to_string());
                    }
                }

                // Handle project drag start in Projects pane
                if self.screen == Screen::Dashboard
                    && self.dashboard_focus == DashboardPane::Projects
                {
                    self.drag_project =
                        self.project_rects.iter().rev().find_map(|(rect, index)| {
                            if mouse.column >= rect.x
                                && mouse.column < rect.x + rect.width
                                && mouse.row >= rect.y
                                && mouse.row < rect.y + rect.height
                            {
                                Some(*index)
                            } else {
                                None
                            }
                        });
                    if let Some(project_index) = self.drag_project {
                        self.selected_project = project_index;
                    }
                }

                if let Some((action, rect)) =
                    self.resolve_hit_target(mouse.column, mouse.row, false)
                {
                    let maybe_click_target = self.text_input_click_target(&action);
                    let maybe_recent_change_target = self.recent_change_click_target(&action);
                    let mut select_all = false;
                    let mut open_commit_rename = None;
                    if let Some(target) = maybe_click_target {
                        let now = Instant::now();
                        if self.last_text_input_click_target == Some(target)
                            && self
                                .last_text_input_click_at
                                .map(|previous| {
                                    now.duration_since(previous) <= Duration::from_millis(400)
                                })
                                .unwrap_or(false)
                        {
                            select_all = true;
                        }
                        self.last_text_input_click_target = Some(target);
                        self.last_text_input_click_at = Some(now);
                    } else {
                        self.last_text_input_click_target = None;
                        self.last_text_input_click_at = None;
                    }

                    if let Some(target) = maybe_recent_change_target {
                        let now = Instant::now();
                        if self.last_recent_change_click_target == Some(target)
                            && self
                                .last_recent_change_click_at
                                .map(|previous| {
                                    now.duration_since(previous) <= Duration::from_millis(400)
                                })
                                .unwrap_or(false)
                        {
                            open_commit_rename = Some(target.view);
                        }
                        self.last_recent_change_click_target = Some(target);
                        self.last_recent_change_click_at = Some(now);
                    } else {
                        self.last_recent_change_click_target = None;
                        self.last_recent_change_click_at = None;
                    }

                    // Handle commit rename textarea double-click for select all
                    if matches!(action, HitAction::CommitRenameMessageField) {
                        let now = Instant::now();
                        if self
                            .commit_rename_textarea_click_at
                            .map(|previous| {
                                now.duration_since(previous) <= Duration::from_millis(400)
                            })
                            .unwrap_or(false)
                        {
                            // Double-click: select all text in textarea
                            if let Some(dialog) = &mut self.commit_rename_dialog {
                                dialog.message_editor.select_all();
                            }
                        } else {
                            // Single click: track for potential double-click
                            self.commit_rename_textarea_click_at = Some(now);
                            self.commit_rename_textarea_rect = Some(rect);
                            // Position cursor in textarea based on click
                            if let Some(dialog) = &mut self.commit_rename_dialog {
                                let inner = Rect {
                                    x: rect.x + 1,
                                    y: rect.y + 1,
                                    width: rect.width.saturating_sub(2),
                                    height: rect.height.saturating_sub(2),
                                };
                                // Calculate number width like render_textarea_editor does
                                let lines = dialog.message_editor.lines();
                                let (cursor_row, _) = dialog.message_editor.cursor();
                                let visible_height = inner.height.max(1) as usize;
                                let end_row = (cursor_row + visible_height).min(lines.len()).max(1);
                                let number_width = end_row.to_string().len().max(2) as u16;
                                // Calculate row considering scroll offset
                                let start_row = cursor_row
                                    .saturating_sub(visible_height / 2)
                                    .min(lines.len().saturating_sub(visible_height));
                                let clicked_row =
                                    mouse.row.saturating_sub(inner.y) as usize + start_row;
                                // Calculate col accounting for line number column
                                let clicked_col =
                                    mouse.column.saturating_sub(inner.x + number_width + 1)
                                        as usize;
                                self.status = StatusMessage::info(format!(
                                    "Click: mouse({}, {}) inner({}, {}) start({}) -> row({}) col({})",
                                    mouse.column,
                                    mouse.row,
                                    inner.x,
                                    inner.y,
                                    start_row,
                                    clicked_row,
                                    clicked_col
                                ));
                                dialog
                                    .message_editor
                                    .move_cursor(tui_textarea::CursorMove::Jump(
                                        clicked_row as u16,
                                        clicked_col as u16,
                                    ));
                            }
                        }
                    } else {
                        self.commit_rename_textarea_click_at = None;
                        self.commit_rename_textarea_rect = None;
                    }

                    if let Err(error) = self.handle_hit_action(action) {
                        self.status = StatusMessage::error(error.to_string());
                    }

                    if select_all {
                        if let Some(input) = self.active_text_input_mut() {
                            input.select_all();
                        }
                    } else if maybe_click_target.is_some() {
                        self.set_text_input_cursor_from_mouse(rect, mouse.column);
                    }

                    if let Some(view) = open_commit_rename
                        && let Err(error) = self.open_commit_rename_from_view(view)
                    {
                        self.status = StatusMessage::error(error.to_string());
                    }
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if self.screen == Screen::Dashboard
                    && self.overview_tab == OverviewTab::ProjectSettings
                {
                    self.update_project_settings_tab_drag(&mouse);
                    return;
                }

                if let Some(from_scope) = self.overview_drag_scope {
                    let target_scope =
                        self.overview_tile_rects
                            .iter()
                            .rev()
                            .find_map(|(rect, scope)| {
                                (mouse.column >= rect.x
                                    && mouse.column < rect.x + rect.width
                                    && mouse.row >= rect.y
                                    && mouse.row < rect.y + rect.height)
                                    .then_some(*scope)
                            });
                    if let Some(to_scope) = target_scope
                        && to_scope != from_scope
                    {
                        self.reorder_dashboard_tile_scope(from_scope, to_scope);
                        self.overview_drag_scope = Some(to_scope);
                    }
                }

                // Handle project drag reordering
                if let Some(from_project) = self.drag_project {
                    let target_project =
                        self.project_rects.iter().rev().find_map(|(rect, index)| {
                            (mouse.column >= rect.x
                                && mouse.column < rect.x + rect.width
                                && mouse.row >= rect.y
                                && mouse.row < rect.y + rect.height)
                                .then_some(*index)
                        });
                    if let Some(to_project) = target_project
                        && to_project != from_project
                    {
                        self.reorder_projects(from_project, to_project);
                        self.drag_project = Some(to_project);
                    }
                }

                if let Some((action, rect)) =
                    self.resolve_hit_target(mouse.column, mouse.row, false)
                    && let Some(last_target) = self.last_text_input_click_target
                    && last_target.same_field_action(&action)
                {
                    self.update_text_input_drag_selection(rect, mouse.column);
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                if self.screen == Screen::Dashboard
                    && self.overview_tab == OverviewTab::ProjectSettings
                {
                    self.finish_project_settings_tab_drag(&mouse);
                }
                self.overview_drag_scope = None;
                self.drag_project = None;
            }
            MouseEventKind::Down(MouseButton::Right) => {
                if self.overview_bump_workflow_dialog.is_none()
                    && self.screen == Screen::Dashboard
                    && self.overview_tab == OverviewTab::Overview
                    && let Some(scope_index) =
                        self.overview_tile_rects
                            .iter()
                            .rev()
                            .find_map(|(rect, scope)| {
                                (mouse.column >= rect.x
                                    && mouse.column < rect.x + rect.width
                                    && mouse.row >= rect.y
                                    && mouse.row < rect.y + rect.height)
                                    .then_some(*scope)
                            })
                    && let Err(error) = self.select_dashboard_overview_scope(scope_index)
                {
                    self.status = StatusMessage::error(error.to_string());
                }
                let selected_text = self
                    .active_text_input_mut()
                    .and_then(|input| input.selected_text().map(str::to_string));
                if let Some(selection) = selected_text {
                    self.copy_text_to_clipboard(&selection);
                    return;
                }

                let action = self.resolve_hit_action(mouse.column, mouse.row, true);
                if action.is_none() && self.active_text_input_mut().is_some() {
                    self.paste_from_clipboard();
                    return;
                }

                if let Some(action) = action {
                    if let Err(error) = self.handle_hit_action(action) {
                        self.status = StatusMessage::error(error.to_string());
                    }
                } else {
                    self.paste_from_clipboard();
                }
            }
            _ => {}
        }
    }

    pub(crate) fn resolve_hit_target(
        &self,
        column: u16,
        row: u16,
        right_click: bool,
    ) -> Option<(HitAction, Rect)> {
        self.hit_targets
            .iter()
            .enumerate()
            .filter_map(|(index, target)| {
                if !target.contains(column, row) {
                    return None;
                }

                let action = if right_click {
                    target.right_action.clone()
                } else {
                    Some(target.action.clone())
                }?;

                if self.browser_dialog.is_some() {
                    if !matches!(action, HitAction::BrowserSelect(_)) {
                        return None;
                    }
                    return Some((
                        target.rect.width as u32 * target.rect.height as u32,
                        usize::MAX - index,
                        action,
                        target.rect,
                    ));
                }

                if self.delete_confirmation_dialog.is_some()
                    && !matches!(
                        action,
                        HitAction::ConfirmDeleteRequest | HitAction::CancelDeleteRequest
                    )
                {
                    return None;
                }

                if self.overview_bump_kind_dialog.is_some()
                    && !matches!(
                        action,
                        HitAction::SelectOverviewBumpKind(_)
                            | HitAction::ConfirmOverviewBumpKind
                            | HitAction::CancelOverviewBumpKind
                    )
                {
                    return None;
                }

                if self.recent_changes_dialog.is_some()
                    && self.commit_rename_dialog.is_none()
                    && self.tag_dialog.is_none()
                    && self.tag_annotation_dialog.is_none()
                    && !matches!(
                        action,
                        HitAction::SelectRecentChangesTab(_)
                            | HitAction::CycleRecentChangesScope(_)
                            | HitAction::CloseRecentChanges
                            | HitAction::ScrollRecentChanges(_)
                            | HitAction::SelectRecentChangeLine(_, _)
                            | HitAction::OpenTagDialog
                    )
                {
                    return None;
                }

                if self.commit_rename_dialog.is_some()
                    && !matches!(
                        action,
                        HitAction::ToggleCommitRenameForcePush
                            | HitAction::SaveCommitRename
                            | HitAction::CancelCommitRename
                            | HitAction::CommitRenameMessageField
                    )
                {
                    return None;
                }

                if self.release_now_notes_dialog.is_some()
                    && !matches!(
                        action,
                        HitAction::SaveReleaseNowNotes
                            | HitAction::CancelReleaseNowNotes
                            | HitAction::ReleaseNowNotesField
                    )
                {
                    return None;
                }

                if self.top_picks_editor_dialog.is_some()
                    && !matches!(
                        action,
                        HitAction::SaveTopPicks
                            | HitAction::CancelTopPicks
                            | HitAction::TopPicksEditorField
                    )
                {
                    return None;
                }

                if self.release_now_dialog.is_some()
                    && self.release_now_notes_dialog.is_none()
                    && !matches!(
                        action,
                        HitAction::CycleReleaseNowOption(_)
                            | HitAction::ToggleReleaseNowChangelog
                            | HitAction::EditReleaseNowNotes
                            | HitAction::RunReleaseNow
                            | HitAction::ContinueReleaseNowWarning
                            | HitAction::RunReleaseNowMirrorSync
                            | HitAction::RefreshReleaseNowMirrorSync
                            | HitAction::SelectReleaseNowArtifactsChoice(_)
                            | HitAction::ContinueReleaseNowArtifactsCustomize
                            | HitAction::BackReleaseNowArtifactsCustomize
                            | HitAction::ToggleReleaseNowAutoFollow
                            | HitAction::CancelReleaseNowRun
                            | HitAction::ScrollReleaseNow(_)
                            | HitAction::CloseReleaseNow
                    )
                {
                    return None;
                }

                if self.std_changelog_sub_branch_dialog.is_some()
                    && !matches!(action, HitAction::SelectStdChangelogSubBranchChoice(_))
                {
                    return None;
                }

                if self.project_edit_dialog.is_some()
                    && !matches!(
                        action,
                        HitAction::EditProjectField(_)
                            | HitAction::FocusProjectEditDialog
                            | HitAction::ProjectEditScopeAction(_)
                            | HitAction::SaveProjectEdit
                            | HitAction::RemoveProject
                            | HitAction::CancelProjectEdit
                            | HitAction::BrowseProjectTargetPath
                            | HitAction::BrowseProjectRepoRoot
                            | HitAction::EnableProjectCustomTargetKey
                    )
                {
                    return None;
                }

                Some((
                    target.rect.width as u32 * target.rect.height as u32,
                    usize::MAX - index,
                    action,
                    target.rect,
                ))
            })
            .min_by_key(|(area, reverse_index, _, _)| (*area, *reverse_index))
            .map(|(_, _, action, rect)| (action, rect))
    }

    pub(crate) fn resolve_hit_action(
        &self,
        column: u16,
        row: u16,
        right_click: bool,
    ) -> Option<HitAction> {
        self.resolve_hit_target(column, row, right_click)
            .map(|(action, _)| action)
    }

    pub(crate) fn text_input_click_target(
        &self,
        action: &HitAction,
    ) -> Option<TextInputClickTarget> {
        Some(match action {
            HitAction::WizardField(field) => TextInputClickTarget::Wizard(*field),
            HitAction::EditProjectField(field) => TextInputClickTarget::ProjectEdit(*field),
            HitAction::SelectProjectSettingsField(field) => {
                TextInputClickTarget::ProjectSettings(*field)
            }
            HitAction::CommitRenameMessageField => TextInputClickTarget::CommitRenameMessage,
            _ => return None,
        })
    }

    pub(crate) fn recent_change_click_target(
        &self,
        action: &HitAction,
    ) -> Option<RecentChangeClickTarget> {
        match action {
            HitAction::SelectRecentChangeLine(view, line_index) => Some(RecentChangeClickTarget {
                view: *view,
                line_index: *line_index,
            }),
            _ => None,
        }
    }

    pub(crate) fn active_text_input_mut(&mut self) -> Option<&mut TextInput> {
        if matches!(self.screen, Screen::Wizard) {
            return self.wizard.active_input_mut();
        }

        if let Some(dialog) = &mut self.project_edit_dialog {
            return dialog.active_input_mut();
        }

        if self.screen == Screen::Dashboard && self.overview_tab == OverviewTab::ProjectSettings {
            return self.project_settings_state.active_input_mut();
        }

        // Commit rename and tag annotation use TuiTextArea directly, not through active_text_input_mut
        None
    }

    pub(crate) fn set_text_input_cursor_from_mouse(&mut self, rect: Rect, column: u16) {
        let is_commit_rename = self.commit_rename_dialog.is_some();
        let is_project_edit = self.project_edit_dialog.is_some();
        let is_project_settings = self.screen == Screen::Dashboard
            && self.overview_tab == OverviewTab::ProjectSettings
            && !is_project_edit;
        if let Some(input) = self.active_text_input_mut() {
            let (click_offset, field_width) = if is_commit_rename || is_project_edit {
                // Commit rename has borders but no label in the rect
                let border_offset = column.saturating_sub(rect.x + 1) as usize;
                let width = rect.width.saturating_sub(2) as usize;
                (border_offset, width)
            } else if is_project_settings
                && rect.width > FORM_LABEL_WIDTH.saturating_add(6)
                && column >= rect.x.saturating_add(FORM_LABEL_WIDTH)
            {
                let label_offset = column.saturating_sub(rect.x + FORM_LABEL_WIDTH) as usize;
                let width = rect.width.saturating_sub(FORM_LABEL_WIDTH) as usize;
                (label_offset, width)
            } else {
                let border_offset = column.saturating_sub(rect.x + 1) as usize;
                let width = rect.width.saturating_sub(2) as usize;
                (border_offset, width)
            };
            let cursor = input.cursor_position_at_click(click_offset, field_width, true);
            input.set_cursor_position(cursor);
            input.clear_selection();
        }
    }

    pub(crate) fn update_text_input_drag_selection(&mut self, rect: Rect, column: u16) {
        let is_commit_rename = self.commit_rename_dialog.is_some();
        let is_project_edit = self.project_edit_dialog.is_some();
        let is_project_settings = self.screen == Screen::Dashboard
            && self.overview_tab == OverviewTab::ProjectSettings
            && !is_project_edit;
        if let Some(input) = self.active_text_input_mut() {
            let (click_offset, field_width) = if is_commit_rename || is_project_edit {
                let border_offset = column.saturating_sub(rect.x + 1) as usize;
                let width = rect.width.saturating_sub(2) as usize;
                (border_offset, width)
            } else if is_project_settings
                && rect.width > FORM_LABEL_WIDTH.saturating_add(6)
                && column >= rect.x.saturating_add(FORM_LABEL_WIDTH)
            {
                let label_offset = column.saturating_sub(rect.x + FORM_LABEL_WIDTH) as usize;
                let width = rect.width.saturating_sub(FORM_LABEL_WIDTH) as usize;
                (label_offset, width)
            } else {
                let border_offset = column.saturating_sub(rect.x + 1) as usize;
                let width = rect.width.saturating_sub(2) as usize;
                (border_offset, width)
            };
            let cursor = input.cursor_position_at_click(click_offset, field_width, true);
            if input.selection_anchor().is_none() {
                input.begin_selection_at(cursor);
            }
            input.set_cursor_position(cursor);
        }
    }

    pub(crate) fn handle_toast_mouse(&mut self, mouse: MouseEvent) -> bool {
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let interaction =
                    self.toaster
                        .handle_click(mouse.column, mouse.row, ToastMouseButton::Left);
                self.handle_toast_interaction(interaction)
            }
            MouseEventKind::Down(MouseButton::Right) => {
                let interaction =
                    self.toaster
                        .handle_click(mouse.column, mouse.row, ToastMouseButton::Right);
                self.handle_toast_interaction(interaction)
            }
            _ => false,
        }
    }
}
