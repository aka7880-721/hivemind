// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit License
// For details, see the LICENSE file in the repository root.

//! Top Picks changelog feature - allows users to highlight significant improvements
//! via `top{priority}:` prefixed commit messages.
//!
//! The hierarchy (higher priority = higher position):
//! - QuickDownloads (950) - if enabled and Position is "Top"
//! - ! (850) - breaking changes
//! - TopPicks (825) - this feature
//! - @. (800) - dotted new feat/enh announcement
//! - @ (700) - new
//! - Category(Specific) (650) - e.g. `enh(Git)`
//! - Category (500) - plain category
//! - QuickDownloads (100) - if enabled and Position is "Bottom"

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::changelog::ParsedCommit;
use tui_textarea::TextArea as TuiTextArea;

/// Represents a single Top Pick entry
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct TopPick {
    /// Priority within Top Picks section (1-20, 1 = highest position)
    pub priority: Option<u8>,
    /// The header text (from * in message)
    pub header: String,
    /// Bullet points (from ** and *** in message)
    pub bullets: Vec<TopPickBullet>,
    /// Original commit hash (for reference, not displayed)
    pub commit_hash: String,
    /// Whether this was added by referencing an existing priority
    pub is_reference: bool,
}

/// A bullet point within a Top Pick entry
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct TopPickBullet {
    pub level: usize, // 1 = **, 2 = ***
    pub text: String,
}

/// Optional introduction block for the Top Picks section (editor `INTRO:` prefix).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct TopPicksIntro {
    pub lines: Vec<TopPicksIntroLine>,
}

/// A single line in the Top Picks introduction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TopPicksIntroLine {
    Text(String),
    Bullet(String),
}

/// Priority values for section ordering (documented for reference)
#[allow(dead_code)]
pub(crate) const PRIORITY_QUICK_DOWNLOADS_TOP: u16 = 950;
#[allow(dead_code)]
pub(crate) const PRIORITY_BREAKING: u16 = 850;
#[allow(dead_code)]
pub(crate) const PRIORITY_TOP_PICKS: u16 = 825;
#[allow(dead_code)]
pub(crate) const PRIORITY_DOTTED_NEW: u16 = 800;
#[allow(dead_code)]
pub(crate) const PRIORITY_NEW: u16 = 700;
#[allow(dead_code)]
pub(crate) const PRIORITY_SPECIFIC_CATEGORY: u16 = 650;
#[allow(dead_code)]
pub(crate) const PRIORITY_PLAIN_CATEGORY: u16 = 500;
#[allow(dead_code)]
pub(crate) const PRIORITY_QUICK_DOWNLOADS_BOTTOM: u16 = 100;

/// Leading spaces before `-` that mark a nested bullet in editor / `.tp_edits.md` text.
pub(crate) const TOP_PICK_NESTED_INDENT_SPACES: usize = 2;

fn top_pick_nested_indent(level: usize) -> &'static str {
    if level == 0 {
        ""
    } else {
        "  "
    }
}

fn top_pick_indent_level(leading_spaces: usize) -> usize {
    if leading_spaces >= TOP_PICK_NESTED_INDENT_SPACES {
        1
    } else {
        0
    }
}

/// Extract Top Picks from parsed commits
pub(crate) fn extract_top_picks(commits: &[&ParsedCommit]) -> Vec<TopPick> {
    // Collect all top picks (both headers with * and bullets-only with **)
    let mut all_picks: Vec<TopPick> = Vec::new();

    for commit in commits {
        if !commit.is_top_pick_config && !commit.is_top_pick_reference {
            continue;
        }

        let items = &commit.message_items;
        let header = extract_header(items);
        let bullets = extract_bullets(items);

        all_picks.push(TopPick {
            priority: commit.top_pick_priority,
            header,
            bullets,
            commit_hash: commit.short_hash.clone(),
            is_reference: commit.is_top_pick_reference,
        });
    }

    // Merge picks by priority: if one has header and one doesn't, merge bullets into header
    let mut merged: HashMap<u8, TopPick> = HashMap::new();

    for pick in all_picks {
        if let Some(priority) = pick.priority {
            match merged.entry(priority) {
                std::collections::hash_map::Entry::Occupied(mut entry) => {
                    let existing = entry.get_mut();
                    // If this pick has a header and existing doesn't, use this header
                    if !pick.header.is_empty() && existing.header.is_empty() {
                        existing.header = pick.header;
                    }
                    // If existing has header and this doesn't, just add bullets
                    // If both have headers (shouldn't happen), keep existing and add bullets
                    existing.bullets.extend(pick.bullets);
                }
                std::collections::hash_map::Entry::Vacant(entry) => {
                    entry.insert(pick);
                }
            }
        }
    }

    let mut picks: Vec<TopPick> = merged.into_values().collect();
    // Sort by priority
    picks.sort_by_key(|a| a.priority);
    picks
}

/// Merge commit-based Top Picks with manual edits from memory file
/// Memory file edits take precedence over commit-based picks (by slot/priority,
/// with a normalized-header fallback for minor header tweaks).
pub(crate) fn merge_top_picks_with_edits(
    commit_picks: Vec<TopPick>,
    edits_content: &str,
) -> (Vec<TopPick>, Option<TopPicksIntro>) {
    if edits_content.trim().is_empty() {
        return (commit_picks, None);
    }

    let (intro, picks_text) = split_intro_and_picks_text(edits_content);

    // Parse the edits content into picks
    let edited_picks = TopPicksEditorDialog::text_to_picks(&picks_text);

    if edited_picks.is_empty() {
        if intro.is_some() {
            return (commit_picks, intro);
        }
        return (commit_picks, None);
    }

    // Merge: edited picks take precedence by priority slot. Header matching is
    // retained as a fallback so cosmetic header changes still collapse.
    let seen_priorities: HashSet<u8> = edited_picks.iter().filter_map(|p| p.priority).collect();
    let seen_headers: HashSet<String> = edited_picks
        .iter()
        .map(|p| normalized_header_key(&p.header))
        .filter(|key| !key.is_empty())
        .collect();

    let mut result = edited_picks;

    // Add commit picks that don't have a matching edited slot/header
    for pick in commit_picks {
        let priority_overridden = pick
            .priority
            .is_some_and(|priority| seen_priorities.contains(&priority));
        let header_overridden = seen_headers.contains(&normalized_header_key(&pick.header));
        if !priority_overridden && !header_overridden {
            result.push(pick);
        }
    }

    // Sort by priority
    sort_top_picks(&mut result);
    (result, intro)
}

/// Split editor text into an optional `INTRO:` block and the remaining pick lines.
pub(crate) fn split_intro_and_picks_text(text: &str) -> (Option<TopPicksIntro>, String) {
    let mut intro_lines: Vec<TopPicksIntroLine> = Vec::new();
    let mut pick_lines: Vec<String> = Vec::new();
    let mut in_intro = false;
    let mut intro_started = false;

    for line in text.lines() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with("//") {
            if !intro_started {
                continue;
            }
            if in_intro {
                continue;
            }
            pick_lines.push(line.to_string());
            continue;
        }

        if !intro_started {
            if let Some(rest) = intro_marker_body(trimmed) {
                intro_started = true;
                in_intro = true;
                if !rest.is_empty() {
                    intro_lines.push(TopPicksIntroLine::Text(rest.to_string()));
                }
                continue;
            }
        } else if in_intro {
            if TopPicksEditorDialog::parse_header_line(trimmed).is_some() {
                in_intro = false;
                pick_lines.push(line.to_string());
                continue;
            }
            if let Some(text) = trimmed.strip_prefix('-') {
                let bullet = text.trim_start();
                if !bullet.is_empty() {
                    intro_lines.push(TopPicksIntroLine::Bullet(bullet.to_string()));
                }
                continue;
            }
            intro_lines.push(TopPicksIntroLine::Text(trimmed.to_string()));
            continue;
        }

        pick_lines.push(line.to_string());
    }

    let intro = if intro_lines.is_empty() {
        None
    } else {
        Some(TopPicksIntro { lines: intro_lines })
    };
    (intro, pick_lines.join("\n"))
}

fn intro_marker_body(trimmed: &str) -> Option<&str> {
    let upper = trimmed.to_ascii_uppercase();
    if upper == "INTRO:" {
        return Some("");
    }
    if upper.starts_with("INTRO:") {
        return Some(trimmed[6..].trim_start());
    }
    None
}

fn normalized_header_key(header: &str) -> String {
    let mut normalized = String::new();
    let mut previous_was_space = false;

    for ch in header.chars() {
        if ch.is_alphanumeric() {
            normalized.extend(ch.to_lowercase());
            previous_was_space = false;
        } else if ch.is_whitespace() && !normalized.is_empty() && !previous_was_space {
            normalized.push(' ');
            previous_was_space = true;
        }
    }

    normalized.trim().to_string()
}

/// Extract header from message items (text before first ** or ***)
fn extract_header(items: &[crate::changelog::MessageItem]) -> String {
    if let Some(item) = items.iter().next() {
        match item {
            crate::changelog::MessageItem::Text(text) => {
                return text.trim().to_string();
            }
            crate::changelog::MessageItem::NestedList { intro, .. } => {
                return intro.trim().trim_end_matches(':').to_string();
            }
        }
    }
    "Untitled".to_string()
}

/// Extract bullet points from message items
fn extract_bullets(items: &[crate::changelog::MessageItem]) -> Vec<TopPickBullet> {
    let mut bullets = Vec::new();

    for item in items {
        if let crate::changelog::MessageItem::NestedList { items, .. } = item {
            for entry in items {
                // parse_top_pick_message uses ** → 2, *** → 3 (same marker levels as feat nested lists).
                // First bullets must render as column-0 list items under the h4 (not indented).
                bullets.push(TopPickBullet {
                    level: entry.level.saturating_sub(2),
                    text: entry.text.clone(),
                });
            }
        }
    }

    bullets
}

/// Sort top picks by priority (lower number = higher position), then alphabetically
/// Priority 1 comes first, then 2, then 3, etc. Unprioritized picks come last.
pub(crate) fn sort_top_picks(picks: &mut [TopPick]) {
    picks.sort_by(|a, b| {
        // First compare by priority (lower number = first)
        match (a.priority, b.priority) {
            (Some(ap), Some(bp)) => ap.cmp(&bp),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        }
        .then_with(|| {
            // Then alphabetically by header
            a.header.cmp(&b.header)
        })
    });
}

/// Render Top Picks section as markdown
pub(crate) fn render_top_picks_section(
    picks: &[TopPick],
    intro: Option<&TopPicksIntro>,
) -> Vec<String> {
    let mut lines = Vec::new();

    if picks.is_empty() && intro.is_none() {
        return lines;
    }

    // Header
    lines.push("### 💥 💥 💥 This Release's Top Picks ...  💥 💥 💥".to_string());
    lines.push(String::new());

    if let Some(intro) = intro {
        lines.extend(render_top_picks_intro(intro));
    }

    // Numbered entries
    for (index, pick) in picks.iter().enumerate() {
        let number = index + 1;
        lines.push(format!(
            "#### **{}. &nbsp;&nbsp;&nbsp;{}**",
            number, pick.header
        ));

        // Group bullets by level for hierarchical rendering
        render_bullets_hierarchical(&mut lines, &pick.bullets);
    }

    // Footer
    lines.push(String::new());
    lines.push("<sub>...  🎉 Enjoy!</sub>".to_string());
    lines.push(String::new());
    lines.push("<br>".to_string());
    lines.push(String::new());

    lines
}

fn render_top_picks_intro(intro: &TopPicksIntro) -> Vec<String> {
    let mut lines = vec!["<sup>💬 Intro:</sup>  ".to_string()];
    for line in &intro.lines {
        match line {
            TopPicksIntroLine::Text(text) => {
                lines.push(format!("<sup>_{}_</sup>  ", text));
            }
            TopPicksIntroLine::Bullet(text) => {
                lines.push(format!("<sup>_- {}_</sup>", text));
            }
        }
    }
    lines.push(String::new());
    lines
}

/// Render bullets hierarchically
fn render_bullets_hierarchical(lines: &mut Vec<String>, bullets: &[TopPickBullet]) {
    if bullets.is_empty() {
        return;
    }

    // Group consecutive bullets of the same level
    let mut i = 0;
    while i < bullets.len() {
        let bullet = &bullets[i];

        // Level 0 = ** (first bullet level) -> no indent; level 1 = *** (nested) -> 2 spaces
        let indent = top_pick_nested_indent(bullet.level);
        lines.push(format!("{}- {}", indent, bullet.text));

        i += 1;
    }

    // Add spacing after bullets
    if !lines.is_empty() && !lines.last().unwrap().is_empty() {
        lines.push(String::new());
    }
}

/// Check if a commit message is a top pick config (top/top{priority})
pub(crate) fn is_top_pick_config_prefix(prefix: &str) -> Option<u8> {
    let normalized = prefix.trim().to_ascii_lowercase();

    if normalized == "top" {
        return Some(0); // No explicit priority
    }

    if let Some(num_part) = normalized.strip_prefix("top")
        && let Ok(priority) = num_part.parse::<u8>()
        && (1..=20).contains(&priority)
    {
        return Some(priority);
    }

    None
}

/// Check if a commit should be excluded from standard changelog (only appears in Top Picks)
pub(crate) fn is_top_pick_only_commit(commit: &ParsedCommit) -> bool {
    commit.is_top_pick_config || commit.is_top_pick_reference
}

/// Dialog for editing Top Picks with a text area editor
pub(crate) struct TopPicksEditorDialog {
    pub editor: TuiTextArea<'static>,
    pub placeholder: String,
}

impl TopPicksEditorDialog {
    /// Create a new editor dialog with existing top picks
    pub fn with_picks(picks: &[TopPick]) -> Self {
        let text = Self::picks_to_text(picks);
        Self::with_text(&text)
    }

    /// Create a new editor dialog with raw text content
    pub fn with_text(text: &str) -> Self {
        let editor_text = if text.trim().is_empty() {
            // Provide template for new users
            "// Top Picks - highlight key features for this release\n\nINTRO:\nThis release focuses on:\n- key theme one\n- key theme two\n\n1. First key feature or improvement\n- What this does for users\n- Why it matters\n  - Technical detail if needed\n\n2. Second highlight\n- Bullet describing the benefit\n\n// Lines starting with // are ignored\n// INTRO: block is optional; use '1. Header' then '- Bullet' (indent nested lines with 2+ spaces)"
                .lines()
                .collect::<Vec<_>>()
        } else {
            text.lines().collect::<Vec<_>>()
        };
        let mut editor = TuiTextArea::from(editor_text);
        editor.set_placeholder_text("Define Top Picks using the format:\nINTRO:\nOptional intro line\n- Optional intro bullet\n\n1. Header text\n- Bullet point\n  - Nested bullet (2+ spaces)\n\n2. Another header\n- Another bullet\n\n// Lines starting with // are ignored");
        editor.set_tab_length(2);
        editor.set_max_histories(100);
        Self {
            editor,
            placeholder: "Edit Top Picks in Markdown format".to_string(),
        }
    }

    /// Convert TopPicks to editable text format
    fn picks_to_text(picks: &[TopPick]) -> String {
        if picks.is_empty() {
            return String::new();
        }

        let mut lines = Vec::new();
        for (index, pick) in picks.iter().enumerate() {
            let number = index + 1;
            lines.push(format!("{}. {}", number, pick.header));

            for bullet in &pick.bullets {
                let indent = top_pick_nested_indent(bullet.level);
                lines.push(format!("{}- {}", indent, bullet.text));
            }

            // Add blank line between picks (except after last)
            if index < picks.len().saturating_sub(1) {
                lines.push(String::new());
            }
        }

        lines.join("\n")
    }

    /// Parse text format into TopPicks
    pub(crate) fn text_to_picks(text: &str) -> Vec<TopPick> {
        let mut picks = Vec::new();
        let mut current_pick: Option<TopPick> = None;

        for line in text.lines() {
            let trimmed = line.trim_start();

            // Skip empty lines and comments (lines starting with //)
            if trimmed.is_empty() || trimmed.starts_with("//") {
                continue;
            }

            // Check for header line (starts with number followed by .)
            if let Some((priority, header)) = Self::parse_header_line(trimmed) {
                // Save previous pick if exists
                if let Some(pick) = current_pick.take() {
                    picks.push(pick);
                }
                // Start new pick
                current_pick = Some(TopPick {
                    priority: Some(priority),
                    header,
                    bullets: Vec::new(),
                    commit_hash: String::new(),
                    is_reference: false,
                });
            } else if let Some(bullet) = Self::parse_bullet_line(line) {
                // Add bullet to current pick
                if let Some(ref mut pick) = current_pick {
                    pick.bullets.push(bullet);
                }
            }
        }

        // Don't forget the last pick
        if let Some(pick) = current_pick {
            picks.push(pick);
        }

        picks
    }

    /// Parse a header line like "1. Header text" or "1.   Header text"
    fn parse_header_line(line: &str) -> Option<(u8, String)> {
        // Match pattern: number followed by . and optional whitespace
        let mut chars = line.chars().peekable();
        let mut digits = String::new();

        // Skip digits
        while chars.peek().is_some_and(|c| c.is_ascii_digit()) {
            digits.push(chars.next()?);
        }
        let priority = digits.parse::<u8>().ok().filter(|value| *value > 0)?;

        // Check for .
        if chars.next() != Some('.') {
            return None;
        }

        // Skip whitespace
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }

        // Rest is the header
        let header: String = chars.collect();
        if header.is_empty() {
            return None;
        }

        Some((priority, header))
    }

    /// Parse a bullet line like "- Bullet text" or "    - Nested bullet"
    fn parse_bullet_line(line: &str) -> Option<TopPickBullet> {
        let leading_spaces = line.chars().take_while(|c| *c == ' ').count();
        let trimmed = line.trim_start();

        if !trimmed.starts_with("-") {
            return None;
        }

        // Skip "-" and whitespace
        let text = trimmed[1..].trim_start().to_string();
        if text.is_empty() {
            return None;
        }

        // Level 0 = top-level (`-`); level 1 = nested (`  -` or `    -`, etc.)
        let level = top_pick_indent_level(leading_spaces);

        Some(TopPickBullet { level, text })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::changelog::{MessageItem, NestedListEntry};

    #[test]
    fn detects_top_pick_prefixes() {
        assert_eq!(is_top_pick_config_prefix("top"), Some(0));
        assert_eq!(is_top_pick_config_prefix("top1"), Some(1));
        assert_eq!(is_top_pick_config_prefix("top5"), Some(5));
        assert_eq!(is_top_pick_config_prefix("top20"), Some(20));
        assert_eq!(is_top_pick_config_prefix("top21"), None); // Out of range
        assert_eq!(is_top_pick_config_prefix("feat"), None);
        assert_eq!(is_top_pick_config_prefix("fix"), None);
    }

    #[test]
    fn extracts_header_from_text_item() {
        let items = vec![MessageItem::Text("This is a header".to_string())];
        assert_eq!(extract_header(&items), "This is a header");
    }

    #[test]
    fn extracts_header_from_nested_list() {
        let items = vec![MessageItem::NestedList {
            intro: "Header:".to_string(),
            items: vec![],
            summary: None,
        }];
        assert_eq!(extract_header(&items), "Header");
    }

    #[test]
    fn extracts_bullets_from_nested_list() {
        let items = vec![MessageItem::NestedList {
            intro: "Header:".to_string(),
            items: vec![
                NestedListEntry {
                    level: 2,
                    text: "Level 1 item".to_string(),
                },
                NestedListEntry {
                    level: 3,
                    text: "Level 2 item".to_string(),
                },
            ],
            summary: None,
        }];

        let bullets = extract_bullets(&items);
        assert_eq!(bullets.len(), 2);
        assert_eq!(bullets[0].level, 0);
        assert_eq!(bullets[0].text, "Level 1 item");
        assert_eq!(bullets[1].level, 1);
        assert_eq!(bullets[1].text, "Level 2 item");
    }

    #[test]
    fn sorts_picks_by_priority_then_alphabetically() {
        let mut picks = vec![
            TopPick {
                priority: None,
                header: "Zebra".to_string(),
                bullets: vec![],
                commit_hash: "a".to_string(),
                is_reference: false,
            },
            TopPick {
                priority: Some(5),
                header: "Apple".to_string(),
                bullets: vec![],
                commit_hash: "b".to_string(),
                is_reference: false,
            },
            TopPick {
                priority: None,
                header: "Alpha".to_string(),
                bullets: vec![],
                commit_hash: "c".to_string(),
                is_reference: false,
            },
            TopPick {
                priority: Some(10),
                header: "Banana".to_string(),
                bullets: vec![],
                commit_hash: "d".to_string(),
                is_reference: false,
            },
        ];

        sort_top_picks(&mut picks);

        // Lower priority first (1 comes before 5), then alphabetical for same/no priority
        assert_eq!(picks[0].priority, Some(5));
        assert_eq!(picks[0].header, "Apple");
        assert_eq!(picks[1].priority, Some(10));
        assert_eq!(picks[1].header, "Banana");
        assert_eq!(picks[2].priority, None);
        assert_eq!(picks[2].header, "Alpha");
        assert_eq!(picks[3].priority, None);
        assert_eq!(picks[3].header, "Zebra");
    }

    #[test]
    fn same_priority_sorted_alphabetically() {
        let mut picks = vec![
            TopPick {
                priority: Some(5),
                header: "Zebra".to_string(),
                bullets: vec![],
                commit_hash: "a".to_string(),
                is_reference: false,
            },
            TopPick {
                priority: Some(5),
                header: "Apple".to_string(),
                bullets: vec![],
                commit_hash: "b".to_string(),
                is_reference: false,
            },
        ];

        sort_top_picks(&mut picks);

        assert_eq!(picks[0].header, "Apple");
        assert_eq!(picks[1].header, "Zebra");
    }

    #[test]
    fn renders_top_picks_section() {
        let picks = vec![TopPick {
            priority: Some(1),
            header: "First improvement".to_string(),
            bullets: vec![TopPickBullet {
                level: 0,
                text: "Contains this".to_string(),
            }],
            commit_hash: "abc".to_string(),
            is_reference: false,
        }];

        let lines = render_top_picks_section(&picks, None);
        let output = lines.join("\n");

        assert!(output.contains("This Release's Top Picks"));
        assert!(output.contains("1."));
        assert!(output.contains("First improvement"));
        assert!(output.contains("- Contains this"));
        assert!(output.contains("🎉 Enjoy!"));
    }

    /// First `**` bullets must not be indented; no blank line between h4 and list (valid CommonMark).
    #[test]
    fn top_picks_h4_immediately_followed_by_top_level_list() {
        let picks = vec![TopPick {
            priority: Some(1),
            header: "This is first huge improvement".to_string(),
            bullets: vec![TopPickBullet {
                level: 0,
                text: "Contains this".to_string(),
            }],
            commit_hash: "abc".to_string(),
            is_reference: false,
        }];
        let lines = render_top_picks_section(&picks, None);
        let h4 = lines
            .iter()
            .position(|l| l.starts_with("#### **1."))
            .expect("numbered top pick heading");
        assert!(
            h4 + 1 < lines.len()
                && lines[h4 + 1] == "- Contains this"
                && !lines[h4 + 1].starts_with(' '),
            "expected `- Contains this` directly after h4, got: {:?}",
            lines.get(h4..(h4 + 3).min(lines.len()))
        );
    }

    #[test]
    fn merge_top_picks_edits_override_matching_headers() {
        let commit_picks = vec![
            TopPick {
                priority: Some(1),
                header: "Important improvement".to_string(),
                bullets: vec![TopPickBullet {
                    level: 0,
                    text: "Original bullet".to_string(),
                }],
                commit_hash: "abc".to_string(),
                is_reference: false,
            },
            TopPick {
                priority: Some(3),
                header: "Commit-only pick".to_string(),
                bullets: vec![TopPickBullet {
                    level: 0,
                    text: "Still included".to_string(),
                }],
                commit_hash: "def".to_string(),
                is_reference: false,
            },
        ];

        let (merged, intro) = merge_top_picks_with_edits(
            commit_picks,
            "1. Important improvement\n- Edited bullet\n\n2. Manual pick\n- Added from editor",
        );

        assert!(intro.is_none());
        assert_eq!(merged.len(), 3);
        assert_eq!(merged[0].header, "Important improvement");
        assert_eq!(merged[0].bullets[0].text, "Edited bullet");
        assert!(merged.iter().any(|pick| pick.header == "Manual pick"));
        assert!(merged.iter().any(|pick| pick.header == "Commit-only pick"));
    }

    #[test]
    fn merge_top_picks_edits_override_same_priority_when_header_changes() {
        let commit_picks = vec![
            TopPick {
                priority: Some(1),
                header: "TOP PICKS EDITOR!".to_string(),
                bullets: vec![TopPickBullet {
                    level: 0,
                    text: "Original bullet".to_string(),
                }],
                commit_hash: "abc".to_string(),
                is_reference: false,
            },
            TopPick {
                priority: Some(2),
                header: "Auto-README changelog injection!".to_string(),
                bullets: vec![TopPickBullet {
                    level: 0,
                    text: "Second item".to_string(),
                }],
                commit_hash: "def".to_string(),
                is_reference: false,
            },
        ];

        let (merged, _) = merge_top_picks_with_edits(
            commit_picks,
            "1. TOP PICKS EDITOR! ⭐\n- Edited bullet\n\n2. Auto-README changelog injection!\n- Second item",
        );

        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].header, "TOP PICKS EDITOR! ⭐");
        assert_eq!(merged[0].bullets[0].text, "Edited bullet");
        assert_eq!(merged[1].header, "Auto-README changelog injection!");
    }

    #[test]
    fn text_to_picks_preserves_explicit_priority_numbers() {
        let picks = TopPicksEditorDialog::text_to_picks("3. Reordered item\n- Bullet");

        assert_eq!(picks.len(), 1);
        assert_eq!(picks[0].priority, Some(3));
        assert_eq!(picks[0].header, "Reordered item");
    }

    #[test]
    fn split_intro_and_picks_text_parses_intro_block() {
        let text = "INTRO:\nThis release focuses on these elements:\n- render\n- bugfixes\n\n1. Rendering enhancements\n- bullet";
        let (intro, picks_text) = split_intro_and_picks_text(text);
        let intro = intro.expect("intro");
        assert_eq!(intro.lines.len(), 3);
        assert!(matches!(intro.lines[0], TopPicksIntroLine::Text(_)));
        assert!(matches!(intro.lines[1], TopPicksIntroLine::Bullet(_)));
        assert!(matches!(intro.lines[2], TopPicksIntroLine::Bullet(_)));
        let picks = TopPicksEditorDialog::text_to_picks(&picks_text);
        assert_eq!(picks.len(), 1);
        assert_eq!(picks[0].header, "Rendering enhancements");
    }

    #[test]
    fn render_top_picks_section_includes_intro_sup_lines() {
        let intro = TopPicksIntro {
            lines: vec![
                TopPicksIntroLine::Text("This release focuses on these elements:".to_string()),
                TopPicksIntroLine::Bullet("render".to_string()),
                TopPicksIntroLine::Bullet("bugfixes".to_string()),
            ],
        };
        let picks = vec![TopPick {
            priority: Some(1),
            header: "Rendering enhancements".to_string(),
            bullets: vec![],
            commit_hash: String::new(),
            is_reference: false,
        }];
        let output = render_top_picks_section(&picks, Some(&intro)).join("\n");
        assert!(output.contains("<sup>💬 Intro:</sup>"));
        assert!(output.contains("<sup>_This release focuses on these elements:_</sup>"));
        assert!(output.contains("<sup>_- render_</sup>"));
        assert!(output.contains("<sup>_- bugfixes_</sup>"));
        assert!(output.contains("#### **1."));
        assert!(output.contains("Rendering enhancements"));
    }

    #[test]
    fn merge_top_picks_with_edits_preserves_intro() {
        let (merged, intro) =
            merge_top_picks_with_edits(vec![], "INTRO:\nRelease theme\n\n1. Manual only\n- bullet");
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].header, "Manual only");
        let intro = intro.expect("intro");
        assert_eq!(intro.lines.len(), 1);
    }

    #[test]
    fn parse_bullet_line_accepts_two_or_four_space_nested_indent() {
        let two = TopPicksEditorDialog::parse_bullet_line("  - nested two")
            .expect("two-space nested");
        assert_eq!(two.level, 1);
        assert_eq!(two.text, "nested two");

        let four = TopPicksEditorDialog::parse_bullet_line("    - nested four")
            .expect("four-space nested");
        assert_eq!(four.level, 1);

        let top = TopPicksEditorDialog::parse_bullet_line("- top level").expect("top");
        assert_eq!(top.level, 0);
    }

    #[test]
    fn text_to_picks_round_trip_nested_two_space_indent() {
        let text = "1. Feature\n- parent\n  - child";
        let picks = TopPicksEditorDialog::text_to_picks(text);
        assert_eq!(picks.len(), 1);
        assert_eq!(picks[0].bullets.len(), 2);
        assert_eq!(picks[0].bullets[0].level, 0);
        assert_eq!(picks[0].bullets[1].level, 1);

        let rendered = TopPicksEditorDialog::picks_to_text(&picks);
        assert!(rendered.contains("- parent"));
        assert!(rendered.contains("  - child"));
    }

    #[test]
    fn render_nested_bullets_with_two_space_indent() {
        let picks = vec![TopPick {
            priority: Some(1),
            header: "Header".to_string(),
            bullets: vec![
                TopPickBullet {
                    level: 0,
                    text: "parent".to_string(),
                },
                TopPickBullet {
                    level: 1,
                    text: "child".to_string(),
                },
            ],
            commit_hash: String::new(),
            is_reference: false,
        }];
        let lines = render_top_picks_section(&picks, None);
        assert!(lines.iter().any(|line| line == "  - child"));
    }
}
