// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit License v1.2

use crate::config::IntegrationMode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OverviewBumpWorkflow {
    JustBump,
    Commit,
    CommitAndTag,
    CommitAndPush,
    BranchCommit,
    BranchCommitAndPush,
}

pub(crate) fn overview_bump_workflow_options(
    integration_mode: IntegrationMode,
) -> Vec<OverviewBumpWorkflow> {
    match integration_mode {
        IntegrationMode::LocalOnly => vec![OverviewBumpWorkflow::JustBump],
        IntegrationMode::GitLocalOnly => vec![
            OverviewBumpWorkflow::JustBump,
            OverviewBumpWorkflow::Commit,
            OverviewBumpWorkflow::CommitAndTag,
        ],
        IntegrationMode::GitHubEnabled | IntegrationMode::GitLabEnabled => vec![
            OverviewBumpWorkflow::JustBump,
            OverviewBumpWorkflow::Commit,
            OverviewBumpWorkflow::CommitAndPush,
            OverviewBumpWorkflow::BranchCommit,
            OverviewBumpWorkflow::BranchCommitAndPush,
        ],
    }
}

impl OverviewBumpWorkflow {
    pub(crate) fn display_name(self) -> &'static str {
        match self {
            OverviewBumpWorkflow::JustBump => "Just bump",
            OverviewBumpWorkflow::Commit => "Bump & Commit",
            OverviewBumpWorkflow::CommitAndTag => "Bump & Commit & Tag",
            OverviewBumpWorkflow::CommitAndPush => "Bump & Commit & Push",
            OverviewBumpWorkflow::BranchCommit => "Branch & Bump & Commit",
            OverviewBumpWorkflow::BranchCommitAndPush => "Branch & Bump & Commit & Push",
        }
    }

    pub(crate) fn description(self) -> &'static str {
        match self {
            OverviewBumpWorkflow::JustBump => "Writes the updated version files only.",
            OverviewBumpWorkflow::Commit => {
                "Stages the version files and commits them with the standard bump message."
            }
            OverviewBumpWorkflow::CommitAndTag => {
                "Stages and commits the version files, then creates a tag named after the new version."
            }
            OverviewBumpWorkflow::CommitAndPush => {
                "Stages and commits the version files, then pushes the bump commit to the configured remote."
            }
            OverviewBumpWorkflow::BranchCommit => {
                "Creates a new branch, stages and commits the version files there, and leaves pushing for later."
            }
            OverviewBumpWorkflow::BranchCommitAndPush => {
                "Creates a new branch, stages and commits the version files there, then pushes the new branch to the configured remote."
            }
        }
    }

    pub(crate) fn requires_push(self) -> bool {
        matches!(
            self,
            OverviewBumpWorkflow::CommitAndPush | OverviewBumpWorkflow::BranchCommitAndPush
        )
    }

    pub(crate) fn requires_tag(self) -> bool {
        matches!(self, OverviewBumpWorkflow::CommitAndTag)
    }

    pub(crate) fn requires_branch(self) -> bool {
        matches!(
            self,
            OverviewBumpWorkflow::BranchCommit | OverviewBumpWorkflow::BranchCommitAndPush
        )
    }
}

#[derive(Clone)]
pub(crate) struct RepoBumpOperation {
    pub(crate) repo_root: String,
    pub(crate) remote_spec: Option<String>,
    pub(crate) stage_paths: Vec<String>,
}
