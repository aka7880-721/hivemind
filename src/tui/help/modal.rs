// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit SA-PS License
// For details, see the LICENSE file in the repository root.

use std::process::Stdio;

use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use image::imageops::FilterType;
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget, Wrap};
use ratatui_image::{Image, Resize, picker::Picker, protocol::Protocol};

use crate::tui::centered_rect;
use crate::tui::markdown_render::MarkdownView;

use super::assets::create_help_picker;
use super::content::markdown_for;
use super::context::HelpContext;

pub(crate) struct HelpModal {
    pub(crate) context: HelpContext,
    scroll: u16,
    body_width: u16,
    markdown: MarkdownView,
    body_area: Rect,
    picker: Picker,
    image_protocols: Vec<Option<Protocol>>,
    image_protocol_width: u16,
}

impl HelpModal {
    pub(crate) fn new(context: HelpContext) -> Self {
        let picker = create_help_picker();
        let markdown = MarkdownView::new(markdown_for(context), 80, context.asset_dir(), &picker);
        let image_protocols = Self::build_image_protocols(&picker, &markdown.render());
        Self {
            context,
            scroll: 0,
            body_width: 80,
            markdown,
            body_area: Rect::default(),
            picker,
            image_protocols,
            image_protocol_width: 80,
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

        self.body_width = body_inner.width.max(20);
        self.markdown.set_width(self.body_width);
        let rendered = self.markdown.render();
        if self.body_width != self.image_protocol_width {
            self.image_protocols = Self::build_image_protocols(&self.picker, &rendered);
            self.image_protocol_width = self.body_width;
        }

        let body = Text::from(rendered.lines);
        frame.render_widget(
            Paragraph::new(body)
                .wrap(Wrap { trim: false })
                .scroll((self.scroll, 0)),
            body_inner,
        );

        for (idx, placement) in rendered.images.iter().enumerate() {
            let Some(protocol) = self.image_protocols.get(idx).and_then(|p| p.as_ref()) else {
                continue;
            };
            let top = body_inner.y as i32 + placement.row as i32 - self.scroll as i32;
            let bottom = top + placement.height_cells as i32;
            let viewport_top = body_inner.y as i32;
            let viewport_bottom = body_inner.y as i32 + body_inner.height as i32;
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
                body_inner.x.saturating_add(placement.col as u16),
                clip_top,
                placement.width_cells.min(body_inner.width),
                vis_h,
            );
            Image::new(protocol).render(rect, frame.buffer_mut());
        }
    }

    fn build_image_protocols(
        picker: &Picker,
        rendered: &comfy_txt_engine::markdown::RenderedMarkdown,
    ) -> Vec<Option<Protocol>> {
        let (font_w, font_h) = font_size(picker);
        let proto = picker.protocol_type();
        rendered
            .images
            .iter()
            .map(|placement| {
                let target_w = (placement.width_cells as u32).saturating_mul(font_w as u32);
                let target_h =
                    (placement.height_cells as u32).saturating_mul(row_pixel_height(font_h, proto));
                let scaled = placement.image.resize_exact(
                    target_w.max(1),
                    target_h.max(1),
                    FilterType::Triangle,
                );
                picker
                    .new_protocol(
                        scaled,
                        Rect::new(0, 0, placement.width_cells, placement.height_cells).into(),
                        Resize::Fit(None),
                    )
                    .ok()
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

fn font_size(picker: &Picker) -> (u16, u16) {
    let font = picker.font_size();
    if font.width == 0 || font.height == 0 {
        (8, 16)
    } else {
        (font.width, font.height)
    }
}

fn row_pixel_height(font_h: u16, proto: ratatui_image::picker::ProtocolType) -> u32 {
    match proto {
        ratatui_image::picker::ProtocolType::Halfblocks => (font_h as u32) * 2,
        _ => font_h as u32,
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
