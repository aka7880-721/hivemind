// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit SA-PSLicense
//
// For details, see the LICENSE file in the repository root.

//! Custom parsers for build manifests that are not plain JSON/TOML/YAML/XML.

use std::path::Path;

use anyhow::{Result, anyhow, bail};

pub(crate) fn is_description_filename(path: &str) -> bool {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("DESCRIPTION"))
}

pub(crate) fn is_cmake_filename(path: &str) -> bool {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("CMakeLists.txt"))
}

pub(crate) fn is_makefile_filename(path: &str) -> bool {
    let file_name = Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path);
    matches!(
        file_name.to_ascii_lowercase().as_str(),
        "makefile" | "gnumakefile"
    )
}

pub(crate) fn is_gradle_filename(path: &str) -> bool {
    let file_name = Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path)
        .to_ascii_lowercase();
    file_name == "build.gradle"
        || file_name == "build.gradle.kts"
        || file_name == "settings.gradle"
        || file_name == "settings.gradle.kts"
}

pub(crate) fn is_plist_filename(path: &str) -> bool {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.to_ascii_lowercase().ends_with(".plist"))
        .unwrap_or(false)
}

pub(crate) fn is_project_clj_filename(path: &str) -> bool {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("project.clj"))
}

pub(crate) fn extract_description_value(content: &str, key_path: &str) -> Result<String> {
    let field = normalize_description_field(key_path)?;
    for line in content.lines() {
        if let Some(value) = parse_description_field_line(line, field) {
            return Ok(value);
        }
    }
    Err(anyhow!("missing DESCRIPTION field '{}'", field))
}

pub(crate) fn write_description_value(content: &str, key_path: &str, new_value: &str) -> Result<String> {
    let field = normalize_description_field(key_path)?;
    let mut updated = String::new();
    let mut replaced = false;
    for line in content.lines() {
        if !updated.is_empty() {
            updated.push('\n');
        }
        if !replaced && parse_description_field_line(line, field).is_some() {
            updated.push_str(&format!("{field}: {new_value}"));
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
        Err(anyhow!("missing DESCRIPTION field '{}'", field))
    }
}

fn normalize_description_field(key_path: &str) -> Result<&'static str> {
    let key_path = key_path.trim();
    if key_path.is_empty() || key_path == "@" || key_path == "." || key_path.eq_ignore_ascii_case("version") {
        return Ok("Version");
    }
    if key_path == "Version" {
        return Ok("Version");
    }
    bail!("DESCRIPTION key path must be 'Version' (R package version field)");
}

fn parse_description_field_line(line: &str, field: &str) -> Option<String> {
    let trimmed = line.trim();
    let (name, value) = trimmed.split_once(':')?;
    if !name.trim().eq_ignore_ascii_case(field) {
        return None;
    }
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    Some(value.to_string())
}

pub(crate) fn extract_cmake_value(content: &str, key_path: &str) -> Result<String> {
    let key_path = key_path.trim();
    if key_path.is_empty() || key_path == "@" || key_path == "." || key_path == "project" {
        return extract_cmake_project_version(content);
    }
    extract_cmake_set_variable(content, key_path)
}

pub(crate) fn write_cmake_value(content: &str, key_path: &str, new_value: &str) -> Result<String> {
    let key_path = key_path.trim();
    if key_path.is_empty() || key_path == "@" || key_path == "." || key_path == "project" {
        return write_cmake_project_version(content, new_value);
    }
    write_cmake_set_variable(content, key_path, new_value)
}

fn extract_cmake_project_version(content: &str) -> Result<String> {
    for fragment in content.split("project(").skip(1) {
        if let Some(version) = cmake_token_after_keyword(fragment, "VERSION") {
            return Ok(version);
        }
    }
    Err(anyhow!(
        "CMakeLists.txt does not contain project(... VERSION ...) — use key 'project' or a set() variable name"
    ))
}

fn write_cmake_project_version(content: &str, new_value: &str) -> Result<String> {
    let mut updated = String::new();
    let mut replaced = false;
    for line in content.lines() {
        if !updated.is_empty() {
            updated.push('\n');
        }
        if !replaced
            && line.contains("project(")
            && let Some(old) = cmake_token_after_keyword(line, "VERSION")
        {
            updated.push_str(&line.replace(&old, new_value));
            replaced = true;
            continue;
        }
        updated.push_str(line);
    }
    if replaced {
        if content.ends_with('\n') {
            updated.push('\n');
        }
        Ok(updated)
    } else {
        Err(anyhow!("CMakeLists.txt does not contain project(... VERSION ...)"))
    }
}

fn cmake_token_after_keyword(fragment: &str, keyword: &str) -> Option<String> {
    let lower = fragment.to_ascii_lowercase();
    let keyword_lower = keyword.to_ascii_lowercase();
    let index = lower.find(&keyword_lower)?;
    let after = fragment[index + keyword.len()..].trim_start();
    parse_cmake_version_token(after)
}

fn parse_cmake_version_token(token: &str) -> Option<String> {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return None;
    }
    let end = trimmed
        .find(|character: char| character.is_whitespace() || matches!(character, ')' | ','))
        .unwrap_or(trimmed.len());
    let version = trimmed[..end].trim_matches('"').trim();
    if version.is_empty() {
        return None;
    }
    Some(version.to_string())
}

fn extract_cmake_set_variable(content: &str, variable: &str) -> Result<String> {
    let marker = format!("set({variable}");
    let marker_alt = format!("set({variable} ");
    for line in content.lines() {
        let trimmed = line.trim();
        if (trimmed.starts_with(&marker) || trimmed.starts_with(&marker_alt))
            && let Some(version) = cmake_set_line_value(trimmed, variable)
        {
            return Ok(version);
        }
    }
    Err(anyhow!("missing CMake set({}) statement", variable))
}

fn cmake_set_line_value(line: &str, variable: &str) -> Option<String> {
    let open = format!("set({variable}");
    let rest = line.strip_prefix(&open)?.trim();
    let rest = rest.strip_prefix(')').unwrap_or(rest).trim();
    let rest = rest.strip_prefix(' ').unwrap_or(rest);
    parse_cmake_version_token(rest)
}

fn write_cmake_set_variable(content: &str, variable: &str, new_value: &str) -> Result<String> {
    let marker = format!("set({variable}");
    let mut updated = String::new();
    let mut replaced = false;
    for line in content.lines() {
        if !updated.is_empty() {
            updated.push('\n');
        }
        let trimmed = line.trim();
        if !replaced && trimmed.starts_with(&marker) {
            let indent: String = line.chars().take_while(|ch| ch.is_whitespace()).collect();
            updated.push_str(&format!("{indent}set({variable} {new_value})"));
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
        Err(anyhow!("missing CMake set({}) statement", variable))
    }
}

pub(crate) fn extract_makefile_value(content: &str, key_path: &str) -> Result<String> {
    let variable = normalize_makefile_variable(key_path)?;
    for line in content.lines() {
        if let Some(value) = parse_makefile_assignment(line, variable) {
            return Ok(value);
        }
    }
    Err(anyhow!("Makefile does not define {}", variable))
}

pub(crate) fn write_makefile_value(content: &str, key_path: &str, new_value: &str) -> Result<String> {
    let variable = normalize_makefile_variable(key_path)?;
    let mut updated = String::new();
    let mut replaced = false;
    for line in content.lines() {
        if !updated.is_empty() {
            updated.push('\n');
        }
        if !replaced && parse_makefile_assignment(line, variable).is_some() {
            let indent: String = line.chars().take_while(|ch| ch.is_whitespace()).collect();
            if line.contains(":=") {
                updated.push_str(&format!("{indent}{variable} := {new_value}"));
            } else {
                updated.push_str(&format!("{indent}{variable} = {new_value}"));
            }
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
        Err(anyhow!("Makefile does not define {}", variable))
    }
}

fn normalize_makefile_variable(key_path: &str) -> Result<&'static str> {
    let key_path = key_path.trim();
    if key_path.is_empty() || key_path == "@" || key_path == "." || key_path.eq_ignore_ascii_case("version") {
        return Ok("VERSION");
    }
    if key_path == "VERSION" || key_path == "version" {
        return Ok("VERSION");
    }
    bail!("Makefile key path must be 'VERSION'");
}

fn parse_makefile_assignment(line: &str, variable: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.starts_with('#') {
        return None;
    }
    let (name, value) = if let Some((name, value)) = trimmed.split_once(":=") {
        (name.trim(), value.trim())
    } else if let Some((name, value)) = trimmed.split_once('=') {
        (name.trim(), value.trim())
    } else {
        return None;
    };
    if name != variable {
        return None;
    }
    let value = value.trim().trim_end_matches('\\').trim();
    if value.is_empty() {
        return None;
    }
    Some(value.to_string())
}

pub(crate) fn extract_gradle_value(content: &str, key_path: &str) -> Result<String> {
    let key = normalize_gradle_key(key_path)?;
    for line in content.lines() {
        if let Some(value) = parse_gradle_assignment(line, key) {
            return Ok(value);
        }
    }
    Err(anyhow!("Gradle file does not define {}", key))
}

pub(crate) fn write_gradle_value(content: &str, key_path: &str, new_value: &str) -> Result<String> {
    let key = normalize_gradle_key(key_path)?;
    let mut updated = String::new();
    let mut replaced = false;
    for line in content.lines() {
        if !updated.is_empty() {
            updated.push('\n');
        }
        if !replaced && parse_gradle_assignment(line, key).is_some() {
            updated.push_str(&replace_gradle_assignment_line(line, key, new_value));
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
        Err(anyhow!("Gradle file does not define {}", key))
    }
}

fn normalize_gradle_key(key_path: &str) -> Result<&'static str> {
    let key_path = key_path.trim();
    if key_path.is_empty() || key_path == "@" || key_path == "." || key_path == "version" {
        return Ok("version");
    }
    if key_path == "versionName" {
        return Ok("versionName");
    }
    if key_path == "versionCode" {
        return Ok("versionCode");
    }
    bail!("Gradle key path must be 'version', 'versionName', or 'versionCode'");
}

fn parse_gradle_assignment(line: &str, key: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.starts_with("//") || trimmed.starts_with("/*") {
        return None;
    }
    let trimmed = trimmed
        .strip_prefix("val ")
        .unwrap_or(trimmed)
        .strip_prefix("var ")
        .unwrap_or(trimmed)
        .trim();
    let (name, remainder) = if let Some((name, remainder)) = trimmed.split_once('=') {
        (name.trim(), remainder.trim())
    } else if let Some((name, remainder)) = trimmed.split_once(' ') {
        (name.trim(), remainder.trim())
    } else {
        return None;
    };
    if name != key {
        return None;
    }
    parse_quoted_or_bare_token(remainder)
}

fn replace_gradle_assignment_line(line: &str, key: &str, new_value: &str) -> String {
    let indent: String = line.chars().take_while(|ch| ch.is_whitespace()).collect();
    let trimmed = line.trim();
    let val_prefix = if trimmed.starts_with("val ") {
        "val "
    } else if trimmed.starts_with("var ") {
        "var "
    } else {
        ""
    };
    let uses_equals = trimmed.contains('=');
    let quote = if line.contains('\'') { '\'' } else { '"' };
    if uses_equals {
        format!("{indent}{val_prefix}{key} = {quote}{new_value}{quote}")
    } else {
        format!("{indent}{val_prefix}{key} {quote}{new_value}{quote}")
    }
}

fn parse_quoted_or_bare_token(token: &str) -> Option<String> {
    let token = token.trim().trim_end_matches(',');
    if (token.starts_with('"') && token.ends_with('"')) || (token.starts_with('\'') && token.ends_with('\'')) {
        return Some(token[1..token.len() - 1].to_string());
    }
    if token.chars().all(|ch| ch.is_ascii_digit() || ch == '.') {
        return Some(token.to_string());
    }
    None
}

pub(crate) fn extract_plist_value(content: &str, key_path: &str) -> Result<String> {
    let key = normalize_plist_key(key_path)?;
    plist_value_for_key(content, key).ok_or_else(|| anyhow!("plist does not contain key '{}'", key))
}

pub(crate) fn write_plist_value(content: &str, key_path: &str, new_value: &str) -> Result<String> {
    let key = normalize_plist_key(key_path)?;
    let marker = format!("<key>{key}</key>");
    let mut updated = String::new();
    let mut replaced = false;
    let mut after_key = false;
    for line in content.lines() {
        if !updated.is_empty() {
            updated.push('\n');
        }
        if after_key {
            if parse_plist_string_line(line).is_some() {
                let indent: String = line.chars().take_while(|ch| ch.is_whitespace()).collect();
                updated.push_str(&format!("{indent}<string>{new_value}</string>"));
                replaced = true;
                after_key = false;
                continue;
            }
            after_key = false;
        }
        updated.push_str(line);
        if line.contains(&marker) {
            after_key = true;
        }
    }
    if replaced {
        if content.ends_with('\n') {
            updated.push('\n');
        }
        Ok(updated)
    } else {
        Err(anyhow!("plist does not contain editable key '{}'", key))
    }
}

fn normalize_plist_key(key_path: &str) -> Result<&'static str> {
    let key_path = key_path.trim();
    if key_path.is_empty()
        || key_path == "@"
        || key_path == "."
        || key_path == "CFBundleShortVersionString"
    {
        return Ok("CFBundleShortVersionString");
    }
    if key_path == "CFBundleVersion" {
        return Ok("CFBundleVersion");
    }
    bail!("plist key path must be 'CFBundleShortVersionString' or 'CFBundleVersion'");
}

fn plist_value_for_key(content: &str, key: &str) -> Option<String> {
    let marker = format!("<key>{key}</key>");
    let mut lines = content.lines();
    while let Some(line) = lines.next() {
        if line.contains(&marker) {
            for next in lines.by_ref() {
                if let Some(value) = parse_plist_string_line(next) {
                    return Some(value);
                }
                if next.contains("<key>") {
                    break;
                }
            }
        }
    }
    None
}

fn parse_plist_string_line(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let inner = trimmed.strip_prefix("<string>")?.strip_suffix("</string>")?;
    if inner.is_empty() {
        return None;
    }
    Some(inner.to_string())
}

pub(crate) fn extract_clojure_value(content: &str, key_path: &str) -> Result<String> {
    let key_path = key_path.trim();
    if key_path.is_empty() || key_path == "@" || key_path == "." || key_path == "defproject" {
        return extract_clojure_defproject_version(content);
    }
    if key_path == "version" || key_path == ":version" {
        return extract_clojure_keyword_version(content);
    }
    bail!("project.clj key path must be 'defproject' or 'version'");
}

pub(crate) fn write_clojure_value(content: &str, key_path: &str, new_value: &str) -> Result<String> {
    let key_path = key_path.trim();
    if key_path.is_empty() || key_path == "@" || key_path == "." || key_path == "defproject" {
        return write_clojure_defproject_version(content, new_value);
    }
    if key_path == "version" || key_path == ":version" {
        return write_clojure_keyword_version(content, new_value);
    }
    bail!("project.clj key path must be 'defproject' or 'version'");
}

fn extract_clojure_defproject_version(content: &str) -> Result<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("(defproject ")
            && let Some(version) = defproject_version_string(trimmed)
        {
            return Ok(version);
        }
    }
    Err(anyhow!("project.clj does not contain (defproject name \"version\" ...)"))
}

fn write_clojure_defproject_version(content: &str, new_value: &str) -> Result<String> {
    let mut updated = String::new();
    let mut replaced = false;
    for line in content.lines() {
        if !updated.is_empty() {
            updated.push('\n');
        }
        let trimmed = line.trim();
        if !replaced
            && trimmed.starts_with("(defproject ")
            && let Some(old) = defproject_version_string(trimmed)
        {
            updated.push_str(&line.replace(&format!("\"{old}\""), &format!("\"{new_value}\"")));
            replaced = true;
            continue;
        }
        updated.push_str(line);
    }
    if replaced {
        if content.ends_with('\n') {
            updated.push('\n');
        }
        Ok(updated)
    } else {
        Err(anyhow!("project.clj does not contain (defproject name \"version\" ...)"))
    }
}

fn extract_clojure_keyword_version(content: &str) -> Result<String> {
    for line in content.lines() {
        if let Some(version) = parse_clojure_keyword_line(line, "version") {
            return Ok(version);
        }
    }
    Err(anyhow!("project.clj does not contain :version \"...\""))
}

fn write_clojure_keyword_version(content: &str, new_value: &str) -> Result<String> {
    let mut updated = String::new();
    let mut replaced = false;
    for line in content.lines() {
        if !updated.is_empty() {
            updated.push('\n');
        }
        if !replaced && parse_clojure_keyword_line(line, "version").is_some() {
            let indent: String = line.chars().take_while(|ch| ch.is_whitespace()).collect();
            updated.push_str(&format!("{indent}:version \"{new_value}\""));
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
        Err(anyhow!("project.clj does not contain :version \"...\""))
    }
}

fn parse_clojure_keyword_line(line: &str, keyword: &str) -> Option<String> {
    let marker = format!(":{keyword}");
    if !line.contains(&marker) {
        return None;
    }
    let after = line.split(&marker).nth(1)?.trim();
    first_quoted_string(after)
}

fn defproject_version_string(input: &str) -> Option<String> {
    let mut quoted = Vec::new();
    let mut rest = input;
    while let Some(value) = first_quoted_string(rest) {
        quoted.push(value);
        let marker = format!("\"{}\"", quoted.last()?);
        rest = rest.split_once(&marker).map(|(_, remainder)| remainder).unwrap_or("");
    }
    if quoted.len() >= 2 {
        quoted.pop()
    } else {
        quoted.first().cloned()
    }
}

fn first_quoted_string(input: &str) -> Option<String> {
    let start = input.find('"')? + 1;
    let end = input[start..].find('"')? + start;
    let value = &input[start..end];
    if value.is_empty() {
        return None;
    }
    Some(value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn description_version_round_trip() {
        let content = "Package: demo\nVersion: 1.2.3\nTitle: Demo\n";
        let read = extract_description_value(content, "Version").expect("read");
        assert_eq!(read, "1.2.3");
        let updated = write_description_value(content, "Version", "2.0.0").expect("write");
        assert!(updated.contains("Version: 2.0.0"));
    }

    #[test]
    fn cmake_project_version_round_trip() {
        let content = "cmake_minimum_required(VERSION 3.16)\nproject(demo VERSION 1.2.3)\n";
        let read = extract_cmake_value(content, "project").expect("read");
        assert_eq!(read, "1.2.3");
        let updated = write_cmake_value(content, "project", "2.0.0").expect("write");
        assert!(updated.contains("VERSION 2.0.0"));
    }

    #[test]
    fn makefile_version_round_trip() {
        let content = "VERSION := 1.2.3\nall:\n\t@echo $(VERSION)\n";
        let read = extract_makefile_value(content, "VERSION").expect("read");
        assert_eq!(read, "1.2.3");
        let updated = write_makefile_value(content, "VERSION", "2.0.0").expect("write");
        assert!(updated.contains("VERSION := 2.0.0"));
    }

    #[test]
    fn gradle_version_round_trip() {
        let content = "plugins { id(\"java\") }\nversion = \"1.2.3\"\n";
        let read = extract_gradle_value(content, "version").expect("read");
        assert_eq!(read, "1.2.3");
        let updated = write_gradle_value(content, "version", "2.0.0").expect("write");
        assert!(updated.contains("version = \"2.0.0\""));
    }

    #[test]
    fn plist_version_round_trip() {
        let content = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleShortVersionString</key>
<string>1.2.3</string>
</dict></plist>
"#;
        let read = extract_plist_value(content, "CFBundleShortVersionString").expect("read");
        assert_eq!(read, "1.2.3");
        let updated =
            write_plist_value(content, "CFBundleShortVersionString", "2.0.0").expect("write");
        assert!(updated.contains("<string>2.0.0</string>"));
    }

    #[test]
    fn clojure_defproject_round_trip() {
        let content = "(defproject demo \"1.2.3\"\n  :description \"demo\")\n";
        let read = extract_clojure_value(content, "defproject").expect("read");
        assert_eq!(read, "1.2.3");
        let updated = write_clojure_value(content, "defproject", "2.0.0").expect("write");
        assert!(updated.contains("\"2.0.0\""));
    }
}
