// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyVersionBumper License v1.2
//
// For details, see the LICENSE file in the repository root.

use std::{fs, path::{Path, PathBuf}};

use anyhow::{Context, Result, bail};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::versioning::VersionScheme;

pub const SCHEMA_VERSION: u32 = 1;

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
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            accent_color: "cyan".to_string(),
            show_mouse_hints: true,
            show_tab_hints: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub name: String,
    pub project_type: ProjectType,
    pub integration_mode: IntegrationMode,
    pub unified_versioning: bool,
    pub version_scheme: VersionScheme,
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
                "{} | {} | {} target{}",
                self.project_type.display_name(),
                self.version_scheme.display_name(),
                self.targets.len(),
                if self.targets.len() == 1 { "" } else { "s" }
            ),
            ProjectType::Branched => format!(
                "{} | {} scope{} | {}",
                self.project_type.display_name(),
                self.branches.len(),
                if self.branches.len() == 1 { "" } else { "s" },
                if self.unified_versioning {
                    format!("unified {}", self.version_scheme.display_name())
                } else {
                    "per-branch versioning".to_string()
                }
            ),
        }
    }

    pub fn detail_lines(&self) -> Vec<String> {
        let mut lines = vec![
            format!("Name: {}", self.name),
            format!("Project type: {}", self.project_type.display_name()),
            format!("Integration: {}", self.integration_mode.display_name()),
        ];

        if self.project_type == ProjectType::AllInOne {
            lines.push(format!("Version scheme: {}", self.version_scheme.display_name()));
            for target in &self.targets {
                lines.push(format!("Target: {} -> {} [{}]", target.path, target.key_path, target.format.display_name()));
            }
        } else {
            lines.push(format!("Unified versioning: {}", if self.unified_versioning { "yes" } else { "no" }));
            for branch in &self.branches {
                lines.push(format!(
                    "{}: {} ({})",
                    branch.scope_kind.display_name(),
                    branch.display_name(),
                    branch.version_scheme.display_name()
                ));
                if let Some(repo) = &branch.repo {
                    lines.push(format!("  Scope repo override: {}", repo.local_root));
                    if let Some(remote) = &repo.remote_url {
                        lines.push(format!("    Remote override: {}", remote));
                    }
                }
                for target in &branch.targets {
                    lines.push(format!("  {} -> {} [{}]", target.path, target.key_path, target.format.display_name()));
                }
            }
        }

        if let Some(repo) = &self.repo {
            lines.push(format!("Repo root: {}", repo.local_root));
            if let Some(remote) = &repo.remote_url {
                lines.push(format!("Remote: {}", remote));
            }
        }

        lines
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
    pub version_scheme: VersionScheme,
    #[serde(default)]
    pub targets: Vec<TargetSpec>,
}

impl BranchConfig {
    pub fn new(name: impl Into<String>, version_scheme: VersionScheme, targets: Vec<TargetSpec>) -> Self {
        let name = name.into();
        Self {
            label: name.clone(),
            name,
            scope_kind: BranchScopeKind::default(),
            repo: None,
            version_scheme,
            targets,
        }
    }

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
            BranchScopeKind::Branch => "Branch",
            BranchScopeKind::Module => "Module",
            BranchScopeKind::Service => "Service",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoConfig {
    pub local_root: String,
    pub remote_url: Option<String>,
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
        let project_dirs = ProjectDirs::from("com", "ComfyHome", "ComfyVersionBumper")
            .context("unable to locate the user config directory")?;
        let path = project_dirs.config_dir().join("config.toml");
        Ok(Self { path })
    }

    pub fn load(&self) -> Result<AppConfig> {
        if !self.path.exists() {
            return Ok(AppConfig::default());
        }

        let raw = fs::read_to_string(&self.path)
            .with_context(|| format!("failed to read {}", self.path.display()))?;
        let config = toml::from_str::<AppConfig>(&raw)
            .with_context(|| format!("failed to parse {}", self.path.display()))?;
        Ok(config)
    }

    pub fn save(&self, config: &AppConfig) -> Result<()> {
        if config.schema_version != SCHEMA_VERSION {
            bail!("unsupported config schema version {}", config.schema_version);
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

    pub fn path(&self) -> &Path {
        &self.path
    }
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
        assert_eq!(branch.targets.len(), 1);
    }
}