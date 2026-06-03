// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit SA-PS License
// For details, see the LICENSE file in the repository root.

use comfy_txt_engine::{
    ThemeConfig,
    markdown::{
        MarkdownInteractiveState, MarkdownRenderer, RenderedMarkdown, image::ResolvedImage,
    },
};
use ratatui::text::{Line, Text};

use ratatui_image::picker::Picker;

use crate::tui::help::assets::HelpImageResolver;

/// Stateful markdown view (supports interactive `<details>` toggles, links, and images).
pub(crate) struct MarkdownView {
    renderer: MarkdownRenderer,
    blocks: Vec<comfy_txt_engine::markdown::MarkdownBlock>,
    resolved_images: Vec<ResolvedImage>,
    asset_resolver: Option<HelpImageResolver>,
    details_state: MarkdownInteractiveState,
    width: u16,
}

impl MarkdownView {
    pub(crate) fn new(
        markdown: &str,
        width: u16,
        asset_dir: Option<&str>,
        picker: &Picker,
    ) -> Self {
        let view_width = width.max(20);
        let render_width = usize::from(view_width);
        let renderer = MarkdownRenderer::new(render_width);
        let mut asset_resolver = asset_dir.map(|dir| HelpImageResolver::new(dir, picker));
        let (blocks, resolved_images) = if let Some(resolver) = asset_resolver.as_mut() {
            renderer.parse_with_images(markdown, resolver)
        } else {
            (renderer.parse(markdown), Vec::new())
        };
        Self {
            renderer,
            blocks,
            resolved_images,
            asset_resolver,
            details_state: MarkdownInteractiveState::default(),
            width: view_width,
        }
    }

    pub(crate) fn set_width(&mut self, width: u16) {
        self.width = width.max(20);
        self.renderer.set_max_width(self.width as usize);
    }

    pub(crate) fn render(&self) -> RenderedMarkdown {
        let theme = ThemeConfig::default();
        if let Some(resolver) = self.asset_resolver.as_ref() {
            let mut resolver = resolver.clone();
            self.renderer.render_interactive_with_images(
                &self.blocks,
                &theme,
                &self.details_state,
                &self.resolved_images,
                &mut resolver,
                self.width,
                120,
            )
        } else {
            self.renderer
                .render_interactive(&self.blocks, &theme, &self.details_state)
        }
    }

    pub(crate) fn line_count(&self) -> usize {
        self.render().lines.len()
    }

    pub(crate) fn toggle_details_at_document_line(&mut self, document_line: usize) -> bool {
        let rendered = self.render();
        rendered.toggle_at_document_line(document_line, &mut self.details_state)
    }

    pub(crate) fn link_at_document_line(
        &self,
        document_line: usize,
        column: u16,
    ) -> Option<String> {
        self.render()
            .link_at_document_line(document_line, column)
            .map(str::to_string)
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
    let picker = crate::tui::help::assets::create_help_picker();
    MarkdownView::new(markdown, width, None, &picker)
        .render()
        .lines
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
        let picker = crate::tui::help::assets::create_help_picker();
        let mut view = MarkdownView::new(md, 40, None, &picker);
        assert!(view.toggle_details_at_document_line(0));
        let text: String = view
            .render()
            .lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect();
        assert!(text.contains("-hidden-"));
    }

    #[test]
    fn records_link_hit_for_markdown_link() {
        let md = "See [ComfyHome](https://comfyhome.io) today.";
        let picker = crate::tui::help::assets::create_help_picker();
        let rendered = MarkdownView::new(md, 60, None, &picker).render();
        assert!(
            rendered
                .link_hits
                .iter()
                .any(|h| h.url == "https://comfyhome.io"),
            "expected link hit: {:?}",
            rendered.link_hits
        );
    }
}
