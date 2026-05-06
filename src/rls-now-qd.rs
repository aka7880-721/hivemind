// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit License
//
// For details, see the LICENSE file in the repository root.

//! ReleaseNOW Quick-Downloads: HTML section for GitHub release notes with
//! `releases/download/{tag}/{file}` links. Artifact-to-slot matching uses
//! basename heuristics; see [`assign_artifacts_to_slots`].

use std::path::Path;

use crate::config::{QuickDownloadsPosition, ReleaseNowQuickDownloadsSettings};

const LOGO_BASE: &str = "https://github.com/comfy-home/ComfyGit/blob/main/assets/logos-3rd-party";
const NOT_AVAILABLE_TITLE: &str = "\u{1f6ab} Not Available at the moment!";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct QuickDownloadSlots {
    pub win_msi: Option<String>,
    pub win_zip: Option<String>,
    pub appimage_x86_64: Option<String>,
    pub appimage_aarch64: Option<String>,
    pub rpm_x86_64: Option<String>,
    pub rpm_aarch64: Option<String>,
    pub deb_amd64: Option<String>,
    pub deb_arm64: Option<String>,
    pub tar_amd64: Option<String>,
    pub tar_arm64: Option<String>,
    pub mac_intel_pkg: Option<String>,
    pub mac_intel_app_zip: Option<String>,
    pub mac_m_pkg: Option<String>,
    pub mac_m_app_zip: Option<String>,
}

fn basename(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(path)
        .to_string()
}

fn lower(s: &str) -> String {
    s.to_lowercase()
}

fn is_arm64ish(s: &str) -> bool {
    let l = lower(s);
    l.contains("aarch64") || l.contains("arm64")
}

fn is_amd64ish(s: &str) -> bool {
    let l = lower(s);
    l.contains("x86_64") || l.contains("amd64") || l.contains("x64")
}

fn is_intelish(s: &str) -> bool {
    let l = lower(s);
    is_amd64ish(s) || l.contains("intel") || l.contains("universal")
}

fn is_apple_silicon(s: &str) -> bool {
    let l = lower(s);
    is_arm64ish(s)
        || (l.contains("apple") && l.contains("silicon"))
        || l.contains("m1")
        || l.contains("m2")
        || l.contains("m3")
}

/// Assigns discovered release filenames (basenames) to QD table slots using
/// extension and architecture tokens in the basename (case-insensitive).
pub(crate) fn assign_artifacts_to_slots(artifact_paths: &[String]) -> QuickDownloadSlots {
    let mut slots = QuickDownloadSlots::default();
    let names: Vec<String> = artifact_paths.iter().map(|p| basename(p)).collect();

    // Windows MSI
    let msi: Vec<_> = names
        .iter()
        .filter(|n| lower(n).ends_with(".msi"))
        .cloned()
        .collect();
    if !msi.is_empty() {
        slots.win_msi = Some(
            msi.into_iter()
                .min_by_key(|n| {
                    let l = lower(n);
                    let win = l.contains("win") || l.contains("windows");
                    (!win, l.len())
                })
                .expect("non-empty"),
        );
    }

    // Windows portable zip (exclude obvious Linux/mac archives)
    let win_zips: Vec<_> = names
        .iter()
        .filter(|n| {
            let l = lower(n);
            l.ends_with(".zip")
                && (l.contains("win") || l.contains("windows"))
                && !l.contains("linux")
                && !l.contains("darwin")
                && !l.contains("macos")
                && !l.contains("appimage")
        })
        .cloned()
        .collect();
    if !win_zips.is_empty() {
        slots.win_zip = Some(win_zips[0].clone());
    } else {
        let zips: Vec<_> = names
            .iter()
            .filter(|n| {
                let l = lower(n);
                l.ends_with(".zip")
                    && !l.contains("linux")
                    && !l.contains("appimage")
                    && !l.contains("darwin")
                    && !l.contains("macos")
                    && !l.contains(".app.")
            })
            .cloned()
            .collect();
        if zips.len() == 1 {
            slots.win_zip = Some(zips[0].clone());
        }
    }

    for n in &names {
        let l = lower(n);
        if l.ends_with(".appimage") || l.contains("appimage") {
            if is_arm64ish(n) && slots.appimage_aarch64.is_none() {
                slots.appimage_aarch64 = Some(n.clone());
            } else if is_amd64ish(n) && !is_arm64ish(n) && slots.appimage_x86_64.is_none() {
                slots.appimage_x86_64 = Some(n.clone());
            } else if slots.appimage_x86_64.is_none() && slots.appimage_aarch64.is_none() {
                if is_arm64ish(n) {
                    slots.appimage_aarch64 = Some(n.clone());
                } else {
                    slots.appimage_x86_64 = Some(n.clone());
                }
            }
        }
    }

    for n in &names {
        let l = lower(n);
        if l.ends_with(".rpm") {
            if is_arm64ish(n) && slots.rpm_aarch64.is_none() {
                slots.rpm_aarch64 = Some(n.clone());
            } else if is_amd64ish(n) && slots.rpm_x86_64.is_none() {
                slots.rpm_x86_64 = Some(n.clone());
            }
        }
    }

    for n in &names {
        let l = lower(n);
        if l.ends_with(".deb") {
            if is_arm64ish(n) && slots.deb_arm64.is_none() {
                slots.deb_arm64 = Some(n.clone());
            } else if (is_amd64ish(n) || l.contains("amd64")) && slots.deb_amd64.is_none() {
                slots.deb_amd64 = Some(n.clone());
            }
        }
    }

    for n in &names {
        let l = lower(n);
        if l.ends_with(".tar.gz") || l.ends_with(".tgz") {
            if is_arm64ish(n) && slots.tar_arm64.is_none() {
                slots.tar_arm64 = Some(n.clone());
            } else if is_amd64ish(n) && slots.tar_amd64.is_none() {
                slots.tar_amd64 = Some(n.clone());
            }
        }
    }

    let mut pkgs: Vec<String> = names
        .iter()
        .filter(|n| lower(n).ends_with(".pkg"))
        .cloned()
        .collect();
    pkgs.sort();

    for n in &pkgs {
        if lower(n).contains("universal") {
            if slots.mac_intel_pkg.is_none() {
                slots.mac_intel_pkg = Some(n.clone());
            }
            if slots.mac_m_pkg.is_none() {
                slots.mac_m_pkg = Some(n.clone());
            }
        }
    }
    for n in &pkgs {
        if lower(n).contains("universal") {
            continue;
        }
        if is_apple_silicon(n) {
            if slots.mac_m_pkg.is_none() {
                slots.mac_m_pkg = Some(n.clone());
            }
        } else if (is_amd64ish(n) || lower(n).contains("intel")) && slots.mac_intel_pkg.is_none() {
            slots.mac_intel_pkg = Some(n.clone());
        }
    }
    for n in &pkgs {
        if lower(n).contains("universal") {
            continue;
        }
        if slots.mac_intel_pkg.is_none() {
            slots.mac_intel_pkg = Some(n.clone());
        } else if slots.mac_m_pkg.is_none() {
            slots.mac_m_pkg = Some(n.clone());
        }
    }

    let app_zips: Vec<_> = names
        .iter()
        .filter(|n| {
            let l = lower(n);
            l.ends_with(".zip")
                && (l.contains(".app") || l.contains("darwin") || l.contains("macos"))
        })
        .cloned()
        .collect();

    for n in &app_zips {
        if lower(n).contains("universal") {
            if slots.mac_intel_app_zip.is_none() {
                slots.mac_intel_app_zip = Some(n.clone());
            }
            if slots.mac_m_app_zip.is_none() {
                slots.mac_m_app_zip = Some(n.clone());
            }
        }
    }
    for n in &app_zips {
        if lower(n).contains("universal") {
            continue;
        }
        if is_apple_silicon(n) {
            if slots.mac_m_app_zip.is_none() {
                slots.mac_m_app_zip = Some(n.clone());
            }
        } else if is_intelish(n) && slots.mac_intel_app_zip.is_none() {
            slots.mac_intel_app_zip = Some(n.clone());
        }
    }
    for n in &app_zips {
        if lower(n).contains("universal") {
            continue;
        }
        if slots.mac_intel_app_zip.is_none() {
            slots.mac_intel_app_zip = Some(n.clone());
        } else if slots.mac_m_app_zip.is_none() {
            slots.mac_m_app_zip = Some(n.clone());
        }
    }

    slots
}

fn encode_path_segment(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(*b as char);
            }
            _ => {
                use std::fmt::Write;
                let _ = write!(out, "%{:02X}", b);
            }
        }
    }
    out
}

pub(crate) fn github_release_download_url(
    owner: &str,
    repo: &str,
    tag: &str,
    file_name: &str,
) -> String {
    format!(
        "https://github.com/{}/{}/releases/download/{}/{}",
        encode_path_segment(owner),
        encode_path_segment(repo),
        encode_path_segment(tag),
        encode_path_segment(file_name)
    )
}

fn esc_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
}

fn sub_linked_img(href: &str, img_src: &str, height: &str, title: &str) -> String {
    format!(
        r#"<sub><a href="{}"><img src="{}" height="{}" title="{}"/></a></sub>"#,
        esc_attr(href),
        img_src,
        height,
        esc_attr(title)
    )
}

fn sub_disabled_img(img_src: &str, height: &str) -> String {
    format!(
        r#"<sub><img src="{}" height="{}" title="{}"/></sub>"#,
        img_src,
        height,
        esc_attr(NOT_AVAILABLE_TITLE)
    )
}

fn sup_link(href: &str, label: &str, title: &str) -> String {
    format!(
        r#"<sup><a href="{}" title="{}">{}</a></sup>"#,
        esc_attr(href),
        esc_attr(title),
        esc_attr(label)
    )
}

fn sup_link_or_grey(
    owner: &str,
    repo: &str,
    tag: &str,
    file: Option<&String>,
    label: &str,
    title: &str,
) -> String {
    match file {
        Some(f) => {
            let url = github_release_download_url(owner, repo, tag, f);
            sup_link(&url, label, title)
        }
        None => format!(
            r#"<sup><span title="{}">{}</span></sup>"#,
            esc_attr(NOT_AVAILABLE_TITLE),
            esc_attr(label)
        ),
    }
}

pub(crate) fn build_quick_downloads_section_html(
    owner: &str,
    repo: &str,
    tag: &str,
    slots: &QuickDownloadSlots,
    footer_message: &str,
) -> String {
    let win_cell = {
        let msi = slots.win_msi.as_ref().map(|f| {
            let u = github_release_download_url(owner, repo, tag, f);
            sub_linked_img(
                &u,
                &format!("{}/msi1.svg", LOGO_BASE),
                "32",
                "Windows MSI installer (recommended)",
            )
        });
        let zip = slots.win_zip.as_ref().map(|f| {
            let u = github_release_download_url(owner, repo, tag, f);
            sub_linked_img(
                &u,
                &format!("{}/zip.svg", LOGO_BASE),
                "32",
                "Portable Archive",
            )
        });
        let msi_s =
            msi.unwrap_or_else(|| sub_disabled_img(&format!("{}/msi1.svg", LOGO_BASE), "32"));
        let zip_s =
            zip.unwrap_or_else(|| sub_disabled_img(&format!("{}/zip.svg", LOGO_BASE), "32"));
        format!("{msi_s}<br><br>{zip_s}")
    };

    let linux_cell = format!(
        r#"<img src="https://upload.wikimedia.org/wikipedia/commons/7/73/App-image-logo.svg" height="31" title="AppImage"/>  <img src="https://cdn.brandfetch.io/idJz03xsbD/theme/dark/logo.svg?c=1bxid64Mup7aczewSAYMX" height="30" title="AppImage may be executed on any Linux, but it's mainly used in: Arch / Manjaro / EndeavourOS / NixOS / Gentoo / etc..."/>    <sup>➠</sup>   {} <sup>/</sup> {}<br>     <sub><img src="https://fedoraproject.org/w/uploads/2/2d/Logo_fedoralogo.png" height="30" title="RPM installer for Fedora/RHEL/SUSE family"/></sub>       <sup>➠</sup>   {} <sup>/</sup> {}<br>          <sub><img src="{}/ubuntu.svg" height="32" title="Ubuntu DEB installer"/></sub>   <img src="{}/debian.svg" height="27" title="Debian DEB installer"/>          <sup>➠</sup>   {} <sup>/</sup> {}<br>               <sub><img src="{}/tar.svg" height="30" title="Archived Portable (gz.tar)"/></sub>            <sup>➠</sup>   {} <sup>/</sup> {}"#,
        sup_link_or_grey(
            owner,
            repo,
            tag,
            slots.appimage_x86_64.as_ref(),
            "x86_64",
            "x86_64 → AMD/Intel"
        ),
        sup_link_or_grey(
            owner,
            repo,
            tag,
            slots.appimage_aarch64.as_ref(),
            "aarch64",
            "aarch64 → ARM"
        ),
        sup_link_or_grey(
            owner,
            repo,
            tag,
            slots.rpm_x86_64.as_ref(),
            "x86_64",
            "x86_64 → AMD/Intel CPU"
        ),
        sup_link_or_grey(
            owner,
            repo,
            tag,
            slots.rpm_aarch64.as_ref(),
            "aarch64",
            "aarch64 → ARM"
        ),
        LOGO_BASE,
        LOGO_BASE,
        sup_link_or_grey(
            owner,
            repo,
            tag,
            slots.deb_amd64.as_ref(),
            "amd64",
            "amd64 → AMD/Intel CPU"
        ),
        sup_link_or_grey(
            owner,
            repo,
            tag,
            slots.deb_arm64.as_ref(),
            "arm64",
            "arm64 = ARM CPU, e.g. Raspberry Pi"
        ),
        LOGO_BASE,
        sup_link_or_grey(
            owner,
            repo,
            tag,
            slots.tar_amd64.as_ref(),
            "amd64",
            "amd64 → AMD/Intel CPU"
        ),
        sup_link_or_grey(
            owner,
            repo,
            tag,
            slots.tar_arm64.as_ref(),
            "arm64",
            "arm64 = ARM CPU, e.g. Raspberry Pi"
        ),
    );

    let mac_pkg_app = |pkg: Option<&String>, app: Option<&String>| {
        let pkg_s = match pkg {
            Some(f) => {
                let u = github_release_download_url(owner, repo, tag, f);
                sub_linked_img(
                    &u,
                    &format!("{}/pkg.svg", LOGO_BASE),
                    "32",
                    "A .pkg installer (non-portable) - recommended",
                )
            }
            None => sub_disabled_img(&format!("{}/pkg.svg", LOGO_BASE), "32"),
        };
        let app_s = match app {
            Some(f) => {
                let u = github_release_download_url(owner, repo, tag, f);
                sub_linked_img(
                    &u,
                    &format!("{}/app.svg", LOGO_BASE),
                    "32",
                    "A zipped .app bundle (portable)",
                )
            }
            None => sub_disabled_img(&format!("{}/app.svg", LOGO_BASE), "32"),
        };
        format!("{pkg_s}      {app_s}")
    };

    let mac_cell = format!(
        r#"<span title="Extra details shown on hover">INTEL-era</span><sup><sub>   (2020-)</sub></sup><br>{}<br><br><span title="Extra details shown on hover">M-era</span><sup><sub>   (2020+)</sub></sup><br>{}"#,
        mac_pkg_app(
            slots.mac_intel_pkg.as_ref(),
            slots.mac_intel_app_zip.as_ref()
        ),
        mac_pkg_app(slots.mac_m_pkg.as_ref(), slots.mac_m_app_zip.as_ref())
    );

    let footer_esc = footer_message.trim();
    format!(
        r#"<div align="center">

|⟱  Q U I C K - D O W N L O A D S        A V A I L A B L E        H E R E  ⟱|
|-|

|   <sub><img src="https://cdn.brandfetch.io/idO_D7E2El/theme/dark/logo.svg?c=1bxid64Mup7aczewSAYMX&t=1756706346242" height="24" title="Should work also below Win11. Please open Issue if you have any problems."/></sub>   |✪|<sub><img src="https://www.svgrepo.com/show/448236/linux.svg" height="30" /></sub> <sup>Linux Distributions</sup>|✪|<sub><img src="https://www.svgrepo.com/show/303125/apple-logo.svg" height="24" /></sub> <sub><sup>macOS</sup></sub>|
|:-:|:-:|-|:-:|:-:|
|{win_cell}|‧<br>✦<br>‧<br>✦<br>‧<br>✦<br>‧|{linux_cell}|‧<br>✦<br>‧<br>✦<br>‧<br>✦<br>‧|{mac_cell}|

<sub><sup>{} </sub></sup>

</div>"#,
        esc_attr(footer_esc)
    )
}

/// When Quick-Downloads is enabled, merges the HTML section with user notes.
/// Emits warning strings (e.g. unparsable remote) for progress logging.
pub(crate) fn finalize_release_notes_with_quick_downloads(
    user_notes_markdown: Option<String>,
    remote_url: Option<&str>,
    tag: &str,
    artifact_files: &[String],
    qd: &ReleaseNowQuickDownloadsSettings,
    warnings: &mut Vec<String>,
) -> Option<String> {
    let user_trim = user_notes_markdown
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    if !qd.enabled {
        return user_trim;
    }

    let Some(url) = remote_url.map(str::trim).filter(|u| !u.is_empty()) else {
        warnings.push(
            "Quick-Downloads skipped: no remote URL is configured for this scope.".to_string(),
        );
        return user_trim;
    };

    let Some((owner, repo)) = crate::git::github_owner_repo_from_remote_url(url) else {
        warnings.push(format!(
            "Quick-Downloads skipped: remote URL is not a recognized GitHub SSH/HTTPS URL: {url}"
        ));
        return user_trim;
    };

    let slots = assign_artifacts_to_slots(artifact_files);
    let html =
        build_quick_downloads_section_html(&owner, &repo, tag, &slots, qd.footer_message.trim());

    let merged = match qd.position {
        QuickDownloadsPosition::Top => match user_trim {
            Some(u) => format!("{html}\n\n{u}"),
            None => html,
        },
        QuickDownloadsPosition::Bottom => match user_trim {
            Some(u) => format!("{u}\n\n{html}"),
            None => html,
        },
    };

    Some(merged)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        DEFAULT_QUICK_DOWNLOADS_FOOTER, QuickDownloadsPosition, ReleaseNowQuickDownloadsSettings,
    };

    #[test]
    fn github_release_download_url_encodes_segments() {
        let u = github_release_download_url("comfy-home", "ComfyGit", "v0.1.2", "a b.msi");
        assert!(u.contains("v0.1.2"));
        assert!(u.contains("a%20b.msi") || u.contains("releases/download"));
    }

    #[test]
    fn assign_windows_msi_and_zip() {
        let s = assign_artifacts_to_slots(&[
            "/dist/latest/x/foo-1.0-windows-x64.msi".to_string(),
            "/dist/latest/x/app-windows-x64.zip".to_string(),
        ]);
        assert_eq!(s.win_msi.as_deref(), Some("foo-1.0-windows-x64.msi"));
        assert_eq!(s.win_zip.as_deref(), Some("app-windows-x64.zip"));
    }

    #[test]
    fn assign_appimage_arch() {
        let s = assign_artifacts_to_slots(&[
            "ComfyGit-x86_64.AppImage".to_string(),
            "ComfyGit-aarch64.AppImage".to_string(),
        ]);
        assert!(s.appimage_x86_64.is_some());
        assert!(s.appimage_aarch64.is_some());
    }

    #[test]
    fn missing_slots_still_produce_html() {
        let slots = QuickDownloadSlots::default();
        let html = build_quick_downloads_section_html(
            "o",
            "r",
            "v1",
            &slots,
            DEFAULT_QUICK_DOWNLOADS_FOOTER,
        );
        assert!(html.contains("<div align=\"center\">"));
        assert!(html.contains(NOT_AVAILABLE_TITLE) || html.contains("&#x1f6ab;"));
    }

    #[test]
    fn finalize_merges_qd_top_and_bottom() {
        let mut warnings = Vec::new();
        let mut qd = ReleaseNowQuickDownloadsSettings {
            enabled: true,
            position: QuickDownloadsPosition::Top,
            footer_message: "ft".to_string(),
        };
        let out = finalize_release_notes_with_quick_downloads(
            Some("# Notes\n".to_string()),
            Some("git@github.com:acme/demo.git"),
            "v1",
            &["x-win.msi".to_string()],
            &qd,
            &mut warnings,
        )
        .expect("merged notes");
        assert!(out.starts_with("<div"));
        assert!(out.contains("# Notes"));

        qd.position = QuickDownloadsPosition::Bottom;
        let out2 = finalize_release_notes_with_quick_downloads(
            Some("# Notes\n".to_string()),
            Some("git@github.com:acme/demo.git"),
            "v1",
            &["x-win.msi".to_string()],
            &qd,
            &mut warnings,
        )
        .expect("merged notes");
        assert!(out2.starts_with("# Notes"));
        assert!(out2.contains("<div"));
    }
}
