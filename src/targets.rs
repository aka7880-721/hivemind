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
        TargetFormat::GoMod => {
            write_gomod_value(&target.path, &content, &target.key_path, new_version)
        }
        TargetFormat::Ruby => {
            write_ruby_value(&target.path, &content, &target.key_path, new_version)
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
        TargetFormat::Yaml => extract_yaml_value(&content, key_path)?,
        TargetFormat::Xml => extract_xml_value(&content, key_path)?,
        TargetFormat::Ini => extract_ini_value(&content, key_path)?,
        TargetFormat::Plain => extract_plain_value(&content, key_path)?,
        TargetFormat::GoMod => extract_gomod_value(&content, key_path)?,
        TargetFormat::Ruby => extract_ruby_value(path, &content, key_path)?,
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

pub(crate) fn is_gomod_filename(path: &str) -> bool {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("go.mod"))
}

pub(crate) fn is_ruby_manifest_filename(path: &str) -> bool {
    let file_name = Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path);
    file_name.eq_ignore_ascii_case("Gemfile")
        || file_name.to_ascii_lowercase().ends_with(".gemspec")
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
    if is_gomod_filename(path) {
        return Ok(TargetFormat::GoMod);
    }
    if is_ruby_manifest_filename(path) {
        return Ok(TargetFormat::Ruby);
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
            } else if extract_gomod_value(content, "comment").is_ok() {
                Ok(TargetFormat::GoMod)
            } else if extract_ruby_value(path, content, "version").is_ok() {
                Ok(TargetFormat::Ruby)
            } else if extract_plain_value(content, "").is_ok() {
                Ok(TargetFormat::Plain)
            } else {
                Err(anyhow!(
                    "unable to detect target format (supported: JSON, TOML, YAML, XML, INI, go.mod, Ruby, plain version file)"
                ))
            }
        }
    }
}

enum GoModKey<'a> {
    Comment,
    Require(&'a str),
}

fn parse_gomod_key_path(key_path: &str) -> Result<GoModKey<'_>> {
    let key_path = key_path.trim();
    if key_path.is_empty() || key_path == "@" || key_path == "." || key_path == "comment" {
        return Ok(GoModKey::Comment);
    }
    if let Some(module) = key_path.strip_prefix("require.") {
        let module = module.trim();
        if module.is_empty() {
            bail!("go.mod require key path must be require.<module path>");
        }
        return Ok(GoModKey::Require(module));
    }
    bail!("go.mod key path must be 'comment' or 'require.<module path>'");
}

fn extract_gomod_value(content: &str, key_path: &str) -> Result<String> {
    match parse_gomod_key_path(key_path)? {
        GoModKey::Comment => extract_gomod_comment_version(content),
        GoModKey::Require(module) => extract_gomod_require_version(content, module),
    }
}

fn write_gomod_value(path: &str, content: &str, key_path: &str, new_value: &str) -> Result<()> {
    let updated = match parse_gomod_key_path(key_path)? {
        GoModKey::Comment => write_gomod_comment_version(content, new_value)?,
        GoModKey::Require(module) => write_gomod_require_version(content, module, new_value)?,
    };
    fs::write(path, updated).with_context(|| format!("failed to write {}", path))?;
    Ok(())
}

fn extract_gomod_comment_version(content: &str) -> Result<String> {
    content
        .lines()
        .find_map(parse_gomod_comment_line)
        .ok_or_else(|| {
            anyhow!(
                "go.mod has no // version comment; add '// version 1.2.3' after the module line or use require.<module>"
            )
        })
}

fn parse_gomod_comment_line(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if !trimmed.starts_with("//") {
        return None;
    }
    let rest = trimmed[2..].trim();
    let (keyword, version) = rest.split_once(|character: char| character == ':' || character.is_whitespace())?;
    if !keyword.eq_ignore_ascii_case("version") {
        return None;
    }
    let version = version.trim();
    if version.is_empty() {
        return None;
    }
    Some(version.to_string())
}

fn write_gomod_comment_version(content: &str, new_value: &str) -> Result<String> {
    let mut lines: Vec<String> = content.lines().map(ToOwned::to_owned).collect();
    let had_trailing_newline = content.ends_with('\n');
    for line in &mut lines {
        if parse_gomod_comment_line(line).is_some() {
            *line = format!("// version {new_value}");
            return join_gomod_lines(lines, had_trailing_newline);
        }
    }

    let insert_at = lines
        .iter()
        .position(|line| line.trim_start().starts_with("module "))
        .map(|index| index + 1)
        .unwrap_or(0);
    lines.insert(insert_at, format!("// version {new_value}"));
    join_gomod_lines(lines, had_trailing_newline)
}

fn join_gomod_lines(lines: Vec<String>, had_trailing_newline: bool) -> Result<String> {
    let mut rendered = lines.join("\n");
    if had_trailing_newline {
        rendered.push('\n');
    }
    Ok(rendered)
}

fn split_gomod_require_entry(line: &str) -> Option<(&str, &str)> {
    let trimmed = line.trim().trim_end_matches(',');
    if trimmed.is_empty() || trimmed.starts_with("//") {
        return None;
    }
    let mut parts = trimmed.split_whitespace();
    let module = parts.next()?;
    let version = parts.next()?;
    Some((module, version))
}

fn extract_gomod_require_version(content: &str, module: &str) -> Result<String> {
    let mut in_block = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("require ") && !trimmed.starts_with("require (") {
            if let Some(require_line) = trimmed.strip_prefix("require ")
                && let Some((entry_module, version)) = split_gomod_require_entry(require_line.trim())
                && entry_module == module
            {
                return Ok(version.to_string());
            }
            continue;
        }
        if trimmed == "require (" {
            in_block = true;
            continue;
        }
        if in_block {
            if trimmed == ")" {
                in_block = false;
                continue;
            }
            if let Some((entry_module, version)) = split_gomod_require_entry(trimmed)
                && entry_module == module
            {
                return Ok(version.to_string());
            }
        }
    }
    Err(anyhow!("missing require entry for module '{}'", module))
}

fn write_gomod_require_version(content: &str, module: &str, new_value: &str) -> Result<String> {
    let mut updated = String::new();
    let mut in_block = false;
    let mut replaced = false;

    for line in content.lines() {
        if !updated.is_empty() {
            updated.push('\n');
        }
        let trimmed = line.trim();
        if trimmed.starts_with("require ")
            && !trimmed.starts_with("require (")
            && let Some((entry_module, version)) =
                split_gomod_require_entry(trimmed.strip_prefix("require ").unwrap_or("").trim())
            && entry_module == module
        {
            updated.push_str(&line.replace(version, new_value));
            replaced = true;
            continue;
        }
        if trimmed == "require (" {
            in_block = true;
            updated.push_str(line);
            continue;
        }
        if in_block {
            if let Some((entry_module, version)) = split_gomod_require_entry(trimmed)
                && entry_module == module
            {
                updated.push_str(&line.replace(version, new_value));
                replaced = true;
                continue;
            }
            if trimmed == ")" {
                in_block = false;
            }
        }
        updated.push_str(line);
    }

    if replaced {
        if content.ends_with('\n') {
            updated.push('\n');
        }
        Ok(updated)
    } else {
        Err(anyhow!("missing require entry for module '{}'", module))
    }
}

fn extract_ruby_value(path: &str, content: &str, key_path: &str) -> Result<String> {
    let key_path = key_path.trim();
    if let Some(gem_name) = key_path.strip_prefix("gem.") {
        let gem_name = gem_name.trim();
        if gem_name.is_empty() {
            bail!("Ruby gem key path must be gem.<name>");
        }
        return extract_ruby_gem_version(content, gem_name);
    }
    if !key_path.is_empty()
        && key_path != "version"
        && key_path != "@"
        && key_path != "."
    {
        bail!("Ruby key path must be 'version' or 'gem.<name>'");
    }
    if path.to_ascii_lowercase().ends_with(".gemspec") {
        extract_gemspec_version(content)
    } else {
        extract_gemfile_version(content)
    }
}

fn write_ruby_value(path: &str, content: &str, key_path: &str, new_value: &str) -> Result<()> {
    let updated = if let Some(gem_name) = key_path.trim().strip_prefix("gem.") {
        let gem_name = gem_name.trim();
        if gem_name.is_empty() {
            bail!("Ruby gem key path must be gem.<name>");
        }
        write_ruby_gem_version(content, gem_name, new_value)?
    } else if path.to_ascii_lowercase().ends_with(".gemspec") {
        write_gemspec_version(content, new_value)?
    } else {
        write_gemfile_version(content, new_value)?
    };
    fs::write(path, updated).with_context(|| format!("failed to write {}", path))?;
    Ok(())
}

fn extract_gemspec_version(content: &str) -> Result<String> {
    for line in content.lines() {
        if let Some(version) = parse_gemspec_version_line(line) {
            return Ok(version);
        }
    }
    Err(anyhow!("gemspec does not contain s.version = '...'"))
}

fn write_gemspec_version(content: &str, new_value: &str) -> Result<String> {
    let mut updated = String::new();
    let mut replaced = false;
    for line in content.lines() {
        if !updated.is_empty() {
            updated.push('\n');
        }
        if !replaced && parse_gemspec_version_line(line).is_some() {
            updated.push_str(&replace_gemspec_version_line(line, new_value));
            replaced = true;
        } else {
            updated.push_str(line);
        }
    }
    if replaced {
        if content.ends_with('\n') {
            updated.push('\n');
        }
        Ok(updated)
    } else {
        Err(anyhow!("gemspec does not contain s.version = '...'"))
    }
}

fn extract_gemfile_version(content: &str) -> Result<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(version) = parse_ruby_assignment_version(trimmed, "VERSION") {
            return Ok(version);
        }
        if let Some(version) = parse_ruby_assignment_version(trimmed, "version") {
            return Ok(version);
        }
    }
    Err(anyhow!(
        "Gemfile does not contain VERSION = '...' or version = '...'; use a .gemspec target or gem.<name>"
    ))
}

fn write_gemfile_version(content: &str, new_value: &str) -> Result<String> {
    let mut updated = String::new();
    let mut replaced = false;
    for line in content.lines() {
        if !updated.is_empty() {
            updated.push('\n');
        }
        let trimmed = line.trim();
        if !replaced && parse_ruby_assignment_version(trimmed, "VERSION").is_some() {
            updated.push_str(&replace_ruby_assignment_version(line, "VERSION", new_value));
            replaced = true;
        } else if !replaced && parse_ruby_assignment_version(trimmed, "version").is_some() {
            updated.push_str(&replace_ruby_assignment_version(line, "version", new_value));
            replaced = true;
        } else {
            updated.push_str(line);
        }
    }
    if replaced {
        if content.ends_with('\n') {
            updated.push('\n');
        }
        Ok(updated)
    } else {
        Err(anyhow!("Gemfile does not contain VERSION = '...' or version = '...'"))
    }
}

fn parse_ruby_assignment_version(line: &str, name: &str) -> Option<String> {
    let rest = line.strip_prefix(name)?.trim();
    let rest = rest.strip_prefix('=')?.trim();
    parse_ruby_quoted_value(rest)
}

fn parse_ruby_quoted_value(token: &str) -> Option<String> {
    let token = token.trim();
    if (token.starts_with('\'') && token.ends_with('\'')) || (token.starts_with('"') && token.ends_with('"')) {
        let inner = &token[1..token.len() - 1];
        if inner.is_empty() {
            return None;
        }
        return Some(inner.to_string());
    }
    None
}

fn replace_ruby_assignment_version(line: &str, name: &str, new_value: &str) -> String {
    if line.contains('\'') {
        return format!("  {name} = '{new_value}'");
    }
    format!("  {name} = \"{new_value}\"")
}

fn extract_ruby_gem_version(content: &str, gem_name: &str) -> Result<String> {
    for line in content.lines() {
        if let Some(version) = parse_ruby_gem_line_version(line, gem_name) {
            return Ok(version);
        }
    }
    Err(anyhow!("missing gem entry for '{}'", gem_name))
}

fn parse_gemspec_version_line(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let marker = ".version";
    let idx = trimmed.find(marker)?;
    let rest = trimmed[idx + marker.len()..].trim();
    let rest = rest.strip_prefix('=')?.trim();
    parse_ruby_quoted_value(rest)
}

fn replace_gemspec_version_line(line: &str, new_value: &str) -> String {
    let indent: String = line.chars().take_while(|ch| ch.is_whitespace()).collect();
    let quote = if line.contains('\'') { '\'' } else { '"' };
    format!("{indent}s.version = {quote}{new_value}{quote}")
}

fn parse_ruby_quoted_value_with_remainder(input: &str) -> Option<(String, &str)> {
    let input = input.trim();
    let quote = input.chars().next()?;
    if quote != '\'' && quote != '"' {
        return None;
    }
    let end = input[1..].find(quote)?;
    let value = input[1..1 + end].to_string();
    Some((value, input[1 + end + 1..].trim_start()))
}

fn parse_ruby_gem_line_version(line: &str, gem_name: &str) -> Option<String> {
    let trimmed = line.trim();
    if !trimmed.starts_with("gem ") {
        return None;
    }
    let rest = trimmed.strip_prefix("gem ")?.trim();
    let (name, remainder) = parse_ruby_quoted_value_with_remainder(rest)?;
    if name != gem_name {
        return None;
    }
    let remainder = remainder.strip_prefix(',').unwrap_or(remainder).trim();
    parse_ruby_quoted_value(remainder)
}

fn write_ruby_gem_version(content: &str, gem_name: &str, new_value: &str) -> Result<String> {
    let mut updated = String::new();
    let mut replaced = false;
    for line in content.lines() {
        if !updated.is_empty() {
            updated.push('\n');
        }
        if !replaced && parse_ruby_gem_line_version(line, gem_name).is_some() {
            let quote = if line.contains('\'') { '\'' } else { '"' };
            updated.push_str(&format!("gem '{gem_name}', {quote}{new_value}{quote}"));
            replaced = true;
        } else {
            updated.push_str(line);
        }
    }
    if replaced {
        if content.ends_with('\n') {
            updated.push('\n');
        }
        Ok(updated)
    } else {
        Err(anyhow!("missing gem entry for '{}'", gem_name))
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
    fn gomod_comment_version_round_trip() {
        let content = "module example.com/demo\n\ngo 1.22\n// version 1.2.3\n";
        let dir = std::env::temp_dir().join(format!("comfygit-gomod-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join("go.mod");
        fs::write(&path, content).expect("write go.mod");

        let read = extract_gomod_value(content, "comment").expect("read");
        assert_eq!(read, "1.2.3");

        write_gomod_value(path.to_str().expect("path"), content, "comment", "2.0.0").expect("write");
        let updated = fs::read_to_string(&path).expect("read back");
        assert!(updated.contains("// version 2.0.0"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn gemspec_version_round_trip() {
        let content = "Gem::Specification.new do |s|\n  s.name = 'demo'\n  s.version = '1.2.3'\nend\n";
        let dir = std::env::temp_dir().join(format!("comfygit-gemspec-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join("demo.gemspec");
        fs::write(&path, content).expect("write gemspec");

        let read = extract_ruby_value("demo.gemspec", content, "version").expect("read");
        assert_eq!(read, "1.2.3");

        write_ruby_value(path.to_str().expect("path"), content, "version", "2.0.0").expect("write");
        let updated = fs::read_to_string(&path).expect("read back");
        assert!(updated.contains("s.version = '2.0.0'"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn csproj_version_round_trip() {
        let content = r#"<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <Version>1.2.3</Version>
  </PropertyGroup>
</Project>
"#;
        let dir = std::env::temp_dir().join(format!("comfygit-csproj-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join("App.csproj");
        fs::write(&path, content).expect("write csproj");

        let read = extract_xml_value(content, "PropertyGroup.Version").expect("read");
        assert_eq!(read, "1.2.3");

        write_xml_value(
            path.to_str().expect("path"),
            content,
            "PropertyGroup.Version",
            "2.0.0",
        )
        .expect("write");
        let updated = fs::read_to_string(&path).expect("read back");
        assert!(updated.contains(">2.0.0<"));

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
