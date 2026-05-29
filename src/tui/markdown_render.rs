// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit SA-PS License
// For details, see the LICENSE file in the repository root.

use ratatui::text::{Line, Span, Text};
use ratatui_markdown::{ThemeConfig, markdown::MarkdownRenderer};

/// Render markdown into ratatui `Text` for use with `Paragraph` scrolling.
pub(crate) fn render_markdown(markdown: &str, width: u16) -> Text<'static> {
    Text::from(render_markdown_lines(markdown, width))
}

pub(crate) fn markdown_line_count(markdown: &str, width: u16) -> usize {
    render_markdown_lines(markdown, width).len()
}

fn render_markdown_lines(markdown: &str, width: u16) -> Vec<Line<'static>> {
    let width = usize::max(width as usize, 20);
    let renderer = MarkdownRenderer::new(width);
    let blocks = renderer.parse(markdown);
    let lines = renderer.render(&blocks, &ThemeConfig::default());
    adopt_lines(lines)
}

/// `ratatui-markdown` renders with ratatui 0.29; ComfyGit uses ratatui 0.30.
fn adopt_lines(lines: Vec<ratatui29::text::Line<'static>>) -> Vec<Line<'static>> {
    lines.into_iter().map(adopt_line).collect()
}

fn adopt_line(line: ratatui29::text::Line<'static>) -> Line<'static> {
    Line::from(
        line.spans
            .into_iter()
            .map(|span| Span::styled(span.content, adopt_style(span.style)))
            .collect::<Vec<_>>(),
    )
}

fn adopt_style(style: ratatui29::style::Style) -> ratatui::style::Style {
    let mut out = ratatui::style::Style::default();
    if let Some(fg) = style.fg {
        out = out.fg(adopt_color(fg));
    }
    if let Some(bg) = style.bg {
        out = out.bg(adopt_color(bg));
    }
    if !style.add_modifier.is_empty() {
        out = out.add_modifier(adopt_modifier(style.add_modifier));
    }
    if !style.sub_modifier.is_empty() {
        out = out.remove_modifier(adopt_modifier(style.sub_modifier));
    }
    out
}

fn adopt_color(color: ratatui29::style::Color) -> ratatui::style::Color {
    match color {
        ratatui29::style::Color::Reset => ratatui::style::Color::Reset,
        ratatui29::style::Color::Black => ratatui::style::Color::Black,
        ratatui29::style::Color::Red => ratatui::style::Color::Red,
        ratatui29::style::Color::Green => ratatui::style::Color::Green,
        ratatui29::style::Color::Yellow => ratatui::style::Color::Yellow,
        ratatui29::style::Color::Blue => ratatui::style::Color::Blue,
        ratatui29::style::Color::Magenta => ratatui::style::Color::Magenta,
        ratatui29::style::Color::Cyan => ratatui::style::Color::Cyan,
        ratatui29::style::Color::Gray => ratatui::style::Color::Gray,
        ratatui29::style::Color::DarkGray => ratatui::style::Color::DarkGray,
        ratatui29::style::Color::LightRed => ratatui::style::Color::LightRed,
        ratatui29::style::Color::LightGreen => ratatui::style::Color::LightGreen,
        ratatui29::style::Color::LightYellow => ratatui::style::Color::LightYellow,
        ratatui29::style::Color::LightBlue => ratatui::style::Color::LightBlue,
        ratatui29::style::Color::LightMagenta => ratatui::style::Color::LightMagenta,
        ratatui29::style::Color::LightCyan => ratatui::style::Color::LightCyan,
        ratatui29::style::Color::White => ratatui::style::Color::White,
        ratatui29::style::Color::Rgb(r, g, b) => ratatui::style::Color::Rgb(r, g, b),
        ratatui29::style::Color::Indexed(i) => ratatui::style::Color::Indexed(i),
    }
}

fn adopt_modifier(modifier: ratatui29::style::Modifier) -> ratatui::style::Modifier {
    let mut out = ratatui::style::Modifier::empty();
    if modifier.contains(ratatui29::style::Modifier::BOLD) {
        out |= ratatui::style::Modifier::BOLD;
    }
    if modifier.contains(ratatui29::style::Modifier::DIM) {
        out |= ratatui::style::Modifier::DIM;
    }
    if modifier.contains(ratatui29::style::Modifier::ITALIC) {
        out |= ratatui::style::Modifier::ITALIC;
    }
    if modifier.contains(ratatui29::style::Modifier::UNDERLINED) {
        out |= ratatui::style::Modifier::UNDERLINED;
    }
    if modifier.contains(ratatui29::style::Modifier::SLOW_BLINK) {
        out |= ratatui::style::Modifier::SLOW_BLINK;
    }
    if modifier.contains(ratatui29::style::Modifier::RAPID_BLINK) {
        out |= ratatui::style::Modifier::RAPID_BLINK;
    }
    if modifier.contains(ratatui29::style::Modifier::REVERSED) {
        out |= ratatui::style::Modifier::REVERSED;
    }
    if modifier.contains(ratatui29::style::Modifier::HIDDEN) {
        out |= ratatui::style::Modifier::HIDDEN;
    }
    if modifier.contains(ratatui29::style::Modifier::CROSSED_OUT) {
        out |= ratatui::style::Modifier::CROSSED_OUT;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_markdown_table_with_box_drawing() {
        let md = "| Key | Action |\n|-----|--------|\n| **?** | Help |\n";
        let text = render_markdown(md, 60);
        let rendered = text
            .lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            rendered.contains('│') || rendered.contains('|'),
            "expected table borders in:\n{rendered}"
        );
        assert!(rendered.contains('?'), "expected cell content in:\n{rendered}");
    }
}
