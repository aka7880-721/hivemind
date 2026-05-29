#!/usr/bin/env bash
# Copyright © 2026 ComfyHome™
# All rights reserved.
# 
# Licensed under the ComfyGit SA-PS License
#
# For details, see the LICENSE file in the repository root.

# Usage:
#   ./scripts/releaseNOW.sh                      # Build for current platform only
#   ./scripts/releaseNOW.sh --win64              # Build for Windows (x86_64)
#   ./scripts/releaseNOW.sh --linux              # Build for Linux (both amd64 and arm64)
#   ./scripts/releaseNOW.sh --linux=amd64        # Build for Linux amd64 only
#   ./scripts/releaseNOW.sh --linux=arm64       # Build for Linux arm64 only
#   ./scripts/releaseNOW.sh --mac               # Build for macOS (Intel + Apple Silicon)
#   ./scripts/releaseNOW.sh --mac-intel         # Build for macOS x86_64 only
#   ./scripts/releaseNOW.sh --mac-silicon       # Build for macOS arm64 only
#   ./scripts/releaseNOW.sh --all               # Build for all platforms
#   ./scripts/releaseNOW.sh --test-only         # Run fmt/clippy/test only, skip packaging
#   ./scripts/releaseNOW.sh --no-checks       # Skip fmt/clippy/test checks
#   ./scripts/releaseNOW.sh --skip-test         # Skip tests
#   ./scripts/releaseNOW.sh --skip-msi          # Skip MSI generation
#   ./scripts/releaseNOW.sh --skip-appimage     # Skip Linux AppImage (.AppImage) packaging
#   ./scripts/releaseNOW.sh --skip-deb          # Skip Linux .deb package
#   ./scripts/releaseNOW.sh --skip-rpm          # Skip Linux .rpm package
#   ./scripts/releaseNOW.sh --mac-ci-no-wait    # macOS via GHA: trigger only, no download
#   ./scripts/releaseNOW.sh --linux=arm64 --skip-deb --skip-rpm   # AppImage (and archives) only
#
# Previous releases are kept under dist/old/<platform>/<version>/ (e.g. dist/old/linux-amd64/0.18.1/).
#
# Linux AppImages (x86_64 / aarch64): native builds use appimagetool on PATH.
# Cross-arch uses Podman + Fedora target-arch images; appimagetool is fetched as the
# upstream .AppImage but extracted with 7z (inner usr/bin/appimagetool) so qemu-user
# does not try to execute the AppImage ELF wrapper.
#
# AppImage:
# - first run must be `./comfygit-*.AppImage install-shell`
# - subsequent runs can be `cg` or `comfygit`
# - to remove AppImage-installed shell integration from the system:
#   - run `./comfygit-*.AppImage uninstall-shell`
#   * alternatively, run `cg uninstall-shell` from the AppImage
#   ** This is especially important if you used the AppImage to install the shell integration and now want to remove it and install it again via the package manager. If you do not do this, the shell integration will not work correctly.

set -euo pipefail

# Error handling: exit on any error
ErrorActionPreference() {
    return 0
}

############################################################
#        DEFINE VARIABLES IN THE FOLLOWING SECTION:          #
############################################################

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
DIST_ROOT="${PROJECT_ROOT}/dist"
BIN_NAME='cg'
DELEGATE_BIN_NAME='ComfyGit'
NAME='ComfyGit'
MANUFACTURER='ComfyHome'
MAINTAINER='ComfyHome <support@comfyhome.io>'
DESCRIPTION='Project management done differently. ComfyGit is a tool to help you manage and automate versioning and changelog generation for your projects. It integrates with your git repository and provides a simple interface for bumping versions, generating release notes, changelogs, quick CLI actions, branch management, and much more...'
LICENSE='SA-PS'
PACKAGE='comfygit'
PACKAGE_ID='com.comfyhome.comfygit'
# Linux AppImage (.AppImage) icon
APP_IMAGE_ICON_PATH="${PROJECT_ROOT}/assets/logos-3rd-party/portable.png"
APP_IMAGE_ICON_NAME='portable'
APP_IMAGE_ICON_EXTENSION='png'
# appimagetool AppImage releases (used inside podman for cross-arch AppImage builds)
APPIMAGETOOL_UPSTREAM_URL='https://github.com/AppImage/AppImageKit/releases/download/continuous'

# WiX/MSI GUIDs (must remain stable for Windows Installer upgrade detection)
UPGRADE_CODE_GUID='C5A2D8A8-9DAB-4D4E-84F3-065086D36B89'
EXE_GUID='F9B8F26B-8E1B-4622-89D8-8D3DB7BA4012'
DELEGATE_EXE_GUID='4C6141A2-72F5-4926-9D9C-7B57EF7D276D'
README_GUID='D11D6877-3D25-4BA0-A52F-262C045F9135'
LICENSE_GUID='A6902A1B-00A6-4AE9-B1EA-0EF5113B4E35'
SHELL_PS1_GUID='2D3DB5B3-5E4E-4B08-98AF-0CE2922E5118'
SHELL_CMD_GUID='7E283F53-7E67-4D4A-99E4-06401DE0B35F'
INSTALL_PS1_GUID='38FFCE6E-41D2-48EB-950D-5F6E23B5C2C4'
SHELL_MODULE_GUID='A1B2C3D4-E5F6-47A8-B9C0-1D2E3F4A5B6C'

############################################################
#                END OF VARIABLE DEFINITIONS                 #
############################################################

# Parse command-line arguments
TEST_ONLY=false
NO_CHECKS=false
SKIP_TEST=false
SKIP_MSI=false
SKIP_APPIMAGE=false
SKIP_DEB=false
SKIP_RPM=false
LINUX=''
MAC=false
MAC_ARCH='both'
WIN64=false
ALL=false
MAC_CI_WAIT=true

while [[ $# -gt 0 ]]; do
    case $1 in
        --test|--test-only)
            TEST_ONLY=true
            shift
            ;;
        --no-checks)
            NO_CHECKS=true
            shift
            ;;
        --skip-test|--skip-tests)
            SKIP_TEST=true
            shift
            ;;
        --skip-msi)
            SKIP_MSI=true
            shift
            ;;
        --skip-appimage)
            SKIP_APPIMAGE=true
            shift
            ;;
        --skip-deb)
            SKIP_DEB=true
            shift
            ;;
        --skip-rpm)
            SKIP_RPM=true
            shift
            ;;
        --linux)
            LINUX='both'
            shift
            ;;
        --linux=*)
            LINUX="${1#*=}"
            shift
            ;;
        --linux:amd|--linux:amd64)
            LINUX='amd64'
            shift
            ;;
        --linux:arm|--linux:arm64)
            LINUX='arm64'
            shift
            ;;
        --mac)
            MAC=true
            MAC_ARCH='both'
            shift
            ;;
        --mac-intel|--mac-x64|--mac-amd64)
            MAC=true
            MAC_ARCH='intel'
            shift
            ;;
        --mac-silicon|--mac-arm|--mac-arm64)
            MAC=true
            MAC_ARCH='silicon'
            shift
            ;;
        --win64|--windows)
            WIN64=true
            shift
            ;;
        --all)
            ALL=true
            shift
            ;;
        --mac-ci-no-wait)
            MAC_CI_WAIT=false
            shift
            ;;
        -h|--help)
            echo "Usage: $0 [options]"
            echo "Options:"
            echo "  --linux              Build for Linux (both architectures)"
            echo "  --linux=amd64        Build for Linux amd64 only"
            echo "  --linux=arm64        Build for Linux arm64 only"
            echo "  --mac                Build for macOS (Intel + Apple Silicon)"
            echo "  --mac-intel          Build for macOS x86_64 only"
            echo "  --mac-silicon        Build for macOS arm64 (Apple Silicon) only"
            echo "  --win64              Build for Windows x64"
            echo "  --all                Build for all platforms"
            echo "  --test-only          Run fmt/clippy/test only, skip packaging"
            echo "  --no-checks          Skip fmt/clippy/test checks"
            echo "  --skip-test          Skip tests"
            echo "  --skip-msi           Skip MSI generation"
            echo "  --skip-appimage      Skip Linux AppImage (.AppImage) generation"
            echo "  --skip-deb           Skip Linux .deb package generation"
            echo "  --skip-rpm           Skip Linux .rpm package generation"
            echo "  --mac-ci-no-wait     Trigger macOS GitHub Actions only (no wait/download)"
            echo "                       Default: wait for CI and download artifacts into dist/"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

# Color codes for output
CYAN='\033[0;36m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

log_info() {
    echo -e "${CYAN}$1${NC}"
}

log_success() {
    echo -e "${GREEN}$1${NC}"
}

log_warning() {
    echo -e "${YELLOW}$1${NC}"
}

log_error() {
    echo -e "${RED}$1${NC}"
}

# Target definitions
declare -A TARGET_CATALOG
declare -A TARGET_TRIPLE
declare -A TARGET_PLATFORM
declare -A TARGET_ARCH
declare -A TARGET_BINARY_NAME
declare -A TARGET_DELEGATE_BINARY_NAME
declare -A TARGET_ARCHIVE_KIND
declare -A TARGET_SUPPORTS_INSTALLER

TARGET_CATALOG['windows-x64']='windows-x64'
TARGET_TRIPLE['windows-x64']='x86_64-pc-windows-gnu'
TARGET_PLATFORM['windows-x64']='windows'
TARGET_ARCH['windows-x64']='x64'
TARGET_BINARY_NAME['windows-x64']="${BIN_NAME}.exe"
TARGET_DELEGATE_BINARY_NAME['windows-x64']="${DELEGATE_BIN_NAME}.exe"
TARGET_ARCHIVE_KIND['windows-x64']='zip'
TARGET_SUPPORTS_INSTALLER['windows-x64']='true'

TARGET_CATALOG['linux-amd64']='linux-amd64'
TARGET_TRIPLE['linux-amd64']='x86_64-unknown-linux-gnu'
TARGET_PLATFORM['linux-amd64']='linux'
TARGET_ARCH['linux-amd64']='amd64'
TARGET_BINARY_NAME['linux-amd64']="${BIN_NAME}"
TARGET_DELEGATE_BINARY_NAME['linux-amd64']="${DELEGATE_BIN_NAME}"
TARGET_ARCHIVE_KIND['linux-amd64']='tar.gz'
TARGET_SUPPORTS_INSTALLER['linux-amd64']='true'

TARGET_CATALOG['linux-arm64']='linux-arm64'
TARGET_TRIPLE['linux-arm64']='aarch64-unknown-linux-gnu'
TARGET_PLATFORM['linux-arm64']='linux'
TARGET_ARCH['linux-arm64']='arm64'
TARGET_BINARY_NAME['linux-arm64']="${BIN_NAME}"
TARGET_DELEGATE_BINARY_NAME['linux-arm64']="${DELEGATE_BIN_NAME}"
TARGET_ARCHIVE_KIND['linux-arm64']='tar.gz'
TARGET_SUPPORTS_INSTALLER['linux-arm64']='true'

TARGET_CATALOG['mac-amd64']='mac-amd64'
TARGET_TRIPLE['mac-amd64']='x86_64-apple-darwin'
TARGET_PLATFORM['mac-amd64']='macos'
TARGET_ARCH['mac-amd64']='amd64'
TARGET_BINARY_NAME['mac-amd64']="${BIN_NAME}"
TARGET_DELEGATE_BINARY_NAME['mac-amd64']="${DELEGATE_BIN_NAME}"
TARGET_ARCHIVE_KIND['mac-amd64']='tar.gz'
TARGET_SUPPORTS_INSTALLER['mac-amd64']='true'

TARGET_CATALOG['mac-arm64']='mac-arm64'
TARGET_TRIPLE['mac-arm64']='aarch64-apple-darwin'
TARGET_PLATFORM['mac-arm64']='macos'
TARGET_ARCH['mac-arm64']='arm64'
TARGET_BINARY_NAME['mac-arm64']="${BIN_NAME}"
TARGET_DELEGATE_BINARY_NAME['mac-arm64']="${DELEGATE_BIN_NAME}"
TARGET_ARCHIVE_KIND['mac-arm64']='tar.gz'
TARGET_SUPPORTS_INSTALLER['mac-arm64']='true'

get_project_version() {
    local manifest_path="${PROJECT_ROOT}/Cargo.toml"
    if [[ ! -f "$manifest_path" ]]; then
        log_error "Cargo.toml not found at $manifest_path"
        exit 1
    fi
    local version
    version=$(grep -E '^version\s*=\s*"[^"]+"' "$manifest_path" | head -1 | sed -E 's/.*"([^"]+)".*/\1/')
    if [[ -z "$version" ]]; then
        log_error "Unable to parse package version from $manifest_path"
        exit 1
    fi
    echo "$version"
}

get_linux_selection() {
    case "$LINUX" in
        '')
            echo ''
            ;;
        both|true)
            echo 'linux-amd64 linux-arm64'
            ;;
        amd|amd64)
            echo 'linux-amd64'
            ;;
        arm|arm64)
            echo 'linux-arm64'
            ;;
        *)
            log_error "Unsupported --linux value '$LINUX'. Use --linux, --linux=amd64, or --linux=arm64."
            exit 1
            ;;
    esac
}

get_selected_targets() {
    local selection=()

    if [[ "$ALL" == true ]]; then
        for target in "${TARGET_CATALOG[@]}"; do
            selection+=("$target")
        done
        echo "${selection[@]}"
        return
    fi

    local linux_targets
    linux_targets=$(get_linux_selection)
    for target in $linux_targets; do
        selection+=("$target")
    done

    if [[ "$MAC" == true ]]; then
        case "$MAC_ARCH" in
            intel)
                selection+=('mac-amd64')
                ;;
            silicon)
                selection+=('mac-arm64')
                ;;
            *)
                selection+=('mac-amd64' 'mac-arm64')
                ;;
        esac
    fi

    if [[ "$WIN64" == true ]]; then
        selection+=('windows-x64')
    fi

    if [[ ${#selection[@]} -eq 0 ]]; then
        # Default to windows-x64 on Windows, linux-amd64 on Linux
        if [[ "$OSTYPE" == "linux-gnu"* ]] || [[ "$OSTYPE" == "linux"* ]]; then
            selection+=('linux-amd64')
        elif [[ "$OSTYPE" == "darwin"* ]]; then
            selection+=('mac-amd64')
        fi
    fi

    echo "${selection[@]}"
}

ensure_rust_target() {
    local triple="$1"
    log_info "Ensuring Rust target $triple is installed..."
    rustup target add "$triple"
}

get_host_triple() {
    local rustc_info
    rustc_info=$(rustc -Vv 2>/dev/null || true)
    if [[ -z "$rustc_info" ]]; then
        echo ''
        return
    fi
    echo "$rustc_info" | grep -E '^host:' | sed -E 's/^host:\s*(.+)$/\1/'
}

get_cross_compiler_candidates() {
    local target_triple="$1"
    local cross_compile="$2"

    case "$target_triple" in
        'x86_64-unknown-linux-musl')
            if [[ "$cross_compile" == true ]]; then
                echo 'x86_64-linux-musl-gcc x86_64-linux-gnu-gcc'
            else
                echo 'gcc cc'
            fi
            ;;
        'aarch64-apple-darwin')
            if [[ "$cross_compile" == true ]]; then
                echo 'aarch64-apple-darwin-clang clang cc'
            else
                echo 'clang cc'
            fi
            ;;
        'x86_64-apple-darwin')
            if [[ "$cross_compile" == true ]]; then
                echo 'x86_64-apple-darwin-clang clang cc'
            else
                echo 'clang cc'
            fi
            ;;
        'x86_64-pc-windows-gnu')
            if [[ "$cross_compile" == true ]]; then
                echo 'x86_64-w64-mingw32-gcc x86_64-w64-mingw32-cc mingw-w64-gcc mingw64-gcc'
            else
                echo 'gcc cc'
            fi
            ;;
        'aarch64-unknown-linux-gnu')
            if [[ "$cross_compile" == true ]]; then
                echo 'cross aarch64-linux-gnu-gcc aarch64-linux-gnu-cc'
            else
                echo 'gcc cc'
            fi
            ;;
        *)
            if [[ "$cross_compile" == true ]]; then
                echo 'cc'
            else
                echo 'cc'
            fi
            ;;
    esac
}

get_binary_path() {
    local target_id="$1"
    echo "${PROJECT_ROOT}/target/${TARGET_TRIPLE[$target_id]}/release/${TARGET_BINARY_NAME[$target_id]}"
}

get_delegate_binary_path() {
    local target_id="$1"
    local delegate_path="${PROJECT_ROOT}/target/${TARGET_TRIPLE[$target_id]}/release/${TARGET_DELEGATE_BINARY_NAME[$target_id]}"
    if [[ -f "$delegate_path" ]]; then
        echo "$delegate_path"
        return
    fi

    local fallback_name
    if [[ "${TARGET_PLATFORM[$target_id]}" == "windows" ]]; then
        fallback_name='cg-bin.exe'
    else
        fallback_name='cg-bin'
    fi
    local fallback_path="${PROJECT_ROOT}/target/${TARGET_TRIPLE[$target_id]}/release/${fallback_name}"
    if [[ -f "$fallback_path" ]]; then
        echo "$fallback_path"
        return
    fi

    echo "$delegate_path"
}

get_runtime_binary_name() {
    local target_id="$1"
    if [[ "${TARGET_PLATFORM[$target_id]}" == "windows" ]]; then
        echo 'ComfyGit.exe'
    else
        echo 'ComfyGit'
    fi
}

get_shell_asset_directory() {
    echo "${PROJECT_ROOT}/assets/shell"
}

get_install_helper_paths() {
    echo "${PROJECT_ROOT}/scripts/install-shell-integration.ps1"
    echo "${PROJECT_ROOT}/scripts/install-shell-integration.sh"
    echo "${PROJECT_ROOT}/scripts/uninstall-shell-integration.sh"
}

test_unix_text_asset() {
    local path="$1"
    local filename
    filename=$(basename "$path")
    [[ "$filename" == "$BIN_NAME" ]] || [[ "$filename" == *.sh ]]
}

copy_packaged_asset() {
    local source_path="$1"
    local destination_path="$2"

    if test_unix_text_asset "$source_path"; then
        # Normalize line endings to LF
        tr -d '\r' < "$source_path" > "$destination_path"
    else
        cp "$source_path" "$destination_path"
    fi
}

copy_packaged_directory() {
    local source_dir="$1"
    local destination_dir="$2"

    if [[ ! -d "$source_dir" ]]; then
        return
    fi

    source_dir="${source_dir%/}"

    find "$source_dir" -type f | while read -r file; do
        local relative_path="${file#"${source_dir}/"}"
        local destination_path="${destination_dir}/${relative_path}"
        local destination_parent
        destination_parent=$(dirname "$destination_path")
        mkdir -p "$destination_parent"
        copy_packaged_asset "$file" "$destination_path"
    done
}

set_unix_permissions_if_available() {
    local path="$1"
    local mode="$2"

    if command -v chmod &>/dev/null; then
        chmod "$mode" "$path" || true
    fi
}

xml_escape() {
    local str="$1"
    str="${str//&/&amp;}"
    str="${str//</&lt;}"
    str="${str//>/&gt;}"
    str="${str//\"/&quot;}"
    echo "$str"
}

get_public_launcher_paths() {
    local target_id="$1"
    local shell_asset_dir
    shell_asset_dir=$(get_shell_asset_directory)

    case "${TARGET_PLATFORM[$target_id]}" in
        'windows')
            echo "${shell_asset_dir}/cg.ps1"
            echo "${shell_asset_dir}/cg.cmd"
            echo "${shell_asset_dir}/ComfyGit.psm1"
            ;;
        *)
            echo "${shell_asset_dir}/cg"
            echo ''
            echo ''
            ;;
    esac
}

copy_public_launchers() {
    local target_id="$1"
    local destination_dir="$2"

    local launcher_paths
    launcher_paths=$(get_public_launcher_paths "$target_id")
    local primary_launcher
    primary_launcher=$(echo "$launcher_paths" | head -1)
    local secondary_launcher
    secondary_launcher=$(echo "$launcher_paths" | sed -n '2p')
    local module_launcher
    module_launcher=$(echo "$launcher_paths" | sed -n '3p')

    if [[ ! -f "$primary_launcher" ]]; then
        log_error "Public launcher asset not found at $primary_launcher"
        exit 1
    fi

    case "${TARGET_PLATFORM[$target_id]}" in
        'windows')
            cp "$primary_launcher" "${destination_dir}/cg.ps1"
            if [[ -f "$secondary_launcher" ]]; then
                cp "$secondary_launcher" "${destination_dir}/cg.cmd"
            fi
            if [[ -f "$module_launcher" ]]; then
                cp "$module_launcher" "${destination_dir}/ComfyGit.psm1"
            fi
            ;;
        *)
            local target_path="${destination_dir}/${BIN_NAME}"
            copy_packaged_asset "$primary_launcher" "$target_path"
            set_unix_permissions_if_available "$target_path" '+x'

            local alias_path="${destination_dir}/comfygit"
            copy_packaged_asset "$primary_launcher" "$alias_path"
            set_unix_permissions_if_available "$alias_path" '+x'
            ;;
    esac
}

get_unix_post_install_script() {
    cat <<'EOF'
#!/usr/bin/env sh
set -eu

if [ -x /usr/local/share/comfygit/scripts/install-shell-integration.sh ]; then
  /usr/local/share/comfygit/scripts/install-shell-integration.sh /usr/local/share/comfygit/shell /usr/local/bin || true
fi

exit 0
EOF
}

copy_shell_integration_assets() {
    local destination_root="$1"
    local shell_asset_dir
    shell_asset_dir=$(get_shell_asset_directory)
    local shell_destination="${destination_root}/shell"
    local script_destination="${destination_root}/scripts"

    if [[ -d "$shell_asset_dir" ]]; then
        mkdir -p "$shell_destination"
        copy_packaged_directory "$shell_asset_dir" "$shell_destination"
    fi

    local helper_paths
    helper_paths=$(get_install_helper_paths)
    for helper_path in $helper_paths; do
        if [[ -f "$helper_path" ]]; then
            mkdir -p "$script_destination"
            local destination_path="${script_destination}/$(basename "$helper_path")"
            copy_packaged_asset "$helper_path" "$destination_path"
            if test_unix_text_asset "$helper_path"; then
                set_unix_permissions_if_available "$destination_path" '+x'
            fi
        fi
    done

    # Set executable permissions on all Unix text assets in shell directory
    if [[ -d "$shell_destination" ]]; then
        find "$shell_destination" -type f | while read -r file; do
            if test_unix_text_asset "$file"; then
                set_unix_permissions_if_available "$file" '+x'
            fi
        done
    fi
}

get_target_subfolder() {
    local target_id="$1"
    case "$target_id" in
        'windows-x64') echo 'windows-x64' ;;
        'linux-amd64') echo 'linux-amd64' ;;
        'linux-arm64') echo 'linux-arm64' ;;
        'mac-amd64') echo 'macos-x86_64' ;;
        'mac-arm64') echo 'macos-aarch64' ;;
        *)
            log_error "Unsupported target id '$target_id'"
            exit 1
            ;;
    esac
}

get_target_latest_dir() {
    local target_id="$1"
    local subfolder
    subfolder=$(get_target_subfolder "$target_id")
    echo "${DIST_ROOT}/latest/${subfolder}"
}

get_target_old_dir() {
    local target_id="$1"
    local subfolder
    subfolder=$(get_target_subfolder "$target_id")
    echo "${DIST_ROOT}/old/${subfolder}"
}

# Extract x.y.z from "${PACKAGE}-<ver>-..." basename.
extract_pkg_version_from_basename() {
    local base=$1
    if [[ "$base" != "${PACKAGE}-"* ]]; then
        return 1
    fi
    local rest="${base#${PACKAGE}-}"
    local ver
    ver=$(echo "$rest" | grep -oE '^[0-9]+\.[0-9]+\.[0-9]+' || true)
    if [[ -z "$ver" ]]; then
        return 1
    fi
    echo "$ver"
}

semver_lt() {
    [[ "$1" != "$2" ]] && [[ "$(printf '%s\n' "$1" "$2" | sort -V | head -n1)" == "$1" ]]
}

# Move only artifacts of the same kind that this run replaces (older semver -> dist/old/.../<ver>/).
# Same-version rebuild: remove the exact file so the packager can rewrite it.
archive_superseded_for_suffix() {
    local latest_dir=$1
    local target_id=$2
    local new_version=$3
    local suffix_fragment=$4

    if [[ ! -d "$latest_dir" ]]; then
        return 0
    fi

    local old_root
    old_root=$(get_target_old_dir "$target_id")
    local f base ver

    shopt -s nullglob
    for f in "${latest_dir}/${PACKAGE}-"*; do
        [[ -f "$f" ]] || continue
        base=$(basename "$f")
        [[ "$base" == *"$suffix_fragment"* ]] || continue
        if ! ver=$(extract_pkg_version_from_basename "$base"); then
            continue
        fi
        if semver_lt "$ver" "$new_version"; then
            mkdir -p "${old_root}/${ver}"
            log_info "Archiving superseded artifact ${base} -> ${old_root}/${ver}/"
            mv "$f" "${old_root}/${ver}/" || true
        elif [[ "$ver" == "$new_version" ]]; then
            rm -f "$f" || true
        fi
    done
    shopt -u nullglob
}

build_release_binary() {
    local target_id="$1"

    ensure_rust_target "${TARGET_TRIPLE[$target_id]}"

    local host_triple
    host_triple=$(get_host_triple)
    local cross_compile=false
    if [[ -n "$host_triple" ]] && [[ "$host_triple" != "${TARGET_TRIPLE[$target_id]}" ]]; then
        cross_compile=true
    fi

    local use_cross=false
    if [[ "$cross_compile" == true ]]; then
        local candidates
        candidates=$(get_cross_compiler_candidates "${TARGET_TRIPLE[$target_id]}" true)
        local found_compiler=''
        for candidate in $candidates; do
            if command -v "$candidate" &>/dev/null; then
                found_compiler="$candidate"
                break
            fi
        done

        if [[ -z "$found_compiler" ]]; then
            log_error "Cross-compiling to ${TARGET_TRIPLE[$target_id]} from host $host_triple requires a compatible cross compiler. Install one of $candidates."
            exit 1
        fi

        if [[ "$found_compiler" == "cross" ]]; then
            use_cross=true
            local triple="${TARGET_TRIPLE[$target_id]}"
            log_info "Using 'cross' (container-based) for target ${triple}..."
            # Host checks (cargo test/clippy) share target/ with cross builds. Clear
            # target-triple artifacts and host build-script outputs so proc-macro and
            # glibc mismatches (e.g. crossterm/document-features) cannot bleed across.
            log_info "Cleaning cross-build artifacts for ${triple}..."
            cargo clean --target "$triple" -q 2>/dev/null || true
            rm -rf "${PROJECT_ROOT}/target/release/build" 2>/dev/null || true
        else
            log_info "Using cross-compiler $found_compiler for target ${TARGET_TRIPLE[$target_id]}..."

            local host_compiler=''
            if command -v gcc &>/dev/null; then
                host_compiler='gcc'
            elif command -v cc &>/dev/null; then
                host_compiler='cc'
            else
                log_error "No host C compiler was found for build script compilation."
                exit 1
            fi

            export CC="$host_compiler"
            export CXX="$host_compiler"
            export HOST_CC="$host_compiler"
            export HOST_CXX="$host_compiler"
            export TARGET_CC="$found_compiler"
            export TARGET_CXX="$found_compiler"

            local target_env
            target_env=$(echo "${TARGET_TRIPLE[$target_id]}" | tr '-' '_' | tr '[:lower:]' '[:upper:]')
            local target_env_lower
            target_env_lower=$(echo "${TARGET_TRIPLE[$target_id]}" | tr '-' '_')

            export "CARGO_TARGET_${target_env}_LINKER"="$found_compiler"
            export "CC_${target_env_lower}"="$found_compiler"
            export "CXX_${target_env_lower}"="$found_compiler"
            export RUSTFLAGS="-C linker=$found_compiler"
        fi
    fi

    log_info "Building $target_id..."
    if [[ "$use_cross" == true ]]; then
        CROSS_CONTAINER_ENGINE=podman cross build --release --target "${TARGET_TRIPLE[$target_id]}"
    else
        cargo build --release --target "${TARGET_TRIPLE[$target_id]}"
    fi

    local binary_path
    binary_path=$(get_binary_path "$target_id")
    if [[ ! -f "$binary_path" ]]; then
        log_error "Release binary not found at $binary_path"
        exit 1
    fi

    echo "$binary_path"
}

new_portable_package() {
    local target_id="$1"
    local version="$2"
    local binary_path="$3"
    local delegate_binary_path="$4"

    local package_dir="${DIST_ROOT}/tmp-${PACKAGE}-${version}-${TARGET_PLATFORM[$target_id]}-${TARGET_ARCH[$target_id]}"
    if [[ -d "$package_dir" ]]; then
        rm -rf "$package_dir"
    fi
    mkdir -p "$package_dir"

    local root_dir_name="${PACKAGE}-${version}"
    local package_root="${package_dir}/${root_dir_name}"
    mkdir -p "$package_root"

    local runtime_name
    runtime_name=$(get_runtime_binary_name "$target_id")
    cp "$delegate_binary_path" "${package_root}/${runtime_name}"
    cp "$delegate_binary_path" "${package_root}/${TARGET_DELEGATE_BINARY_NAME[$target_id]}"
    if [[ "${TARGET_PLATFORM[$target_id]}" == "macos" ]]; then
        cp "$delegate_binary_path" "${package_root}/${TARGET_BINARY_NAME[$target_id]}"
        set_unix_permissions_if_available "${package_root}/${TARGET_BINARY_NAME[$target_id]}" '+x'
    else
        copy_public_launchers "$target_id" "$package_root"
    fi
    cp "${PROJECT_ROOT}/README.md" "$package_root"
    cp "${PROJECT_ROOT}/LICENSE.md" "$package_root"
    copy_shell_integration_assets "$package_root"

    local target_latest
    target_latest=$(get_target_latest_dir "$target_id")
    mkdir -p "$target_latest"

    local portable_suffix="-${TARGET_PLATFORM[$target_id]}-${TARGET_ARCH[$target_id]}-portable."
    archive_superseded_for_suffix "$target_latest" "$target_id" "$version" "$portable_suffix"

    local archive_path
    if [[ "${TARGET_ARCHIVE_KIND[$target_id]}" == "zip" ]]; then
        archive_path="${target_latest}/${PACKAGE}-${version}-${TARGET_PLATFORM[$target_id]}-${TARGET_ARCH[$target_id]}-portable.zip"
        if [[ -f "$archive_path" ]]; then
            rm -f "$archive_path"
        fi
        (cd "$package_dir" && zip -r "$archive_path" "$root_dir_name")
    else
        archive_path="${target_latest}/${PACKAGE}-${version}-${TARGET_PLATFORM[$target_id]}-${TARGET_ARCH[$target_id]}-portable.tar.gz"
        if [[ -f "$archive_path" ]]; then
            rm -f "$archive_path"
        fi
        tar -czf "$archive_path" -C "$package_dir" "$root_dir_name"
    fi

    rm -rf "$package_dir"

    log_success "Created portable package: $archive_path"
    echo "$archive_path"
}

should_package_installer_for_target() {
    local target_id="$1"
    [[ "${TARGET_SUPPORTS_INSTALLER[$target_id]:-}" == "true" ]] || return 1
    case "${TARGET_PLATFORM[$target_id]}" in
        windows) [[ "$SKIP_MSI" == false ]] ;;
        macos|linux) return 0 ;;
        *) return 1 ;;
    esac
}

prepare_mac_icns() {
    local png_path="$1"
    local icns_out="$2"

    if [[ ! -f "$png_path" ]] || ! command -v iconutil &>/dev/null || ! command -v sips &>/dev/null; then
        return 1
    fi

    local iconset_dir
    iconset_dir=$(mktemp -d)
    sips -z 16 16 "$png_path" --out "${iconset_dir}/icon_16x16.png" >/dev/null 2>&1 || true
    sips -z 32 32 "$png_path" --out "${iconset_dir}/icon_16x16@2x.png" >/dev/null 2>&1 || true
    sips -z 32 32 "$png_path" --out "${iconset_dir}/icon_32x32.png" >/dev/null 2>&1 || true
    sips -z 64 64 "$png_path" --out "${iconset_dir}/icon_32x32@2x.png" >/dev/null 2>&1 || true
    sips -z 128 128 "$png_path" --out "${iconset_dir}/icon_128x128.png" >/dev/null 2>&1 || true
    sips -z 256 256 "$png_path" --out "${iconset_dir}/icon_128x128@2x.png" >/dev/null 2>&1 || true
    sips -z 256 256 "$png_path" --out "${iconset_dir}/icon_256x256.png" >/dev/null 2>&1 || true
    sips -z 512 512 "$png_path" --out "${iconset_dir}/icon_256x256@2x.png" >/dev/null 2>&1 || true
    sips -z 512 512 "$png_path" --out "${iconset_dir}/icon_512x512.png" >/dev/null 2>&1 || true
    sips -z 1024 1024 "$png_path" --out "${iconset_dir}/icon_512x512@2x.png" >/dev/null 2>&1 || true
    if iconutil -c icns "$iconset_dir" -o "$icns_out" 2>/dev/null; then
        rm -rf "$iconset_dir"
        return 0
    fi
    rm -rf "$iconset_dir"
    return 1
}

build_mac_app_bundle() {
    local target_id="$1"
    local version="$2"
    local delegate_binary_path="$3"
    local staging_root="$4"
    local target_latest="$5"

    local app_bundle_name="${NAME}.app"
    local app_root="${staging_root}/${app_bundle_name}"
    local contents_dir="${app_root}/Contents"
    local macos_dir="${contents_dir}/MacOS"
    local resources_dir="${contents_dir}/Resources"
    local mac_icon_path="${PROJECT_ROOT}/assets/logos-3rd-party/portable.png"

    if [[ -d "$app_root" ]]; then
        rm -rf "$app_root"
    fi
    mkdir -p "$macos_dir" "$resources_dir"

    cp "$delegate_binary_path" "${macos_dir}/${BIN_NAME}"
    set_unix_permissions_if_available "${macos_dir}/${BIN_NAME}" '+x'

    local icon_plist=''
    if prepare_mac_icns "$mac_icon_path" "${resources_dir}/AppIcon.icns"; then
        icon_plist='  <key>CFBundleIconFile</key>
  <string>AppIcon</string>'
    fi

    cat > "${contents_dir}/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key>
  <string>${BIN_NAME}</string>
  <key>CFBundleIdentifier</key>
  <string>${PACKAGE_ID}</string>
  <key>CFBundleName</key>
  <string>${NAME}</string>
  <key>CFBundleDisplayName</key>
  <string>${NAME}</string>
  <key>CFBundleVersion</key>
  <string>${version}</string>
  <key>CFBundleShortVersionString</key>
  <string>${version}</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
${icon_plist}
  <key>LSMinimumSystemVersion</key>
  <string>10.13</string>
  <key>NSHighResolutionCapable</key>
  <true/>
</dict>
</plist>
EOF

    archive_superseded_for_suffix "$target_latest" "$target_id" "$version" "-${TARGET_PLATFORM[$target_id]}-${TARGET_ARCH[$target_id]}.app.zip"
    local app_zip_path="${target_latest}/${PACKAGE}-${version}-${TARGET_PLATFORM[$target_id]}-${TARGET_ARCH[$target_id]}.app.zip"
    if [[ -f "$app_zip_path" ]]; then
        rm -f "$app_zip_path"
    fi

    if command -v ditto &>/dev/null; then
        ditto -c -k --sequesterRsrc --keepParent "$app_root" "$app_zip_path"
        log_success "Created Mac application bundle: $app_zip_path" >&2
    else
        log_warning "ditto not found; skipping .app.zip for $target_id" >&2
    fi

    printf '%s\n' "$app_root"
}

new_mac_installer_artifacts() {
    local target_id="$1"
    local version="$2"
    local binary_path="$3"
    local delegate_binary_path="$4"

    if [[ "$OSTYPE" != "darwin"* ]]; then
        log_warning "Mac installer packaging requires running on macOS. Skipping PKG/DMG generation."
        return
    fi

    local staging_root="${DIST_ROOT}/macpkg-${target_id}-${version}"
    local payload_root="${staging_root}/root"
    local bin_dir="${payload_root}/usr/local/bin"
    local share_dir="${payload_root}/usr/local/share/comfygit"

    if [[ -d "$staging_root" ]]; then
        rm -rf "$staging_root"
    fi

    mkdir -p "$bin_dir"
    local runtime_name
    runtime_name=$(get_runtime_binary_name "$target_id")
    copy_shell_integration_assets "$share_dir"
    cp "$delegate_binary_path" "${bin_dir}/${runtime_name}"
    cp "$delegate_binary_path" "${bin_dir}/${TARGET_BINARY_NAME[$target_id]}"
    if [[ "${TARGET_DELEGATE_BINARY_NAME[$target_id]}" != "$runtime_name" ]] &&
        [[ "${TARGET_DELEGATE_BINARY_NAME[$target_id]}" != "${TARGET_BINARY_NAME[$target_id]}" ]]; then
        cp "$delegate_binary_path" "${bin_dir}/${TARGET_DELEGATE_BINARY_NAME[$target_id]}"
    fi
    set_unix_permissions_if_available "${bin_dir}/${runtime_name}" '+x'
    set_unix_permissions_if_available "${bin_dir}/${TARGET_BINARY_NAME[$target_id]}" '+x'
    if [[ -f "${bin_dir}/${TARGET_DELEGATE_BINARY_NAME[$target_id]}" ]]; then
        set_unix_permissions_if_available "${bin_dir}/${TARGET_DELEGATE_BINARY_NAME[$target_id]}" '+x'
    fi

    local pkg_scripts_dir="${staging_root}/pkgscripts"
    mkdir -p "$pkg_scripts_dir"
    local post_install_path="${pkg_scripts_dir}/postinstall"
    get_unix_post_install_script > "$post_install_path"
    tr -d '\r' < "$post_install_path" > "${post_install_path}.tmp" && mv "${post_install_path}.tmp" "$post_install_path"
    set_unix_permissions_if_available "$post_install_path" '+x'

    local target_latest
    target_latest=$(get_target_latest_dir "$target_id")
    mkdir -p "$target_latest"

    archive_superseded_for_suffix "$target_latest" "$target_id" "$version" "-${TARGET_PLATFORM[$target_id]}-${TARGET_ARCH[$target_id]}.pkg"
    archive_superseded_for_suffix "$target_latest" "$target_id" "$version" "-${TARGET_PLATFORM[$target_id]}-${TARGET_ARCH[$target_id]}.dmg"

    local app_root
    app_root=$(build_mac_app_bundle "$target_id" "$version" "$delegate_binary_path" "$staging_root" "$target_latest")

    local pkg_path="${target_latest}/${PACKAGE}-${version}-${TARGET_PLATFORM[$target_id]}-${TARGET_ARCH[$target_id]}.pkg"

    if ! command -v pkgbuild &>/dev/null; then
        log_error "pkgbuild not found on this macOS host; cannot generate .pkg installer."
        rm -rf "$staging_root"
        exit 1
    fi

    if ! pkgbuild --root "$payload_root" --scripts "$pkg_scripts_dir" --install-location / --identifier "$PACKAGE_ID" --version "$version" "$pkg_path"; then
        log_error "pkgbuild failed for $target_id"
        rm -rf "$staging_root"
        exit 1
    fi

    local pkg_bytes=0
    if [[ -f "$pkg_path" ]]; then
        pkg_bytes=$(wc -c <"$pkg_path" | tr -d ' ')
    fi
    if [[ "$pkg_bytes" -lt 500000 ]]; then
        log_error "PKG is too small (${pkg_bytes} bytes); expected compiled binaries under ${bin_dir}"
        rm -rf "$staging_root"
        exit 1
    fi

    local dmg_source="${staging_root}/dmg"
    if [[ -d "$dmg_source" ]]; then
        rm -rf "$dmg_source"
    fi
    mkdir -p "$dmg_source"
    cp "$pkg_path" "$dmg_source"
    if [[ -d "$app_root" ]]; then
        cp -R "$app_root" "$dmg_source/"
    fi
    cp "${PROJECT_ROOT}/README.md" "$dmg_source"

    local dmg_path="${target_latest}/${PACKAGE}-${version}-${TARGET_PLATFORM[$target_id]}-${TARGET_ARCH[$target_id]}.dmg"

    if ! command -v hdiutil &>/dev/null; then
        log_error "hdiutil not found on this macOS host; cannot generate .dmg image."
        rm -rf "$staging_root"
        exit 1
    fi

    hdiutil create -volname "${NAME} ${version}" -srcfolder "$dmg_source" -ov -format UDZO "$dmg_path"

    log_success "Created Mac installer package: $pkg_path"
    log_success "Created Mac disk image: $dmg_path"

    rm -rf "$staging_root"
}


new_windows_installer_artifacts() {
    local target_id="$1"
    local version="$2"
    local binary_path="$3"
    local delegate_binary_path="$4"

    if ! command -v wixl &>/dev/null; then
        log_warning "wixl (msitools) was not found. MSI packaging was skipped."
        log_warning "Install msitools and rerun this script to produce an MSI."
        return
    fi

    local target_latest
    target_latest=$(get_target_latest_dir "$target_id")
    mkdir -p "$target_latest"

    archive_superseded_for_suffix "$target_latest" "$target_id" "$version" "-${TARGET_PLATFORM[$target_id]}-${TARGET_ARCH[$target_id]}.msi"

    local staging_root="${DIST_ROOT}/winpkg-${target_id}-${version}"
    if [[ -d "$staging_root" ]]; then
        rm -rf "$staging_root"
    fi
    mkdir -p "$staging_root"

    local msi_path="${target_latest}/${PACKAGE}-${version}-${TARGET_PLATFORM[$target_id]}-${TARGET_ARCH[$target_id]}.msi"
    local wxs_path="${staging_root}/${BIN_NAME}-installer.wxs"

    local shell_asset_dir
    shell_asset_dir=$(get_shell_asset_directory)
    local cg_ps1_path="${shell_asset_dir}/cg.ps1"
    local cg_cmd_path="${shell_asset_dir}/cg.cmd"
    local readme_path="${PROJECT_ROOT}/README.md"
    local license_path="${PROJECT_ROOT}/LICENSE.md"
    local install_ps1_path="${PROJECT_ROOT}/scripts/install-shell-integration.ps1"
    local shell_module_path="${shell_asset_dir}/ComfyGit.psm1"

    for f in "$cg_ps1_path" "$delegate_binary_path" "$readme_path" "$license_path" "$cg_cmd_path" "$install_ps1_path" "$shell_module_path"; do
        if [[ ! -f "$f" ]]; then
            log_warning "Required file for MSI not found: $f. Skipping MSI generation."
            rm -rf "$staging_root"
            return
        fi
    done

    local cg_ps1_xml comfygit_exe_xml readme_xml license_xml cg_cmd_xml install_ps1_xml shell_module_xml
    cg_ps1_xml=$(xml_escape "$cg_ps1_path")
    comfygit_exe_xml=$(xml_escape "$delegate_binary_path")
    readme_xml=$(xml_escape "$readme_path")
    license_xml=$(xml_escape "$license_path")
    cg_cmd_xml=$(xml_escape "$cg_cmd_path")
    install_ps1_xml=$(xml_escape "$install_ps1_path")
    shell_module_xml=$(xml_escape "$shell_module_path")

    cat > "$wxs_path" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<Wix xmlns="http://schemas.microsoft.com/wix/2006/wi">
  <Product Name="$NAME" Version="$version" Manufacturer="$MANUFACTURER" Id="*" UpgradeCode="$UPGRADE_CODE_GUID" Language="1033" Codepage="1252">
    <Package InstallerVersion="500" Compressed="yes" InstallScope="perMachine" />
    <MajorUpgrade DowngradeErrorMessage="A newer version of $NAME is already installed." AllowSameVersionUpgrades="yes" />
    <Media Id="1" Cabinet="media1.cab" EmbedCab="yes" />
    <Directory Id="TARGETDIR" Name="SourceDir">
      <Directory Id="ProgramFiles64Folder">
        <Directory Id="INSTALLFOLDER" Name="$NAME">
          <Component Id="cgExecutableComponent" Guid="$EXE_GUID">
            <File Id="cgExecutableFile" Name="cg.ps1" Source="$cg_ps1_xml" KeyPath="yes" />
            <Environment Id="AddcgToPath" Action="set" Name="PATH" Part="last" Permanent="no" System="yes" Value="[INSTALLFOLDER]" />
          </Component>
          <Component Id="ComfyGitExecutableComponent" Guid="$DELEGATE_EXE_GUID">
            <File Id="ComfyGitExecutableFile" Name="ComfyGit.exe" Source="$comfygit_exe_xml" KeyPath="yes" />
          </Component>
          <Component Id="ReadmeComponent" Guid="$README_GUID">
            <File Id="ReadmeFile" Name="README.md" Source="$readme_xml" KeyPath="yes" />
          </Component>
          <Component Id="LicenseComponent" Guid="$LICENSE_GUID">
            <File Id="LicenseFile" Name="LICENSE.md" Source="$license_xml" KeyPath="yes" />
          </Component>
          <Component Id="ShellCmdComponent" Guid="$SHELL_CMD_GUID">
            <File Id="ShellCmdFile" Name="cg.cmd" Source="$cg_cmd_xml" KeyPath="yes" />
          </Component>
          <Directory Id="ScriptsDirectory" Name="scripts">
            <Component Id="InstallPs1Component" Guid="$INSTALL_PS1_GUID">
              <File Id="InstallPs1File" Name="install-shell-integration.ps1" Source="$install_ps1_xml" KeyPath="yes" />
            </Component>
          </Directory>
        </Directory>
        <Directory Id="PowerShellFolder" Name="PowerShell">
          <Directory Id="PSModulesFolder" Name="Modules">
            <Directory Id="ComfyGitModuleFolder" Name="ComfyGit">
              <Component Id="ShellModuleComponent" Guid="$SHELL_MODULE_GUID">
                <File Id="ShellModuleFile" Name="ComfyGit.psm1" Source="$shell_module_xml" KeyPath="yes" />
              </Component>
            </Directory>
          </Directory>
        </Directory>
      </Directory>
    </Directory>
    <Feature Id="MainFeature" Title="$NAME" Level="1">
      <ComponentRef Id="cgExecutableComponent" />
      <ComponentRef Id="ComfyGitExecutableComponent" />
      <ComponentRef Id="ReadmeComponent" />
      <ComponentRef Id="LicenseComponent" />
      <ComponentRef Id="ShellCmdComponent" />
      <ComponentRef Id="ShellModuleComponent" />
      <ComponentRef Id="InstallPs1Component" />
    </Feature>
  </Product>
</Wix>
EOF

    wixl "$wxs_path" -o "$msi_path"

    rm -rf "$staging_root"

    log_success "Created Windows MSI package: $msi_path"
}

new_linux_installer_artifacts() {
    local target_id="$1"
    local version="$2"
    local binary_path="$3"
    local delegate_binary_path="$4"

    if [[ "$OSTYPE" != "linux-gnu"* ]] && [[ "$OSTYPE" != "linux"* ]]; then
        log_warning "Linux installer packaging requires running on Linux. Skipping DEB/RPM generation."
        return
    fi

    local staging_root="${DIST_ROOT}/linuxpkg-${target_id}-${version}"
    local payload_root="${staging_root}/root"
    local bin_dir="${payload_root}/usr/local/bin"
    local share_dir="${payload_root}/usr/local/share/comfygit"

    if [[ -d "$staging_root" ]]; then
        rm -rf "$staging_root"
    fi

    mkdir -p "$bin_dir"
    local runtime_name
    runtime_name=$(get_runtime_binary_name "$target_id")
    cp "$delegate_binary_path" "${bin_dir}/${runtime_name}"
    cp "$delegate_binary_path" "${bin_dir}/${TARGET_DELEGATE_BINARY_NAME[$target_id]}"
    set_unix_permissions_if_available "${bin_dir}/${runtime_name}" '+x'
    set_unix_permissions_if_available "${bin_dir}/${TARGET_DELEGATE_BINARY_NAME[$target_id]}" '+x'
    copy_public_launchers "$target_id" "$bin_dir"
    copy_shell_integration_assets "$share_dir"

    if [[ "$SKIP_DEB" == true ]] && [[ "$SKIP_RPM" == true ]]; then
        log_info "Skipping .deb and .rpm (--skip-deb --skip-rpm); no Linux installer packages produced."
        rm -rf "$staging_root"
        return 0
    fi

    local target_latest
    target_latest=$(get_target_latest_dir "$target_id")
    mkdir -p "$target_latest"

    local deb_path=''
    if [[ "$SKIP_DEB" != true ]]; then
        archive_superseded_for_suffix "$target_latest" "$target_id" "$version" "-${TARGET_PLATFORM[$target_id]}-${TARGET_ARCH[$target_id]}.deb"
        local deb_arch
        case "${TARGET_ARCH[$target_id]}" in
            'amd64') deb_arch='amd64' ;;
            'arm64') deb_arch='arm64' ;;
            *) deb_arch="${TARGET_ARCH[$target_id]}" ;;
        esac

        local deb_root="${staging_root}/debroot"
        local deb_control="${deb_root}/DEBIAN"

        mkdir -p "$deb_control"
        cp -r "${payload_root}/"* "$deb_root/" 2>/dev/null || true

        cat > "${deb_control}/control" <<EOF
Package: ${PACKAGE}
Version: ${version}
Section: utils
Priority: optional
Architecture: ${deb_arch}
Maintainer: ${MAINTAINER}
License: ${LICENSE}
Description: ${DESCRIPTION}
EOF

        local deb_postinst_path="${deb_control}/postinst"
        get_unix_post_install_script > "$deb_postinst_path"
        tr -d '\r' < "$deb_postinst_path" > "${deb_postinst_path}.tmp" && mv "${deb_postinst_path}.tmp" "$deb_postinst_path"
        set_unix_permissions_if_available "$deb_postinst_path" '0755'

        deb_path="${target_latest}/${PACKAGE}-${version}-${TARGET_PLATFORM[$target_id]}-${TARGET_ARCH[$target_id]}.deb"

        if ! command -v dpkg-deb &>/dev/null; then
            log_error "dpkg-deb not found on this Linux host; cannot generate .deb installer."
            rm -rf "$staging_root"
            exit 1
        fi

        dpkg-deb --root-owner-group --build "$deb_root" "$deb_path"
    else
        log_info "Skipping .deb (--skip-deb)"
    fi

    local rpm_path=''
    local rpm_latest_path=''
    if [[ "$SKIP_RPM" != true ]]; then
        # RPM packaging
        local rpm_arch
        case "${TARGET_ARCH[$target_id]}" in
            'amd64') rpm_arch='x86_64' ;;
            'arm64') rpm_arch='aarch64' ;;
            *) rpm_arch="${TARGET_ARCH[$target_id]}" ;;
        esac

        archive_superseded_for_suffix "$target_latest" "$target_id" "$version" "-linux-${rpm_arch}.rpm"

        local rpm_root="${staging_root}/rpmbuild"
        local rpm_build_root="${rpm_root}/BUILDROOT"
        local rpm_top_dir="${rpm_root}/rpmbuild"
        local rpm_spec_dir="${rpm_top_dir}/SPECS"
        local rpm_build_dir="${rpm_top_dir}/BUILD"
        local rpm_rpms_dir="${rpm_top_dir}/RPMS"
        local rpm_srpm_dir="${rpm_top_dir}/SRPMS"

        for dir in "$rpm_build_root" "$rpm_spec_dir" "$rpm_build_dir" "$rpm_rpms_dir" "$rpm_srpm_dir"; do
            mkdir -p "$dir"
        done

        local spec_path="${rpm_spec_dir}/${PACKAGE}.spec"
        cat > "$spec_path" <<EOF
Name: ${PACKAGE}
Version: ${version}
Release: 1%{?dist}
Summary: ${DESCRIPTION}
License: ${LICENSE}
Group: Applications/Networking
BuildArch: ${rpm_arch}

%description
${DESCRIPTION}

%prep
:

%build

%install
mkdir -p %{buildroot}
cp -a ${payload_root}/. %{buildroot}/

%post
if [ -x /usr/local/share/comfygit/scripts/install-shell-integration.sh ]; then
  /usr/local/share/comfygit/scripts/install-shell-integration.sh /usr/local/share/comfygit/shell /usr/local/bin || true
fi

%files
/usr/local/bin/${TARGET_DELEGATE_BINARY_NAME[$target_id]}
/usr/local/bin/${BIN_NAME}
/usr/local/bin/${PACKAGE}
/usr/local/share/${PACKAGE}
EOF

        if ! command -v rpmbuild &>/dev/null; then
            log_warning "rpmbuild not found on this Linux host; skipping .rpm installer generation."
            if [[ -n "$deb_path" ]]; then
                log_success "Created Linux installer package: $deb_path"
            fi
            rm -rf "$staging_root"
            return
        fi

        # Skip architecture compatibility check for cross-arch builds —
        # rpmbuild --target works for packaging pre-built binaries even when
        # the host doesn't natively support the target architecture.

        local host_arch
        host_arch=$(uname -m)
        local use_container_rpm=false

        # For cross-arch RPM builds, use a container with the target architecture
        if [[ "$host_arch" != "aarch64" ]] && [[ "$rpm_arch" == "aarch64" ]]; then
            use_container_rpm=true
        elif [[ "$host_arch" != "armv7hl" ]] && [[ "$rpm_arch" == "armv7hl" ]]; then
            use_container_rpm=true
        fi

        if [[ "$use_container_rpm" == true ]]; then
            if ! command -v podman &>/dev/null; then
                log_warning "podman not found; cannot build cross-arch RPM. Skipping RPM packaging."
                if [[ -n "$deb_path" ]]; then
                    log_success "Created Linux installer package: $deb_path"
                fi
                rm -rf "$staging_root"
                return
            fi

            local container_image="registry.fedoraproject.org/fedora:40-${rpm_arch}"
            log_info "Building RPM for $rpm_arch using container $container_image..."

            local oci_platform
            case "$rpm_arch" in
                x86_64) oci_platform='linux/amd64' ;;
                aarch64) oci_platform='linux/arm64' ;;
                armv7hl) oci_platform='linux/arm/v7' ;;
                *) oci_platform='linux/amd64' ;;
            esac

            # Pull the container image if not already present
            podman pull --platform "$oci_platform" "$container_image" >/dev/null 2>&1 || true

            # Copy the staging directory into a temporary location that the container can access
            local container_staging="${staging_root}/container"
            mkdir -p "$container_staging"
            cp -r "$payload_root" "$container_staging/payload"

            # Generate a container-specific spec file with /build paths
            local container_spec_path="${container_staging}/package.spec"
            cat > "$container_spec_path" <<EOF
Name: ${PACKAGE}
Version: ${version}
Release: 1%{?dist}
Summary: ${DESCRIPTION}
License: ${LICENSE}
Group: Applications/Networking
BuildArch: ${rpm_arch}

%description
${DESCRIPTION}

%prep
:

%build

%install
mkdir -p %{buildroot}
cp -a /build/payload/. %{buildroot}/

%post
if [ -x /usr/local/share/comfygit/scripts/install-shell-integration.sh ]; then
  /usr/local/share/comfygit/scripts/install-shell-integration.sh /usr/local/share/comfygit/shell /usr/local/bin || true
fi

%files
/usr/local/bin/${TARGET_DELEGATE_BINARY_NAME[$target_id]}
/usr/local/bin/${BIN_NAME}
/usr/local/bin/${PACKAGE}
/usr/local/share/${PACKAGE}
EOF

            # Run rpmbuild inside the container (install rpmbuild first as base image lacks it)
            # Variables are expanded on host side before passing to container
            podman run --rm --platform "$oci_platform" -v "${container_staging}:/build:z" "$container_image" \
                bash -c "
                    set -e
                    dnf install -y rpm-build >/dev/null 2>&1
                    mkdir -p /build/rpmbuild/{BUILDROOT,BUILD,RPMS,SOURCES,SPECS,SRPMS}
                    cp /build/package.spec /build/rpmbuild/SPECS/
                    mkdir -p /build/rpmbuild/BUILDROOT/${PACKAGE}-${version}-1.${rpm_arch}
                    cp -a /build/payload/. /build/rpmbuild/BUILDROOT/${PACKAGE}-${version}-1.${rpm_arch}/
                    rpmbuild --define '_topdir /build/rpmbuild' --define '_buildarch ${rpm_arch}' \
                        --buildroot /build/rpmbuild/BUILDROOT/${PACKAGE}-${version}-1.${rpm_arch} \
                        -bb /build/rpmbuild/SPECS/package.spec
                "

            # Copy the resulting RPM back
            mkdir -p "$rpm_rpms_dir"
            cp "${container_staging}/rpmbuild/RPMS/${rpm_arch}"/*.rpm "$rpm_rpms_dir/" 2>/dev/null || true
            rm -rf "$container_staging"
        else
            rpmbuild --target "$rpm_arch" --define "_topdir ${rpm_top_dir}" --buildroot "$rpm_build_root" -bb "$spec_path"
        fi

        rpm_path=$(find "$rpm_rpms_dir" -name '*.rpm' -print -quit)
        if [[ -z "$rpm_path" ]]; then
            log_error "RPM output not found after rpmbuild."
            rm -rf "$staging_root"
            exit 1
        fi

        mkdir -p "$target_latest"
        local rpm_output_name="${PACKAGE}-${version}-linux-${rpm_arch}.rpm"
        rpm_latest_path="${target_latest}/${rpm_output_name}"
        cp "$rpm_path" "$rpm_latest_path"
    else
        log_info "Skipping .rpm (--skip-rpm)"
    fi

    rm -rf "$staging_root"

    if [[ -n "$deb_path" ]]; then
        log_success "Created Linux installer package: $deb_path"
    fi
    if [[ -n "$rpm_path" ]]; then
        log_success "Created Linux installer package: $rpm_path"
        log_success "Copied RPM to latest output: $rpm_latest_path"
    fi
}

########################################################################################################################################################################################
# Linux AppImage (.AppImage) packaging
########################################################################################################################################################################################

# Portable AppImage for glibc Linux (Arch, Manjaro, Fedora, etc.).
# Produces one image per target: x86_64 (linux-amd64) and aarch64 (linux-arm64).
# Same-arch: requires appimagetool on PATH.
# Cross-arch x86_64 <-> aarch64: requires podman; runs appimagetool inside a Fedora
# container for the target ISA (QEMU user emulation when needed).

new_linux_appimage() {
    local target_id="$1"
    local version="$2"
    local _binary_path="$3"
    local delegate_binary_path="$4"

    if [[ "$SKIP_APPIMAGE" == true ]]; then
        return 0
    fi

    if [[ "$OSTYPE" != "linux-gnu"* ]] && [[ "$OSTYPE" != "linux"* ]]; then
        return 0
    fi

    local appimage_arch
    case "${TARGET_ARCH[$target_id]}" in
        amd64) appimage_arch='x86_64' ;;
        arm64) appimage_arch='aarch64' ;;
        *)
            log_warning "AppImage not defined for arch ${TARGET_ARCH[$target_id]}; skipping."
            return 0
            ;;
    esac

    local host_m
    host_m=$(uname -m)

    local staging="${DIST_ROOT}/appimage-${target_id}-${version}"
    if [[ -d "$staging" ]]; then
        rm -rf "$staging"
    fi

    local appdir="${staging}/${NAME}.AppDir"
    local bin_dir="${appdir}/usr/local/bin"
    local share_dir="${appdir}/usr/local/share/comfygit"

    mkdir -p "$bin_dir"
    local runtime_name
    runtime_name=$(get_runtime_binary_name "$target_id")
    cp "$delegate_binary_path" "${bin_dir}/${runtime_name}"
    cp "$delegate_binary_path" "${bin_dir}/${TARGET_DELEGATE_BINARY_NAME[$target_id]}"
    set_unix_permissions_if_available "${bin_dir}/${runtime_name}" '+x'
    set_unix_permissions_if_available "${bin_dir}/${TARGET_DELEGATE_BINARY_NAME[$target_id]}" '+x'
    copy_public_launchers "$target_id" "$bin_dir"
    copy_shell_integration_assets "$share_dir"

    local apprun_path="${appdir}/AppRun"
    cat > "$apprun_path" <<EOF
#!/usr/bin/env sh
HERE="\$(dirname "\$0")"
export PATH="\${HERE}/usr/local/bin:\${PATH}"
exec "\${HERE}/usr/local/bin/${BIN_NAME}" "\$@"
EOF
    tr -d '\r' < "$apprun_path" > "${apprun_path}.tmp" && mv "${apprun_path}.tmp" "$apprun_path"
    set_unix_permissions_if_available "$apprun_path" '+x'

    local desktop_path="${appdir}/${PACKAGE}.desktop"
    {
        echo '[Desktop Entry]'
        echo 'Type=Application'
        echo "Name=${NAME}"
        echo 'Comment=Version and changelog management for Git projects'
        echo "Exec=${BIN_NAME}"
        echo "Icon=${APP_IMAGE_ICON_NAME}"
        echo 'Categories=Development;Utility;'
        echo 'Terminal=true'
        echo "X-AppImage-Version=${version}"
    } > "$desktop_path"

    # Copy the icon file to the AppDir
    cp "$APP_IMAGE_ICON_PATH" "${appdir}/${APP_IMAGE_ICON_NAME}.${APP_IMAGE_ICON_EXTENSION}"

    local target_latest
    target_latest=$(get_target_latest_dir "$target_id")
    mkdir -p "$target_latest"

    archive_superseded_for_suffix "$target_latest" "$target_id" "$version" "-${appimage_arch}.AppImage"

    local out_image="${target_latest}/${PACKAGE}-${version}-${appimage_arch}.AppImage"
    if [[ -f "$out_image" ]]; then
        rm -f "$out_image"
    fi

    local out_dir out_fn
    out_dir=$(dirname "$out_image")
    out_fn=$(basename "$out_image")

    local use_podman_cross=false
    if [[ "$host_m" != "$appimage_arch" ]]; then
        if [[ "$host_m" == "x86_64" && "$appimage_arch" == "aarch64" ]] || [[ "$host_m" == "aarch64" && "$appimage_arch" == "x86_64" ]]; then
            use_podman_cross=true
        else
            log_warning "AppImage cross-build from host $host_m to $appimage_arch is not supported; skipping $target_id."
            rm -rf "$staging"
            return 0
        fi
    fi

    if [[ "$use_podman_cross" == true ]]; then
        if ! command -v podman &>/dev/null; then
            log_warning "podman not found; cannot build ${appimage_arch} AppImage on ${host_m} host. Install podman or build on native ${appimage_arch}."
            rm -rf "$staging"
            return 0
        fi

        local container_image="registry.fedoraproject.org/fedora:40-${appimage_arch}"
        local oci_platform
        case "$appimage_arch" in
            x86_64) oci_platform='linux/amd64' ;;
            aarch64) oci_platform='linux/arm64' ;;
            *) oci_platform='linux/amd64' ;;
        esac
        log_info "Building ${appimage_arch} AppImage via podman ($container_image, platform=${oci_platform})..."
        podman pull --platform "$oci_platform" "$container_image" >/dev/null 2>&1 || true

        if ! podman run --rm -i --platform "$oci_platform" \
            -v "${staging}:/build/staging:z" \
            -v "${out_dir}:/build/out:z" \
            "$container_image" \
            bash -s <<EOF
set -euo pipefail
# AppImageKit ships an ELF wrapper that qemu-user rejects as exec format / invalid ABI when
# running Fedora aarch64 images on an x86_64 host. Extract the SquashFS with 7z and run
# usr/bin/appimagetool (needs gpgme for libgpgme.so).
dnf install -y -q curl ca-certificates squashfs-tools zstd file p7zip p7zip-plugins gpgme desktop-file-utils >/dev/null 2>&1
curl -fsSL -o /tmp/appimagetool.AppImage "${APPIMAGETOOL_UPSTREAM_URL}/appimagetool-${appimage_arch}.AppImage"
mkdir -p /tmp/appimagetool-extract
7z x -o/tmp/appimagetool-extract /tmp/appimagetool.AppImage -y >/dev/null
# 7z does not preserve all execute bits; appimagetool shells out to usr/lib/appimagekit/mksquashfs.
find /tmp/appimagetool-extract/usr -type f -exec chmod a+x {} +
ARCH=${appimage_arch} /tmp/appimagetool-extract/usr/bin/appimagetool --no-appstream "/build/staging/${NAME}.AppDir" "/build/out/${out_fn}"
EOF
        then
            log_warning "appimagetool (podman cross-arch) failed for $target_id"
            rm -rf "$staging"
            return 0
        fi
    else
        if ! command -v appimagetool &>/dev/null; then
            log_warning "appimagetool not on PATH; skipping AppImage for $target_id. Install AppImageKit (e.g. package appimagetool) or see https://github.com/AppImage/AppImageKit"
            rm -rf "$staging"
            return 0
        fi

        if ! (
            cd "$staging" || exit 1
            ARCH="$appimage_arch" appimagetool --no-appstream "${NAME}.AppDir" "$out_image"
        ); then
            log_warning "appimagetool failed for $target_id"
            rm -rf "$staging"
            return 0
        fi
    fi

    rm -rf "$staging"
    log_success "Created AppImage: $out_image"
}

is_macos_host() {
    [[ "${OSTYPE:-}" == darwin* ]]
}

is_mac_target() {
    [[ "${TARGET_PLATFORM[$1]:-}" == "macos" ]]
}

MACOS_CI_WORKFLOW='macos-release.yml'
MACOS_CI_ARTIFACT='macos-packages'

mac_ci_arch_workflow_field() {
    case "$MAC_ARCH" in
        intel) echo 'intel' ;;
        silicon) echo 'silicon' ;;
        *) echo 'all' ;;
    esac
}

gh_repo_slug() {
    if command -v gh &>/dev/null; then
        local slug
        slug=$(gh repo view --json nameWithOwner -q .nameWithOwner 2>/dev/null || true)
        if [[ -n "$slug" ]]; then
            echo "$slug"
            return 0
        fi
    fi
    local remote
    remote=$(git -C "$PROJECT_ROOT" remote get-url origin 2>/dev/null || true)
    if [[ "$remote" =~ github\.com[:/]([^/]+/[^/.]+) ]]; then
        echo "${BASH_REMATCH[1]}"
        return 0
    fi
    return 1
}

utc_now_minus_seconds() {
    local seconds="${1:-0}"
    local epoch
    epoch=$(($(date +%s) - seconds))
    if date -u -d "@${epoch}" +%Y-%m-%dT%H:%M:%SZ 2>/dev/null; then
        return 0
    fi
    date -u -r "${epoch}" +%Y-%m-%dT%H:%M:%SZ
}

parse_gh_workflow_run_id() {
    printf '%s\n' "$1" | sed -n 's|.*/actions/runs/\([0-9][0-9]*\).*|\1|p' | tail -1
}

resolve_macos_ci_run_id() {
    local branch="$1"
    local not_before="${2:-}"
    local attempt=0
    local max_attempts=45
    while (( attempt < max_attempts )); do
        local run_id=''
        if [[ -n "$not_before" ]]; then
            run_id=$(gh run list \
                --workflow="$MACOS_CI_WORKFLOW" \
                --branch="$branch" \
                --limit 20 \
                --json databaseId,createdAt \
                -q 'map(select(.createdAt >= "'"$not_before"'")) | .[0].databaseId // empty' 2>/dev/null || true)
        else
            run_id=$(gh run list \
                --workflow="$MACOS_CI_WORKFLOW" \
                --branch="$branch" \
                --limit 1 \
                --json databaseId \
                -q '.[0].databaseId' 2>/dev/null || true)
        fi
        if [[ -n "$run_id" && "$run_id" != "null" ]]; then
            echo "$run_id"
            return 0
        fi
        sleep 2
        attempt=$((attempt + 1))
    done
    return 1
}

print_macos_ci_download_hints() {
    local run_id="$1"
    local repo_slug=''
    if repo_slug=$(gh_repo_slug); then
        log_info "macOS CI run: https://github.com/${repo_slug}/actions/runs/${run_id}"
    fi
    log_info "Manual download (use an empty directory; gh fails if files already exist under the target):"
    log_info "  staging=\$(mktemp -d); gh run download ${run_id} -n ${MACOS_CI_ARTIFACT} -D \"\$staging\""
    log_info "Or wait for this script (default) to finish the run and merge artifacts into dist/."
}

merge_macos_ci_staging_into_dist() {
    local staging="$1"
    mkdir -p "${DIST_ROOT}/latest" "${DIST_ROOT}/old"
    local mac_dir=''
    if [[ -d "$staging/latest" ]]; then
        for mac_dir in "$staging/latest"/macos-*; do
            [[ -d "$mac_dir" ]] || continue
            rm -rf "${DIST_ROOT}/latest/$(basename "$mac_dir")"
        done
        cp -a "$staging/latest/." "${DIST_ROOT}/latest/"
    fi
    if [[ -d "$staging/old" ]]; then
        for mac_dir in "$staging/old"/macos-*; do
            [[ -d "$mac_dir" ]] || continue
            rm -rf "${DIST_ROOT}/old/$(basename "$mac_dir")"
        done
        cp -a "$staging/old/." "${DIST_ROOT}/old/"
    fi
    if [[ ! -d "$staging/latest" && ! -d "$staging/old" ]]; then
        cp -a "$staging/." "${DIST_ROOT}/"
    fi
}

download_macos_ci_artifacts() {
    local run_id="$1"
    local staging
    staging=$(mktemp -d "${TMPDIR:-/tmp}/comfygit-macos-ci.XXXXXX")
    log_info "Downloading '${MACOS_CI_ARTIFACT}' from run ${run_id} into ${staging}..."
    if ! gh run download "$run_id" -n "$MACOS_CI_ARTIFACT" -D "$staging"; then
        rm -rf "$staging"
        log_warning "Failed to download macOS CI artifact '${MACOS_CI_ARTIFACT}'."
        return 1
    fi
    merge_macos_ci_staging_into_dist "$staging"
    rm -rf "$staging"
    log_success "macOS CI artifacts merged into ${DIST_ROOT}/"
}

# macOS binaries use native C deps (e.g. aws-lc-sys via rustls); cross-compile from Linux is unreliable.
trigger_macos_ci_workflow() {
    local version="$1"
    if ! command -v gh &>/dev/null; then
        log_warning "macOS packaging requires a macOS host (or GitHub Actions)."
        log_warning "GitHub CLI 'gh' not found; run the 'macOS release packaging' workflow from GitHub Actions UI."
        return 1
    fi
    if ! gh auth status &>/dev/null; then
        log_warning "GitHub CLI is not authenticated; run 'gh auth login' to trigger and download macOS CI artifacts."
        return 1
    fi
    log_info "macOS targets cannot be cross-compiled on this host (native C deps need the macOS SDK)."
    log_info "Triggering GitHub Actions workflow '${MACOS_CI_WORKFLOW}'..."
    local current_ref
    current_ref=$(git -C "$PROJECT_ROOT" rev-parse --abbrev-ref HEAD 2>/dev/null || echo "main")
    local mac_arch_field
    mac_arch_field=$(mac_ci_arch_workflow_field)
    local trigger_after
    trigger_after=$(utc_now_minus_seconds 10)
    local trigger_output=''
    local trigger_status=0
    trigger_output=$(gh workflow run "$MACOS_CI_WORKFLOW" --ref "$current_ref" --field version="$version" --field arch="$mac_arch_field" 2>&1) || trigger_status=$?
    printf '%s\n' "$trigger_output" >&2
    if (( trigger_status != 0 )); then
        log_warning "Failed to trigger GitHub Actions macOS release workflow; trigger it manually from GitHub UI."
        return 1
    fi
    log_success "macOS release workflow triggered on ref '${current_ref}' (version ${version})."

    local run_id=''
    run_id=$(parse_gh_workflow_run_id "$trigger_output")
    if [[ -z "$run_id" ]]; then
        if ! run_id=$(resolve_macos_ci_run_id "$current_ref" "$trigger_after"); then
            log_warning "Could not resolve workflow run ID yet."
            log_warning "List runs: gh run list --workflow=${MACOS_CI_WORKFLOW}"
            return 0
        fi
    fi
    log_info "Tracking macOS CI run ${run_id}."
    print_macos_ci_download_hints "$run_id"

    if [[ "$MAC_CI_WAIT" != true ]]; then
        log_info "Skipping CI wait/download (--mac-ci-no-wait)."
        return 0
    fi

    log_info "Waiting for macOS CI (Ctrl+C to stop waiting; download later with gh run download ${run_id})..."
    if ! gh run watch "$run_id" --exit-status; then
        log_error "macOS CI failed. Logs: gh run view ${run_id} --log-failed"
        return 1
    fi
    download_macos_ci_artifacts "$run_id"
}

partition_targets_for_host() {
    LOCAL_TARGETS=()
    MAC_TARGETS_OFF_HOST=()
    for target_id in $TARGETS; do
        if is_mac_target "$target_id" && ! is_macos_host; then
            MAC_TARGETS_OFF_HOST+=("$target_id")
        else
            LOCAL_TARGETS+=("$target_id")
        fi
    done
}

# Main execution
cd "$PROJECT_ROOT"

TARGETS=$(get_selected_targets)

if [[ "$TEST_ONLY" == true ]]; then
    log_info "Running cargo fmt, cargo clippy, and cargo test in test-only mode."

    log_info "Running cargo fmt --check..."
    cargo fmt --all -- --check

    log_info "Running cargo clippy..."
    cargo clippy --all-targets --all-features -- -D warnings

    log_info "Running cargo test..."
    cargo test

    log_success "Test-only mode complete; skipping compilation and packaging."
    exit 0
fi

if [[ "$NO_CHECKS" == true ]]; then
    log_warning "Skipping cargo fmt, cargo clippy, and cargo test because --no-checks was specified."
else
    log_info "Running cargo fmt --check..."
    cargo fmt --all -- --check

    log_info "Running cargo clippy..."
    cargo clippy --all-targets --all-features -- -D warnings

    if [[ "$SKIP_TEST" == true ]]; then
        log_warning "Skipping cargo test because --skip-test was specified."
    else
        log_info "Running cargo test..."
        cargo test
    fi
fi

VERSION=$(get_project_version)

mkdir -p "$DIST_ROOT"
mkdir -p "${DIST_ROOT}/latest"
mkdir -p "${DIST_ROOT}/old"

partition_targets_for_host

if [[ ${#MAC_TARGETS_OFF_HOST[@]} -gt 0 ]] && [[ ${#LOCAL_TARGETS[@]} -eq 0 ]]; then
    if ! trigger_macos_ci_workflow "$VERSION"; then
        exit 1
    fi
    log_success "Release build complete (macOS packages from GitHub Actions in dist/)."
    exit 0
fi

if [[ ${#MAC_TARGETS_OFF_HOST[@]} -gt 0 ]]; then
    trigger_macos_ci_workflow "$VERSION" || true
fi

for target_id in "${LOCAL_TARGETS[@]}"; do
    binary_path=$(build_release_binary "$target_id")
    delegate_binary_path=$(get_delegate_binary_path "$target_id")
    if [[ ! -f "$delegate_binary_path" ]]; then
        log_error "Delegate binary not found at $delegate_binary_path"
        exit 1
    fi
    new_portable_package "$target_id" "$VERSION" "$binary_path" "$delegate_binary_path" > /dev/null

    if should_package_installer_for_target "$target_id"; then
        case "${TARGET_PLATFORM[$target_id]}" in
            'windows')
                if command -v wix &>/dev/null; then
                    log_warning "WiX v4 CLI found but not supported in this script. Use wixl (msitools) on Linux."
                elif command -v wixl &>/dev/null; then
                    new_windows_installer_artifacts "$target_id" "$VERSION" "$binary_path" "$delegate_binary_path" || {
                        log_warning "Windows MSI packaging failed for $target_id"
                    }
                else
                    log_warning "wixl (msitools) was not found. MSI packaging was skipped."
                    log_warning "Install msitools and rerun this script to produce an MSI."
                fi
                ;;
            'macos')
                new_mac_installer_artifacts "$target_id" "$VERSION" "$binary_path" "$delegate_binary_path" || {
                    log_warning "Mac installer packaging failed for $target_id"
                }
                ;;
            'linux')
                if [[ "$OSTYPE" == "linux-gnu"* ]] || [[ "$OSTYPE" == "linux"* ]]; then
                    new_linux_installer_artifacts "$target_id" "$VERSION" "$binary_path" "$delegate_binary_path" || {
                        log_warning "Linux installer packaging failed for $target_id"
                    }
                    new_linux_appimage "$target_id" "$VERSION" "$binary_path" "$delegate_binary_path" || {
                        log_warning "Linux AppImage packaging failed for $target_id"
                    }
                else
                    log_warning "Linux installer packaging requires Linux host. Skipping for $target_id."
                fi
                ;;
        esac
    elif [[ "${TARGET_SUPPORTS_INSTALLER[$target_id]}" != "true" ]] && [[ "$SKIP_MSI" == false ]]; then
        log_warning "Installer packaging is not configured for $target_id; only the portable archive was produced."
    fi
done

log_success "Release build complete!"
