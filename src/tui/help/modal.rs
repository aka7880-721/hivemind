// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit SA-PS License
// For details, see the LICENSE file in the repository root.

use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::tui::centered_rect;
use crate::tui::markdown_render::MarkdownView;

use super::content::markdown_for;
use super::context::HelpContext;

pub(crate) struct HelpModal {
    pub(crate) context: HelpContext,
    scroll: u16,
    body_width: u16,
    markdown: MarkdownView,
    body_area: Rect,
}

impl HelpModal {
    pub(crate) fn new(context: HelpContext) -> Self {
        Self {
            context,
            scroll: 0,
            body_width: 80,
            markdown: MarkdownView::new(markdown_for(context), 80),
            body_area: Rect::default(),
        }
    }

    pub(crate) fn scroll_wheel(&mut self, delta: i16) {
        self.scroll_by(delta);
    }

    pub(crate) fn handle_mouse(&mut self, mouse: MouseEvent) -> bool {
        if mouse.kind != MouseEventKind::Down(MouseButton::Left) {
            return false;
        }
        if !self.body_area.contains(Position::new(mouse.column, mouse.row)) {
            return false;
        }
        let rel_row = mouse.row.saturating_sub(self.body_area.y) as usize;
        let document_line = self.scroll as usize + rel_row;
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
                "? or Esc close  |  ↑/↓ PgUp/PgDn or wheel scroll  |  Home/End jump  |  click summary to expand",
            ))
            .style(Style::default().fg(Color::DarkGray)),
            sections[0],
        );

        let body_block = Block::default().borders(Borders::ALL);
        let body_inner = body_block.inner(sections[1]);
        frame.render_widget(body_block, sections[1]);
        self.body_area = body_inner;

        self.body_width = body_inner.width.max(20);
        self.markdown
            .set_width(self.body_width);
        let rendered = self.markdown.render();
        let body = Text::from(rendered.lines);
        frame.render_widget(
            Paragraph::new(body)
                .wrap(Wrap { trim: false })
                .scroll((self.scroll, 0)),
            body_inner,
        );
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
