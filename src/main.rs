// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyVersionBumper License v1.2
//
// For details, see the LICENSE file in the repository root.

mod app;
mod branding;
mod config;
mod dialogs;
mod git;
mod targets;
mod ui;
mod versioning;

fn main() -> anyhow::Result<()> {
    app::run()
}