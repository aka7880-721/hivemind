use ratatui::{
	Frame,
	layout::{Alignment, Constraint, Direction, Layout, Rect},
	style::{Color, Modifier, Style},
	text::{Line, Span},
	widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::{config::BranchScopeKind, versioning::VersionScheme};

pub(crate) const TILE_WIDTH: u16 = 32;
pub(crate) const SEMVER_TILE_HEIGHT: u16 = 8;
pub(crate) const CALVER_TILE_HEIGHT: u16 = 7;

pub(crate) struct OverviewTileData {
	pub(crate) name: String,
	pub(crate) scope_kind: Option<BranchScopeKind>,
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

	let sections = Layout::default()
		.direction(Direction::Vertical)
		.constraints([
			Constraint::Length(2),
			Constraint::Length(3),
			Constraint::Length(1),
			Constraint::Length(1),
		])
		.split(inner);
	let body = Layout::default()
		.direction(Direction::Horizontal)
		.constraints([Constraint::Length(5), Constraint::Min(12)])
		.split(sections[1]);
	let parts = split_semver(&tile.preview_version);
	let version_rows = Layout::default()
		.direction(Direction::Vertical)
		.constraints([Constraint::Length(1), Constraint::Length(1), Constraint::Length(1)])
		.split(body[0]);
	let detail_rows = Layout::default()
		.direction(Direction::Vertical)
		.constraints([Constraint::Length(1), Constraint::Length(1), Constraint::Length(1)])
		.split(body[1]);
	let button_row = Layout::default()
		.direction(Direction::Horizontal)
		.constraints([Constraint::Fill(1), Constraint::Length(8), Constraint::Length(8), Constraint::Length(8), Constraint::Fill(1)])
		.split(sections[3]);

	let title_lines = vec![
		Line::from(tile.name.clone()).alignment(Alignment::Center),
		Line::from(tile.scope_kind.map(|kind| kind.display_name()).unwrap_or("Project")).style(Style::default().fg(Color::Gray)).alignment(Alignment::Center),
	];
	frame.render_widget(Paragraph::new(title_lines).alignment(Alignment::Center), sections[0]);

	for (row, value) in version_rows.iter().zip(parts.iter()) {
		frame.render_widget(
			Paragraph::new(Line::from(Span::styled(
				value.clone(),
				Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
			)))
			.alignment(Alignment::Center),
			*row,
		);
	}

	let details = [
		format!("tag..→HEAD: {}", tile.commits_since_tag_label),
		format!("last bump: {}", tile.last_bump_label),
		format!("last commit: {}", tile.last_commit_label),
	];
	for (row, detail) in detail_rows.iter().zip(details.iter()) {
		frame.render_widget(
			Paragraph::new(detail.as_str()).wrap(Wrap { trim: false }),
			*row,
		);
	}

	let view_rect = button_row[1];
	let bump_rect = button_row[2];
	let tag_rect = button_row[3];
	render_tile_button(frame, view_rect, "view", Color::Rgb(70, 110, 150));
	render_tile_button(frame, bump_rect, "bump", Color::Rgb(120, 170, 80));
	render_tile_button(frame, tag_rect, "tag", Color::Rgb(170, 140, 70));

	OverviewTileHotspots {
		tile_rect: area,
		title_rect: sections[0],
		major_rect: Some(version_rows[0]),
		minor_rect: Some(version_rows[1]),
		patch_rect: Some(version_rows[2]),
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

	let sections = Layout::default()
		.direction(Direction::Vertical)
		.constraints([
			Constraint::Length(2),
			Constraint::Length(3),
			Constraint::Length(1),
		])
		.split(inner);
	let middle = Layout::default()
		.direction(Direction::Horizontal)
		.constraints([Constraint::Min(12), Constraint::Length(8)])
		.split(sections[1]);
	let detail_rows = Layout::default()
		.direction(Direction::Vertical)
		.constraints([Constraint::Length(1), Constraint::Length(1), Constraint::Length(1)])
		.split(middle[0]);
	let action_rows = Layout::default()
		.direction(Direction::Vertical)
		.constraints([Constraint::Length(1), Constraint::Length(1), Constraint::Length(1)])
		.split(middle[1]);

	frame.render_widget(
		Paragraph::new(vec![
			Line::from(tile.name.clone()).alignment(Alignment::Center),
			Line::from(tile.scope_kind.map(|kind| kind.display_name()).unwrap_or("Project")).style(Style::default().fg(Color::Gray)).alignment(Alignment::Center),
		])
		.alignment(Alignment::Center),
		sections[0],
	);

	let details = [
		format!("tag..→HEAD: {}", tile.commits_since_tag_label),
		format!("last bump: {}", tile.last_bump_label),
		format!("last commit: {}", tile.last_commit_label),
	];
	for (row, detail) in detail_rows.iter().zip(details.iter()) {
		frame.render_widget(Paragraph::new(detail.as_str()), *row);
	}

	render_tile_button(frame, action_rows[0], "bump", Color::Rgb(120, 170, 80));
	render_tile_button(frame, action_rows[1], "view", Color::Rgb(70, 110, 150));
	render_tile_button(frame, action_rows[2], "tag", Color::Rgb(170, 140, 70));

	let version_rect = sections[2];
	frame.render_widget(
		Paragraph::new(spaced_version(&tile.preview_version))
			.alignment(Alignment::Center)
			.style(Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
		version_rect,
	);

	OverviewTileHotspots {
		tile_rect: area,
		title_rect: sections[0],
		major_rect: None,
		minor_rect: None,
		patch_rect: None,
		version_rect: Some(version_rect),
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
