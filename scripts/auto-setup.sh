#!/usr/bin/env bash
set -euo pipefail
# Copyright © 2026 ComfyHome™
# All rights reserved.
#
# Licensed under the ComfyGit SA-PS License
#
# For details, see the LICENSE file in the repository root.
# Installation/update script for ComfyGit (Linux/macOS).
# Windows should use a dedicated PowerShell script.

APP_NAME="${APP_NAME:-comfygit}"
PRIMARY_BIN="${PRIMARY_BIN:-comfygit}"
ALIAS_BIN="${ALIAS_BIN:-cg}"
ALIAS_BIN_ENABLED="${ALIAS_BIN_ENABLED:-true}"
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"
TMP_ROOT="${TMP_ROOT:-${TMPDIR:-/tmp}}"
SHELL_INSTALL_CMD="${SHELL_INSTALL_CMD:-install-shell}"
APPIMAGE_SHELL_INSTALL_ON_FRESH="${APPIMAGE_SHELL_INSTALL_ON_FRESH:-true}"
DEBUG="${DEBUG:-false}"

# Optional private-release auth
PRIVATE_REPO="${PRIVATE_REPO:-false}"
RELEASE_TOKEN="${RELEASE_TOKEN:-}"
GITHUB_TOKEN="${GITHUB_TOKEN:-}"
GITLAB_TOKEN="${GITLAB_TOKEN:-}"
TOKEN_HEADER_NAME="${TOKEN_HEADER_NAME:-}"

# GitHub defaults
GITHUB_OWNER="${GITHUB_OWNER:-comfy-home}"
GITHUB_REPO="${GITHUB_REPO:-ComfyGit}"
GITHUB_LATEST_URL="${GITHUB_LATEST_URL:-https://github.com/${GITHUB_OWNER}/${GITHUB_REPO}/releases/latest}"

# GitLab defaults
GITLAB_PROJECT_PATH="${GITLAB_PROJECT_PATH:-comfyhome%2Fdist%2FComfyGit}"
GITLAB_API_BASE="${GITLAB_API_BASE:-https://gitlab.com/api/v4/projects/${GITLAB_PROJECT_PATH}}"
GITLAB_TAGS_API="${GITLAB_TAGS_API:-${GITLAB_API_BASE}/repository/tags}"
GITLAB_RELEASE_BASE="${GITLAB_RELEASE_BASE:-https://gitlab.com/${GITLAB_PROJECT_PATH//%2F//}/-/releases}"

CUSTOM_RELEASE_BASE="${CUSTOM_RELEASE_BASE:-}"
SOURCE="${SOURCE:-}"
ASSET_URL="${ASSET_URL:-}"
ASSET_NAME="${ASSET_NAME:-}"
PACKAGE_KIND_OVERRIDE="${PACKAGE_KIND_OVERRIDE:-}"
DRY_RUN="${DRY_RUN:-false}"
YES="${YES:-false}"
INSTALL_DIR_EXPLICIT=false

PLATFORM=""
ARCH=""
DISTRO_ID="unknown"
DISTRO_LIKE=""
PACKAGE_KIND=""
ASSET_FILE=""
RESOLVED_ASSET_URL=""
RESOLVED_TAG=""
RESOLVED_ASSET_FILENAME=""
MODE="install"
CURRENT_BIN_PATH=""

usage() {
  cat <<'EOF'
Usage: ./scripts/auto-setup.sh [options]

Options:
  --source github|gitlab|custom   Release source (if omitted, asks interactively)
  --custom-base URL               Base URL for custom release hosting
  --asset-url URL                 Full direct asset URL (custom source shortcut)
  --asset-name NAME               Asset filename (used with custom-base)
  --package deb|rpm|appimage|pkg  Override package selection
  --private                       Enable token auth for private repos
  --token TOKEN                   Generic token fallback (when source token missing)
  --github-token TOKEN            GitHub token (preferred when --source github)
  --gitlab-token TOKEN            GitLab token (preferred when --source gitlab)
  --token-header NAME             Custom token header (default: source-specific)
  --install-dir DIR               Install destination for AppImage/manual mode
  --yes                           Non-interactive mode (accept defaults)
  --dry-run                       Print actions without downloading/installing
  --debug                         Verbose debug output (set -x)
  -h, --help                      Show this help
EOF
}

log() { printf '%s\n' "$*"; }
warn() { printf 'WARN: %s\n' "$*" >&2; }
err() { printf 'ERROR: %s\n' "$*" >&2; exit 1; }

is_true() { [[ "${1,,}" == "1" || "${1,,}" == "true" || "${1,,}" == "yes" || "${1,,}" == "on" ]]; }

require_cmd() {
  local cmd="$1"
  command -v "$cmd" >/dev/null 2>&1 || err "Required command not found: $cmd"
}

parse_args() {
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --source) SOURCE="${2:-}"; shift 2 ;;
      --custom-base) CUSTOM_RELEASE_BASE="${2:-}"; shift 2 ;;
      --asset-url) ASSET_URL="${2:-}"; shift 2 ;;
      --asset-name) ASSET_NAME="${2:-}"; shift 2 ;;
      --package) PACKAGE_KIND_OVERRIDE="${2:-}"; shift 2 ;;
      --private) PRIVATE_REPO=true; shift ;;
      --token) RELEASE_TOKEN="${2:-}"; shift 2 ;;
      --github-token) GITHUB_TOKEN="${2:-}"; shift 2 ;;
      --gitlab-token) GITLAB_TOKEN="${2:-}"; shift 2 ;;
      --token-header) TOKEN_HEADER_NAME="${2:-}"; shift 2 ;;
      --install-dir) INSTALL_DIR="${2:-}"; INSTALL_DIR_EXPLICIT=true; shift 2 ;;
      --yes) YES=true; shift ;;
      --dry-run) DRY_RUN=true; shift ;;
      --debug) DEBUG=true; shift ;;
      -h|--help) usage; exit 0 ;;
      *) err "Unknown argument: $1" ;;
    esac
  done
}

setup_debug() {
  if is_true "$DEBUG"; then
    set -x
  fi
}

detect_platform() {
  local os arch
  os="$(uname -s)"
  arch="$(uname -m)"
  case "$os" in
    Linux) PLATFORM="linux" ;;
    Darwin) PLATFORM="macos" ;;
    *) err "Unsupported OS: $os (Windows should use a PowerShell installer)" ;;
  esac
  case "$arch" in
    x86_64|amd64) ARCH="amd64" ;;
    arm64|aarch64) ARCH="aarch64" ;;
    *) err "Unsupported architecture: $arch" ;;
  esac
}

detect_linux_distro() {
  if [[ "$PLATFORM" != "linux" ]]; then
    return
  fi
  if [[ -f /etc/os-release ]]; then
    DISTRO_ID="$(sed -n 's/^ID=//p' /etc/os-release | tr -d '"' | head -n1)"
    DISTRO_LIKE="$(sed -n 's/^ID_LIKE=//p' /etc/os-release | tr -d '"' | head -n1)"
  fi
}

detect_mode_and_existing_install() {
  CURRENT_BIN_PATH="$(command -v "$PRIMARY_BIN" 2>/dev/null || true)"
  if [[ -z "$CURRENT_BIN_PATH" ]] && is_true "$ALIAS_BIN_ENABLED"; then
    CURRENT_BIN_PATH="$(command -v "$ALIAS_BIN" 2>/dev/null || true)"
  fi
  if [[ -n "$CURRENT_BIN_PATH" ]]; then
    MODE="update"
    if [[ "$INSTALL_DIR_EXPLICIT" == false ]]; then
      INSTALL_DIR="$(dirname "$CURRENT_BIN_PATH")"
    fi
  else
    MODE="install"
  fi
}

choose_package_kind() {
  if [[ -n "$PACKAGE_KIND_OVERRIDE" ]]; then
    PACKAGE_KIND="$PACKAGE_KIND_OVERRIDE"
    return
  fi
  if [[ "$PLATFORM" == "macos" ]]; then
    PACKAGE_KIND="pkg"
    return
  fi
  if [[ "$DISTRO_ID" =~ ^(debian|ubuntu|linuxmint|pop)$ ]] || [[ "$DISTRO_LIKE" == *debian* ]]; then
    PACKAGE_KIND="deb"
  elif [[ "$DISTRO_ID" =~ ^(fedora|rhel|centos|rocky|almalinux|opensuse|sles)$ ]] || [[ "$DISTRO_LIKE" == *rhel* ]] || [[ "$DISTRO_LIKE" == *fedora* ]] || [[ "$DISTRO_LIKE" == *suse* ]]; then
    PACKAGE_KIND="rpm"
  elif command -v dpkg >/dev/null 2>&1; then
    PACKAGE_KIND="deb"
  elif command -v rpm >/dev/null 2>&1; then
    PACKAGE_KIND="rpm"
  else
    PACKAGE_KIND="appimage"
  fi
}

resolve_asset_filename() {
  case "$PACKAGE_KIND" in
    deb) ASSET_FILE="${APP_NAME}-{VERSION}-linux-$([[ "$ARCH" == "amd64" ]] && echo amd64 || echo arm64).deb" ;;
    rpm) ASSET_FILE="${APP_NAME}-{VERSION}-linux-$([[ "$ARCH" == "amd64" ]] && echo x86_64 || echo aarch64).rpm" ;;
    appimage) ASSET_FILE="${APP_NAME}-{VERSION}-$([[ "$ARCH" == "amd64" ]] && echo x86_64 || echo aarch64).AppImage" ;;
    pkg) ASSET_FILE="${APP_NAME}-{VERSION}-macos-$([[ "$ARCH" == "amd64" ]] && echo x86_64 || echo aarch64).pkg" ;;
    *) err "Unsupported package kind: $PACKAGE_KIND" ;;
  esac
}

prompt_source_if_needed() {
  if [[ -n "$SOURCE" ]]; then return; fi
  if [[ "$YES" == true ]]; then SOURCE="github"; return; fi
  log "Choose release source:"
  log "  1) GitHub"
  log "  2) GitLab"
  log "  3) Custom URL"
  printf "Selection [1-3]: "
  read -r sel
  case "$sel" in
    1) SOURCE="github" ;;
    2) SOURCE="gitlab" ;;
    3) SOURCE="custom" ;;
    *) err "Invalid selection: $sel" ;;
  esac
}

token_for_source() {
  case "$SOURCE" in
    github) printf '%s\n' "${GITHUB_TOKEN:-${RELEASE_TOKEN:-}}" ;;
    gitlab) printf '%s\n' "${GITLAB_TOKEN:-${RELEASE_TOKEN:-}}" ;;
    *) printf '%s\n' "${RELEASE_TOKEN:-${GITHUB_TOKEN:-${GITLAB_TOKEN:-}}}" ;;
  esac
}

get_auth_header() {
  local token
  if ! is_true "$PRIVATE_REPO"; then
    return 0
  fi
  token="$(token_for_source)"
  [[ -n "$token" ]] || err "Private mode enabled but no token provided for source '$SOURCE' (--token, --github-token, --gitlab-token)"
  if [[ -n "$TOKEN_HEADER_NAME" ]]; then
    printf '%s\n' "${TOKEN_HEADER_NAME}: ${token}"
    return 0
  fi
  case "$SOURCE" in
    gitlab) printf '%s\n' "PRIVATE-TOKEN: ${token}" ;;
    *) printf '%s\n' "Authorization: Bearer ${token}" ;;
  esac
}

curl_run() {
  local header="" hflag=()
  header="$(get_auth_header || true)"
  if [[ -n "$header" ]]; then
    hflag=(-H "$header")
  fi
  curl "${hflag[@]}" "$@"
}

latest_tag_from_github() {
  local api_url tag
  api_url="https://api.github.com/repos/${GITHUB_OWNER}/${GITHUB_REPO}/releases/latest"
  tag="$(curl_run -fsSL "$api_url" | sed -n 's/.*"tag_name":[[:space:]]*"\([^"]*\)".*/\1/p' | head -n1 || true)"
  if [[ -n "$tag" ]]; then
    printf '%s\n' "$tag"
    return 0
  fi
  curl_run -fsSLI "$GITHUB_LATEST_URL" | awk -F'/' 'tolower($1) ~ /^location:/ {gsub("\r","",$NF); print $NF}' | tail -n1
}

latest_tag_from_gitlab() {
  curl_run -fsSL "$GITLAB_TAGS_API?per_page=1" | sed -n 's/.*"name":"\([^"]*\)".*/\1/p' | head -n1
}

gitlab_api_download_url() {
  local tag="$1" asset_path="$2"
  asset_path="${asset_path#/}"
  printf '%s\n' "${GITLAB_API_BASE}/releases/${tag}/downloads/${asset_path}"
}

gitlab_web_download_url() {
  local tag="$1" asset_path="$2"
  asset_path="${asset_path#/}"
  printf '%s\n' "${GITLAB_RELEASE_BASE}/${tag}/downloads/${asset_path}"
}

append_private_token_query() {
  local url="$1" token="$2"
  if [[ -z "$token" ]] || [[ "$url" == *private_token=* ]]; then
    printf '%s\n' "$url"
    return
  fi
  if [[ "$url" == *\?* ]]; then
    printf '%s\n' "${url}&private_token=${token}"
  else
    printf '%s\n' "${url}?private_token=${token}"
  fi
}

curl_gitlab_authed_download() {
  local url="$1" target="$2"
  local token="" authed_url hflag=()

  if is_true "$PRIVATE_REPO"; then
    token="$(token_for_source)"
    [[ -n "$token" ]] || err "Private mode enabled but no GitLab token available"
    hflag=(-H "PRIVATE-TOKEN: ${token}")
    authed_url="$(append_private_token_query "$url" "$token")"
  else
    authed_url="$url"
  fi

  if [[ "$DRY_RUN" == true ]]; then
    log "[dry-run] curl -fL ${hflag[*]+"${hflag[*]}"} '$authed_url' -o '$target'"
    return 0
  fi

  curl "${hflag[@]}" -fL "$authed_url" -o "$target"
}

collect_gitlab_download_urls() {
  local tag="$1" filename="$2"
  local release_json path link_url

  printf '%s\n' "$(gitlab_api_download_url "$tag" "$filename")"
  printf '%s\n' "$(gitlab_web_download_url "$tag" "$filename")"

  release_json="$(curl_run -fsSL "${GITLAB_API_BASE}/releases/${tag}")" || return 0
  while IFS= read -r path; do
    [[ -n "$path" ]] || continue
    printf '%s\n' "$(gitlab_api_download_url "$tag" "$path")"
    printf '%s\n' "$(gitlab_web_download_url "$tag" "$path")"
  done < <(printf '%s' "$release_json" | grep -o '"direct_asset_path":"[^"]*"' | sed 's/"direct_asset_path":"//;s/"$//')

  link_url="$(printf '%s' "$release_json" | tr ',' '\n' | grep -F "$filename" | grep '"url"' | head -n1 | sed -n 's/.*"url":"\([^"]*\)".*/\1/p')"
  [[ -n "$link_url" ]] && printf '%s\n' "$link_url"
}

download_gitlab_release_asset() {
  local target="$1" tag="$2" filename="$3"
  local url tried=0

  while IFS= read -r url; do
    [[ -n "$url" ]] || continue
    tried=1
    log "Downloading: $url"
    if curl_gitlab_authed_download "$url" "$target"; then
      RESOLVED_ASSET_URL="$url"
      return 0
    fi
    if is_true "$DEBUG"; then
      warn "Download attempt failed for: $url"
    fi
  done < <(collect_gitlab_download_urls "$tag" "$filename")

  [[ "$tried" -eq 1 ]] || err "Could not resolve any GitLab download URL for '${filename}' (release ${tag})"
  err "Could not download '${filename}' for release '${tag}' (token may lack read_api scope, or asset path mismatch)"
}

resolve_asset_url() {
  local tag version filename
  filename="$ASSET_FILE"
  case "$SOURCE" in
    github)
      tag="$(latest_tag_from_github)"
      [[ -n "$tag" ]] || err "Could not resolve latest GitHub tag"
      version="${tag#v}"
      filename="${filename/\{VERSION\}/$version}"
      RESOLVED_ASSET_URL="https://github.com/${GITHUB_OWNER}/${GITHUB_REPO}/releases/download/${tag}/${filename}"
      ;;
    gitlab)
      tag="$(latest_tag_from_gitlab)"
      [[ -n "$tag" ]] || err "Could not resolve latest GitLab tag"
      version="${tag#v}"
      filename="${filename/\{VERSION\}/$version}"
      RESOLVED_TAG="$tag"
      RESOLVED_ASSET_FILENAME="$filename"
      RESOLVED_ASSET_URL="$(gitlab_api_download_url "$tag" "$filename")"
      ;;
    custom)
      if [[ -n "$ASSET_URL" ]]; then
        RESOLVED_ASSET_URL="$ASSET_URL"
      else
        [[ -n "$CUSTOM_RELEASE_BASE" ]] || err "--custom-base required for custom source (or provide --asset-url)"
        [[ -n "$ASSET_NAME" ]] || err "--asset-name is required with --source custom when --asset-url is not provided"
        RESOLVED_ASSET_URL="${CUSTOM_RELEASE_BASE%/}/${ASSET_NAME}"
      fi
      ;;
    *) err "Unsupported source: $SOURCE" ;;
  esac
}

download_asset() {
  local target="$1"
  if [[ "$SOURCE" == "gitlab" ]]; then
    download_gitlab_release_asset "$target" "$RESOLVED_TAG" "$RESOLVED_ASSET_FILENAME"
    return
  fi
  log "Downloading: $RESOLVED_ASSET_URL"
  if [[ "$DRY_RUN" == true ]]; then
    log "[dry-run] curl -fL '$RESOLVED_ASSET_URL' -o '$target'"
    return
  fi
  curl_run -fL "$RESOLVED_ASSET_URL" -o "$target"
}

run_root_command() {
  if [[ "$DRY_RUN" == true ]]; then
    log "[dry-run] $*"
    return
  fi
  if [[ "${EUID:-$(id -u)}" -eq 0 ]]; then "$@"; else require_cmd sudo; sudo "$@"; fi
}

install_deb() {
  local file="$1"
  if command -v apt-get >/dev/null 2>&1; then run_root_command apt-get install -y "$file"; else run_root_command dpkg -i "$file"; fi
}

install_rpm() {
  local file="$1"
  if command -v dnf >/dev/null 2>&1; then
    run_root_command dnf install -y "$file"
  elif command -v yum >/dev/null 2>&1; then
    run_root_command yum install -y "$file"
  elif command -v zypper >/dev/null 2>&1; then
    run_root_command zypper --non-interactive install "$file"
  else
    run_root_command rpm -Uvh "$file"
  fi
}

run_appimage_shell_install_if_needed() {
  local dst_bin="$1"
  if [[ "$MODE" != "install" ]] || ! is_true "$APPIMAGE_SHELL_INSTALL_ON_FRESH"; then
    return
  fi
  if [[ "$DRY_RUN" == true ]]; then
    log "[dry-run] '$dst_bin' '$SHELL_INSTALL_CMD'"
    return
  fi
  if "$dst_bin" "$SHELL_INSTALL_CMD"; then
    log "Ran '$SHELL_INSTALL_CMD' for AppImage fresh installation."
  else
    warn "AppImage installed, but '${SHELL_INSTALL_CMD}' failed."
  fi
}

install_appimage() {
  local file="$1" dst_bin dst_alias
  dst_bin="${INSTALL_DIR}/${PRIMARY_BIN}"
  dst_alias="${INSTALL_DIR}/${ALIAS_BIN}"
  if [[ "$DRY_RUN" == true ]]; then
    log "[dry-run] install -m 0755 '$file' '$dst_bin'"
    if is_true "$ALIAS_BIN_ENABLED"; then
      log "[dry-run] ln -sf '$dst_bin' '$dst_alias'"
    fi
    run_appimage_shell_install_if_needed "$dst_bin"
    return
  fi
  if [[ ! -w "$INSTALL_DIR" ]]; then
    require_cmd sudo
    sudo mkdir -p "$INSTALL_DIR"
    sudo install -m 0755 "$file" "$dst_bin"
    if is_true "$ALIAS_BIN_ENABLED"; then
      sudo ln -sf "$dst_bin" "$dst_alias"
    fi
  else
    mkdir -p "$INSTALL_DIR"
    install -m 0755 "$file" "$dst_bin"
    if is_true "$ALIAS_BIN_ENABLED"; then
      ln -sf "$dst_bin" "$dst_alias"
    fi
  fi
  run_appimage_shell_install_if_needed "$dst_bin"
}

install_pkg() { local file="$1"; run_root_command installer -pkg "$file" -target /; }

install_downloaded_asset() {
  local file="$1"
  case "$PACKAGE_KIND" in
    deb) install_deb "$file" ;;
    rpm) install_rpm "$file" ;;
    appimage) install_appimage "$file" ;;
    pkg) install_pkg "$file" ;;
    *) err "Unsupported package kind for install: $PACKAGE_KIND" ;;
  esac
}

main() {
  parse_args "$@"
  setup_debug
  require_cmd curl
  require_cmd awk
  detect_platform
  detect_linux_distro
  detect_mode_and_existing_install
  choose_package_kind
  resolve_asset_filename
  prompt_source_if_needed
  resolve_asset_url

  local work_dir download_path
  work_dir="$(mktemp -d "${TMP_ROOT%/}/${APP_NAME}-setup.XXXXXX")"
  download_path="${work_dir}/${ASSET_FILE//\{VERSION\}/latest}"

  log "Mode: ${MODE}"
  log "Platform: ${PLATFORM}/${ARCH}"
  if [[ "$PLATFORM" == "linux" ]]; then
    log "Linux distro: ${DISTRO_ID:-unknown}${DISTRO_LIKE:+ (like: ${DISTRO_LIKE})}"
  fi
  log "Package kind: ${PACKAGE_KIND}"
  log "Install dir: ${INSTALL_DIR}"
  log "Source: ${SOURCE}"
  if is_true "$PRIVATE_REPO"; then
    log "Private mode: enabled"
  fi

  download_asset "$download_path"
  install_downloaded_asset "$download_path"
  if [[ "$DRY_RUN" == false ]]; then rm -rf "$work_dir"; fi
  log "Done. '${APP_NAME}' installed/updated."
}

main "$@"
