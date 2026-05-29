// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit SA-PS License
//
// For details, see the LICENSE file in the repository root.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use anyhow::{Context, Result, bail};

pub const CLI_NAME: &str = "snif";

pub fn is_available() -> bool {
    Command::new(CLI_NAME)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

pub fn install_instructions() -> &'static str {
    "SNIF is not installed or not on PATH.\n\
     Install the standalone SNIF binary, then retry.\n\
     \n\
     • GitHub (coming soon): https://github.com/not-yet-available\n\
     • GitLab (coming soon): https://gitlab.com/not-yet-available\n\
     \n\
     Verify: snif --version\n\
     ComfyGit SNIF features stay disabled until `snif` is available from the official channels."
}

pub fn ensure_available() -> Result<()> {
    if is_available() {
        Ok(())
    } else {
        bail!("{}", install_instructions())
    }
}

pub fn dispatch_with_root(args: Vec<String>, root: PathBuf) -> Result<()> {
    ensure_available()?;
    let status = Command::new(CLI_NAME)
        .current_dir(&root)
        .args(&args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("failed to run {CLI_NAME}"))?;
    if status.success() {
        Ok(())
    } else {
        bail!("{CLI_NAME} exited with {}", status)
    }
}

pub fn run_search(
    root: &Path,
    filters: &str,
    pattern: &str,
    case_sensitive: bool,
) -> Result<Vec<String>> {
    ensure_available()?;
    let mut args = Vec::new();
    if case_sensitive {
        args.push("-e");
    }
    args.push(filters);
    args.push(pattern);
    let output = run_capture(root, &args)?;
    Ok(parse_cli_output(&output))
}

pub fn run_replace(
    root: &Path,
    filters: &str,
    pattern: &str,
    replacement: &str,
    case_insensitive: bool,
) -> Result<Vec<String>> {
    ensure_available()?;
    let mut args = vec!["rpl", "--yes"];
    if case_insensitive {
        args.push("-any");
    }
    args.extend([filters, pattern, replacement]);
    let output = run_capture(root, &args)?;
    Ok(parse_cli_output(&output))
}

fn run_capture(root: &Path, args: &[&str]) -> Result<Output> {
    Command::new(CLI_NAME)
        .current_dir(root)
        .args(args)
        .output()
        .with_context(|| format!("failed to run {CLI_NAME} {}", args.join(" ")))
        .and_then(|output| {
            if output.status.success() {
                Ok(output)
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let stdout = String::from_utf8_lossy(&output.stdout);
                let detail = if stderr.trim().is_empty() {
                    stdout.trim().to_string()
                } else {
                    stderr.trim().to_string()
                };
                bail!("{CLI_NAME} failed: {detail}")
            }
        })
}

fn parse_cli_output(output: &Output) -> Vec<String> {
    let combined = {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.trim().is_empty() {
            stdout.into_owned()
        } else if stdout.trim().is_empty() {
            stderr.into_owned()
        } else {
            format!("{stdout}{stderr}")
        }
    };
    combined
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take_while(|line| !line.starts_with('─') && !line.contains("SUMMARY"))
        .map(str::to_string)
        .collect()
}
