use ratatui::{
	Frame,
	layout::{Alignment, Constraint, Direction, Flex, Layout, Rect},
	style::{Color, Modifier, Style},
	text::{Line, Span},
	widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::versioning::VersionScheme;

pub(crate) const TILE_WIDTH: u16 = 34;
pub(crate) const SEMVER_TILE_HEIGHT: u16 = 9;
pub(crate) const CALVER_TILE_HEIGHT: u16 = 9;

pub(crate) struct OverviewTileData {
	pub(crate) name: String,
	pub(crate) scheme: VersionScheme,
	pub(crate) preview_version: String,
	pub(crate) commits_since_tag_label: String,
	pub(crate) last_bump_label: String,
	pub(crate) last_commit_label: String,
	pub(crate) selected: bool,
}

#[derive(Clone, Default)]
pub(crate) struct OverviewTileHotspots {
	pub(crate) tile_rect: Rect,
	pub(crate) title_rect: Rect,
	pub(crate) major_rect: Option<Rect>,
	pub(crate) minor_rect: Option<Rect>,
	pub(crate) patch_rect: Option<Rect>,
	pub(crate) version_rect: Option<Rect>,
	pub(crate) view_rect: Rect,
	pub(crate) bump_rect: Rect,
	pub(crate) tag_rect: Rect,
}

pub(crate) fn tile_height(scheme: VersionScheme) -> u16 {
	if scheme == VersionScheme::SemVer {
		SEMVER_TILE_HEIGHT
	} else {
		CALVER_TILE_HEIGHT
	}
}

pub(crate) fn render_overview_tile(
	frame: &mut Frame,
	area: Rect,
	tile: &OverviewTileData,
) -> OverviewTileHotspots {
	if tile.scheme == VersionScheme::SemVer {
		render_semver_tile(frame, area, tile)
	} else {
		render_calver_tile(frame, area, tile)
	}
}

fn render_semver_tile(frame: &mut Frame, area: Rect, tile: &OverviewTileData) -> OverviewTileHotspots {
	let border_style = if tile.selected {
		Style::default().fg(Color::Cyan)
	} else {
		Style::default().fg(Color::DarkGray)
	};
	let block = Block::default().borders(Borders::ALL).border_style(border_style);
	let inner = block.inner(area);
	frame.render_widget(block, area);

	let columns = Layout::default()
		.direction(Direction::Horizontal)
		.constraints([Constraint::Length(5), Constraint::Min(12)])
		.split(inner);
	let left_column = columns[0];
	let right_column = columns[1];
	frame.render_widget(Block::default().borders(Borders::RIGHT).border_style(border_style), left_column);

	let left_rows = Layout::default()
		.direction(Direction::Vertical)
		.constraints([Constraint::Length(1); 7])
		.split(left_column);
	let right_rows = Layout::default()
		.direction(Direction::Vertical)
		.constraints([Constraint::Length(1); 7])
		.split(right_column);

	frame.render_widget(Paragraph::new("ver.").alignment(Alignment::Center), left_rows[0]);
	frame.render_widget(Paragraph::new(tile.name.as_str()).alignment(Alignment::Center), right_rows[0]);
	frame.render_widget(
		Paragraph::new(dot_fill(left_rows[1].width)).alignment(Alignment::Center).style(Style::default().fg(Color::DarkGray)),
		left_rows[1],
	);
	frame.render_widget(
		Paragraph::new(dot_fill(right_rows[1].width)).alignment(Alignment::Center).style(Style::default().fg(Color::DarkGray)),
		right_rows[1],
	);

	let parts = split_semver(&tile.preview_version);
	for (row, value) in [left_rows[2], left_rows[4], left_rows[6]].iter().zip(parts.iter()) {
		frame.render_widget(
			Paragraph::new(Line::from(Span::styled(
				value.clone(),
				Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
			)))
			.alignment(Alignment::Center),
			*row,
		);
	}
	frame.render_widget(
		Paragraph::new("  ·  ").alignment(Alignment::Center).style(Style::default().fg(Color::DarkGray)),
		left_rows[3],
	);
	frame.render_widget(
		Paragraph::new("  ·  ").alignment(Alignment::Center).style(Style::default().fg(Color::DarkGray)),
		left_rows[5],
	);

	let details = [
		format_activity_detail(" tag..→HEAD", &tile.commits_since_tag_label, 8),
		format_activity_detail(" last bump", &tile.last_bump_label, 9),
		format_activity_detail(" last commit", &tile.last_commit_label, 7),
	];
	for (row, detail) in [right_rows[2], right_rows[3], right_rows[4]].iter().zip(details.iter()) {
		frame.render_widget(Paragraph::new(detail.as_str()).wrap(Wrap { trim: false }), *row);
	}

	let action_block = Block::default().borders(Borders::TOP).border_style(border_style);
	let action_inner = action_block.inner(right_rows[6]);
	frame.render_widget(action_block, right_rows[6]);
	let button_row = Layout::default()
		.direction(Direction::Horizontal)
		.constraints([Constraint::Length(6), Constraint::Length(6), Constraint::Length(6)])
		.flex(Flex::SpaceEvenly)
		.split(action_inner);

	let view_rect = button_row[0];
	let bump_rect = button_row[1];
	let tag_rect = button_row[2];
	render_tile_button(frame, view_rect, "view", Color::Rgb(70, 110, 150));
	render_tile_button(frame, bump_rect, "bump", Color::Rgb(120, 170, 80));
	render_tile_button(frame, tag_rect, "tag", Color::Rgb(170, 140, 70));

	OverviewTileHotspots {
		tile_rect: area,
		title_rect: right_rows[0],
		major_rect: Some(left_rows[2]),
		minor_rect: Some(left_rows[4]),
		patch_rect: Some(left_rows[6]),
		version_rect: None,
		view_rect,
		bump_rect,
		tag_rect,
	}
}

fn render_calver_tile(frame: &mut Frame, area: Rect, tile: &OverviewTileData) -> OverviewTileHotspots {
	let border_style = if tile.selected {
		Style::default().fg(Color::Cyan)
	} else {
		Style::default().fg(Color::DarkGray)
	};
	let block = Block::default().borders(Borders::ALL).border_style(border_style);
	let inner = block.inner(area);
	frame.render_widget(block, area);

	let rows = Layout::default()
		.direction(Direction::Vertical)
		.constraints([Constraint::Length(1); 7])
		.split(inner);
	frame.render_widget(Paragraph::new(tile.name.as_str()).alignment(Alignment::Center), rows[0]);
	frame.render_widget(
		Paragraph::new(dot_fill(rows[1].width)).alignment(Alignment::Center).style(Style::default().fg(Color::DarkGray)),
		rows[1],
	);

	let middle = Layout::default()
		.direction(Direction::Horizontal)
		.constraints([Constraint::Min(12), Constraint::Length(7)])
		.split(Rect {
			x: inner.x,
			y: rows[2].y,
			width: inner.width,
			height: 3,
		});
	let detail_rows = Layout::default()
		.direction(Direction::Vertical)
		.constraints([Constraint::Length(1), Constraint::Length(1), Constraint::Length(1)])
		.split(middle[0]);
	let action_rows = Layout::default()
		.direction(Direction::Vertical)
		.constraints([Constraint::Length(1), Constraint::Length(1), Constraint::Length(1)])
		.split(middle[1]);
	frame.render_widget(Block::default().borders(Borders::LEFT).border_style(border_style), middle[1]);

	let details = [
		format_activity_detail("tag..→HEAD", &tile.commits_since_tag_label, 8),
		format_activity_detail("last bump", &tile.last_bump_label, 9),
		format_activity_detail("last commit", &tile.last_commit_label, 7),
	];
	for (row, detail) in detail_rows.iter().zip(details.iter()) {
		frame.render_widget(Paragraph::new(detail.as_str()), *row);
	}

	render_tile_button(frame, action_rows[0], "bump", Color::Rgb(120, 170, 80));
	render_tile_button(frame, action_rows[1], "view", Color::Rgb(70, 110, 150));
	render_tile_button(frame, action_rows[2], "tag", Color::Rgb(170, 140, 70));

	let version_block = Block::default().borders(Borders::TOP).border_style(border_style);
	let version_inner = version_block.inner(rows[6]);
	frame.render_widget(version_block, rows[6]);
	frame.render_widget(
		Paragraph::new(spaced_version(&tile.preview_version))
			.alignment(Alignment::Center)
			.style(Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
		version_inner,
	);

	OverviewTileHotspots {
		tile_rect: area,
		title_rect: rows[0],
		major_rect: None,
		minor_rect: None,
		patch_rect: None,
		version_rect: Some(version_inner),
		view_rect: action_rows[1],
		bump_rect: action_rows[0],
		tag_rect: action_rows[2],
	}
}

fn render_tile_button(frame: &mut Frame, area: Rect, label: &str, bg: Color) {
	frame.render_widget(
		Paragraph::new(format!(" {} ", label))
			.alignment(Alignment::Center)
			.style(Style::default().fg(Color::Black).bg(bg)),
		area,
	);
}

fn format_activity_detail(label: &str, value: &str, width: usize) -> String {
	format!("{}: {:>width$}", label, value, width = width)
}

fn dot_fill(width: u16) -> String {
	"·".repeat(width as usize)
}

fn split_semver(version: &str) -> [String; 3] {
	let parts = version.split('.').map(ToOwned::to_owned).collect::<Vec<_>>();
	[
		parts.get(0).cloned().unwrap_or_else(|| "?".to_string()),
		parts.get(1).cloned().unwrap_or_else(|| "?".to_string()),
		parts.get(2).cloned().unwrap_or_else(|| "?".to_string()),
	]
}

fn spaced_version(version: &str) -> String {
	version.split('.').collect::<Vec<_>>().join(" . ")
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn split_semver_pads_missing_parts() {
		assert_eq!(split_semver("1.2"), ["1".to_string(), "2".to_string(), "?".to_string()]);
	}

	#[test]
	fn dot_fill_matches_requested_width() {
		assert_eq!(dot_fill(5), "·····");
	}

	#[test]
	fn format_activity_detail_right_aligns_value() {
		assert_eq!(format_activity_detail("last bump", "5d ago", 9), "last bump:    5d ago");
	}
}
