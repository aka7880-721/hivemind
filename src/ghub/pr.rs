// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit License v1.2

use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::{
    forge::ForgePullRequest,
    ghub::cli::{self, CLI_NAME},
    ghub::remote,
};

pub const PR_LIST_FIELDS: &str =
    "number,title,baseRefName,headRefName,createdAt,author,mergeable,mergeStateStatus";

pub fn list_open_pull_requests(repo_root: &str, limit: usize) -> Result<Vec<ForgePullRequest>> {
    let limit = limit.to_string();
    let output = cli::run_in_repo(
        repo_root,
        &[
            "pr",
            "list",
            "--state",
            "open",
            "--limit",
            &limit,
            "--json",
            PR_LIST_FIELDS,
        ],
    )?;
    if !output.status.success() {
        bail_cli_failure("pr list", &output)?;
    }

    let listed: Vec<GhPullRequest> =
        serde_json::from_slice(&output.stdout).context("failed to parse gh pr list output")?;
    let repository_issue_root = remote::repository_web_url(repo_root);
    Ok(listed
        .into_iter()
        .map(|pr| pr.into_forge(repository_issue_root.as_deref()))
        .collect())
}

pub fn view_pull_request(repo_root: &str, number: u64) -> Result<ForgePullRequest> {
    let output = cli::run_in_repo(
        repo_root,
        &["pr", "view", &number.to_string(), "--json", PR_LIST_FIELDS],
    )?;
    if !output.status.success() {
        bail_cli_failure("pr view", &output)?;
    }
    let pr: GhPullRequest =
        serde_json::from_slice(&output.stdout).context("failed to parse gh pr view output")?;
    let repository_issue_root = remote::repository_web_url(repo_root);
    Ok(pr.into_forge(repository_issue_root.as_deref()))
}

pub fn fetch_mergeability(repo_root: &str, number: u64) -> Result<crate::forge::ForgeMergeability> {
    let output = cli::run_in_repo(
        repo_root,
        &[
            "pr",
            "view",
            &number.to_string(),
            "--json",
            "mergeable,mergeStateStatus",
        ],
    )?;
    if !output.status.success() {
        bail_cli_failure("pr view", &output)?;
    }
    let raw: GhMergeability =
        serde_json::from_slice(&output.stdout).context("failed to parse gh pr view mergeability output")?;
    Ok(crate::forge::ForgeMergeability {
        mergeable: raw.mergeable,
        merge_state_status: raw.merge_state_status,
    })
}

pub fn merge_pull_request(repo_root: &str, number: u64, subject: &str) -> Result<String> {
    let output = cli::run_in_repo(
        repo_root,
        &[
            "pr",
            "merge",
            &number.to_string(),
            "--merge",
            "--subject",
            subject,
        ],
    )?;
    if !output.status.success() {
        bail_cli_failure("pr merge", &output)?;
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub fn create_pull_request(
    repo_root: &str,
    target_branch: &str,
    current_branch: &str,
    title: &str,
    body_path: &Path,
) -> Result<String> {
    let output = cli::run_in_repo(
        repo_root,
        &[
            "pr",
            "create",
            "--base",
            target_branch,
            "--head",
            current_branch,
            "--title",
            title,
            "--body-file",
            &body_path.display().to_string(),
        ],
    )?;
    if !output.status.success() {
        bail_cli_failure("pr create", &output)?;
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub fn lookup_created_pull_request(
    repo_root: &str,
    current_branch: &str,
    target_branch: &str,
) -> Result<(u64, String)> {
    let output = cli::run_in_repo(
        repo_root,
        &[
            "pr",
            "list",
            "--head",
            current_branch,
            "--state",
            "open",
            "--limit",
            "20",
            "--json",
            "number,url,baseRefName",
        ],
    )?;
    if !output.status.success() {
        bail_cli_failure("pr list", &output)?;
    }

    let listed: Vec<CreatedPullRequestLookup> = serde_json::from_slice(&output.stdout)
        .context("failed to parse gh pr list output for the newly created branch")?;
    let matched = listed
        .into_iter()
        .find(|pull_request| pull_request.base_ref_name == target_branch)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "created PR for branch '{current_branch}' targeting '{target_branch}' could not be resolved"
            )
        })?;
    Ok((matched.number, matched.url))
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
#[serde(rename_all = "camelCase")]
struct GhMergeability {
    mergeable: String,
    merge_state_status: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhPullRequest {
    number: u64,
    title: String,
    base_ref_name: String,
    head_ref_name: String,
    created_at: String,
    author: Option<GhPullRequestAuthor>,
    mergeable: String,
    merge_state_status: String,
}

#[derive(Deserialize)]
struct GhPullRequestAuthor {
    login: String,
    name: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreatedPullRequestLookup {
    number: u64,
    url: String,
    base_ref_name: String,
}

impl GhPullRequest {
    fn into_forge(self, repository_issue_root: Option<&str>) -> ForgePullRequest {
        let mergeable_state = self.mergeable;
        let status = self.merge_state_status;
        let issue_url = repository_issue_root
            .filter(|_| {
                !mergeable_state.eq_ignore_ascii_case("MERGEABLE")
                    || !status.eq_ignore_ascii_case("CLEAN")
            })
            .map(|root| format!("{root}/pull/{}/conflicts", self.number));
        let author = self
            .author
            .and_then(|author| {
                let login = author.login.trim().to_string();
                if login.is_empty() {
                    author.name.filter(|name| !name.trim().is_empty())
                } else {
                    Some(login)
                }
            })
            .unwrap_or_else(|| "-".to_string());
        ForgePullRequest {
            number: self.number,
            title: self.title,
            target_branch: self.base_ref_name,
            source_branch: self.head_ref_name,
            created_at: self.created_at,
            author,
            status,
            mergeable_state,
            issue_url,
        }
    }
}
