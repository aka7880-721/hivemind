// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit SA-PS License
// For details, see the LICENSE file in the repository root.

use std::process::Stdio;

use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget, Wrap};
use ratatui_image::{Image, Resize, picker::Picker, protocol::Protocol};

use crate::tui::centered_rect;
use crate::tui::help::assets::{
    HELP_LAYOUT_WIDTH, create_help_picker, safe_font_size, scale_image_to_cells,
};
use crate::tui::markdown_render::MarkdownView;

use super::content::markdown_for;
use super::context::HelpContext;

struct PreparedImage {
    width_cells: u16,
    height_cells: u16,
    protocol: Protocol,
}

fn frozen_text_area(body: Rect) -> Rect {
    Rect::new(
        body.x,
        body.y,
        HELP_LAYOUT_WIDTH.min(body.width),
        body.height,
    )
}

pub(crate) struct HelpModal {
    pub(crate) context: HelpContext,
    scroll: u16,
    markdown: MarkdownView,
    body_area: Rect,
    picker: Picker,
    prepared_images: Vec<PreparedImage>,
}

impl HelpModal {
    pub(crate) fn new(context: HelpContext) -> Self {
        let picker = create_help_picker();
        let markdown = MarkdownView::new(
            markdown_for(context),
            HELP_LAYOUT_WIDTH,
            context.asset_dir(),
            &picker,
        );
        let prepared_images = Self::prepare_images(&picker, &markdown.render());
        Self {
            context,
            scroll: 0,
            markdown,
            body_area: Rect::default(),
            picker,
            prepared_images,
        }
    }

    pub(crate) fn scroll_wheel(&mut self, delta: i16) {
        self.scroll_by(delta);
    }

    pub(crate) fn handle_mouse(&mut self, mouse: MouseEvent) -> bool {
        if mouse.kind != MouseEventKind::Down(MouseButton::Left) {
            return false;
        }
        if !self
            .body_area
            .contains(Position::new(mouse.column, mouse.row))
        {
            return false;
        }
        let rel_row = mouse.row.saturating_sub(self.body_area.y) as usize;
        let document_line = self.scroll as usize + rel_row;
        let column = mouse.column.saturating_sub(self.body_area.x);
        if let Some(url) = self.markdown.link_at_document_line(document_line, column) {
            open_url(&url);
            return true;
        }
        self.markdown.toggle_details_at_document_line(document_line)
    }

    pub(crate) fn handle_key(&mut self, key: KeyEvent) -> bool {
        if key.modifiers.is_empty() && matches!(key.code, KeyCode::Char('?') | KeyCode::Esc) {
            return true;
        }

        match key.code {
            KeyCode::PageUp => self.scroll_by(-8),
            KeyCode::PageDown => self.scroll_by(8),
            KeyCode::Up if key.modifiers.is_empty() => self.scroll_by(-1),
            KeyCode::Down if key.modifiers.is_empty() => self.scroll_by(1),
            KeyCode::Home => self.scroll = 0,
            KeyCode::End => self.scroll = self.max_scroll(),
            KeyCode::Enter | KeyCode::Char(' ') => {
                let document_line = self.scroll as usize;
                self.markdown.toggle_details_at_document_line(document_line);
            }
            _ => {}
        }
        false
    }

    pub(crate) fn render(&mut self, frame: &mut Frame, area: Rect) {
        let popup = centered_rect(area, 88, 86);
        frame.render_widget(Clear, popup);

        let block = Block::default()
            .borders(Borders::ALL)
            .title(format!(" Help — {} ", self.context.title()))
            .title_style(Style::default().add_modifier(Modifier::BOLD))
            .border_style(Style::default().fg(Color::Cyan));
        let inner = block.inner(popup);
        frame.render_widget(block, popup);

        let sections = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(2), Constraint::Min(8)])
            .split(inner);

        frame.render_widget(
            Paragraph::new(Line::from(
                "? or Esc close  |  scroll  |  click links / summary  |  Home/End",
            ))
            .style(Style::default().fg(Color::DarkGray)),
            sections[0],
        );

        let body_block = Block::default().borders(Borders::ALL);
        let body_inner = body_block.inner(sections[1]);
        frame.render_widget(body_block, sections[1]);
        self.body_area = body_inner;

        // Layout width is frozen at modal open — terminal resize only clips, never rescales.
        let rendered = self.markdown.render();
        let text_area = frozen_text_area(body_inner);

        if self.prepared_images.len() != rendered.images.len() {
            self.prepared_images = Self::prepare_images(&self.picker, &rendered);
        }

        let body = Text::from(rendered.lines);
        frame.render_widget(
            Paragraph::new(body)
                .wrap(Wrap { trim: false })
                .scroll((self.scroll, 0)),
            text_area,
        );

        for (idx, placement) in rendered.images.iter().enumerate() {
            let Some(prepared) = self.prepared_images.get(idx) else {
                continue;
            };
            let top = text_area.y as i32 + placement.row as i32 - self.scroll as i32;
            let bottom = top + prepared.height_cells as i32;
            let viewport_top = text_area.y as i32;
            let viewport_bottom = text_area.y as i32 + text_area.height as i32;
            if bottom <= viewport_top || top >= viewport_bottom {
                continue;
            }
            let clip_top = top.max(viewport_top) as u16;
            let clip_bottom = bottom.min(viewport_bottom) as u16;
            let vis_h = clip_bottom.saturating_sub(clip_top);
            if vis_h == 0 {
                continue;
            }
            let rect = Rect::new(
                text_area.x.saturating_add(placement.col as u16),
                clip_top,
                prepared.width_cells,
                vis_h,
            );
            Image::new(&prepared.protocol).render(rect, frame.buffer_mut());
        }
    }

    fn prepare_images(
        picker: &Picker,
        rendered: &comfy_txt_engine::markdown::RenderedMarkdown,
    ) -> Vec<PreparedImage> {
        let (font_w, font_h) = safe_font_size(picker);
        let proto = picker.protocol_type();
        rendered
            .images
            .iter()
            .filter_map(|placement| {
                let scaled = scale_image_to_cells(
                    &placement.image,
                    placement.width_cells,
                    placement.height_cells,
                    font_w,
                    font_h,
                    proto,
                );
                picker
                    .new_protocol(
                        scaled,
                        Rect::new(0, 0, placement.width_cells, placement.height_cells).into(),
                        Resize::Fit(None),
                    )
                    .ok()
                    .map(|protocol| PreparedImage {
                        width_cells: placement.width_cells,
                        height_cells: placement.height_cells,
                        protocol,
                    })
            })
            .collect()
    }

    fn max_scroll(&self) -> u16 {
        self.markdown
            .line_count()
            .saturating_sub(1)
            .min(u16::MAX as usize) as u16
    }

    fn scroll_by(&mut self, delta: i16) {
        if delta.is_negative() {
            self.scroll = self.scroll.saturating_sub(delta.unsigned_abs());
        } else {
            self.scroll = self.scroll.saturating_add(delta as u16);
        }
        self.scroll = self.scroll.min(self.max_scroll());
    }
}

fn open_url(url: &str) -> bool {
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(url)
            .env_remove("DRI_PRIME")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .is_ok()
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .is_ok()
    }
    #[cfg(windows)]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .is_ok()
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        let _ = url;
        false
    }
}
