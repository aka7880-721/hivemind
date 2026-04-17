// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit License v1.2
//
// For details, see the LICENSE file in the repository root.

mod app;
mod branding;
mod changelog;
mod config;
mod dialogs;
mod git;
mod mmr;
#[path = "overview-pg.rs"]
mod overview_pg;
mod project_edit;
mod project_wizard;
mod targets;
mod tiles;
mod ui;
mod versioning;

fn main() -> anyhow::Result<()> {
    app::run()
}