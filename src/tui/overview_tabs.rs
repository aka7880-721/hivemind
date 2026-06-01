// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit License v1.2
//
// For details, see the LICENSE file in the repository root.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
};
use ratatui_comfy_tabs::{TabNav, TabNavState, TabReorderPolicy, TabWheelDirection};
use ratatui::widgets::StatefulWidget;

use crate::config::UiSettings;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OverviewTab {
    Overview,
    RecentChanges,
    ProjectDetail,
    ProjectSettings,
}

const OVERVIEW_TABS_WITH_RECENT: [OverviewTab; 4] = [
    OverviewTab::Overview,
    OverviewTab::RecentChanges,
    OverviewTab::ProjectDetail,
    OverviewTab::ProjectSettings,
];

const OVERVIEW_TABS_WITHOUT_RECENT: [OverviewTab; 3] = [
    OverviewTab::Overview,
    OverviewTab::ProjectDetail,
    OverviewTab::ProjectSettings,
];

pub(crate) fn overview_tabs(include_recent_changes: bool) -> &'static [OverviewTab] {
    if include_recent_changes {
        &OVERVIEW_TABS_WITH_RECENT
    } else {
        &OVERVIEW_TABS_WITHOUT_RECENT
    }
}

pub(crate) fn project_settings_tab_nav<'a>(
    labels: &'a [&'a str],
    selected: usize,
    tab_pinned: &'a [bool],
    ui: &UiSettings,
) -> TabNav<'a> {
    apply_tab_selection_flash(
        comfy_tab_nav(labels, selected)
            .reorder_policy(TabReorderPolicy::SomePinned)
            .tab_pinned(tab_pinned)
            .mouse_reorder(true),
        ui,
    )
}

pub(crate) fn apply_tab_selection_flash<'a>(nav: TabNav<'a>, ui: &UiSettings) -> TabNav<'a> {
    nav.selection_flash(ui.tab_selection_flash_enabled).selection_flash_style(
        Style::default().fg(Color::Indexed(ui.tab_selection_flash_color)),
    )
}

pub(crate) fn sync_tab_nav_flash_state(state: &mut TabNavState, ui: &UiSettings) {
    state.selection_flash_enabled = ui.tab_selection_flash_enabled;
}

pub(crate) fn comfy_tab_nav<'a>(labels: &'a [&'a str], selected: usize) -> TabNav<'a> {
    // mouse_wheel / mouse_click are opt-in at the widget level; the app must still
    // forward wheel/click events to TabNavState::handle_mouse_wheel / handle_mouse_click.
    TabNav::new(labels, selected)
        .highlight_style(Style::default().fg(Color::Cyan))
        .border_style(Style::default().fg(Color::DarkGray))
        .style(Style::default().fg(Color::White))
        .indicator(None)
        .mouse_wheel(true)
        .mouse_click(true)
}

/// Apply one wheel step over `strip_area`. Returns the new selected index when consumed.
pub(crate) fn wheel_tab_strip(
    strip_area: Rect,
    labels: &[&str],
    active_index: usize,
    mouse_column: u16,
    mouse_row: u16,
    direction: TabWheelDirection,
) -> Option<usize> {
    let nav = comfy_tab_nav(labels, active_index);
    let mut state = TabNavState::new(active_index);
    if !state.handle_mouse_wheel(&nav, strip_area, mouse_column, mouse_row, direction) {
        return None;
    }
    Some(state.selected)
}

pub(crate) fn render_overview_tabs(
    frame: &mut Frame,
    area: Rect,
    active_tab: OverviewTab,
    include_recent_changes: bool,
    ui: &UiSettings,
    state: &mut TabNavState,
) {
    let labels = overview_tab_specs(include_recent_changes)
        .iter()
        .map(|(_, label)| *label)
        .collect::<Vec<_>>();
    let active_index = overview_tab_specs(include_recent_changes)
        .iter()
        .position(|(tab, _)| *tab == active_tab)
        .unwrap_or(0);
    state.selected = active_index;
    sync_tab_nav_flash_state(state, ui);
    let nav = apply_tab_selection_flash(comfy_tab_nav(&labels, active_index), ui);
    StatefulWidget::render(nav, area, frame.buffer_mut(), state);
}

pub(crate) fn overview_tab_rects(
    area: Rect,
    include_recent_changes: bool,
) -> Vec<(OverviewTab, Rect)> {
    let specs = overview_tab_specs(include_recent_changes);
    let labels: Vec<&str> = specs.iter().map(|(_, label)| *label).collect();
    let nav = comfy_tab_nav(&labels, 0);
    let rects = nav.tab_rects(area);
    specs
        .iter()
        .zip(rects)
        .map(|((tab, _), rect)| (*tab, rect))
        .collect()
}

/// Returns the tab selected by a wheel step when the pointer is over `strip_area`.
pub(crate) fn wheel_overview_tab(
    strip_area: Rect,
    include_recent_changes: bool,
    active_tab: OverviewTab,
    mouse_column: u16,
    mouse_row: u16,
    direction: TabWheelDirection,
) -> Option<OverviewTab> {
    let specs = overview_tab_specs(include_recent_changes);
    let labels: Vec<&str> = specs.iter().map(|(_, label)| *label).collect();
    let active_index = specs
        .iter()
        .position(|(tab, _)| *tab == active_tab)
        .unwrap_or(0);
    let selected = wheel_tab_strip(
        strip_area,
        &labels,
        active_index,
        mouse_column,
        mouse_row,
        direction,
    )?;
    specs.get(selected).map(|(tab, _)| *tab)
}

pub(crate) fn overview_tab_index(tab: OverviewTab, include_recent_changes: bool) -> usize {
    overview_tab_specs(include_recent_changes)
        .iter()
        .position(|(candidate, _)| *candidate == tab)
        .unwrap_or(0)
}

fn overview_tab_specs(include_recent_changes: bool) -> &'static [(OverviewTab, &'static str)] {
    if include_recent_changes {
        &[
            (OverviewTab::Overview, "Overview"),
            (OverviewTab::RecentChanges, "Recent Changes"),
            (OverviewTab::ProjectDetail, "Project Detail"),
            (OverviewTab::ProjectSettings, "Project Settings"),
        ]
    } else {
        &[
            (OverviewTab::Overview, "Overview"),
            (OverviewTab::ProjectDetail, "Project Detail"),
            (OverviewTab::ProjectSettings, "Project Settings"),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overview_tab_rects_include_recent_tab_when_requested() {
        let rects = overview_tab_rects(Rect::new(0, 0, 120, 3), true);
        assert_eq!(rects.len(), 4);
        assert_eq!(rects[1].0, OverviewTab::RecentChanges);
        assert_eq!(rects[3].0, OverviewTab::ProjectSettings);
    }

    #[test]
    fn wheel_tab_strip_advances_selection_when_hovering() {
        let area = Rect::new(0, 0, 120, 3);
        let labels = ["Overview", "Detail"];
        let selected = wheel_tab_strip(area, &labels, 0, 5, 1, TabWheelDirection::Down);
        assert_eq!(selected, Some(1));
    }
}
