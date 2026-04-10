use ratatui::{
	Frame,
	layout::{Constraint, Direction, Layout, Rect},
	style::{Color, Style},
};
use tui_tabs::TabNav;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum OverviewTab {
	Overview,
	ProjectDetail,
}

pub(crate) fn render_overview_tabs(frame: &mut Frame, area: Rect, active_tab: OverviewTab) {
	let labels = ["Overview", "Project Detail"];
	let active_index = match active_tab {
		OverviewTab::Overview => 0,
		OverviewTab::ProjectDetail => 1,
	};
	let tabs = TabNav::new(&labels, active_index)
		.highlight_style(Style::default().fg(Color::Cyan))
		.border_style(Style::default().fg(Color::DarkGray))
		.style(Style::default().fg(Color::White))
		.indicator(None);
	frame.render_widget(tabs, area);
}

pub(crate) fn overview_tab_rects(area: Rect) -> [(OverviewTab, Rect); 2] {
	let layout = Layout::default()
		.direction(Direction::Horizontal)
		.constraints([Constraint::Length(16), Constraint::Length(22)])
		.split(area);
	[
		(OverviewTab::Overview, layout[0]),
		(OverviewTab::ProjectDetail, layout[1]),
	]
}
