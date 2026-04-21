// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit License v1.2
//
// For details, see the LICENSE file in the repository root.

use anyhow::Result;
use chrono::{Local, TimeZone};

use crate::git::{GitCancellation, build_git_args, run_git_checked_owned_with_cancel};

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
