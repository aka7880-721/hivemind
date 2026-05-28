// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit License v1.2

use anyhow::{Result, bail};

use crate::ghub::cli::{self, CLI_NAME};

pub fn last_release_published_at(repo_root: &str) -> Result<Option<String>> {
    cli::ensure_available()?;
    let output = cli::run_in_repo(
        repo_root,
        &[
            "release",
            "list",
            "--limit",
            "1",
            "--json",
            "publishedAt",
            "--jq",
            ".[]?.publishedAt",
        ],
    )?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("{CLI_NAME} release list failed: {}", stderr.trim());
    }
    let result = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok((!result.is_empty()).then_some(result))
}

pub fn last_release_tag(repo_root: &str) -> Result<Option<String>> {
    cli::ensure_available()?;
    let output = cli::run_in_repo(
        repo_root,
        &[
            "release",
            "list",
            "--limit",
            "1",
            "--json",
            "tagName",
            "--jq",
            ".[]?.tagName",
        ],
    )?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("{CLI_NAME} release list failed: {}", stderr.trim());
    }
    let result = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok((!result.is_empty()).then_some(result))
}

pub fn latest_public_release_tag(repo_root: &str) -> Option<String> {
    let output = cli::run_in_repo(
        repo_root,
        &[
            "release",
            "list",
            "--limit",
            "1",
            "--json",
            "tagName",
            "--jq",
            ".[].tagName",
        ],
    )
    .ok()?;
    if !output.status.success() {
        return None;
    }
    let tag = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!tag.is_empty()).then_some(tag)
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
