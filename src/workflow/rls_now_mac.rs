// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit SA-PS License
//
// For details, see the LICENSE file in the repository root.

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use tokio::time::sleep;

use crate::git::GitCancellation;
use crate::workflow::rls_now::ReleaseNowScript;
use crate::workflow::runtime::run_blocking_job;

const MACOS_CI_WORKFLOW: &str = "macos-release.yml";
const MACOS_CI_ARTIFACT: &str = "macos-packages";
const MACOS_BUILD_STEP: &str = "Build macOS packages via releaseNOW.sh";
const MACOS_ARTIFACT_DIRS: [&str; 2] = ["macos-x86_64", "macos-aarch64"];
const MAC_CI_POLL_INTERVAL: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MacCiArch {
    All,
    Intel,
    Silicon,
}

#[derive(Debug, Clone)]
pub(crate) struct MacCiConfig {
    pub version: String,
    pub arch: MacCiArch,
    pub github_repo: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct MacCiSession {
    pub run_id: u64,
    pub github_repo: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) enum MacCiFinishOutcome {
    Success,
    Failed { warning: String },
}

pub(crate) fn detect_external_mac_ci(
    script: &ReleaseNowScript,
    tag_name: &str,
) -> Option<MacCiConfig> {
    if cfg!(target_os = "macos") {
        return None;
    }
    if script.label != "MacOS" {
        return None;
    }
    let lower = script.script_path.to_ascii_lowercase();
    if !lower.contains("--mac") {
        return None;
    }
    let arch = if lower.contains("--mac-intel")
        || lower.contains("--mac-x64")
        || lower.contains("--mac-amd64")
    {
        MacCiArch::Intel
    } else if lower.contains("--mac-silicon")
        || lower.contains("--mac-arm")
        || lower.contains("--mac-arm64")
    {
        MacCiArch::Silicon
    } else {
        MacCiArch::All
    };
    let version = tag_name.trim_start_matches(['v', 'V']).trim().to_string();
    if version.is_empty() {
        return None;
    }
    Some(MacCiConfig {
        version,
        arch,
        github_repo: None,
    })
}

pub(crate) fn partition_mac_scripts(
    scripts: &[ReleaseNowScript],
    tag_name: &str,
) -> (Option<MacCiConfig>, Vec<ReleaseNowScript>) {
    let mut mac_config = None;
    let mut local_scripts = Vec::new();
    for script in scripts {
        if let Some(config) = detect_external_mac_ci(script, tag_name) {
            if mac_config.is_none() {
                mac_config = Some(config);
            }
        } else {
            local_scripts.push(script.clone());
        }
    }
    (mac_config, local_scripts)
}

fn arch_workflow_field(arch: MacCiArch) -> &'static str {
    match arch {
        MacCiArch::Intel => "intel",
        MacCiArch::Silicon => "silicon",
        MacCiArch::All => "all",
    }
}

fn gh_command(repo_root: &str, github_repo: Option<&str>) -> Command {
    let mut command = Command::new("gh");
    command.current_dir(repo_root);
    if let Some(repo) = github_repo.filter(|value| !value.trim().is_empty()) {
        command.args(["-R", repo.trim()]);
    }
    command
}

fn ensure_gh_available() -> Result<()> {
    let output = Command::new("gh")
        .arg("auth")
        .arg("status")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .context("failed to invoke gh; install GitHub CLI and run 'gh auth login'")?;
    if output.status.success() {
        Ok(())
    } else {
        bail!("GitHub CLI is not authenticated; run 'gh auth login' before ReleaseNOW macOS CI")
    }
}

fn current_git_ref(repo_root: &str) -> Result<String> {
    let output = Command::new("git")
        .current_dir(repo_root)
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .context("failed to read current git branch")?;
    if !output.status.success() {
        bail!("failed to resolve current git branch for macOS CI trigger");
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn parse_run_id_from_output(output: &str) -> Option<u64> {
    output
        .lines()
        .filter_map(|line| {
            line.rsplit('/')
                .next()
                .and_then(|segment| segment.parse::<u64>().ok())
        })
        .next_back()
}

fn trigger_macos_workflow(repo_root: &str, config: &MacCiConfig) -> Result<(MacCiSession, String)> {
    ensure_gh_available()?;
    let git_ref = current_git_ref(repo_root)?;
    let arch = arch_workflow_field(config.arch);
    let github_repo = config.github_repo.as_deref();
    let trigger = |include_arch: bool| -> Result<std::process::Output> {
        let mut command = gh_command(repo_root, github_repo);
        command.args([
            "workflow",
            "run",
            MACOS_CI_WORKFLOW,
            "--ref",
            &git_ref,
            "--field",
            &format!("version={}", config.version),
        ]);
        if include_arch {
            command.args(["--field", &format!("arch={arch}")]);
        }
        command
            .output()
            .with_context(|| format!("failed to trigger workflow {MACOS_CI_WORKFLOW}"))
    };

    let mut output = trigger(true)?;
    let mut combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if !output.status.success()
        && combined.contains("Unexpected inputs")
        && combined.contains("arch")
    {
        output = trigger(false)?;
        combined = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    if !output.status.success() {
        bail!("failed to trigger macOS CI workflow: {}", combined.trim());
    }
    let run_id = parse_run_id_from_output(&combined).ok_or_else(|| {
        anyhow::anyhow!("macOS CI workflow triggered but run ID was not found in gh output")
    })?;
    Ok((
        MacCiSession {
            run_id,
            github_repo: config.github_repo.clone(),
        },
        combined,
    ))
}

fn resolve_run_id_after_trigger(
    repo_root: &str,
    git_ref: &str,
    not_before: &str,
    github_repo: Option<&str>,
) -> Result<u64> {
    for _ in 0..45 {
        let output = gh_command(repo_root, github_repo)
            .args([
                "run",
                "list",
                "--workflow",
                MACOS_CI_WORKFLOW,
                "--branch",
                git_ref,
                "--limit",
                "20",
                "--json",
                "databaseId,createdAt",
                "-q",
                &format!("map(select(.createdAt >= \"{not_before}\")) | .[0].databaseId // empty"),
            ])
            .output()
            .context("failed to list macOS CI workflow runs")?;
        if output.status.success() {
            let run_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if let Ok(id) = run_id.parse::<u64>() {
                return Ok(id);
            }
        }
        std::thread::sleep(Duration::from_secs(2));
    }
    bail!("timed out waiting for macOS CI workflow run to appear")
}

fn utc_now_minus_seconds(seconds: u64) -> String {
    let epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .saturating_sub(seconds);
    if let Ok(formatted) = Command::new("date")
        .args(["-u", "-d", &format!("@{epoch}"), "+%Y-%m-%dT%H:%M:%SZ"])
        .output()
        && formatted.status.success()
    {
        return String::from_utf8_lossy(&formatted.stdout)
            .trim()
            .to_string();
    }
    Command::new("date")
        .args(["-u", "-r", &epoch.to_string(), "+%Y-%m-%dT%H:%M:%SZ"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .unwrap_or_default()
}

fn run_conclusion(repo_root: &str, run_id: u64, github_repo: Option<&str>) -> Result<String> {
    let output = gh_command(repo_root, github_repo)
        .args([
            "run",
            "view",
            &run_id.to_string(),
            "--json",
            "status,conclusion",
            "-q",
            ".status + \"|\" + (.conclusion // \"\")",
        ])
        .output()
        .with_context(|| format!("failed to read macOS CI run {run_id}"))?;
    if !output.status.success() {
        bail!(
            "failed to query macOS CI run {}: {}",
            run_id,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn build_step_status(
    repo_root: &str,
    run_id: u64,
    github_repo: Option<&str>,
) -> Result<Option<String>> {
    let output = gh_command(repo_root, github_repo)
        .args([
            "run",
            "view",
            &run_id.to_string(),
            "--json",
            "jobs",
            "-q",
            &format!(".jobs[] | .steps[] | select(.name == \"{MACOS_BUILD_STEP}\") | .status"),
        ])
        .output()
        .with_context(|| format!("failed to read macOS CI steps for run {run_id}"))?;
    if !output.status.success() {
        return Ok(None);
    }
    let status = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if status.is_empty() {
        Ok(None)
    } else {
        Ok(Some(status))
    }
}

fn fetch_run_log(repo_root: &str, run_id: u64, github_repo: Option<&str>) -> Result<String> {
    let output = gh_command(repo_root, github_repo)
        .args(["run", "view", &run_id.to_string(), "--log"])
        .output()
        .with_context(|| format!("failed to fetch macOS CI log for run {run_id}"))?;
    Ok(format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    ))
}

fn format_mac_log_lines(raw: &str) -> Vec<String> {
    raw.lines()
        .map(|line| format!("[MacOS][ci] {line}"))
        .filter(|line| !line.trim().is_empty())
        .collect()
}

fn semver_lt(left: &str, right: &str) -> bool {
    left != right
        && left
            .split('.')
            .zip(right.split('.'))
            .map(|(l, r)| {
                let l_num = l.parse::<u32>().unwrap_or(0);
                let r_num = r.parse::<u32>().unwrap_or(0);
                l_num.cmp(&r_num)
            })
            .find(|ordering| *ordering != std::cmp::Ordering::Equal)
            == Some(std::cmp::Ordering::Less)
}

fn extract_pkg_version_from_basename(base: &str, package: &str) -> Option<String> {
    let rest = base.strip_prefix(&format!("{package}-"))?;
    let version = rest
        .split('-')
        .next()
        .filter(|part| part.chars().all(|c| c.is_ascii_digit() || c == '.'))?;
    if version.matches('.').count() >= 2 {
        Some(version.to_string())
    } else {
        None
    }
}

fn archive_superseded_mac_artifacts(repo_root: &str, version: &str) -> Result<()> {
    let package = read_package_name(repo_root).unwrap_or_else(|| "comfygit".to_string());
    for dir_name in MACOS_ARTIFACT_DIRS {
        let latest_dir = Path::new(repo_root)
            .join("dist")
            .join("latest")
            .join(dir_name);
        if !latest_dir.is_dir() {
            continue;
        }
        let old_root = Path::new(repo_root).join("dist").join("old").join(dir_name);
        for entry in fs::read_dir(&latest_dir)? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(base) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let Some(existing_version) = extract_pkg_version_from_basename(base, &package) else {
                continue;
            };
            if semver_lt(&existing_version, version) {
                let destination_dir = old_root.join(&existing_version);
                fs::create_dir_all(&destination_dir)?;
                let destination = destination_dir.join(base);
                if destination.exists() {
                    fs::remove_file(&destination)?;
                }
                fs::rename(&path, &destination)?;
            } else if existing_version == version {
                fs::remove_file(&path)?;
            }
        }
    }
    Ok(())
}

fn read_package_name(repo_root: &str) -> Option<String> {
    let cargo_path = Path::new(repo_root).join("Cargo.toml");
    let content = fs::read_to_string(cargo_path).ok()?;
    let parsed: toml::Value = toml::from_str(&content).ok()?;
    parsed
        .get("package")?
        .get("name")?
        .as_str()
        .map(str::to_string)
}

fn download_macos_artifacts(
    repo_root: &str,
    run_id: u64,
    github_repo: Option<&str>,
) -> Result<PathBuf> {
    let staging = std::env::temp_dir().join(format!(
        "comfygit-macos-ci-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    if staging.exists() {
        fs::remove_dir_all(&staging)?;
    }
    fs::create_dir_all(&staging)?;
    let status = gh_command(repo_root, github_repo)
        .args([
            "run",
            "download",
            &run_id.to_string(),
            "-n",
            MACOS_CI_ARTIFACT,
            "-D",
            &staging.display().to_string(),
        ])
        .status()
        .with_context(|| format!("failed to download macOS CI artifact for run {run_id}"))?;
    if !status.success() {
        fs::remove_dir_all(&staging).ok();
        bail!("gh run download failed for macOS CI run {run_id}");
    }
    Ok(staging)
}

fn merge_macos_staging(repo_root: &str, staging: &Path) -> Result<()> {
    let dist_root = Path::new(repo_root).join("dist");
    for sub in ["latest", "old"] {
        let source = staging.join(sub);
        if !source.is_dir() {
            continue;
        }
        let destination = dist_root.join(sub);
        fs::create_dir_all(&destination)?;
        if sub == "latest" {
            for entry in fs::read_dir(&source)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    let name = path
                        .file_name()
                        .and_then(|value| value.to_str())
                        .unwrap_or("");
                    if name.starts_with("macos-") {
                        let target = destination.join(name);
                        if target.exists() {
                            fs::remove_dir_all(&target)?;
                        }
                    }
                }
            }
        }
        copy_dir_recursive(&source, &destination)?;
    }
    if !staging.join("latest").is_dir() && !staging.join("old").is_dir() {
        copy_dir_recursive(staging, &dist_root)?;
    }
    Ok(())
}

fn copy_dir_recursive(source: &Path, destination: &Path) -> Result<()> {
    if source.is_dir() {
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            let from = entry.path();
            let to = destination.join(entry.file_name());
            if from.is_dir() {
                fs::create_dir_all(&to)?;
                copy_dir_recursive(&from, &to)?;
            } else {
                if let Some(parent) = to.parent() {
                    fs::create_dir_all(parent)?;
                }
                if to.exists() {
                    fs::remove_file(&to)?;
                }
                fs::copy(&from, &to)?;
            }
        }
    }
    Ok(())
}

pub(crate) async fn trigger_mac_ci_session(
    repo_root: String,
    config: MacCiConfig,
) -> Result<(MacCiSession, Vec<String>)> {
    run_blocking_job(move || {
        let trigger_after = utc_now_minus_seconds(10);
        let git_ref = current_git_ref(&repo_root)?;
        let (session, trigger_output) = trigger_macos_workflow(&repo_root, &config)?;
        let mut lines = format_mac_log_lines(&trigger_output);
        lines.insert(
            0,
            format!(
                "[MacOS] Triggering GitHub Actions workflow '{MACOS_CI_WORKFLOW}' on ref '{git_ref}' (version {}).",
                config.version
            ),
        );
        if config.github_repo.is_some() {
            lines.insert(
                1,
                format!(
                    "[MacOS] Using GitHub repository {} for macOS CI.",
                    config.github_repo.as_deref().unwrap_or_default()
                ),
            );
        }
        lines.push(format!("[MacOS] Tracking macOS CI run {}.", session.run_id));
        if parse_run_id_from_output(&trigger_output).is_none() {
            let resolved = resolve_run_id_after_trigger(
                &repo_root,
                &git_ref,
                &trigger_after,
                config.github_repo.as_deref(),
            )?;
            lines.push(format!("[MacOS] Resolved macOS CI run {}.", resolved));
            return Ok((
                MacCiSession {
                    run_id: resolved,
                    github_repo: config.github_repo,
                },
                lines,
            ));
        }
        Ok((session, lines))
    })
    .await
}

pub(crate) async fn stream_mac_ci_until_build_started(
    repo_root: String,
    session: MacCiSession,
    cancel: GitCancellation,
    mut emit: impl FnMut(Vec<String>) + Send,
) -> Result<()> {
    let mut emitted_lines = 0usize;
    loop {
        if cancel.is_cancelled() {
            bail!("ReleaseNOW cancelled by user");
        }
        let repo_root_for_poll = repo_root.clone();
        let run_id = session.run_id;
        let github_repo = session.github_repo.clone();
        let (step_status, log_tail, run_state) = run_blocking_job(move || {
            let github_repo = github_repo.as_deref();
            let step_status = build_step_status(&repo_root_for_poll, run_id, github_repo)?;
            let log = fetch_run_log(&repo_root_for_poll, run_id, github_repo).unwrap_or_default();
            let run_state = run_conclusion(&repo_root_for_poll, run_id, github_repo)?;
            Ok::<_, anyhow::Error>((step_status, log, run_state))
        })
        .await?;

        let formatted = format_mac_log_lines(&log_tail);
        if formatted.len() > emitted_lines {
            emit(formatted[emitted_lines..].to_vec());
            emitted_lines = formatted.len();
        }

        if matches!(step_status.as_deref(), Some("in_progress" | "completed")) {
            emit(vec![format!(
                "[MacOS] macOS CI reached '{MACOS_BUILD_STEP}'; continuing with other configured builds."
            )]);
            return Ok(());
        }

        let (status, conclusion) = run_state.split_once('|').unwrap_or((&run_state, ""));
        if status == "completed" {
            if conclusion == "success" {
                emit(vec![
                    "[MacOS] macOS CI finished before the build step was detected; continuing."
                        .to_string(),
                ]);
                return Ok(());
            }
            bail!(
                "macOS CI run {} failed before build step started",
                session.run_id
            );
        }

        sleep(MAC_CI_POLL_INTERVAL).await;
    }
}

pub(crate) async fn watch_mac_ci_to_completion(
    repo_root: String,
    session: MacCiSession,
    cancel: GitCancellation,
    mut emit: impl FnMut(Vec<String>) + Send,
) -> Result<()> {
    let mut emitted_lines = 0usize;
    loop {
        if cancel.is_cancelled() {
            bail!("ReleaseNOW cancelled by user");
        }
        let repo_root_for_poll = repo_root.clone();
        let run_id = session.run_id;
        let github_repo = session.github_repo.clone();
        let (log_tail, run_state) = run_blocking_job(move || {
            let github_repo = github_repo.as_deref();
            let log = fetch_run_log(&repo_root_for_poll, run_id, github_repo).unwrap_or_default();
            let run_state = run_conclusion(&repo_root_for_poll, run_id, github_repo)?;
            Ok::<_, anyhow::Error>((log, run_state))
        })
        .await?;

        let formatted = format_mac_log_lines(&log_tail);
        if formatted.len() > emitted_lines {
            emit(formatted[emitted_lines..].to_vec());
            emitted_lines = formatted.len();
        }

        let (status, conclusion) = run_state.split_once('|').unwrap_or((&run_state, ""));
        if status == "completed" {
            if conclusion == "success" {
                emit(vec![format!(
                    "[MacOS] macOS CI run {} completed successfully.",
                    session.run_id
                )]);
                return Ok(());
            }
            bail!(
                "macOS CI run {} failed; inspect with: gh run view {} --log-failed",
                session.run_id,
                session.run_id
            );
        }

        sleep(MAC_CI_POLL_INTERVAL).await;
    }
}

pub(crate) async fn finish_mac_ci_and_merge_artifacts(
    repo_root: String,
    session: MacCiSession,
    version: String,
    cancel: GitCancellation,
    mut emit: impl FnMut(Vec<String>) + Send,
) -> Result<MacCiFinishOutcome> {
    match watch_mac_ci_to_completion(repo_root.clone(), session.clone(), cancel, &mut emit).await {
        Ok(()) => {}
        Err(error) => {
            return Ok(MacCiFinishOutcome::Failed {
                warning: format!(
                    "macOS build failed ({error}). Fix the macOS workflow, then rerun ReleaseNOW with the MacOS-only option to publish macOS artifacts."
                ),
            });
        }
    }

    let repo_root_for_download = repo_root.clone();
    let run_id = session.run_id;
    let github_repo = session.github_repo.clone();
    let version_for_archive = version.clone();
    run_blocking_job(move || {
        archive_superseded_mac_artifacts(&repo_root_for_download, &version_for_archive)?;
        let staging =
            download_macos_artifacts(&repo_root_for_download, run_id, github_repo.as_deref())?;
        merge_macos_staging(&repo_root_for_download, &staging)?;
        fs::remove_dir_all(&staging).ok();
        Ok::<_, anyhow::Error>(())
    })
    .await?;

    emit(vec![
        format!("[MacOS] Downloading '{MACOS_CI_ARTIFACT}' from run {run_id}..."),
        "[MacOS] Archived superseded macOS artifacts under dist/old/.".to_string(),
        "[MacOS] macOS CI artifacts merged into dist/latest/.".to_string(),
    ]);
    Ok(MacCiFinishOutcome::Success)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_external_mac_ci_from_mac_script_on_linux() {
        let script = ReleaseNowScript {
            label: "MacOS".to_string(),
            script_path: "./scripts/releaseNOW.sh --mac --no-checks".to_string(),
        };
        let config = detect_external_mac_ci(&script, "v0.33.9");
        assert!(config.is_some());
        let config = config.expect("config");
        assert_eq!(config.version, "0.33.9");
        assert_eq!(config.arch, MacCiArch::All);
    }

    #[test]
    fn detect_external_mac_ci_is_none_on_macos_host() {
        if cfg!(target_os = "macos") {
            let script = ReleaseNowScript {
                label: "MacOS".to_string(),
                script_path: "./scripts/releaseNOW.sh --mac".to_string(),
            };
            assert!(detect_external_mac_ci(&script, "0.33.9").is_none());
        }
    }

    #[test]
    fn parse_run_id_from_gh_output() {
        let output = "✓ Created workflow_dispatch event\nhttps://github.com/comfy-home/ComfyGit/actions/runs/26601105576\n";
        assert_eq!(parse_run_id_from_output(output), Some(26601105576));
    }
}
