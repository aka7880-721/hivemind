// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit SA-PS License
//
// For details, see the LICENSE file in the repository root.

mod app;
mod changelog;
mod cli;
mod config;
mod forge;
mod ghub;
mod git;
mod glab;
mod snif;
mod tui;
mod workflow;

pub fn run_entrypoint() -> anyhow::Result<()> {
    match cli::dispatch()? {
        cli::StartupMode::Handled => Ok(()),
        cli::StartupMode::LaunchTui => app::run(),
    }
}
