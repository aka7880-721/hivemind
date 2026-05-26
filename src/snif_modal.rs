// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit SA-PS License

use std::path::PathBuf;

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};
use snif__by_comfyhome::engine::{ReplaceOptions, SearchOptions, replace, search};

use crate::app::{App, StatusMessage};
use crate::cli::project_root;
use crate::config::ProjectConfig;
use crate::ui::centered_rect;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SnifMode {
    Search,
    Replace,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SnifEditField {
    Filters,
    Pattern,
    Replacement,
}

pub(crate) struct SnifModal {
    root: PathBuf,
    filters_input: String,
    pattern_input: String,
    replacement_input: String,
    mode: SnifMode,
    edit_field: SnifEditField,
    output_lines: Vec<String>,
    output_scroll: usize,
    case_sensitive: bool,
    case_insensitive_replace: bool,
}

impl SnifModal {
    fn new(root: PathBuf) -> Self {
        Self {
            root,
            filters_input: "*".to_string(),
            pattern_input: String::new(),
            replacement_input: String::new(),
            mode: SnifMode::Search,
            edit_field: SnifEditField::Pattern,
            output_lines: vec![
                "SNIF — Search, Narrow, Inspect, Fix".into(),
                "Enter pattern, F2 to run, Esc to close.".into(),
            ],
            output_scroll: 0,
            case_sensitive: false,
            case_insensitive_replace: false,
        }
    }

    fn run_search(&mut self) -> Result<()> {
        if self.pattern_input.is_empty() {
            return Ok(());
        }
        let result = search(&SearchOptions {
            root: self.root.clone(),
            filters: self.filters_input.clone(),
            pattern: self.pattern_input.clone(),
            case_sensitive: self.case_sensitive,
        })?;
        self.output_lines.clear();
        for m in &result.matches {
            self.output_lines
                .push(format!("{}:{}:{}", m.path.display(), m.line_number, m.line));
        }
        self.output_lines.push(format!(
            "--- {} match(es), {} exact ---",
            result.summary.total_matches, result.summary.exact_matches
        ));
        self.output_scroll = 0;
        Ok(())
    }

    fn run_replace(&mut self) -> Result<()> {
        if self.pattern_input.is_empty() || self.replacement_input.is_empty() {
            return Ok(());
        }
        let changed = replace(&ReplaceOptions {
            root: self.root.clone(),
            filters: self.filters_input.clone(),
            pattern: self.pattern_input.clone(),
            replacement: self.replacement_input.clone(),
            case_insensitive: self.case_insensitive_replace,
            yes: true,
        })?;
        self.output_lines.clear();
        for path in changed {
            self.output_lines.push(path.display().to_string());
        }
        self.output_lines.push("Done.".into());
        self.output_scroll = 0;
        Ok(())
    }

    fn push_char(&mut self, c: char) {
        match self.edit_field {
            SnifEditField::Filters => self.filters_input.push(c),
            SnifEditField::Pattern => self.pattern_input.push(c),
            SnifEditField::Replacement => self.replacement_input.push(c),
        }
    }

    fn backspace(&mut self) {
        match self.edit_field {
            SnifEditField::Filters => {
                self.filters_input.pop();
            }
            SnifEditField::Pattern => {
                self.pattern_input.pop();
            }
            SnifEditField::Replacement => {
                self.replacement_input.pop();
            }
        }
    }

    fn cycle_field(&mut self) {
        self.edit_field = match self.edit_field {
            SnifEditField::Filters => SnifEditField::Pattern,
            SnifEditField::Pattern => SnifEditField::Replacement,
            SnifEditField::Replacement => SnifEditField::Filters,
        };
    }
}

impl App {
    pub(crate) fn open_snif_dialog(&mut self) -> Result<()> {
        let project = self.selected_project()?.clone();
        let root = snif_project_root(&project)?;
        self.snif_dialog = Some(SnifModal::new(root));
        self.status = StatusMessage::info("SNIF: edit pattern, F2 run, m toggle mode, Esc close.");
        Ok(())
    }

    pub(crate) fn close_snif_dialog(&mut self) {
        self.snif_dialog = None;
        self.status = StatusMessage::info("SNIF closed.");
    }

    pub(crate) fn handle_snif_key(&mut self, key: KeyEvent) -> Result<()> {
        let Some(dialog) = &mut self.snif_dialog else {
            return Ok(());
        };

        match key.code {
            KeyCode::Esc => self.close_snif_dialog(),
            KeyCode::F(2) | KeyCode::Enter => {
                let mode = dialog.mode;
                let result = match mode {
                    SnifMode::Search => dialog.run_search(),
                    SnifMode::Replace => dialog.run_replace(),
                };
                if let Err(error) = result {
                    self.status = StatusMessage::error(error.to_string());
                } else {
                    self.status = StatusMessage::success("SNIF operation complete.");
                }
            }
            KeyCode::Char('m') | KeyCode::Char('M') => {
                dialog.mode = match dialog.mode {
                    SnifMode::Search => SnifMode::Replace,
                    SnifMode::Replace => SnifMode::Search,
                };
            }
            KeyCode::Char('e') => dialog.case_sensitive = !dialog.case_sensitive,
            KeyCode::Char('a') => {
                dialog.case_insensitive_replace = !dialog.case_insensitive_replace;
            }
            KeyCode::Tab => dialog.cycle_field(),
            KeyCode::Up => dialog.output_scroll = dialog.output_scroll.saturating_sub(1),
            KeyCode::Down => dialog.output_scroll = dialog.output_scroll.saturating_add(1),
            KeyCode::Backspace => dialog.backspace(),
            KeyCode::Char(c)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                dialog.push_char(c);
            }
            _ => {}
        }
        Ok(())
    }

    pub(crate) fn render_snif_dialog(&mut self, frame: &mut Frame, area: Rect) {
        let Some(dialog) = &self.snif_dialog else {
            return;
        };

        let popup = centered_rect(area, 88, 82);
        frame.render_widget(Clear, popup);
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" SNIF — Search, Narrow, Inspect, Fix ")
            .border_style(Style::default().fg(Color::Cyan));
        let inner = block.inner(popup);
        frame.render_widget(block, popup);

        let sections = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(8),
                Constraint::Min(6),
                Constraint::Length(2),
            ])
            .split(inner);

        let mode = match dialog.mode {
            SnifMode::Search => "search",
            SnifMode::Replace => "rpl",
        };

        let ops = vec![
            Line::from(vec![Span::raw(format!(
                " Root: {} ",
                dialog.root.display()
            ))]),
            Line::from(vec![
                Span::raw(" Mode: "),
                Span::styled(
                    mode,
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" (m)  "),
                Span::raw("F2 run  Esc close  Tab field"),
            ]),
            Line::from(format!(" File-spec: {}", dialog.filters_input)),
            Line::from(format!(" Pattern:   {}", dialog.pattern_input)),
            Line::from(format!(" Replace:   {}", dialog.replacement_input)),
        ];
        frame.render_widget(
            Paragraph::new(ops)
                .block(Block::default().borders(Borders::ALL).title(" Input "))
                .wrap(Wrap { trim: false }),
            sections[0],
        );

        let visible: Vec<ListItem> = dialog
            .output_lines
            .iter()
            .skip(dialog.output_scroll)
            .take(sections[1].height.saturating_sub(2) as usize)
            .map(|line| ListItem::new(Line::from(line.as_str())))
            .collect();
        frame.render_widget(
            List::new(visible).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!(" Results ({}) ", dialog.output_lines.len())),
            ),
            sections[1],
        );

        frame.render_widget(
            Paragraph::new("F2 run · Esc close · m mode · e case-sensitive search · a case-insensitive replace"),
            sections[2],
        );
    }
}

fn snif_project_root(project: &ProjectConfig) -> Result<PathBuf> {
    project_root(project)
}
