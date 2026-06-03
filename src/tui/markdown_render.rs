// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit SA-PS License
// For details, see the LICENSE file in the repository root.

use comfy_txt_engine::{
    ThemeConfig,
    markdown::{MarkdownInteractiveState, MarkdownRenderer, RenderedMarkdown},
};
use ratatui::text::{Line, Text};

/// Stateful markdown view (supports interactive `<details>` toggles).
pub(crate) struct MarkdownView {
    renderer: MarkdownRenderer,
    blocks: Vec<comfy_txt_engine::markdown::MarkdownBlock>,
    details_state: MarkdownInteractiveState,
}

impl MarkdownView {
    pub(crate) fn new(markdown: &str, width: u16) -> Self {
        let width = usize::max(width as usize, 20);
        let renderer = MarkdownRenderer::new(width);
        let blocks = renderer.parse(markdown);
        Self {
            renderer,
            blocks,
            details_state: MarkdownInteractiveState::default(),
        }
    }

    pub(crate) fn set_width(&mut self, width: u16) {
        self.renderer
            .set_max_width(usize::max(width as usize, 20));
    }

    pub(crate) fn render(&self) -> RenderedMarkdown {
        self.renderer
            .render_interactive(&self.blocks, &ThemeConfig::default(), &self.details_state)
    }

    pub(crate) fn line_count(&self) -> usize {
        self.render().lines.len()
    }

    pub(crate) fn toggle_details_at_document_line(&mut self, document_line: usize) -> bool {
        let rendered = self.render();
        rendered.toggle_at_document_line(document_line, &mut self.details_state)
    }
}

/// Render markdown into ratatui `Text` for use with `Paragraph` scrolling.
pub(crate) fn render_markdown(markdown: &str, width: u16) -> Text<'static> {
    Text::from(render_markdown_lines(markdown, width))
}

pub(crate) fn markdown_line_count(markdown: &str, width: u16) -> usize {
    render_markdown_lines(markdown, width).len()
}

fn render_markdown_lines(markdown: &str, width: u16) -> Vec<Line<'static>> {
    MarkdownView::new(markdown, width).render().lines
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

    #[test]
    fn renders_details_collapsed_by_default() {
        let md = "<details>\n<summary>Tips</summary>\n\n-secret-\n</details>";
        let lines = render_markdown_lines(md, 40);
        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect();
        assert!(text.contains("Tips"));
        assert!(!text.contains("-secret-"));
    }

    #[test]
    fn details_toggle_reveals_body() {
        let md = "<details>\n<summary>Tips</summary>\n\n-hidden-\n</details>";
        let mut view = MarkdownView::new(md, 40);
        assert!(view.toggle_details_at_document_line(0));
        let text: String = view
            .render()
            .lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect();
        assert!(text.contains("-hidden-"));
    }
}
