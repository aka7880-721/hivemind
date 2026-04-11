use ratatui::{
	Frame,
	layout::{Constraint, Direction, Layout, Rect},
	style::{Color, Style},
};
use tui_tabs::TabNav;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OverviewTab {
	Overview,
	RecentChanges,
	ProjectDetail,
}


pub(crate) fn render_overview_tabs(frame: &mut Frame, area: Rect, active_tab: OverviewTab, include_recent_changes: bool) {
	let labels = overview_tab_specs(include_recent_changes)
		.iter()
		.map(|(_, label, _)| *label)
		.collect::<Vec<_>>();
	let active_index = overview_tab_specs(include_recent_changes)
		.iter()
		.position(|(tab, _, _)| *tab == active_tab)
		.unwrap_or(0);
	let tabs = TabNav::new(&labels, active_index)
		.highlight_style(Style::default().fg(Color::Cyan))
		.border_style(Style::default().fg(Color::DarkGray))
		.style(Style::default().fg(Color::White))
		.indicator(None);
	frame.render_widget(tabs, area);
}


pub(crate) fn overview_tab_rects(area: Rect, include_recent_changes: bool) -> Vec<(OverviewTab, Rect)> {
	let specs = overview_tab_specs(include_recent_changes);
	let layout = Layout::default()
		.direction(Direction::Horizontal)
		.constraints(specs.iter().map(|(_, _, width)| Constraint::Length(*width)).collect::<Vec<_>>())
		.split(area);
	specs
		.iter()
		.enumerate()
		.map(|(index, (tab, _, _))| (*tab, layout[index]))
		.collect()
}

fn overview_tab_specs(include_recent_changes: bool) -> &'static [(OverviewTab, &'static str, u16)] {
	if include_recent_changes {
		&[
			(OverviewTab::Overview, "Overview", 16),
			(OverviewTab::RecentChanges, "Recent Changes", 22),
			(OverviewTab::ProjectDetail, "Project Detail", 22),
		]
	} else {
		&[
			(OverviewTab::Overview, "Overview", 16),
			(OverviewTab::ProjectDetail, "Project Detail", 22),
		]
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn overview_tab_rects_include_recent_tab_when_requested() {
		let rects = overview_tab_rects(Rect::new(0, 0, 80, 3), true);
		assert_eq!(rects.len(), 3);
		assert_eq!(rects[1].0, OverviewTab::RecentChanges);
	}
}
