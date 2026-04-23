// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit License v1.2
// For details, see the LICENSE file in the repository root.
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use chrono::{Local, NaiveDate};

const FOOTER: &str =
    "<br>\n\n---\n... ✨ made with [ComfyGit](https://github.com/comfy-home/ComfyGit)";
const TEMP_CHANGELOG_FILE: &str = "changelog_temp.md";
const HISTORY_DIR_NAME: &str = ".changelogs";
const HISTORY_SUMMARY_FILE: &str = "README.md";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Category {
    Features,
    Fixes,
    Broken,
    Build,
    Maintenance,
    Enhancements,
    Documentation,
    Visuals,
    UiChanges,
    Refactor,
    Performance,
    Tests,
    Removed,
    Other,
}

impl Category {
    pub(crate) fn heading(self) -> &'static str {
        match self {
            Self::Features => "🧩 Features",
            Self::Fixes => "🐛 Fix(es)",
            Self::Broken => "⛓️‍💥 Not Working Yet / Broken",
            Self::Build => "📦 Build",
            Self::Maintenance => "🔧 Maintenance",
            Self::Enhancements => "💎 Enhancements",
            Self::Documentation => "ℹ️ Documentation",
            Self::Visuals => "🎨 Visuals",
            Self::UiChanges => "📱UI Changes",
            Self::Refactor => "♻️ Refactor",
            Self::Performance => "🚀 Performance",
            Self::Tests => "🧪 Tests",
            Self::Removed => "🗑️ Removed",
            Self::Other => "📝 Other",
        }
    }

    fn order(self) -> usize {
        match self {
            Self::Features => 0,
            Self::Fixes => 1,
            Self::Broken => 2,
            Self::Build => 3,
            Self::Maintenance => 4,
            Self::Enhancements => 5,
            Self::Documentation => 6,
            Self::Visuals => 7,
            Self::UiChanges => 8,
            Self::Refactor => 9,
            Self::Performance => 10,
            Self::Tests => 11,
            Self::Removed => 12,
            Self::Other => 13,
        }
    }

    pub(crate) fn from_alias(alias: &str) -> Option<Self> {
        match normalize_alias(alias).as_str() {
            "feat" | "ft" | "feature" | "element" => Some(Self::Features),
            "fix" | "bugfix" | "bf" => Some(Self::Fixes),
            "broken" | "brkn" | "brk" | "notworking" | "dnw" | "fail" => Some(Self::Broken),
            "build" | "bld" | "rls" | "release" => Some(Self::Build),
            "chore" | "chores" | "depup" | "dpndc" | "dep" | "mtn" | "mtnnc" | "mt" | "upd"
            | "bump" | "bmp" => Some(Self::Maintenance),
            "enh" | "improve" | "impr" | "imp" | "improvement" | "improvements" | "upgrade"
            | "upg" => Some(Self::Enhancements),
            "docs" | "dox" | "ducu" | "documentation" => Some(Self::Documentation),
            "style" | "stl" | "vis" | "visual" | "visuals" => Some(Self::Visuals),
            "ui" | "gui" | "fe" | "frontend" => Some(Self::UiChanges),
            "ref" | "refactor" | "rfc" | "rf" | "rfb" | "refurb" | "makeover" => {
                Some(Self::Refactor)
            }
            "perf" | "prf" | "opt" | "optim" => Some(Self::Performance),
            "test" | "tst" | "try" => Some(Self::Tests),
            "rem" | "del" | "rm" => Some(Self::Removed),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParsedCommit {
    pub(crate) raw_subject: String,
    pub(crate) short_hash: String,
    pub(crate) category: Option<Category>,
    pub(crate) specific: Option<String>,
    pub(crate) specific_heading: Option<&'static str>,
    pub(crate) is_new: bool,
    pub(crate) is_breaking: bool,
    pub(crate) is_ignored: bool,
    pub(crate) message_items: Vec<MessageItem>,
}

impl ParsedCommit {
    #[cfg(test)]
    pub(crate) fn parse(subject: &str, short_hash: impl Into<String>) -> Self {
        Self::parse_single(subject, short_hash.into())
    }

    pub(crate) fn parse_many(subject: &str, short_hash: impl Into<String>) -> Vec<Self> {
        let short_hash = short_hash.into();
        split_subject_clauses(subject)
            .into_iter()
            .map(|clause| Self::parse_single(&clause, short_hash.clone()))
            .collect()
    }

    fn parse_single(subject: &str, short_hash: String) -> Self {
        let raw_subject = subject.trim().to_string();
        let mut remainder = raw_subject.as_str().trim();
        let mut is_breaking = false;
        let mut is_new = false;
        let mut is_ignored = false;

        loop {
            let trimmed = remainder.trim_start();
            if let Some(next) = trimmed.strip_prefix('!') {
                is_breaking = true;
                remainder = next;
                continue;
            }
            if let Some(next) = trimmed.strip_prefix('@') {
                is_new = true;
                remainder = next;
                continue;
            }
            if let Some(next) = trimmed.strip_prefix('~') {
                is_ignored = true;
                remainder = next;
                continue;
            }
            remainder = trimmed;
            break;
        }

        let (prefix, message) = split_prefix_and_message(remainder);
        let (category, specific, specific_heading) = parse_prefix(prefix);
        let message_items = parse_message_items(message);

        Self {
            raw_subject,
            short_hash,
            category,
            specific,
            specific_heading,
            is_new,
            is_breaking,
            is_ignored,
            message_items,
        }
    }

    fn effective_category(&self) -> Category {
        self.category.unwrap_or(Category::Other)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MessageItem {
    Text(String),
    NestedList {
        intro: String,
        items: Vec<NestedListEntry>,
        summary: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NestedListEntry {
    level: usize,
    text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RenderedChangelog {
    pub(crate) markdown: String,
}

impl RenderedChangelog {
    pub(crate) fn new(markdown: String) -> Self {
        Self { markdown }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChangelogDocument {
    current_tag: String,
    date: NaiveDate,
    previous_public_release: Option<String>,
    context_lines: Vec<String>,
    release_message: Option<String>,
    commits: Vec<ParsedCommit>,
}

impl ChangelogDocument {
    pub(crate) fn new(current_tag: impl Into<String>, commits: Vec<ParsedCommit>) -> Self {
        Self {
            current_tag: current_tag.into(),
            date: Local::now().date_naive(),
            previous_public_release: None,
            context_lines: Vec::new(),
            release_message: None,
            commits,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_date(mut self, date: NaiveDate) -> Self {
        self.date = date;
        self
    }

    pub(crate) fn with_release_message(mut self, release_message: impl Into<String>) -> Self {
        let message = release_message.into();
        if !message.trim().is_empty() {
            self.release_message = Some(message.trim().to_string());
        }
        self
    }

    pub(crate) fn with_previous_public_release(
        mut self,
        previous_public_release: impl Into<String>,
    ) -> Self {
        let value = previous_public_release.into();
        if !value.trim().is_empty() {
            self.previous_public_release = Some(value.trim().to_string());
        }
        self
    }

    pub(crate) fn render_markdown(&self) -> RenderedChangelog {
        let header = match self.previous_public_release.as_ref() {
            Some(previous_public) => format!(
                "## Changelog {} <sub><sup>← {} (Previous Public Version)</sup></sub>",
                self.current_tag, previous_public
            ),
            None => format!("## Changelog {}", self.current_tag),
        };

        let mut lines = vec![
            header,
            self.date.format("%Y-%m-%d").to_string(),
            String::new(),
        ];

        if !self.context_lines.is_empty() {
            lines.extend(self.context_lines.iter().cloned());
            lines.push(String::new());
        }

        if let Some(release_message) = &self.release_message {
            lines.push(release_message.clone());
            lines.push(String::new());
        }

        lines.push("#### What's changed:".to_string());
        lines.push(String::new());

        let visible_commits = self
            .commits
            .iter()
            .filter(|commit| !commit.is_ignored)
            .collect::<Vec<_>>();

        render_breaking_section(&mut lines, &visible_commits);

        let non_breaking = visible_commits
            .iter()
            .copied()
            .filter(|commit| !commit.is_breaking)
            .collect::<Vec<_>>();

        let rendered_new_specific = render_new_specific_sections(&mut lines, &non_breaking);
        let rendered_specific = render_specific_sections(&mut lines, &non_breaking);
        if (rendered_new_specific || rendered_specific) && has_general_improvements(&non_breaking) {
            lines.push("### 🛠️ General:".to_string());
            lines.push(String::new());
        }

        render_new_plain_section(&mut lines, &non_breaking);
        render_plain_category_sections(&mut lines, &non_breaking);

        if lines.last().is_some_and(|line| !line.is_empty()) {
            lines.push(String::new());
        }
        lines.push(FOOTER.to_string());

        RenderedChangelog::new(lines.join("\n"))
    }
}

pub(crate) fn build_document_from_git_log(
    current_tag: impl Into<String>,
    lines: &[String],
) -> ChangelogDocument {
    let commits = lines
        .iter()
        .flat_map(|line| parse_graph_log_entries(line))
        .collect::<Vec<_>>();
    ChangelogDocument::new(current_tag, commits)
}

pub(crate) fn std_changelog_gen(
    current_tag: impl Into<String>,
    lines: &[String],
) -> RenderedChangelog {
    build_document_from_git_log(current_tag, lines).render_markdown()
}

pub(crate) fn rls_changelog_gen(
    current_tag: impl Into<String>,
    lines: &[String],
    last_public: Option<&str>,
) -> RenderedChangelog {
    let mut document = build_document_from_git_log(current_tag, lines);
    if let Some(last_public) = last_public.filter(|value| !value.trim().is_empty()) {
        document = document.with_previous_public_release(last_public);
    }
    document.render_markdown()
}

pub(crate) fn ensure_previous_public_release_header(
    markdown: &str,
    current_tag: &str,
    previous_public_release: Option<&str>,
) -> String {
    let Some(previous_public_release) = previous_public_release
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return markdown.to_string();
    };

    let plain_header = format!("## Changelog {}", current_tag.trim());
    let enriched_header = format!(
        "## Changelog {} <sub><sup>← {} (Previous Public Version)</sup></sub>",
        current_tag.trim(),
        previous_public_release
    );

    let mut lines = markdown.lines().map(ToOwned::to_owned).collect::<Vec<_>>();
    let Some(header_index) = lines.iter().position(|line| !line.trim().is_empty()) else {
        return markdown.to_string();
    };

    let current_header = lines[header_index].trim();
    if current_header == enriched_header {
        return markdown.to_string();
    }
    if current_header != plain_header {
        return markdown.to_string();
    }

    lines[header_index] = enriched_header;
    lines.join("\n")
}

#[allow(dead_code)]
pub(crate) fn custom_changelog_gen(
    current_tag: impl Into<String>,
    lines: &[String],
    release_message: Option<&str>,
) -> RenderedChangelog {
    let mut document = build_document_from_git_log(current_tag, lines);
    if let Some(release_message) = release_message.filter(|value| !value.trim().is_empty()) {
        document = document.with_release_message(release_message.to_string());
    }
    document.render_markdown()
}

pub(crate) fn sum_changelog_gen(repo_root: &str) -> Result<PathBuf> {
    let history_dir = Path::new(repo_root).join(HISTORY_DIR_NAME);
    fs::create_dir_all(&history_dir)
        .with_context(|| format!("failed to create {}", history_dir.display()))?;
    write_history_summary_readme(&history_dir)
}

pub(crate) fn write_changelog_markdown(
    repo_root: &str,
    changelog_path: &str,
    markdown: &str,
) -> Result<PathBuf> {
    let output_path = resolve_changelog_path(repo_root, changelog_path);
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let rendered = if output_path.is_file() {
        let existing = fs::read_to_string(&output_path)
            .with_context(|| format!("failed to read {}", output_path.display()))?;
        let existing = existing.trim();
        if existing.is_empty() {
            markdown.trim_end().to_string()
        } else {
            format!("{}\n\n{}", markdown.trim_end(), existing)
        }
    } else {
        markdown.trim_end().to_string()
    };

    fs::write(&output_path, rendered)
        .with_context(|| format!("failed to write {}", output_path.display()))?;
    Ok(output_path)
}

pub(crate) fn write_temp_changelog_markdown(repo_root: &str, markdown: &str) -> Result<PathBuf> {
    let output_path = Path::new(repo_root).join(TEMP_CHANGELOG_FILE);
    fs::write(&output_path, markdown.trim_end())
        .with_context(|| format!("failed to write {}", output_path.display()))?;
    Ok(output_path)
}

pub(crate) fn archive_changelog_markdown(
    repo_root: &str,
    label: &str,
    markdown: &str,
) -> Result<PathBuf> {
    let history_dir = Path::new(repo_root).join(HISTORY_DIR_NAME);
    fs::create_dir_all(&history_dir)
        .with_context(|| format!("failed to create {}", history_dir.display()))?;

    let file_name =
        format_history_file_name(label, Local::now().format("%Y%m%d-%H%M%S-%3f").to_string());
    let output_path = history_dir.join(file_name);
    fs::write(&output_path, markdown.trim_end())
        .with_context(|| format!("failed to write {}", output_path.display()))?;
    write_history_summary_readme(&history_dir)?;
    Ok(output_path)
}

pub(crate) fn rebuild_history_summary_readme(repo_root: &str) -> Result<Option<PathBuf>> {
    let history_dir = Path::new(repo_root).join(HISTORY_DIR_NAME);
    if !history_dir.is_dir() {
        return Ok(None);
    }

    write_history_summary_readme(&history_dir).map(Some)
}

pub(crate) fn find_archived_changelog_markdown(
    repo_root: &str,
    label: &str,
) -> Result<Option<String>> {
    let history_dir = Path::new(repo_root).join(HISTORY_DIR_NAME);
    if !history_dir.is_dir() {
        return Ok(None);
    }

    let candidates = history_label_candidates(label);
    let mut matches = fs::read_dir(&history_dir)
        .with_context(|| format!("failed to read {}", history_dir.display()))?
        .filter_map(|entry| entry.ok().map(|item| item.path()))
        .filter(|path| {
            path.extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case("md"))
        })
        .filter_map(|path| {
            let stem = path.file_stem()?.to_str()?;
            let parsed = parse_history_file_stem(stem)?;
            Some((path, parsed))
        })
        .filter(|(_, parsed)| {
            candidates
                .iter()
                .any(|candidate| candidate == &parsed.label)
        })
        .collect::<Vec<_>>();

    if matches.is_empty() {
        return Ok(None);
    }

    matches.sort_by(|left, right| {
        left.1
            .timestamp
            .cmp(&right.1.timestamp)
            .then_with(|| left.0.cmp(&right.0))
    });
    let (path, _) = matches.pop().expect("matches should not be empty");
    Ok(Some(fs::read_to_string(&path).with_context(|| {
        format!("failed to read {}", path.display())
    })?))
}

fn resolve_changelog_path(repo_root: &str, changelog_path: &str) -> PathBuf {
    let candidate = Path::new(changelog_path);
    if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        Path::new(repo_root).join(candidate)
    }
}

fn sanitize_history_label(label: &str) -> String {
    let sanitized = label
        .trim()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    let sanitized = sanitized.trim_matches('-');
    if sanitized.is_empty() {
        "changelog".to_string()
    } else {
        sanitized.to_string()
    }
}

fn format_history_file_name(label: &str, timestamp: String) -> String {
    format!("{}__{}.md", sanitize_history_label(label), timestamp)
}

fn history_label_candidates(label: &str) -> Vec<String> {
    let trimmed = label.trim();
    let mut candidates = Vec::new();

    for candidate in [
        trimmed.to_string(),
        trimmed.strip_prefix('v').unwrap_or(trimmed).to_string(),
        trimmed
            .rsplit_once("-v")
            .map(|(_, version)| version.to_string())
            .unwrap_or_else(|| trimmed.to_string()),
    ] {
        let sanitized = sanitize_history_label(&candidate);
        if !sanitized.is_empty() && !candidates.iter().any(|existing| existing == &sanitized) {
            candidates.push(sanitized);
        }
    }

    candidates
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ArchivedChangelogEntry {
    timestamp: String,
    label: String,
    markdown: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedHistoryFileStem {
    timestamp: String,
    label: String,
}

fn write_history_summary_readme(history_dir: &Path) -> Result<PathBuf> {
    let entries = load_archived_changelog_entries(history_dir)?;
    let summary_path = history_dir.join(HISTORY_SUMMARY_FILE);
    let rendered = render_history_summary_markdown(&entries);
    fs::write(&summary_path, rendered.trim_end())
        .with_context(|| format!("failed to write {}", summary_path.display()))?;
    Ok(summary_path)
}

fn load_archived_changelog_entries(history_dir: &Path) -> Result<Vec<ArchivedChangelogEntry>> {
    let mut entries = fs::read_dir(history_dir)
        .with_context(|| format!("failed to read {}", history_dir.display()))?
        .filter_map(|entry| entry.ok().map(|item| item.path()))
        .filter(|path| {
            path.extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case("md"))
        })
        .filter(|path| {
            path.file_name().and_then(|value| value.to_str()) != Some(HISTORY_SUMMARY_FILE)
        })
        .filter_map(|path| {
            let stem = path.file_stem()?.to_str()?;
            let parsed = parse_history_file_stem(stem)?;
            Some((path, parsed))
        })
        .map(|(path, parsed)| {
            Ok(ArchivedChangelogEntry {
                timestamp: parsed.timestamp,
                label: parsed.label,
                markdown: fs::read_to_string(&path)
                    .with_context(|| format!("failed to read {}", path.display()))?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    entries.sort_by(|left, right| right.timestamp.cmp(&left.timestamp));
    Ok(entries)
}

fn parse_history_file_stem(stem: &str) -> Option<ParsedHistoryFileStem> {
    if let Some((label, timestamp)) = stem.split_once("__") {
        let normalized_timestamp = normalize_history_timestamp(timestamp)?;
        let normalized_label = sanitize_history_label(label);
        if normalized_label.is_empty() {
            return None;
        }
        return Some(ParsedHistoryFileStem {
            timestamp: normalized_timestamp,
            label: normalized_label,
        });
    }

    let mut parts = stem.splitn(4, '-');
    let date = parts.next()?;
    let time = parts.next()?;
    let millis = parts.next()?;
    let label = parts.next()?.trim();
    let normalized_timestamp = normalize_history_timestamp(&format!("{date}-{time}-{millis}"))?;
    let normalized_label = sanitize_history_label(label);
    if normalized_label.is_empty() {
        return None;
    }

    Some(ParsedHistoryFileStem {
        timestamp: normalized_timestamp,
        label: normalized_label,
    })
}

fn normalize_history_timestamp(timestamp: &str) -> Option<String> {
    let mut parts = timestamp.splitn(3, '-');
    let date = parts.next()?;
    let time = parts.next()?;
    let millis = parts.next()?;

    if date.len() != 8
        || time.len() != 6
        || millis.len() != 3
        || !date.chars().all(|character| character.is_ascii_digit())
        || !time.chars().all(|character| character.is_ascii_digit())
        || !millis.chars().all(|character| character.is_ascii_digit())
    {
        return None;
    }

    Some(format!("{date}-{time}-{millis}"))
}

fn render_history_summary_markdown(entries: &[ArchivedChangelogEntry]) -> String {
    let mut seen = HashSet::new();
    let mut lines = vec![
		"# Changelog History".to_string(),
		String::new(),
		"Newest archived changelogs first. When multiple archived files represent the same version, only the newest archive is included here.".to_string(),
		String::new(),
	];

    for entry in entries {
        let dedupe_key = history_summary_key(&entry.label);
        if !seen.insert(dedupe_key) {
            continue;
        }

        let normalized = strip_summary_footer(&entry.markdown);
        if normalized.is_empty() {
            continue;
        }

        lines.push(normalized);
        lines.push(String::new());
        lines.push("---".to_string());
        lines.push(String::new());
    }

    if lines.len() == 4 {
        lines.push("No archived changelogs yet.".to_string());
        lines.push(String::new());
    } else {
        while lines.last().is_some_and(|line| line.is_empty()) {
            lines.pop();
        }
        if lines.last().is_some_and(|line| line == "---") {
            lines.pop();
        }
        while lines.last().is_some_and(|line| line.is_empty()) {
            lines.pop();
        }
        lines.push(String::new());
    }

    lines.push(FOOTER.to_string());
    lines.join("\n")
}

fn history_summary_key(label: &str) -> String {
    history_label_candidates(label)
        .pop()
        .unwrap_or_else(|| sanitize_history_label(label))
}

fn strip_summary_footer(markdown: &str) -> String {
    markdown
        .trim()
        .strip_suffix(FOOTER)
        .unwrap_or(markdown.trim())
        .trim_end()
        .to_string()
}

#[cfg(test)]
fn parse_graph_log_line(line: &str) -> Option<ParsedCommit> {
    parse_graph_log_entries(line).into_iter().next()
}

fn parse_graph_log_entries(line: &str) -> Vec<ParsedCommit> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let chars = trimmed.char_indices().collect::<Vec<_>>();
    let mut index = 0;
    while index < chars.len() {
        let (start_offset, current) = chars[index];
        if !current.is_ascii_hexdigit() {
            index += 1;
            continue;
        }

        let mut end_index = index;
        while end_index < chars.len() && chars[end_index].1.is_ascii_hexdigit() {
            end_index += 1;
        }

        let end_offset = chars
            .get(end_index)
            .map(|(offset, _)| *offset)
            .unwrap_or(trimmed.len());
        let hash = &trimmed[start_offset..end_offset];
        if hash.len() >= 7 {
            let subject = trimmed[end_offset..].trim();
            if !subject.is_empty() {
                return ParsedCommit::parse_many(subject, hash.to_string());
            }
        }

        index = end_index;
    }

    Vec::new()
}

fn normalize_alias(alias: &str) -> String {
    alias
        .trim()
        .trim_matches(|ch: char| ch.is_ascii_punctuation() || ch.is_whitespace())
        .to_ascii_lowercase()
}

fn split_prefix_and_message(input: &str) -> (&str, &str) {
    let mut depth = 0;
    for (index, ch) in input.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' if depth > 0 => depth -= 1,
            ':' if depth == 0 => return (&input[..index], input[index + 1..].trim()),
            _ => {}
        }
    }
    (input.trim(), input.trim())
}

fn parse_prefix(prefix: &str) -> (Option<Category>, Option<String>, Option<&'static str>) {
    let trimmed = prefix.trim();
    if trimmed.is_empty() {
        return (None, None, None);
    }

    if let Some(dotted) = trimmed.strip_prefix('.') {
        let (category, specific) = parse_prefix_parts(dotted);
        let specific_heading = category.and_then(singular_specific_heading);
        return (category, specific, specific_heading);
    }

    let (category, specific) = parse_prefix_parts(trimmed);
    (category, specific, None)
}

fn parse_prefix_parts(prefix: &str) -> (Option<Category>, Option<String>) {
    let trimmed = prefix.trim();
    if trimmed.is_empty() {
        return (None, None);
    }

    if let Some((category_part, specific_part)) = trimmed.split_once('→') {
        return (
            Category::from_alias(category_part),
            normalize_specific(specific_part),
        );
    }

    if trimmed.starts_with('(') && trimmed.ends_with(')') {
        return (None, normalize_specific(&trimmed[1..trimmed.len() - 1]));
    }

    if let Some(open_index) = trimmed.find('(')
        && trimmed.ends_with(')')
    {
        let category_part = &trimmed[..open_index];
        let specific_part = &trimmed[open_index + 1..trimmed.len() - 1];
        return (
            Category::from_alias(category_part),
            normalize_specific(specific_part),
        );
    }

    (Category::from_alias(trimmed), None)
}

fn singular_specific_heading(category: Category) -> Option<&'static str> {
    match category {
        Category::Features => Some("Feature"),
        Category::Enhancements => Some("Enhancement"),
        _ => None,
    }
}

fn normalize_specific(value: &str) -> Option<String> {
    let cleaned = value.trim();
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned.to_string())
    }
}

fn parse_message_items(message: &str) -> Vec<MessageItem> {
    let trimmed = message.trim();
    if trimmed.is_empty() {
        return vec![MessageItem::Text("No details provided".to_string())];
    }

    trimmed
        .split(';')
        .filter_map(|segment| {
            let segment = segment.trim();
            if segment.is_empty() {
                return None;
            }

            if let Some((intro, items, summary)) = parse_nested_list(segment) {
                return Some(MessageItem::NestedList {
                    intro,
                    items,
                    summary,
                });
            }

            Some(MessageItem::Text(segment.to_string()))
        })
        .collect()
}

fn split_subject_clauses(subject: &str) -> Vec<String> {
    let mut clauses = Vec::new();
    let mut current = String::new();

    for segment in subject.split(';') {
        let trimmed = segment.trim();
        if trimmed.is_empty() {
            continue;
        }

        if current.is_empty() {
            current.push_str(trimmed);
            continue;
        }

        if looks_like_prefixed_clause(trimmed) {
            clauses.push(current);
            current = trimmed.to_string();
        } else {
            current.push_str("; ");
            current.push_str(trimmed);
        }
    }

    if !current.is_empty() {
        clauses.push(current);
    }

    clauses
}

fn looks_like_prefixed_clause(segment: &str) -> bool {
    let trimmed = segment.trim_start();
    let Some((prefix, _)) = trimmed.split_once(':') else {
        return false;
    };
    let prefix = prefix.trim();
    if prefix.is_empty() {
        return false;
    }

    let mut remainder = prefix;
    loop {
        let next = remainder.trim_start();
        if let Some(stripped) = next.strip_prefix('!') {
            remainder = stripped;
            continue;
        }
        if let Some(stripped) = next.strip_prefix('@') {
            remainder = stripped;
            continue;
        }
        break;
    }

    let remainder = remainder.trim();
    if remainder.is_empty() {
        return false;
    }

    let (category, specific, specific_heading) = parse_prefix(remainder);
    category.is_some() || specific.is_some() || specific_heading.is_some()
}

fn parse_nested_list(segment: &str) -> Option<(String, Vec<NestedListEntry>, Option<String>)> {
    let (intro, trailing) = segment.split_once(':')?;
    if !trailing.contains('*') || !trailing.trim_start().starts_with('*') {
        return None;
    }

    let (items, summary) = parse_nested_list_items(trailing.trim_start())?;
    if items.is_empty() {
        return None;
    }

    Some((format!("{}:", intro.trim()), items, summary))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NestedListMarker {
    Level(usize),
    End,
}

fn parse_nested_list_items(trailing: &str) -> Option<(Vec<NestedListEntry>, Option<String>)> {
    let mut entries = Vec::new();
    let mut summary = None;
    let mut cursor = 0;

    while cursor < trailing.len() {
        let Some((marker_index, marker)) = find_next_nested_list_marker(trailing, cursor) else {
            break;
        };

        let marker_len = nested_list_marker_len(marker);
        let content_start = marker_index + marker_len;

        match marker {
            NestedListMarker::End => {
                let text = trailing[content_start..].trim();
                if !text.is_empty() {
                    summary = Some(text.to_string());
                }
                break;
            }
            NestedListMarker::Level(level) => {
                let next_index = find_next_nested_list_marker(trailing, content_start)
                    .map(|(index, _)| index)
                    .unwrap_or(trailing.len());
                let text = trailing[content_start..next_index].trim();
                if !text.is_empty() {
                    entries.push(NestedListEntry {
                        level,
                        text: text.to_string(),
                    });
                }
                cursor = next_index;
            }
        }
    }

    (!entries.is_empty()).then_some((entries, summary))
}

fn find_next_nested_list_marker(input: &str, start: usize) -> Option<(usize, NestedListMarker)> {
    let bytes = input.as_bytes();
    let mut index = start;

    while index < bytes.len() {
        if bytes[index] != b'*' {
            index += 1;
            continue;
        }

        if input[index..].starts_with("*end*") {
            return Some((index, NestedListMarker::End));
        }
        if input[index..].starts_with("***") {
            return Some((index, NestedListMarker::Level(3)));
        }
        if input[index..].starts_with("**") {
            return Some((index, NestedListMarker::Level(2)));
        }
        return Some((index, NestedListMarker::Level(1)));
    }

    None
}

fn nested_list_marker_len(marker: NestedListMarker) -> usize {
    match marker {
        NestedListMarker::Level(level) => level,
        NestedListMarker::End => 5,
    }
}

fn render_breaking_section(lines: &mut Vec<String>, commits: &[&ParsedCommit]) {
    let breaking = commits
        .iter()
        .copied()
        .filter(|commit| commit.is_breaking)
        .collect::<Vec<_>>();
    if breaking.is_empty() {
        return;
    }

    lines.push("## 💥⚠️ BREAKING CHANGE ⚠️💥".to_string());
    lines.push(String::new());

    let specific_names = ordered_specific_names(&breaking);
    for specific_name in &specific_names {
        lines.push(format!("### {}", specific_name));
        lines.push(String::new());
        let group_commits: Vec<&ParsedCommit> = breaking
            .iter()
            .copied()
            .filter(|commit| commit.specific.as_deref() == Some(specific_name.as_str()))
            .collect::<Vec<_>>();
        render_category_subsections(lines, &group_commits, 4);
    }

    let unspecific: Vec<&ParsedCommit> = breaking
        .iter()
        .copied()
        .filter(|commit| commit.specific.is_none())
        .collect::<Vec<_>>();
    render_category_subsections(lines, &unspecific, 3);
}

fn render_new_specific_sections(lines: &mut Vec<String>, commits: &[&ParsedCommit]) -> bool {
    let specific_keys = ordered_new_specific_keys(
        &commits
            .iter()
            .copied()
            .filter(|commit| commit.is_new && commit.specific.is_some())
            .collect::<Vec<_>>(),
    );
    if specific_keys.is_empty() {
        return false;
    }

    for (specific_name, specific_heading) in specific_keys {
        if let Some(specific_heading) = specific_heading {
            lines.push(format!(
                "### ✨ New {}: {}",
                specific_heading, specific_name
            ));
            lines.push(String::new());
            let section_commits: Vec<&ParsedCommit> = commits
                .iter()
                .copied()
                .filter(|commit| {
                    commit.is_new
                        && commit.specific.as_deref() == Some(specific_name.as_str())
                        && commit.specific_heading == Some(specific_heading)
                })
                .collect::<Vec<_>>();
            for commit in section_commits {
                render_commit_bullets(lines, commit);
            }
            end_specific_section(lines);
        } else {
            lines.push(format!("### ✨ New in {}:", specific_name));
            lines.push(String::new());
            let section_commits: Vec<&ParsedCommit> = commits
                .iter()
                .copied()
                .filter(|commit| {
                    commit.is_new
                        && commit.specific.as_deref() == Some(specific_name.as_str())
                        && commit.specific_heading == specific_heading
                })
                .collect::<Vec<_>>();
            render_category_subsections(lines, &section_commits, 4);
            end_specific_section(lines);
        }
    }

    true
}

fn render_new_plain_section(lines: &mut Vec<String>, commits: &[&ParsedCommit]) {
    let new_commits: Vec<&ParsedCommit> = commits
        .iter()
        .copied()
        .filter(|commit| commit.is_new && commit.specific.is_none())
        .collect::<Vec<_>>();
    if new_commits.is_empty() {
        return;
    }

    lines.push("### ✨ New:".to_string());
    lines.push(String::new());
    render_category_subsections(lines, &new_commits, 4);
}

fn render_specific_sections(lines: &mut Vec<String>, commits: &[&ParsedCommit]) -> bool {
    let specific_names = ordered_specific_names(
        &commits
            .iter()
            .copied()
            .filter(|commit| !commit.is_new && commit.specific.is_some())
            .collect::<Vec<_>>(),
    );
    if specific_names.is_empty() {
        return false;
    }

    for specific_name in specific_names {
        lines.push(format!("### Changed in {}", specific_name));
        lines.push(String::new());
        let section_commits: Vec<&ParsedCommit> = commits
            .iter()
            .copied()
            .filter(|commit| {
                !commit.is_new && commit.specific.as_deref() == Some(specific_name.as_str())
            })
            .collect::<Vec<_>>();
        render_category_subsections(lines, &section_commits, 4);
        end_specific_section(lines);
    }

    true
}

fn has_general_improvements(commits: &[&ParsedCommit]) -> bool {
    commits.iter().any(|commit| commit.specific.is_none())
}

fn end_specific_section(lines: &mut Vec<String>) {
    if lines.last().is_some_and(|line| !line.is_empty()) {
        lines.push(String::new());
    }
    lines.push("---".to_string());
    lines.push(String::new());
}

fn render_plain_category_sections(lines: &mut Vec<String>, commits: &[&ParsedCommit]) {
    let plain_commits: Vec<&ParsedCommit> = commits
        .iter()
        .copied()
        .filter(|commit| !commit.is_new && commit.specific.is_none())
        .collect::<Vec<_>>();

    for category in ordered_categories(&plain_commits) {
        lines.push(format!("### {}", category.heading()));
        lines.push(String::new());
        for commit in plain_commits
            .iter()
            .filter(|commit| commit.effective_category() == category)
        {
            render_commit_bullets(lines, commit);
        }
    }
}

fn render_category_subsections<T>(lines: &mut Vec<String>, commits: &[T], heading_level: usize)
where
    T: std::borrow::Borrow<ParsedCommit>,
{
    for category in ordered_categories(commits) {
        lines.push(format!(
            "{} {}",
            "#".repeat(heading_level),
            category.heading()
        ));
        lines.push(String::new());
        for commit in commits
            .iter()
            .map(std::borrow::Borrow::borrow)
            .filter(|commit| commit.effective_category() == category)
        {
            render_commit_bullets(lines, commit);
        }
    }
}

fn render_commit_bullets(lines: &mut Vec<String>, commit: &ParsedCommit) {
    for item in &commit.message_items {
        match item {
            MessageItem::Text(text) => {
                lines.push(format!("* {}   _({})_", text, commit.short_hash));
            }
            MessageItem::NestedList {
                intro,
                items,
                summary,
            } => {
                lines.push(format!("{}   _({})_", intro, commit.short_hash));
                for item in items {
                    lines.push(format!("{}* {}", "  ".repeat(item.level - 1), item.text));
                }
                if let Some(summary) = summary {
                    lines.push(String::new());
                    lines.push(format!("<sup>💡 >> {}</sup>", summary));
                }
            }
        }
    }
    lines.push(String::new());
}

fn ordered_categories<T>(commits: &[T]) -> Vec<Category>
where
    T: std::borrow::Borrow<ParsedCommit>,
{
    let mut categories = commits
        .iter()
        .map(|commit| commit.borrow().effective_category())
        .collect::<Vec<_>>();
    categories.sort_by_key(|category| category.order());
    categories.dedup();
    categories
}

fn ordered_specific_names<T>(commits: &[T]) -> Vec<String>
where
    T: std::borrow::Borrow<ParsedCommit>,
{
    let mut names = Vec::new();
    for commit in commits {
        let commit = commit.borrow();
        if let Some(name) = &commit.specific
            && !names.iter().any(|existing| existing == name)
        {
            names.push(name.clone());
        }
    }
    names
}

fn ordered_new_specific_keys<T>(commits: &[T]) -> Vec<(String, Option<&'static str>)>
where
    T: std::borrow::Borrow<ParsedCommit>,
{
    let mut keys = Vec::new();
    for commit in commits {
        let commit = commit.borrow();
        if let Some(name) = &commit.specific {
            let candidate = (name.clone(), commit.specific_heading);
            if !keys.iter().any(|existing| existing == &candidate) {
                keys.push(candidate);
            }
        }
    }
    keys
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_alias_and_specific_group_with_arrow() {
        let parsed = ParsedCommit::parse("feat → Phase 1: add parsing pipeline", "abc1234");

        assert_eq!(parsed.category, Some(Category::Features));
        assert_eq!(parsed.specific.as_deref(), Some("Phase 1"));
        assert_eq!(parsed.specific_heading, None);
        assert!(!parsed.is_new);
        assert!(!parsed.is_breaking);
        assert_eq!(
            parsed.message_items,
            vec![MessageItem::Text("add parsing pipeline".to_string())]
        );
    }

    #[test]
    fn parses_broken_aliases() {
        let parsed = ParsedCommit::parse("brk: release updater is not available yet", "abc1234");

        assert_eq!(parsed.category, Some(Category::Broken));
        assert_eq!(parsed.effective_category(), Category::Broken);
    }

    #[test]
    fn renders_new_specific_heading_without_duplicate_category() {
        let changelog = ChangelogDocument::new(
            "v0.7.2",
            vec![ParsedCommit::parse(
                "@.feat(Changelog Generation): add summary generation",
                "abc1234",
            )],
        )
        .with_date(NaiveDate::from_ymd_opt(2026, 4, 16).unwrap())
        .render_markdown();

        assert!(
            changelog
                .markdown
                .contains("### ✨ New Feature: Changelog Generation")
        );
        assert!(changelog.markdown.contains("* add summary generation"));
        assert!(!changelog.markdown.contains("#### 🧩 Features"));
    }

    #[test]
    fn parses_new_modifier_without_category() {
        let parsed = ParsedCommit::parse("@: first public release", "abc1234");

        assert!(parsed.is_new);
        assert_eq!(parsed.category, None);
        assert_eq!(parsed.specific, None);
        assert_eq!(parsed.effective_category(), Category::Other);
    }

    #[test]
    fn parses_ignore_modifier_without_category() {
        let parsed = ParsedCommit::parse("~: ignore this commit", "abc1234");

        assert!(parsed.is_ignored);
        assert_eq!(parsed.category, None);
        assert_eq!(parsed.specific, None);
        assert_eq!(
            parsed.message_items,
            vec![MessageItem::Text("ignore this commit".to_string())]
        );
    }

    #[test]
    fn parses_ignore_modifier_with_category() {
        let parsed = ParsedCommit::parse("~feat: ignore this feature", "abc1234");

        assert!(parsed.is_ignored);
        assert_eq!(parsed.category, Some(Category::Features));
        assert_eq!(
            parsed.message_items,
            vec![MessageItem::Text("ignore this feature".to_string())]
        );
    }

    #[test]
    fn ignored_commits_are_excluded_from_rendered_changelog() {
        let changelog = ChangelogDocument::new(
            "v0.8.0",
            vec![
                ParsedCommit::parse("~: ignore this commit", "abc1234"),
                ParsedCommit::parse("fix: include this commit", "def5678"),
            ],
        )
        .with_date(NaiveDate::from_ymd_opt(2026, 4, 17).unwrap())
        .render_markdown();

        assert!(!changelog.markdown.contains("ignore this commit"));
        assert!(changelog.markdown.contains("include this commit"));
    }

    #[test]
    fn parses_breaking_specific_without_category() {
        let parsed = ParsedCommit::parse("! → due to db migration: require reindex", "abc1234");

        assert!(parsed.is_breaking);
        assert_eq!(parsed.category, None);
        assert_eq!(parsed.specific.as_deref(), Some("due to db migration"));
        assert_eq!(
            parsed.message_items,
            vec![MessageItem::Text("require reindex".to_string())]
        );
    }

    #[test]
    fn keeps_later_colons_inside_the_message_body() {
        let parsed = ParsedCommit::parse("fix: bugs in the UI: render, borders", "abc1234");

        assert_eq!(parsed.category, Some(Category::Fixes));
        assert_eq!(
            parsed.message_items,
            vec![MessageItem::Text(
                "bugs in the UI: render, borders".to_string()
            )]
        );
    }

    #[test]
    fn splits_semicolons_and_builds_nested_lists() {
        let parsed = ParsedCommit::parse(
            "fix: bugs in the UI: *render *borders; auth button in settings modal",
            "abc1234",
        );

        assert_eq!(
            parsed.message_items,
            vec![
                MessageItem::NestedList {
                    intro: "bugs in the UI:".to_string(),
                    items: vec![
                        NestedListEntry {
                            level: 1,
                            text: "render".to_string(),
                        },
                        NestedListEntry {
                            level: 1,
                            text: "borders".to_string(),
                        },
                    ],
                    summary: None,
                },
                MessageItem::Text("auth button in settings modal".to_string()),
            ]
        );
    }

    #[test]
    fn parses_nested_list_modifiers_for_indentation_and_summary() {
        let parsed = ParsedCommit::parse(
            "@enh(DEMO message): Improvements: *This is major element **This indented sub-information ***This is double-sub info **One more sub-info *Another major **With this sub-info *end*This is end message to sum it up",
            "abc1234",
        );

        assert_eq!(
            parsed.message_items,
            vec![MessageItem::NestedList {
                intro: "Improvements:".to_string(),
                items: vec![
                    NestedListEntry {
                        level: 1,
                        text: "This is major element".to_string(),
                    },
                    NestedListEntry {
                        level: 2,
                        text: "This indented sub-information".to_string(),
                    },
                    NestedListEntry {
                        level: 3,
                        text: "This is double-sub info".to_string(),
                    },
                    NestedListEntry {
                        level: 2,
                        text: "One more sub-info".to_string(),
                    },
                    NestedListEntry {
                        level: 1,
                        text: "Another major".to_string(),
                    },
                    NestedListEntry {
                        level: 2,
                        text: "With this sub-info".to_string(),
                    },
                ],
                summary: Some("This is end message to sum it up".to_string()),
            }]
        );
    }

    #[test]
    fn renders_nested_list_modifiers_with_indentation_and_summary_spacing() {
        let changelog = ChangelogDocument::new(
            "v1.0.0",
            vec![ParsedCommit::parse(
                "@enh(DEMO message): Improvements: *This is major element **This indented sub-information ***This is double-sub info **One more sub-info *Another major **With this sub-info *end*This is end message to sum it up",
                "b38b72e",
            )],
        )
        .with_date(NaiveDate::from_ymd_opt(2026, 4, 23).unwrap())
        .render_markdown();

        assert!(changelog.markdown.contains("### ✨ New in DEMO message:"));
        assert!(changelog.markdown.contains("#### 💎 Enhancements"));
        assert!(changelog.markdown.contains("Improvements:   _(b38b72e)_"));
        assert!(changelog.markdown.contains("\n* This is major element"));
        assert!(
            changelog
                .markdown
                .contains("\n  * This indented sub-information")
        );
        assert!(
            changelog
                .markdown
                .contains("\n    * This is double-sub info")
        );
        assert!(changelog.markdown.contains("\n  * One more sub-info"));
        assert!(changelog.markdown.contains("\n* Another major"));
        assert!(changelog.markdown.contains("\n  * With this sub-info"));
        assert!(
            changelog
                .markdown
                .contains("\n\n  This is end message to sum it up\n")
        );
    }

    #[test]
    fn splits_semicolons_into_new_prefixed_entries() {
        let parsed = ParsedCommit::parse_many(
            "@feat(Tiles): add reset functionality for pending version; @enh(Changelog Preview): implement changelog preview handling",
            "abc1234",
        );

        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].category, Some(Category::Features));
        assert_eq!(parsed[0].specific.as_deref(), Some("Tiles"));
        assert_eq!(parsed[1].category, Some(Category::Enhancements));
        assert_eq!(parsed[1].specific.as_deref(), Some("Changelog Preview"));
    }

    #[test]
    fn parses_dotted_new_specific_heading_for_supported_categories() {
        let feature = ParsedCommit::parse("@.feat(Tiles): add reset flow", "abc1234");
        let enhancement =
            ParsedCommit::parse("@.enh(Changelog Preview): add preview actions", "abc1234");

        assert_eq!(feature.specific_heading, Some("Feature"));
        assert_eq!(feature.specific.as_deref(), Some("Tiles"));
        assert_eq!(enhancement.specific_heading, Some("Enhancement"));
        assert_eq!(enhancement.specific.as_deref(), Some("Changelog Preview"));
    }

    #[test]
    fn parses_dotted_new_specific_heading_with_internal_colon() {
        let parsed = ParsedCommit::parse(
            "@.enh(CLI & bmp: Branch b4 bmp, bmp CLI options): add overview branch bump dialog",
            "abc1234",
        );

        assert_eq!(parsed.specific_heading, Some("Enhancement"));
        assert_eq!(
            parsed.specific.as_deref(),
            Some("CLI & bmp: Branch b4 bmp, bmp CLI options")
        );
        assert_eq!(
            parsed.message_items,
            vec![MessageItem::Text(
                "add overview branch bump dialog".to_string()
            )]
        );
    }

    #[test]
    fn renders_markdown_with_breaking_new_specific_and_footer() {
        let changelog = ChangelogDocument::new(
            "v0.4.0",
            vec![
                ParsedCommit::parse("!fix: remove legacy auth flow", "a1b2c3d"),
                ParsedCommit::parse("@feat(UI): ship new dashboard", "b2c3d4e"),
                ParsedCommit::parse("enh(APP): smooth drag behavior", "c3d4e5f"),
                ParsedCommit::parse("docs: update examples", "d4e5f6a"),
            ],
        )
        .with_date(NaiveDate::from_ymd_opt(2026, 4, 12).unwrap())
        .with_release_message("Heads-up: this release updates the public dashboard.")
        .render_markdown();

        assert!(changelog.markdown.contains("## Changelog v0.4.0"));
        assert!(changelog.markdown.contains("## 💥⚠️ BREAKING CHANGE ⚠️💥"));
        assert!(changelog.markdown.contains("### ✨ New in UI:"));
        assert!(changelog.markdown.contains("#### 🧩 Features"));
        assert!(changelog.markdown.contains("### Changed in APP"));
        assert!(changelog.markdown.contains("#### 💎 Enhancement"));
        assert!(changelog.markdown.contains("### ℹ️ Documentation"));
        assert!(
            changelog
                .markdown
                .contains("Heads-up: this release updates the public dashboard.")
        );
        assert!(
            changelog
                .markdown
                .contains("... ✨ made with [ComfyGit](https://github.com/comfy-home/ComfyGit)")
        );
    }

    #[test]
    fn release_now_generator_places_previous_public_release_in_header() {
        let changelog = rls_changelog_gen(
            "v0.7.3",
            &["abc1234 fix: tighten ReleaseNOW history selection".to_string()],
            Some("v0.7.1"),
        );

        assert!(changelog.markdown.contains(
            "## Changelog v0.7.3 <sub><sup>← v0.7.1 (Previous Public Version)</sup></sub>"
        ));
        assert!(changelog.markdown.contains("2026-"));
    }

    #[test]
    fn ensure_previous_public_release_header_updates_plain_archived_release_header() {
        let markdown = [
            "## Changelog v0.11.2",
            "2026-04-22",
            "",
            "#### What's changed:",
        ]
        .join("\n");

        let updated = ensure_previous_public_release_header(&markdown, "v0.11.2", Some("v0.10.11"));

        assert!(updated.contains(
            "## Changelog v0.11.2 <sub><sup>← v0.10.11 (Previous Public Version)</sup></sub>"
        ));
        assert!(updated.contains("#### What's changed:"));
    }

    #[test]
    fn standard_and_custom_generators_use_shared_engine() {
        let lines = vec!["abc1234 feat: ship shared generator wrappers".to_string()];
        let standard = std_changelog_gen("v0.7.3", &lines);
        let custom = custom_changelog_gen("v0.7.3", &lines, Some("Custom range output."));

        assert!(standard.markdown.contains("### 🧩 Features"));
        assert!(custom.markdown.contains("Custom range output."));
    }

    #[test]
    fn parses_graph_log_lines_into_commits() {
        let parsed = parse_graph_log_line("* a1b2c3d feat(UI): ship new dashboard")
            .expect("graph line should parse");

        assert_eq!(parsed.short_hash, "a1b2c3d");
        assert_eq!(parsed.category, Some(Category::Features));
        assert_eq!(parsed.specific.as_deref(), Some("UI"));
    }

    #[test]
    fn renders_dotted_new_specific_heading() {
        let changelog = ChangelogDocument::new(
            "v0.4.0",
            vec![ParsedCommit::parse(
                "@.enh(Changelog Preview): add preview save button",
                "abc1234",
            )],
        )
        .with_date(NaiveDate::from_ymd_opt(2026, 4, 12).unwrap())
        .render_markdown();

        assert!(
            changelog
                .markdown
                .contains("### ✨ New Enhancement: Changelog Preview")
        );
    }

    #[test]
    fn renders_general_improvements_after_specific_sections() {
        let changelog = ChangelogDocument::new(
            "0.6.1",
            vec![
                ParsedCommit::parse(
                    "fix(Tiles): keep mouse-selected tile focus in sync",
                    "56dcce1",
                ),
                ParsedCommit::parse("fix: update licensing statement", "e2fa12d"),
            ],
        )
        .with_date(NaiveDate::from_ymd_opt(2026, 4, 13).unwrap())
        .render_markdown();

        let specific_index = changelog
            .markdown
            .find("### Changed in Tiles")
            .expect("specific section should render");
        let separator_index = changelog
            .markdown
            .find("\n---\n\n### 🛠️ General:")
            .expect("separator and general improvements header should render");
        let general_fix_index = changelog
            .markdown
            .rfind("### 🐛 Fix(es)")
            .expect("general fixes section should render");

        assert!(specific_index < separator_index);
        assert!(separator_index < general_fix_index);
    }

    #[test]
    fn writes_temp_changelog_to_repo_root() {
        let repo_root = std::env::temp_dir().join(format!(
            "cg-temp-changelog-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&repo_root).expect("repo root should be created");

        let output_path =
            write_temp_changelog_markdown(&repo_root.display().to_string(), "hello world")
                .expect("temp changelog should be written");

        assert_eq!(
            output_path.file_name().and_then(|name| name.to_str()),
            Some("changelog_temp.md")
        );
        assert_eq!(
            std::fs::read_to_string(&output_path).expect("temp changelog should be readable"),
            "hello world"
        );

        let _ = std::fs::remove_dir_all(repo_root);
    }

    #[test]
    fn archived_changelog_lookup_matches_tag_prefix_variants() {
        let repo_root = std::env::temp_dir().join(format!(
            "cg-history-changelog-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&repo_root).expect("repo root should be created");

        archive_changelog_markdown(&repo_root.display().to_string(), "1.2.3", "history payload")
            .expect("history changelog should be written");

        let markdown =
            find_archived_changelog_markdown(&repo_root.display().to_string(), "core-v1.2.3")
                .expect("lookup should succeed")
                .expect("history changelog should be found");
        assert_eq!(markdown, "history payload");

        let _ = std::fs::remove_dir_all(repo_root);
    }

    #[test]
    fn archived_summary_keeps_only_newest_duplicate_version() {
        let repo_root = std::env::temp_dir().join(format!(
            "cg-history-summary-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let history_dir = repo_root.join(HISTORY_DIR_NAME);
        std::fs::create_dir_all(&history_dir).expect("history dir should be created");

        std::fs::write(
            history_dir.join("20260415-101010-001-v1-2-3.md"),
            "## Changelog v1.2.3\n\nold entry\n\n".to_string() + FOOTER,
        )
        .expect("old entry should be written");
        std::fs::write(
            history_dir.join("core-v1-2-3__20260415-111111-002.md"),
            "## Changelog core-v1.2.3\n\nnew entry\n\n".to_string() + FOOTER,
        )
        .expect("new entry should be written");
        std::fs::write(
            history_dir.join("v1-2-2__20260414-090000-003.md"),
            "## Changelog v1.2.2\n\nolder version\n\n".to_string() + FOOTER,
        )
        .expect("older version should be written");

        let summary_path =
            sum_changelog_gen(&repo_root.display().to_string()).expect("summary should be written");
        let summary = std::fs::read_to_string(summary_path).expect("summary should be readable");

        assert!(summary.contains("# Changelog History"));
        assert!(summary.contains("new entry"));
        assert!(summary.contains("older version"));
        assert!(!summary.contains("old entry"));

        let _ = std::fs::remove_dir_all(repo_root);
    }

    #[test]
    fn parses_legacy_and_new_history_file_stems() {
        let legacy = parse_history_file_stem("20260415-111111-002-core-v1-2-3")
            .expect("legacy stem should parse");
        let current = parse_history_file_stem("core-v1-2-3__20260415-111111-002")
            .expect("new stem should parse");

        assert_eq!(legacy.timestamp, "20260415-111111-002");
        assert_eq!(current.timestamp, "20260415-111111-002");
        assert_eq!(legacy.label, "core-v1-2-3");
        assert_eq!(current.label, "core-v1-2-3");
    }
}
