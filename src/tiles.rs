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
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::versioning::VersionScheme;

pub(crate) const TILE_WIDTH: u16 = 34;
pub(crate) const SEMVER_TILE_HEIGHT: u16 = 9;
pub(crate) const CALVER_TILE_HEIGHT: u16 = 9;
const SEMVER_LEFT_WIDTH: usize = 5;
const CALVER_ACTION_WIDTH: usize = 6;
const RLS_BUTTON_STYLE: Style = Style::new().fg(Color::Black).bg(Color::Indexed(207));
const BUMP_BUTTON_STYLE: Style = Style::new().fg(Color::Black).bg(Color::Indexed(46));
const TAG_BUTTON_STYLE: Style = Style::new().fg(Color::Black).bg(Color::LightYellow);

pub(crate) struct OverviewTileData {
    pub(crate) name: String,
    pub(crate) scheme: VersionScheme,
    pub(crate) preview_version: String,
    pub(crate) commits_since_tag_label: String,
    pub(crate) dev_display: String,
    pub(crate) dev_output: String,
    pub(crate) rls_display: String,
    pub(crate) rls_output: String,
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
    pub(crate) dev_info_rect: Rect,
    pub(crate) rls_info_rect: Rect,
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

fn render_semver_tile(
    frame: &mut Frame,
    area: Rect,
    tile: &OverviewTileData,
) -> OverviewTileHotspots {
    let border_style = border_style(tile.selected);
    let tile_style = tile_style(tile.selected);
    let content_width = area.width.saturating_sub(2) as usize;
    let right_width = content_width.saturating_sub(SEMVER_LEFT_WIDTH + 1);
    let parts = split_semver(&tile.preview_version);
    let button_slots = space_evenly_positions(right_width, &[6, 6, 5]);
    let button_positions = [
        button_slots[0] + 1,
        button_slots[1] + 2,
        button_slots[2] + 2,
    ];
    let button_line = build_button_line(right_width, &button_positions, &["BUMP", "TAG", "RLS"]);

    let rows = [
        border_top_semver(right_width),
        format!(
            "║{:^5}│{:^right_width$}║",
            " ver.",
            tile.name,
            right_width = right_width
        ),
        format!(
            "║{}│{}║",
            dot_fill(SEMVER_LEFT_WIDTH),
            dot_fill(right_width)
        ),
        format!(
            "║{:^5}│{}║",
            parts[0],
            format_tile_tag_row("🏷️", &tile.commits_since_tag_label, right_width)
        ),
        format!(
            "║{:^5}│{}║",
            "·",
            format_tile_dev_info_row("🚧", &tile.dev_display, &tile.dev_output, right_width)
        ),
        format!(
            "║{:^5}│{}║",
            parts[1],
            format_tile_info_row("🌍", &tile.rls_display, &tile.rls_output, right_width)
        ),
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
            StyledRange::new(6 + button_positions[0], 6, BUMP_BUTTON_STYLE),
            StyledRange::new(6 + button_positions[1], 5, TAG_BUTTON_STYLE),
            StyledRange::new(6 + button_positions[2], 5, RLS_BUTTON_STYLE),
        ],
    );

    let right_x = area.x + 1 + SEMVER_LEFT_WIDTH as u16 + 1;
    let inner_y = area.y + 1;
    OverviewTileHotspots {
        tile_rect: area,
        title_rect: Rect::new(right_x, inner_y, right_width as u16, 1),
        reset_rect: Some(Rect::new(area.x + 1, inner_y, SEMVER_LEFT_WIDTH as u16, 1)),
        major_rect: Some(Rect::new(
            area.x + 1,
            inner_y + 2,
            SEMVER_LEFT_WIDTH as u16,
            1,
        )),
        minor_rect: Some(Rect::new(
            area.x + 1,
            inner_y + 4,
            SEMVER_LEFT_WIDTH as u16,
            1,
        )),
        patch_rect: Some(Rect::new(
            area.x + 1,
            inner_y + 6,
            SEMVER_LEFT_WIDTH as u16,
            1,
        )),
        version_rect: None,
        bump_rect: Rect::new(right_x + button_positions[0] as u16, inner_y + 6, 6, 1),
        tag_rect: Rect::new(right_x + button_positions[1] as u16, inner_y + 6, 6, 1),
        view_rect: Rect::new(right_x + button_positions[2] as u16, inner_y + 6, 5, 1),
        dev_info_rect: Rect::new(right_x, inner_y + 3, right_width as u16, 1),
        rls_info_rect: Rect::new(right_x, inner_y + 4, right_width as u16, 1),
    }
}

fn render_calver_tile(
    frame: &mut Frame,
    area: Rect,
    tile: &OverviewTileData,
) -> OverviewTileHotspots {
    let border_style = border_style(tile.selected);
    let tile_style = tile_style(tile.selected);
    let content_width = area.width.saturating_sub(2) as usize;
    let detail_width = content_width.saturating_sub(CALVER_ACTION_WIDTH + 1);

    let rows = [
        format!("╔{}╗", "═".repeat(content_width)),
        format!(
            "║{:^content_width$}║",
            tile.name,
            content_width = content_width
        ),
        format!("║{}║", dot_fill(content_width)),
        format!(
            "║{}│{:^action_width$}║",
            format_tile_tag_row("🏷️", &tile.commits_since_tag_label, detail_width),
            "bump",
            action_width = CALVER_ACTION_WIDTH
        ),
        format!(
            "║{}│{:^action_width$}║",
            format_tile_dev_info_row("🚧", &tile.dev_display, &tile.dev_output, detail_width),
            "rls",
            action_width = CALVER_ACTION_WIDTH
        ),
        format!(
            "║{}│{:^action_width$}║",
            format_tile_info_row("🌍", &tile.rls_display, &tile.rls_output, detail_width),
            "tag",
            action_width = CALVER_ACTION_WIDTH
        ),
        format!(
            "╟{}┴{}╢",
            "─".repeat(detail_width),
            "─".repeat(CALVER_ACTION_WIDTH)
        ),
        format!(
            "║{:^content_width$}║",
            spaced_version(&tile.preview_version),
            content_width = content_width
        ),
        format!("╚{}╝", "═".repeat(content_width)),
    ];
    render_rows(frame, area, &rows, border_style, tile_style);
    for (row_offset, label) in [(3_u16, "bump"), (4, "rls"), (5, "tag")].into_iter() {
        let action_start = 1 + detail_width + 1;
        let action_style = match label {
            "rls" => RLS_BUTTON_STYLE,
            "bump" => BUMP_BUTTON_STYLE,
            _ => TAG_BUTTON_STYLE,
        };
        render_highlighted_row(
            frame,
            Rect::new(area.x, area.y + row_offset, area.width, 1),
            &rows[row_offset as usize],
            border_style,
            tile_style,
            &[StyledRange::new(
                action_start,
                CALVER_ACTION_WIDTH,
                action_style,
            )],
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
        bump_rect: Rect::new(action_x, inner_y + 3, CALVER_ACTION_WIDTH as u16, 1),
        view_rect: Rect::new(action_x, inner_y + 4, CALVER_ACTION_WIDTH as u16, 1),
        tag_rect: Rect::new(action_x, inner_y + 5, CALVER_ACTION_WIDTH as u16, 1),
        dev_info_rect: Rect::new(area.x + 1, inner_y + 3, detail_width as u16, 1),
        rls_info_rect: Rect::new(area.x + 1, inner_y + 4, detail_width as u16, 1),
    }
}

fn render_rows(
    frame: &mut Frame,
    area: Rect,
    rows: &[String],
    border_style: Style,
    tile_style: Style,
) {
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
        Paragraph::new(styled_row_with_highlights(
            row,
            border_style,
            tile_style,
            highlights,
        ))
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
    let border_chars = [
        '╔', '╗', '╚', '╝', '║', '═', '│', '╤', '╧', '╟', '╢', '├', '┴', '─',
    ];
    let spans =
        row.graphemes(true)
            .enumerate()
            .map(|(index, grapheme)| {
                let is_border = grapheme.chars().count() == 1
                    && grapheme.chars().next().is_some_and(|character| {
                        border_chars.contains(&character) || character == '·'
                    });
                let style = highlights
                    .iter()
                    .find(|highlight| highlight.contains(index))
                    .map(|highlight| highlight.style)
                    .unwrap_or_else(|| {
                        if is_border {
                            border_style
                        } else {
                            tile_style.fg(Color::White)
                        }
                    });
                Span::styled(grapheme.to_string(), style)
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
    format!(
        "╔{}╤{}╗",
        "═".repeat(SEMVER_LEFT_WIDTH),
        "═".repeat(right_width)
    )
}

fn border_bottom_semver(right_width: usize) -> String {
    format!(
        "╚{}╧{}╝",
        "═".repeat(SEMVER_LEFT_WIDTH),
        "═".repeat(right_width)
    )
}

fn format_tile_info_row(icon: &str, label: &str, value: &str, total_width: usize) -> String {
    center_to_width(&format!("{icon} → {label}: {value}"), total_width)
}

fn format_tile_dev_info_row(icon: &str, label: &str, value: &str, total_width: usize) -> String {
    center_to_width(&format!("{icon} last {label}: {value}"), total_width)
}

fn format_tile_tag_row(icon: &str, value: &str, total_width: usize) -> String {
    center_to_width(&format!("{icon}..HEAD: {value}"), total_width)
}

fn center_to_width(value: &str, width: usize) -> String {
    pad_to_width(truncate_to_width(value, width), width, Alignment::Center)
}

fn truncate_to_width(value: &str, width: usize) -> String {
    let mut rendered = String::new();
    let mut used_width = 0usize;

    for grapheme in value.graphemes(true) {
        let grapheme_width = UnicodeWidthStr::width(grapheme);
        if used_width + grapheme_width > width {
            break;
        }
        rendered.push_str(grapheme);
        used_width += grapheme_width;
    }

    rendered
}

fn pad_to_width(value: String, width: usize, alignment: Alignment) -> String {
    let visible_width = UnicodeWidthStr::width(value.as_str());
    if visible_width >= width {
        return value;
    }

    let remaining = width - visible_width;
    let (left_pad, right_pad) = match alignment {
        Alignment::Center => (remaining / 2, remaining - (remaining / 2)),
        Alignment::Right => (remaining, 0),
        _ => (0, remaining),
    };

    format!("{}{}{}", " ".repeat(left_pad), value, " ".repeat(right_pad))
}

fn dot_fill(width: usize) -> String {
    "·".repeat(width)
}

fn split_semver(version: &str) -> [String; 3] {
    let parts = version
        .split('.')
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    [
        parts.first().cloned().unwrap_or_else(|| "?".to_string()),
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
        assert_eq!(
            split_semver("1.2"),
            ["1".to_string(), "2".to_string(), "?".to_string()]
        );
    }

    #[test]
    fn dot_fill_matches_requested_width() {
        assert_eq!(dot_fill(5), "·····");
    }

    #[test]
    fn format_tile_info_row_centers_unicode_prefix_without_overflow() {
        let formatted = format_tile_info_row("🚧", "tag..→HEAD", "8c ahead", 28);

        assert_eq!(UnicodeWidthStr::width(formatted.as_str()), 28);
        assert!(formatted.contains("🚧 → tag..→HEAD: 8c ahead"));
        assert!(formatted.starts_with(' '));
        assert!(formatted.ends_with(' '));
    }

    #[test]
    fn format_tile_dev_row_starts_with_last_prefix() {
        let formatted = format_tile_dev_info_row("🚧", "tag..→HEAD", "8c ahead", 28);

        assert_eq!(UnicodeWidthStr::width(formatted.as_str()), 28);
        assert!(formatted.contains("🚧 last tag..→HEAD: 8c ahead"));
    }

    #[test]
    fn format_tile_tag_row_centers_unicode_prefix_without_overflow() {
        let formatted = format_tile_tag_row("🏷️️", "11c ahead", 22);

        assert_eq!(UnicodeWidthStr::width(formatted.as_str()), 22);
        assert!(formatted.contains("🏷️️..HEAD: 11c ahead"));
        assert!(formatted.starts_with(' '));
        assert!(formatted.ends_with(' '));
    }

    #[test]
    fn styled_row_keeps_dev_icon_grapheme_intact() {
        let line = styled_row(
            "║🚧→ bump: 5h ago       ║",
            Style::default(),
            Style::default(),
        );
        let rendered = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<Vec<_>>();

        assert!(rendered.contains(&"🚧"));
        assert!(rendered.contains(&"→"));
    }

    #[test]
    fn button_positions_are_spread_evenly() {
        assert_eq!(space_evenly_positions(26, &[6, 6, 5]), vec![1, 10, 19]);
    }
}
