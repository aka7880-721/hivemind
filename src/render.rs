// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyVersionBumper License v1.2
//
// For details, see the LICENSE file in the repository root.

use super::*;

impl App {
	pub(crate) fn draw(&mut self, frame: &mut Frame) {
		self.sync_status_toasts();
		self.hit_targets.clear();
		self.overview_tile_viewport = None;
		self.overview_recent_viewport = None;
		self.overview_tile_rects.clear();

		let header_height = header_height_for_viewport(frame.area().height);
		let footer_height = if self.config.ui.hide_footer { 0 } else { 3 };
		let root = Layout::default()
			.direction(Direction::Vertical)
			.constraints([
				Constraint::Length(header_height),
				Constraint::Length(3),
				Constraint::Min(12),
				Constraint::Length(footer_height),
			])
			.split(frame.area());

		self.render_header(frame, root[0]);
		self.render_nav(frame, root[1]);

		match self.screen {
			Screen::Dashboard => self.render_dashboard(frame, root[2]),
			Screen::Wizard => self.render_wizard(frame, root[2]),
			Screen::UiSettings => self.render_ui_settings(frame, root[2]),
		}

		if self.bump_dialog.is_some() {
			self.render_bump_dialog(frame, frame.area());
		}
		if self.overview_bump_workflow_dialog.is_some() {
			self.render_overview_bump_workflow_dialog(frame, frame.area());
		}
		if self.overview_bump_warning_dialog.is_some() {
			self.render_overview_bump_warning_dialog(frame, frame.area());
		}
		if self.main_branch_warning_dialog.is_some() {
			self.render_main_branch_warning_dialog(frame, frame.area());
		}
		if self.changelog_preview_dialog.is_some() {
			self.render_changelog_preview_dialog(frame, frame.area());
		}
		if self.recent_changes_dialog.is_some() {
			self.render_recent_changes_dialog(frame, frame.area());
		}
		if self.tag_dialog.is_some() {
			self.render_tag_dialog(frame, frame.area());
		}
		if self.tag_annotation_dialog.is_some() {
			self.render_tag_annotation_dialog(frame, frame.area());
		}
		if self.project_edit_dialog.is_some() {
			self.render_project_edit_dialog(frame, frame.area());
		}
		if self.release_now_dialog.is_some() {
			self.render_release_now_dialog(frame, frame.area());
		}
		if self.release_now_notes_dialog.is_some() {
			self.render_release_now_notes_dialog(frame, frame.area());
		}
		if self.delete_confirmation_dialog.is_some() {
			self.render_delete_confirmation_dialog(frame, frame.area());
		}
		if self.browser_dialog.is_some() {
			self.render_browser_dialog(frame, frame.area());
		}
		if self.progress_dialog.is_some() {
			self.render_progress_dialog(frame, frame.area());
		}

		if !self.config.ui.hide_footer {
			self.render_footer(frame, root[3]);
		}
		self.transient_toaster.set_area(frame.area());
		let transient_area = self.transient_toaster.toast_area();
		if self.transient_toaster.has_toast() {
			self.sticky_toaster.set_area_avoiding(frame.area(), &[transient_area]);
		} else {
			self.sticky_toaster.set_area(frame.area());
		}
		frame.render_widget(&self.transient_toaster, frame.area());
		frame.render_widget(&self.sticky_toaster, frame.area());
	}

	fn render_header(&mut self, frame: &mut Frame, area: Rect) {
		let block = Block::default()
			.borders(Borders::ALL)
			.title(" © 2026 ComfyHome™ ")
			.border_style(Style::default().fg(Color::Cyan));
		let inner = block.inner(area);
		frame.render_widget(block, area);
		self.render_header_contact(frame, area);

		let logo = self.logo.render(inner.height);
		let header = choose_header_content(inner.width, logo.width(), &format!("v{}", APP_VERSION));

		if header.show_logo() {
			let chunks = Layout::default()
				.direction(Direction::Horizontal)
				.constraints([
					Constraint::Fill(1),
					Constraint::Length(header.logo_margin()),
					Constraint::Length(logo.width()),
					Constraint::Length(header.logo_gap()),
					Constraint::Length(header.banner().width()),
					Constraint::Fill(1),
				])
				.flex(Flex::Center)
				.split(inner);

			let logo_area = center_vertically(chunks[2], logo.lines().len() as u16);
			frame.render_widget(Paragraph::new(logo.lines().to_vec()), logo_area);

			let banner_area = center_vertically(chunks[4], header.banner().lines().len() as u16);
			frame.render_widget(Paragraph::new(header.banner().lines().to_vec()), banner_area);
		} else {
			let chunks = Layout::default()
				.direction(Direction::Horizontal)
				.constraints([
					Constraint::Fill(1),
					Constraint::Length(header.banner().width()),
					Constraint::Fill(1),
				])
				.flex(Flex::Center)
				.split(inner);
			let banner_area = center_vertically(chunks[1], header.banner().lines().len() as u16);
			frame.render_widget(Paragraph::new(header.banner().lines().to_vec()), banner_area);
		}
	}

	fn render_header_contact(&self, frame: &mut Frame, area: Rect) {
		if area.width <= SUPPORT_EMAIL.len() as u16 + 4 {
			return;
		}

		let contact_width = SUPPORT_EMAIL.len() as u16;
		let contact_area = Rect {
			x: area.x + area.width.saturating_sub(contact_width + 2),
			y: area.y,
			width: contact_width,
			height: 1,
		};
		frame.render_widget(
			Paragraph::new(SUPPORT_EMAIL).style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
			contact_area,
		);
	}

	fn render_nav(&mut self, frame: &mut Frame, area: Rect) {
		if area.width == 0 || area.height == 0 {
			return;
		}

		let labels = self.main_tab_labels();
		let widths = labels
			.iter()
			.map(|label| (label.chars().count() as u16 + 6).max(14))
			.collect::<Vec<_>>();

		let active_index = self.current_main_tab_index();
		let border_rect = Rect {
			x: area.x,
			y: area.y + area.height.saturating_sub(1),
			width: area.width,
			height: 1,
		};
		frame.render_widget(
			Paragraph::new("─".repeat(area.width as usize)).style(Style::default().fg(Color::DarkGray)),
			border_rect,
		);

		let left_rect = Rect { x: area.x, y: area.y, width: widths[0].min(area.width), height: area.height };
		self.render_main_nav_tab(frame, left_rect, &labels[0], active_index == 0);
		self.hit_targets.push(HitTarget::new(left_rect, HitAction::Switch(main_screen_from_index(0))));

		let right_total_width = widths.iter().skip(1).copied().sum::<u16>();
		let mut current_x = area.x + area.width.saturating_sub(right_total_width);
		for index in 1..labels.len() {
			let width = widths[index].min(area.x + area.width - current_x);
			if width == 0 {
				continue;
			}
			let rect = Rect {
				x: current_x,
				y: area.y,
				width,
				height: area.height,
			};
			self.render_main_nav_tab(frame, rect, &labels[index], active_index == index);
			self.hit_targets.push(HitTarget::new(rect, HitAction::Switch(main_screen_from_index(index))));
			current_x = current_x.saturating_add(widths[index]);
		}
	}

	fn render_main_nav_tab(&self, frame: &mut Frame, area: Rect, label: &str, selected: bool) {
		if area.width == 0 || area.height == 0 {
			return;
		}

		let border_style = if selected {
			Style::default().fg(Color::Cyan)
		} else {
			Style::default().fg(Color::DarkGray)
		};
		let label_style = if selected {
			Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
		} else {
			Style::default().fg(Color::White)
		};
		let block = Block::default()
			.borders(Borders::TOP | Borders::LEFT | Borders::RIGHT)
			.border_style(border_style);
		let inner = block.inner(area);
		frame.render_widget(block, area);
		frame.render_widget(Paragraph::new(label).alignment(Alignment::Center).style(label_style), inner);

		if selected && area.width > 2 {
			let clear_rect = Rect {
				x: area.x + 1,
				y: area.y + area.height.saturating_sub(1),
				width: area.width.saturating_sub(2),
				height: 1,
			};
			frame.render_widget(Paragraph::new(" ".repeat(clear_rect.width as usize)), clear_rect);
		}
	}

	pub(crate) fn scope_repo_roots(&self, project: &ProjectConfig, scope_count: usize) -> Vec<Option<String>> {
		if project.integration_mode.requires_repo() {
			match collect_all_branch_git_scope_contexts(project) {
				Ok(contexts) => (0..scope_count)
					.map(|index| contexts.get(index).map(|context| context.repo_root.clone()))
					.collect(),
				Err(_) => vec![None; scope_count],
			}
		} else {
			vec![None; scope_count]
		}
	}

	fn render_dashboard(&mut self, frame: &mut Frame, area: Rect) {
		let chunks = Layout::default()
			.direction(Direction::Horizontal)
			.constraints([Constraint::Length(38), Constraint::Min(40)])
			.split(area);

		let left_block = Block::default()
			.borders(Borders::ALL)
			.title(" Projects ")
			.border_style(if self.dashboard_focus == DashboardPane::Projects {
				Style::default().fg(Color::Cyan)
			} else {
				Style::default()
			});
		let left_inner = left_block.inner(chunks[0]);
		frame.render_widget(left_block, chunks[0]);
		if self.config.projects.is_empty() {
			let onboarding = vec![
				Line::from("No projects have been configured yet.".bold()),
				Line::from(""),
				Line::from("Press N or click New Project to start onboarding."),
				Line::from("This first slice stores config in your user profile."),
				Line::from("Branched projects can now manage multiple scopes in the wizard."),
				Line::from("Target validation reads JSON or TOML and checks a key path."),
			];
			frame.render_widget(Paragraph::new(onboarding).wrap(Wrap { trim: false }), left_inner);
		} else {
			let items = self
				.config
				.projects
				.iter()
				.map(|project| ListItem::new(vec![
					Line::from(project.name.clone()).style(Style::default().add_modifier(Modifier::BOLD)),
					Line::from(project.summary()).style(Style::default().fg(Color::Indexed(240))),
				]))
				.collect::<Vec<_>>();
			let mut state = ListState::default();
			state.select(Some(self.selected_project.min(self.config.projects.len().saturating_sub(1))));
			let list = List::new(items)
				.highlight_style(Style::default().bg(Color::Blue).fg(Color::Black))
				.highlight_symbol("> ");
			frame.render_stateful_widget(list, left_inner, &mut state);

			let row_height = 2_u16;
			for (index, _) in self.config.projects.iter().enumerate() {
				let rect = Rect {
					x: left_inner.x,
					y: left_inner.y + index as u16 * row_height,
					width: left_inner.width,
					height: row_height,
				};
				if rect.y < left_inner.y + left_inner.height {
					self.hit_targets.push(HitTarget::new(rect, HitAction::SelectProject(index)));
				}
			}
		}

		let right_sections = Layout::default()
			.direction(Direction::Vertical)
			.constraints([Constraint::Length(3), Constraint::Min(8)])
			.split(chunks[1]);
		self.overview_show_recent_tab = self.should_use_recent_changes_tab(right_sections[1]);
		if !self.overview_show_recent_tab && self.overview_tab == OverviewTab::RecentChanges {
			self.overview_tab = OverviewTab::Overview;
		}
		render_overview_tabs(frame, right_sections[0], self.overview_tab, self.overview_show_recent_tab);
		for (tab, rect) in overview_tab_rects(right_sections[0], self.overview_show_recent_tab) {
			self.hit_targets.push(HitTarget::new(rect, HitAction::SelectOverviewTab(tab)));
		}

		let overview_body = Block::default()
			.borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
			.title(" Overview ")
			.border_style(if self.dashboard_focus == DashboardPane::Overview {
				Style::default().fg(Color::Cyan)
			} else {
				Style::default()
			});
		let overview_inner = overview_body.inner(right_sections[1]);
		frame.render_widget(overview_body, right_sections[1]);

		if self.overview_tab == OverviewTab::ProjectDetail {
			let lines = if let Some(project) = self.config.projects.get(self.selected_project) {
				let mut lines = project
					.detail_lines()
					.into_iter()
					.map(Line::from)
					.collect::<Vec<_>>();
				lines.push(Line::raw(""));
				lines.push(Line::from("Available actions:".yellow().bold()));
				lines.push(Line::from("- B opens a bump preview for the selected project"));
				if project.integration_mode.requires_repo() {
					lines.push(Line::from("- G opens the git log from the configured repo"));
					if project.changelog_enabled_for_scope(self.overview_focused_scope) {
						lines.push(Line::from("- C opens a changelog preview using the current commit history"));
					}
					lines.push(Line::from("- T creates a local tag in the configured repo"));
				} else {
					lines.push(Line::from("- git actions unlock once the project is git-backed"));
				}
				lines.push(Line::from("- the current slice writes JSON and TOML targets"));
				lines
			} else {
				vec![
					Line::from("Supported schemes:".bold()),
					Line::from("- SemVer MAJOR.MINOR.PATCH → e.g. 1.4.2"),
					Line::from("- CalVer YYYY.MM.Micro → e.g. 2024.06.3"),
					Line::from("- CalVer YY.MM.Micro → e.g. 24.06.3"),
					Line::from("- CalVer YYYY.MM.DD.Micro → e.g. 2024.06.15.3"),
					Line::from("- Hybrid YYYY.MINOR.PATCH → e.g. 2024.4.2"),
					Line::from("- Hybrid YYYY.PATCH → e.g. 2024.2"),
				]
			};
			frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), overview_inner);
		} else if self.overview_tab == OverviewTab::ProjectSettings {
			super::p_s_s::render_project_settings(self, frame, overview_inner);
		} else {
			self.render_dashboard_overview(frame, overview_inner);
		}
	}

	#[allow(dead_code)]

	fn render_dashboard_overview(&mut self, frame: &mut Frame, area: Rect) {
		overview::render_dashboard_overview(self, frame, area);
	}

	fn should_use_recent_changes_tab(&self, area: Rect) -> bool {
		overview::should_use_recent_changes_tab(self, area)
	}

	fn render_bump_dialog(&mut self, frame: &mut Frame, area: Rect) {
		let Some(dialog) = &self.bump_dialog else {
			return;
		};

		let popup = centered_rect(area, 74, 58);
		frame.render_widget(Clear, popup);
		let block = Block::default().borders(Borders::ALL).title(" Bump Preview ").border_style(Style::default().fg(Color::Cyan));
		let inner = block.inner(popup);
		frame.render_widget(block, popup);

		let sections = Layout::default()
			.direction(Direction::Vertical)
			.constraints([
				Constraint::Length(7),
				Constraint::Min(6),
				Constraint::Length(3),
			])
			.split(inner);

		let next_version = dialog.preview_next_version();
		let mut summary = vec![
			Line::from(format!("Project: {}", dialog.project_name)).bold(),
			Line::from(format!(
				"Mode: {}",
				if dialog.unified_versioning {
					"Project-wide synchronized"
				} else {
					"Selected scope only"
				}
			)),
			Line::from(format!("Scheme: {}", dialog.active_scheme().display_name())),
			Line::from(format!("Current version: {}", dialog.current_version_label())),
			Line::from(format!("Action: < {} >", dialog.selected_action().display_name())).style(Style::default().fg(Color::Yellow)),
		];
		if dialog.can_select_scope() {
			let scope = dialog.active_scope();
			summary.push(Line::from(format!(
				"Selected scope: < {} ({}) >",
				scope.display_name,
				scope.scope_kind.map(|kind| kind.display_name()).unwrap_or("Project")
			)));
		}
		match next_version {
			Ok(next_version) => summary.push(Line::from(format!("Next version: {}", next_version)).style(Style::default().fg(Color::Green).add_modifier(Modifier::BOLD))),
			Err(error) => summary.push(Line::from(format!("Next version: {}", error)).style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))),
		}
		summary.push(Line::from(if dialog.can_select_scope() {
			"Left/Right changes the action. Up/Down changes scope. Enter applies to the selected scope."
		} else {
			"Left/Right changes the action. Enter applies it to every listed target."
		}));
		frame.render_widget(Paragraph::new(summary).wrap(Wrap { trim: false }), sections[0]);

		let target_lines = dialog
			.scopes
			.iter()
			.enumerate()
			.flat_map(|(index, scope)| {
				let marker = if dialog.can_select_scope() && index == dialog.selected_scope {
					">"
				} else {
					"-"
				};
				let header_style = if scope.has_mismatch() {
					Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
				} else if dialog.can_select_scope() && index == dialog.selected_scope {
					Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
				} else {
					Style::default().add_modifier(Modifier::BOLD)
				};
				std::iter::once(
					Line::from(format!(
						"{} {} ({}) -> {}",
						marker,
						scope.display_name,
						scope.scope_kind.map(|kind| kind.display_name()).unwrap_or("Project"),
						scope.version_label()
					))
					.style(header_style),
				)
				.chain(scope.targets.iter().map(|target| {
					Line::from(format!(
						"  {}: {} -> {} [{}]",
						target.label,
						target.path,
						target.key_path,
						target.format.display_name()
					))
				}))
			})
			.collect::<Vec<_>>();
		let target_block = Block::default().borders(Borders::ALL).title(" Targets ");
		let target_inner = target_block.inner(sections[1]);
		frame.render_widget(target_block, sections[1]);
		frame.render_widget(Paragraph::new(target_lines).wrap(Wrap { trim: false }), target_inner);

		let mut buttons = Vec::new();
		if dialog.can_select_scope() {
			buttons.push(DialogButton::new(
				format!("Scope: < {} >", dialog.active_scope().display_name),
				false,
				HitAction::CycleBumpScope(1),
				Style::default().fg(Color::Black).bg(Color::Rgb(140, 220, 180)),
			));
		}
		buttons.push(DialogButton::new(
			format!("< {} >", dialog.selected_action().display_name()),
			false,
			HitAction::CycleBumpAction(1),
			Style::default().fg(Color::Black).bg(Color::Yellow),
		));
		buttons.push(DialogButton::new("Apply", false, HitAction::ApplyBump, Style::default().fg(Color::Black).bg(Color::Green)));
		buttons.push(DialogButton::new("Cancel", false, HitAction::CancelBump, Style::default().fg(Color::White).bg(Color::Red)));
		self.render_button_row(frame, sections[2], &buttons);
	}

	fn render_overview_bump_workflow_dialog(&mut self, frame: &mut Frame, area: Rect) {
		let Some(dialog) = &self.overview_bump_workflow_dialog else {
			return;
		};

		let popup = centered_rect(area, 78, 46);
		frame.render_widget(Clear, popup);
		let block = Block::default()
			.borders(Borders::ALL)
			.title(" Bump Action ")
			.border_style(Style::default().fg(Color::Cyan));
		let inner = block.inner(popup);
		frame.render_widget(block, popup);

		let sections = Layout::default()
			.direction(Direction::Vertical)
			.constraints([Constraint::Length(5), Constraint::Min(9), Constraint::Length(BUTTON_ROW_HEIGHT)])
			.split(inner);

		let header = vec![
			Line::from(format!("Project: {}", dialog.project_name)).bold(),
			Line::from(format!("Scope: {}", dialog.scope_label)),
			Line::from(format!("Next version: {}", dialog.next_version)).style(Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
			Line::from("What would you like to do?"),
		];
		frame.render_widget(Paragraph::new(header).wrap(Wrap { trim: false }), sections[0]);

		let option_rows = Layout::default()
			.direction(Direction::Vertical)
			.constraints(vec![Constraint::Length(3); dialog.options.len()])
			.split(sections[1]);

		for (index, (option, row)) in dialog.options.iter().zip(option_rows.iter()).enumerate() {
			let selected = index == dialog.selected;
			let row_block = Block::default().borders(Borders::ALL).border_style(if selected {
				Style::default().fg(Color::Cyan)
			} else {
				Style::default().fg(Color::DarkGray)
			});
			let row_inner = row_block.inner(*row);
			frame.render_widget(row_block, *row);
			let lines = vec![
				Line::from(format!("{}. {}", index + 1, option.display_name())).style(if selected {
					Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
				} else {
					Style::default().add_modifier(Modifier::BOLD)
				}),
				Line::from(option.description()),
			];
			frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), row_inner);
			self.hit_targets.push(HitTarget::new(*row, HitAction::SelectOverviewBumpWorkflow(index)));
		}

		self.render_button_row(
			frame,
			sections[2],
			&[
				DialogButton::new("Run", false, HitAction::ConfirmOverviewBumpWorkflow, Style::default().fg(Color::Black).bg(Color::Green)),
				DialogButton::new("Cancel", false, HitAction::CancelOverviewBumpWorkflow, Style::default().fg(Color::White).bg(Color::Red)),
			],
		);
	}

	fn render_overview_bump_warning_dialog(&mut self, frame: &mut Frame, area: Rect) {
		let Some(dialog) = &self.overview_bump_warning_dialog else {
			return;
		};

		let popup = centered_rect(area, 82, 54);
		frame.render_widget(Clear, popup);
		let block = Block::default()
			.borders(Borders::ALL)
			.title(" Staged Files Warning ")
			.border_style(Style::default().fg(Color::Yellow));
		let inner = block.inner(popup);
		frame.render_widget(block, popup);

		let sections = Layout::default()
			.direction(Direction::Vertical)
			.constraints([Constraint::Length(4), Constraint::Min(10), Constraint::Length(BUTTON_ROW_HEIGHT)])
			.split(inner);

		let header = vec![
			Line::from("Previously staged files will be included in the bump commit.")
				.style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
			Line::from("Choose whether to continue, unstage the unrelated files, or cancel this bump."),
			Line::from(format!("Action: {}", dialog.workflow.display_name())),
		];
		frame.render_widget(Paragraph::new(header).wrap(Wrap { trim: false }), sections[0]);

		let mut lines = Vec::new();
		for repo in &dialog.repos {
			lines.push(Line::from(format!("Repo: {}", repo.repo_root)).style(Style::default().add_modifier(Modifier::BOLD)));
			for path in &repo.extra_paths {
				lines.push(Line::from(format!("  - {}", path)));
			}
			lines.push(Line::raw(""));
		}
		frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), sections[1]);

		self.render_button_row(
			frame,
			sections[2],
			&[
				DialogButton::new(
					"Continue",
					dialog.selected == 0,
					HitAction::SelectOverviewBumpWarningChoice(0),
					Style::default().fg(Color::Black).bg(Color::Yellow),
				),
				DialogButton::new(
					"Unstage Extras",
					dialog.selected == 1,
					HitAction::SelectOverviewBumpWarningChoice(1),
					Style::default().fg(Color::Black).bg(Color::Rgb(140, 220, 180)),
				),
				DialogButton::new(
					"Cancel",
					dialog.selected == 2,
					HitAction::SelectOverviewBumpWarningChoice(2),
					Style::default().fg(Color::White).bg(Color::Red),
				),
			],
		);
	}

	fn render_main_branch_warning_dialog(&mut self, frame: &mut Frame, area: Rect) {
		let Some(dialog) = &self.main_branch_warning_dialog else {
			return;
		};

		let popup = centered_rect(area, 84, 52);
		frame.render_widget(Clear, popup);
		let block = Block::default()
			.borders(Borders::ALL)
			.title(" Main Branch Warning ")
			.border_style(Style::default().fg(Color::Yellow));
		let inner = block.inner(popup);
		frame.render_widget(block, popup);

		let sections = Layout::default()
			.direction(Direction::Vertical)
			.constraints([Constraint::Length(4), Constraint::Min(8), Constraint::Length(BUTTON_ROW_HEIGHT)])
			.split(inner);

		let header = vec![
			Line::from("It seems like you are not on main branch. Please, choose what would you like to do...")
				.style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
			Line::from("The bump can switch the affected repository to main first, or continue on the current branch."),
		];
		frame.render_widget(Paragraph::new(header).wrap(Wrap { trim: false }), sections[0]);

		let mut lines = Vec::new();
		for repo in &dialog.repos {
			lines.push(
				Line::from(format!("Repo: {}", repo.repo_root)).style(Style::default().add_modifier(Modifier::BOLD)),
			);
			lines.push(Line::from(format!("Current branch: {}", repo.current_branch)));
			lines.push(Line::raw(""));
		}
		frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), sections[1]);

		self.render_button_row(
			frame,
			sections[2],
			&[
				DialogButton::new(
					dialog.switch_label(),
					dialog.selected == 0,
					HitAction::SelectMainBranchWarningChoice(0),
					Style::default().fg(Color::Black).bg(Color::Yellow),
				),
				DialogButton::new(
					"Ignore & Continue",
					dialog.selected == 1,
					HitAction::SelectMainBranchWarningChoice(1),
					Style::default().fg(Color::Black).bg(Color::Rgb(140, 220, 180)),
				),
				DialogButton::new(
					"Cancel",
					dialog.selected == 2,
					HitAction::SelectMainBranchWarningChoice(2),
					Style::default().fg(Color::White).bg(Color::Red),
				),
			],
		);
	}

	fn render_changelog_preview_dialog(&mut self, frame: &mut Frame, area: Rect) {
		let Some(dialog) = &self.changelog_preview_dialog else {
			return;
		};

		let popup = centered_rect(area, 88, 78);
		frame.render_widget(Clear, popup);
		let block = Block::default()
			.borders(Borders::ALL)
			.title(" Changelog Preview ")
			.border_style(Style::default().fg(Color::Cyan));
		let inner = block.inner(popup);
		frame.render_widget(block, popup);

		let sections = Layout::default()
			.direction(Direction::Vertical)
			.constraints(if dialog.workflow.is_some() {
				[Constraint::Length(4), Constraint::Length(8), Constraint::Min(8), Constraint::Length(BUTTON_ROW_HEIGHT)]
			} else {
				[Constraint::Length(4), Constraint::Length(0), Constraint::Min(15), Constraint::Length(BUTTON_ROW_HEIGHT)]
			})
			.split(inner);

		let mut header = vec![
			Line::from(format!("Project: {}", dialog.project_name)).bold(),
			Line::from(format!("Version: {}", dialog.next_version)),
		];
		if let Some(workflow) = dialog.workflow {
			header.push(Line::from(format!("Workflow: {}", workflow.display_name())));
			header.push(Line::from("Review the generated changelog below. Edit the optional multi-line release notes in Markdown, save to repo root if needed, then confirm to write and stage the changelog file."));
		} else {
			header.push(Line::from("Preview mode: current git history rendered as the changelog."));
			header.push(Line::from("Press Ctrl+S or Save to write changelog_temp.md in each repo root, or Enter/F2/Close to dismiss this preview."));
		}
		frame.render_widget(Paragraph::new(header).wrap(Wrap { trim: false }), sections[0]);

		if dialog.workflow.is_some() {
			self.render_textarea_editor(
				frame,
				sections[1],
				" Release Notes ",
				dialog.release_message_placeholder.as_str(),
				&dialog.release_message,
			);
		}

		let body_block = Block::default().borders(Borders::ALL).title(" Preview ");
		let body_inner = body_block.inner(sections[2]);
		frame.render_widget(body_block, sections[2]);
		let preview_markdown = dialog.combined_preview_markdown();
		let body = tui_markdown::from_str(&preview_markdown);
		frame.render_widget(
			Paragraph::new(body)
				.wrap(Wrap { trim: false })
				.scroll((dialog.scroll, 0)),
			body_inner,
		);

		self.render_button_row(
			frame,
			sections[3],
			&[
				DialogButton::new(if dialog.workflow.is_some() { "Continue" } else { "Close" }, false, HitAction::ConfirmChangelogPreview, Style::default().fg(Color::Black).bg(Color::Green)),
				DialogButton::new("Save", false, HitAction::SaveChangelogPreview, Style::default().fg(Color::Black).bg(Color::Rgb(140, 220, 180))),
				DialogButton::new("Scroll", false, HitAction::ScrollChangelogPreview(3), Style::default().fg(Color::Black).bg(Color::Yellow)),
				DialogButton::new(if dialog.workflow.is_some() { "Cancel" } else { "Back" }, false, HitAction::CancelChangelogPreview, Style::default().fg(Color::White).bg(Color::Red)),
			],
		);
	}
	fn render_recent_changes_dialog(&mut self, frame: &mut Frame, area: Rect) {
		let Some(dialog) = &self.recent_changes_dialog else {
			return;
		};

		let popup = centered_rect(area, 82, 72);
		frame.render_widget(Clear, popup);
		let block = Block::default().borders(Borders::ALL).title(" Git Commits ").border_style(Style::default().fg(Color::Cyan));
		let inner = block.inner(popup);
		frame.render_widget(block, popup);

		let sections = Layout::default()
			.direction(Direction::Vertical)
			.constraints([Constraint::Length(4), Constraint::Length(3), Constraint::Min(8), Constraint::Length(3)])
			.split(inner);

		let header = vec![
			Line::from(format!("Project: {}", dialog.project_name)).bold(),
			Line::from(format!(
				"Scope: {} ({})",
				dialog.active_scope().display_name,
				dialog.active_scope().scope_kind.map(|kind| kind.display_name()).unwrap_or("Project")
			)),
			Line::from(format!("Repo: {}", dialog.active_scope().repo_root)),
			Line::from(format!("View: {}", dialog.current_range().label)),
			Line::from(if dialog.can_select_scope() {
				"Tab switches view. Left/Right changes scope only on Recent. In History, Left/Right browses tag windows. [ and ] still change scope. R refreshes the current scope."
			} else {
				"Tab switches view. Left/Right moves history when History is active. R refreshes the current scope."
			}),
		];
		frame.render_widget(Paragraph::new(header).wrap(Wrap { trim: false }), sections[0]);

		let tab_labels = ["Recent Changes", "History"];
		let tab_index = if dialog.active_tab == RecentChangesTab::Recent { 0 } else { 1 };
		let tabs = TabNav::new(&tab_labels, tab_index)
			.highlight_style(Style::default().fg(Color::Cyan))
			.border_style(Style::default().fg(Color::DarkGray))
			.style(Style::default().fg(Color::White))
			.indicator(None);
		frame.render_widget(tabs, sections[1]);

		let tab_layout = Layout::default()
			.direction(Direction::Horizontal)
			.constraints([Constraint::Length(22), Constraint::Length(14)])
			.split(sections[1]);
		self.hit_targets.push(HitTarget::new(tab_layout[0], HitAction::SelectRecentChangesTab(RecentChangesTab::Recent)));
		self.hit_targets.push(HitTarget::new(tab_layout[1], HitAction::SelectRecentChangesTab(RecentChangesTab::History)));

		let body_block = Block::default().borders(Borders::ALL).title(" git log ");
		let body_inner = body_block.inner(sections[2]);
		frame.render_widget(body_block, sections[2]);
		let graph_base_column = git_graph_base_column(&dialog.current_range().lines);
		let body = if dialog.current_range().lines.is_empty() {
			vec![Line::from(if dialog.active_tab == RecentChangesTab::History {
				"No history range is available yet."
			} else {
				"No recent changes to display."
			})]
		} else {
			dialog
				.current_range()
				.lines
				.iter()
				.map(|line| colorize_git_log_line(line, graph_base_column))
				.collect::<Vec<_>>()
		};
		frame.render_widget(
			Paragraph::new(body)
				.wrap(Wrap { trim: false })
				.scroll((dialog.scroll, 0)),
			body_inner,
		);

		let mut buttons = Vec::new();
		if dialog.can_select_scope() {
			buttons.push(DialogButton::new(
				format!("Scope: < {} >", dialog.active_scope().display_name),
				false,
				HitAction::CycleRecentChangesScope(1),
				Style::default().fg(Color::Black).bg(Color::Rgb(140, 220, 180)),
			));
		}
		buttons.push(DialogButton::new("Scroll", false, HitAction::ScrollRecentChanges(3), Style::default().fg(Color::Black).bg(Color::Yellow)));
		buttons.push(DialogButton::new("Create Tag", false, HitAction::OpenTagDialog, Style::default().fg(Color::Black).bg(Color::Green)));
		buttons.push(DialogButton::new("Close", false, HitAction::CloseRecentChanges, Style::default().fg(Color::White).bg(Color::Red)));
		self.render_button_row(frame, sections[3], &buttons);
	}

	fn render_tag_dialog(&mut self, frame: &mut Frame, area: Rect) {
		let Some(dialog) = &self.tag_dialog else {
			return;
		};

		let popup = centered_rect(area, 74, 42);
		frame.render_widget(Clear, popup);
		let block = Block::default().borders(Borders::ALL).title(" Create Tag ").border_style(Style::default().fg(Color::Cyan));
		let inner = block.inner(popup);
		frame.render_widget(block, popup);

		let sections = Layout::default()
			.direction(Direction::Vertical)
			.constraints([Constraint::Length(4), Constraint::Length(3), Constraint::Min(8), Constraint::Length(BUTTON_ROW_HEIGHT)])
			.split(inner);

		let header = vec![
			Line::from(format!("Project: {}", dialog.project_name)).bold(),
			Line::from(format!(
				"Scope: {} ({})",
				dialog.active_scope().display_name,
				dialog.active_scope().scope_kind.map(|kind| kind.display_name()).unwrap_or("Project")
			)),
			Line::from(format!("Repo: {}", dialog.active_scope().repo_root)),
			Line::from(format!("Action: < {} >", dialog.selected_action().display_name())).style(Style::default().fg(Color::Yellow)),
			Line::from(if dialog.can_select_scope() {
				"Edit the tag name, add an optional annotation, then run the selected action. [ and ] change scope."
			} else {
				"Edit the tag name, add an optional annotation, then run the selected action."
			}),
		];
		frame.render_widget(Paragraph::new(header).wrap(Wrap { trim: false }), sections[0]);

		let input_row = Layout::default()
			.direction(Direction::Horizontal)
			.constraints([Constraint::Length(20), Constraint::Min(10)])
			.split(sections[1]);
		frame.render_widget(Paragraph::new("Tag name"), input_row[0]);
		let input_block = Block::default()
			.borders(Borders::ALL)
			.title(" value ")
			.border_style(Style::default().fg(Color::Cyan));
		frame.render_widget(
			Paragraph::new(dialog.tag_name.display_value(true))
				.block(input_block)
				.style(Style::default().fg(Color::White)),
			input_row[1],
		);

		let notes = vec![
			Line::from(format!("Suggested tag: {}", dialog.active_scope().suggested_tag_name)),
			Line::from(if dialog.annotation.trim().is_empty() {
				"Annotation: none"
			} else {
				"Annotation: attached"
			}),
			Line::from(match dialog.selected_action() {
				TagAction::CreateLocal => "Creates a local tag only.",
				TagAction::CreateAndPush => "Creates the local tag if needed, then pushes it.",
				TagAction::CreatePushAndRelease => "Creates the tag, pushes it, then publishes a GitHub release with CVB-generated notes.",
			}),
		];
		frame.render_widget(Paragraph::new(notes).wrap(Wrap { trim: false }), sections[2]);

		let mut buttons = Vec::new();
		if dialog.can_select_scope() {
			buttons.push(DialogButton::new(
				format!("Scope: < {} >", dialog.active_scope().display_name),
				false,
				HitAction::CycleTagScope(1),
				Style::default().fg(Color::Black).bg(Color::Rgb(140, 220, 180)),
			));
		}
		buttons.push(DialogButton::new(
			format!("< {} >", dialog.selected_action().display_name()),
			false,
			HitAction::CycleTagAction(1),
			Style::default().fg(Color::Black).bg(Color::Yellow),
		));
		buttons.push(DialogButton::new(
			if dialog.annotation.trim().is_empty() {
				"Annotation"
			} else {
				"Annotation Added"
			},
			false,
			HitAction::OpenTagAnnotation,
			Style::default().fg(Color::Black).bg(Color::Rgb(140, 220, 180)),
		));
		buttons.push(DialogButton::new("Run", false, HitAction::CreateTag, Style::default().fg(Color::Black).bg(Color::Green)));
		buttons.push(DialogButton::new("Cancel", false, HitAction::CancelTagDialog, Style::default().fg(Color::White).bg(Color::Red)));
		self.render_button_row(frame, sections[3], &buttons);
	}

	fn render_ui_settings(&mut self, frame: &mut Frame, area: Rect) {
		let block = Block::default().borders(Borders::ALL).title(" UI Settings ");
		let inner = block.inner(area);
		frame.render_widget(block, area);

		let sections = Layout::default()
			.direction(Direction::Vertical)
			.constraints([Constraint::Min(8), Constraint::Length(BUTTON_ROW_HEIGHT)])
			.split(inner);

		let lines = vec![
			Line::from("Adjust interface preferences for the current config.".bold()),
			Line::raw(""),
			Line::from(format!(
				"Tab hints: {}",
				if self.config.ui.show_tab_hints { "visible" } else { "hidden" }
			)),
			Line::from(format!(
				"Footer: {}",
				if self.config.ui.hide_footer { "hidden" } else { "visible" }
			)),
			Line::from(format!("Footer content: {}", self.config.ui.footer_content.display_name())),
			Line::raw(""),
			Line::from("When enabled, the main tabs show [1]..[4] hints."),
			Line::from("T, Enter, or Space toggles tab hints."),
			Line::from("C, Left, or Right changes footer content alignment."),
			Line::from("H toggles footer visibility."),
		];
		frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), sections[0]);

		self.render_button_row(
			frame,
			sections[1],
			&[
				DialogButton::new(
					if self.config.ui.show_tab_hints { "Hide Tab Hints" } else { "Show Tab Hints" },
					true,
					HitAction::ToggleTabHints,
					Style::default().fg(Color::Black).bg(Color::Rgb(140, 220, 180)),
				),
				DialogButton::new(
					format!("Footer Content: < {} >", self.config.ui.footer_content.display_name()),
					false,
					HitAction::CycleFooterContent(1),
					Style::default().fg(Color::Black).bg(Color::Rgb(180, 205, 255)),
				),
				DialogButton::new(
					if self.config.ui.hide_footer { "Show Footer" } else { "Hide Footer" },
					false,
					HitAction::ToggleFooter,
					Style::default().fg(Color::Black).bg(Color::Yellow),
				),
			],
		);
	}

	fn render_progress_dialog(&mut self, frame: &mut Frame, area: Rect) {
		let Some(dialog) = &self.progress_dialog else {
			return;
		};

		let popup = centered_rect(area, 56, 18);
		frame.render_widget(Clear, popup);
		let block = Block::default()
			.borders(Borders::ALL)
			.title(format!(" {} ", dialog.title))
			.border_style(Style::default().fg(Color::Yellow));
		let inner = block.inner(popup);
		frame.render_widget(block, popup);

		let lines = vec![
			Line::from(dialog.message.clone()).bold(),
			Line::raw(""),
			Line::from("The interface will update when this step completes.")
				.style(Style::default().fg(Color::Gray)),
		];
		frame.render_widget(
			Paragraph::new(lines)
				.alignment(Alignment::Center)
				.wrap(Wrap { trim: false }),
			inner,
		);
	}

	fn render_project_edit_dialog(&mut self, frame: &mut Frame, area: Rect) {
		let Some(_) = &self.project_edit_dialog else {
			return;
		};

		let popup = centered_rect(area, 78, 64);
		frame.render_widget(Clear, popup);
		let block = Block::default().borders(Borders::ALL).title(" Edit Project ").border_style(Style::default().fg(Color::Cyan));
		let inner = block.inner(popup);
		frame.render_widget(block, popup);

		let sections = Layout::default()
			.direction(Direction::Vertical)
			.constraints([
				Constraint::Length(4),
				Constraint::Min(6),
				Constraint::Length(BUTTON_GAP_HEIGHT),
				Constraint::Length(BUTTON_ROW_HEIGHT),
			])
			.split(inner);

		let (project_name, field_rows_data, show_above, show_below, save_focused, remove_focused, cancel_focused) = {
			let dialog = self.project_edit_dialog.as_mut().expect("dialog checked above");
			let (fields, row_height, top, bottom) = dialog.refresh_body_window(sections[1].height);
			let constraints = vec![Constraint::Length(row_height); fields.len()];
			let field_rows = Layout::default()
				.direction(Direction::Vertical)
				.constraints(constraints)
				.split(sections[1]);
			let rows = fields
				.iter()
				.zip(field_rows.iter())
				.map(|(field, row)| {
					let focused = *field == dialog.focus;
					let (label, action) = dialog.render_field(*field);
					let side_button = project_edit_form_row_button(*field);
					let value = dialog.display_value_for_field(*field, focused, visible_field_width(row.width, side_button.is_some()));
					(*row, label, action, side_button, focused, value)
				})
				.collect::<Vec<_>>();
			(
				dialog.project_name.clone(),
				rows,
				top,
				bottom,
				dialog.focus == ProjectEditFocus::Save,
				dialog.focus == ProjectEditFocus::Remove,
				dialog.focus == ProjectEditFocus::Cancel,
			)
		};

		let header = vec![
			Line::from(format!("Project: {}", project_name)).bold(),
			Line::from("Edit the same core fields as New Project, then press F2 or Save."),
			Line::from("Tab/Shift+Tab moves between fields. Left/Right changes enums. Enter applies scope action rows. Ctrl+O browses. PgUp/PgDn or wheel scrolls. Del removes the project."),
		];
		frame.render_widget(Paragraph::new(header).wrap(Wrap { trim: false }), sections[0]);

		for (row, label, action, side_button, focused, value) in field_rows_data {
			let button_rect = self.render_form_row(frame, row, label, value, focused, action.clone(), side_button.clone());
			self.hit_targets.push(HitTarget::new(row, action));
			if let (Some(rect), Some(button)) = (button_rect, side_button) {
				self.hit_targets.push(HitTarget::new(rect, button.action));
			}
		}
		render_vertical_overflow_indicators(frame, sections[1], show_above, show_below);

		self.render_button_row(
			frame,
			sections[3],
			&[
				DialogButton::new("Save", save_focused, HitAction::SaveProjectEdit, Style::default().fg(Color::Black).bg(Color::Green)),
				DialogButton::new("Remove", remove_focused, HitAction::RemoveProject, Style::default().fg(Color::White).bg(Color::Red)),
				DialogButton::new("Cancel", cancel_focused, HitAction::CancelProjectEdit, Style::default().fg(Color::Black).bg(Color::Rgb(230, 190, 90))),
			],
		);
	}

	fn render_delete_confirmation_dialog(&mut self, frame: &mut Frame, area: Rect) {
		let Some(dialog) = &self.delete_confirmation_dialog else {
			return;
		};

		let popup = centered_rect(area, 60, 28);
		frame.render_widget(Clear, popup);
		let block = Block::default()
			.borders(Borders::ALL)
			.title(" Confirm Delete ")
			.border_style(Style::default().fg(Color::Yellow));
		let inner = block.inner(popup);
		frame.render_widget(block, popup);

		let sections = Layout::default()
			.direction(Direction::Vertical)
			.constraints([Constraint::Min(6), Constraint::Length(BUTTON_ROW_HEIGHT)])
			.split(inner);

		let lines = match &dialog.target {
			DeleteConfirmationTarget::Project { project_name, .. } => vec![
				Line::from("Are you sure?".yellow().bold()),
				Line::from(format!("Project: {}", project_name)).bold(),
				Line::from("This removes the project from the saved config."),
				Line::from("Files, tags, and repositories on disk are not deleted."),
				Line::from("Use Left/Right or Tab to pick an option. Y confirms, N cancels."),
			],
			DeleteConfirmationTarget::Scope {
				project_name,
				scope_name,
				scope_kind,
				removes_project,
				..
			} => {
				let mut lines = vec![
					Line::from("Are you sure?".yellow().bold()),
					Line::from(format!("Project: {}", project_name)).bold(),
					Line::from(format!("Scope: {} ({})", scope_name, scope_kind.display_name())),
					Line::from("This removes the scope from the saved config."),
				];
				if *removes_project {
					lines.push(Line::from("It is the last remaining scope, so the whole project will also be removed.").style(Style::default().fg(Color::Yellow)));
				} else {
					lines.push(Line::from("Files, tags, and repositories on disk are not deleted."));
				}
				lines.push(Line::from("Use Left/Right or Tab to pick an option. Y confirms, N cancels."));
				lines
			}
		};
		frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), sections[0]);

		self.render_button_row(
			frame,
			sections[1],
			&[
				DialogButton::new(
					"Delete",
					dialog.confirm_selected,
					HitAction::ConfirmDeleteRequest,
					Style::default().fg(Color::White).bg(Color::Red),
				),
				DialogButton::new(
					"Cancel",
					!dialog.confirm_selected,
					HitAction::CancelDeleteRequest,
					Style::default().fg(Color::Black).bg(Color::Rgb(230, 190, 90)),
				),
			],
		);
	}

	fn render_release_now_dialog(&mut self, frame: &mut Frame, area: Rect) {
		let Some(dialog) = &self.release_now_dialog else {
			return;
		};

		let popup = centered_rect(area, 88, 76);
		frame.render_widget(Clear, popup);
		let block = Block::default()
			.borders(Borders::ALL)
			.title(" ReleaseNOW ")
			.border_style(Style::default().fg(Color::Cyan));
		let inner = block.inner(popup);
		frame.render_widget(block, popup);

		let sections = Layout::default()
			.direction(Direction::Vertical)
			.constraints([Constraint::Length(6), Constraint::Length(3), Constraint::Min(10), Constraint::Length(BUTTON_ROW_HEIGHT)])
			.split(inner);

		let mut header = vec![
			Line::from(format!("Project: {}", dialog.project_name)).bold(),
			Line::from(format!("Scope: {}", dialog.scope_label)),
			Line::from(format!("Repo: {}", dialog.repo_root)),
			Line::from(format!("Tag: {}", dialog.tag_name)).style(Style::default().fg(Color::Yellow)),
		];
		if dialog.is_warning_mode() {
			header.push(Line::from("The most recent bump does not look fresh enough for a confident release."));
			header.push(Line::from("Choose whether to continue anyway or cancel this release."));
		} else if dialog.is_running() {
			header.push(Line::from("ReleaseNOW is running now. Live stdout/stderr is streaming into the log pane below."));
			header.push(Line::from("Use the mouse wheel or PgUp/PgDn to review output. F toggles follow mode and X cancels the run."));
		} else if dialog.is_completed() {
			header.push(Line::from("ReleaseNOW finished. Review artifacts and logs below."));
			header.push(Line::from("Esc or Enter closes this dialog. Mouse wheel and PgUp/PgDn scroll the log."));
		} else {
			header.push(Line::from(format!(
				"Build: < {} > | Changelog: {}",
				dialog.selected_option().label,
				if dialog.attach_changelog { "Yes" } else { "No" }
			)));
			header.push(Line::from("Left/Right cycles build options. C toggles changelog. E edits notes. Enter or F2 runs ReleaseNOW."));
		}
		frame.render_widget(Paragraph::new(header).wrap(Wrap { trim: false }), sections[0]);

		if !dialog.is_warning_mode() {
			let config_line = if dialog.is_running() {
				format!(
					"Running: {} | Follow: {} | Cancel: {} | Live log lines: {}",
					dialog.selected_option().label,
					if dialog.auto_follow() { "On" } else { "Off" },
					if dialog.cancel_requested() { "requested" } else { "ready" },
					dialog.log_lines.len()
				)
			} else if dialog.is_completed() {
				format!("Artifacts: {}", dialog.artifact_files.len())
			} else {
				format!(
					"Selected: {} | Notes: {}",
					dialog.selected_option().label,
					if dialog.attach_changelog { "attached" } else { "disabled" }
				)
			};
			frame.render_widget(
				Paragraph::new(config_line)
					.alignment(Alignment::Center)
					.style(Style::default().fg(Color::Gray)),
				sections[1],
			);
		}

		let body_block = Block::default().borders(Borders::ALL).title(dialog.body_title());
		let body_inner = body_block.inner(sections[2]);
		if let Some(dialog) = &mut self.release_now_dialog {
			dialog.set_body_viewport_height(body_inner.height);
		}
		let dialog = self.release_now_dialog.as_ref().expect("ReleaseNOW dialog should stay open while rendering");
		frame.render_widget(body_block, sections[2]);
		frame.render_widget(
			Paragraph::new(dialog.rendered_body_lines())
				.wrap(Wrap { trim: false })
				.scroll((dialog.scroll_offset(), 0)),
			body_inner,
		);

		if dialog.is_warning_mode() {
			self.render_button_row(
				frame,
				sections[3],
				&[
					DialogButton::new(
						"Yes, I'm ready",
						dialog.warning_confirm_selected,
						HitAction::ContinueReleaseNowWarning,
						Style::default().fg(Color::Black).bg(Color::Yellow),
					),
					DialogButton::new(
						"OMG, no, cancel!",
						!dialog.warning_confirm_selected,
						HitAction::CloseReleaseNow,
						Style::default().fg(Color::White).bg(Color::Red),
					),
				],
			);
		} else if dialog.is_running() {
			self.render_button_row(
				frame,
				sections[3],
				&[
					DialogButton::new(
						format!("Follow: {}", if dialog.auto_follow() { "On" } else { "Off" }),
						false,
						HitAction::ToggleReleaseNowAutoFollow,
						Style::default().fg(Color::Black).bg(Color::Rgb(180, 205, 255)),
					),
					DialogButton::new("Scroll", false, HitAction::ScrollReleaseNow(3), Style::default().fg(Color::Black).bg(Color::Yellow)),
					DialogButton::new(
						if dialog.cancel_requested() { "Cancelling..." } else { "Cancel Run" },
						false,
						HitAction::CancelReleaseNowRun,
						Style::default().fg(Color::White).bg(Color::Red),
					),
				],
			);
		} else if dialog.is_completed() {
			self.render_button_row(
				frame,
				sections[3],
				&[
					DialogButton::new("Scroll", false, HitAction::ScrollReleaseNow(3), Style::default().fg(Color::Black).bg(Color::Yellow)),
					DialogButton::new("Close", true, HitAction::CloseReleaseNow, Style::default().fg(Color::White).bg(Color::Red)),
				],
			);
		} else {
			let mut buttons = vec![
				DialogButton::new(
					format!("Build: < {} >", dialog.selected_option().label),
					false,
					HitAction::CycleReleaseNowOption(1),
					Style::default().fg(Color::Black).bg(Color::Rgb(180, 205, 255)),
				),
				DialogButton::new(
					format!("Changelog: {}", if dialog.attach_changelog { "On" } else { "Off" }),
					false,
					HitAction::ToggleReleaseNowChangelog,
					Style::default().fg(Color::Black).bg(Color::Rgb(140, 220, 180)),
				),
			];
			if dialog.attach_changelog {
				buttons.push(DialogButton::new(
					"Edit Notes",
					false,
					HitAction::EditReleaseNowNotes,
					Style::default().fg(Color::Black).bg(Color::Rgb(230, 190, 90)),
				));
			}
			buttons.push(DialogButton::new("Run", false, HitAction::RunReleaseNow, Style::default().fg(Color::Black).bg(Color::Green)));
			buttons.push(DialogButton::new("Cancel", false, HitAction::CloseReleaseNow, Style::default().fg(Color::White).bg(Color::Red)));
			self.render_button_row(frame, sections[3], &buttons);
		}
	}

	fn render_release_now_notes_dialog(&mut self, frame: &mut Frame, area: Rect) {
		let Some(dialog) = &self.release_now_notes_dialog else {
			return;
		};

		let popup = centered_rect(area, 84, 62);
		frame.render_widget(Clear, popup);
		let block = Block::default()
			.borders(Borders::ALL)
			.title(" Edit Release Notes ")
			.border_style(Style::default().fg(Color::Cyan));
		let inner = block.inner(popup);
		frame.render_widget(block, popup);

		let sections = Layout::default()
			.direction(Direction::Vertical)
			.constraints([Constraint::Length(3), Constraint::Min(10), Constraint::Length(BUTTON_ROW_HEIGHT)])
			.split(inner);

		frame.render_widget(
			Paragraph::new(vec![
				Line::from("Edit the Markdown that will be attached to the GitHub release.").bold(),
				Line::from("Ctrl+S or F2 saves. Esc closes without saving."),
			])
			.wrap(Wrap { trim: false }),
			sections[0],
		);
		self.render_textarea_editor(frame, sections[1], " Release Notes ", dialog.placeholder.as_str(), &dialog.editor);
		self.render_button_row(
			frame,
			sections[2],
			&[
				DialogButton::new("Save", false, HitAction::SaveReleaseNowNotes, Style::default().fg(Color::Black).bg(Color::Green)),
				DialogButton::new("Cancel", false, HitAction::CancelReleaseNowNotes, Style::default().fg(Color::White).bg(Color::Red)),
			],
		);
	}

	fn render_wizard(&mut self, frame: &mut Frame, area: Rect) {
		let chunks = Layout::default()
			.direction(Direction::Horizontal)
			.constraints([Constraint::Percentage(58), Constraint::Percentage(42)])
			.split(area);

		let block = Block::default().borders(Borders::ALL).title(" New Project Wizard ");
		let inner = block.inner(chunks[0]);
		frame.render_widget(block, chunks[0]);

		let left_sections = Layout::default()
			.direction(Direction::Vertical)
			.constraints([
				Constraint::Min(6),
				Constraint::Length(BUTTON_GAP_HEIGHT),
				Constraint::Length(BUTTON_ROW_HEIGHT),
			])
			.split(inner);

		let (fields, row_height, show_above, show_below) = self.wizard.refresh_body_window(left_sections[0].height);
		let constraints = vec![Constraint::Length(row_height); fields.len()];

		let rows = Layout::default()
			.direction(Direction::Vertical)
			.constraints(constraints)
			.split(left_sections[0]);

		for (field, row) in fields.iter().zip(rows.iter()) {
			let focused = *field == self.wizard.focus;
			let (label, action) = self.wizard.render_field(*field);
			let side_button = wizard_form_row_button(*field);
			let value = self.wizard.display_value_for_field(*field, focused, visible_field_width(row.width, side_button.is_some()));
			let button_rect = self.render_form_row(frame, *row, label, value, focused, action.clone(), side_button.clone());
			self.hit_targets.push(HitTarget::new(*row, action));
			if let (Some(rect), Some(button)) = (button_rect, side_button) {
				self.hit_targets.push(HitTarget::new(rect, button.action));
			}
		}
		render_vertical_overflow_indicators(frame, left_sections[0], show_above, show_below);

		self.render_button_row(
			frame,
			left_sections[2],
			&[
				DialogButton::new("Read", self.wizard.focus == WizardField::Validate, HitAction::ValidateWizard, Style::default().fg(Color::Black).bg(Color::Yellow)),
				DialogButton::new("Save", self.wizard.focus == WizardField::Save, HitAction::SaveWizard, Style::default().fg(Color::Black).bg(Color::Green)),
				DialogButton::new("Cancel", self.wizard.focus == WizardField::Cancel, HitAction::CancelWizard, Style::default().fg(Color::White).bg(Color::Red)),
			],
		);

		let side_block = Block::default().borders(Borders::ALL).title(" Validation And Notes ");
		let side_inner = side_block.inner(chunks[1]);
		frame.render_widget(side_block, chunks[1]);

		let mut lines = vec![
			Line::from("Wizard notes".bold()),
			Line::from(""),
			Line::from(format!("Project type: {}", self.wizard.project_type.display_name())),
			Line::from(format!("Integration: {}", self.wizard.integration_mode.display_name())),
			Line::from(format!("Version scheme: {}", self.wizard.version_scheme.display_name())),
			Line::from(if self.wizard.project_type == ProjectType::Branched {
				format!(
					"Unified versioning: {}",
					if self.wizard.unified_versioning { "on" } else { "off" }
				)
			} else {
				"Unified versioning: always on for all-in-one projects".to_string()
			}),
			Line::from(format!("Example: {}", self.wizard.version_scheme.example())),
			Line::from(format!("Rule: {}", self.wizard.version_scheme.description())),
			Line::raw(""),
			Line::from(if self.wizard.project_type == ProjectType::Branched {
				format!("Scopes: {} configured. Left/Right changes the selected scope.", self.wizard.scopes.len())
			} else {
				"All-in-one projects manage one target directly.".to_string()
			}),
			Line::from("Use the scope action rows to add, remove, or reorder branched entries."),
			Line::raw(""),
		];

		let active_probe = if self.wizard.project_type == ProjectType::Branched {
			self.wizard.current_scope().and_then(|scope| scope.last_probe.as_ref())
		} else {
			self.wizard.last_probe.as_ref()
		};

		if self.wizard.project_type == ProjectType::Branched {
			let validated = self
				.wizard
				.scopes
				.iter()
				.filter(|scope| matches!(scope.last_probe.as_ref().map(|probe| probe.kind), Some(ProbeKind::Success)))
				.count();
			lines.push(Line::from(format!("Validated scopes: {}/{}", validated, self.wizard.scopes.len())));
		}

		if let Some(probe) = active_probe {
			let color = match probe.kind {
				ProbeKind::Success => Color::Green,
				ProbeKind::Warning => Color::Yellow,
				ProbeKind::Error => Color::Red,
			};
			lines.push(Line::from(Span::styled("Read target", Style::default().fg(color).add_modifier(Modifier::BOLD))));
			lines.push(Line::from(probe.message.clone()));
			if let Some(version) = &probe.version {
				lines.push(Line::from(format!("Detected version: {}", version)));
			}
			if let Some(format) = probe.format {
				lines.push(Line::from(format!("Detected format: {}", format.display_name())));
			}
		} else {
			lines.push(Line::from("Use F5 or Read to inspect the selected target file."));
			lines.push(Line::from("Save requires every branched scope to be read successfully."));
		}
		frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), side_inner);
	}

	fn render_form_row(
		&self,
		frame: &mut Frame,
		area: Rect,
		label: &'static str,
		value: String,
		focused: bool,
		_action: HitAction,
		side_button: Option<FormRowButton>,
	) -> Option<Rect> {
		let label_area = center_vertically(
			Rect {
				x: area.x,
				y: area.y,
				width: FORM_LABEL_WIDTH,
				height: area.height,
			},
			1,
		);
		frame.render_widget(
			Paragraph::new(Line::from(Span::styled(
				label,
				Style::default().fg(Color::Rgb(220, 220, 220)),
			))),
			label_area,
		);

		let row = if side_button.is_some() {
			Layout::default()
				.direction(Direction::Horizontal)
				.constraints([
					Constraint::Length(FORM_LABEL_WIDTH),
					Constraint::Min(10),
					Constraint::Length(1),
					Constraint::Length(BROWSE_BUTTON_WIDTH),
				])
				.split(area)
		} else {
			Layout::default()
				.direction(Direction::Horizontal)
				.constraints([Constraint::Length(FORM_LABEL_WIDTH), Constraint::Min(10)])
				.split(area)
		};

		let field_index = 1;
		let field_area = center_vertically(row[field_index], area.height.min(3));
		let style = Style::default().fg(Color::Rgb(235, 235, 235));
		let block = if focused {
			Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::Cyan))
		} else {
			Block::default().borders(Borders::ALL)
		};
		frame.render_widget(
			Paragraph::new(Line::from(Span::styled(value, style))).block(block),
			field_area,
		);

		if let Some(button) = side_button {
			let button_area = center_vertically(row[3], area.height.min(3));
			frame.render_widget(
				Paragraph::new(button.label)
					.alignment(Alignment::Center)
					.style(Style::default().fg(Color::Black).bg(Color::Cyan))
					.block(Block::default().borders(Borders::ALL)),
				button_area,
			);
			return Some(button_area);
		}

		None
	}

	fn render_button_row(&mut self, frame: &mut Frame, area: Rect, buttons: &[DialogButton]) {
		if buttons.is_empty() {
			return;
		}

		let mut constraints = Vec::with_capacity(buttons.len() * 2 + 1);
		constraints.push(Constraint::Fill(1));
		for button in buttons {
			constraints.push(Constraint::Length((button.label.chars().count() as u16 + 6).max(14)));
			constraints.push(Constraint::Fill(1));
		}
		let chunks = Layout::default()
			.direction(Direction::Horizontal)
			.constraints(constraints)
			.flex(Flex::Center)
			.split(area);

		for (index, button) in buttons.iter().enumerate() {
			let rect = center_vertically(chunks[(index * 2) + 1], area.height.min(3));
			let style = if button.focused {
				button.style.add_modifier(Modifier::BOLD)
			} else {
				button.style
			};
			let block = if button.focused {
				Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::Cyan))
			} else {
				Block::default().borders(Borders::ALL)
			};
			frame.render_widget(
				Paragraph::new(button.label.as_str())
					.alignment(Alignment::Center)
					.style(style)
					.block(block),
				rect,
			);
			self.hit_targets.push(HitTarget::new(rect, button.action.clone()));
		}
	}

	fn render_tag_annotation_dialog(&mut self, frame: &mut Frame, area: Rect) {
		let Some(dialog) = &self.tag_annotation_dialog else {
			return;
		};

		let popup = centered_rect(area, 78, 62);
		frame.render_widget(Clear, popup);
		let block = Block::default()
			.borders(Borders::ALL)
			.title(" Tag Annotation ")
			.border_style(Style::default().fg(Color::Cyan));
		let inner = block.inner(popup);
		frame.render_widget(block, popup);

		let sections = Layout::default()
			.direction(Direction::Vertical)
			.constraints([Constraint::Length(2), Constraint::Min(8), Constraint::Length(BUTTON_ROW_HEIGHT)])
			.split(inner);

		frame.render_widget(
			Paragraph::new("Enter inserts a new line. F2 or Ctrl+S saves the annotation. Esc cancels."),
			sections[0],
		);
		self.render_tag_annotation_editor(frame, sections[1], dialog);

		self.render_button_row(
			frame,
			sections[2],
			&[
				DialogButton::new("Save", false, HitAction::SaveTagAnnotation, Style::default().fg(Color::Black).bg(Color::Green)),
				DialogButton::new("Cancel", false, HitAction::CancelTagAnnotation, Style::default().fg(Color::White).bg(Color::Red)),
			],
		);
	}

	fn render_tag_annotation_editor(&self, frame: &mut Frame, area: Rect, dialog: &TagAnnotationDialog) {
		self.render_textarea_editor(frame, area, " Annotation ", dialog.placeholder.as_str(), &dialog.editor);
	}

	fn render_textarea_editor(
		&self,
		frame: &mut Frame,
		area: Rect,
		title: &str,
		placeholder: &str,
		editor: &TuiTextArea<'_>,
	) {
		let block = Block::default()
			.borders(Borders::ALL)
			.title(title)
			.border_style(Style::default().fg(Color::Cyan));
		let inner = block.inner(area);
		frame.render_widget(block, area);

		let lines = editor.lines();
		let (cursor_row, cursor_col) = editor.cursor();
		let visible_height = inner.height.max(1) as usize;
		let start_row = cursor_row.saturating_sub(visible_height / 2).min(lines.len().saturating_sub(visible_height));
		let end_row = (start_row + visible_height).min(lines.len());
		let number_width = end_row.max(1).to_string().len().max(2);
		let content_width = inner.width.saturating_sub(number_width as u16 + 1).max(1) as usize;

		let body = if lines.len() == 1 && lines[0].is_empty() {
			vec![Line::from(Span::styled(
				placeholder,
				Style::default().fg(Color::DarkGray),
			))]
		} else {
			lines[start_row..end_row]
				.iter()
				.enumerate()
				.map(|(offset, line)| {
					let row_index = start_row + offset;
					let active = row_index == cursor_row;
					render_annotation_line(line, row_index + 1, number_width, content_width, active.then_some(cursor_col))
				})
				.collect::<Vec<_>>()
		};

		frame.render_widget(Paragraph::new(body), inner);
	}

	fn render_footer(&self, frame: &mut Frame, area: Rect) {
		let block = Block::default().borders(Borders::ALL).title(" Controls ");
		let inner = block.inner(area);
		frame.render_widget(block, area);

		let help = if self.browser_dialog.is_some() {
			Line::from("Arrows navigate | Enter open folder or select file | U use folder | Mouse click or wheel | Esc cancel")
		} else if self.project_edit_dialog.is_some() {
			Line::from("Tab move | Left/Right change enums | PgUp/PgDn or wheel scroll | Ctrl+O browse | F2 save | Del remove | Esc cancel")
		} else if self.tag_annotation_dialog.is_some() {
			Line::from("Type annotation | Enter newline | F2 or Ctrl+S save | Esc cancel")
		} else if self.changelog_preview_dialog.is_some() {
			Line::from("Type release notes | Ctrl+S save preview | F2 continue/close | PgUp/PgDn or wheel scroll preview | Esc cancel")
		} else if self.tag_dialog.is_some() {
			Line::from("Type tag name | [ ] scope | A annotation | Left/Right action | Enter run | Esc cancel")
		} else if self.recent_changes_dialog.is_some() {
			Line::from("1/2 switch tabs | [ ] scope | Left/Right history | Up/Down scroll | R reload | T create tag | Esc close")
		} else if self.bump_dialog.is_some() {
			Line::from("Up/Down scope | Left/Right change bump action | Enter apply | Esc cancel")
		} else if self.overview_bump_workflow_dialog.is_some() {
			Line::from("1-3 or Up/Down choose action | Enter run | Esc cancel")
		} else if self.overview_bump_warning_dialog.is_some() {
			Line::from("1-3 or Up/Down choose warning action | Enter confirm | Esc cancel")
		} else {
			match self.screen {
				Screen::Dashboard => self.dashboard_footer_line(),
				Screen::UiSettings => ui_settings_footer_line(),
				Screen::Wizard => Line::from("Tab move | Left/Right change enums | PgUp/PgDn or wheel scroll | Ctrl+O browse | F5 read target | F2 save | Esc cancel"),
			}
		};
		let alignment = match self.config.ui.footer_content {
			FooterContent::Centered => Alignment::Center,
			FooterContent::Left => Alignment::Left,
		};
		frame.render_widget(Paragraph::new(vec![help]).alignment(alignment), inner);
	}

	fn render_browser_dialog(&mut self, frame: &mut Frame, area: Rect) {
		let Some(dialog) = &self.browser_dialog else {
			return;
		};

		let popup = centered_rect(area, 78, 72);
		frame.render_widget(Clear, popup);
		let block = Block::default()
			.borders(Borders::ALL)
			.title(format!(" {} ", dialog.title))
			.border_style(Style::default().fg(Color::Cyan));
		let inner = block.inner(popup);
		frame.render_widget(block, popup);

		let sections = Layout::default()
			.direction(Direction::Vertical)
			.constraints([Constraint::Length(2), Constraint::Min(8), Constraint::Length(2)])
			.split(inner);

		let instructions = if dialog.select_directories {
			"Arrows navigate | Enter open folder | U use highlighted folder | Esc cancel"
		} else {
			"Arrows navigate | Enter select file or open folder | Mouse click selects | Esc cancel"
		};
		frame.render_widget(Paragraph::new(instructions), sections[0]);

		let body_block = Block::default()
			.borders(Borders::ALL)
			.title(format!(" {} ", dialog.explorer.cwd().display()));
		let body_inner = body_block.inner(sections[1]);
		frame.render_widget(body_block, sections[1]);

		let files = dialog.explorer.files();
		let selected = dialog.explorer.selected_idx();
		let (start, end) = browser_visible_range(files.len(), selected, body_inner.height as usize);
		let items = files[start..end]
			.iter()
			.map(|file| ListItem::new(file.name.clone()))
			.collect::<Vec<_>>();
		let mut state = ListState::default();
		state.select(Some(selected.saturating_sub(start)));
		let list = List::new(items)
			.highlight_style(Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD))
			.highlight_symbol("> ");
		frame.render_stateful_widget(list, body_inner, &mut state);

		for (offset, _) in files[start..end].iter().enumerate() {
			let rect = Rect {
				x: body_inner.x,
				y: body_inner.y + offset as u16,
				width: body_inner.width,
				height: 1,
			};
			self.hit_targets.push(HitTarget::new(rect, HitAction::BrowserSelect(start + offset)));
		}

		let selected_path = dialog.explorer.current().path.display().to_string();
		frame.render_widget(
			Paragraph::new(format!("Selected: {}", selected_path)).wrap(Wrap { trim: false }),
			sections[2],
		);
	}

	fn dashboard_footer_line(&self) -> Line<'static> {
		let mut spans = main_tabs_shortcut_spans();
		spans.push(Span::raw(" | "));
		spans.extend(shortcut_key_label("TAB", " Switch Pane"));
		spans.push(Span::raw(" | "));
		match self.dashboard_focus {
			DashboardPane::Projects => {
				spans.extend(shortcut_token("↑/↓"));
				spans.push(Span::raw(" projects"));
			}
			DashboardPane::Overview => {
				spans.extend(shortcut_token("←/→"));
				spans.push(Span::raw(" tile focus | "));
				spans.extend(shortcut_token("↑/↓"));
				spans.push(Span::raw(" recent changes"));
			}
		}
		spans.push(Span::raw(" | "));
		spans.extend(shortcut_key_label("N", "ew Project"));
		spans.push(Span::raw(" | "));
		spans.extend(shortcut_key_label("E", "dit Project"));
		spans.push(Span::raw(" | "));
		spans.extend(shortcut_key_label("D", "elete"));
		spans.push(Span::raw(" | "));
		spans.extend(shortcut_key_label("L", " ReleaseNOW"));
		spans.push(Span::raw(" | "));
		spans.extend(shortcut_key_label("G", "itlog"));
		spans.push(Span::raw(" / "));
		spans.extend(shortcut_key_label("C", "hangelog"));
		spans.push(Span::raw(" | "));
		spans.extend(shortcut_key_label("B", "ump"));
		spans.push(Span::raw(" | "));		
		spans.extend(shortcut_key_label("T", "ag"));
		spans.push(Span::raw(" | "));
		spans.extend(shortcut_key_label("R", "eload"));
		spans.push(Span::raw(" | "));
		if self.overview_tab == OverviewTab::ProjectSettings {
			spans.extend(shortcut_token("[ ]"));
			spans.push(Span::raw(" sub-tabs | "));
			spans.extend(shortcut_key_label("Space", " Toggle Changelog"));
			spans.push(Span::raw(" | "));
		}
		spans.extend(shortcut_key_label("H", "ide Footer"));
		spans.push(Span::raw(" | "));
		spans.extend(shortcut_key_label("Q", "uit"));
		Line::from(spans)
	}
}