// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit License v1.2

use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::{
    forge::ForgePullRequest,
    glab::cli::{self, CLI_NAME},
    glab::remote,
};

pub fn list_open_merge_requests(repo_root: &str, limit: usize) -> Result<Vec<ForgePullRequest>> {
    let limit = limit.to_string();
    let output = cli::run_in_repo(
        repo_root,
        &["mr", "list", "--per-page", &limit, "--output", "json"],
    )?;
    if !output.status.success() {
        bail_cli_failure("mr list", &output)?;
    }

    let listed: Vec<GlabMergeRequest> =
        serde_json::from_slice(&output.stdout).context("failed to parse glab mr list output")?;
    let repository_issue_root = remote::repository_web_url(repo_root);
    Ok(listed
        .into_iter()
        .map(|mr| mr.into_forge(repository_issue_root.as_deref()))
        .collect())
}

pub fn view_merge_request(repo_root: &str, number: u64) -> Result<ForgePullRequest> {
    let output = cli::run_in_repo(
        repo_root,
        &["mr", "view", &number.to_string(), "--output", "json"],
    )?;
    if !output.status.success() {
        bail_cli_failure("mr view", &output)?;
    }
    let mr: GlabMergeRequest =
        serde_json::from_slice(&output.stdout).context("failed to parse glab mr view output")?;
    let repository_issue_root = remote::repository_web_url(repo_root);
    Ok(mr.into_forge(repository_issue_root.as_deref()))
}

pub fn fetch_mergeability(repo_root: &str, number: u64) -> Result<crate::forge::ForgeMergeability> {
    let mr = view_merge_request(repo_root, number)?;
    Ok(crate::forge::ForgeMergeability {
        mergeable: mr.mergeable_state.clone(),
        merge_state_status: mr.status.clone(),
    })
}

pub fn merge_merge_request(repo_root: &str, number: u64, subject: &str) -> Result<String> {
    let output = cli::run_in_repo(
        repo_root,
        &[
            "mr",
            "merge",
            &number.to_string(),
            "--merge-commit",
            "--message",
            subject,
        ],
    )?;
    if !output.status.success() {
        bail_cli_failure("mr merge", &output)?;
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub fn create_merge_request(
    repo_root: &str,
    target_branch: &str,
    current_branch: &str,
    title: &str,
    body_path: &Path,
) -> Result<String> {
    let output = cli::run_in_repo(
        repo_root,
        &[
            "mr",
            "create",
            "--target-branch",
            target_branch,
            "--source-branch",
            current_branch,
            "--title",
            title,
            "--description",
            &std::fs::read_to_string(body_path).with_context(|| {
                format!(
                    "failed to read merge request body from '{}'",
                    body_path.display()
                )
            })?,
        ],
    )?;
    if !output.status.success() {
        bail_cli_failure("mr create", &output)?;
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub fn lookup_created_merge_request(
    repo_root: &str,
    current_branch: &str,
    target_branch: &str,
) -> Result<(u64, String)> {
    let output = cli::run_in_repo(
        repo_root,
        &[
            "mr",
            "list",
            "--source-branch",
            current_branch,
            "--per-page",
            "20",
            "--output",
            "json",
        ],
    )?;
    if !output.status.success() {
        bail_cli_failure("mr list", &output)?;
    }

    let listed: Vec<GlabMergeRequestLookup> = serde_json::from_slice(&output.stdout)
        .context("failed to parse glab mr list output for the newly created branch")?;
    let matched = listed
        .into_iter()
        .find(|mr| mr.target_branch == target_branch)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "created merge request for branch '{current_branch}' targeting '{target_branch}' could not be resolved"
            )
        })?;
    Ok((matched.iid, matched.web_url))
}

fn bail_cli_failure(action: &str, output: &std::process::Output) -> Result<()> {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !stderr.is_empty() {
        bail!("{CLI_NAME} {action} failed: {stderr}");
    }
    if !stdout.is_empty() {
        bail!("{CLI_NAME} {action} failed: {stdout}");
    }
    bail!(
        "{CLI_NAME} {action} failed with exit code {:?}",
        output.status.code()
    );
}

#[derive(Deserialize)]
struct GlabMergeRequest {
    iid: u64,
    title: String,
    target_branch: String,
    source_branch: String,
    created_at: String,
    author: Option<GlabAuthor>,
    merge_status: Option<String>,
    detailed_merge_status: Option<String>,
    web_url: Option<String>,
}

#[derive(Deserialize)]
struct GlabAuthor {
    username: Option<String>,
    name: Option<String>,
}

#[derive(Deserialize)]
struct GlabMergeRequestLookup {
    iid: u64,
    web_url: String,
    target_branch: String,
}

fn is_mergeable_status(status: &str) -> bool {
    matches!(
        status.to_ascii_lowercase().as_str(),
        "can_be_merged" | "mergeable"
    )
}

impl GlabMergeRequest {
    fn into_forge(self, repository_issue_root: Option<&str>) -> ForgePullRequest {
        let mergeable_state = self
            .detailed_merge_status
            .clone()
            .or(self.merge_status.clone())
            .unwrap_or_else(|| "unknown".to_string());
        let status = mergeable_state.clone();
        let issue_url = repository_issue_root
            .filter(|_| !is_mergeable_status(&mergeable_state))
            .map(|root| format!("{root}/-/merge_requests/{}/conflicts", self.iid));
        let author = self
            .author
            .and_then(|author| {
                author
                    .username
                    .filter(|value| !value.trim().is_empty())
                    .or(author.name.filter(|name| !name.trim().is_empty()))
            })
            .unwrap_or_else(|| "-".to_string());
        ForgePullRequest {
            number: self.iid,
            title: self.title,
            target_branch: self.target_branch,
            source_branch: self.source_branch,
            created_at: self.created_at,
            author,
            status,
            mergeable_state,
            issue_url: issue_url.or(self.web_url),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MR_VIEW_JSON: &str = r#"{
        "iid": 140,
        "title": "feature",
        "target_branch": "0.35.x",
        "source_branch": "v0.35.1-dev",
        "created_at": "2026-05-30T11:00:53.045Z",
        "author": { "username": "dev-ComfyHome", "name": "Tom" },
        "detailed_merge_status": "mergeable",
        "web_url": "https://gitlab.com/comfyhome/dist/ComfyGit/-/merge_requests/140"
    }"#;

    const MR_LIST_JSON: &str = r#"[{
        "iid": 140,
        "web_url": "https://gitlab.com/comfyhome/dist/ComfyGit/-/merge_requests/140",
        "target_branch": "0.35.x",
        "source_branch": "v0.35.1-dev"
    }]"#;

    #[test]
    fn parses_glab_mr_view_snake_case_json() {
        let mr: GlabMergeRequest = serde_json::from_str(MR_VIEW_JSON).expect("parse mr view");
        let forge = mr.into_forge(Some("https://gitlab.com/comfyhome/dist/ComfyGit"));
        assert_eq!(forge.number, 140);
        assert_eq!(forge.target_branch, "0.35.x");
        assert_eq!(forge.source_branch, "v0.35.1-dev");
        assert_eq!(forge.mergeable_state, "mergeable");
        assert_eq!(forge.author, "dev-ComfyHome");
    }

    #[test]
    fn parses_glab_mr_list_lookup_snake_case_json() {
        let listed: Vec<GlabMergeRequestLookup> =
            serde_json::from_str(MR_LIST_JSON).expect("parse mr list");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].iid, 140);
        assert_eq!(listed[0].target_branch, "0.35.x");
    }
}
