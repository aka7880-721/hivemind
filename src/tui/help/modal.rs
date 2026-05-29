// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit SA-PS License
// For details, see the LICENSE file in the repository root.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::tui::centered_rect;

use super::content::markdown_for;
use crate::tui::markdown_render::{markdown_line_count, render_markdown};
use super::context::HelpContext;

pub(crate) struct HelpModal {
    pub(crate) context: HelpContext,
    scroll: u16,
    /// Updated each frame so scroll limits match the rendered layout width.
    body_width: u16,
}

impl HelpModal {
    pub(crate) fn new(context: HelpContext) -> Self {
        Self {
            context,
            scroll: 0,
            body_width: 80,
        }
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
                "? or Esc close  |  ↑/↓ PgUp/PgDn scroll  |  Home/End jump",
            ))
            .style(Style::default().fg(Color::DarkGray)),
            sections[0],
        );

        let body_block = Block::default().borders(Borders::ALL);
        let body_inner = body_block.inner(sections[1]);
        frame.render_widget(body_block, sections[1]);

        self.body_width = body_inner.width.max(20);
        let body = render_markdown(markdown_for(self.context), self.body_width);
        frame.render_widget(
            Paragraph::new(body)
                .wrap(Wrap { trim: false })
                .scroll((self.scroll, 0)),
            body_inner,
        );
    }

    fn max_scroll(&self) -> u16 {
        markdown_line_count(markdown_for(self.context), self.body_width)
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
