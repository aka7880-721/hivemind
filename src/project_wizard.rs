// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit License v1.2
//
// For details, see the LICENSE file in the repository root.

use std::{collections::HashSet, path::Path};

use anyhow::{Result, bail};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::text::Line;

use crate::{
    app::{
        HitAction, ScopeAction, ScopeDraft, clamp_dialog_scroll, cycle_target_key_preset,
        default_target_key_for_path, derive_repo_root_from_target_path, dialog_form_row_height,
        dialog_visible_rows, rotate_scope_kind,
    },
    config::{
        BranchConfig, BranchScopeKind, ChangelogSettings, DEFAULT_CHANGELOG_PATH, IntegrationMode,
        ProjectConfig, ProjectType, ReleaseNowSettings, RepoConfig, TargetFormat, TargetSpec,
        TileInfoSettings, TileRotationTarget,
    },
    dialogs::TextInput,
    targets::{ProbeKind, TargetProbe},
    versioning::VersionScheme,
};

#[derive(Clone)]
pub(crate) struct ProjectWizard {
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
    pub(crate) tile_auto_rotation: bool,
    pub(crate) tile_rotates: TileRotationTarget,
    pub(crate) tile_rotation_timing: TextInput,
    pub(crate) project_type: ProjectType,
    pub(crate) unified_versioning: bool,
    pub(crate) integration_mode: IntegrationMode,
    pub(crate) version_scheme: VersionScheme,
    pub(crate) focus: WizardField,
    pub(crate) last_probe: Option<TargetProbe>,
}

impl Default for ProjectWizard {
    fn default() -> Self {
        Self {
            name: TextInput::with_value(""),
            target_path: TextInput::with_value(""),
            target_key: TextInput::with_value("version"),
            target_key_custom: false,
            scopes: vec![ScopeDraft::new("core")],
            selected_scope: 0,
            field_scroll: 0,
            viewport_rows: 1,
            repo_root: TextInput::with_value(""),
            remote_url: TextInput::with_value(""),
            changelog_path: TextInput::with_value(DEFAULT_CHANGELOG_PATH),
            tile_auto_rotation: true,
            tile_rotates: TileRotationTarget::Both,
            tile_rotation_timing: TextInput::with_value("5"),
            project_type: ProjectType::AllInOne,
            unified_versioning: false,
            integration_mode: IntegrationMode::LocalOnly,
            version_scheme: VersionScheme::SemVer,
            focus: WizardField::ProjectType,
            last_probe: None,
        }
    }
}

impl ProjectWizard {
    pub(crate) fn focus_accepts_text(&self) -> bool {
        matches!(
            self.focus,
            WizardField::Name
                | WizardField::ScopeName
                | WizardField::TargetPath
                | WizardField::RepoRoot
                | WizardField::RemoteUrl
                | WizardField::TileRotationTiming
        ) || (self.focus == WizardField::TargetKey && self.target_key_accepts_text())
    }

    fn visible_fields(&self) -> Vec<WizardField> {
        let mut fields = vec![WizardField::ProjectType, WizardField::Name];
        if self.project_type == ProjectType::Branched {
            fields.extend([
                WizardField::ScopeSelection,
                WizardField::ScopeName,
                WizardField::ScopeKind,
                WizardField::UnifiedVersioning,
            ]);
        }
        fields.extend([
            WizardField::VersionScheme,
            WizardField::IntegrationMode,
            WizardField::TargetPath,
            WizardField::TargetKey,
        ]);
        if self.project_type == ProjectType::Branched {
            fields.extend([
                WizardField::AddScope,
                WizardField::RemoveScope,
                WizardField::MoveScopeUp,
                WizardField::MoveScopeDown,
            ]);
        }
        let integration_mode = self.selected_integration_mode();
        if integration_mode.requires_repo() {
            fields.push(WizardField::RepoRoot);
        }
        if integration_mode.requires_remote() {
            fields.push(WizardField::RemoteUrl);
        }
        fields.push(WizardField::TileAutoRotation);
        if self.tile_auto_rotation {
            fields.push(WizardField::TileRotates);
            fields.push(WizardField::TileRotationTiming);
        }
        fields.extend([
            WizardField::Validate,
            WizardField::Save,
            WizardField::Cancel,
        ]);
        fields
    }

    fn body_fields(&self) -> Vec<WizardField> {
        self.visible_fields()
            .into_iter()
            .filter(|field| {
                !matches!(
                    field,
                    WizardField::Validate | WizardField::Save | WizardField::Cancel
                )
            })
            .collect()
    }

    pub(crate) fn focus_next(&mut self) {
        let fields = self.visible_fields();
        let index = fields
            .iter()
            .position(|field| *field == self.focus)
            .unwrap_or(0);
        self.focus = fields[(index + 1) % fields.len()];
        self.ensure_focus_visible();
    }

    pub(crate) fn focus_previous(&mut self) {
        let fields = self.visible_fields();
        let index = fields
            .iter()
            .position(|field| *field == self.focus)
            .unwrap_or(0);
        self.focus = fields[(index + fields.len() - 1) % fields.len()];
        self.ensure_focus_visible();
    }

    pub(crate) fn render_field(&self, field: WizardField) -> (&'static str, HitAction) {
        match field {
            WizardField::Name => ("Project name", HitAction::WizardField(field)),
            WizardField::ProjectType => ("Project type", HitAction::WizardField(field)),
            WizardField::ScopeSelection => ("Scope", HitAction::WizardField(field)),
            WizardField::ScopeName => ("Scope name", HitAction::WizardField(field)),
            WizardField::ScopeKind => ("Scope kind", HitAction::WizardField(field)),
            WizardField::VersionScheme => ("Version scheme", HitAction::WizardField(field)),
            WizardField::UnifiedVersioning => ("Unified versioning", HitAction::WizardField(field)),
            WizardField::IntegrationMode => ("Integration", HitAction::WizardField(field)),
            WizardField::TargetPath => ("Target path", HitAction::WizardField(field)),
            WizardField::TargetKey => ("Target key", HitAction::WizardField(field)),
            WizardField::AddScope => ("Add scope", HitAction::WizardScopeAction(ScopeAction::Add)),
            WizardField::RemoveScope => (
                "Remove scope",
                HitAction::WizardScopeAction(ScopeAction::Remove),
            ),
            WizardField::MoveScopeUp => (
                "Move scope up",
                HitAction::WizardScopeAction(ScopeAction::MoveUp),
            ),
            WizardField::MoveScopeDown => (
                "Move scope down",
                HitAction::WizardScopeAction(ScopeAction::MoveDown),
            ),
            WizardField::RepoRoot => ("Repo root", HitAction::WizardField(field)),
            WizardField::RemoteUrl => ("Remote URL", HitAction::WizardField(field)),
            WizardField::TileAutoRotation => ("Auto-rotation", HitAction::WizardField(field)),
            WizardField::TileRotates => ("Rotates", HitAction::WizardField(field)),
            WizardField::TileRotationTiming => ("Rotation (s)", HitAction::WizardField(field)),
            WizardField::Validate => ("Read", HitAction::ValidateWizard),
            WizardField::Save => ("Save", HitAction::SaveWizard),
            WizardField::Cancel => ("Cancel", HitAction::CancelWizard),
        }
    }

    pub(crate) fn display_value_for_field(
        &self,
        field: WizardField,
        focused: bool,
        max_width: usize,
    ) -> Line<'static> {
        match field {
            WizardField::Name => self.name.display_line_with_width(focused, max_width),
            WizardField::ProjectType => {
                Line::from(format!("< {} >", self.project_type.display_name()))
            }
            WizardField::ScopeSelection => Line::from(self.selected_scope_summary()),
            WizardField::ScopeName => self
                .current_scope()
                .map(|scope| scope.name.display_line_with_width(focused, max_width))
                .unwrap_or_else(|| Line::from("(no scope)")),
            WizardField::ScopeKind => self
                .current_scope()
                .map(|scope| Line::from(format!("< {} >", scope.scope_kind.display_name())))
                .unwrap_or_else(|| {
                    Line::from(format!("< {} >", BranchScopeKind::Branch.display_name()))
                }),
            WizardField::VersionScheme => Line::from(format!(
                "< {} >",
                self.selected_version_scheme().display_name()
            )),
            WizardField::UnifiedVersioning => {
                if self.project_type == ProjectType::Branched {
                    Line::from(format!(
                        "< {} >",
                        if self.unified_versioning { "Yes" } else { "No" }
                    ))
                } else {
                    Line::from("Always yes for all-in-one projects")
                }
            }
            WizardField::IntegrationMode => Line::from(format!(
                "< {} >",
                self.selected_integration_mode().display_name()
            )),
            WizardField::TargetPath => {
                if self.project_type == ProjectType::Branched {
                    self.current_scope()
                        .map(|scope| {
                            scope
                                .target_path
                                .display_line_with_width(focused, max_width)
                        })
                        .unwrap_or_else(|| Line::from(String::new()))
                } else {
                    self.target_path.display_line_with_width(focused, max_width)
                }
            }
            WizardField::TargetKey => {
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
            WizardField::AddScope => Line::from("Create a new scope draft"),
            WizardField::RemoveScope => Line::from("Drop the selected scope"),
            WizardField::MoveScopeUp => Line::from("Move the selected scope earlier"),
            WizardField::MoveScopeDown => Line::from("Move the selected scope later"),
            WizardField::RepoRoot => self.repo_root.display_line_with_width(focused, max_width),
            WizardField::RemoteUrl => self.remote_url.display_line_with_width(focused, max_width),
            WizardField::TileAutoRotation => Line::from(format!(
                "< {} >",
                if self.tile_auto_rotation { "Yes" } else { "No" }
            )),
            WizardField::TileRotates => {
                Line::from(format!("< {} >", self.tile_rotates.display_name()))
            }
            WizardField::TileRotationTiming => self
                .tile_rotation_timing
                .display_line_with_width(focused, max_width),
            WizardField::Validate => Line::from("Validate target"),
            WizardField::Save => Line::from("Persist project"),
            WizardField::Cancel => Line::from("Discard changes"),
        }
    }

    pub(crate) fn adjust_current_enum(&mut self, delta: i32) {
        match self.focus {
            WizardField::ProjectType => {
                self.project_type = if delta >= 0 {
                    self.project_type.next()
                } else {
                    self.project_type.previous()
                };
                if self.project_type == ProjectType::Branched {
                    self.seed_scope_from_primary_target();
                } else {
                    self.copy_selected_scope_to_primary_target();
                }
            }
            WizardField::ScopeSelection => self.move_scope_selection(delta),
            WizardField::ScopeKind => {
                if let Some(scope) = self.current_scope_mut() {
                    scope.scope_kind = rotate_scope_kind(scope.scope_kind, delta);
                }
            }
            WizardField::TargetKey => self.rotate_target_key_preset(delta),
            WizardField::VersionScheme => {
                if self.project_type == ProjectType::Branched {
                    if let Some(scope) = self.current_scope_mut() {
                        scope.version_scheme = if delta >= 0 {
                            scope.version_scheme.next()
                        } else {
                            scope.version_scheme.previous()
                        };
                    }
                } else {
                    self.version_scheme = if delta >= 0 {
                        self.version_scheme.next()
                    } else {
                        self.version_scheme.previous()
                    };
                }
                self.clear_validation_results();
            }
            WizardField::UnifiedVersioning if self.project_type == ProjectType::Branched => {
                self.unified_versioning = !self.unified_versioning;
            }
            WizardField::IntegrationMode => {
                let next_mode = if delta >= 0 {
                    self.selected_integration_mode().next()
                } else {
                    self.selected_integration_mode().previous()
                };
                self.set_selected_integration_mode(next_mode);
            }
            WizardField::TileAutoRotation => {
                self.tile_auto_rotation = !self.tile_auto_rotation;
            }
            WizardField::TileRotates => {
                self.tile_rotates = if delta >= 0 {
                    self.tile_rotates.next()
                } else {
                    self.tile_rotates.previous()
                };
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
            KeyCode::Char(_)
            | KeyCode::Backspace
            | KeyCode::Delete
            | KeyCode::Left
            | KeyCode::Right
            | KeyCode::Home
            | KeyCode::End => {
                input.handle_key(key);
                if matches!(
                    key.code,
                    KeyCode::Char(_) | KeyCode::Backspace | KeyCode::Delete
                ) {
                    self.after_text_edit();
                }
            }
            _ => {}
        }
    }

    pub(crate) fn active_input_mut(&mut self) -> Option<&mut TextInput> {
        match self.focus {
            WizardField::Name => Some(&mut self.name),
            WizardField::ScopeName => self.current_scope_mut().map(|scope| &mut scope.name),
            WizardField::TargetPath => {
                if self.project_type == ProjectType::Branched {
                    self.current_scope_mut().map(|scope| &mut scope.target_path)
                } else {
                    Some(&mut self.target_path)
                }
            }
            WizardField::TargetKey => {
                if self.project_type == ProjectType::Branched
                    && self
                        .current_scope()
                        .is_some_and(|scope| scope.target_key_custom)
                {
                    self.current_scope_mut().map(|scope| &mut scope.target_key)
                } else if self.project_type != ProjectType::Branched && self.target_key_custom {
                    Some(&mut self.target_key)
                } else {
                    None
                }
            }
            WizardField::RepoRoot => Some(&mut self.repo_root),
            WizardField::RemoteUrl => Some(&mut self.remote_url),
            WizardField::TileRotationTiming => Some(&mut self.tile_rotation_timing),
            _ => None,
        }
    }

    pub(crate) fn insert_text(&mut self, text: &str) -> bool {
        if let Some(input) = self.active_input_mut() {
            input.insert_str(text);
            self.after_text_edit();
            return true;
        }
        false
    }

    fn after_text_edit(&mut self) {
        if matches!(self.focus, WizardField::TargetPath | WizardField::TargetKey) {
            if self.project_type == ProjectType::Branched {
                if let Some(scope) = self.current_scope_mut() {
                    scope.last_probe = None;
                }
            } else {
                self.last_probe = None;
            }
        }
        if self.focus == WizardField::ScopeName
            && let Some(scope) = self.current_scope_mut()
        {
            scope.sync_label_if_needed();
        }
        if self.focus == WizardField::TargetPath {
            self.sync_target_key_preset_with_path();
            self.prefill_repo_root_from_target_path();
        }
        if matches!(self.focus, WizardField::RepoRoot | WizardField::RemoteUrl) {
            self.persist_scope_repo_inputs();
        }
    }

    fn ensure_focus_visible(&mut self) {
        let fields = self.visible_fields();
        if !fields.contains(&self.focus) {
            self.focus = fields.first().copied().unwrap_or(WizardField::Name);
        }
        let body_fields = self.body_fields();
        self.sync_body_scroll(&body_fields);
    }

    pub(crate) fn refresh_body_window(
        &mut self,
        viewport_height: u16,
    ) -> (Vec<WizardField>, u16, bool, bool) {
        let body_fields = self.body_fields();
        let row_height = dialog_form_row_height(viewport_height);
        self.viewport_rows = dialog_visible_rows(viewport_height, row_height);
        self.sync_body_scroll(&body_fields);
        let start = self
            .field_scroll
            .min(body_fields.len().saturating_sub(self.viewport_rows));
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

    fn sync_body_scroll(&mut self, body_fields: &[WizardField]) {
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
            self.current_scope()
                .map(|scope| scope.target_path.value())
                .unwrap_or("")
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
                    scope
                        .target_key
                        .set_value(default_target_key_for_path(scope.target_path.value()));
                }
                scope.last_probe = None;
            }
        } else {
            self.target_path.set_value(path);
            if !self.target_key_custom {
                self.target_key
                    .set_value(default_target_key_for_path(self.target_path.value()));
            }
            self.last_probe = None;
        }
        self.prefill_repo_root_from_target_path();
    }

    fn target_key_accepts_text(&self) -> bool {
        if self.project_type == ProjectType::Branched {
            self.current_scope()
                .is_some_and(|scope| scope.target_key_custom)
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
        self.focus = WizardField::TargetKey;
    }

    fn rotate_target_key_preset(&mut self, delta: i32) {
        if self.project_type == ProjectType::Branched {
            if let Some(scope) = self.current_scope_mut() {
                let next = cycle_target_key_preset(
                    scope.target_path.value(),
                    scope.target_key.value(),
                    delta,
                );
                scope.target_key.set_value(next);
                scope.target_key_custom = false;
            }
        } else {
            let next =
                cycle_target_key_preset(self.target_path.value(), self.target_key.value(), delta);
            self.target_key.set_value(next);
            self.target_key_custom = false;
        }
    }

    fn sync_target_key_preset_with_path(&mut self) {
        if self.project_type == ProjectType::Branched {
            if let Some(scope) = self.current_scope_mut()
                && !scope.target_key_custom
            {
                scope
                    .target_key
                    .set_value(default_target_key_for_path(scope.target_path.value()));
            }
        } else if !self.target_key_custom {
            self.target_key
                .set_value(default_target_key_for_path(self.target_path.value()));
        }
    }

    pub(crate) fn set_repo_root_from_browse(&mut self, path: String) {
        self.repo_root.set_value(path);
    }

    pub(crate) fn build_project(&self) -> Result<ProjectConfig> {
        if self.name.value.trim().is_empty() {
            bail!("project name is required");
        }

        let effective_integration_mode = if self.project_type == ProjectType::Branched {
            self.scopes
                .iter()
                .map(|scope| scope.integration_mode)
                .max_by_key(|mode| match mode {
                    IntegrationMode::LocalOnly => 0,
                    IntegrationMode::GitLocalOnly => 1,
                    IntegrationMode::GitHubEnabled => 2,
                })
                .unwrap_or(IntegrationMode::LocalOnly)
        } else {
            self.integration_mode
        };

        let repo = if self.project_type == ProjectType::Branched {
            None
        } else if effective_integration_mode.requires_repo() {
            let root = self.repo_root.value.trim();
            if root.is_empty() {
                bail!("repo root is required for git-backed projects");
            }
            if !Path::new(root).is_dir() {
                bail!("repo root does not exist: {}", root);
            }
            let remote = if effective_integration_mode.requires_remote() {
                let value = self.remote_url.value.trim();
                if value.is_empty() {
                    bail!("remote URL is required for GitHub-enabled projects");
                }
                Some(value.to_string())
            } else {
                None
            };
            Some(RepoConfig {
                local_root: root.to_string(),
                remote_url: remote,
                ..RepoConfig::default()
            })
        } else {
            None
        };

        let project = if self.project_type == ProjectType::AllInOne {
            if self.target_path.value.trim().is_empty() {
                bail!("target path is required");
            }
            if self.target_key.value.trim().is_empty() {
                bail!("target key is required");
            }
            match &self.last_probe {
                Some(probe) if matches!(probe.kind, ProbeKind::Success) => {}
                Some(_) => bail!("read the target successfully before saving"),
                None => bail!("validate the target before saving"),
            }

            let target = TargetSpec {
                label: "Version".to_string(),
                path: self.target_path.value.trim().to_string(),
                key_path: self.target_key.value.trim().to_string(),
                format: self
                    .last_probe
                    .as_ref()
                    .and_then(|probe| probe.format)
                    .unwrap_or(TargetFormat::Auto),
            };
            ProjectConfig {
                name: self.name.value.trim().to_string(),
                alias: String::new(),
                project_type: ProjectType::AllInOne,
                integration_mode: effective_integration_mode,
                unified_versioning: true,
                version_scheme: self.version_scheme,
                changelog: self.build_changelog_settings(false),
                release_now: ReleaseNowSettings::default(),
                tile_info: self.build_tile_info_settings()?,
                targets: vec![target],
                branches: Vec::new(),
                repo,
            }
        } else {
            ProjectConfig {
                name: self.name.value.trim().to_string(),
                alias: String::new(),
                project_type: ProjectType::Branched,
                integration_mode: effective_integration_mode,
                unified_versioning: self.unified_versioning,
                version_scheme: self.selected_version_scheme(),
                changelog: self.build_changelog_settings(false),
                release_now: ReleaseNowSettings::default(),
                tile_info: self.build_tile_info_settings()?,
                targets: Vec::new(),
                branches: self.build_branches(true)?,
                repo,
            }
        };

        Ok(project)
    }

    fn build_changelog_settings(&self, enabled: bool) -> ChangelogSettings {
        ChangelogSettings {
            enabled,
            file_path: if self.changelog_path.value.trim().is_empty() {
                DEFAULT_CHANGELOG_PATH.to_string()
            } else {
                self.changelog_path.value.trim().to_string()
            },
            hide_pr_messages: false,
            hide_bump_messages: false,
            mini_commit_hashes: false,
        }
    }

    fn build_tile_info_settings(&self) -> Result<TileInfoSettings> {
        let rotation_timing_seconds = parse_tile_rotation_seconds(
            self.tile_rotation_timing.value.trim(),
            self.tile_auto_rotation,
        )?;
        Ok(TileInfoSettings {
            auto_rotation: self.tile_auto_rotation,
            rotates: self.tile_rotates,
            remembered_dev_mode: 0,
            remembered_rls_mode: 0,
            rotation_timing_seconds,
        })
    }

    pub(crate) fn current_scope(&self) -> Option<&ScopeDraft> {
        self.scopes.get(self.selected_scope)
    }

    pub(crate) fn current_scope_mut(&mut self) -> Option<&mut ScopeDraft> {
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
        self.persist_scope_repo_inputs();
        let len = self.scopes.len() as i32;
        let next = (self.selected_scope as i32 + delta).rem_euclid(len) as usize;
        self.selected_scope = next;
        self.sync_repo_inputs_from_scope();
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
        self.persist_scope_repo_inputs();
        let mut scope = ScopeDraft::new(self.next_scope_name());
        scope.version_scheme = self.selected_version_scheme();
        scope.integration_mode = self.selected_integration_mode();
        self.scopes.push(scope);
        self.selected_scope = self.scopes.len().saturating_sub(1);
        self.sync_repo_inputs_from_scope();
        self.focus = WizardField::ScopeName;
    }

    pub(crate) fn remove_selected_scope(&mut self) -> Result<()> {
        if self.scopes.len() <= 1 {
            bail!("branched projects need at least one scope");
        }
        self.persist_scope_repo_inputs();
        self.scopes.remove(self.selected_scope);
        self.selected_scope = self.selected_scope.min(self.scopes.len().saturating_sub(1));
        self.sync_repo_inputs_from_scope();
        self.focus = WizardField::ScopeSelection;
        Ok(())
    }

    pub(crate) fn move_selected_scope(&mut self, delta: isize) {
        if self.scopes.len() < 2 {
            return;
        }
        self.persist_scope_repo_inputs();
        let len = self.scopes.len() as isize;
        let next = (self.selected_scope as isize + delta).clamp(0, len - 1) as usize;
        if next != self.selected_scope {
            self.scopes.swap(self.selected_scope, next);
            self.selected_scope = next;
        }
        self.sync_repo_inputs_from_scope();
    }

    fn clear_validation_results(&mut self) {
        self.last_probe = None;
        for scope in &mut self.scopes {
            scope.last_probe = None;
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
        let target_format = self
            .last_probe
            .as_ref()
            .and_then(|probe| probe.format)
            .unwrap_or(TargetFormat::Auto);
        let default_version_scheme = self.version_scheme;
        let default_integration_mode = self.integration_mode;
        if let Some(scope) = self.current_scope_mut() {
            if scope.target_path.value.trim().is_empty() && !target_path.is_empty() {
                scope.target_path.set_value(target_path);
                scope.format = target_format;
            }
            if scope.target_key.value.trim().is_empty() && !target_key.is_empty() {
                scope.target_key.set_value(target_key);
                scope.target_key_custom = target_key_custom;
            }
            scope.version_scheme = default_version_scheme;
            scope.integration_mode = default_integration_mode;
        }
        self.sync_repo_inputs_from_scope();
    }

    fn copy_selected_scope_to_primary_target(&mut self) {
        let selected = self.current_scope().map(|scope| {
            (
                scope.target_path.value().to_string(),
                scope.target_key.value().to_string(),
                scope.target_key_custom,
                scope.last_probe.clone(),
            )
        });
        if let Some((target_path, target_key, target_key_custom, probe)) = selected {
            self.target_path.set_value(target_path);
            self.target_key.set_value(target_key);
            self.target_key_custom = target_key_custom;
            self.last_probe = probe;
        }
    }

    fn build_branches(&self, require_probe: bool) -> Result<Vec<BranchConfig>> {
        if self.scopes.is_empty() {
            bail!("branched projects need at least one scope");
        }

        let mut names = HashSet::new();
        let mut branches = Vec::with_capacity(self.scopes.len());
        for scope in &self.scopes {
            let branch = scope.build_branch(require_probe)?;
            let key = branch.name.trim().to_ascii_lowercase();
            if !names.insert(key) {
                bail!("scope names must be unique");
            }
            branches.push(branch);
        }
        Ok(branches)
    }

    fn selected_version_scheme(&self) -> VersionScheme {
        if self.project_type == ProjectType::Branched {
            self.current_scope()
                .map(|scope| scope.version_scheme)
                .unwrap_or(self.version_scheme)
        } else {
            self.version_scheme
        }
    }

    fn selected_integration_mode(&self) -> IntegrationMode {
        if self.project_type == ProjectType::Branched {
            self.current_scope()
                .map(|scope| scope.integration_mode)
                .unwrap_or(self.integration_mode)
        } else {
            self.integration_mode
        }
    }

    fn set_selected_integration_mode(&mut self, mode: IntegrationMode) {
        if self.project_type == ProjectType::Branched {
            if let Some(scope) = self.current_scope_mut() {
                scope.integration_mode = mode;
            }
        } else {
            self.integration_mode = mode;
        }
        self.persist_scope_repo_inputs();
        self.sync_repo_inputs_from_scope();
    }

    fn persist_scope_repo_inputs(&mut self) {
        if self.project_type != ProjectType::Branched {
            return;
        }
        let root = self.repo_root.value.trim().to_string();
        let remote = self.remote_url.value.trim().to_string();
        if let Some(scope) = self.current_scope_mut() {
            scope.repo = match scope.integration_mode {
                IntegrationMode::LocalOnly => None,
                IntegrationMode::GitLocalOnly => Some(RepoConfig {
                    local_root: root,
                    remote_url: None,
                    ..RepoConfig::default()
                }),
                IntegrationMode::GitHubEnabled => Some(RepoConfig {
                    local_root: root,
                    remote_url: if remote.is_empty() {
                        None
                    } else {
                        Some(remote)
                    },
                    ..RepoConfig::default()
                }),
            };
        }
    }

    fn sync_repo_inputs_from_scope(&mut self) {
        if self.project_type != ProjectType::Branched {
            return;
        }
        if let Some(scope) = self.current_scope() {
            let repo_root = scope
                .repo
                .as_ref()
                .map(|r| r.local_root.clone())
                .unwrap_or_default();
            let remote_url = scope
                .repo
                .as_ref()
                .and_then(|r| r.remote_url.clone())
                .unwrap_or_default();
            self.repo_root.set_value(repo_root);
            self.remote_url.set_value(remote_url);
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum WizardField {
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
    TileAutoRotation,
    TileRotates,
    TileRotationTiming,
    Validate,
    Save,
    Cancel,
}

fn parse_tile_rotation_seconds(value: &str, enabled: bool) -> Result<u64> {
    if !enabled {
        return Ok(5);
    }

    let trimmed = value.trim();
    if trimmed.is_empty() {
        bail!("tile rotation timing is required when auto-rotation is enabled");
    }

    let parsed = trimmed
        .parse::<u64>()
        .map_err(|_| anyhow::anyhow!("tile rotation timing must be a positive whole number"))?;
    if parsed == 0 {
        bail!("tile rotation timing must be at least 1 second");
    }

    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_project_keeps_changelog_disabled_until_scope_settings_change_it() {
        let mut wizard = ProjectWizard::default();
        wizard.name.set_value("Example".to_string());
        wizard.target_path.set_value("Cargo.toml".to_string());
        wizard.target_key.set_value("package.version".to_string());
        wizard.last_probe = Some(TargetProbe {
            kind: ProbeKind::Success,
            message: "ok".to_string(),
            version: Some("0.1.0".to_string()),
            format: Some(TargetFormat::Toml),
        });
        wizard
            .changelog_path
            .set_value("docs/CHANGELOG.md".to_string());

        let project = wizard.build_project().expect("project should build");

        assert!(!project.changelog.enabled);
        assert_eq!(project.changelog.file_path, "docs/CHANGELOG.md");
    }

    #[test]
    fn visible_fields_show_rotates_only_when_auto_rotation_enabled() {
        let mut wizard = ProjectWizard::default();
        assert!(wizard.visible_fields().contains(&WizardField::TileRotates));

        wizard.tile_auto_rotation = false;

        let fields = wizard.visible_fields();
        assert!(!fields.contains(&WizardField::TileRotates));
        assert!(!fields.contains(&WizardField::TileRotationTiming));
    }

    #[test]
    fn build_project_persists_tile_rotation_target() {
        let mut wizard = ProjectWizard::default();
        wizard.name.set_value("Example".to_string());
        wizard.target_path.set_value("Cargo.toml".to_string());
        wizard.target_key.set_value("package.version".to_string());
        wizard.tile_rotates = TileRotationTarget::RlsLineOnly;
        wizard.last_probe = Some(TargetProbe {
            kind: ProbeKind::Success,
            message: "ok".to_string(),
            version: Some("0.1.0".to_string()),
            format: Some(TargetFormat::Toml),
        });

        let project = wizard.build_project().expect("project should build");

        assert_eq!(project.tile_info.rotates, TileRotationTarget::RlsLineOnly);
    }
}
