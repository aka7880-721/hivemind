// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit SA-PSLicense
//
// For details, see the LICENSE file in the repository root.

use std::{borrow::Cow, fs, path::Path};

use anyhow::{Context, Result, anyhow, bail};
use configparser::ini::Ini;
use roxmltree::Document;
use serde_json::Value as JsonValue;
use serde_yaml::Value as YamlValue;
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
    if key_path.trim().is_empty() && !is_plain_version_filename(path) {
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
        TargetFormat::Yaml => {
            write_yaml_value(&target.path, &content, &target.key_path, new_version)
        }
        TargetFormat::Xml => write_xml_value(&target.path, &content, &target.key_path, new_version),
        TargetFormat::Ini => write_ini_value(&target.path, &content, &target.key_path, new_version),
        TargetFormat::Plain => write_plain_value(&target.path, &content, new_version),
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
        TargetFormat::Yaml => extract_yaml_value(&content, key_path)?,
        TargetFormat::Xml => extract_xml_value(&content, key_path)?,
        TargetFormat::Ini => extract_ini_value(&content, key_path)?,
        TargetFormat::Plain => extract_plain_value(&content, key_path)?,
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

pub(crate) fn is_plain_version_filename(path: &str) -> bool {
    let file_name = Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path)
        .to_ascii_lowercase();
    matches!(
        file_name.as_str(),
        "version" | "version.txt" | ".version" | "version.md" | "VERSION"
    )
}

fn detect_format(path: &str, content: &str) -> Result<TargetFormat> {
    if is_plain_version_filename(path) {
        return Ok(TargetFormat::Plain);
    }

    let extension = Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase());

    match extension.as_deref() {
        Some("json") => Ok(TargetFormat::Json),
        Some("toml") => Ok(TargetFormat::Toml),
        Some("yaml") | Some("yml") => Ok(TargetFormat::Yaml),
        Some("xml") => Ok(TargetFormat::Xml),
        Some("cfg") => Ok(TargetFormat::Ini),
        _ => {
            if serde_json::from_str::<JsonValue>(content).is_ok() {
                Ok(TargetFormat::Json)
            } else if toml::from_str::<toml::Value>(content).is_ok() {
                Ok(TargetFormat::Toml)
            } else if serde_yaml::from_str::<YamlValue>(content).is_ok() {
                Ok(TargetFormat::Yaml)
            } else if Document::parse(content).is_ok() {
                Ok(TargetFormat::Xml)
            } else if load_ini(content).is_ok() {
                Ok(TargetFormat::Ini)
            } else if extract_plain_value(content, "").is_ok() {
                Ok(TargetFormat::Plain)
            } else {
                Err(anyhow!(
                    "unable to detect target format (supported: JSON, TOML, YAML, XML, INI, plain version file)"
                ))
            }
        }
    }
}

fn plain_key_path(key_path: &str) -> Result<()> {
    let key_path = key_path.trim();
    if key_path.is_empty() || key_path == "." || key_path == "@" {
        return Ok(());
    }
    bail!("plain version files do not use key paths (leave the key empty)");
}

fn extract_plain_value(content: &str, key_path: &str) -> Result<String> {
    plain_key_path(key_path)?;
    parse_plain_version(content)
}

fn write_plain_value(path: &str, content: &str, new_version: &str) -> Result<()> {
    let had_trailing_newline = content.ends_with('\n');
    let mut rendered = new_version.to_string();
    if had_trailing_newline {
        rendered.push('\n');
    }
    fs::write(path, rendered).with_context(|| format!("failed to write {}", path))?;
    Ok(())
}

fn parse_plain_version(content: &str) -> Result<String> {
    let line = content
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
        .trim();
    if line.is_empty() {
        bail!("plain version file is empty");
    }
    let version = line
        .trim_matches(|character: char| character == '"' || character == '\'')
        .trim()
        .to_string();
    if version.is_empty() {
        bail!("plain version file does not contain a version value");
    }
    Ok(version)
}

fn parse_ini_key_path(key_path: &str) -> Result<(&str, &str)> {
    let key_path = key_path.trim();
    if key_path.is_empty() {
        bail!("INI targets require a section.key path (for example metadata.version)");
    }
    let (section, key) = key_path.split_once('.').ok_or_else(|| {
        anyhow!("INI key path must use section.key format (for example metadata.version)")
    })?;
    if section.is_empty() || key.is_empty() {
        bail!("INI key path must use section.key format (for example metadata.version)");
    }
    Ok((section, key))
}

fn load_ini(content: &str) -> Result<Ini> {
    let mut ini = Ini::new();
    ini.read(content.to_string())
        .map_err(|error| anyhow!("invalid INI target: {error}"))?;
    Ok(ini)
}

fn extract_ini_value(content: &str, key_path: &str) -> Result<String> {
    let (section, key) = parse_ini_key_path(key_path)?;
    let ini = load_ini(content)?;
    ini.get(section, key)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("missing key '{}'", key_path))
}

fn write_ini_value(path: &str, content: &str, key_path: &str, new_value: &str) -> Result<()> {
    let (section, key) = parse_ini_key_path(key_path)?;
    let mut ini = load_ini(content)?;
    ini.setstr(section, key, Some(new_value));
    ini.write(path)
        .with_context(|| format!("failed to write {}", path))?;
    Ok(())
}

fn extract_yaml_value(content: &str, key_path: &str) -> Result<String> {
    let value = serde_yaml::from_str::<YamlValue>(content).context("invalid YAML target")?;
    let located = locate_yaml_value(&value, key_path)?;
    yaml_value_as_string(located, key_path)
}

fn write_yaml_value(path: &str, content: &str, key_path: &str, new_value: &str) -> Result<()> {
    let mut value = serde_yaml::from_str::<YamlValue>(content).context("invalid YAML target")?;
    let located = locate_yaml_value_mut(&mut value, key_path)?;
    *located = YamlValue::String(new_value.to_string());
    let rendered = serde_yaml::to_string(&value).context("failed to serialize YAML target")?;
    fs::write(path, rendered).with_context(|| format!("failed to write {}", path))?;
    Ok(())
}

fn locate_yaml_value<'a>(value: &'a YamlValue, key_path: &str) -> Result<&'a YamlValue> {
    let mut current = value;
    for segment in key_path.split('.') {
        current = current
            .get(segment)
            .ok_or_else(|| anyhow!("missing key '{}'", key_path))?;
    }
    Ok(current)
}

fn locate_yaml_value_mut<'a>(
    value: &'a mut YamlValue,
    key_path: &str,
) -> Result<&'a mut YamlValue> {
    let mut current = value;
    for segment in key_path.split('.') {
        current = current
            .get_mut(segment)
            .ok_or_else(|| anyhow!("missing key '{}'", key_path))?;
    }
    Ok(current)
}

fn yaml_value_as_string(value: &YamlValue, key_path: &str) -> Result<String> {
    match value {
        YamlValue::String(text) => Ok(text.clone()),
        YamlValue::Number(number) => Ok(number.to_string()),
        YamlValue::Bool(flag) => Ok(flag.to_string()),
        _ => bail!(
            "key '{}' is present, but its value is not a string",
            key_path
        ),
    }
}

fn extract_xml_value(content: &str, key_path: &str) -> Result<String> {
    let key_path = key_path.trim();
    if key_path.is_empty() {
        bail!("XML targets require a dotted element path (for example project.version)");
    }
    let document = Document::parse(content).context("invalid XML target")?;
    let mut node = document.root_element();
    let mut segments: Vec<&str> = key_path.split('.').collect();
    if segments
        .first()
        .is_some_and(|segment| node.tag_name().name() == *segment)
    {
        segments.remove(0);
    }
    for segment in segments {
        node = node
            .children()
            .find(|child| child.is_element() && child.tag_name().name() == segment)
            .ok_or_else(|| anyhow!("missing XML element '{}'", key_path))?;
    }
    node.text()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            anyhow!(
                "XML element '{}' is present, but it does not contain text",
                key_path
            )
        })
}

fn write_xml_value(path: &str, content: &str, key_path: &str, new_value: &str) -> Result<()> {
    let key_path = key_path.trim();
    if key_path.is_empty() {
        bail!("XML targets require a dotted element path (for example project.version)");
    }
    let old = extract_xml_value(content, key_path)?;
    if old == new_value {
        return Ok(());
    }
    let tag = key_path.rsplit('.').next().unwrap_or(key_path);
    let updated = replace_xml_tag_text(content, tag, &old, new_value)
        .ok_or_else(|| anyhow!("XML element '{}' does not contain editable text", key_path))?;
    fs::write(path, updated).with_context(|| format!("failed to write {}", path))?;
    Ok(())
}

fn replace_xml_tag_text(content: &str, tag: &str, old: &str, new: &str) -> Option<String> {
    let open_prefix = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut cursor = 0usize;
    while let Some(open_start) = content[cursor..].find(&open_prefix) {
        let abs_open = cursor + open_start;
        let after_open = content[abs_open..].find('>')? + abs_open + 1;
        let close_rel = content[after_open..].find(&close)?;
        let text_end = after_open + close_rel;
        if content[after_open..text_end].trim() == old {
            let mut updated =
                String::with_capacity(content.len() + new.len().saturating_sub(old.len()));
            updated.push_str(&content[..after_open]);
            updated.push_str(new);
            updated.push_str(&content[text_end..]);
            return Some(updated);
        }
        cursor = text_end + close.len();
    }
    None
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

    if let Some(package) = value.get("package")
        && package.get(key_path).is_some()
    {
        return Cow::Owned(format!("package.{}", key_path));
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
    fn yaml_version_round_trip() {
        let dir = std::env::temp_dir().join(format!("comfygit-yaml-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join("Chart.yaml");
        fs::write(&path, "apiVersion: v2\nversion: 1.2.3\n").expect("write yaml");

        let read = extract_yaml_value("apiVersion: v2\nversion: 1.2.3\n", "version").expect("read");
        assert_eq!(read, "1.2.3");

        write_yaml_value(
            path.to_str().expect("path"),
            "apiVersion: v2\nversion: 1.2.3\n",
            "version",
            "2.0.0",
        )
        .expect("write");
        let updated = fs::read_to_string(&path).expect("read back");
        assert!(updated.contains("2.0.0"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn xml_maven_version_round_trip() {
        let content = r#"<?xml version="1.0" encoding="UTF-8"?>
<project>
  <version>1.2.3</version>
</project>
"#;
        let dir = std::env::temp_dir().join(format!("comfygit-xml-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join("pom.xml");
        fs::write(&path, content).expect("write xml");

        let read = extract_xml_value(content, "project.version").expect("read");
        assert_eq!(read, "1.2.3");

        write_xml_value(
            path.to_str().expect("path"),
            content,
            "project.version",
            "2.0.0",
        )
        .expect("write");
        let updated = fs::read_to_string(&path).expect("read back");
        assert!(updated.contains(">2.0.0<"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn plain_version_file_round_trip() {
        let dir = std::env::temp_dir().join(format!("comfygit-plain-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join("VERSION");
        fs::write(&path, "1.2.3\n").expect("write version");

        let read = extract_plain_value("1.2.3\n", "").expect("read");
        assert_eq!(read, "1.2.3");

        write_plain_value(path.to_str().expect("path"), "1.2.3\n", "2.0.0").expect("write");
        assert_eq!(
            fs::read_to_string(&path).expect("read back").trim(),
            "2.0.0"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn ini_setup_cfg_version_round_trip() {
        let content = "[metadata]\nname = demo\nversion = 1.2.3\n";
        let dir = std::env::temp_dir().join(format!("comfygit-ini-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join("setup.cfg");
        fs::write(&path, content).expect("write ini");

        let read = extract_ini_value(content, "metadata.version").expect("read");
        assert_eq!(read, "1.2.3");

        write_ini_value(
            path.to_str().expect("path"),
            content,
            "metadata.version",
            "2.0.0",
        )
        .expect("write");
        let updated = fs::read_to_string(&path).expect("read back");
        assert!(updated.contains("version=2.0.0"));

        let _ = fs::remove_dir_all(&dir);
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
