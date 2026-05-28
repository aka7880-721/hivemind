// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit License v1.2

use std::process::{Command, Output, Stdio};

use anyhow::{Context, Result, bail};

pub const CLI_NAME: &str = "gh";

pub fn ensure_available() -> Result<()> {
    let output = Command::new(CLI_NAME)
        .arg("--version")
        .output()
        .with_context(|| format!("failed to invoke {CLI_NAME}; install GitHub CLI"))?;
    if output.status.success() {
        Ok(())
    } else {
        bail!("{CLI_NAME} is not available or not functioning; install GitHub CLI")
    }
}

pub fn ensure_authenticated() -> Result<()> {
    ensure_available()?;
    let output = Command::new(CLI_NAME)
        .args(["auth", "status"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context("failed to invoke gh auth status")?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let detail = if stderr.is_empty() { stdout } else { stderr };
        bail!("GitHub CLI is not authenticated: {detail}")
    }
}

pub fn run_in_repo(repo_root: &str, args: &[&str]) -> Result<Output> {
    Command::new(CLI_NAME)
        .current_dir(repo_root)
        .args(args)
        .output()
        .with_context(|| format!("failed to execute {CLI_NAME} {}", args.join(" ")))
}
