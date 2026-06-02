// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit SA-PS License

use super::super::*;
use crate::app::project_settings;

use crossterm::event::{self, Event, MouseEvent, MouseEventKind};
use ratatui_comfy_tabs::{TabOrientation, TabWheelDirection};
use std::time::{Duration, Instant};

use crate::{
    tui::{OverviewTab, wheel_overview_tab, wheel_tab_strip},
    workflow::dialogs::RecentChangesTab,
};

impl App {
    /// Track pointer position for wheel hit-testing (scroll events often report 0,0).
    pub(crate) fn update_mouse_position(&mut self, mouse: &MouseEvent) {
        match mouse.kind {
            MouseEventKind::Moved
            | MouseEventKind::Down(_)
            | MouseEventKind::Up(_)
            | MouseEventKind::Drag(_) => {
                self.last_mouse_column = mouse.column;
                self.last_mouse_row = mouse.row;
                self.has_mouse_position = true;
            }
            MouseEventKind::ScrollUp
            | MouseEventKind::ScrollDown
            | MouseEventKind::ScrollLeft
            | MouseEventKind::ScrollRight => {
                if mouse.column != 0 || mouse.row != 0 {
                    self.last_mouse_column = mouse.column;
                    self.last_mouse_row = mouse.row;
                    self.has_mouse_position = true;
                }
            }
        }
    }

    fn mouse_position_for_wheel(&self, mouse: &MouseEvent) -> (u16, u16) {
        if mouse.column != 0 || mouse.row != 0 {
            (mouse.column, mouse.row)
        } else if self.has_mouse_position {
            (self.last_mouse_column, self.last_mouse_row)
        } else {
            (mouse.column, mouse.row)
        }
    }

    /// Cycles a visible tab strip when the pointer is over it. Returns `true` when consumed.
    pub(crate) fn try_handle_tab_wheel(&mut self, mouse: &MouseEvent) -> bool {
        let Some(direction) = tab_wheel_direction(mouse) else {
            return false;
        };

        self.drain_matching_tab_wheel(direction);
        let (column, row) = self.mouse_position_for_wheel(mouse);

        if self.recent_changes_dialog.is_some()
            && self.commit_rename_dialog.is_none()
            && self.tag_dialog.is_none()
            && self.tag_annotation_dialog.is_none()
            && self.handle_recent_changes_tab_wheel(column, row, direction)
        {
            return true;
        }

        if self.screen == Screen::Dashboard {
            if self.overview_tab == OverviewTab::ProjectSettings
                && self.project_settings_tab_strip_area.is_some_and(|area| {
                    self.handle_project_settings_tab_wheel(column, row, area, direction)
                })
            {
                return true;
            }

            if self
                .overview_tab_strip_area
                .is_some_and(|area| self.handle_overview_tab_wheel(column, row, area, direction))
            {
                return true;
            }
        }

        false
    }

    fn handle_overview_tab_wheel(
        &mut self,
        column: u16,
        row: u16,
        strip_area: Rect,
        direction: TabWheelDirection,
    ) -> bool {
        let Some(tab) = wheel_overview_tab(
            strip_area,
            self.overview_show_recent_tab,
            self.overview_tab,
            column,
            row,
            direction,
        ) else {
            return false;
        };

        if tab != self.overview_tab {
            self.overview_tab = tab;
            crate::app::ui_settings::flash_overview_tab_selection(
                self,
                self.overview_show_recent_tab,
            );
            self.dashboard_focus = DashboardPane::Overview;
        }
        true
    }

    fn handle_project_settings_tab_wheel(
        &mut self,
        column: u16,
        row: u16,
        strip_area: Rect,
        direction: TabWheelDirection,
    ) -> bool {
        let Some(project) = self.config.projects.get(self.selected_project).cloned() else {
            return false;
        };
        let scope_index =
            project_settings::active_scope_index(&project, self.overview_focused_scope);
        let default_strip = project_settings::project_settings_tab_strip(
            &project,
            project.release_now_for_scope(scope_index).enabled,
        );
        let strip = project_settings::merge_project_settings_tab_order(
            &default_strip,
            &self.project_settings_tab_order,
        );
        let labels: Vec<&str> = strip
            .iter()
            .map(|tab| project_settings::project_settings_tab_label(*tab))
            .collect();
        let active_index = strip
            .iter()
            .position(|tab| *tab == self.project_settings_tab)
            .unwrap_or(0);
        let Some(selected) =
            wheel_tab_strip(strip_area, &labels, active_index, column, row, direction)
        else {
            return false;
        };

        let Some(tab) = strip.get(selected).copied() else {
            return false;
        };
        if tab != self.project_settings_tab {
            self.project_settings_tab = tab;
            self.flash_project_settings_tab_selection();
            project_settings::sync_project_settings_state(self);
        }
        true
    }

    fn handle_recent_changes_tab_wheel(
        &mut self,
        column: u16,
        row: u16,
        direction: TabWheelDirection,
    ) -> bool {
        let Some(strip_area) = self.recent_changes_tab_strip_area else {
            return false;
        };

        let tab_labels = ["Recent Changes", "History"];
        let active_index = if self
            .recent_changes_dialog
            .as_ref()
            .is_some_and(|dialog| dialog.active_tab == RecentChangesTab::History)
        {
            1
        } else {
            0
        };
        let Some(selected) = wheel_tab_strip(
            strip_area,
            &tab_labels,
            active_index,
            column,
            row,
            direction,
        ) else {
            return false;
        };

        let tab = if selected == 0 {
            RecentChangesTab::Recent
        } else {
            RecentChangesTab::History
        };
        if self
            .recent_changes_dialog
            .as_ref()
            .is_some_and(|dialog| dialog.active_tab == tab)
        {
            return true;
        }

        if let Err(error) = self.handle_hit_action(HitAction::SelectRecentChangesTab(tab)) {
            self.status = StatusMessage::error(error.to_string());
        }
        true
    }

    fn drain_matching_tab_wheel(&mut self, direction: TabWheelDirection) {
        let deadline = Instant::now() + Duration::from_millis(30);
        while Instant::now() < deadline {
            if !event::poll(Duration::from_millis(0)).unwrap_or(false) {
                break;
            }
            let Ok(Event::Mouse(mouse)) = event::read() else {
                break;
            };
            self.update_mouse_position(&mouse);
            if tab_wheel_direction(&mouse) != Some(direction) {
                break;
            }
        }
    }
}

fn tab_wheel_direction(mouse: &MouseEvent) -> Option<TabWheelDirection> {
    let vertical = match mouse.kind {
        MouseEventKind::ScrollUp => Some(TabWheelDirection::Up),
        MouseEventKind::ScrollDown => Some(TabWheelDirection::Down),
        _ => None,
    };
    let horizontal = match mouse.kind {
        MouseEventKind::ScrollLeft => Some(TabWheelDirection::Up),
        MouseEventKind::ScrollRight => Some(TabWheelDirection::Down),
        _ => None,
    };
    TabWheelDirection::from_axes(vertical, horizontal, TabOrientation::Horizontal)
}
