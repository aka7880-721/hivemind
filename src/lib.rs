// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit License v1.2
//
// For details, see the LICENSE file in the repository root.

mod app;
mod branding;
mod changelog;
mod cli;
mod cli_init;
mod config;
mod dialogs;
mod forge;
mod ghub;
mod git;
mod glab;
mod project_edit;
mod project_wizard;
mod snif_modal;
mod target_custom;
mod targets;
mod tiles;
mod tui;
mod ui;
mod variator;
mod versioning;
mod workflow;

pub fn run_entrypoint() -> anyhow::Result<()> {
    match cli::dispatch()? {
        cli::StartupMode::Handled => Ok(()),
        cli::StartupMode::LaunchTui => app::run(),
    }
}
