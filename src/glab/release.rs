// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit License v1.2

use std::process::Command;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::glab::cli::{self, CLI_NAME};

pub fn last_release_published_at(repo_root: &str) -> Result<Option<String>> {
    cli::ensure_available()?;
    let releases = list_releases(repo_root, 1)?;
    Ok(releases
        .first()
        .and_then(|release| release.released_at.clone()))
}

pub fn last_release_tag(repo_root: &str) -> Result<Option<String>> {
    cli::ensure_available()?;
    let releases = list_releases(repo_root, 1)?;
    Ok(releases
        .first()
        .and_then(|release| release.tag_name.clone()))
}

pub fn latest_public_release_tag(repo_root: &str) -> Option<String> {
    list_releases(repo_root, 1)
        .ok()?
        .into_iter()
        .next()
        .and_then(|release| release.tag_name)
}

pub fn delete_release(repo_root: &str, tag_name: &str) -> Result<()> {
    cli::ensure_available()?;
    let output = cli::run_in_repo(repo_root, &["release", "delete", tag_name, "--yes"])?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("{CLI_NAME} release delete failed: {}", stderr.trim());
    }
    Ok(())
}

pub fn encode_gitlab_api_project(repo_selector: &str) -> String {
    repo_selector
        .split('/')
        .map(encode_gitlab_api_path_segment)
        .collect::<Vec<_>>()
        .join("%2F")
}

pub fn encode_gitlab_api_tag(tag_name: &str) -> String {
    tag_name
        .chars()
        .map(|ch| match ch {
            '.' => "%2E".to_string(),
            c if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '/') => c.to_string(),
            c => c
                .encode_utf8(&mut [0; 4])
                .bytes()
                .map(|byte| format!("%{byte:02X}"))
                .collect(),
        })
        .collect()
}

pub fn list_release_asset_links(
    repo_selector: &str,
    tag_name: &str,
) -> Result<Vec<GlabReleaseAssetLink>> {
    cli::ensure_available()?;
    let project = encode_gitlab_api_project(repo_selector);
    let tag = encode_gitlab_api_tag(tag_name);
    let endpoint = format!("projects/{project}/releases/{tag}");
    let output = run_glab_api(&["--output", "json", &endpoint])?;
    if !output.status.success() {
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        bail!(
            "{CLI_NAME} API request failed for release assets: {}",
            combined.trim()
        );
    }
    let release: GlabReleaseDetail =
        serde_json::from_slice(&output.stdout).context("failed to parse GitLab release assets")?;
    Ok(release.assets.links)
}

pub fn delete_release_asset_link(repo_selector: &str, tag_name: &str, link_id: u64) -> Result<()> {
    cli::ensure_available()?;
    let project = encode_gitlab_api_project(repo_selector);
    let tag = encode_gitlab_api_tag(tag_name);
    let endpoint = format!("projects/{project}/releases/{tag}/assets/links/{link_id}");
    let output = run_glab_api(&["--method", "DELETE", &endpoint])?;
    if !output.status.success() {
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        bail!(
            "{CLI_NAME} API failed to delete release asset link {link_id}: {}",
            combined.trim()
        );
    }
    Ok(())
}

pub fn remove_conflicting_release_assets(
    repo_selector: &str,
    tag_name: &str,
    asset_names: &[String],
) -> Result<Vec<String>> {
    if asset_names.is_empty() {
        return Ok(Vec::new());
    }

    let wanted = asset_names
        .iter()
        .map(|name| name.to_ascii_lowercase())
        .collect::<std::collections::HashSet<_>>();
    let mut removed = Vec::new();
    for link in list_release_asset_links(repo_selector, tag_name)? {
        if wanted.contains(&link.name.to_ascii_lowercase()) {
            delete_release_asset_link(repo_selector, tag_name, link.id)?;
            removed.push(link.name);
        }
    }
    Ok(removed)
}

fn run_glab_api(args: &[&str]) -> Result<std::process::Output> {
    Command::new(CLI_NAME)
        .arg("api")
        .args(args)
        .output()
        .with_context(|| format!("failed to execute {CLI_NAME} api"))
}

fn encode_gitlab_api_path_segment(segment: &str) -> String {
    segment
        .chars()
        .map(|ch| match ch {
            c if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') => c.to_string(),
            c => c
                .encode_utf8(&mut [0; 4])
                .bytes()
                .map(|byte| format!("%{byte:02X}"))
                .collect(),
        })
        .collect()
}

fn list_releases(repo_root: &str, limit: usize) -> Result<Vec<GlabReleaseSummary>> {
    let limit = limit.to_string();
    let output = cli::run_in_repo(
        repo_root,
        &["release", "list", "-P", &limit, "--output", "json"],
    )?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("{CLI_NAME} release list failed: {}", stderr.trim());
    }
    serde_json::from_slice(&output.stdout).context("failed to parse glab release list output")
}

#[derive(Debug, Clone, Deserialize)]
pub struct GlabReleaseAssetLink {
    pub id: u64,
    pub name: String,
}

#[derive(Deserialize)]
struct GlabReleaseDetail {
    assets: GlabReleaseAssets,
}

#[derive(Deserialize)]
struct GlabReleaseAssets {
    links: Vec<GlabReleaseAssetLink>,
}

#[derive(Deserialize)]
struct GlabReleaseSummary {
    tag_name: Option<String>,
    released_at: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_gitlab_api_project_escapes_nested_groups() {
        assert_eq!(
            encode_gitlab_api_project("comfyhome/dev/SNIF"),
            "comfyhome%2Fdev%2FSNIF"
        );
    }

    #[test]
    fn encode_gitlab_api_tag_escapes_dots() {
        assert_eq!(encode_gitlab_api_tag("v0.3.2"), "v0%2E3%2E2");
    }
}
