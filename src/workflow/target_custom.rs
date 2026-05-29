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

pub(crate) fn write_description_value(
    content: &str,
    key_path: &str,
    new_value: &str,
) -> Result<String> {
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
    if key_path.is_empty()
        || key_path == "@"
        || key_path == "."
        || key_path.eq_ignore_ascii_case("version")
    {
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
        Err(anyhow!(
            "CMakeLists.txt does not contain project(... VERSION ...)"
        ))
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

pub(crate) fn write_makefile_value(
    content: &str,
    key_path: &str,
    new_value: &str,
) -> Result<String> {
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
    if key_path.is_empty()
        || key_path == "@"
        || key_path == "."
        || key_path.eq_ignore_ascii_case("version")
    {
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
    if (token.starts_with('"') && token.ends_with('"'))
        || (token.starts_with('\'') && token.ends_with('\''))
    {
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
    let inner = trimmed
        .strip_prefix("<string>")?
        .strip_suffix("</string>")?;
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

pub(crate) fn write_clojure_value(
    content: &str,
    key_path: &str,
    new_value: &str,
) -> Result<String> {
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
    Err(anyhow!(
        "project.clj does not contain (defproject name \"version\" ...)"
    ))
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
        Err(anyhow!(
            "project.clj does not contain (defproject name \"version\" ...)"
        ))
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
        rest = rest
            .split_once(&marker)
            .map(|(_, remainder)| remainder)
            .unwrap_or("");
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

// --- Part 4: Swift, Elixir, Scala, Cabal, Autoconf ---

pub(crate) fn is_package_swift_filename(path: &str) -> bool {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("Package.swift"))
}

pub(crate) fn is_mix_exs_filename(path: &str) -> bool {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("mix.exs"))
}

pub(crate) fn is_build_sbt_filename(path: &str) -> bool {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("build.sbt"))
}

pub(crate) fn is_cabal_filename(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("cabal"))
}

pub(crate) fn is_configure_ac_filename(path: &str) -> bool {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("configure.ac"))
}

pub(crate) fn extract_swift_package_value(content: &str, key_path: &str) -> Result<String> {
    let key_path = key_path.trim();
    if key_path.is_empty() || key_path == "@" || key_path == "." || key_path == "comment" {
        return extract_swift_comment_version(content);
    }
    if key_path == "version" || key_path == "packageVersion" {
        return extract_swift_let_version(content, key_path);
    }
    bail!("Package.swift key path must be 'version', 'packageVersion', or 'comment'");
}

pub(crate) fn write_swift_package_value(
    content: &str,
    key_path: &str,
    new_value: &str,
) -> Result<String> {
    let key_path = key_path.trim();
    if key_path.is_empty() || key_path == "@" || key_path == "." || key_path == "comment" {
        return write_swift_comment_version(content, new_value);
    }
    if key_path == "version" || key_path == "packageVersion" {
        return write_swift_let_version(content, key_path, new_value);
    }
    bail!("Package.swift key path must be 'version', 'packageVersion', or 'comment'");
}

fn extract_swift_comment_version(content: &str) -> Result<String> {
    content
        .lines()
        .find_map(parse_swift_comment_line)
        .ok_or_else(|| anyhow!("Package.swift has no // version comment"))
}

fn parse_swift_comment_line(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if !trimmed.starts_with("//") {
        return None;
    }
    let rest = trimmed[2..].trim();
    let (keyword, version) = rest.split_once(|c: char| c == ':' || c.is_whitespace())?;
    if !keyword.eq_ignore_ascii_case("version") {
        return None;
    }
    let version = version.trim();
    if version.is_empty() {
        return None;
    }
    Some(version.to_string())
}

fn write_swift_comment_version(content: &str, new_value: &str) -> Result<String> {
    let mut lines: Vec<String> = content.lines().map(ToOwned::to_owned).collect();
    let had_trailing_newline = content.ends_with('\n');
    for line in &mut lines {
        if parse_swift_comment_line(line).is_some() {
            *line = format!("// version: {new_value}");
            return join_lines(lines, had_trailing_newline);
        }
    }
    lines.insert(0, format!("// version: {new_value}"));
    join_lines(lines, had_trailing_newline)
}

fn extract_swift_let_version(content: &str, variable: &str) -> Result<String> {
    for line in content.lines() {
        if let Some(version) = parse_swift_let_line(line, variable) {
            return Ok(version);
        }
    }
    Err(anyhow!(
        "Package.swift does not define let {variable} = \"...\""
    ))
}

fn parse_swift_let_line(line: &str, variable: &str) -> Option<String> {
    let trimmed = line.trim();
    let prefix = format!("let {variable}");
    if !trimmed.starts_with(&prefix) {
        return None;
    }
    let rest = trimmed[prefix.len()..].trim().strip_prefix('=')?.trim();
    parse_quoted_or_bare_token(rest)
}

fn write_swift_let_version(content: &str, variable: &str, new_value: &str) -> Result<String> {
    let mut updated = String::new();
    let mut replaced = false;
    for line in content.lines() {
        if !updated.is_empty() {
            updated.push('\n');
        }
        if !replaced && parse_swift_let_line(line, variable).is_some() {
            let indent: String = line.chars().take_while(|ch| ch.is_whitespace()).collect();
            updated.push_str(&format!("{indent}let {variable} = \"{new_value}\""));
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
        Err(anyhow!(
            "Package.swift does not define let {variable} = \"...\""
        ))
    }
}

pub(crate) fn extract_elixir_mix_value(content: &str, key_path: &str) -> Result<String> {
    let field = normalize_elixir_field(key_path)?;
    for line in content.lines() {
        if let Some(value) = parse_elixir_field_line(line, field) {
            return Ok(value);
        }
    }
    Err(anyhow!("mix.exs does not contain {}: \"...\"", field))
}

pub(crate) fn write_elixir_mix_value(
    content: &str,
    key_path: &str,
    new_value: &str,
) -> Result<String> {
    let field = normalize_elixir_field(key_path)?;
    let mut updated = String::new();
    let mut replaced = false;
    for line in content.lines() {
        if !updated.is_empty() {
            updated.push('\n');
        }
        if !replaced && let Some(old) = parse_elixir_field_line(line, field) {
            updated.push_str(&line.replace(&format!("\"{old}\""), &format!("\"{new_value}\"")));
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
        Err(anyhow!("mix.exs does not contain {}: \"...\"", field))
    }
}

fn normalize_elixir_field(key_path: &str) -> Result<&'static str> {
    let key_path = key_path.trim();
    if key_path.is_empty() || key_path == "@" || key_path == "." || key_path == "version" {
        return Ok("version");
    }
    bail!("mix.exs key path must be 'version' (package version)");
}

fn parse_elixir_field_line(line: &str, field: &str) -> Option<String> {
    let trimmed = line.trim();
    let marker = format!("{field}:");
    let index = trimmed.find(&marker)?;
    let rest = trimmed[index + marker.len()..]
        .trim()
        .trim_start_matches(',')
        .trim()
        .trim_end_matches([',', ']']);
    first_quoted_string(rest).or_else(|| parse_quoted_or_bare_token(rest))
}

pub(crate) fn extract_scala_sbt_value(content: &str, key_path: &str) -> Result<String> {
    let key = normalize_sbt_key(key_path)?;
    for line in content.lines() {
        if let Some(value) = parse_sbt_version_line(line, key) {
            return Ok(value);
        }
    }
    Err(anyhow!("build.sbt does not contain {}", key))
}

pub(crate) fn write_scala_sbt_value(
    content: &str,
    key_path: &str,
    new_value: &str,
) -> Result<String> {
    let key = normalize_sbt_key(key_path)?;
    let mut updated = String::new();
    let mut replaced = false;
    for line in content.lines() {
        if !updated.is_empty() {
            updated.push('\n');
        }
        if !replaced && parse_sbt_version_line(line, key).is_some() {
            updated.push_str(&replace_sbt_version_line(line, key, new_value));
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
        Err(anyhow!("build.sbt does not contain {}", key))
    }
}

fn normalize_sbt_key(key_path: &str) -> Result<&'static str> {
    let key_path = key_path.trim();
    if key_path.is_empty() || key_path == "@" || key_path == "." || key_path == "version" {
        return Ok("version");
    }
    if key_path == "ThisBuild / version" || key_path == "ThisBuild/version" {
        return Ok("ThisBuild / version");
    }
    bail!("build.sbt key path must be 'version' or 'ThisBuild / version'");
}

fn parse_sbt_version_line(line: &str, key: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.starts_with("//") {
        return None;
    }
    let prefix = if key == "ThisBuild / version" {
        "ThisBuild / version"
    } else {
        "version"
    };
    if !trimmed.starts_with(prefix) {
        return None;
    }
    let rest = trimmed[prefix.len()..].trim();
    let rest = rest.strip_prefix(":=")?.trim();
    parse_quoted_or_bare_token(rest)
}

fn replace_sbt_version_line(line: &str, key: &str, new_value: &str) -> String {
    let indent: String = line.chars().take_while(|ch| ch.is_whitespace()).collect();
    if key == "ThisBuild / version" {
        format!("{indent}ThisBuild / version := \"{new_value}\"")
    } else {
        format!("{indent}version := \"{new_value}\"")
    }
}

pub(crate) fn extract_cabal_value(content: &str, key_path: &str) -> Result<String> {
    let field = normalize_cabal_field(key_path)?;
    for line in content.lines() {
        if let Some(value) = parse_cabal_field_line(line, field) {
            return Ok(value);
        }
    }
    Err(anyhow!("cabal file does not contain field '{}'", field))
}

pub(crate) fn write_cabal_value(content: &str, key_path: &str, new_value: &str) -> Result<String> {
    let field = normalize_cabal_field(key_path)?;
    let mut updated = String::new();
    let mut replaced = false;
    for line in content.lines() {
        if !updated.is_empty() {
            updated.push('\n');
        }
        if !replaced && parse_cabal_field_line(line, field).is_some() {
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
        Err(anyhow!("cabal file does not contain field '{}'", field))
    }
}

fn normalize_cabal_field(key_path: &str) -> Result<&'static str> {
    let key_path = key_path.trim();
    if key_path.is_empty() || key_path == "@" || key_path == "." || key_path == "version" {
        return Ok("version");
    }
    if key_path == "name" {
        return Ok("name");
    }
    bail!("cabal key path must be 'version' (package version)");
}

fn parse_cabal_field_line(line: &str, field: &str) -> Option<String> {
    if line.starts_with(char::is_whitespace) || line.trim_start().starts_with("--") {
        return None;
    }
    let (name, value) = line.split_once(':')?;
    if name.trim() != field {
        return None;
    }
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    Some(value.to_string())
}

pub(crate) fn extract_autoconf_value(content: &str, key_path: &str) -> Result<String> {
    let macro_name = normalize_autoconf_macro(key_path)?;
    for line in content.lines() {
        if let Some(version) = parse_ac_init_version(line, macro_name) {
            return Ok(version);
        }
    }
    Err(anyhow!(
        "configure.ac does not contain {} with a version argument",
        macro_name
    ))
}

pub(crate) fn write_autoconf_value(
    content: &str,
    key_path: &str,
    new_value: &str,
) -> Result<String> {
    let macro_name = normalize_autoconf_macro(key_path)?;
    let mut updated = String::new();
    let mut replaced = false;
    for line in content.lines() {
        if !updated.is_empty() {
            updated.push('\n');
        }
        if !replaced && parse_ac_init_version(line, macro_name).is_some() {
            updated.push_str(&replace_ac_init_version(line, new_value));
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
        Err(anyhow!(
            "configure.ac does not contain {} with a version argument",
            macro_name
        ))
    }
}

fn normalize_autoconf_macro(key_path: &str) -> Result<&'static str> {
    let key_path = key_path.trim();
    if key_path.is_empty() || key_path == "@" || key_path == "." || key_path == "AC_INIT" {
        return Ok("AC_INIT");
    }
    if key_path == "AC_INIT" {
        return Ok("AC_INIT");
    }
    bail!("configure.ac key path must be 'AC_INIT'");
}

fn parse_ac_init_version(line: &str, macro_name: &str) -> Option<String> {
    let trimmed = line.trim();
    if !trimmed.starts_with(macro_name) {
        return None;
    }
    let args = trimmed
        .strip_prefix(macro_name)?
        .trim()
        .trim_start_matches('(')
        .trim_end_matches(')')
        .trim();
    ac_init_argument(args, 1)
}

fn ac_init_argument(args: &str, index: usize) -> Option<String> {
    let tokens = tokenize_ac_init_args(args);
    tokens.get(index).cloned()
}

fn tokenize_ac_init_args(args: &str) -> Vec<String> {
    if args.contains('[') {
        let mut tokens = Vec::new();
        let mut rest = args;
        while let Some(start) = rest.find('[') {
            if let Some(end) = rest[start + 1..].find(']') {
                let end = start + 1 + end;
                tokens.push(rest[start + 1..end].trim().to_string());
                rest = &rest[end + 1..];
            } else {
                break;
            }
        }
        if !tokens.is_empty() {
            return tokens;
        }
    }
    args.split(',')
        .map(|token| {
            token
                .trim()
                .trim_matches(|ch| ch == '"' || ch == '\'')
                .to_string()
        })
        .filter(|token| !token.is_empty())
        .collect()
}

fn ac_init_args_from_line(trimmed: &str) -> &str {
    trimmed
        .strip_prefix("AC_INIT")
        .unwrap_or(trimmed)
        .trim()
        .trim_start_matches('(')
        .trim_end_matches(')')
        .trim()
}

fn replace_ac_init_version(line: &str, new_value: &str) -> String {
    let trimmed = line.trim();
    let indent: String = line.chars().take_while(|ch| ch.is_whitespace()).collect();
    let args = ac_init_args_from_line(trimmed);
    if trimmed.contains('[')
        && let Some(package) = ac_init_argument(args, 0)
    {
        if let Some(email) = ac_init_argument(args, 2) {
            return format!("{indent}AC_INIT([{package}], [{new_value}], [{email}])");
        }
        return format!("{indent}AC_INIT([{package}], [{new_value}])");
    }
    let tokens = tokenize_ac_init_args(args);
    if tokens.len() >= 3 {
        format!(
            "{indent}AC_INIT({}, {}, {})",
            tokens[0], new_value, tokens[2]
        )
    } else if tokens.len() == 2 {
        format!("{indent}AC_INIT({}, {})", tokens[0], new_value)
    } else {
        format!("{indent}AC_INIT([project], [{new_value}])")
    }
}

fn join_lines(lines: Vec<String>, had_trailing_newline: bool) -> Result<String> {
    let mut rendered = lines.join("\n");
    if had_trailing_newline {
        rendered.push('\n');
    }
    Ok(rendered)
}

// --- Part 5: Meson, Nimble, LuaRocks rockspec ---

pub(crate) fn is_meson_filename(path: &str) -> bool {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("meson.build"))
}

pub(crate) fn is_nimble_filename(path: &str) -> bool {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.to_ascii_lowercase().ends_with(".nimble"))
        .unwrap_or(false)
}

pub(crate) fn is_rockspec_filename(path: &str) -> bool {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.to_ascii_lowercase().ends_with(".rockspec"))
        .unwrap_or(false)
}

pub(crate) fn extract_meson_value(content: &str, key_path: &str) -> Result<String> {
    let key_path = key_path.trim();
    if key_path.is_empty()
        || key_path == "@"
        || key_path == "."
        || key_path.eq_ignore_ascii_case("project")
        || key_path.eq_ignore_ascii_case("version")
    {
        for line in content.lines() {
            if let Some(version) = parse_meson_version_line(line) {
                return Ok(version);
            }
        }
        return Err(anyhow!(
            "meson.build does not contain project(..., version: '…') — use key 'project' or 'version'"
        ));
    }
    Err(anyhow!(
        "meson.build key path must be 'project' or 'version'"
    ))
}

pub(crate) fn write_meson_value(content: &str, key_path: &str, new_value: &str) -> Result<String> {
    let key_path = key_path.trim();
    if !(key_path.is_empty()
        || key_path == "@"
        || key_path == "."
        || key_path.eq_ignore_ascii_case("project")
        || key_path.eq_ignore_ascii_case("version"))
    {
        bail!("meson.build key path must be 'project' or 'version'");
    }
    let mut updated = String::new();
    let mut replaced = false;
    for line in content.lines() {
        if !updated.is_empty() {
            updated.push('\n');
        }
        if !replaced && let Some(old) = parse_meson_version_line(line) {
            updated.push_str(&replace_meson_version_line(line, &old, new_value));
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
        Err(anyhow!("meson.build does not contain a version field"))
    }
}

fn parse_meson_version_line(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    let lower = trimmed.to_ascii_lowercase();
    let keyword_index = lower.find("version")?;
    let before = trimmed[..keyword_index].trim();
    if !before.is_empty() && !before.ends_with(',') && !before.ends_with('(') {
        return None;
    }
    let after_keyword = trimmed[keyword_index + "version".len()..].trim_start();
    let after_colon = after_keyword.strip_prefix(':')?.trim_start();
    parse_quoted_assignment_value(after_colon)
}

fn replace_meson_version_line(line: &str, old_value: &str, new_value: &str) -> String {
    if line.contains(&format!("'{old_value}'")) {
        return line.replace(&format!("'{old_value}'"), &format!("'{new_value}'"));
    }
    if line.contains(&format!("\"{old_value}\"")) {
        return line.replace(&format!("\"{old_value}\""), &format!("\"{new_value}\""));
    }
    line.replace(old_value, new_value)
}

pub(crate) fn extract_nimble_value(content: &str, key_path: &str) -> Result<String> {
    extract_assignment_value(content, key_path, "version", "Nim .nimble")
}

pub(crate) fn write_nimble_value(content: &str, key_path: &str, new_value: &str) -> Result<String> {
    write_assignment_value(content, key_path, "version", new_value, "Nim .nimble")
}

pub(crate) fn extract_rockspec_value(content: &str, key_path: &str) -> Result<String> {
    extract_assignment_value(content, key_path, "version", "LuaRocks .rockspec")
}

pub(crate) fn write_rockspec_value(
    content: &str,
    key_path: &str,
    new_value: &str,
) -> Result<String> {
    write_assignment_value(
        content,
        key_path,
        "version",
        new_value,
        "LuaRocks .rockspec",
    )
}

fn extract_assignment_value(
    content: &str,
    key_path: &str,
    default_field: &str,
    manifest_label: &str,
) -> Result<String> {
    let field = normalize_assignment_field(key_path, default_field)?;
    for line in content.lines() {
        if let Some(value) = parse_assignment_line(line, field) {
            return Ok(value);
        }
    }
    Err(anyhow!("{manifest_label} does not define {field}"))
}

fn write_assignment_value(
    content: &str,
    key_path: &str,
    default_field: &str,
    new_value: &str,
    manifest_label: &str,
) -> Result<String> {
    let field = normalize_assignment_field(key_path, default_field)?;
    let mut updated = String::new();
    let mut replaced = false;
    for line in content.lines() {
        if !updated.is_empty() {
            updated.push('\n');
        }
        if !replaced && parse_assignment_line(line, field).is_some() {
            let indent: String = line.chars().take_while(|ch| ch.is_whitespace()).collect();
            updated.push_str(&format!("{indent}{field} = \"{new_value}\""));
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
        Err(anyhow!("{manifest_label} does not define {field}"))
    }
}

fn normalize_assignment_field<'a>(key_path: &str, default_field: &'a str) -> Result<&'a str> {
    let key_path = key_path.trim();
    if key_path.is_empty()
        || key_path == "@"
        || key_path == "."
        || key_path.eq_ignore_ascii_case(default_field)
    {
        return Ok(default_field);
    }
    if key_path == default_field {
        return Ok(default_field);
    }
    bail!("key path must be '{}'", default_field);
}

fn parse_assignment_line(line: &str, field: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    let (name, value) = trimmed.split_once('=')?;
    if name.trim() != field {
        return None;
    }
    parse_quoted_assignment_value(value.trim())
}

// --- Part 6: Perl Makefile.PL, Bazel MODULE.bazel ---

pub(crate) fn is_makefile_pl_filename(path: &str) -> bool {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("Makefile.PL"))
}

pub(crate) fn is_bazel_module_filename(path: &str) -> bool {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("MODULE.bazel"))
}

pub(crate) fn extract_makefile_pl_value(content: &str, key_path: &str) -> Result<String> {
    let field = normalize_makefile_pl_field(key_path)?;
    for line in content.lines() {
        if let Some(version) = parse_makefile_pl_version_line(line, field) {
            return Ok(version);
        }
    }
    if field != "VERSION" {
        for line in content.lines() {
            if let Some(version) = parse_makefile_pl_version_line(line, "VERSION") {
                return Ok(version);
            }
        }
    }
    Err(anyhow!(
        "Makefile.PL does not define a version (try key 'VERSION' or 'version')"
    ))
}

pub(crate) fn write_makefile_pl_value(
    content: &str,
    key_path: &str,
    new_value: &str,
) -> Result<String> {
    let field = normalize_makefile_pl_field(key_path)?;
    let mut updated = String::new();
    let mut replaced = false;
    for line in content.lines() {
        if !updated.is_empty() {
            updated.push('\n');
        }
        if !replaced && let Some(old) = parse_makefile_pl_version_line(line, field) {
            updated.push_str(&replace_makefile_pl_version_line(line, &old, new_value));
            replaced = true;
            continue;
        }
        updated.push_str(line);
    }
    if !replaced && field != "VERSION" {
        let mut fallback = String::new();
        let mut replaced = false;
        for line in content.lines() {
            if !fallback.is_empty() {
                fallback.push('\n');
            }
            if !replaced && let Some(old) = parse_makefile_pl_version_line(line, "VERSION") {
                fallback.push_str(&replace_makefile_pl_version_line(line, &old, new_value));
                replaced = true;
                continue;
            }
            fallback.push_str(line);
        }
        if replaced {
            if content.ends_with('\n') {
                fallback.push('\n');
            }
            return Ok(fallback);
        }
    }
    if replaced {
        if content.ends_with('\n') {
            updated.push('\n');
        }
        Ok(updated)
    } else {
        Err(anyhow!(
            "Makefile.PL does not define a version field to update"
        ))
    }
}

fn normalize_makefile_pl_field(key_path: &str) -> Result<&'static str> {
    let key_path = key_path.trim();
    if key_path.is_empty() || key_path == "@" || key_path == "." || key_path == "VERSION" {
        return Ok("VERSION");
    }
    if key_path.eq_ignore_ascii_case("version") {
        return Ok("version");
    }
    bail!("Makefile.PL key path must be 'VERSION' or 'version'");
}

fn parse_makefile_pl_version_line(line: &str, field: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    if field.eq_ignore_ascii_case("VERSION") {
        if let Some(value) = parse_perl_hash_version_line(trimmed, "VERSION") {
            return Some(value);
        }
        return parse_perl_scalar_version_line(trimmed);
    }
    if field == "version" {
        if let Some(value) = parse_perl_hash_version_line(trimmed, "version") {
            return Some(value);
        }
        return parse_perl_scalar_version_line(trimmed);
    }
    None
}

fn parse_perl_hash_version_line(line: &str, field: &str) -> Option<String> {
    let marker = format!("{field} =>");
    let marker_alt = format!("{field}=>");
    let lower = line.to_ascii_lowercase();
    let marker_lower = marker.to_ascii_lowercase();
    let marker_alt_lower = marker_alt.to_ascii_lowercase();
    if !lower.contains(&marker_lower) && !lower.contains(&marker_alt_lower) {
        return None;
    }
    let index = lower
        .find(&marker_lower)
        .or_else(|| lower.find(&marker_alt_lower))?;
    let after = line[index + field.len()..].trim_start();
    let after = after.strip_prefix("=>")?.trim_start();
    parse_quoted_assignment_value(after)
}

fn parse_perl_scalar_version_line(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let lower = trimmed.to_ascii_lowercase();
    if !lower.contains("$version") {
        return None;
    }
    let index = lower.find("$version")?;
    let after = trimmed[index + "$version".len()..].trim_start();
    let after = after.strip_prefix('=')?.trim_start();
    parse_quoted_assignment_value(after)
}

fn replace_makefile_pl_version_line(line: &str, old_value: &str, new_value: &str) -> String {
    if line.contains(&format!("'{old_value}'")) {
        return line.replace(&format!("'{old_value}'"), &format!("'{new_value}'"));
    }
    if line.contains(&format!("\"{old_value}\"")) {
        return line.replace(&format!("\"{old_value}\""), &format!("\"{new_value}\""));
    }
    line.replace(old_value, new_value)
}

pub(crate) fn extract_bazel_value(content: &str, key_path: &str) -> Result<String> {
    let key_path = key_path.trim();
    if key_path.is_empty()
        || key_path == "@"
        || key_path == "."
        || key_path.eq_ignore_ascii_case("module")
        || key_path.eq_ignore_ascii_case("version")
    {
        for line in content.lines() {
            if let Some(version) = parse_bazel_version_line(line) {
                return Ok(version);
            }
        }
        return Err(anyhow!(
            "MODULE.bazel does not contain module(..., version = \"…\") — use key 'module' or 'version'"
        ));
    }
    Err(anyhow!(
        "MODULE.bazel key path must be 'module' or 'version'"
    ))
}

pub(crate) fn write_bazel_value(content: &str, key_path: &str, new_value: &str) -> Result<String> {
    let key_path = key_path.trim();
    if !(key_path.is_empty()
        || key_path == "@"
        || key_path == "."
        || key_path.eq_ignore_ascii_case("module")
        || key_path.eq_ignore_ascii_case("version"))
    {
        bail!("MODULE.bazel key path must be 'module' or 'version'");
    }
    let mut updated = String::new();
    let mut replaced = false;
    for line in content.lines() {
        if !updated.is_empty() {
            updated.push('\n');
        }
        if !replaced && let Some(old) = parse_bazel_version_line(line) {
            updated.push_str(&replace_bazel_version_line(line, &old, new_value));
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
        Err(anyhow!("MODULE.bazel does not contain a version field"))
    }
}

fn parse_bazel_version_line(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    let lower = trimmed.to_ascii_lowercase();
    if !lower.starts_with("version") {
        return None;
    }
    let after_keyword = trimmed["version".len()..].trim_start();
    let after_equals = after_keyword.strip_prefix('=')?.trim_start();
    parse_quoted_assignment_value(after_equals)
}

fn replace_bazel_version_line(line: &str, old_value: &str, new_value: &str) -> String {
    if line.contains(&format!("'{old_value}'")) {
        return line.replace(&format!("'{old_value}'"), &format!("'{new_value}'"));
    }
    if line.contains(&format!("\"{old_value}\"")) {
        return line.replace(&format!("\"{old_value}\""), &format!("\"{new_value}\""));
    }
    line.replace(old_value, new_value)
}

fn parse_quoted_assignment_value(token: &str) -> Option<String> {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return None;
    }
    let trimmed = trimmed.trim_end_matches(',').trim();
    if (trimmed.starts_with('"') && trimmed.ends_with('"'))
        || (trimmed.starts_with('\'') && trimmed.ends_with('\''))
    {
        let inner = &trimmed[1..trimmed.len() - 1];
        if inner.is_empty() {
            return None;
        }
        return Some(inner.to_string());
    }
    let end = trimmed
        .find(|character: char| character.is_whitespace() || character == ',')
        .unwrap_or(trimmed.len());
    let bare = trimmed[..end].trim();
    if bare.is_empty() {
        return None;
    }
    Some(bare.to_string())
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

    #[test]
    fn swift_package_version_round_trip() {
        let content = "// swift-tools-version:5.9\nlet version = \"1.2.3\"\n";
        let read = extract_swift_package_value(content, "version").expect("read");
        assert_eq!(read, "1.2.3");
        let updated = write_swift_package_value(content, "version", "2.0.0").expect("write");
        assert!(updated.contains("let version = \"2.0.0\""));
    }

    #[test]
    fn elixir_mix_version_round_trip() {
        let content = "def project do\n  [app: :demo, version: \"1.2.3\"]\nend\n";
        let read = extract_elixir_mix_value(content, "version").expect("read");
        assert_eq!(read, "1.2.3");
        let updated = write_elixir_mix_value(content, "version", "2.0.0").expect("write");
        assert!(updated.contains("version: \"2.0.0\""));
    }

    #[test]
    fn scala_sbt_version_round_trip() {
        let content = "ThisBuild / version := \"1.2.3\"\n";
        let read = extract_scala_sbt_value(content, "ThisBuild / version").expect("read");
        assert_eq!(read, "1.2.3");
        let updated =
            write_scala_sbt_value(content, "ThisBuild / version", "2.0.0").expect("write");
        assert!(updated.contains("ThisBuild / version := \"2.0.0\""));
    }

    #[test]
    fn cabal_version_round_trip() {
        let content = "cabal-version: 2.2\nname: demo\nversion: 1.2.3\n";
        let read = extract_cabal_value(content, "version").expect("read");
        assert_eq!(read, "1.2.3");
        let updated = write_cabal_value(content, "version", "2.0.0").expect("write");
        assert!(updated.contains("version: 2.0.0"));
    }

    #[test]
    fn autoconf_ac_init_round_trip() {
        let content = "AC_INIT([myapp], [1.2.3], [bug@example.com])\n";
        let read = extract_autoconf_value(content, "AC_INIT").expect("read");
        assert_eq!(read, "1.2.3");
        let updated = write_autoconf_value(content, "AC_INIT", "2.0.0").expect("write");
        assert!(updated.contains("[2.0.0]"));
    }

    #[test]
    fn meson_project_version_round_trip() {
        let content = "project('demo', 'c',\n  version : '1.2.3',\n)\n";
        let read = extract_meson_value(content, "project").expect("read");
        assert_eq!(read, "1.2.3");
        let updated = write_meson_value(content, "project", "2.0.0").expect("write");
        assert!(updated.contains("'2.0.0'"));
    }

    #[test]
    fn nimble_version_round_trip() {
        let content = "version = \"1.2.3\"\nauthor = \"demo\"\n";
        let read = extract_nimble_value(content, "version").expect("read");
        assert_eq!(read, "1.2.3");
        let updated = write_nimble_value(content, "version", "2.0.0").expect("write");
        assert!(updated.contains("version = \"2.0.0\""));
    }

    #[test]
    fn rockspec_version_round_trip() {
        let content = "package = \"demo\"\nversion = \"1.2.3-1\"\n";
        let read = extract_rockspec_value(content, "version").expect("read");
        assert_eq!(read, "1.2.3-1");
        let updated = write_rockspec_value(content, "version", "2.0.0").expect("write");
        assert!(updated.contains("version = \"2.0.0\""));
    }

    #[test]
    fn makefile_pl_version_round_trip() {
        let content = "use ExtUtils::MakeMaker;\nWriteMakefile(\n    VERSION => '1.2.3',\n);\n";
        let read = extract_makefile_pl_value(content, "VERSION").expect("read");
        assert_eq!(read, "1.2.3");
        let updated = write_makefile_pl_value(content, "VERSION", "2.0.0").expect("write");
        assert!(updated.contains("VERSION => '2.0.0'"));
    }

    #[test]
    fn bazel_module_version_round_trip() {
        let content = "module(\n    name = \"demo\",\n    version = \"1.2.3\",\n)\n";
        let read = extract_bazel_value(content, "module").expect("read");
        assert_eq!(read, "1.2.3");
        let updated = write_bazel_value(content, "module", "2.0.0").expect("write");
        assert!(updated.contains("version = \"2.0.0\""));
    }
}
