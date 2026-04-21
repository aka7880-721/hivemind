// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit License v1.2
//
// For details, see the LICENSE file in the repository root.

use anyhow::Result;
use chrono::{Local, TimeZone};

use crate::git::{
    GitCancellation, build_git_args, ensure_git_repo_with_cancel,
    run_git_checked_owned_with_cancel, run_git_with_cancel,
};

pub(crate) fn last_commit_label(
    repo_root: &str,
    pathspecs: &[String],
    cancel: Option<GitCancellation>,
) -> Result<String> {
    let last_commit_timestamp = run_git_checked_owned_with_cancel(
        repo_root,
        build_git_args(&["log", "-1", "--format=%ct", "HEAD"], pathspecs),
        cancel,
    )?;

    Ok(format_relative_git_timestamp(last_commit_timestamp.trim())
        .unwrap_or_else(|| "n/a".to_string()))
}

pub(crate) fn format_relative_git_timestamp(timestamp: &str) -> Option<String> {
    let seconds = timestamp.parse::<i64>().ok()?;
    let then = Local.timestamp_opt(seconds, 0).single()?;
    let now = Local::now();
    let delta = now.signed_duration_since(then);
    let minutes = delta.num_minutes().max(0);

    let label = if minutes < 60 {
        format!("{}m ago", minutes.max(1))
    } else if minutes < 60 * 24 {
        format!("{}h ago", (minutes / 60).max(1))
    } else if minutes < 60 * 24 * 7 {
        format!("{}d ago", (minutes / (60 * 24)).max(1))
    } else if minutes < 60 * 24 * 365 {
        format!("{}w ago", (minutes / (60 * 24 * 7)).max(1))
    } else {
        format!("{}y ago", (minutes / (60 * 24 * 365)).max(1))
    };

    Some(label)
}

pub(crate) fn last_tag_name(
    repo_root: &str,
    cancel: Option<GitCancellation>,
) -> Result<Option<String>> {
    let describe = run_git_with_cancel(repo_root, &["describe", "--tags", "--abbrev=0"], cancel)?;
    if !describe.success {
        return Ok(None);
    }

    let tag = describe.stdout.trim().to_string();
    Ok((!tag.is_empty()).then_some(tag))
}

pub(crate) fn latest_local_tag_with_cancel(
    repo_root: &str,
    cancel: Option<GitCancellation>,
) -> Result<Option<String>> {
    last_tag_name(repo_root, cancel)
}

#[allow(dead_code)]
pub(crate) fn last_tag_time(
    repo_root: &str,
    pathspecs: &[String],
    cancel: Option<GitCancellation>,
) -> Result<String> {
    ensure_git_repo_with_cancel(repo_root, cancel.clone())?;

    let describe = run_git_with_cancel(
        repo_root,
        &["describe", "--tags", "--abbrev=0"],
        cancel.clone(),
    )?;
    if !describe.success {
        return Ok("n/a".to_string());
    }

    let tag = describe.stdout.trim().to_string();
    if tag.is_empty() {
        return Ok("n/a".to_string());
    }

    let tag_timestamp = run_git_checked_owned_with_cancel(
        repo_root,
        build_git_args(&["log", "-1", "--format=%ct", &tag], pathspecs),
        cancel,
    )?;

    Ok(format_relative_git_timestamp(tag_timestamp.trim()).unwrap_or_else(|| "n/a".to_string()))
}

pub(crate) fn last_bump_time(
    repo_root: &str,
    pathspecs: &[String],
    cancel: Option<GitCancellation>,
) -> Result<Option<i64>> {
    ensure_git_repo_with_cancel(repo_root, cancel.clone())?;

    let args = build_git_args(
        &[
            "log",
            "-1",
            "--grep=bump: CG app version bump",
            "--format=%ct",
            "HEAD",
        ],
        pathspecs,
    );

    let output = run_git_checked_owned_with_cancel(repo_root, args, cancel)?;
    let trimmed = output.trim();
    if trimmed.is_empty() {
        Ok(None)
    } else {
        Ok(trimmed.parse::<i64>().ok())
    }
}

pub(crate) fn recent_merge_check(
    repo_root: &str,
    pathspecs: &[String],
    cancel: Option<GitCancellation>,
) -> Result<String> {
    ensure_git_repo_with_cancel(repo_root, cancel.clone())?;

    let args = build_git_args(
        &[
            "log",
            "-1",
            "--grep=Merge pull request",
            "--format=%ct",
            "HEAD",
        ],
        pathspecs,
    );

    let output = run_git_checked_owned_with_cancel(repo_root, args, cancel)?;
    let trimmed = output.trim();
    let timestamp = trimmed.parse::<i64>().ok();
    let now = Local::now().timestamp();

    match timestamp {
        Some(last_merge_ts) if now.saturating_sub(last_merge_ts) < 5 * 60 => Ok("pass".to_string()),
        _ => Ok("fail".to_string()),
    }
}
