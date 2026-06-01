//! Copyright © 2026 ComfyHome™
//! All rights reserved.
//!
//! Licensed under the ComfyGit SA-PS License
//!
//! For details, see the LICENSE file in the repository root.

use super::super::*;
use crate::app::project_settings::{
    self, ProjectSettingsTab, merge_project_settings_tab_order, project_settings_tab_label,
    project_settings_tab_pins, project_settings_tab_strip,
};
use crate::tui::{OverviewTab, project_settings_tab_nav};

use crossterm::event::MouseEvent;
use ratatui_comfy_tabs::{TabReorderPolicy, try_reorder};

impl App {
    pub(crate) fn try_start_project_settings_tab_drag(&mut self, mouse: &MouseEvent) -> bool {
        if self.screen != Screen::Dashboard || self.overview_tab != OverviewTab::ProjectSettings {
            return false;
        }
        let Some(area) = self.project_settings_tab_strip_area else {
            return false;
        };
        let Some((labels, pinned, active_index)) = self.project_settings_tab_nav_inputs() else {
            return false;
        };
        let nav = project_settings_tab_nav(&labels, active_index, &pinned, &self.config.ui);
        self.project_settings_tab_nav_state
            .handle_mouse_reorder_press(&nav, area, mouse.column, mouse.row)
    }

    pub(crate) fn update_project_settings_tab_drag(&mut self, mouse: &MouseEvent) {
        let Some(drag) = self.project_settings_tab_nav_state.reorder_drag else {
            return;
        };
        let from = drag.source;
        let Some(area) = self.project_settings_tab_strip_area else {
            return;
        };
        let Some((mut strip, labels, pinned, active_index)) =
            self.project_settings_tab_nav_pack()
        else {
            return;
        };
        let nav = project_settings_tab_nav(&labels, active_index, &pinned, &self.config.ui);
        let prev_hover = drag.hover;
        self.project_settings_tab_nav_state.handle_mouse_reorder_drag(
            &nav,
            area,
            mouse.column,
            mouse.row,
        );
        let Some(drag) = self.project_settings_tab_nav_state.reorder_drag else {
            return;
        };
        if drag.hover == prev_hover || drag.hover == from {
            return;
        }
        if try_reorder(
            &mut strip,
            from,
            drag.hover,
            TabReorderPolicy::SomePinned,
            Some(&pinned),
        ) {
            self.project_settings_tab_order = strip;
            self.project_settings_tab_nav_state.reorder_drag =
                Some(ratatui_comfy_tabs::TabReorderDrag {
                    source: drag.hover,
                    hover: drag.hover,
                    armed: true,
                });
        }
    }

    pub(crate) fn finish_project_settings_tab_drag(&mut self, mouse: &MouseEvent) {
        if !self.project_settings_tab_nav_state.is_reorder_dragging() {
            return;
        }
        let Some((mut strip, labels, pinned, active_index)) =
            self.project_settings_tab_nav_pack()
        else {
            self.project_settings_tab_nav_state.cancel_reorder_drag();
            return;
        };
        let nav = project_settings_tab_nav(&labels, active_index, &pinned, &self.config.ui);
        if let Some(reorder) = self
            .project_settings_tab_nav_state
            .handle_mouse_reorder_release(&nav)
        {
            let _ = try_reorder(
                &mut strip,
                reorder.from,
                reorder.to,
                TabReorderPolicy::SomePinned,
                Some(&pinned),
            );
            self.project_settings_tab_order = strip;
        } else {
            self.apply_project_settings_tab_click(mouse);
        }
    }

    fn apply_project_settings_tab_click(&mut self, mouse: &MouseEvent) {
        let Some(area) = self.project_settings_tab_strip_area else {
            return;
        };
        let Some((strip, labels, pinned, active_index)) = self.project_settings_tab_nav_pack()
        else {
            return;
        };
        let nav = project_settings_tab_nav(&labels, active_index, &pinned, &self.config.ui);
        if !self.project_settings_tab_nav_state.handle_mouse_click(
            &nav,
            area,
            mouse.column,
            mouse.row,
        ) {
            return;
        }
        let Some(tab) = strip
            .get(self.project_settings_tab_nav_state.selected)
            .copied()
        else {
            return;
        };
        if tab != self.project_settings_tab {
            self.project_settings_tab = tab;
            self.flash_project_settings_tab_selection();
            project_settings::sync_project_settings_state(self);
        }
    }

    fn project_settings_tab_nav_inputs(&self) -> Option<(Vec<&'static str>, Vec<bool>, usize)> {
        let (_, labels, pinned, active_index) = self.project_settings_tab_nav_pack()?;
        Some((labels, pinned, active_index))
    }

    pub(crate) fn flash_project_settings_tab_selection(&mut self) {
        if !self.config.ui.tab_selection_flash_enabled {
            return;
        }
        crate::tui::sync_tab_nav_flash_state(
            &mut self.project_settings_tab_nav_state,
            &self.config.ui,
        );
        let Some((strip, _, _, _)) = self.project_settings_tab_nav_pack() else {
            return;
        };
        let Some(index) = strip
            .iter()
            .position(|tab| *tab == self.project_settings_tab)
        else {
            return;
        };
        self.project_settings_tab_nav_state.flash_selection(index);
        self.project_settings_tab_nav_state.selected = index;
    }

    fn project_settings_tab_nav_pack(
        &self,
    ) -> Option<(Vec<ProjectSettingsTab>, Vec<&'static str>, Vec<bool>, usize)> {
        let project = self.config.projects.get(self.selected_project)?;
        let scope_index =
            project_settings::active_scope_index(&project, self.overview_focused_scope);
        let release_now = project.release_now_for_scope(scope_index).enabled;
        let default_strip = project_settings_tab_strip(&project, release_now);
        let strip = merge_project_settings_tab_order(&default_strip, &self.project_settings_tab_order);
        let labels: Vec<&str> = strip
            .iter()
            .map(|tab| project_settings_tab_label(*tab))
            .collect();
        let pinned = project_settings_tab_pins(&strip);
        let active_index = strip
            .iter()
            .position(|tab| *tab == self.project_settings_tab)
            .unwrap_or(0);
        Some((strip, labels, pinned, active_index))
    }
}
