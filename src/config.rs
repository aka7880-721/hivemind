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
    pub targets: Vec<TargetSpec>,
    pub branches: Vec<BranchConfig>,
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
                "{} | {} branch{} | {}",
                self.project_type.display_name(),
                self.branches.len(),
                if self.branches.len() == 1 { "" } else { "es" },
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
                lines.push(format!("Branch: {} ({})", branch.name, branch.version_scheme.display_name()));
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
    pub version_scheme: VersionScheme,
    pub targets: Vec<TargetSpec>,
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