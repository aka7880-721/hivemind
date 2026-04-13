// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyVersionBumper License v1.2
//
// For details, see the LICENSE file in the repository root.

use ratatui::{
	Frame,
	layout::{Constraint, Direction, Layout, Rect},
	style::{Color, Style, Stylize},
	text::Line,
	widgets::{Paragraph, Wrap},
};
use tui_checkbox::Checkbox;
use tui_tabs::TabNav;

use super::{App, HitAction, HitTarget};
use crate::config::{ProjectConfig, ProjectType};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProjectSettingsTab {
	General,
	Distro,
}

impl ProjectSettingsTab {
	pub(crate) fn step(self, delta: isize) -> Self {
		let tabs = [Self::General, Self::Distro];
		let index = tabs.iter().position(|tab| *tab == self).unwrap_or(0) as isize;
		tabs[(index + delta).rem_euclid(tabs.len() as isize) as usize]
	}
}

pub(crate) fn render_project_settings(app: &mut App, frame: &mut Frame, area: Rect) {
	let Some(project) = app.config.projects.get(app.selected_project).cloned() else {
		frame.render_widget(Paragraph::new("Select a project to manage per-scope settings."), area);
		return;
	};

	let sections = Layout::default()
		.direction(Direction::Vertical)
		.constraints([Constraint::Length(3), Constraint::Min(8)])
		.split(area);

	render_project_settings_tabs(app, frame, sections[0]);

	let scope_index = active_scope_index(&project, app.overview_focused_scope);
	match app.project_settings_tab {
		ProjectSettingsTab::General => render_general_settings(app, frame, sections[1], &project, scope_index),
		ProjectSettingsTab::Distro => render_distro_settings(frame, sections[1], &project, scope_index),
	}
}

fn render_project_settings_tabs(app: &mut App, frame: &mut Frame, area: Rect) {
	let labels = ["General", "Distro"];
	let active_index = match app.project_settings_tab {
		ProjectSettingsTab::General => 0,
		ProjectSettingsTab::Distro => 1,
	};
	let tabs = TabNav::new(&labels, active_index)
		.highlight_style(Style::default().fg(Color::Cyan))
		.border_style(Style::default().fg(Color::DarkGray))
		.style(Style::default().fg(Color::White))
		.indicator(None);
	frame.render_widget(tabs, area);

	let rects = Layout::default()
		.direction(Direction::Horizontal)
		.constraints([Constraint::Length(16), Constraint::Length(16)])
		.split(area);
	app.hit_targets.push(HitTarget::new(rects[0], HitAction::SelectProjectSettingsTab(ProjectSettingsTab::General)));
	app.hit_targets.push(HitTarget::new(rects[1], HitAction::SelectProjectSettingsTab(ProjectSettingsTab::Distro)));
}

fn render_general_settings(
	app: &mut App,
	frame: &mut Frame,
	area: Rect,
	project: &ProjectConfig,
	scope_index: usize,
) {
	let sections = Layout::default()
		.direction(Direction::Vertical)
		.constraints([Constraint::Length(4), Constraint::Length(3), Constraint::Min(6)])
		.split(area);

	let scope_name = active_scope_name(project, scope_index);
	let scope_kind = active_scope_kind(project, scope_index);
	let header = vec![
		Line::from(format!("Selected scope: {}", scope_name)).bold(),
		Line::from(format!("Scope type: {}", scope_kind)),
		Line::from(format!("Changelog path: {}", project.changelog.effective_path())),
	];
	frame.render_widget(Paragraph::new(header).wrap(Wrap { trim: false }), sections[0]);

	let enabled = project.changelog_enabled_for_scope(scope_index);
	let checkbox = Checkbox::new(" Changelog Generation", enabled)
		.style(Style::default().fg(Color::White))
		.checkbox_style(Style::default().fg(if enabled { Color::Green } else { Color::DarkGray }))
		.label_style(Style::default().fg(Color::White));
	frame.render_widget(checkbox, sections[1]);
	app.hit_targets.push(HitTarget::new(sections[1], HitAction::ToggleProjectScopeChangelog));

	let body = vec![
		Line::from("This toggle now lives at the scope level.".yellow()),
		Line::from(if project.project_type == ProjectType::Branched {
			"Use the focused overview tile or click another tile to switch scopes."
		} else {
			"All-in-one projects apply this setting to the single project scope."
		}),
		Line::from("Edit Project stays on E. The changelog path is still managed in project setup/edit dialogs."),
		Line::from("Press Space or Enter to toggle the selected checkbox. Use [ and ] to switch General/Distro."),
	];
	frame.render_widget(Paragraph::new(body).wrap(Wrap { trim: false }), sections[2]);
}

fn render_distro_settings(frame: &mut Frame, area: Rect, project: &ProjectConfig, scope_index: usize) {
	let lines = vec![
		Line::from(format!("Scope: {}", active_scope_name(project, scope_index))).bold(),
		Line::raw(""),
		Line::from("Distro-specific scope settings will land here next."),
		Line::from("The first migrated setting is under General: changelog generation."),
	];
	frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

fn active_scope_index(project: &ProjectConfig, focused_scope: usize) -> usize {
	if project.project_type == ProjectType::Branched {
		focused_scope.min(project.branches.len().saturating_sub(1))
	} else {
		0
	}
}

fn active_scope_name(project: &ProjectConfig, scope_index: usize) -> String {
	if project.project_type == ProjectType::Branched {
		project
			.branches
			.get(scope_index)
			.map(|branch| branch.display_name().to_string())
			.unwrap_or_else(|| "Selected scope".to_string())
	} else {
		project.name.clone()
	}
}

fn active_scope_kind(project: &ProjectConfig, scope_index: usize) -> &'static str {
	if project.project_type == ProjectType::Branched {
		project
			.branches
			.get(scope_index)
			.map(|branch| branch.scope_kind.display_name())
			.unwrap_or("Scope")
	} else {
		"Project"
	}
}