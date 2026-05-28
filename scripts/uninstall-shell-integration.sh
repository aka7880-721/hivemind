#!/usr/bin/env sh
set -eu
# Copyright © 2026 ComfyHome™
# All rights reserved.
#
# Licensed under the ComfyGit SA-PS License
#
# For details, see the LICENSE file in the repository root.
#
# Reverses scripts/install-shell-integration.sh (user install, non-root).
# Run from AppImage: ./comfygit-*.AppImage uninstall-shell
# Or: sh uninstall-shell-integration.sh [bin_dir]
#
# bin_dir defaults to ~/.local/bin (or set COMFYGIT_BIN_DIR before invoking).

config_home=${XDG_CONFIG_HOME:-"$HOME/.config"}
target_dir="$config_home/comfygit"
target_file="$target_dir/cg.sh"
bin_dir=${1:-}
if [ -z "$bin_dir" ] && [ -n "${COMFYGIT_BIN_DIR:-}" ]; then
  bin_dir=$COMFYGIT_BIN_DIR
fi

remove_line_exact() {
  profile_path=$1
  line=$2
  if [ ! -f "$profile_path" ]; then
    return 0
  fi
  tmp=$(mktemp "${TMPDIR:-/tmp}/cg-uninstall.XXXXXX")
  grep -Fvx "$line" "$profile_path" >"$tmp" || true
  mv "$tmp" "$profile_path"
}

# Remove marker line and the following PATH line (install-shell PATH hook).
strip_path_hook() {
  profile_path=$1
  marker="# comfygit-install-shell: PATH"
  path_line='[ -d "$HOME/.local/bin" ] && case ":${PATH:-}:" in *:"$HOME/.local/bin":*) ;; *) PATH="$HOME/.local/bin${PATH:+:$PATH}"; export PATH ;; esac'
  if [ ! -f "$profile_path" ]; then
    return 0
  fi
  tmp=$(mktemp "${TMPDIR:-/tmp}/cg-uninstall.XXXXXX")
  skip=0
  while IFS= read -r line || [ -n "$line" ]; do
    if [ "$skip" -eq 1 ]; then
      skip=0
      continue
    fi
    if [ "$line" = "$marker" ]; then
      skip=1
      continue
    fi
    if [ "$line" = "$path_line" ]; then
      continue
    fi
    printf '%s\n' "$line"
  done <"$profile_path" >"$tmp"
  mv "$tmp" "$profile_path"
}

strip_fish_config_fallback() {
  fish_config="$config_home/fish/config.fish"
  marker="# comfygit-install-shell"
  src_line="test -f \"$config_home/fish/conf.d/comfygit.fish\"; and source \"$config_home/fish/conf.d/comfygit.fish\""
  if [ ! -f "$fish_config" ]; then
    return 0
  fi
  tmp=$(mktemp "${TMPDIR:-/tmp}/cg-uninstall.XXXXXX")
  skip=0
  while IFS= read -r line || [ -n "$line" ]; do
    if [ "$skip" -eq 1 ]; then
      skip=0
      continue
    fi
    if [ "$line" = "$marker" ]; then
      skip=1
      continue
    fi
    if [ "$line" = "$src_line" ]; then
      continue
    fi
    printf '%s\n' "$line"
  done <"$fish_config" >"$tmp"
  mv "$tmp" "$fish_config"
}

strip_pwsh_cg_block() {
  f=$1
  marker="# comfygit-install-shell (cg for pwsh)"
  if [ ! -f "$f" ]; then
    return 0
  fi
  if ! grep -F "$marker" "$f" >/dev/null 2>&1; then
    return 0
  fi
  tmp=$(mktemp "${TMPDIR:-/tmp}/cg-uninstall.XXXXXX")
  skip=0
  while IFS= read -r line || [ -n "$line" ]; do
    if [ "$skip" -eq 1 ]; then
      trimmed=$(printf '%s' "$line" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')
      if [ "$trimmed" = "}" ]; then
        skip=0
      fi
      continue
    fi
    if [ "$line" = "$marker" ]; then
      skip=1
      continue
    fi
    printf '%s\n' "$line"
  done <"$f" >"$tmp"
  mv "$tmp" "$f"
}

remove_if_comfygit_launcher() {
  path=$1
  if [ ! -f "$path" ]; then
    return 0
  fi
  # The compiled binary embeds this marker string; only treat shell launchers as ours.
  case $(head -c 2 "$path" 2>/dev/null) in
    '#!'*) ;;
    *) return 0 ;;
  esac
  if grep -Fq "comfygit-install-launcher" "$path" 2>/dev/null; then
    rm -f "$path"
    printf '%s\n' "Removed AppImage launcher script $path"
  fi
}

remove_if_asset_launcher() {
  path=$1
  if [ ! -f "$path" ]; then
    return 0
  fi
  if head -n 1 "$path" | grep -q '^#!.*sh' 2>/dev/null \
    && grep -Fq 'ComfyGit' "$path" \
    && grep -Fq 'script_dir=' "$path" 2>/dev/null; then
    rm -f "$path"
    printf '%s\n' "Removed launcher wrapper $path"
  fi
}

uninstall_user_shell_integration() {
  if [ -z "$bin_dir" ]; then
    bin_dir="$HOME/.local/bin"
  fi

  remove_line_exact "$HOME/.bashrc" ". \"$target_file\""
  remove_line_exact "$HOME/.zshrc" ". \"$target_file\""
  remove_line_exact "$HOME/.zprofile" ". \"$target_file\""

  strip_path_hook "$HOME/.profile"
  strip_path_hook "$HOME/.zprofile"

  strip_fish_config_fallback

  rm -f "$config_home/fish/conf.d/comfygit.fish"
  printf '%s\n' "Removed fish conf.d snippet (if present)"

  strip_pwsh_cg_block "$config_home/powershell/profile.ps1"
  strip_pwsh_cg_block "$config_home/powershell/Microsoft.PowerShell_profile.ps1"

  rm -f "$target_file"
  rmdir "$target_dir" 2>/dev/null || true
  printf '%s\n' "Removed $target_file (if present)"

  remove_if_comfygit_launcher "$bin_dir/ComfyGit"
  remove_if_asset_launcher "$bin_dir/cg"
  remove_if_asset_launcher "$bin_dir/comfygit"
  rm -f "$bin_dir/cg.ps1"

  printf '%s\n' "ComfyGit user shell integration removed. Open a new terminal session."
}

if [ "$(id -u)" -eq 0 ] && [ "${COMFYGIT_UNINSTALL_AS_USER:-0}" != "1" ]; then
  printf '%s\n' "This uninstall script is for per-user integration only. Run as a normal user (not root), or set COMFYGIT_UNINSTALL_AS_USER=1 if you know you used install-global." >&2
  exit 1
fi

uninstall_user_shell_integration
