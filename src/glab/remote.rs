// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit License v1.2

use crate::git::{default_push_remote_name, run_git_checked};

pub fn owner_repo_from_remote_url(remote_url: &str) -> Option<(String, String)> {
    let remote_url = remote_url.trim();
    let path = remote_url
        .strip_prefix("git@gitlab.com:")
        .or_else(|| remote_url.strip_prefix("https://gitlab.com/"))
        .or_else(|| remote_url.strip_prefix("ssh://git@gitlab.com/"))?;

    let path = path.trim_end_matches(".git");
    let mut segments = path.split('/');
    let owner = segments.next()?.trim();
    let repo = segments.next()?.trim();
    if owner.is_empty() || repo.is_empty() || segments.next().is_some() {
        return None;
    }

    Some((owner.to_string(), repo.to_string()))
}

pub fn repository_web_url_from_remote_url(remote_url: &str) -> Option<String> {
    let (owner, repo) = owner_repo_from_remote_url(remote_url)?;
    Some(format!("https://gitlab.com/{}/{}", owner, repo))
}

pub fn repository_web_url(repo_root: &str) -> Option<String> {
    let remote_name = default_push_remote_name(repo_root).ok()?;
    let remote_url = run_git_checked(repo_root, &["remote", "get-url", &remote_name]).ok()?;
    repository_web_url_from_remote_url(remote_url.trim())
}

pub fn merge_request_conflicts_url(repo_root: &str, mr_number: u64) -> Option<String> {
    let repository_url = repository_web_url(repo_root)?;
    Some(format!(
        "{}/-/merge_requests/{}/conflicts",
        repository_url, mr_number
    ))
}

pub fn release_page_url(remote_url: &str, tag: &str) -> Option<String> {
    let (owner, repo) = owner_repo_from_remote_url(remote_url)?;
    Some(format!(
        "https://gitlab.com/{}/{}/-/releases/{}",
        owner, repo, tag
    ))
}

pub fn release_download_url(owner: &str, repo: &str, tag: &str, file_name: &str) -> String {
    format!(
        "https://gitlab.com/{}/{}/-/releases/{}/downloads/{}",
        encode_path_segment(owner),
        encode_path_segment(repo),
        encode_path_segment(tag),
        encode_path_segment(file_name)
    )
}

fn encode_path_segment(segment: &str) -> String {
    segment
        .chars()
        .map(|ch| match ch {
            ' ' => "%20".to_string(),
            c if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~') => c.to_string(),
            c => {
                let mut encoded = String::new();
                for byte in c.to_string().as_bytes() {
                    encoded.push_str(&format!("%{:02X}", byte));
                }
                encoded
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repository_web_url_parser_accepts_https_and_ssh_formats() {
        assert_eq!(
            repository_web_url_from_remote_url(
                "https://gitlab.com/my-group/my-project.git"
            )
            .as_deref(),
            Some("https://gitlab.com/my-group/my-project")
        );
        assert_eq!(
            repository_web_url_from_remote_url("git@gitlab.com:my-group/my-project.git").as_deref(),
            Some("https://gitlab.com/my-group/my-project")
        );
    }

    #[test]
    fn repository_web_url_parser_rejects_non_gitlab_remotes() {
        assert!(
            repository_web_url_from_remote_url("https://github.com/org/repo.git").is_none()
        );
    }

    #[test]
    fn owner_repo_from_remote_url_parses_ssh_https_and_ssh_scheme() {
        assert_eq!(
            owner_repo_from_remote_url("git@gitlab.com:group/project.git"),
            Some(("group".to_string(), "project".to_string()))
        );
        assert_eq!(
            owner_repo_from_remote_url("https://gitlab.com/foo/bar.git"),
            Some(("foo".to_string(), "bar".to_string()))
        );
        assert_eq!(
            owner_repo_from_remote_url("ssh://git@gitlab.com/org/repo.git"),
            Some(("org".to_string(), "repo".to_string()))
        );
    }

    #[test]
    fn release_page_url_from_ssh() {
        let url = release_page_url("git@gitlab.com:group/project.git", "v1.0.0");
        assert_eq!(
            url.as_deref(),
            Some("https://gitlab.com/group/project/-/releases/v1.0.0")
        );
    }

    #[test]
    fn release_download_url_encodes_segments() {
        let u = release_download_url("group", "project", "v0.1.2", "a b.msi");
        assert!(u.contains("a%20b.msi"));
        assert!(u.contains("/-/releases/"));
    }
}
