// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit License v1.2

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

pub fn release_exists(repo_root: &str, tag_name: &str) -> Result<bool> {
    let output = cli::run_in_repo(repo_root, &["release", "view", tag_name])?;
    Ok(output.status.success())
}

fn list_releases(repo_root: &str, limit: usize) -> Result<Vec<GlabReleaseSummary>> {
    let limit = limit.to_string();
    let output = cli::run_in_repo(
        repo_root,
        &["release", "list", "--per-page", &limit, "--output", "json"],
    )?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("{CLI_NAME} release list failed: {}", stderr.trim());
    }
    serde_json::from_slice(&output.stdout).context("failed to parse glab release list output")
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GlabReleaseSummary {
    tag_name: Option<String>,
    released_at: Option<String>,
}
