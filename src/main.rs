// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyVersionBumper License v1.1.
// You may use, modify, and redistribute this file for non‑commercial purposes only,
// provided that attribution is preserved and Branding Elements remain intact.
//
// For details, see the LICENSE file in the repository root.

mod app;
mod branding;
mod config;
mod versioning;

fn main() -> anyhow::Result<()> {
    app::run()
}