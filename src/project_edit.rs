// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyVersionBumper License v1.2
//
// For details, see the LICENSE file in the repository root.

use std::{collections::HashSet, path::Path};

use anyhow::{Result, anyhow, bail};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::text::Line;

use crate::{
	app::{
		HitAction, ScopeAction, ScopeDraft, clamp_dialog_scroll, cycle_target_key_preset,
		default_target_key_for_path, derive_repo_root_from_target_path,
		dialog_form_row_height, dialog_visible_rows, rotate_scope_kind,
	},
	config::{
		BranchConfig, BranchScopeKind, ChangelogSettings, IntegrationMode, ProjectConfig,
		ProjectType, RepoConfig, TargetSpec, DEFAULT_CHANGELOG_PATH,
	},
	dialogs::TextInput,
	versioning::VersionScheme,
};

#[derive(Clone)]
pub(crate) struct ProjectEditDialog {
	pub(crate) project_index: usize,
	pub(crate) project_name: String,
	pub(crate) name: TextInput,
	pub(crate) target_path: TextInput,
	pub(crate) target_key: TextInput,
	pub(crate) target_key_custom: bool,
	pub(crate) scopes: Vec<ScopeDraft>,
	pub(crate) selected_scope: usize,
	pub(crate) field_scroll: usize,
	pub(crate) viewport_rows: usize,
	pub(crate) repo_root: TextInput,
	pub(crate) remote_url: TextInput,
	pub(crate) changelog_path: TextInput,
	pub(crate) project_type: ProjectType,
	pub(crate) unified_versioning: bool,
	pub(crate) integration_mode: IntegrationMode,
	pub(crate) version_scheme: VersionScheme,
	pub(crate) focus: ProjectEditFocus,
}

impl ProjectEditDialog {
	pub(crate) fn from_project(project_index: usize, project: &ProjectConfig) -> Result<Self> {
		let primary_target = if project.project_type == ProjectType::AllInOne {
			project.targets.first()
		} else {
			project.branches.first().and_then(|branch| branch.targets.first())
		}
		.ok_or_else(|| anyhow!("selected project does not contain any editable targets yet"))?;

		let scopes = if project.project_type == ProjectType::Branched {
			project
				.branches
				.iter()
				.map(ScopeDraft::from_branch)
				.collect::<Result<Vec<_>>>()?
		} else {
			vec![ScopeDraft::from_target("core", primary_target)]
		};

		let repo_root = project.repo.as_ref().map(|repo| repo.local_root.clone()).unwrap_or_default();
		let remote_url = project
			.repo
			.as_ref()
			.and_then(|repo| repo.remote_url.clone())
			.unwrap_or_default();

		Ok(Self {
			project_index,
			project_name: project.name.clone(),
			name: TextInput::with_value(project.name.clone()),
			target_path: TextInput::with_value(primary_target.path.clone()),
			target_key: TextInput::with_value(primary_target.key_path.clone()),
			target_key_custom: crate::app::target_key_is_custom(&primary_target.path, &primary_target.key_path),
			scopes,
			selected_scope: 0,
			field_scroll: 0,
			viewport_rows: 1,
			repo_root: TextInput::with_value(repo_root),
			remote_url: TextInput::with_value(remote_url),
			changelog_path: TextInput::with_value(project.changelog.effective_path().to_string()),
			project_type: project.project_type,
			unified_versioning: project.unified_versioning,
			integration_mode: project.integration_mode,
			version_scheme: project.version_scheme,
			focus: ProjectEditFocus::Name,
		})
	}

	pub(crate) fn focus_next(&mut self) {
		let fields = self.visible_fields();
		let index = fields.iter().position(|field| *field == self.focus).unwrap_or(0);
		self.focus = fields[(index + 1) % fields.len()];
		self.ensure_focus_visible();
	}

	pub(crate) fn focus_previous(&mut self) {
		let fields = self.visible_fields();
		let index = fields.iter().position(|field| *field == self.focus).unwrap_or(0);
		self.focus = fields[(index + fields.len() - 1) % fields.len()];
		self.ensure_focus_visible();
	}

	pub(crate) fn is_save_focused(&self) -> bool {
		self.focus == ProjectEditFocus::Save
	}

	pub(crate) fn is_remove_focused(&self) -> bool {
		self.focus == ProjectEditFocus::Remove
	}

	pub(crate) fn is_cancel_focused(&self) -> bool {
		self.focus == ProjectEditFocus::Cancel
	}

	pub(crate) fn focus_accepts_text(&self) -> bool {
		matches!(
			self.focus,
			ProjectEditFocus::Name
				| ProjectEditFocus::ScopeName
				| ProjectEditFocus::TargetPath
				| ProjectEditFocus::RepoRoot
				| ProjectEditFocus::RemoteUrl
		) || (self.focus == ProjectEditFocus::TargetKey && self.target_key_accepts_text())
	}

	fn visible_fields(&self) -> Vec<ProjectEditFocus> {
		let mut fields = vec![ProjectEditFocus::Name, ProjectEditFocus::ProjectType];
		if self.project_type == ProjectType::Branched {
			fields.extend([
				ProjectEditFocus::ScopeSelection,
				ProjectEditFocus::ScopeName,
				ProjectEditFocus::ScopeKind,
				ProjectEditFocus::UnifiedVersioning,
			]);
		}
		fields.extend([
			ProjectEditFocus::VersionScheme,
			ProjectEditFocus::IntegrationMode,
			ProjectEditFocus::TargetPath,
			ProjectEditFocus::TargetKey,
		]);
		if self.project_type == ProjectType::Branched {
			fields.extend([
				ProjectEditFocus::AddScope,
				ProjectEditFocus::RemoveScope,
				ProjectEditFocus::MoveScopeUp,
				ProjectEditFocus::MoveScopeDown,
			]);
		}
		if self.integration_mode.requires_repo() {
			fields.push(ProjectEditFocus::RepoRoot);
		}
		if self.integration_mode.requires_remote() {
			fields.push(ProjectEditFocus::RemoteUrl);
		}
		fields.extend([ProjectEditFocus::Save, ProjectEditFocus::Remove, ProjectEditFocus::Cancel]);
		fields
	}

	fn body_fields(&self) -> Vec<ProjectEditFocus> {
		self.visible_fields()
			.into_iter()
			.filter(|field| !matches!(field, ProjectEditFocus::Save | ProjectEditFocus::Remove | ProjectEditFocus::Cancel))
			.collect()
	}

	pub(crate) fn render_field(&self, field: ProjectEditFocus) -> (&'static str, HitAction) {
		match field {
			ProjectEditFocus::Name => ("Project name", HitAction::EditProjectField(field)),
			ProjectEditFocus::ProjectType => ("Project type", HitAction::EditProjectField(field)),
			ProjectEditFocus::ScopeSelection => ("Scope", HitAction::EditProjectField(field)),
			ProjectEditFocus::ScopeName => ("Scope name", HitAction::EditProjectField(field)),
			ProjectEditFocus::ScopeKind => ("Scope kind", HitAction::EditProjectField(field)),
			ProjectEditFocus::VersionScheme => ("Version scheme", HitAction::EditProjectField(field)),
			ProjectEditFocus::UnifiedVersioning => ("Unified versioning", HitAction::EditProjectField(field)),
			ProjectEditFocus::IntegrationMode => ("Integration", HitAction::EditProjectField(field)),
			ProjectEditFocus::TargetPath => ("Target path", HitAction::EditProjectField(field)),
			ProjectEditFocus::TargetKey => ("Target key", HitAction::EditProjectField(field)),
			ProjectEditFocus::AddScope => ("Add scope", HitAction::ProjectEditScopeAction(ScopeAction::Add)),
			ProjectEditFocus::RemoveScope => ("Remove scope", HitAction::ProjectEditScopeAction(ScopeAction::Remove)),
			ProjectEditFocus::MoveScopeUp => ("Move scope up", HitAction::ProjectEditScopeAction(ScopeAction::MoveUp)),
			ProjectEditFocus::MoveScopeDown => ("Move scope down", HitAction::ProjectEditScopeAction(ScopeAction::MoveDown)),
			ProjectEditFocus::RepoRoot => ("Repo root", HitAction::EditProjectField(field)),
			ProjectEditFocus::RemoteUrl => ("Remote URL", HitAction::EditProjectField(field)),
			ProjectEditFocus::Save => ("Save", HitAction::SaveProjectEdit),
			ProjectEditFocus::Remove => ("Remove", HitAction::RemoveProject),
			ProjectEditFocus::Cancel => ("Cancel", HitAction::CancelProjectEdit),
		}
	}

	pub(crate) fn display_value_for_field(&self, field: ProjectEditFocus, focused: bool, max_width: usize) -> Line<'static> {
		match field {
			ProjectEditFocus::Name => self.name.display_line_with_width(focused, max_width),
			ProjectEditFocus::ProjectType => Line::from(format!("< {} >", self.project_type.display_name())),
			ProjectEditFocus::ScopeSelection => Line::from(self.selected_scope_summary()),
			ProjectEditFocus::ScopeName => self
				.current_scope()
				.map(|scope| scope.name.display_line_with_width(focused, max_width))
				.unwrap_or_else(|| Line::from("(no scope)")),
			ProjectEditFocus::ScopeKind => self
				.current_scope()
				.map(|scope| Line::from(format!("< {} >", scope.scope_kind.display_name())))
				.unwrap_or_else(|| Line::from(format!("< {} >", BranchScopeKind::Branch.display_name()))),
			ProjectEditFocus::VersionScheme => Line::from(format!("< {} >", self.version_scheme.display_name())),
			ProjectEditFocus::UnifiedVersioning => {
				if self.project_type == ProjectType::Branched {
					Line::from(format!("< {} >", if self.unified_versioning { "Yes" } else { "No" }))
				} else {
					Line::from("Always yes for all-in-one projects")
				}
			}
			ProjectEditFocus::IntegrationMode => Line::from(format!("< {} >", self.integration_mode.display_name())),
			ProjectEditFocus::TargetPath => {
				if self.project_type == ProjectType::Branched {
					self.current_scope()
						.map(|scope| scope.target_path.display_line_with_width(focused, max_width))
						.unwrap_or_else(|| Line::from(String::new()))
				} else {
					self.target_path.display_line_with_width(focused, max_width)
				}
			}
			ProjectEditFocus::TargetKey => {
				if self.project_type == ProjectType::Branched {
					self.current_scope()
						.map(|scope| {
							if scope.target_key_custom {
								scope.target_key.display_line_with_width(focused, max_width)
							} else {
								Line::from(format!("< {} >", scope.target_key.value()))
							}
						})
						.unwrap_or_else(|| Line::from(String::new()))
				} else if self.target_key_custom {
					self.target_key.display_line_with_width(focused, max_width)
				} else {
					Line::from(format!("< {} >", self.target_key.value()))
				}
			}
			ProjectEditFocus::AddScope => Line::from("Create a new scope draft"),
			ProjectEditFocus::RemoveScope => Line::from("Drop the selected scope"),
			ProjectEditFocus::MoveScopeUp => Line::from("Move the selected scope earlier"),
			ProjectEditFocus::MoveScopeDown => Line::from("Move the selected scope later"),
			ProjectEditFocus::RepoRoot => self.repo_root.display_line_with_width(focused, max_width),
			ProjectEditFocus::RemoteUrl => self.remote_url.display_line_with_width(focused, max_width),
			ProjectEditFocus::Save => Line::from("Persist project"),
			ProjectEditFocus::Remove => Line::from("Delete project"),
			ProjectEditFocus::Cancel => Line::from("Discard changes"),
		}
	}

	pub(crate) fn adjust_current_enum(&mut self, delta: i32) {
		match self.focus {
			ProjectEditFocus::ProjectType => {
				self.project_type = if delta >= 0 { self.project_type.next() } else { self.project_type.previous() };
				if self.project_type == ProjectType::Branched {
					self.seed_scope_from_primary_target();
				} else {
					self.copy_selected_scope_to_primary_target();
				}
			}
			ProjectEditFocus::ScopeSelection => self.move_scope_selection(delta),
			ProjectEditFocus::ScopeKind => {
				if let Some(scope) = self.current_scope_mut() {
					scope.scope_kind = rotate_scope_kind(scope.scope_kind, delta);
				}
			}
			ProjectEditFocus::TargetKey => self.rotate_target_key_preset(delta),
			ProjectEditFocus::VersionScheme => {
				self.version_scheme = if delta >= 0 { self.version_scheme.next() } else { self.version_scheme.previous() };
			}
			ProjectEditFocus::UnifiedVersioning => {
				if self.project_type == ProjectType::Branched {
					self.unified_versioning = !self.unified_versioning;
				}
			}
			ProjectEditFocus::IntegrationMode => {
				self.integration_mode = if delta >= 0 { self.integration_mode.next() } else { self.integration_mode.previous() };
			}
			_ => {}
		}
		self.prefill_repo_root_from_target_path();
		self.ensure_focus_visible();
	}

	pub(crate) fn handle_text_input(&mut self, key: KeyEvent) {
		let Some(input) = self.active_input_mut() else {
			return;
		};
		match key.code {
			KeyCode::Char(_) | KeyCode::Backspace | KeyCode::Delete | KeyCode::Left | KeyCode::Right | KeyCode::Home | KeyCode::End => {
				input.handle_key(key);
			}
			_ => {}
		}
		if matches!(key.code, KeyCode::Char(_) | KeyCode::Backspace | KeyCode::Delete) {
			if self.focus == ProjectEditFocus::ScopeName {
				if let Some(scope) = self.current_scope_mut() {
					scope.sync_label_if_needed();
				}
			}
			if self.focus == ProjectEditFocus::TargetPath {
				self.sync_target_key_preset_with_path();
				self.prefill_repo_root_from_target_path();
			}
		}
	}

	pub(crate) fn insert_text(&mut self, text: &str) -> bool {
		if let Some(input) = self.active_input_mut() {
			input.insert_str(text);
			if self.focus == ProjectEditFocus::TargetPath {
				self.prefill_repo_root_from_target_path();
			}
			return true;
		}
		false
	}

	pub(crate) fn active_input_mut(&mut self) -> Option<&mut TextInput> {
		match self.focus {
			ProjectEditFocus::Name => Some(&mut self.name),
			ProjectEditFocus::ScopeName => self.current_scope_mut().map(|scope| &mut scope.name),
			ProjectEditFocus::TargetPath => {
				if self.project_type == ProjectType::Branched {
					self.current_scope_mut().map(|scope| &mut scope.target_path)
				} else {
					Some(&mut self.target_path)
				}
			}
			ProjectEditFocus::TargetKey => {
				if self.project_type == ProjectType::Branched && self.current_scope().is_some_and(|scope| scope.target_key_custom) {
					self.current_scope_mut().map(|scope| &mut scope.target_key)
				} else if self.project_type != ProjectType::Branched && self.target_key_custom {
					Some(&mut self.target_key)
				} else {
					None
				}
			}
			ProjectEditFocus::RepoRoot => Some(&mut self.repo_root),
			ProjectEditFocus::RemoteUrl => Some(&mut self.remote_url),
			_ => None,
		}
	}

	fn ensure_focus_visible(&mut self) {
		let fields = self.visible_fields();
		if !fields.contains(&self.focus) {
			self.focus = fields.first().copied().unwrap_or(ProjectEditFocus::Name);
		}
		let body_fields = self.body_fields();
		self.sync_body_scroll(&body_fields);
	}

	pub(crate) fn refresh_body_window(&mut self, viewport_height: u16) -> (Vec<ProjectEditFocus>, u16, bool, bool) {
		let body_fields = self.body_fields();
		let row_height = dialog_form_row_height(viewport_height);
		self.viewport_rows = dialog_visible_rows(viewport_height, row_height);
		self.sync_body_scroll(&body_fields);
		let start = self.field_scroll.min(body_fields.len().saturating_sub(self.viewport_rows));
		let end = (start + self.viewport_rows).min(body_fields.len());
		(
			body_fields[start..end].to_vec(),
			row_height,
			start > 0,
			end < body_fields.len(),
		)
	}

	pub(crate) fn scroll_body(&mut self, delta: isize) {
		let body_fields = self.body_fields();
		if body_fields.is_empty() {
			self.field_scroll = 0;
			return;
		}

		let max_scroll = body_fields.len().saturating_sub(self.viewport_rows.max(1));
		let next = (self.field_scroll as isize + delta).clamp(0, max_scroll as isize) as usize;
		self.field_scroll = next;
	}

	fn sync_body_scroll(&mut self, body_fields: &[ProjectEditFocus]) {
		let visible_rows = self.viewport_rows.max(1);
		clamp_dialog_scroll(
			&mut self.field_scroll,
			body_fields.len(),
			visible_rows,
			body_fields.iter().position(|field| *field == self.focus),
		);
	}

	fn prefill_repo_root_from_target_path(&mut self) {
		if !self.repo_root.is_empty() {
			return;
		}
		let target_path = if self.project_type == ProjectType::Branched {
			self.current_scope().map(|scope| scope.target_path.value()).unwrap_or("")
		} else {
			self.target_path.value()
		};
		if let Some(repo_root) = derive_repo_root_from_target_path(target_path) {
			self.repo_root.set_value(repo_root);
		}
	}

	pub(crate) fn set_target_path_from_browse(&mut self, path: String) {
		if self.project_type == ProjectType::Branched {
			if let Some(scope) = self.current_scope_mut() {
				scope.target_path.set_value(path);
				if !scope.target_key_custom {
					scope.target_key.set_value(default_target_key_for_path(scope.target_path.value()));
				}
			}
		} else {
			self.target_path.set_value(path);
			if !self.target_key_custom {
				self.target_key.set_value(default_target_key_for_path(self.target_path.value()));
			}
		}
		self.prefill_repo_root_from_target_path();
	}

	fn target_key_accepts_text(&self) -> bool {
		if self.project_type == ProjectType::Branched {
			self.current_scope().is_some_and(|scope| scope.target_key_custom)
		} else {
			self.target_key_custom
		}
	}

	pub(crate) fn enable_custom_target_key(&mut self) {
		if self.project_type == ProjectType::Branched {
			if let Some(scope) = self.current_scope_mut() {
				scope.target_key_custom = true;
			}
		} else {
			self.target_key_custom = true;
		}
		self.focus = ProjectEditFocus::TargetKey;
	}

	fn rotate_target_key_preset(&mut self, delta: i32) {
		if self.project_type == ProjectType::Branched {
			if let Some(scope) = self.current_scope_mut() {
				let next = cycle_target_key_preset(scope.target_path.value(), scope.target_key.value(), delta);
				scope.target_key.set_value(next);
				scope.target_key_custom = false;
			}
		} else {
			let next = cycle_target_key_preset(self.target_path.value(), self.target_key.value(), delta);
			self.target_key.set_value(next);
			self.target_key_custom = false;
		}
	}

	fn sync_target_key_preset_with_path(&mut self) {
		if self.project_type == ProjectType::Branched {
			if let Some(scope) = self.current_scope_mut() {
				if !scope.target_key_custom {
					scope.target_key.set_value(default_target_key_for_path(scope.target_path.value()));
				}
			}
		} else if !self.target_key_custom {
			self.target_key.set_value(default_target_key_for_path(self.target_path.value()));
		}
	}

	pub(crate) fn set_repo_root_from_browse(&mut self, path: String) {
		self.repo_root.set_value(path);
	}

	pub(crate) fn apply(&self, project: &mut ProjectConfig) -> Result<()> {
		let project_name = self.name.value.trim();
		if project_name.is_empty() {
			bail!("project name cannot be empty");
		}

		let existing_target = if project.project_type == ProjectType::AllInOne {
			project.targets.first()
		} else {
			project.branches.first().and_then(|branch| branch.targets.first())
		}
		.ok_or_else(|| anyhow!("selected project does not contain any editable targets yet"))?
		.clone();

		project.name = project_name.to_string();
		project.project_type = self.project_type;
		project.integration_mode = self.integration_mode;
		project.unified_versioning = self.project_type == ProjectType::AllInOne || self.unified_versioning;
		project.version_scheme = self.version_scheme;
		let preserved_all_in_one_changelog_enabled = project.changelog.enabled;
		let preserved_branch_changelog_enabled = project
			.branches
			.iter()
			.map(|branch| branch.changelog_enabled)
			.collect::<Vec<_>>();
		project.changelog = self.build_changelog_settings(preserved_all_in_one_changelog_enabled);

		if self.project_type == ProjectType::AllInOne {
			let target_path = self.target_path.value.trim();
			if target_path.is_empty() {
				bail!("target path cannot be empty");
			}

			let target_key = self.target_key.value.trim();
			if target_key.is_empty() {
				bail!("target key cannot be empty");
			}

			let target = TargetSpec {
				label: existing_target.label,
				path: target_path.to_string(),
				key_path: target_key.to_string(),
				format: existing_target.format,
			};
			project.targets = vec![target];
			project.branches.clear();
		} else {
			project.targets.clear();
			project.branches = self.build_branches(false)?;
			for (branch, enabled) in project.branches.iter_mut().zip(preserved_branch_changelog_enabled.into_iter()) {
				branch.changelog_enabled = enabled;
			}
		}

		if self.project_type == ProjectType::AllInOne {
			project.changelog.enabled = preserved_all_in_one_changelog_enabled;
		} else if let Some(first_branch) = project.branches.first() {
			project.changelog.enabled = first_branch.changelog_enabled;
		}

		if self.integration_mode.requires_repo() {
			let repo_root = self.repo_root.value.trim();
			if repo_root.is_empty() {
				bail!("repo root cannot be empty");
			}
			if !Path::new(repo_root).is_dir() {
				bail!("repo root does not exist: {}", repo_root);
			}

			let remote_url = if self.integration_mode.requires_remote() {
				let remote_url = self.remote_url.value.trim();
				if remote_url.is_empty() {
					bail!("remote URL cannot be empty");
				}
				Some(remote_url.to_string())
			} else {
				None
			};

			project.repo = Some(RepoConfig {
				local_root: repo_root.to_string(),
				remote_url,
			});
		} else {
			project.repo = None;
		}

		Ok(())
	}

	fn build_changelog_settings(&self, enabled: bool) -> ChangelogSettings {
		ChangelogSettings {
			enabled,
			file_path: if self.changelog_path.value.trim().is_empty() {
				DEFAULT_CHANGELOG_PATH.to_string()
			} else {
				self.changelog_path.value.trim().to_string()
			},
		}
	}

	pub(crate) fn current_scope(&self) -> Option<&ScopeDraft> {
		self.scopes.get(self.selected_scope)
	}

	fn current_scope_mut(&mut self) -> Option<&mut ScopeDraft> {
		self.scopes.get_mut(self.selected_scope)
	}

	fn selected_scope_summary(&self) -> String {
		let total = self.scopes.len();
		if total == 0 {
			"< no scopes >".to_string()
		} else {
			let summary = self
				.current_scope()
				.map(|scope| scope.display_name())
				.unwrap_or_else(|| "(unknown)".to_string());
			format!("< {}/{}: {} >", self.selected_scope + 1, total, summary)
		}
	}

	fn move_scope_selection(&mut self, delta: i32) {
		if self.scopes.is_empty() {
			return;
		}
		let len = self.scopes.len() as i32;
		let next = (self.selected_scope as i32 + delta).rem_euclid(len) as usize;
		self.selected_scope = next;
	}

	fn next_scope_name(&self) -> String {
		let mut index = self.scopes.len() + 1;
		loop {
			let candidate = format!("scope-{}", index);
			if self
				.scopes
				.iter()
				.all(|scope| !scope.name.value.trim().eq_ignore_ascii_case(&candidate))
			{
				return candidate;
			}
			index += 1;
		}
	}

	pub(crate) fn add_scope(&mut self) {
		let scope = ScopeDraft::new(self.next_scope_name());
		self.scopes.push(scope);
		self.selected_scope = self.scopes.len().saturating_sub(1);
		self.focus = ProjectEditFocus::ScopeName;
	}

	pub(crate) fn remove_selected_scope(&mut self) -> Result<()> {
		if self.scopes.len() <= 1 {
			bail!("branched projects need at least one scope");
		}
		self.scopes.remove(self.selected_scope);
		self.selected_scope = self.selected_scope.min(self.scopes.len().saturating_sub(1));
		self.focus = ProjectEditFocus::ScopeSelection;
		Ok(())
	}

	pub(crate) fn move_selected_scope(&mut self, delta: isize) {
		if self.scopes.len() < 2 {
			return;
		}
		let len = self.scopes.len() as isize;
		let next = (self.selected_scope as isize + delta).clamp(0, len - 1) as usize;
		if next != self.selected_scope {
			self.scopes.swap(self.selected_scope, next);
			self.selected_scope = next;
		}
	}

	fn seed_scope_from_primary_target(&mut self) {
		if self.scopes.is_empty() {
			self.scopes.push(ScopeDraft::new("core"));
			self.selected_scope = 0;
		}
		let target_path = self.target_path.value.trim().to_string();
		let target_key = self.target_key.value.trim().to_string();
		let target_key_custom = self.target_key_custom;
		if let Some(scope) = self.current_scope_mut() {
			if scope.target_path.value.trim().is_empty() && !target_path.is_empty() {
				scope.target_path.set_value(target_path);
			}
			if scope.target_key.value.trim().is_empty() && !target_key.is_empty() {
				scope.target_key.set_value(target_key);
				scope.target_key_custom = target_key_custom;
			}
		}
	}

	fn copy_selected_scope_to_primary_target(&mut self) {
		let selected = self.current_scope().map(|scope| {
			(
				scope.target_path.value().to_string(),
				scope.target_key.value().to_string(),
				scope.target_key_custom,
			)
		});
		if let Some((target_path, target_key, target_key_custom)) = selected {
			self.target_path.set_value(target_path);
			self.target_key.set_value(target_key);
			self.target_key_custom = target_key_custom;
		}
	}

	fn build_branches(&self, require_probe: bool) -> Result<Vec<BranchConfig>> {
		if self.scopes.is_empty() {
			bail!("branched projects need at least one scope");
		}

		let mut names = HashSet::new();
		let mut branches = Vec::with_capacity(self.scopes.len());
		for scope in &self.scopes {
			let branch = scope.build_branch(self.version_scheme, require_probe)?;
			let key = branch.name.trim().to_ascii_lowercase();
			if !names.insert(key) {
				bail!("scope names must be unique");
			}
			branches.push(branch);
		}
		Ok(branches)
	}
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectEditFocus {
	Name,
	ProjectType,
	ScopeSelection,
	ScopeName,
	ScopeKind,
	VersionScheme,
	UnifiedVersioning,
	IntegrationMode,
	TargetPath,
	TargetKey,
	AddScope,
	RemoveScope,
	MoveScopeUp,
	MoveScopeDown,
	RepoRoot,
	RemoteUrl,
	Save,
	Remove,
	Cancel,
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::config::{TargetFormat, TargetSpec};

	#[test]
	fn apply_updates_changelog_path_without_overwriting_enabled_state() {
		let mut project = ProjectConfig {
			name: "Example".to_string(),
			project_type: ProjectType::AllInOne,
			integration_mode: IntegrationMode::LocalOnly,
			unified_versioning: true,
			version_scheme: VersionScheme::SemVer,
			changelog: ChangelogSettings::default(),
			release_now: crate::config::ReleaseNowSettings::default(),
			targets: vec![TargetSpec {
				label: "Version".to_string(),
				path: "Cargo.toml".to_string(),
				key_path: "package.version".to_string(),
				format: TargetFormat::Toml,
			}],
			branches: Vec::new(),
			repo: None,
		};

		let mut dialog = ProjectEditDialog::from_project(0, &project).expect("dialog should build");
		dialog.changelog_path.set_value("notes/CHANGELOG.md".to_string());

		dialog.apply(&mut project).expect("apply should succeed");

		assert!(!project.changelog.enabled);
		assert_eq!(project.changelog.file_path, "notes/CHANGELOG.md");
	}

	#[test]
	fn visible_fields_hide_changelog_path_in_project_edit() {
		let dialog = ProjectEditDialog::from_project(
			0,
			&ProjectConfig {
				name: "demo".to_string(),
				project_type: ProjectType::AllInOne,
				version_scheme: VersionScheme::SemVer,
				unified_versioning: true,
				integration_mode: IntegrationMode::GitHubEnabled,
				release_now: crate::config::ReleaseNowSettings::default(),
				targets: vec![TargetSpec {
					label: "Version".to_string(),
					path: "C:/repo/Cargo.toml".to_string(),
					key_path: "package.version".to_string(),
					format: crate::config::TargetFormat::Toml,
				}],
				branches: vec![],
				repo: Some(RepoConfig {
					local_root: "C:/repo".to_string(),
					remote_url: Some("https://example.test/repo.git".to_string()),
				}),
				changelog: ChangelogSettings {
					enabled: true,
					file_path: "CHANGELOG.md".to_string(),
				},
			},
		)
		.expect("dialog should build");

		let fields = dialog.visible_fields();
		let labels: Vec<_> = fields.iter().map(|field| dialog.render_field(*field).0).collect();
		assert!(!labels.contains(&"Changelog path"));
		assert!(fields.contains(&ProjectEditFocus::RepoRoot));
	}

	#[test]
	fn branched_projects_hide_changelog_path_field() {
		let dialog = ProjectEditDialog::from_project(
			0,
			&ProjectConfig {
				name: "demo".to_string(),
				project_type: ProjectType::Branched,
				version_scheme: VersionScheme::SemVer,
				unified_versioning: false,
				integration_mode: IntegrationMode::GitHubEnabled,
				release_now: crate::config::ReleaseNowSettings::default(),
				targets: vec![],
				branches: vec![BranchConfig {
					name: "core".to_string(),
					label: "Core".to_string(),
					scope_kind: BranchScopeKind::Branch,
					repo: None,
					changelog_enabled: true,
					changelog_path: Some("CHANGELOG.md".to_string()),
					release_now: crate::config::ReleaseNowSettings::default(),
					version_scheme: VersionScheme::SemVer,
					targets: vec![TargetSpec {
						label: "Version".to_string(),
						path: "C:/repo/Cargo.toml".to_string(),
						key_path: "package.version".to_string(),
						format: crate::config::TargetFormat::Toml,
					}],
				}],
				repo: Some(RepoConfig {
					local_root: "C:/repo".to_string(),
					remote_url: Some("https://example.test/repo.git".to_string()),
				}),
				changelog: ChangelogSettings {
					enabled: true,
					file_path: "CHANGELOG.md".to_string(),
				},
			},
		)
		.expect("dialog should build");

		let labels: Vec<_> = dialog
			.visible_fields()
			.iter()
			.map(|field| dialog.render_field(*field).0)
			.collect();
		assert!(!labels.contains(&"Changelog path"));
	}
}
