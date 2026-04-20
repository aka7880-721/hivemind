// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit License v1.2
//
// For details, see the LICENSE file in the repository root.

use super::git_flow::{
    append_repo_stage_paths, apply_repo_bump_workflow, collect_repo_bump_operations,
    collect_unexpected_staged_paths_with_cancel, refresh_target_artifacts, stage_path_for_file,
    unstage_paths,
};
use super::*;
use crate::changelog::{
    archive_changelog_markdown, build_document_from_git_log, sum_changelog_gen,
};
use crate::{
    dialogs::{load_change_range_for_refs_with_cancel, load_recent_change_range_with_cancel},
    git::{GitCancellation, sorted_local_tags_with_cancel, switch_or_create_branch},
};
use std::sync::Arc;
use tokio::{sync::Semaphore, task::JoinSet};

const PLACEHOLDER_VERSION: &str = "1.2.3";
const PLACEHOLDER_COMMITS_AHEAD: &str = "7 ahead";
const PLACEHOLDER_LAST_BUMP: &str = "2 days";
const PLACEHOLDER_LAST_COMMIT: &str = "14 min";

pub(super) fn render_dashboard_overview(app: &mut App, frame: &mut Frame, area: Rect) {
    let Some(project) = app.config.projects.get(app.selected_project).cloned() else {
        frame.render_widget(
            Paragraph::new(vec![Line::from(
                "Select or create a project to populate the overview page.",
            )])
            .wrap(Wrap { trim: false }),
            area,
        );
        return;
    };

    let scopes = match collect_bump_scopes(&project) {
        Ok(scopes) => scopes,
        Err(error) => {
            frame.render_widget(
                Paragraph::new(vec![
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
    let tile_height_budget = area
        .height
        .saturating_sub(9)
        .max(max_tile_height.min(area.height));
    let tile_section_height = desired_tile_height
        .min(tile_height_budget)
        .max(max_tile_height.min(area.height));
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(tile_section_height),
            Constraint::Length(1),
            Constraint::Min(8),
        ])
        .split(area);

    render_dashboard_tiles(app, frame, sections[0], &project, &scopes);

    render_overview_recent_changes(app, frame, sections[2]);
}

pub(super) fn render_overview_recent_changes(app: &mut App, frame: &mut Frame, area: Rect) {
    let recent_block = Block::default()
        .borders(Borders::ALL)
        .title(" Recent Changes ");
    let recent_inner = recent_block.inner(area);
    app.overview_recent_viewport = Some(recent_inner);
    frame.render_widget(recent_block, area);

    let recent_lines = if let Some(dialog) = &app.overview_recent_changes {
        let mut lines = vec![
            Line::from(format!(
                "Scope: {} ({})",
                dialog.active_scope().display_name,
                dialog
                    .active_scope()
                    .scope_kind
                    .map(|kind| kind.display_name())
                    .unwrap_or("Project")
            ))
            .style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Line::from(format!("View: {}", dialog.current_range().label))
                .style(Style::default().fg(Color::Gray)),
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
            Line::from("Recent changes are unavailable.").style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Line::from(error.clone()),
        ]
    } else if let Some(project) = app.config.projects.get(app.selected_project) {
        if uses_dashboard_placeholder(project) {
            placeholder_recent_changes_lines(project)
        } else {
            vec![Line::from(
                "Recent changes are not available for local-only projects.",
            )]
        }
    } else {
        vec![Line::from(
            "Recent changes are not available for local-only projects.",
        )]
    };
    let scroll = app
        .overview_recent_changes
        .as_ref()
        .map(|dialog| dialog.scroll)
        .unwrap_or(0);
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
    } else {
        app.overview_recent_error = None;
    }
}

pub(super) fn ensure_dashboard_tile_state(app: &mut App, scopes: &[BumpScope]) {
    if app.overview_tile_project == Some(app.selected_project)
        && app.overview_scope_order.len() == scopes.len()
        && app.overview_pending_versions.len() == scopes.len()
    {
        app.overview_focused_scope = app
            .overview_focused_scope
            .min(scopes.len().saturating_sub(1));
        return;
    }

    app.overview_tile_project = Some(app.selected_project);
    app.overview_scope_order = (0..scopes.len()).collect();
    let use_placeholder = app
        .config
        .projects
        .get(app.selected_project)
        .map(uses_dashboard_placeholder)
        .unwrap_or(false);
    app.overview_pending_versions = scopes
        .iter()
        .map(|scope| resolved_scope_preview_version(scope, use_placeholder))
        .collect();
    app.overview_tile_scroll = 0;
    app.overview_focused_scope = 0;
}

pub(super) fn invalidate_overview_cache(app: &mut App) {
    app.overview_recent_project = None;
    app.overview_tile_project = None;
    app.overview_activity_project = None;
    app.overview_activity_summaries.clear();
}

pub(super) fn reorder_dashboard_tile_scope(app: &mut App, from_scope: usize, to_scope: usize) {
    let Some(from_index) = app
        .overview_scope_order
        .iter()
        .position(|scope| *scope == from_scope)
    else {
        return;
    };
    let Some(to_index) = app
        .overview_scope_order
        .iter()
        .position(|scope| *scope == to_scope)
    else {
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
    app.overview_tile_scroll =
        (app.overview_tile_scroll as isize + delta).clamp(0, max_scroll as isize) as usize;
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
    let next_index =
        (current_index + delta).clamp(0, app.overview_scope_order.len() as isize - 1) as usize;
    let next_scope = app.overview_scope_order[next_index];
    select_dashboard_overview_scope(app, next_scope)?;
    ensure_dashboard_focus_visible(app, next_index, &scopes);
    Ok(())
}

pub(super) fn ensure_dashboard_focus_visible(
    app: &mut App,
    order_index: usize,
    scopes: &[BumpScope],
) {
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

    let columns = dashboard_tile_columns(area.width).max(1);
    let vertical_gap = 1;
    let row_height = scopes
        .iter()
        .map(|scope| tile_height(scope.scheme))
        .max()
        .unwrap_or(7)
        .saturating_add(vertical_gap);
    let visible_rows =
        ((area.height.saturating_add(vertical_gap)) / row_height.max(1)).max(1) as usize;
    let total_rows = app.overview_scope_order.len().div_ceil(columns);
    let max_scroll = total_rows.saturating_sub(visible_rows);
    app.overview_tile_scroll = app.overview_tile_scroll.min(max_scroll);

    let visible_row_scopes = (app.overview_tile_scroll
        ..(app.overview_tile_scroll + visible_rows).min(total_rows))
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
            .constraints(vec![
                Constraint::Length(TILE_WIDTH.min(area.width));
                row_scopes.len()
            ])
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

            let activity = if app.overview_activity_project == Some(app.selected_project) {
                app.overview_activity_summaries
                    .get(scope_index)
                    .and_then(|entry| entry.as_ref())
            } else {
                None
            };
            let selected = scope_index == app.overview_focused_scope;
            let placeholder = placeholder_activity(scope, project);
            let tile = OverviewTileData {
                name: scope.display_name.clone(),
                scheme: scope.scheme,
                preview_version: app
                    .overview_pending_versions
                    .get(scope_index)
                    .cloned()
                    .unwrap_or_else(|| {
                        resolved_scope_preview_version(scope, uses_dashboard_placeholder(project))
                    }),
                commits_since_tag_label: activity
                    .as_ref()
                    .map(|summary| summary.commits_since_tag_label.clone())
                    .or_else(|| {
                        placeholder
                            .as_ref()
                            .map(|data| data.commits_since_tag_label.to_string())
                    })
                    .unwrap_or_else(|| "n/a".to_string()),
                last_bump_label: activity
                    .as_ref()
                    .map(|summary| summary.last_bump_label.clone())
                    .or_else(|| {
                        placeholder
                            .as_ref()
                            .map(|data| data.last_bump_label.to_string())
                    })
                    .unwrap_or_else(|| "n/a".to_string()),
                last_commit_label: activity
                    .as_ref()
                    .map(|summary| summary.last_commit_label.clone())
                    .or_else(|| {
                        placeholder
                            .as_ref()
                            .map(|data| data.last_commit_label.to_string())
                    })
                    .unwrap_or_else(|| "n/a".to_string()),
                selected,
            };
            let hotspots = render_overview_tile(frame, tile_rect, &tile);
            app.overview_tile_rects
                .push((hotspots.tile_rect, scope_index));

            app.hit_targets.push(HitTarget::new(
                hotspots.title_rect,
                HitAction::SelectOverviewScope(scope_index),
            ));
            app.hit_targets.push(HitTarget::new(
                hotspots.view_rect,
                HitAction::OpenOverviewReleaseNow(scope_index),
            ));
            app.hit_targets.push(HitTarget::new(
                hotspots.bump_rect,
                HitAction::BeginOverviewBump(scope_index),
            ));
            app.hit_targets.push(HitTarget::new(
                hotspots.tag_rect,
                HitAction::OpenOverviewTagDialog(scope_index),
            ));
            if let Some(rect) = hotspots.reset_rect {
                app.hit_targets.push(HitTarget::new(
                    rect,
                    HitAction::ResetOverviewPendingVersion(scope_index),
                ));
            }
            if let Some(rect) = hotspots.major_rect {
                app.hit_targets.push(HitTarget::with_right_action(
                    rect,
                    HitAction::AdjustOverviewVersion(scope_index, OverviewVersionControl::Major, 1),
                    HitAction::AdjustOverviewVersion(
                        scope_index,
                        OverviewVersionControl::Major,
                        -1,
                    ),
                ));
            }
            if let Some(rect) = hotspots.minor_rect {
                app.hit_targets.push(HitTarget::with_right_action(
                    rect,
                    HitAction::AdjustOverviewVersion(scope_index, OverviewVersionControl::Minor, 1),
                    HitAction::AdjustOverviewVersion(
                        scope_index,
                        OverviewVersionControl::Minor,
                        -1,
                    ),
                ));
            }
            if let Some(rect) = hotspots.patch_rect {
                app.hit_targets.push(HitTarget::with_right_action(
                    rect,
                    HitAction::AdjustOverviewVersion(scope_index, OverviewVersionControl::Patch, 1),
                    HitAction::AdjustOverviewVersion(
                        scope_index,
                        OverviewVersionControl::Patch,
                        -1,
                    ),
                ));
            }
            if let Some(rect) = hotspots.version_rect {
                app.hit_targets.push(HitTarget::with_right_action(
                    rect,
                    HitAction::AdjustOverviewVersion(scope_index, OverviewVersionControl::Whole, 1),
                    HitAction::AdjustOverviewVersion(
                        scope_index,
                        OverviewVersionControl::Whole,
                        -1,
                    ),
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
            Paragraph::new("more scopes above")
                .alignment(Alignment::Right)
                .style(Style::default().fg(Color::DarkGray)),
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
            Paragraph::new("more scopes below")
                .alignment(Alignment::Right)
                .style(Style::default().fg(Color::DarkGray)),
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

fn resolved_scope_preview_version(scope: &BumpScope, use_placeholder: bool) -> String {
    scope
        .current_version
        .clone()
        .or_else(|| {
            (use_placeholder && scope.targets.is_empty()).then(|| PLACEHOLDER_VERSION.to_string())
        })
        .unwrap_or_else(|| scope.version_label().to_string())
}

fn uses_dashboard_placeholder(project: &ProjectConfig) -> bool {
    project.integration_mode == IntegrationMode::LocalOnly
        && match project.project_type {
            ProjectType::AllInOne => project.targets.is_empty(),
            ProjectType::Branched => {
                !project.branches.is_empty()
                    && project
                        .branches
                        .iter()
                        .all(|branch| branch.targets.is_empty())
            }
        }
}

fn placeholder_activity(
    scope: &BumpScope,
    project: &ProjectConfig,
) -> Option<OverviewPlaceholderData> {
    if !uses_dashboard_placeholder(project) || !scope.targets.is_empty() {
        return None;
    }

    Some(OverviewPlaceholderData {
        commits_since_tag_label: PLACEHOLDER_COMMITS_AHEAD,
        last_bump_label: PLACEHOLDER_LAST_BUMP,
        last_commit_label: PLACEHOLDER_LAST_COMMIT,
    })
}

fn placeholder_recent_changes_lines(project: &ProjectConfig) -> Vec<Line<'static>> {
    let scope_label = if project.project_type == ProjectType::Branched {
        project
            .branches
            .first()
            .map(|branch| {
                format!(
                    "{} ({})",
                    branch.display_name(),
                    branch.scope_kind.display_name()
                )
            })
            .unwrap_or_else(|| format!("{} (Project)", project.name))
    } else {
        format!("{} (Project)", project.name)
    };

    vec![
        Line::from(format!("Scope: {}", scope_label)).style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Line::from("View: Example history").style(Style::default().fg(Color::Gray)),
        Line::raw(""),
        Line::from("* 9f4c2d1 chore(release): prepare 1.2.3"),
        Line::from("* 4b871aa feat(ui): polish overview placeholders"),
        Line::from("* 8c02e6d fix(versioning): keep scope bump previews in sync"),
    ]
}

struct OverviewPlaceholderData {
    commits_since_tag_label: &'static str,
    last_bump_label: &'static str,
    last_commit_label: &'static str,
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
        .or_else(|| {
            scopes
                .get(scope_index)
                .and_then(|scope| scope.current_version.clone())
        })
        .ok_or_else(|| anyhow!("the selected scope does not have a resolved version value"))?;
    let scope_label = if project.unified_versioning {
        "All configured scopes".to_string()
    } else {
        scopes
            .get(scope_index)
            .map(|scope| scope.display_name.clone())
            .unwrap_or_else(|| project.name.clone())
    };
    let options = overview_bump_workflow_options(project.integration_mode);

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
    app.overview_branch_bump_dialog = None;
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
    app.overview_branch_bump_dialog = None;
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
        .unwrap_or_else(|| {
            resolved_scope_preview_version(scope, uses_dashboard_placeholder(&project))
        });
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
    let restored = resolved_scope_preview_version(scope, uses_dashboard_placeholder(&project));
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

pub(super) async fn build_dashboard_changelog_preview_dialog_async(
    project: &ProjectConfig,
    focused_scope: usize,
    pending_versions: &[String],
    selection: Option<CustomChangelogSelection>,
    cancel: Option<GitCancellation>,
) -> Result<ChangelogPreviewDialog> {
    if !project.integration_mode.requires_repo() {
        bail!("changelog preview requires a git-backed project");
    }

    let scopes = collect_bump_scopes(project)?;
    if scopes.is_empty() {
        bail!("no changelog preview is available because the project has no version scopes");
    }

    let scope_index = focused_scope.min(scopes.len().saturating_sub(1));
    if !project.changelog_enabled_for_scope(scope_index) {
        bail!("changelog generation is disabled for the selected scope");
    }
    let affected_scope_indexes = if project.unified_versioning {
        (0..scopes.len()).collect::<Vec<_>>()
    } else {
        vec![scope_index]
    };
    let enabled_scope_indexes = affected_scope_indexes
        .iter()
        .copied()
        .filter(|index| project.changelog_enabled_for_scope(*index))
        .collect::<Vec<_>>();
    let next_version = pending_versions
        .get(scope_index)
        .cloned()
        .or_else(|| {
            scopes
                .get(scope_index)
                .and_then(|scope| scope.current_version.clone())
        })
        .unwrap_or_else(|| scopes[scope_index].version_label().to_string());
    if enabled_scope_indexes.is_empty() {
        bail!("changelog generation is disabled for the affected scope set");
    }

    let git_contexts = collect_all_branch_git_scope_contexts(project)?;
    let selected_context = git_contexts
        .get(scope_index)
        .or_else(|| git_contexts.first())
        .ok_or_else(|| anyhow!("git scope metadata is unavailable for changelog preview"))?;
    let tags = sorted_local_tags_with_cancel(&selected_context.repo_root, cancel.clone())?;
    let custom_range = (!tags.is_empty()).then(|| {
        CustomChangelogRangeState::new(selected_context.display_name.clone(), tags, selection)
    });
    let changelog_entries = collect_preview_entries_async(
        project,
        &git_contexts,
        &enabled_scope_indexes,
        &next_version,
        custom_range.as_ref(),
        cancel,
    )
    .await?;
    if changelog_entries.is_empty() {
        bail!("no changelog content was generated from the current git history");
    }

    Ok(ChangelogPreviewDialog::preview_only(
        project.name.clone(),
        next_version,
        scope_index,
        custom_range,
        changelog_entries,
    ))
}

pub(super) fn apply_overview_pending_version(
    app: &mut App,
    scope_index: usize,
    open_tag_after: bool,
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
        .or_else(|| {
            scopes
                .get(scope_index)
                .and_then(|scope| scope.current_version.clone())
        })
        .ok_or_else(|| anyhow!("the selected scope does not have a resolved version value"))?;

    for index in &affected_scope_indexes {
        if let Some(scope) = scopes.get(*index) {
            for target in &scope.targets {
                write_target_version(target, &next_version)?;
                refresh_target_artifacts(
                    target,
                    scope_repo_roots
                        .get(*index)
                        .and_then(|root| root.as_deref()),
                )?;
            }
            if let Some(pending) = app.overview_pending_versions.get_mut(*index) {
                *pending = next_version.clone();
            }
        }
    }

    app.sync_dashboard_overview_after_repo_change();

    if open_tag_after {
        if project.integration_mode.requires_repo() {
            let preferred_scope = if project.unified_versioning {
                None
            } else {
                Some(scope_index)
            };
            app.open_tag_dialog_with_scope(preferred_scope, Some(TagAction::CreateAndPush))?;
            app.status =
                StatusMessage::info("Version updated. Review the tag-and-push action next.");
        } else {
            app.status = StatusMessage::warning("Tagging requires a git-backed project.");
        }
    } else {
        app.status = StatusMessage::success(format!(
            "Updated version to {} from the overview tile.",
            next_version
        ));
    }

    Ok(())
}

pub(super) fn confirm_overview_bump_workflow(app: &mut App) -> Result<()> {
    let Some(dialog) = app.overview_bump_workflow_dialog.clone() else {
        return Ok(());
    };
    let workflow = dialog.selected_workflow();

    if workflow.requires_branch() {
        app.overview_branch_bump_dialog = Some(OverviewBranchBumpDialog::new(
            dialog.project_name,
            dialog.scope_label,
            dialog.next_version,
            dialog.scope_index,
            workflow,
        ));
        app.status = StatusMessage::info("Enter the new branch name for the bump workflow.");
        return Ok(());
    }

    if workflow != OverviewBumpWorkflow::JustBump {
        let project = app.selected_project()?.clone();
        app.schedule_progress_job(
            " Checking Staged Files ",
            "Checking repositories for previously staged files before committing the bump.",
            BackgroundJobRequest::CheckOverviewBumpWarnings {
                project,
                scope_index: dialog.scope_index,
                workflow,
            },
        )?;
        app.status = StatusMessage::info(
            "Checking repositories for previously staged files before committing the bump.",
        );
        return Ok(());
    }

    continue_overview_bump_workflow_confirmation(app, dialog.scope_index, workflow)
}

pub(super) fn confirm_overview_branch_bump(app: &mut App) -> Result<()> {
    let Some(dialog) = app.overview_branch_bump_dialog.clone() else {
        return Ok(());
    };

    if dialog.branch_name.value.trim().is_empty() {
        bail!("branch name cannot be empty");
    }

    let project = app.selected_project()?.clone();
    app.schedule_progress_job(
        " Checking Staged Files ",
        "Checking repositories for previously staged files before committing the bump.",
        BackgroundJobRequest::CheckOverviewBumpWarnings {
            project,
            scope_index: dialog.scope_index,
            workflow: dialog.workflow,
        },
    )?;
    app.status = StatusMessage::info(
        "Checking repositories for previously staged files before committing the bump.",
    );
    Ok(())
}

pub(super) fn confirm_overview_bump_warning(app: &mut App) -> Result<()> {
    let Some(dialog) = app.overview_bump_warning_dialog.clone() else {
        return Ok(());
    };
    let branch_name = if dialog.workflow.requires_branch() {
        app.overview_branch_bump_dialog
            .as_ref()
            .map(|branch_dialog| branch_dialog.branch_name.value.trim().to_string())
    } else {
        None
    };

    match dialog.selected_choice() {
        OverviewBumpWarningChoice::Continue => {
            if should_open_overview_changelog_preview(app, dialog.scope_index, dialog.workflow)? {
                app.schedule_overview_workflow_changelog_preview(
                    dialog.scope_index,
                    dialog.workflow,
                )?;
                app.overview_bump_warning_dialog = None;
                return Ok(());
            }
            execute_overview_bump_workflow(
                app,
                dialog.scope_index,
                dialog.workflow,
                branch_name.as_deref(),
            )?;
            app.overview_bump_warning_dialog = None;
            app.overview_branch_bump_dialog = None;
            app.overview_bump_workflow_dialog = None;
        }
        OverviewBumpWarningChoice::UnstageExtras => {
            for repo in &dialog.repos {
                unstage_paths(&repo.repo_root, &repo.extra_paths)?;
            }
            if should_open_overview_changelog_preview(app, dialog.scope_index, dialog.workflow)? {
                app.schedule_overview_workflow_changelog_preview(
                    dialog.scope_index,
                    dialog.workflow,
                )?;
                app.overview_bump_warning_dialog = None;
                return Ok(());
            }
            execute_overview_bump_workflow(
                app,
                dialog.scope_index,
                dialog.workflow,
                branch_name.as_deref(),
            )?;
            app.overview_bump_warning_dialog = None;
            app.overview_branch_bump_dialog = None;
            app.overview_bump_workflow_dialog = None;
        }
        OverviewBumpWarningChoice::Cancel => cancel_overview_bump_warning(app),
    }
    Ok(())
}

pub(super) fn collect_overview_bump_warnings(
    project: &ProjectConfig,
    scope_index: usize,
    cancel: Option<GitCancellation>,
) -> Result<Vec<UnexpectedStagedRepo>> {
    let scopes = collect_bump_scopes(project)?;
    let affected_scope_indexes = if project.unified_versioning {
        (0..scopes.len()).collect::<Vec<_>>()
    } else {
        vec![scope_index]
    };
    let git_contexts = collect_all_branch_git_scope_contexts(project)?;
    let repo_operations =
        collect_repo_bump_operations(project, &scopes, &git_contexts, &affected_scope_indexes)?;
    collect_unexpected_staged_paths_with_cancel(&repo_operations, cancel)
}

pub(super) fn continue_overview_bump_workflow_confirmation(
    app: &mut App,
    scope_index: usize,
    workflow: OverviewBumpWorkflow,
) -> Result<()> {
    if should_open_overview_changelog_preview(app, scope_index, workflow)? {
        app.schedule_overview_workflow_changelog_preview(scope_index, workflow)?;
        return Ok(());
    }

    let branch_name = if workflow.requires_branch() {
        app.overview_branch_bump_dialog
            .as_ref()
            .map(|dialog| dialog.branch_name.value.trim().to_string())
    } else {
        None
    };

    execute_overview_bump_workflow(app, scope_index, workflow, branch_name.as_deref())?;
    app.overview_branch_bump_dialog = None;
    app.overview_bump_workflow_dialog = None;
    Ok(())
}

pub(super) fn execute_overview_bump_workflow(
    app: &mut App,
    scope_index: usize,
    workflow: OverviewBumpWorkflow,
    branch_name: Option<&str>,
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
        .or_else(|| {
            scopes
                .get(scope_index)
                .and_then(|scope| scope.current_version.clone())
        })
        .ok_or_else(|| anyhow!("the selected scope does not have a resolved version value"))?;

    for index in &affected_scope_indexes {
        if let Some(scope) = scopes.get(*index) {
            for target in &scope.targets {
                write_target_version(target, &next_version)?;
                refresh_target_artifacts(
                    target,
                    scope_repo_roots
                        .get(*index)
                        .and_then(|root| root.as_deref()),
                )?;
            }
            if let Some(pending) = app.overview_pending_versions.get_mut(*index) {
                *pending = next_version.clone();
            }
        }
    }

    if workflow != OverviewBumpWorkflow::JustBump {
        let git_contexts = collect_all_branch_git_scope_contexts(&project)?;
        let mut repo_operations = collect_repo_bump_operations(
            &project,
            &scopes,
            &git_contexts,
            &affected_scope_indexes,
        )?;

        if workflow.requires_branch() {
            let branch_name = branch_name
                .ok_or_else(|| anyhow!("the selected workflow requires a branch name"))?;
            for operation in &repo_operations {
                switch_or_create_branch(&operation.repo_root, branch_name)?;
            }
        }

        if let Some(pending_changelog) =
            app.take_matching_pending_changelog_write(scope_index, workflow)
        {
            for entry in &pending_changelog.entries {
                write_changelog_markdown(&entry.repo_root, &entry.changelog_path, &entry.markdown)?;
                let history_path =
                    archive_changelog_markdown(&entry.repo_root, &next_version, &entry.markdown)?;
                let summary_path = sum_changelog_gen(&entry.repo_root)?;
                append_repo_stage_paths(
                    &mut repo_operations,
                    &entry.repo_root,
                    &[
                        entry.stage_path.clone(),
                        stage_path_for_file(&entry.repo_root, &history_path.to_string_lossy()),
                        stage_path_for_file(&entry.repo_root, &summary_path.to_string_lossy()),
                    ],
                );
            }
        }
        apply_repo_bump_workflow(&repo_operations, &next_version, workflow, branch_name)?;
    }

    app.sync_dashboard_overview_after_repo_change();

    app.overview_branch_bump_dialog = None;
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

pub(super) async fn build_overview_workflow_changelog_preview_dialog_async(
    project: &ProjectConfig,
    scope_index: usize,
    workflow: OverviewBumpWorkflow,
    pending_versions: &[String],
    cancel: Option<GitCancellation>,
) -> Result<ChangelogPreviewDialog> {
    if !workflow.requires_tag() {
        bail!("the selected workflow does not require changelog generation");
    }
    if !project.integration_mode.requires_repo() {
        bail!("changelog generation is not available for this project");
    }

    let scopes = collect_bump_scopes(project)?;
    if scopes.is_empty() {
        bail!("no changelog preview is available because the project has no version scopes");
    }

    let scope_index = scope_index.min(scopes.len().saturating_sub(1));
    if !project.changelog_enabled_for_scope(scope_index) {
        bail!("changelog generation is disabled for the selected scope");
    }
    let affected_scope_indexes = if project.unified_versioning {
        (0..scopes.len()).collect::<Vec<_>>()
    } else {
        vec![scope_index]
    };
    let enabled_scope_indexes = affected_scope_indexes
        .iter()
        .copied()
        .filter(|index| project.changelog_enabled_for_scope(*index))
        .collect::<Vec<_>>();
    let next_version = pending_versions
        .get(scope_index)
        .cloned()
        .or_else(|| {
            scopes
                .get(scope_index)
                .and_then(|scope| scope.current_version.clone())
        })
        .ok_or_else(|| anyhow!("the selected scope does not have a resolved version value"))?;
    if enabled_scope_indexes.is_empty() {
        bail!("changelog generation is disabled for the affected scope set");
    }

    let git_contexts = collect_all_branch_git_scope_contexts(project)?;
    let changelog_entries = collect_preview_entries_async(
        project,
        &git_contexts,
        &enabled_scope_indexes,
        &next_version,
        None,
        cancel,
    )
    .await?;
    if changelog_entries.is_empty() {
        bail!("no changelog content was generated from the current git history");
    }

    Ok(ChangelogPreviewDialog::new(
        project.name.clone(),
        next_version,
        scope_index,
        workflow,
        changelog_entries,
    ))
}

fn should_open_overview_changelog_preview(
    app: &mut App,
    scope_index: usize,
    workflow: OverviewBumpWorkflow,
) -> Result<bool> {
    if !workflow.requires_tag() {
        return Ok(false);
    }

    let project = app.selected_project()?.clone();
    if !project.integration_mode.requires_repo()
        || !project.changelog_enabled_for_scope(scope_index)
    {
        return Ok(false);
    }
    Ok(true)
}

fn collect_preview_contexts(
    project: &ProjectConfig,
    git_contexts: &[crate::git::GitScopeContext],
    affected_scope_indexes: &[usize],
) -> Result<Vec<(crate::git::GitScopeContext, String)>> {
    let mut merged_contexts = Vec::<(crate::git::GitScopeContext, String)>::new();
    for scope_index in affected_scope_indexes {
        let context = git_contexts
            .get(*scope_index)
            .or_else(|| git_contexts.first())
            .ok_or_else(|| anyhow!("git scope metadata is unavailable for changelog preview"))?;
        let changelog_path = project.changelog_path_for_scope(*scope_index).to_string();

        if let Some((existing, _)) = merged_contexts
            .iter_mut()
            .find(|(existing, existing_path)| {
                existing.repo_root == context.repo_root && *existing_path == changelog_path
            })
        {
            for path in &context.path_filters {
                if !existing
                    .path_filters
                    .iter()
                    .any(|candidate| candidate == path)
                {
                    existing.path_filters.push(path.clone());
                }
            }
        } else {
            merged_contexts.push((context.clone(), changelog_path));
        }
    }

    Ok(merged_contexts)
}

async fn collect_preview_entries_async(
    project: &ProjectConfig,
    git_contexts: &[crate::git::GitScopeContext],
    affected_scope_indexes: &[usize],
    next_version: &str,
    custom_range: Option<&CustomChangelogRangeState>,
    cancel: Option<GitCancellation>,
) -> Result<Vec<ChangelogPreviewEntry>> {
    let merged_contexts = collect_preview_contexts(project, git_contexts, affected_scope_indexes)?;
    let custom_selection = custom_range.and_then(CustomChangelogRangeState::selection);
    let semaphore = Arc::new(Semaphore::new(BACKGROUND_MAX_PARALLEL_REPO_JOBS.max(1)));
    let mut tasks = JoinSet::new();

    for (context, changelog_path) in merged_contexts {
        let semaphore = Arc::clone(&semaphore);
        let next_version = next_version.to_string();
        let custom_selection = custom_selection.clone();
        let cancel = cancel.clone();
        tasks.spawn(async move {
            let _permit = semaphore
                .acquire_owned()
                .await
                .map_err(|_| anyhow!("preview worker pool is unavailable"))?;
            run_blocking_job(move || {
                let recent_range = if let Some(selection) = &custom_selection {
                    load_change_range_for_refs_with_cancel(
                        &context,
                        &selection.from_ref,
                        selection.to_ref.as_deref().unwrap_or("HEAD"),
                        cancel,
                    )?
                } else {
                    load_recent_change_range_with_cancel(&context, cancel)?
                };
                Ok(ChangelogPreviewEntry {
                    repo_root: context.repo_root.clone(),
                    changelog_path: changelog_path.clone(),
                    stage_path: stage_path_for_file(&context.repo_root, &changelog_path),
                    document: build_document_from_git_log(next_version, &recent_range.lines),
                })
            })
            .await
        });
    }

    let mut entries = Vec::new();
    while let Some(result) = tasks.join_next().await {
        entries.push(result.map_err(|error| anyhow!("preview task failed: {error}"))??);
    }

    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{BranchConfig, ChangelogSettings, ReleaseNowSettings};

    #[test]
    fn empty_local_only_project_uses_dashboard_placeholders() {
        let project = ProjectConfig {
            name: "demo".to_string(),
            alias: String::new(),
            project_type: ProjectType::AllInOne,
            integration_mode: IntegrationMode::LocalOnly,
            unified_versioning: true,
            version_scheme: VersionScheme::SemVer,
            changelog: ChangelogSettings::default(),
            release_now: ReleaseNowSettings::default(),
            targets: Vec::new(),
            branches: Vec::new(),
            repo: None,
        };
        let scope = BumpScope {
            display_name: "demo".to_string(),
            scope_kind: None,
            scheme: VersionScheme::SemVer,
            current_version: None,
            targets: Vec::new(),
        };

        assert!(uses_dashboard_placeholder(&project));
        assert_eq!(
            resolved_scope_preview_version(&scope, true),
            PLACEHOLDER_VERSION
        );
        let placeholder =
            placeholder_activity(&scope, &project).expect("placeholder data should exist");
        assert_eq!(
            placeholder.commits_since_tag_label,
            PLACEHOLDER_COMMITS_AHEAD
        );
    }

    #[test]
    fn configured_branched_project_keeps_real_scope_versions() {
        let project = ProjectConfig {
            name: "demo".to_string(),
            alias: String::new(),
            project_type: ProjectType::Branched,
            integration_mode: IntegrationMode::GitLocalOnly,
            unified_versioning: false,
            version_scheme: VersionScheme::SemVer,
            changelog: ChangelogSettings::default(),
            release_now: ReleaseNowSettings::default(),
            targets: Vec::new(),
            branches: vec![BranchConfig {
                name: "core".to_string(),
                label: String::new(),
                scope_kind: BranchScopeKind::Branch,
                repo: None,
                changelog_enabled: false,
                changelog_path: None,
                release_now: ReleaseNowSettings::default(),
                version_scheme: VersionScheme::SemVer,
                targets: vec![TargetSpec {
                    label: "Cargo".to_string(),
                    path: "Cargo.toml".to_string(),
                    key_path: "package.version".to_string(),
                    format: TargetFormat::Toml,
                }],
            }],
            repo: None,
        };
        let scope = BumpScope {
            display_name: "core".to_string(),
            scope_kind: Some(BranchScopeKind::Branch),
            scheme: VersionScheme::SemVer,
            current_version: Some("2.4.6".to_string()),
            targets: vec![BumpTarget {
                label: "Cargo".to_string(),
                path: "Cargo.toml".to_string(),
                key_path: "package.version".to_string(),
                format: TargetFormat::Toml,
                current_version: "2.4.6".to_string(),
            }],
        };

        assert!(!uses_dashboard_placeholder(&project));
        assert_eq!(resolved_scope_preview_version(&scope, false), "2.4.6");
        assert!(placeholder_activity(&scope, &project).is_none());
    }

    #[test]
    fn changelog_preview_opens_only_for_tag_workflows() {
        let mut app = App::new_for_tests().expect("app should initialize");
        let changelog = ChangelogSettings {
            enabled: true,
            ..ChangelogSettings::default()
        };
        app.config.projects = vec![ProjectConfig {
            name: "demo".to_string(),
            alias: String::new(),
            project_type: ProjectType::AllInOne,
            integration_mode: IntegrationMode::GitLocalOnly,
            unified_versioning: true,
            version_scheme: VersionScheme::SemVer,
            changelog,
            release_now: ReleaseNowSettings::default(),
            targets: Vec::new(),
            branches: Vec::new(),
            repo: None,
        }];
        app.selected_project = 0;

        assert!(
            !should_open_overview_changelog_preview(&mut app, 0, OverviewBumpWorkflow::Commit)
                .expect("check should succeed")
        );
        assert!(
            should_open_overview_changelog_preview(&mut app, 0, OverviewBumpWorkflow::CommitAndTag)
                .expect("check should succeed")
        );
    }
}
