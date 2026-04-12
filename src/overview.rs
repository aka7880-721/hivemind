// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyVersionBumper License v1.2
//
// For details, see the LICENSE file in the repository root.

use super::*;
use crate::changelog::{archive_changelog_markdown, build_document_from_git_log};
use crate::dialogs::load_recent_change_range;
use super::git_flow::{
	append_repo_stage_paths, apply_repo_bump_workflow, collect_repo_bump_operations,
	collect_unexpected_staged_paths, refresh_target_artifacts, stage_path_for_file,
	unstage_paths,
};

pub(super) fn render_dashboard_overview(app: &mut App, frame: &mut Frame, area: Rect) {
	let Some(project) = app.config.projects.get(app.selected_project).cloned() else {
		frame.render_widget(
			Paragraph::new(vec![
				Line::from("Overview".bold()),
				Line::from("Select or create a project to populate the overview page."),
			])
			.wrap(Wrap { trim: false }),
			area,
		);
		return;
	};

	ensure_dashboard_recent_changes(app);

	let scopes = match collect_bump_scopes(&project) {
		Ok(scopes) => scopes,
		Err(error) => {
			frame.render_widget(
				Paragraph::new(vec![
					Line::from("Overview".bold()),
					Line::from(error.to_string()).style(Style::default().fg(Color::Red)),
				])
				.wrap(Wrap { trim: false }),
				area,
			);
			return;
		}
	};
	ensure_dashboard_tile_state(app, &scopes);

	let tile_columns = dashboard_tile_columns(area.width).max(1);
	let tile_rows = app.overview_scope_order.len().max(1).div_ceil(tile_columns);
	let max_tile_height = scopes
		.iter()
		.map(|scope| tile_height(scope.scheme))
		.max()
		.unwrap_or(7);

	if app.overview_show_recent_tab && app.overview_tab == OverviewTab::RecentChanges {
		render_overview_recent_changes(app, frame, area);
		return;
	}

	if app.overview_show_recent_tab {
		render_dashboard_tiles(app, frame, area, &project, &scopes);
		return;
	}

	let row_height = max_tile_height.saturating_add(1);
	let desired_tile_height = tile_rows as u16 * row_height - 1;
	let tile_height_budget = area.height.saturating_sub(9).max(max_tile_height.min(area.height));
	let tile_section_height = desired_tile_height.min(tile_height_budget).max(max_tile_height.min(area.height));
	let sections = Layout::default()
		.direction(Direction::Vertical)
		.constraints([Constraint::Length(tile_section_height), Constraint::Length(1), Constraint::Min(8)])
		.split(area);

	render_dashboard_tiles(app, frame, sections[0], &project, &scopes);

	render_overview_recent_changes(app, frame, sections[2]);
}

pub(super) fn render_overview_recent_changes(app: &mut App, frame: &mut Frame, area: Rect) {
	let recent_block = Block::default().borders(Borders::ALL).title(" Recent Changes ");
	let recent_inner = recent_block.inner(area);
	app.overview_recent_viewport = Some(recent_inner);
	frame.render_widget(recent_block, area);

	let recent_lines = if let Some(dialog) = &app.overview_recent_changes {
		let mut lines = vec![
			Line::from(format!(
				"Scope: {} ({})",
				dialog.active_scope().display_name,
				dialog.active_scope().scope_kind.map(|kind| kind.display_name()).unwrap_or("Project")
			))
			.style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
			Line::from(format!("View: {}", dialog.current_range().label)).style(Style::default().fg(Color::Gray)),
			Line::raw(""),
		];
		if dialog.current_range().lines.is_empty() {
			lines.push(Line::from("No recent changes to display."));
		} else {
			let graph_base_column = git_graph_base_column(&dialog.current_range().lines);
			lines.extend(
				dialog
					.current_range()
					.lines
					.iter()
					.map(|line| colorize_git_log_line(line, graph_base_column)),
			);
		}
		lines
	} else if let Some(error) = &app.overview_recent_error {
		vec![
			Line::from("Recent changes are unavailable.").style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
			Line::from(error.clone()),
		]
	} else {
		vec![Line::from("Recent changes are not available for local-only projects.")]
	};
	let scroll = app.overview_recent_changes.as_ref().map(|dialog| dialog.scroll).unwrap_or(0);
	frame.render_widget(
		Paragraph::new(recent_lines)
			.scroll((scroll, 0))
			.wrap(Wrap { trim: false }),
		recent_inner,
	);
}

pub(super) fn should_use_recent_changes_tab(app: &App, area: Rect) -> bool {
	let Some(project) = app.config.projects.get(app.selected_project) else {
		return false;
	};
	let Ok(scopes) = collect_bump_scopes(project) else {
		return false;
	};
	let max_tile_height = scopes
		.iter()
		.map(|scope| tile_height(scope.scheme))
		.max()
		.unwrap_or(7);
	super::should_use_recent_changes_tab(area.height, max_tile_height)
}

pub(super) fn ensure_dashboard_recent_changes(app: &mut App) {
	let Some(project) = app.config.projects.get(app.selected_project) else {
		app.overview_recent_project = None;
		app.overview_recent_changes = None;
		app.overview_recent_error = None;
		return;
	};

	let project_changed = app.overview_recent_project != Some(app.selected_project);
	app.overview_recent_project = Some(app.selected_project);
	if !project.integration_mode.requires_repo() {
		app.overview_recent_changes = None;
		app.overview_recent_error = None;
		return;
	}

	if project_changed || app.overview_recent_changes.is_none() {
		app.overview_recent_changes = None;
		app.overview_recent_error = None;
		match RecentChangesDialog::from_project(project) {
			Ok(dialog) => app.overview_recent_changes = Some(dialog),
			Err(error) => app.overview_recent_error = Some(error.to_string()),
		}
		return;
	}

	if let Some(dialog) = &mut app.overview_recent_changes {
		if let Err(error) = dialog.refresh_current_scope() {
			app.overview_recent_changes = None;
			app.overview_recent_error = Some(error.to_string());
		} else {
			app.overview_recent_error = None;
		}
	}
}

pub(super) fn ensure_dashboard_tile_state(app: &mut App, scopes: &[BumpScope]) {
	if app.overview_tile_project == Some(app.selected_project)
		&& app.overview_scope_order.len() == scopes.len()
		&& app.overview_pending_versions.len() == scopes.len()
	{
		app.overview_focused_scope = app.overview_focused_scope.min(scopes.len().saturating_sub(1));
		return;
	}

	app.overview_tile_project = Some(app.selected_project);
	app.overview_scope_order = (0..scopes.len()).collect();
	app.overview_pending_versions = scopes
		.iter()
		.map(|scope| scope.current_version.clone().unwrap_or_else(|| scope.version_label().to_string()))
		.collect();
	app.overview_tile_scroll = 0;
	app.overview_focused_scope = 0;
}

pub(super) fn invalidate_overview_cache(app: &mut App) {
	app.overview_recent_project = None;
	app.overview_tile_project = None;
}

pub(super) fn reorder_dashboard_tile_scope(app: &mut App, from_scope: usize, to_scope: usize) {
	let Some(from_index) = app.overview_scope_order.iter().position(|scope| *scope == from_scope) else {
		return;
	};
	let Some(to_index) = app.overview_scope_order.iter().position(|scope| *scope == to_scope) else {
		return;
	};
	if from_index == to_index {
		return;
	}

	let moved = app.overview_scope_order.remove(from_index);
	app.overview_scope_order.insert(to_index, moved);
}

pub(super) fn scroll_dashboard_tiles(app: &mut App, delta: isize) -> Result<()> {
	let viewport = match app.overview_tile_viewport {
		Some(viewport) => viewport,
		None => return Ok(()),
	};
	let project = app.selected_project()?.clone();
	let scopes = collect_bump_scopes(&project)?;
	if scopes.is_empty() {
		app.overview_tile_scroll = 0;
		return Ok(());
	}

	let columns = dashboard_tile_columns(viewport.width).max(1);
	let row_height = scopes
		.iter()
		.map(|scope| tile_height(scope.scheme))
		.max()
		.unwrap_or(7)
		.saturating_add(1);
	let visible_rows = ((viewport.height.saturating_add(1)) / row_height.max(1)).max(1) as usize;
	let total_rows = app.overview_scope_order.len().div_ceil(columns);
	let max_scroll = total_rows.saturating_sub(visible_rows);
	app.overview_tile_scroll = (app.overview_tile_scroll as isize + delta)
		.clamp(0, max_scroll as isize) as usize;
	Ok(())
}

pub(super) fn move_dashboard_overview_focus(app: &mut App, delta: isize) -> Result<()> {
	let project = app.selected_project()?.clone();
	let scopes = collect_bump_scopes(&project)?;
	ensure_dashboard_tile_state(app, &scopes);
	if scopes.is_empty() || app.overview_scope_order.is_empty() {
		return Ok(());
	}

	let current_index = app
		.overview_scope_order
		.iter()
		.position(|scope_index| *scope_index == app.overview_focused_scope)
		.unwrap_or(0) as isize;
	let next_index = (current_index + delta).clamp(0, app.overview_scope_order.len() as isize - 1) as usize;
	let next_scope = app.overview_scope_order[next_index];
	select_dashboard_overview_scope(app, next_scope)?;
	ensure_dashboard_focus_visible(app, next_index, &scopes);
	Ok(())
}

pub(super) fn ensure_dashboard_focus_visible(app: &mut App, order_index: usize, scopes: &[BumpScope]) {
	let Some(viewport) = app.overview_tile_viewport else {
		return;
	};
	if scopes.is_empty() {
		app.overview_tile_scroll = 0;
		return;
	}

	let columns = dashboard_tile_columns(viewport.width).max(1);
	let row_height = scopes
		.iter()
		.map(|scope| tile_height(scope.scheme))
		.max()
		.unwrap_or(7)
		.saturating_add(1);
	let visible_rows = ((viewport.height.saturating_add(1)) / row_height.max(1)).max(1) as usize;
	let row = order_index / columns;
	if row < app.overview_tile_scroll {
		app.overview_tile_scroll = row;
	} else if row >= app.overview_tile_scroll + visible_rows {
		app.overview_tile_scroll = row + 1 - visible_rows;
	}
}

pub(super) fn render_dashboard_tiles(
	app: &mut App,
	frame: &mut Frame,
	area: Rect,
	project: &ProjectConfig,
	scopes: &[BumpScope],
) {
	app.overview_tile_viewport = Some(area);

	if scopes.is_empty() || area.width == 0 || area.height == 0 {
		return;
	}

	let git_contexts = collect_all_branch_git_scope_contexts(project).ok();
	let columns = dashboard_tile_columns(area.width).max(1);
	let vertical_gap = 1;
	let row_height = scopes
		.iter()
		.map(|scope| tile_height(scope.scheme))
		.max()
		.unwrap_or(7)
		.saturating_add(vertical_gap);
	let visible_rows = ((area.height.saturating_add(vertical_gap)) / row_height.max(1)).max(1) as usize;
	let total_rows = app.overview_scope_order.len().div_ceil(columns);
	let max_scroll = total_rows.saturating_sub(visible_rows);
	app.overview_tile_scroll = app.overview_tile_scroll.min(max_scroll);

	let visible_row_scopes = (app.overview_tile_scroll..(app.overview_tile_scroll + visible_rows).min(total_rows))
		.map(|row| {
			let start = row * columns;
			let end = (start + columns).min(app.overview_scope_order.len());
			app.overview_scope_order[start..end].to_vec()
		})
		.filter(|row| !row.is_empty())
		.collect::<Vec<_>>();

	let row_constraints = visible_row_scopes
		.iter()
		.map(|row| {
			let row_tile_height = row
				.iter()
				.filter_map(|scope_index| scopes.get(*scope_index))
				.map(|scope| tile_height(scope.scheme))
				.max()
				.unwrap_or(7);
			Constraint::Length(row_tile_height)
		})
		.collect::<Vec<_>>();
	let row_areas = Layout::default()
		.direction(Direction::Vertical)
		.constraints(row_constraints)
		.flex(Flex::SpaceEvenly)
		.split(area);

	for (row_area, row_scopes) in row_areas.iter().zip(visible_row_scopes.iter()) {
		let column_areas = Layout::default()
			.direction(Direction::Horizontal)
			.constraints(vec![Constraint::Length(TILE_WIDTH.min(area.width)); row_scopes.len()])
			.flex(Flex::SpaceEvenly)
			.split(*row_area);

		for (cell_area, scope_index) in column_areas.iter().zip(row_scopes.iter().copied()) {
			let Some(scope) = scopes.get(scope_index) else {
				continue;
			};

			let tile_rect = center_vertically(*cell_area, tile_height(scope.scheme));
			if tile_rect.width < 12 || tile_rect.height < 4 {
				continue;
			}

			let activity = git_contexts
				.as_ref()
				.and_then(|entries| entries.get(scope_index))
				.and_then(|context| load_scope_activity_summary(context).ok());
			let selected = scope_index == app.overview_focused_scope;
			let tile = OverviewTileData {
				name: scope.display_name.clone(),
				scheme: scope.scheme,
				preview_version: app
					.overview_pending_versions
					.get(scope_index)
					.cloned()
					.unwrap_or_else(|| scope.current_version.clone().unwrap_or_else(|| scope.version_label().to_string())),
				commits_since_tag_label: activity
					.as_ref()
					.map(|summary| summary.commits_since_tag_label.clone())
					.unwrap_or_else(|| "n/a".to_string()),
				last_bump_label: activity
					.as_ref()
					.map(|summary| summary.last_bump_label.clone())
					.unwrap_or_else(|| "n/a".to_string()),
				last_commit_label: activity
					.as_ref()
					.map(|summary| summary.last_commit_label.clone())
					.unwrap_or_else(|| "n/a".to_string()),
				selected,
			};
			let hotspots = render_overview_tile(frame, tile_rect, &tile);
			app.overview_tile_rects.push((hotspots.tile_rect, scope_index));

			app.hit_targets.push(HitTarget::new(hotspots.title_rect, HitAction::SelectOverviewScope(scope_index)));
			app.hit_targets.push(HitTarget::new(hotspots.view_rect, HitAction::OpenOverviewRecentChanges(scope_index)));
			app.hit_targets.push(HitTarget::new(hotspots.bump_rect, HitAction::BeginOverviewBump(scope_index)));
			app.hit_targets.push(HitTarget::new(hotspots.tag_rect, HitAction::ApplyOverviewVersionAndTag(scope_index)));
			if let Some(rect) = hotspots.reset_rect {
				app.hit_targets.push(HitTarget::new(rect, HitAction::ResetOverviewPendingVersion(scope_index)));
			}
			if let Some(rect) = hotspots.major_rect {
				app.hit_targets.push(HitTarget::with_right_action(
					rect,
					HitAction::AdjustOverviewVersion(scope_index, OverviewVersionControl::Major, 1),
					HitAction::AdjustOverviewVersion(scope_index, OverviewVersionControl::Major, -1),
				));
			}
			if let Some(rect) = hotspots.minor_rect {
				app.hit_targets.push(HitTarget::with_right_action(
					rect,
					HitAction::AdjustOverviewVersion(scope_index, OverviewVersionControl::Minor, 1),
					HitAction::AdjustOverviewVersion(scope_index, OverviewVersionControl::Minor, -1),
				));
			}
			if let Some(rect) = hotspots.patch_rect {
				app.hit_targets.push(HitTarget::with_right_action(
					rect,
					HitAction::AdjustOverviewVersion(scope_index, OverviewVersionControl::Patch, 1),
					HitAction::AdjustOverviewVersion(scope_index, OverviewVersionControl::Patch, -1),
				));
			}
			if let Some(rect) = hotspots.version_rect {
				app.hit_targets.push(HitTarget::with_right_action(
					rect,
					HitAction::AdjustOverviewVersion(scope_index, OverviewVersionControl::Whole, 1),
					HitAction::AdjustOverviewVersion(scope_index, OverviewVersionControl::Whole, -1),
				));
			}
		}
	}

	if app.overview_tile_scroll > 0 && area.height > 0 {
		let indicator = Rect {
			x: area.x,
			y: area.y,
			width: area.width,
			height: 1,
		};
		frame.render_widget(
			Paragraph::new("more scopes above").alignment(Alignment::Right).style(Style::default().fg(Color::DarkGray)),
			indicator,
		);
	}

	if app.overview_tile_scroll < max_scroll && area.height > 0 {
		let indicator = Rect {
			x: area.x,
			y: area.y + area.height.saturating_sub(1),
			width: area.width,
			height: 1,
		};
		frame.render_widget(
			Paragraph::new("more scopes below").alignment(Alignment::Right).style(Style::default().fg(Color::DarkGray)),
			indicator,
		);
	}
}

pub(super) fn select_dashboard_overview_scope(app: &mut App, scope_index: usize) -> Result<()> {
	app.dashboard_focus = DashboardPane::Overview;
	app.overview_focused_scope = scope_index;
	ensure_dashboard_recent_changes(app);
	if let Some(dialog) = &mut app.overview_recent_changes {
		dialog.select_scope(scope_index)?;
	}
	Ok(())
}

pub(super) fn begin_overview_bump(app: &mut App, scope_index: usize) -> Result<()> {
	let project = app.selected_project()?.clone();
	if !project.integration_mode.requires_repo() {
		return apply_overview_pending_version(app, scope_index, false);
	}

	let scopes = collect_bump_scopes(&project)?;
	ensure_dashboard_tile_state(app, &scopes);
	let next_version = app
		.overview_pending_versions
		.get(scope_index)
		.cloned()
		.or_else(|| scopes.get(scope_index).and_then(|scope| scope.current_version.clone()))
		.ok_or_else(|| anyhow!("the selected scope does not have a resolved version value"))?;
	let scope_label = if project.unified_versioning {
		"All configured scopes".to_string()
	} else {
		scopes
			.get(scope_index)
			.map(|scope| scope.display_name.clone())
			.unwrap_or_else(|| project.name.clone())
	};
	let options = match project.integration_mode {
		IntegrationMode::LocalOnly => vec![OverviewBumpWorkflow::JustBump],
		IntegrationMode::GitLocalOnly => vec![
			OverviewBumpWorkflow::JustBump,
			OverviewBumpWorkflow::Commit,
			OverviewBumpWorkflow::CommitAndTag,
		],
		IntegrationMode::GitHubEnabled => vec![
			OverviewBumpWorkflow::JustBump,
			OverviewBumpWorkflow::CommitAndPush,
			OverviewBumpWorkflow::CommitPushAndTag,
		],
	};

	app.overview_bump_workflow_dialog = Some(OverviewBumpWorkflowDialog::new(
		project.name,
		scope_label,
		next_version,
		scope_index,
		options,
	));
	app.status = StatusMessage::info("Choose how the tile bump should be applied.");
	Ok(())
}

pub(super) fn select_overview_bump_workflow(app: &mut App, index: usize) {
	if let Some(dialog) = &mut app.overview_bump_workflow_dialog {
		dialog.select(index);
	}
}

pub(super) fn rotate_overview_bump_workflow(app: &mut App, delta: isize) {
	if let Some(dialog) = &mut app.overview_bump_workflow_dialog {
		dialog.rotate(delta);
	}
}

pub(super) fn cancel_overview_bump_workflow(app: &mut App) {
	app.overview_bump_workflow_dialog = None;
	app.status = StatusMessage::info("Tile bump action cancelled.");
}

pub(super) fn select_overview_bump_warning(app: &mut App, index: usize) {
	if let Some(dialog) = &mut app.overview_bump_warning_dialog {
		dialog.select(index);
	}
}

pub(super) fn rotate_overview_bump_warning(app: &mut App, delta: isize) {
	if let Some(dialog) = &mut app.overview_bump_warning_dialog {
		dialog.rotate(delta);
	}
}

pub(super) fn cancel_overview_bump_warning(app: &mut App) {
	app.overview_bump_warning_dialog = None;
	app.overview_bump_workflow_dialog = None;
	app.status = StatusMessage::info("Tile bump action cancelled.");
}

pub(super) fn adjust_overview_pending_version(
	app: &mut App,
	scope_index: usize,
	control: OverviewVersionControl,
	delta: i32,
) -> Result<()> {
	let project = app.selected_project()?.clone();
	let scopes = collect_bump_scopes(&project)?;
	ensure_dashboard_tile_state(app, &scopes);
	let Some(scope) = scopes.get(scope_index) else {
		return Ok(());
	};
	let current = app
		.overview_pending_versions
		.get(scope_index)
		.cloned()
		.unwrap_or_else(|| scope.current_version.clone().unwrap_or_else(|| scope.version_label().to_string()));
	let next = adjust_pending_version_value(scope.scheme, &current, control, delta)?;
	if project.unified_versioning {
		for pending in &mut app.overview_pending_versions {
			*pending = next.clone();
		}
	} else if let Some(pending) = app.overview_pending_versions.get_mut(scope_index) {
		*pending = next;
	}
	Ok(())
}

pub(super) fn reset_overview_pending_version(app: &mut App, scope_index: usize) -> Result<()> {
	let project = app.selected_project()?.clone();
	let scopes = collect_bump_scopes(&project)?;
	ensure_dashboard_tile_state(app, &scopes);
	let Some(scope) = scopes.get(scope_index) else {
		return Ok(());
	};
	let restored = scope.current_version.clone().unwrap_or_else(|| scope.version_label().to_string());
	if project.unified_versioning {
		for pending in &mut app.overview_pending_versions {
			*pending = restored.clone();
		}
	} else if let Some(pending) = app.overview_pending_versions.get_mut(scope_index) {
		*pending = restored.clone();
	}
	app.status = StatusMessage::info(format!("Reset pending version preview to {}.", restored));
	Ok(())
}

pub(super) fn open_dashboard_changelog_preview(app: &mut App) -> Result<()> {
	let project = app.selected_project()?.clone();
	if !project.integration_mode.requires_repo() {
		bail!("changelog preview requires a git-backed project");
	}
	if !project.changelog.enabled {
		bail!("changelog generation is disabled for this project");
	}

	let scopes = collect_bump_scopes(&project)?;
	ensure_dashboard_tile_state(app, &scopes);
	if scopes.is_empty() {
		return Ok(());
	}

	let scope_index = app.overview_focused_scope.min(scopes.len().saturating_sub(1));
	let affected_scope_indexes = if project.unified_versioning {
		(0..scopes.len()).collect::<Vec<_>>()
	} else {
		vec![scope_index]
	};
	let next_version = app
		.overview_pending_versions
		.get(scope_index)
		.cloned()
		.or_else(|| scopes.get(scope_index).and_then(|scope| scope.current_version.clone()))
		.unwrap_or_else(|| scopes[scope_index].version_label().to_string());

	let git_contexts = collect_all_branch_git_scope_contexts(&project)?;
	let changelog_entries = collect_preview_entries(&project, &git_contexts, &affected_scope_indexes, &next_version)?;
	if changelog_entries.is_empty() {
		bail!("no changelog content was generated from the current git history");
	}

	app.open_changelog_preview(ChangelogPreviewDialog::preview_only(
		project.name.clone(),
		next_version,
		scope_index,
		changelog_entries,
	));
	Ok(())
}

pub(super) fn apply_overview_pending_version(app: &mut App, scope_index: usize, open_tag_after: bool) -> Result<()> {
	let project = app.selected_project()?.clone();
	let scopes = collect_bump_scopes(&project)?;
	let scope_repo_roots = app.scope_repo_roots(&project, scopes.len());
	ensure_dashboard_tile_state(app, &scopes);
	let affected_scope_indexes = if project.unified_versioning {
		(0..scopes.len()).collect::<Vec<_>>()
	} else {
		vec![scope_index]
	};
	let next_version = app
		.overview_pending_versions
		.get(scope_index)
		.cloned()
		.or_else(|| scopes.get(scope_index).and_then(|scope| scope.current_version.clone()))
		.ok_or_else(|| anyhow!("the selected scope does not have a resolved version value"))?;

	for index in &affected_scope_indexes {
		if let Some(scope) = scopes.get(*index) {
			for target in &scope.targets {
				write_target_version(target, &next_version)?;
				refresh_target_artifacts(target, scope_repo_roots.get(*index).and_then(|root| root.as_deref()))?;
			}
			if let Some(pending) = app.overview_pending_versions.get_mut(*index) {
				*pending = next_version.clone();
			}
		}
	}

	invalidate_overview_cache(app);
	ensure_dashboard_recent_changes(app);

	if open_tag_after {
		if project.integration_mode.requires_repo() {
			let preferred_scope = if project.unified_versioning { None } else { Some(scope_index) };
			app.open_tag_dialog_with_scope(preferred_scope, Some(TagAction::CreateAndPush))?;
			app.status = StatusMessage::info("Version updated. Review the tag-and-push action next.");
		} else {
			app.status = StatusMessage::warning("Tagging requires a git-backed project.");
		}
	} else {
		app.status = StatusMessage::success(format!("Updated version to {} from the overview tile.", next_version));
	}

	Ok(())
}

pub(super) fn confirm_overview_bump_workflow(app: &mut App) -> Result<()> {
	let Some(dialog) = app.overview_bump_workflow_dialog.clone() else {
		return Ok(());
	};

	if dialog.selected_workflow() != OverviewBumpWorkflow::JustBump {
		let warnings = collect_overview_bump_warnings(app, dialog.scope_index)?;
		if !warnings.is_empty() {
			app.overview_bump_warning_dialog = Some(OverviewBumpWarningDialog::new(
				dialog.scope_index,
				dialog.selected_workflow(),
				warnings,
			));
			app.status = StatusMessage::warning("Previously staged files were found. Review them before committing the bump.");
			return Ok(());
		}
	}

	if open_overview_changelog_preview_if_enabled(app, dialog.scope_index, dialog.selected_workflow())? {
		return Ok(());
	}
	execute_overview_bump_workflow(app, dialog.scope_index, dialog.selected_workflow())?;
	app.overview_bump_workflow_dialog = None;
	Ok(())
}

pub(super) fn confirm_overview_bump_warning(app: &mut App) -> Result<()> {
	let Some(dialog) = app.overview_bump_warning_dialog.clone() else {
		return Ok(());
	};

	match dialog.selected_choice() {
		OverviewBumpWarningChoice::Continue => {
			if open_overview_changelog_preview_if_enabled(app, dialog.scope_index, dialog.workflow)? {
				app.overview_bump_warning_dialog = None;
				return Ok(());
			}
			execute_overview_bump_workflow(app, dialog.scope_index, dialog.workflow)?;
			app.overview_bump_warning_dialog = None;
			app.overview_bump_workflow_dialog = None;
		}
		OverviewBumpWarningChoice::UnstageExtras => {
			for repo in &dialog.repos {
				unstage_paths(&repo.repo_root, &repo.extra_paths)?;
			}
			if open_overview_changelog_preview_if_enabled(app, dialog.scope_index, dialog.workflow)? {
				app.overview_bump_warning_dialog = None;
				return Ok(());
			}
			execute_overview_bump_workflow(app, dialog.scope_index, dialog.workflow)?;
			app.overview_bump_warning_dialog = None;
			app.overview_bump_workflow_dialog = None;
		}
		OverviewBumpWarningChoice::Cancel => cancel_overview_bump_warning(app),
	}
	Ok(())
}

pub(super) fn collect_overview_bump_warnings(app: &App, scope_index: usize) -> Result<Vec<UnexpectedStagedRepo>> {
	let project = app.selected_project()?.clone();
	let scopes = collect_bump_scopes(&project)?;
	let affected_scope_indexes = if project.unified_versioning {
		(0..scopes.len()).collect::<Vec<_>>()
	} else {
		vec![scope_index]
	};
	let git_contexts = collect_all_branch_git_scope_contexts(&project)?;
	let repo_operations = collect_repo_bump_operations(&project, &scopes, &git_contexts, &affected_scope_indexes)?;
	collect_unexpected_staged_paths(&repo_operations)
}

pub(super) fn execute_overview_bump_workflow(
	app: &mut App,
	scope_index: usize,
	workflow: OverviewBumpWorkflow,
) -> Result<()> {
	let project = app.selected_project()?.clone();
	let scopes = collect_bump_scopes(&project)?;
	let scope_repo_roots = app.scope_repo_roots(&project, scopes.len());
	ensure_dashboard_tile_state(app, &scopes);
	let affected_scope_indexes = if project.unified_versioning {
		(0..scopes.len()).collect::<Vec<_>>()
	} else {
		vec![scope_index]
	};
	let next_version = app
		.overview_pending_versions
		.get(scope_index)
		.cloned()
		.or_else(|| scopes.get(scope_index).and_then(|scope| scope.current_version.clone()))
		.ok_or_else(|| anyhow!("the selected scope does not have a resolved version value"))?;

	for index in &affected_scope_indexes {
		if let Some(scope) = scopes.get(*index) {
			for target in &scope.targets {
				write_target_version(target, &next_version)?;
				refresh_target_artifacts(target, scope_repo_roots.get(*index).and_then(|root| root.as_deref()))?;
			}
			if let Some(pending) = app.overview_pending_versions.get_mut(*index) {
				*pending = next_version.clone();
			}
		}
	}

	if workflow != OverviewBumpWorkflow::JustBump {
		let git_contexts = collect_all_branch_git_scope_contexts(&project)?;
		let mut repo_operations = collect_repo_bump_operations(&project, &scopes, &git_contexts, &affected_scope_indexes)?;
		if let Some(pending_changelog) = app.take_matching_pending_changelog_write(scope_index, workflow) {
			for entry in &pending_changelog.entries {
				write_changelog_markdown(&entry.repo_root, &entry.changelog_path, &entry.markdown)?;
				let history_path = archive_changelog_markdown(&entry.repo_root, &next_version, &entry.markdown)?;
				append_repo_stage_paths(
					&mut repo_operations,
					&entry.repo_root,
					&[
						entry.stage_path.clone(),
						stage_path_for_file(&entry.repo_root, &history_path.to_string_lossy()),
					],
				);
			}
		}
		apply_repo_bump_workflow(&repo_operations, &next_version, workflow)?;
	}

	invalidate_overview_cache(app);
	ensure_dashboard_recent_changes(app);

	let target_count = affected_scope_indexes
		.iter()
		.filter_map(|index| scopes.get(*index))
		.map(|scope| scope.targets.len())
		.sum::<usize>();
	let scope_notice = if project.unified_versioning {
		String::new()
	} else {
		scopes
			.get(scope_index)
			.map(|scope| format!(" in scope '{}'", scope.display_name))
			.unwrap_or_default()
	};
	app.status = StatusMessage::success(format!(
		"Updated {} target{}{} to {} via {}.",
		target_count,
		if target_count == 1 { "" } else { "s" },
		scope_notice,
		next_version,
		workflow.display_name()
	));
	Ok(())
}

fn open_overview_changelog_preview_if_enabled(
	app: &mut App,
	scope_index: usize,
	workflow: OverviewBumpWorkflow,
) -> Result<bool> {
	if workflow == OverviewBumpWorkflow::JustBump {
		return Ok(false);
	}

	let project = app.selected_project()?.clone();
	if !project.changelog.enabled || !project.integration_mode.requires_repo() {
		return Ok(false);
	}

	let scopes = collect_bump_scopes(&project)?;
	let affected_scope_indexes = if project.unified_versioning {
		(0..scopes.len()).collect::<Vec<_>>()
	} else {
		vec![scope_index]
	};
	let next_version = app
		.overview_pending_versions
		.get(scope_index)
		.cloned()
		.or_else(|| scopes.get(scope_index).and_then(|scope| scope.current_version.clone()))
		.ok_or_else(|| anyhow!("the selected scope does not have a resolved version value"))?;

	let git_contexts = collect_all_branch_git_scope_contexts(&project)?;
	let changelog_entries = collect_preview_entries(&project, &git_contexts, &affected_scope_indexes, &next_version)?;
	if changelog_entries.is_empty() {
		return Ok(false);
	}

	app.open_changelog_preview(ChangelogPreviewDialog::new(
		project.name.clone(),
		next_version,
		scope_index,
		workflow,
		changelog_entries,
	));
	Ok(true)
}

fn collect_preview_entries(
	project: &ProjectConfig,
	git_contexts: &[crate::git::GitScopeContext],
	affected_scope_indexes: &[usize],
	next_version: &str,
) -> Result<Vec<ChangelogPreviewEntry>> {
	let mut merged_contexts = Vec::<crate::git::GitScopeContext>::new();
	for scope_index in affected_scope_indexes {
		let context = git_contexts
			.get(*scope_index)
			.or_else(|| git_contexts.first())
			.ok_or_else(|| anyhow!("git scope metadata is unavailable for changelog preview"))?;

		if let Some(existing) = merged_contexts.iter_mut().find(|existing| existing.repo_root == context.repo_root) {
			for path in &context.path_filters {
				if !existing.path_filters.iter().any(|candidate| candidate == path) {
					existing.path_filters.push(path.clone());
				}
			}
		} else {
			merged_contexts.push(context.clone());
		}
	}

	merged_contexts
		.into_iter()
		.map(|context| {
			let recent_range = load_recent_change_range(&context)?;
			let changelog_path = project.changelog.effective_path().to_string();
			Ok(ChangelogPreviewEntry {
				repo_root: context.repo_root.clone(),
				changelog_path: changelog_path.clone(),
				stage_path: stage_path_for_file(&context.repo_root, &changelog_path),
				document: build_document_from_git_log(next_version.to_string(), &recent_range.lines),
			})
		})
		.collect()
}