// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyVersionBumper License v1.2
//
// For details, see the LICENSE file in the repository root.

use chrono::{Local, NaiveDate};

const FOOTER: &str = "<br>\n\n---\n... ✨ made with [CVB](https://github.com/comfy-home/perk-ComfyVersionBumper)";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Category {
	Features,
	Fixes,
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
			Self::Build => "📦 Build",
			Self::Maintenance => "🔧 Maintenance",
			Self::Enhancements => "💎 Enhancement",
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
			Self::Build => 2,
			Self::Maintenance => 3,
			Self::Enhancements => 4,
			Self::Documentation => 5,
			Self::Visuals => 6,
			Self::UiChanges => 7,
			Self::Refactor => 8,
			Self::Performance => 9,
			Self::Tests => 10,
			Self::Removed => 11,
			Self::Other => 12,
		}
	}

	pub(crate) fn from_alias(alias: &str) -> Option<Self> {
		match normalize_alias(alias).as_str() {
			"feat" | "ft" | "feature" | "element" => Some(Self::Features),
			"fix" | "bugfix" | "bf" => Some(Self::Fixes),
			"build" | "bld" | "rls" | "release" => Some(Self::Build),
			"chore" | "chores" | "depup" | "dpndc" | "dep" | "mtn" | "mtnnc"
			| "mt" | "upd" | "bump" | "bmp" => Some(Self::Maintenance),
			"enh" | "improve" | "impr" | "imp" | "improvement" | "improvements"
			| "upgrade" | "upg" => Some(Self::Enhancements),
			"docs" | "dox" | "ducu" | "documentation" => Some(Self::Documentation),
			"style" | "stl" | "vis" | "visual" | "visuals" => Some(Self::Visuals),
			"ui" | "gui" | "fe" | "frontend" => Some(Self::UiChanges),
			"ref" | "refactor" | "rfc" | "rf" | "rfb" | "refurb" | "makeover" => Some(Self::Refactor),
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
	pub(crate) is_new: bool,
	pub(crate) is_breaking: bool,
	pub(crate) message_items: Vec<MessageItem>,
}

impl ParsedCommit {
	pub(crate) fn parse(subject: &str, short_hash: impl Into<String>) -> Self {
		let raw_subject = subject.trim().to_string();
		let short_hash = short_hash.into();
		let mut remainder = raw_subject.as_str().trim();
		let mut is_breaking = false;
		let mut is_new = false;

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
			remainder = trimmed;
			break;
		}

		let (prefix, message) = split_prefix_and_message(remainder);
		let (category, specific) = parse_prefix(prefix);
		let message_items = parse_message_items(message);

		Self {
			raw_subject,
			short_hash,
			category,
			specific,
			is_new,
			is_breaking,
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
	NestedList { intro: String, items: Vec<String> },
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
	release_message: Option<String>,
	commits: Vec<ParsedCommit>,
}

impl ChangelogDocument {
	pub(crate) fn new(current_tag: impl Into<String>, commits: Vec<ParsedCommit>) -> Self {
		Self {
			current_tag: current_tag.into(),
			date: Local::now().date_naive(),
			release_message: None,
			commits,
		}
	}

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

	pub(crate) fn render_markdown(&self) -> RenderedChangelog {
		let mut lines = vec![
			format!("## Changelog {}", self.current_tag),
			self.date.format("%Y-%m-%d").to_string(),
			String::new(),
		];

		render_breaking_section(&mut lines, &self.commits);

		if let Some(release_message) = &self.release_message {
			lines.push(release_message.clone());
			lines.push(String::new());
		}

		lines.push("#### What's changed:".to_string());
		lines.push(String::new());

		let non_breaking = self
			.commits
			.iter()
			.filter(|commit| !commit.is_breaking)
			.collect::<Vec<_>>();

		render_new_specific_sections(&mut lines, &non_breaking);
		render_new_plain_section(&mut lines, &non_breaking);
		render_specific_sections(&mut lines, &non_breaking);
		render_plain_category_sections(&mut lines, &non_breaking);

		if lines.last().is_some_and(|line| !line.is_empty()) {
			lines.push(String::new());
		}
		lines.push(FOOTER.to_string());

		RenderedChangelog::new(lines.join("\n"))
	}
}

fn normalize_alias(alias: &str) -> String {
	alias
		.trim()
		.trim_matches(|ch: char| ch.is_ascii_punctuation() || ch.is_whitespace())
		.to_ascii_lowercase()
}

fn split_prefix_and_message(input: &str) -> (&str, &str) {
	match input.find(':') {
		Some(index) => (&input[..index], input[index + 1..].trim()),
		None => (input.trim(), input.trim()),
	}
}

fn parse_prefix(prefix: &str) -> (Option<Category>, Option<String>) {
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

	if let Some(open_index) = trimmed.find('(') {
		if trimmed.ends_with(')') {
			let category_part = &trimmed[..open_index];
			let specific_part = &trimmed[open_index + 1..trimmed.len() - 1];
			return (
				Category::from_alias(category_part),
				normalize_specific(specific_part),
			);
		}
	}

	(Category::from_alias(trimmed), None)
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

			if let Some((intro, items)) = parse_nested_list(segment) {
				return Some(MessageItem::NestedList { intro, items });
			}

			Some(MessageItem::Text(segment.to_string()))
		})
		.collect()
}

fn parse_nested_list(segment: &str) -> Option<(String, Vec<String>)> {
	let (intro, trailing) = segment.split_once(':')?;
	if !trailing.contains('*') {
		return None;
	}

	let items = trailing
		.split('*')
		.map(str::trim)
		.filter(|item| !item.is_empty())
		.map(ToOwned::to_owned)
		.collect::<Vec<_>>();

	if items.is_empty() {
		return None;
	}

	Some((format!("{}:", intro.trim()), items))
}

fn render_breaking_section(lines: &mut Vec<String>, commits: &[ParsedCommit]) {
	let breaking = commits.iter().filter(|commit| commit.is_breaking).collect::<Vec<_>>();
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

fn render_new_specific_sections(lines: &mut Vec<String>, commits: &[&ParsedCommit]) {
	let specific_names = ordered_specific_names(
		&commits
			.iter()
			.copied()
			.filter(|commit| commit.is_new && commit.specific.is_some())
			.collect::<Vec<_>>(),
	);

	for specific_name in specific_names {
		lines.push(format!("### ✨ New in {}:", specific_name));
		lines.push(String::new());
		let section_commits: Vec<&ParsedCommit> = commits
			.iter()
			.copied()
			.filter(|commit| commit.is_new && commit.specific.as_deref() == Some(specific_name.as_str()))
			.collect::<Vec<_>>();
		render_category_subsections(lines, &section_commits, 4);
	}
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

fn render_specific_sections(lines: &mut Vec<String>, commits: &[&ParsedCommit]) {
	let specific_names = ordered_specific_names(
		&commits
			.iter()
			.copied()
			.filter(|commit| !commit.is_new && commit.specific.is_some())
			.collect::<Vec<_>>(),
	);

	for specific_name in specific_names {
		lines.push(format!("### Changed in {}", specific_name));
		lines.push(String::new());
		let section_commits: Vec<&ParsedCommit> = commits
			.iter()
			.copied()
			.filter(|commit| !commit.is_new && commit.specific.as_deref() == Some(specific_name.as_str()))
			.collect::<Vec<_>>();
		render_category_subsections(lines, &section_commits, 4);
	}
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
		for commit in plain_commits.iter().filter(|commit| commit.effective_category() == category) {
			render_commit_bullets(lines, commit);
		}
	}
}

fn render_category_subsections<T>(lines: &mut Vec<String>, commits: &[T], heading_level: usize)
where
	T: std::borrow::Borrow<ParsedCommit>,
{
	for category in ordered_categories(commits) {
		lines.push(format!("{} {}", "#".repeat(heading_level), category.heading()));
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
				lines.push(format!("* {} `{}`", text, commit.short_hash));
			}
			MessageItem::NestedList { intro, items } => {
				lines.push(format!("* {} `{}`", intro, commit.short_hash));
				for item in items {
					lines.push(format!("  * {}", item));
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
		if let Some(name) = &commit.specific {
			if !names.iter().any(|existing| existing == name) {
				names.push(name.clone());
			}
		}
	}
	names
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn parses_alias_and_specific_group_with_arrow() {
		let parsed = ParsedCommit::parse("feat → Phase 1: add parsing pipeline", "abc1234");

		assert_eq!(parsed.category, Some(Category::Features));
		assert_eq!(parsed.specific.as_deref(), Some("Phase 1"));
		assert!(!parsed.is_new);
		assert!(!parsed.is_breaking);
		assert_eq!(parsed.message_items, vec![MessageItem::Text("add parsing pipeline".to_string())]);
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
	fn parses_breaking_specific_without_category() {
		let parsed = ParsedCommit::parse("! → due to db migration: require reindex", "abc1234");

		assert!(parsed.is_breaking);
		assert_eq!(parsed.category, None);
		assert_eq!(parsed.specific.as_deref(), Some("due to db migration"));
		assert_eq!(parsed.message_items, vec![MessageItem::Text("require reindex".to_string())]);
	}

	#[test]
	fn keeps_later_colons_inside_the_message_body() {
		let parsed = ParsedCommit::parse("fix: bugs in the UI: render, borders", "abc1234");

		assert_eq!(parsed.category, Some(Category::Fixes));
		assert_eq!(parsed.message_items, vec![MessageItem::Text("bugs in the UI: render, borders".to_string())]);
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
					items: vec!["render".to_string(), "borders".to_string()],
				},
				MessageItem::Text("auth button in settings modal".to_string()),
			]
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
		assert!(changelog.markdown.contains("Heads-up: this release updates the public dashboard."));
		assert!(changelog.markdown.contains("... ✨ made with [CVB](https://github.com/comfy-home/perk-ComfyVersionBumper)"));
	}
}