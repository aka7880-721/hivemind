// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit License
//
// For details, see the LICENSE file in the repository root.

//! README "What's new" injection for ReleaseNOW.
//!
//! Inserts a `<details>` block into the project's README.md at a configured
//! row.  The block is prepended to any existing content at that row (i.e. the
//! injected block occupies lines starting at `inject_at_row` and all previous
//! content shifts down).
//!
//! TopPicks awareness: when the changelog markdown contains a TopPicks section
//! (detected by the `### 💥` heading), only that section is injected and a
//! "For detailed changelog CLICK HERE" line pointing to the specific GitHub
//! release page is appended before the footer.

use std::{fs, path::Path};

use anyhow::{Context, Result, bail};

const COMFYGIT_LINK: &str = "https://github.com/comfy-home/ComfyGit";
const TOP_PICKS_HEADING_PREFIX: &str = "### 💥";
const FOOTER_RULE: &str = "---";

/// Extract the TopPicks section from changelog markdown, if present.
/// Returns `Some(section_text)` where `section_text` starts with the `### 💥`
/// heading and ends just before the next `###` heading or the footer `---`.
fn extract_top_picks_section(markdown: &str) -> Option<String> {
    let start = markdown.find(TOP_PICKS_HEADING_PREFIX)?;
    let after_start = &markdown[start..];
    // Find next ### heading or standalone --- after the section
    let end = after_start[TOP_PICKS_HEADING_PREFIX.len()..]
        .find("\n###")
        .map(|p| p + TOP_PICKS_HEADING_PREFIX.len())
        .unwrap_or(after_start.len());
    let section = after_start[..end].trim_end().to_string();
    Some(section)
}

/// Build the `<details>` block to inject.
///
/// * `version` – bare version string, e.g. `"v0.3.1"`
/// * `body`    – markdown body (full changelog or TopPicks-only section)
/// * `release_url` – optional GitHub release URL for the "CLICK HERE" link
///   (only used in TopPicks mode)
/// * `top_picks_mode` – when `true` a "CLICK HERE" line is appended
fn build_details_block(
    version: &str,
    body: &str,
    release_url: Option<&str>,
    top_picks_mode: bool,
) -> String {
    let mut lines: Vec<String> = Vec::new();

    lines.push(format!(
        "<details><summary>👀 What's new in {} ...</summary>",
        version
    ));
    lines.push(String::new());
    lines.push(body.trim_end().to_string());
    lines.push(String::new());
    lines.push(String::new());

    if top_picks_mode && let Some(url) = release_url {
        lines.push(format!(
            "<sup>For detailed changelog [CLICK HERE]({})</sup>",
            url
        ));
        lines.push(String::new());
    }

    lines.push(FOOTER_RULE.to_string());
    lines.push(format!(
        "<sup>... ✨ auto-injected by [ComfyGit]({})</sup>",
        COMFYGIT_LINK
    ));
    lines.push(String::new());
    lines.push(FOOTER_RULE.to_string());
    lines.push(String::new());

    lines.join("\n")
}

/// Build the GitHub release page URL for a specific tag.
///
/// Accepts both SSH (`git@github.com:owner/repo.git`) and HTTPS
/// (`https://github.com/owner/repo`) remote URLs.
fn github_release_url(remote_url: &str, tag: &str) -> Option<String> {
    let (owner, repo) = crate::git::github_owner_repo_from_remote_url(remote_url)?;
    Some(format!(
        "https://github.com/{}/{}/releases/tag/{}",
        owner, repo, tag
    ))
}

/// Inject the "What's new" block into `readme_path` at 1-based `inject_at_row`.
///
/// The file is read, the block is inserted before the line at `inject_at_row`
/// (1-indexed), and the file is written back.
fn inject_into_file(readme_path: &Path, inject_at_row: u16, block: &str) -> Result<()> {
    let content = fs::read_to_string(readme_path)
        .with_context(|| format!("Failed to read README at {}", readme_path.display()))?;

    let mut file_lines: Vec<&str> = content.split('\n').collect();

    // Remove trailing empty string caused by trailing newline
    let had_trailing_newline = content.ends_with('\n');
    if had_trailing_newline && file_lines.last() == Some(&"") {
        file_lines.pop();
    }

    let insert_index = if inject_at_row == 0 {
        0
    } else {
        ((inject_at_row as usize) - 1).min(file_lines.len())
    };

    let block_lines: Vec<&str> = block.split('\n').collect();
    let mut result: Vec<&str> = Vec::with_capacity(file_lines.len() + block_lines.len());
    result.extend_from_slice(&file_lines[..insert_index]);
    result.extend_from_slice(&block_lines);
    result.extend_from_slice(&file_lines[insert_index..]);

    let mut output = result.join("\n");
    if had_trailing_newline {
        output.push('\n');
    }

    fs::write(readme_path, output)
        .with_context(|| format!("Failed to write README at {}", readme_path.display()))?;

    Ok(())
}

/// Parameters for README injection.
pub(crate) struct ReadmeInjectionParams<'a> {
    /// Absolute path to the project repository root.
    pub repo_root: &'a str,
    /// Release tag name, e.g. `"v0.3.1"`.
    pub tag_name: &'a str,
    /// Full changelog markdown (may contain TopPicks section).
    pub changelog_markdown: &'a str,
    /// 1-based row at which to insert the block.
    pub inject_at_row: u16,
    /// Remote URL (SSH or HTTPS) used to build the GitHub release link.
    pub remote_url: Option<&'a str>,
}

/// Perform the README injection.
///
/// Returns `Ok(())` on success.  The caller is responsible for committing the
/// modified README.
pub(crate) fn inject_whats_new(params: &ReadmeInjectionParams<'_>) -> Result<()> {
    let readme_path = Path::new(params.repo_root).join("README.md");
    if !readme_path.exists() {
        bail!(
            "README injection: README.md not found at {}",
            readme_path.display()
        );
    }

    let top_picks = extract_top_picks_section(params.changelog_markdown);
    let (body, top_picks_mode) = match &top_picks {
        Some(section) => (section.as_str(), true),
        None => (params.changelog_markdown, false),
    };

    let release_url = params
        .remote_url
        .and_then(|u| github_release_url(u, params.tag_name));

    let block = build_details_block(
        params.tag_name,
        body,
        release_url.as_deref(),
        top_picks_mode,
    );

    inject_into_file(&readme_path, params.inject_at_row, &block)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_top_picks_section() {
        let md = "## Changelog\n\n### 💥 💥 💥 This Release's Top Picks ...  💥 💥 💥\n\n#### 1. Something\n\n---\n... footer";
        let section = extract_top_picks_section(md).unwrap();
        assert!(section.starts_with("### 💥"));
        assert!(!section.contains("## Changelog"));
    }

    #[test]
    fn no_top_picks_returns_none() {
        let md = "## Changelog\n\n### Refactor\n\n* Fix stuff";
        assert!(extract_top_picks_section(md).is_none());
    }

    #[test]
    fn builds_details_block_full_changelog() {
        let block = build_details_block("v0.3.1", "## Changelog body", None, false);
        assert!(block.contains("<details><summary>👀 What's new in v0.3.1 ...</summary>"));
        assert!(block.contains("## Changelog body"));
        assert!(block.contains("auto-injected by [ComfyGit]"));
        assert!(!block.contains("CLICK HERE"));
    }

    #[test]
    fn builds_details_block_top_picks_with_url() {
        let block = build_details_block(
            "v0.3.1",
            "### 💥 Top picks",
            Some("https://github.com/owner/repo/releases/tag/v0.3.1"),
            true,
        );
        assert!(block.contains("CLICK HERE"));
        assert!(block.contains("releases/tag/v0.3.1"));
    }

    #[test]
    fn github_release_url_from_ssh() {
        let url = github_release_url("git@github.com:comfy-home/ComfyGit.git", "v1.0.0");
        assert_eq!(
            url.as_deref(),
            Some("https://github.com/comfy-home/ComfyGit/releases/tag/v1.0.0")
        );
    }

    #[test]
    fn injects_at_correct_row() {
        let mut path = std::env::temp_dir();
        path.push(format!("cg_rls_inj_test_{}.md", std::process::id()));
        fs::write(&path, "line1\nline2\nline3\n").unwrap();

        inject_into_file(&path, 2, "INJECTED\n").unwrap();

        let result = fs::read_to_string(&path).unwrap();
        let _ = fs::remove_file(&path);
        let lines: Vec<&str> = result.lines().collect();
        assert_eq!(lines[0], "line1");
        assert_eq!(lines[1], "INJECTED");
        assert!(result.contains("line2"));
    }
}
