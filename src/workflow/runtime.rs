// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit License v1.2

use std::{
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow, bail};
use tokio::time::sleep;

pub(crate) const NETWORK_RETRY_ATTEMPTS: usize = 2;
const NETWORK_RETRY_DELAY: Duration = Duration::from_millis(750);
pub(crate) const GIT_PUSH_TIMEOUT: Duration = Duration::from_secs(20);

pub(crate) async fn run_blocking_job<T, F>(operation: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T> + Send + 'static,
{
    tokio::task::spawn_blocking(operation)
        .await
        .map_err(|error| anyhow!("background task failed: {error}"))?
}

pub(crate) async fn run_blocking_job_with_timeout<T, F>(
    timeout: Duration,
    operation: F,
) -> Result<Option<T>>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T> + Send + 'static,
{
    match tokio::time::timeout(timeout, run_blocking_job(operation)).await {
        Ok(Ok(value)) => Ok(Some(value)),
        Ok(Err(error)) => Err(error),
        Err(_) => Ok(None),
    }
}

pub(crate) async fn run_command_with_retry_async(
    repo_root: String,
    program: &'static str,
    args: Vec<String>,
    timeout: Duration,
    attempts: usize,
    action: &str,
) -> Result<()> {
    let total_attempts = attempts.max(1);
    let mut last_error = None;
    let action = action.to_string();

    for attempt in 1..=total_attempts {
        let repo_root_for_attempt = repo_root.clone();
        let args_for_attempt = args.clone();
        let action_for_attempt = action.clone();
        match run_blocking_job(move || {
            run_command_checked_with_timeout(
                &repo_root_for_attempt,
                program,
                &args_for_attempt,
                timeout,
                &action_for_attempt,
            )
        })
        .await
        {
            Ok(()) => return Ok(()),
            Err(error) => {
                last_error = Some(error);
                if attempt < total_attempts {
                    sleep(NETWORK_RETRY_DELAY).await;
                }
            }
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow!("{action} failed")))
}

pub(crate) fn run_command_checked_with_timeout(
    repo_root: &str,
    program: &str,
    args: &[String],
    timeout: Duration,
    action: &str,
) -> Result<()> {
    let mut command = Command::new(program);
    command
        .current_dir(repo_root)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .with_context(|| format!("failed to start {action} in '{}'", repo_root))?;
    let started_at = Instant::now();

    loop {
        if let Some(status) = child
            .try_wait()
            .with_context(|| format!("failed to poll {action}"))?
        {
            let output = child
                .wait_with_output()
                .with_context(|| format!("failed to collect output for {action}"))?;
            if status.success() {
                return Ok(());
            }

            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let detail = if stderr.is_empty() { stdout } else { stderr };
            bail!("{action} failed: {detail}");
        }

        if started_at.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait_with_output();
            bail!("{action} timed out after {}s", timeout.as_secs());
        }

        std::thread::sleep(Duration::from_millis(100));
    }
}
