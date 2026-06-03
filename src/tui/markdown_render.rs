// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit SA-PS License
// For details, see the LICENSE file in the repository root.

use comfy_txt_engine::{ThemeConfig, markdown::MarkdownRenderer};
use ratatui::text::{Line, Text};

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
    renderer.render(&blocks, &ThemeConfig::default())
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
        assert!(
            rendered.contains('?'),
            "expected cell content in:\n{rendered}"
        );
    }
}
