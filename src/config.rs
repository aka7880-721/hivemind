// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit License v1.2
//
// For details, see the LICENSE file in the repository root.

use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow, bail};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::versioning::VersionScheme;

pub const SCHEMA_VERSION: u32 = 4;
pub const DEFAULT_CHANGELOG_PATH: &str = "CHANGELOG.md";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub schema_version: u32,
    pub projects: Vec<ProjectConfig>,
    pub ui: UiSettings,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            projects: Vec::new(),
            ui: UiSettings::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UiSettings {
    pub accent_color: String,
    pub show_mouse_hints: bool,
    pub show_tab_hints: bool,
    pub hide_footer: bool,
    pub footer_content: FooterContent,
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            accent_color: "cyan".to_string(),
            show_mouse_hints: true,
            show_tab_hints: true,
            hide_footer: false,
            footer_content: FooterContent::Centered,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FooterContent {
    #[default]
    Centered,
    Left,
}

impl FooterContent {
    pub fn display_name(self) -> &'static str {
        match self {
            FooterContent::Centered => "Centered",
            FooterContent::Left => "Left",
        }
    }

    pub fn next(self) -> Self {
        match self {
            FooterContent::Centered => FooterContent::Left,
            FooterContent::Left => FooterContent::Centered,
        }
    }

    pub fn previous(self) -> Self {
        self.next()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub name: String,
    #[serde(default)]
    pub alias: String,
    pub project_type: ProjectType,
    pub integration_mode: IntegrationMode,
    pub unified_versioning: bool,
    pub version_scheme: VersionScheme,
    #[serde(default)]
    pub changelog: ChangelogSettings,
    #[serde(default)]
    pub release_now: ReleaseNowSettings,
    #[serde(default)]
    pub tile_info: TileInfoSettings,
    #[serde(default)]
    pub targets: Vec<TargetSpec>,
    #[serde(default)]
    pub branches: Vec<BranchConfig>,
    #[serde(default)]
    pub repo: Option<RepoConfig>,
}

impl ProjectConfig {
    pub fn summary(&self) -> String {
        match self.project_type {
            ProjectType::AllInOne => format!(
                "{} | {}",
                self.project_type.display_name(),
                self.version_scheme.display_name()
            ),
            ProjectType::Branched => format!(
                "{} | {} scope{} | {}",
                self.project_type.display_name(),
                self.branches.len(),
                if self.branches.len() == 1 { "" } else { "s" },
                self.branched_scheme_summary()
            ),
        }
    }

    fn branched_scheme_summary(&self) -> &'static str {
        let semver_count = self
            .branches
            .iter()
            .filter(|branch| branch.version_scheme == VersionScheme::SemVer)
            .count();
        if semver_count == self.branches.len() {
            "SemVer"
        } else if semver_count == 0 {
            "CalVer"
        } else {
            "Mixed"
        }
    }

    pub fn detail_lines(&self) -> Vec<String> {
        let mut lines = vec![
            format!("Name: {}", self.name),
            format!("Project type: {}", self.project_type.display_name()),
            format!("Integration: {}", self.integration_mode.display_name()),
        ];

        if !self.alias.trim().is_empty() {
            lines.push(format!("Alias: {}", self.alias.trim()));
        }

        if self.project_type == ProjectType::AllInOne {
            lines.push(format!(
                "Version scheme: {}",
                self.version_scheme.display_name()
            ));
            for target in &self.targets {
                lines.push(format!(
                    "Target: {} -> {} [{}]",
                    target.path,
                    target.key_path,
                    target.format.display_name()
                ));
            }
            lines.push(format!(
                "Changelog: {}",
                if self.changelog.enabled {
                    "Enabled"
                } else {
                    "Disabled"
                }
            ));
        } else {
            for (scope_index, branch) in self.branches.iter().enumerate() {
                lines.push(format!(
                    "{}: {} ({})",
                    branch.scope_kind.display_name(),
                    branch.display_name(),
                    branch.version_scheme.display_name()
                ));
                lines.push(format!(
                    "  Changelog generation: {}",
                    if self.changelog_enabled_for_scope(scope_index) {
                        "Enabled"
                    } else {
                        "Disabled"
                    }
                ));
                if let Some(repo) = &branch.repo {
                    lines.push(format!("  Scope repo override: {}", repo.local_root));
                    if let Some(remote) = &repo.remote_url {
                        lines.push(format!("    Remote override: {}", remote));
                    }
                }
                for target in &branch.targets {
                    lines.push(format!(
                        "  {} -> {} [{}]",
                        target.path,
                        target.key_path,
                        target.format.display_name()
                    ));
                }
            }
        }

        if let Some(repo) = &self.repo {
            lines.push(format!("Repo root: {}", repo.local_root));
            if let Some(remote) = &repo.remote_url {
                lines.push(format!("Remote: {}", remote));
            }
        }

        lines.push(format!(
            "Changelog path: {}",
            self.changelog.effective_path()
        ));

        lines
    }

    pub fn changelog_path_for_scope(&self, scope_index: usize) -> &str {
        match self.project_type {
            ProjectType::AllInOne => self.changelog.effective_path(),
            ProjectType::Branched => self
                .branches
                .get(scope_index)
                .and_then(|branch| branch.changelog_path.as_deref())
                .filter(|path| !path.trim().is_empty())
                .unwrap_or_else(|| self.changelog.effective_path()),
        }
    }

    pub fn set_changelog_path_for_scope(&mut self, scope_index: usize, file_path: String) {
        let normalized = if file_path.trim().is_empty() {
            DEFAULT_CHANGELOG_PATH.to_string()
        } else {
            file_path.trim().to_string()
        };

        match self.project_type {
            ProjectType::AllInOne => self.changelog.file_path = normalized,
            ProjectType::Branched => {
                if let Some(branch) = self.branches.get_mut(scope_index) {
                    branch.changelog_path = Some(normalized.clone());
                }
                self.changelog.file_path = normalized;
            }
        }
    }

    pub fn changelog_enabled_for_scope(&self, scope_index: usize) -> bool {
        match self.project_type {
            ProjectType::AllInOne => self.changelog.enabled,
            ProjectType::Branched => self
                .branches
                .get(scope_index)
                .map(|branch| branch.changelog_enabled)
                .or_else(|| self.branches.first().map(|branch| branch.changelog_enabled))
                .unwrap_or(false),
        }
    }

    pub fn set_changelog_enabled_for_scope(&mut self, scope_index: usize, enabled: bool) {
        match self.project_type {
            ProjectType::AllInOne => self.changelog.enabled = enabled,
            ProjectType::Branched => {
                if let Some(branch) = self.branches.get_mut(scope_index) {
                    branch.changelog_enabled = enabled;
                }
                self.changelog.enabled = self
                    .branches
                    .first()
                    .map(|branch| branch.changelog_enabled)
                    .unwrap_or(false);
            }
        }
    }

    pub fn release_now_for_scope(&self, scope_index: usize) -> &ReleaseNowSettings {
        match self.project_type {
            ProjectType::AllInOne => &self.release_now,
            ProjectType::Branched => self
                .branches
                .get(scope_index)
                .map(|branch| &branch.release_now)
                .or_else(|| self.branches.first().map(|branch| &branch.release_now))
                .unwrap_or(&self.release_now),
        }
    }

    pub fn release_now_for_scope_mut(&mut self, scope_index: usize) -> &mut ReleaseNowSettings {
        match self.project_type {
            ProjectType::AllInOne => &mut self.release_now,
            ProjectType::Branched => {
                if self.branches.is_empty() {
                    &mut self.release_now
                } else if scope_index < self.branches.len() {
                    &mut self.branches[scope_index].release_now
                } else {
                    &mut self.branches[0].release_now
                }
            }
        }
    }

    pub fn repo_config_for_scope(&self, scope_index: usize) -> Option<&RepoConfig> {
        match self.project_type {
            ProjectType::AllInOne => self.repo.as_ref(),
            ProjectType::Branched => self
                .branches
                .get(scope_index)
                .and_then(|branch| branch.repo.as_ref())
                .or_else(|| {
                    self.branches
                        .first()
                        .and_then(|branch| branch.repo.as_ref())
                })
                .or(self.repo.as_ref()),
        }
    }

    pub fn repo_has_custom_main_branch_for_scope(&self, scope_index: usize) -> bool {
        self.repo_config_for_scope(scope_index)
            .is_some_and(|repo| repo.has_custom_main_branch)
    }

    pub fn repo_custom_main_branch_value_for_scope(&self, scope_index: usize) -> String {
        self.repo_config_for_scope(scope_index)
            .map(|repo| repo.custom_main_branch.clone())
            .unwrap_or_default()
    }

    pub fn repo_main_branch_name_for_scope(&self, scope_index: usize) -> Option<&str> {
        self.repo_config_for_scope(scope_index)
            .and_then(RepoConfig::custom_main_branch_name)
    }

    pub fn set_repo_custom_main_branch_for_scope(
        &mut self,
        scope_index: usize,
        enabled: bool,
        main_branch_name: String,
    ) -> Result<()> {
        let repo = self.repo_config_for_scope_mut_or_insert(scope_index)?;
        repo.has_custom_main_branch = enabled;
        repo.custom_main_branch = if enabled {
            main_branch_name.trim().to_string()
        } else {
            String::new()
        };
        Ok(())
    }

    fn repo_config_for_scope_mut_or_insert(
        &mut self,
        scope_index: usize,
    ) -> Result<&mut RepoConfig> {
        match self.project_type {
            ProjectType::AllInOne => {
                if self.repo.is_none() {
                    let repo_root = derive_repo_root_from_targets(&self.targets).ok_or_else(|| {
                        anyhow!(
                            "no repo root is configured for this project and no target path could derive one"
                        )
                    })?;
                    self.repo = Some(RepoConfig::new(repo_root, None));
                }
                Ok(self.repo.as_mut().expect("repo inserted above"))
            }
            ProjectType::Branched => {
                if self
                    .branches
                    .get(scope_index)
                    .and_then(|branch| branch.repo.as_ref())
                    .is_some()
                {
                    return Ok(self
                        .branches
                        .get_mut(scope_index)
                        .and_then(|branch| branch.repo.as_mut())
                        .expect("branch repo exists for selected scope"));
                }

                if let Some(repo) = self.repo.as_mut() {
                    return Ok(repo);
                }

                let repo_root = self
                    .branches
                    .get(scope_index)
                    .or_else(|| self.branches.first())
                    .and_then(|branch| derive_repo_root_from_targets(&branch.targets))
                    .or_else(|| derive_repo_root_from_targets(&self.targets))
                    .ok_or_else(|| {
                        anyhow!(
                            "no repo root is configured for this scope and no target path could derive one"
                        )
                    })?;

                let target_index = if scope_index < self.branches.len() {
                    scope_index
                } else {
                    0
                };
                let branch = self
                    .branches
                    .get_mut(target_index)
                    .ok_or_else(|| anyhow!("branched project does not contain any scopes"))?;
                branch.repo = Some(RepoConfig::new(repo_root, None));
                Ok(branch.repo.as_mut().expect("branch repo inserted above"))
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ChangelogSettings {
    pub enabled: bool,
    pub file_path: String,
}

impl Default for ChangelogSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            file_path: DEFAULT_CHANGELOG_PATH.to_string(),
        }
    }
}

impl ChangelogSettings {
    pub fn effective_path(&self) -> &str {
        let trimmed = self.file_path.trim();
        if trimmed.is_empty() {
            DEFAULT_CHANGELOG_PATH
        } else {
            trimmed
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ReleaseNowSettings {
    pub enabled: bool,
    pub windows_script: String,
    pub linux_arm_script: String,
    pub linux_amd_script: String,
    pub macos_script: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TileInfoSettings {
    pub auto_rotation: bool,
    pub rotates: TileRotationTarget,
    pub remembered_dev_mode: usize,
    pub remembered_rls_mode: usize,
    pub rotation_timing_seconds: u64,
}

impl Default for TileInfoSettings {
    fn default() -> Self {
        Self {
            auto_rotation: true,
            rotates: TileRotationTarget::Both,
            remembered_dev_mode: 0,
            remembered_rls_mode: 0,
            rotation_timing_seconds: 5,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TileRotationTarget {
    #[default]
    Both,
    DevLineOnly,
    RlsLineOnly,
}

impl TileRotationTarget {
    pub fn display_name(self) -> &'static str {
        match self {
            TileRotationTarget::Both => "both",
            TileRotationTarget::DevLineOnly => "dev-line only",
            TileRotationTarget::RlsLineOnly => "rls-line only",
        }
    }

    pub fn next(self) -> Self {
        match self {
            TileRotationTarget::Both => TileRotationTarget::DevLineOnly,
            TileRotationTarget::DevLineOnly => TileRotationTarget::RlsLineOnly,
            TileRotationTarget::RlsLineOnly => TileRotationTarget::Both,
        }
    }

    pub fn previous(self) -> Self {
        match self {
            TileRotationTarget::Both => TileRotationTarget::RlsLineOnly,
            TileRotationTarget::DevLineOnly => TileRotationTarget::Both,
            TileRotationTarget::RlsLineOnly => TileRotationTarget::DevLineOnly,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProjectType {
    #[default]
    AllInOne,
    Branched,
}

impl ProjectType {
    pub fn display_name(self) -> &'static str {
        match self {
            ProjectType::AllInOne => "All-In-One",
            ProjectType::Branched => "Branched",
        }
    }

    pub fn next(self) -> Self {
        match self {
            ProjectType::AllInOne => ProjectType::Branched,
            ProjectType::Branched => ProjectType::AllInOne,
        }
    }

    pub fn previous(self) -> Self {
        self.next()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum IntegrationMode {
    #[default]
    LocalOnly,
    GitLocalOnly,
    GitHubEnabled,
}

impl IntegrationMode {
    pub fn display_name(self) -> &'static str {
        match self {
            IntegrationMode::LocalOnly => "Local-only",
            IntegrationMode::GitLocalOnly => "GitLocal-only",
            IntegrationMode::GitHubEnabled => "GitHub-enabled",
        }
    }

    pub fn next(self) -> Self {
        match self {
            IntegrationMode::LocalOnly => IntegrationMode::GitLocalOnly,
            IntegrationMode::GitLocalOnly => IntegrationMode::GitHubEnabled,
            IntegrationMode::GitHubEnabled => IntegrationMode::LocalOnly,
        }
    }

    pub fn previous(self) -> Self {
        match self {
            IntegrationMode::LocalOnly => IntegrationMode::GitHubEnabled,
            IntegrationMode::GitLocalOnly => IntegrationMode::LocalOnly,
            IntegrationMode::GitHubEnabled => IntegrationMode::GitLocalOnly,
        }
    }

    pub fn requires_repo(self) -> bool {
        self != IntegrationMode::LocalOnly
    }

    pub fn requires_remote(self) -> bool {
        self == IntegrationMode::GitHubEnabled
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchConfig {
    pub name: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub scope_kind: BranchScopeKind,
    #[serde(default)]
    pub repo: Option<RepoConfig>,
    #[serde(default)]
    pub changelog_enabled: bool,
    #[serde(default)]
    pub changelog_path: Option<String>,
    #[serde(default)]
    pub release_now: ReleaseNowSettings,
    pub version_scheme: VersionScheme,
    #[serde(default)]
    pub targets: Vec<TargetSpec>,
}

impl BranchConfig {
    pub fn display_name(&self) -> &str {
        if self.label.trim().is_empty() {
            &self.name
        } else {
            &self.label
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BranchScopeKind {
    #[default]
    Branch,
    Module,
    Service,
}

impl BranchScopeKind {
    pub fn display_name(self) -> &'static str {
        match self {
            BranchScopeKind::Branch => "Core",
            BranchScopeKind::Module => "Module",
            BranchScopeKind::Service => "Service",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct RepoConfig {
    pub local_root: String,
    pub remote_url: Option<String>,
    pub has_custom_main_branch: bool,
    pub custom_main_branch: String,
}

impl RepoConfig {
    pub fn new(local_root: String, remote_url: Option<String>) -> Self {
        Self {
            local_root,
            remote_url,
            ..Self::default()
        }
    }

    pub fn custom_main_branch_name(&self) -> Option<&str> {
        if !self.has_custom_main_branch {
            return None;
        }

        let branch = self.custom_main_branch.trim();
        if branch.is_empty() {
            None
        } else {
            Some(branch)
        }
    }
}

fn derive_repo_root_from_targets(targets: &[TargetSpec]) -> Option<String> {
    targets.iter().find_map(|target| {
        let path = target.path.trim();
        if path.is_empty() {
            return None;
        }

        Path::new(path)
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .map(|parent| parent.display().to_string())
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetSpec {
    pub label: String,
    pub path: String,
    pub key_path: String,
    pub format: TargetFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TargetFormat {
    #[default]
    Auto,
    Json,
    Toml,
}

impl TargetFormat {
    pub fn display_name(self) -> &'static str {
        match self {
            TargetFormat::Auto => "Auto",
            TargetFormat::Json => "JSON",
            TargetFormat::Toml => "TOML",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConfigStore {
    path: PathBuf,
}

impl ConfigStore {
    pub fn locate() -> Result<Self> {
        let project_dirs = ProjectDirs::from("com", "ComfyHome", "ComfyGit")
            .context("unable to locate the user config directory")?;
        let path = project_dirs.config_dir().join("config.toml");
        Ok(Self { path })
    }

    #[cfg(test)]
    pub fn with_path(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn load(&self) -> Result<AppConfig> {
        if !self.path.exists() {
            return Ok(AppConfig::default());
        }

        let raw = fs::read_to_string(&self.path)
            .with_context(|| format!("failed to read {}", self.path.display()))?;
        let config = toml::from_str::<AppConfig>(&raw)
            .with_context(|| format!("failed to parse {}", self.path.display()))?;
        let (config, changed) = migrate_loaded_config(config)?;
        if changed {
            self.save(&config)?;
        }
        Ok(config)
    }

    pub fn save(&self, config: &AppConfig) -> Result<()> {
        if config.schema_version != SCHEMA_VERSION {
            bail!(
                "unsupported config schema version {}",
                config.schema_version
            );
        }

        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let rendered = toml::to_string_pretty(config).context("failed to render config")?;
        fs::write(&self.path, rendered)
            .with_context(|| format!("failed to write {}", self.path.display()))?;
        Ok(())
    }
}

fn migrate_loaded_config(mut config: AppConfig) -> Result<(AppConfig, bool)> {
    let original_schema_version = config.schema_version;
    if config.schema_version > SCHEMA_VERSION {
        bail!(
            "unsupported config schema version {}",
            config.schema_version
        );
    }

    let mut changed = false;

    for project in &mut config.projects {
        if project.project_type != ProjectType::Branched {
            continue;
        }

        if project.branches.is_empty() && !project.targets.is_empty() {
            let targets = std::mem::take(&mut project.targets);
            project.branches.push(BranchConfig {
                name: "core".to_string(),
                label: "core".to_string(),
                scope_kind: BranchScopeKind::Branch,
                repo: None,
                changelog_enabled: project.changelog.enabled,
                changelog_path: Some(project.changelog.effective_path().to_string()),
                release_now: project.release_now.clone(),
                version_scheme: project.version_scheme,
                targets,
            });
            changed = true;
        }

        for branch in &mut project.branches {
            if branch.label.trim().is_empty() {
                branch.label = branch.name.clone();
                changed = true;
            }
            if original_schema_version < 4 {
                branch.changelog_enabled = project.changelog.enabled;
                branch.changelog_path = Some(project.changelog.effective_path().to_string());
                branch.release_now = project.release_now.clone();
                changed = true;
            }
        }
    }

    if config.schema_version != SCHEMA_VERSION {
        config.schema_version = SCHEMA_VERSION;
        changed = true;
    }

    Ok((config, changed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branch_display_name_falls_back_to_name() {
        let branch = BranchConfig {
            name: "core".to_string(),
            label: String::new(),
            scope_kind: BranchScopeKind::Branch,
            repo: None,
            changelog_enabled: false,
            changelog_path: None,
            release_now: ReleaseNowSettings::default(),
            version_scheme: VersionScheme::SemVer,
            targets: Vec::new(),
        };

        assert_eq!(branch.display_name(), "core");
    }

    #[test]
    fn legacy_branch_config_deserializes_with_new_defaults() {
        let raw = r#"
schema_version = 1

[ui]
accent_color = "cyan"
show_mouse_hints = true
show_tab_hints = true

[[projects]]
name = "Example"
project_type = "branched"
integration_mode = "git_local_only"
unified_versioning = true
version_scheme = "sem_ver"
targets = []

[[projects.branches]]
name = "core"
version_scheme = "sem_ver"

[[projects.branches.targets]]
label = "Version"
path = "package.json"
key_path = "version"
format = "json"
"#;

        let config = toml::from_str::<AppConfig>(raw).expect("legacy config should parse");
        let branch = &config.projects[0].branches[0];

        assert_eq!(branch.name, "core");
        assert_eq!(branch.display_name(), "core");
        assert_eq!(branch.scope_kind, BranchScopeKind::Branch);
        assert!(branch.repo.is_none());
        assert!(!branch.changelog_enabled);
        assert_eq!(branch.targets.len(), 1);
        assert!(!config.projects[0].changelog.enabled);
        assert_eq!(
            config.projects[0].changelog.effective_path(),
            DEFAULT_CHANGELOG_PATH
        );
    }

    #[test]
    fn migration_upgrades_legacy_schema_and_branch_labels() {
        let raw = r#"
schema_version = 1

[ui]
accent_color = "cyan"
show_mouse_hints = true
show_tab_hints = true

[[projects]]
name = "Example"
project_type = "branched"
integration_mode = "git_local_only"
unified_versioning = true
version_scheme = "sem_ver"
targets = []

[[projects.branches]]
name = "core"
version_scheme = "sem_ver"

[[projects.branches.targets]]
label = "Version"
path = "package.json"
key_path = "version"
format = "json"
"#;

        let parsed = toml::from_str::<AppConfig>(raw).expect("legacy config should parse");
        let (migrated, changed) = migrate_loaded_config(parsed).expect("migration should succeed");

        assert!(changed);
        assert_eq!(migrated.schema_version, SCHEMA_VERSION);
        assert_eq!(migrated.projects[0].branches[0].label, "core");
        assert!(!migrated.projects[0].branches[0].changelog_enabled);
    }

    #[test]
    fn migration_promotes_legacy_branched_targets_into_default_scope() {
        let config = AppConfig {
            schema_version: 1,
            projects: vec![ProjectConfig {
                name: "Example".to_string(),
                alias: String::new(),
                project_type: ProjectType::Branched,
                integration_mode: IntegrationMode::LocalOnly,
                unified_versioning: true,
                version_scheme: VersionScheme::SemVer,
                changelog: ChangelogSettings::default(),
                release_now: ReleaseNowSettings::default(),
                tile_info: TileInfoSettings::default(),
                targets: vec![TargetSpec {
                    label: "Version".to_string(),
                    path: "package.json".to_string(),
                    key_path: "version".to_string(),
                    format: TargetFormat::Json,
                }],
                branches: Vec::new(),
                repo: None,
            }],
            ui: UiSettings::default(),
        };

        let (migrated, changed) = migrate_loaded_config(config).expect("migration should succeed");

        assert!(changed);
        assert!(migrated.projects[0].targets.is_empty());
        assert_eq!(migrated.projects[0].branches.len(), 1);
        assert_eq!(migrated.projects[0].branches[0].name, "core");
        assert_eq!(
            migrated.projects[0].branches[0].targets[0].path,
            "package.json"
        );
    }

    #[test]
    fn branched_summary_reports_semver_or_mixed_compactly() {
        let semver_project = ProjectConfig {
            name: "Example".to_string(),
            alias: String::new(),
            project_type: ProjectType::Branched,
            integration_mode: IntegrationMode::LocalOnly,
            unified_versioning: false,
            version_scheme: VersionScheme::SemVer,
            changelog: ChangelogSettings::default(),
            release_now: ReleaseNowSettings::default(),
            tile_info: TileInfoSettings::default(),
            targets: Vec::new(),
            branches: vec![BranchConfig {
                name: "core".to_string(),
                label: "core".to_string(),
                scope_kind: BranchScopeKind::Branch,
                repo: None,
                changelog_enabled: false,
                changelog_path: None,
                release_now: ReleaseNowSettings::default(),
                version_scheme: VersionScheme::SemVer,
                targets: Vec::new(),
            }],
            repo: None,
        };

        let mixed_project = ProjectConfig {
            alias: String::new(),
            branches: vec![
                BranchConfig {
                    version_scheme: VersionScheme::SemVer,
                    ..semver_project.branches[0].clone()
                },
                BranchConfig {
                    name: "api".to_string(),
                    label: "api".to_string(),
                    scope_kind: BranchScopeKind::Service,
                    repo: None,
                    changelog_enabled: false,
                    changelog_path: None,
                    release_now: ReleaseNowSettings::default(),
                    version_scheme: VersionScheme::CalVerYearMonthMicro,
                    targets: Vec::new(),
                },
            ],
            ..semver_project.clone()
        };

        assert!(semver_project.summary().ends_with("SemVer"));
        assert!(mixed_project.summary().ends_with("Mixed"));
    }

    #[test]
    fn changelog_settings_fall_back_to_default_path_when_blank() {
        let settings = ChangelogSettings {
            enabled: true,
            file_path: "   ".to_string(),
        };

        assert_eq!(settings.effective_path(), DEFAULT_CHANGELOG_PATH);
    }
}
