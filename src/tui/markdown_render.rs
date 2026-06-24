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

use crate::tui::help::assets::{HELP_IMAGE_MAX_WIDTH, HelpImageResolver};

/// Stateful markdown view (supports interactive `<details>` toggles, links, and images).
pub(crate) struct MarkdownView {
    renderer: MarkdownRenderer,
    blocks: Vec<comfy_txt_engine::markdown::MarkdownBlock>,
    resolved_images: Vec<ResolvedImage>,
    asset_resolver: Option<HelpImageResolver>,
    details_state: MarkdownInteractiveState,
}

impl MarkdownView {
    pub(crate) fn new(
        markdown: &str,
        width: u16,
        asset_dir: Option<&str>,
        picker: &Picker,
    ) -> Self {
        let layout_width = width.max(20);
        let renderer = MarkdownRenderer::new(usize::from(layout_width));
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
        }
    }

    pub(crate) fn set_layout_width(&mut self, width: u16) {
        self.renderer.set_max_width(usize::from(width.max(20)));
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
                HELP_IMAGE_MAX_WIDTH,
                200,
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
    use std::path::PathBuf;

    #[test]
    fn dashboard_projects_help_renders_embedded_images() {
        let md = "![logo](logo-protected.webp)\n\n![Demo](demo.webp)\n";
        let picker = crate::tui::help::assets::create_help_picker();
        let view = MarkdownView::new(md, 120, Some("pages/dashboard"), &picker);
        let rendered = view.render();
        let text: String = rendered
            .lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect();
        assert!(
            !text.contains("[image: logo]"),
            "logo should resolve, text snippet:\n{text}"
        );
        assert!(
            rendered.images.len() >= 2,
            "expected demo + logo placements, got {}",
            rendered.images.len()
        );
        assert!(
            rendered
                .images
                .iter()
                .any(|p| p.width_cells > 0 && p.height_cells > 0),
            "image placements should have non-zero size"
        );
    }

    #[test]
    fn resolves_logo_protected_webp_png_bytes() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src/tui/help/pages/dashboard/logo-protected.webp");
        assert!(path.is_file());
        let img = image::ImageReader::open(&path)
            .expect("open")
            .with_guessed_format()
            .expect("guess format")
            .decode()
            .expect("logo bytes should decode regardless of extension");
        assert_eq!(img.width(), 512);
    }

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
