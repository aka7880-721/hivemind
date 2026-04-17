// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit License v1.2
//
// For details, see the LICENSE file in the repository root.

use ratatui::{
	Frame,
	layout::{Alignment, Rect},
	style::{Color, Style},
	text::{Line, Span},
	widgets::Paragraph,
};

use crate::versioning::VersionScheme;

pub(crate) const TILE_WIDTH: u16 = 34;
pub(crate) const SEMVER_TILE_HEIGHT: u16 = 9;
pub(crate) const CALVER_TILE_HEIGHT: u16 = 9;
const SEMVER_LEFT_WIDTH: usize = 5;
const CALVER_ACTION_WIDTH: usize = 6;
const VIEW_BUTTON_STYLE: Style = Style::new().fg(Color::Black).bg(Color::LightMagenta);
const BUMP_BUTTON_STYLE: Style = Style::new().fg(Color::Black).bg(Color::Green);
const TAG_BUTTON_STYLE: Style = Style::new().fg(Color::White).bg(Color::Yellow);

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
	pub(crate) reset_rect: Option<Rect>,
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

pub(crate) fn render_overview_tile(frame: &mut Frame, area: Rect, tile: &OverviewTileData) -> OverviewTileHotspots {
	if tile.scheme == VersionScheme::SemVer {
		render_semver_tile(frame, area, tile)
	} else {
		render_calver_tile(frame, area, tile)
	}
}

fn render_semver_tile(frame: &mut Frame, area: Rect, tile: &OverviewTileData) -> OverviewTileHotspots {
	let border_style = border_style(tile.selected);
	let tile_style = tile_style(tile.selected);
	let content_width = area.width.saturating_sub(2) as usize;
	let right_width = content_width.saturating_sub(SEMVER_LEFT_WIDTH + 1);
	let parts = split_semver(&tile.preview_version);
	let button_slots = space_evenly_positions(right_width, &[6, 6, 5]);
	let button_positions = [button_slots[0] + 1, button_slots[1] + 1, button_slots[2] + 1];
	let button_line = build_button_line(right_width, &button_positions, &["rls", "bump", "tag"]);

	let rows = [
		border_top_semver(right_width),
		format!("║{:^5}│{:^right_width$}║", " ver.", tile.name, right_width = right_width),
		format!("║{}│{}║", dot_fill(SEMVER_LEFT_WIDTH), dot_fill(right_width)),
		format!("║{:^5}│{}║", parts[0], format_activity_detail("   tag..→HEAD", &tile.commits_since_tag_label, 8, right_width)),
		format!("║{:^5}│{}║", "·", format_activity_detail("   last bump", &tile.last_bump_label, 9, right_width)),
		format!("║{:^5}│{}║", parts[1], format_activity_detail("   last commit", &tile.last_commit_label, 7, right_width)),
		format!("║{:^5}├{}╢", "·", "─".repeat(right_width)),
		format!("║{:^5}│{}║", parts[2], button_line),
		border_bottom_semver(right_width),
	];
	render_rows(frame, area, &rows, border_style, tile_style);
	render_highlighted_row(
		frame,
		Rect::new(area.x, area.y + 7, area.width, 1),
		&rows[7],
		border_style,
		tile_style,
		&[
			StyledRange::new(7 + button_slots[0], 6, VIEW_BUTTON_STYLE),
			StyledRange::new(7 + button_slots[1], 6, BUMP_BUTTON_STYLE),
			StyledRange::new(7 + button_slots[2], 5, TAG_BUTTON_STYLE),
		],
	);

	let right_x = area.x + 1 + SEMVER_LEFT_WIDTH as u16 + 1;
	let inner_y = area.y + 1;
	OverviewTileHotspots {
		tile_rect: area,
		title_rect: Rect::new(right_x, inner_y, right_width as u16, 1),
		reset_rect: Some(Rect::new(area.x + 1, inner_y, SEMVER_LEFT_WIDTH as u16, 1)),
		major_rect: Some(Rect::new(area.x + 1, inner_y + 2, SEMVER_LEFT_WIDTH as u16, 1)),
		minor_rect: Some(Rect::new(area.x + 1, inner_y + 4, SEMVER_LEFT_WIDTH as u16, 1)),
		patch_rect: Some(Rect::new(area.x + 1, inner_y + 6, SEMVER_LEFT_WIDTH as u16, 1)),
		version_rect: None,
		view_rect: Rect::new(right_x + button_slots[0] as u16, inner_y + 6, 6, 1),
		bump_rect: Rect::new(right_x + button_slots[1] as u16, inner_y + 6, 6, 1),
		tag_rect: Rect::new(right_x + button_slots[2] as u16, inner_y + 6, 5, 1),
	}
}

fn render_calver_tile(frame: &mut Frame, area: Rect, tile: &OverviewTileData) -> OverviewTileHotspots {
	let border_style = border_style(tile.selected);
	let tile_style = tile_style(tile.selected);
	let content_width = area.width.saturating_sub(2) as usize;
	let detail_width = content_width.saturating_sub(CALVER_ACTION_WIDTH + 1);

	let rows = [
		format!("╔{}╗", "═".repeat(content_width)),
		format!("║{:^content_width$}║", tile.name, content_width = content_width),
		format!("║{}║", dot_fill(content_width)),
		format!("║{}│{:^action_width$}║", format_activity_detail(" tag..→HEAD", &tile.commits_since_tag_label, 8, detail_width), "bump", action_width = CALVER_ACTION_WIDTH),
		format!("║{}│{:^action_width$}║", format_activity_detail(" last bump", &tile.last_bump_label, 9, detail_width), "rls", action_width = CALVER_ACTION_WIDTH),
		format!("║{}│{:^action_width$}║", format_activity_detail(" last commit", &tile.last_commit_label, 7, detail_width), "tag", action_width = CALVER_ACTION_WIDTH),
		format!("╟{}┴{}╢", "─".repeat(detail_width), "─".repeat(CALVER_ACTION_WIDTH)),
		format!("║{:^content_width$}║", spaced_version(&tile.preview_version), content_width = content_width),
		format!("╚{}╝", "═".repeat(content_width)),
	];
	render_rows(frame, area, &rows, border_style, tile_style);
	for (row_offset, label) in [(3_u16, "bump"), (4, "rls"), (5, "tag")].into_iter() {
		let action_start = 1 + detail_width + 1;
		let action_style = match label {
			"rls" => VIEW_BUTTON_STYLE,
			"bump" => BUMP_BUTTON_STYLE,
			_ => TAG_BUTTON_STYLE,
		};
		render_highlighted_row(
			frame,
			Rect::new(area.x, area.y + row_offset, area.width, 1),
			&rows[row_offset as usize],
			border_style,
			tile_style,
			&[StyledRange::new(action_start, CALVER_ACTION_WIDTH, action_style)],
		);
	}

	let action_x = area.x + 1 + detail_width as u16 + 1;
	let inner_y = area.y + 1;
	OverviewTileHotspots {
		tile_rect: area,
		title_rect: Rect::new(area.x + 1, inner_y, content_width as u16, 1),
		reset_rect: None,
		major_rect: None,
		minor_rect: None,
		patch_rect: None,
		version_rect: Some(Rect::new(area.x + 1, inner_y + 6, content_width as u16, 1)),
		view_rect: Rect::new(action_x, inner_y + 3, CALVER_ACTION_WIDTH as u16, 1),
		bump_rect: Rect::new(action_x, inner_y + 2, CALVER_ACTION_WIDTH as u16, 1),
		tag_rect: Rect::new(action_x, inner_y + 4, CALVER_ACTION_WIDTH as u16, 1),
	}
}

fn render_rows(frame: &mut Frame, area: Rect, rows: &[String], border_style: Style, tile_style: Style) {
	for (index, row) in rows.iter().enumerate() {
		if index as u16 >= area.height {
			break;
		}
		if index == 7 {
			continue;
		}
		let row_rect = Rect::new(area.x, area.y + index as u16, area.width, 1);
		frame.render_widget(
			Paragraph::new(styled_row(row, border_style, tile_style))
				.style(tile_style)
				.alignment(Alignment::Left),
			row_rect,
		);
	}
}

fn render_highlighted_row(
	frame: &mut Frame,
	area: Rect,
	row: &str,
	border_style: Style,
	tile_style: Style,
	highlights: &[StyledRange],
) {
	frame.render_widget(
		Paragraph::new(styled_row_with_highlights(row, border_style, tile_style, highlights))
			.style(tile_style)
			.alignment(Alignment::Left),
		area,
	);
}

fn styled_row(row: &str, border_style: Style, tile_style: Style) -> Line<'static> {
	styled_row_with_highlights(row, border_style, tile_style, &[])
}

fn styled_row_with_highlights(
	row: &str,
	border_style: Style,
	tile_style: Style,
	highlights: &[StyledRange],
) -> Line<'static> {
	let border_chars = ['╔', '╗', '╚', '╝', '║', '═', '│', '╤', '╧', '╟', '╢', '├', '┴', '─'];
	let spans = row
		.chars()
		.enumerate()
		.map(|(index, character)| {
			let style = highlights
				.iter()
				.find(|highlight| highlight.contains(index))
				.map(|highlight| highlight.style)
				.unwrap_or_else(|| {
					if border_chars.contains(&character) || character == '·' {
						border_style
					} else {
						tile_style.fg(Color::White)
					}
				});
			Span::styled(character.to_string(), style)
		})
		.collect::<Vec<_>>();
	Line::from(spans)
}

fn tile_style(selected: bool) -> Style {
	if selected {
		Style::default().bg(Color::Black)
	} else {
		Style::default()
	}
}

fn border_style(selected: bool) -> Style {
	if selected {
		Style::default().fg(Color::Cyan).bg(Color::Black)
	} else {
		Style::default().fg(Color::DarkGray)
	}
}

fn border_top_semver(right_width: usize) -> String {
	format!("╔{}╤{}╗", "═".repeat(SEMVER_LEFT_WIDTH), "═".repeat(right_width))
}

fn border_bottom_semver(right_width: usize) -> String {
	format!("╚{}╧{}╝", "═".repeat(SEMVER_LEFT_WIDTH), "═".repeat(right_width))
}

fn format_activity_detail(label: &str, value: &str, value_width: usize, total_width: usize) -> String {
	let raw = format!("{}: {:>value_width$}", label, value, value_width = value_width);
	fit_to_width(&raw, total_width)
}

fn fit_to_width(value: &str, width: usize) -> String {
	let rendered = value.chars().take(width).collect::<String>();
	if rendered.len() >= width {
		rendered
	} else {
		format!("{rendered:<width$}")
	}
}

fn dot_fill(width: usize) -> String {
	"·".repeat(width)
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

fn space_evenly_positions(width: usize, item_widths: &[usize]) -> Vec<usize> {
	let count = item_widths.len();
	if count == 0 {
		return Vec::new();
	}

	item_widths
		.iter()
		.enumerate()
		.map(|(index, item_width)| {
			let center = ((2 * index + 1) * width) / (2 * count);
			center
				.saturating_sub(item_width / 2)
				.min(width.saturating_sub(*item_width))
		})
		.collect()
}

#[derive(Clone, Copy)]
struct StyledRange {
	start: usize,
	len: usize,
	style: Style,
}

impl StyledRange {
	fn new(start: usize, len: usize, style: Style) -> Self {
		Self { start, len, style }
	}

	fn contains(&self, index: usize) -> bool {
		index >= self.start && index < self.start + self.len
	}
}

fn build_button_line(width: usize, positions: &[usize], labels: &[&str]) -> String {
	let mut cells = vec![' '; width];
	for (position, label) in positions.iter().zip(labels.iter()) {
		for (offset, character) in label.chars().enumerate() {
			if let Some(cell) = cells.get_mut(position + offset) {
				*cell = character;
			}
		}
	}
	cells.into_iter().collect()
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
		assert_eq!(format_activity_detail("last bump", "5d ago", 9, 20), "last bump:    5d ago");
	}

	#[test]
	fn button_positions_are_spread_evenly() {
		assert_eq!(space_evenly_positions(26, &[6, 6, 5]), vec![1, 10, 19]);
	}
}
