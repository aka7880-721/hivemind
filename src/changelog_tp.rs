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

use std::collections::HashMap;

use crate::changelog::ParsedCommit;

/// Represents a single Top Pick entry
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TopPick {
    /// Priority within Top Picks section (1-20, higher = higher position)
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TopPickBullet {
    pub level: usize, // 1 = **, 2 = ***
    pub text: String,
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

/// Extract Top Picks from parsed commits
pub(crate) fn extract_top_picks(commits: &[&ParsedCommit]) -> Vec<TopPick> {
    let mut picks = Vec::new();
    let mut priority_headers: HashMap<u8, String> = HashMap::new();

    // First pass: collect all top picks and track priority->header mappings
    for commit in commits {
        if let Some((priority, items)) = parse_top_pick_from_commit(commit) {
            let header = extract_header(&items);
            if let Some(p) = priority {
                priority_headers.insert(p, header.clone());
            }

            let bullets = extract_bullets(&items);
            picks.push(TopPick {
                priority,
                header,
                bullets,
                commit_hash: commit.short_hash.clone(),
                is_reference: false,
            });
        }
    }

    // Handle cross-references: commits that reference an existing priority
    for commit in commits.iter().rev() {
        if let Some((priority, items)) = parse_top_pick_reference(commit)
            && let Some(existing_header) = priority_headers.get(&priority)
        {
            // Find existing pick and merge bullets
            if let Some(existing) = picks.iter_mut().find(|p| {
                p.priority == Some(priority) && &p.header == existing_header && !p.is_reference
            }) {
                let new_bullets = extract_bullets(&items);
                existing.bullets.extend(new_bullets);
            } else {
                // Create a reference pick if original not found
                let bullets = extract_bullets(&items);
                picks.push(TopPick {
                    priority: Some(priority),
                    header: existing_header.clone(),
                    bullets,
                    commit_hash: commit.short_hash.clone(),
                    is_reference: true,
                });
            }
        }
    }

    picks
}

/// Check if this commit is a top pick and extract priority and items
fn parse_top_pick_from_commit(
    commit: &ParsedCommit,
) -> Option<(Option<u8>, Vec<crate::changelog::MessageItem>)> {
    if commit.is_top_pick_config {
        // Determine priority from the category prefix
        let priority = commit.top_pick_priority;
        Some((priority, commit.message_items.clone()))
    } else {
        None
    }
}

/// Check if this commit references an existing top pick priority
fn parse_top_pick_reference(
    commit: &ParsedCommit,
) -> Option<(u8, Vec<crate::changelog::MessageItem>)> {
    if commit.is_top_pick_reference {
        commit
            .top_pick_priority
            .map(|p| (p, commit.message_items.clone()))
    } else {
        None
    }
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
                bullets.push(TopPickBullet {
                    level: entry.level.saturating_sub(1), // convert 1->0, 2->1, 3->2
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
pub(crate) fn render_top_picks_section(picks: &[TopPick]) -> Vec<String> {
    let mut lines = Vec::new();

    if picks.is_empty() {
        return lines;
    }

    // Header
    lines.push("### 💥 💥 💥 This Release's Top Picks ...  💥 💥 💥".to_string());
    lines.push(String::new());

    // Numbered entries
    for (index, pick) in picks.iter().enumerate() {
        let number = index + 1;
        lines.push(format!(
            "#### **{}. &nbsp;&nbsp;&nbsp;{}**",
            number, pick.header
        ));
        lines.push(String::new());

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

/// Render bullets hierarchically
fn render_bullets_hierarchical(lines: &mut Vec<String>, bullets: &[TopPickBullet]) {
    if bullets.is_empty() {
        return;
    }

    // Group consecutive bullets of the same level
    let mut i = 0;
    while i < bullets.len() {
        let bullet = &bullets[i];

        if bullet.level == 0 {
            // Top-level bullet
            lines.push(format!("- {}", bullet.text));
        } else {
            // Nested bullet
            let indent = "    ".repeat(bullet.level);
            lines.push(format!("{}- {}", indent, bullet.text));
        }

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
                    level: 1,
                    text: "Level 1 item".to_string(),
                },
                NestedListEntry {
                    level: 2,
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

        let lines = render_top_picks_section(&picks);
        let output = lines.join("\n");

        assert!(output.contains("This Release's Top Picks"));
        assert!(output.contains("1."));
        assert!(output.contains("First improvement"));
        assert!(output.contains("- Contains this"));
        assert!(output.contains("🎉 Enjoy!"));
    }
}
