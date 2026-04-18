// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit License v1.2
//
// For details, see the LICENSE file in the repository root.

use std::{borrow::Cow, fs, path::Path};

use anyhow::{Context, Result, anyhow, bail};
use serde_json::Value as JsonValue;
use toml_edit::{DocumentMut, Item, Value, value};

use crate::{
    config::{BranchScopeKind, ProjectConfig, ProjectType, TargetFormat, TargetSpec},
    versioning::VersionScheme,
};

#[derive(Clone)]
pub(crate) struct TargetProbe {
    pub(crate) kind: ProbeKind,
    pub(crate) message: String,
    pub(crate) version: Option<String>,
    pub(crate) format: Option<TargetFormat>,
}

#[derive(Clone, Copy)]
pub(crate) enum ProbeKind {
    Success,
    Warning,
    Error,
}

#[derive(Clone)]
pub(crate) struct BumpTarget {
    pub(crate) label: String,
    pub(crate) path: String,
    pub(crate) key_path: String,
    pub(crate) format: TargetFormat,
    pub(crate) current_version: String,
}

#[derive(Clone)]
pub(crate) struct BumpScope {
    pub(crate) display_name: String,
    pub(crate) scope_kind: Option<BranchScopeKind>,
    pub(crate) scheme: VersionScheme,
    pub(crate) current_version: Option<String>,
    pub(crate) targets: Vec<BumpTarget>,
}

impl BumpScope {
    pub(crate) fn version_label(&self) -> &str {
        self.current_version.as_deref().unwrap_or("mixed values")
    }

    pub(crate) fn has_mismatch(&self) -> bool {
        self.current_version.is_none()
    }
}

#[derive(Clone)]
struct TargetValue {
    version: String,
    format: TargetFormat,
}

pub(crate) fn probe_target(
    path: &str,
    key_path: &str,
    scheme: VersionScheme,
) -> Result<TargetProbe> {
    if path.is_empty() {
        bail!("target path is empty");
    }
    if key_path.is_empty() {
        bail!("target key is empty");
    }

    let target = read_target_value(path, key_path, TargetFormat::Auto)?;
    let format = target.format;
    let version = target.version;

    let kind = match scheme.validate(&version) {
        Ok(()) => ProbeKind::Success,
        Err(_) => ProbeKind::Warning,
    };
    let message = match scheme.validate(&version) {
        Ok(()) => format!("{} -> {} matches {}", path, key_path, scheme.display_name()),
        Err(error) => format!(
            "{} -> {} is readable, but '{}' does not match {}: {}",
            path,
            key_path,
            version,
            scheme.display_name(),
            error
        ),
    };

    Ok(TargetProbe {
        kind,
        message,
        version: Some(version),
        format: Some(format),
    })
}

pub(crate) fn collect_bump_scopes(project: &ProjectConfig) -> Result<Vec<BumpScope>> {
    if project.project_type == ProjectType::AllInOne {
        return Ok(vec![build_bump_scope(
            project.name.clone(),
            None,
            project.version_scheme,
            &project.targets,
        )?]);
    }

    project
        .branches
        .iter()
        .map(|branch| {
            let scheme = if project.unified_versioning {
                project.version_scheme
            } else {
                branch.version_scheme
            };
            build_bump_scope(
                branch.display_name().to_string(),
                Some(branch.scope_kind),
                scheme,
                &branch.targets,
            )
        })
        .collect()
}

pub(crate) fn shared_bump_version(scopes: &[BumpScope]) -> Option<String> {
    let first = scopes.first()?.current_version.as_ref()?;
    if scopes
        .iter()
        .all(|scope| scope.current_version.as_deref() == Some(first.as_str()))
    {
        Some(first.clone())
    } else {
        None
    }
}

pub(crate) fn write_target_version(target: &BumpTarget, new_version: &str) -> Result<()> {
    let content = fs::read_to_string(&target.path)
        .with_context(|| format!("failed to read {}", target.path))?;
    match target.format {
        TargetFormat::Json => {
            write_json_value(&target.path, &content, &target.key_path, new_version)
        }
        TargetFormat::Toml => {
            write_toml_value(&target.path, &content, &target.key_path, new_version)
        }
        TargetFormat::Auto => bail!("cannot write target with unresolved format"),
    }
}

fn read_target_value(path: &str, key_path: &str, hint: TargetFormat) -> Result<TargetValue> {
    let content = fs::read_to_string(path).with_context(|| format!("failed to read {}", path))?;
    let format = if hint == TargetFormat::Auto {
        detect_format(path, &content)?
    } else {
        hint
    };

    let version = match format {
        TargetFormat::Json => extract_json_value(&content, key_path)?,
        TargetFormat::Toml => extract_toml_value(&content, key_path)?,
        TargetFormat::Auto => unreachable!(),
    };

    Ok(TargetValue { version, format })
}

fn build_bump_scope(
    display_name: String,
    scope_kind: Option<BranchScopeKind>,
    scheme: VersionScheme,
    specs: &[TargetSpec],
) -> Result<BumpScope> {
    let mut targets = Vec::with_capacity(specs.len());
    for target in specs {
        let target_value = read_target_value(&target.path, &target.key_path, target.format)?;
        targets.push(BumpTarget {
            label: target.label.clone(),
            path: target.path.clone(),
            key_path: target.key_path.clone(),
            format: target_value.format,
            current_version: target_value.version,
        });
    }

    let current_version = targets
        .first()
        .map(|target| target.current_version.clone())
        .filter(|current| {
            targets
                .iter()
                .all(|target| target.current_version == *current)
        });

    Ok(BumpScope {
        display_name,
        scope_kind,
        scheme,
        current_version,
        targets,
    })
}

fn detect_format(path: &str, content: &str) -> Result<TargetFormat> {
    let extension = Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase());

    match extension.as_deref() {
        Some("json") => Ok(TargetFormat::Json),
        Some("toml") => Ok(TargetFormat::Toml),
        _ => {
            if serde_json::from_str::<JsonValue>(content).is_ok() {
                Ok(TargetFormat::Json)
            } else if toml::from_str::<toml::Value>(content).is_ok() {
                Ok(TargetFormat::Toml)
            } else {
                Err(anyhow!(
                    "unable to detect JSON or TOML format from target file"
                ))
            }
        }
    }
}

fn write_json_value(path: &str, content: &str, key_path: &str, new_value: &str) -> Result<()> {
    let mut value = serde_json::from_str::<JsonValue>(content).context("invalid JSON target")?;
    let located = locate_json_value_mut(&mut value, key_path)?;
    *located = JsonValue::String(new_value.to_string());
    let mut rendered =
        serde_json::to_string_pretty(&value).context("failed to serialize JSON target")?;
    rendered.push('\n');
    fs::write(path, rendered).with_context(|| format!("failed to write {}", path))?;
    Ok(())
}

fn write_toml_value(path: &str, content: &str, key_path: &str, new_value: &str) -> Result<()> {
    let mut document = content
        .parse::<DocumentMut>()
        .context("invalid TOML target")?;
    let target_key = if locate_toml_item_mut(document.as_item_mut(), key_path).is_ok() {
        key_path.to_string()
    } else if !key_path.contains('.') {
        if let Some(package) = document.as_item().get("package") {
            if package.get(key_path).is_some() {
                format!("package.{}", key_path)
            } else {
                key_path.to_string()
            }
        } else {
            key_path.to_string()
        }
    } else {
        key_path.to_string()
    };

    let item = locate_toml_item_mut(document.as_item_mut(), &target_key)?;
    if item.is_value() {
        *item = Item::Value(Value::from(new_value.to_string()));
    } else {
        *item = value(new_value);
    }
    fs::write(path, document.to_string()).with_context(|| format!("failed to write {}", path))?;
    Ok(())
}

fn extract_json_value(content: &str, key_path: &str) -> Result<String> {
    let value = serde_json::from_str::<JsonValue>(content).context("invalid JSON target")?;
    let located = key_path.split('.').try_fold(&value, |current, segment| {
        current
            .get(segment)
            .ok_or_else(|| anyhow!("missing key '{}'", key_path))
    })?;
    located.as_str().map(ToOwned::to_owned).ok_or_else(|| {
        anyhow!(
            "key '{}' is present, but its value is not a string",
            key_path
        )
    })
}

fn extract_toml_value(content: &str, key_path: &str) -> Result<String> {
    let value = toml::from_str::<toml::Value>(content).context("invalid TOML target")?;
    let key_path = expand_toml_key_path(&value, key_path);
    let located = locate_toml_value(&value, &key_path)?;
    located.as_str().map(ToOwned::to_owned).ok_or_else(|| {
        anyhow!(
            "key '{}' is present, but its value is not a string",
            key_path
        )
    })
}

fn expand_toml_key_path<'a>(value: &'a toml::Value, key_path: &'a str) -> Cow<'a, str> {
    if key_path.contains('.') {
        return Cow::Borrowed(key_path);
    }

    if value.get(key_path).is_some() {
        return Cow::Borrowed(key_path);
    }

    if let Some(package) = value.get("package") {
        if package.get(key_path).is_some() {
            return Cow::Owned(format!("package.{}", key_path));
        }
    }

    Cow::Borrowed(key_path)
}

fn locate_toml_value<'a>(value: &'a toml::Value, key_path: &str) -> Result<&'a toml::Value> {
    let mut current = value;
    for segment in key_path.split('.') {
        current = current
            .get(segment)
            .ok_or_else(|| anyhow!("missing key '{}'", key_path))?;
    }
    Ok(current)
}

fn locate_json_value_mut<'a>(
    value: &'a mut JsonValue,
    key_path: &str,
) -> Result<&'a mut JsonValue> {
    let mut current = value;
    for segment in key_path.split('.') {
        current = current
            .get_mut(segment)
            .ok_or_else(|| anyhow!("missing key '{}'", key_path))?;
    }
    Ok(current)
}

fn locate_toml_item_mut<'a>(item: &'a mut Item, key_path: &str) -> Result<&'a mut Item> {
    let mut current = item;
    for segment in key_path.split('.') {
        current = current
            .get_mut(segment)
            .ok_or_else(|| anyhow!("missing key '{}'", key_path))?;
    }
    Ok(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_cargo_toml_version_without_package_prefix() {
        let content = r#"
[package]
name = "comfy-version-bumper"
version = "0.1.0"
edition = "2024"
"#;
        let resolved =
            extract_toml_value(content, "version").expect("should resolve package.version");
        assert_eq!(resolved, "0.1.0");
    }

    #[test]
    fn shared_bump_version_rejects_scope_mismatches() {
        let scopes = vec![
            BumpScope {
                display_name: "Core".to_string(),
                scope_kind: Some(BranchScopeKind::Module),
                scheme: VersionScheme::SemVer,
                current_version: Some("1.2.3".to_string()),
                targets: Vec::new(),
            },
            BumpScope {
                display_name: "API".to_string(),
                scope_kind: Some(BranchScopeKind::Service),
                scheme: VersionScheme::SemVer,
                current_version: Some("1.2.4".to_string()),
                targets: Vec::new(),
            },
        ];

        assert!(shared_bump_version(&scopes).is_none());
    }
}
