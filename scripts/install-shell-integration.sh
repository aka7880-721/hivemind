#!/usr/bin/env sh
set -eu
#
# AppImage users: run  ./Your.AppImage install-shell  (writes ~/.local/bin/ComfyGit and runs this
# script). See also `cg help` → install-shell. Alternatively invoke this script with shell + bin paths.

shell_asset_dir=${1:-/usr/local/share/comfygit/shell}
bin_dir=${2:-}
config_home=${XDG_CONFIG_HOME:-"$HOME/.config"}
target_dir="$config_home/comfygit"
target_file="$target_dir/cg.sh"
global_target_file=${COMFYGIT_GLOBAL_PROFILE:-/etc/profile.d/comfygit.sh}

append_once() {
  profile_path=$1
  line=$2

  parent_dir=$(dirname "$profile_path")
  if [ ! -d "$parent_dir" ]; then
    return
  fi

  if [ ! -f "$profile_path" ]; then
    touch "$profile_path"
  fi

  if ! grep -F "$line" "$profile_path" >/dev/null 2>&1; then
    printf '\n%s\n' "$line" >> "$profile_path"
  fi
}

copy_launchers() {
  mkdir -p "$bin_dir"
  cp "$shell_asset_dir/cg" "$bin_dir/cg"
  chmod +x "$bin_dir/cg"
  cp "$shell_asset_dir/cg" "$bin_dir/comfygit"
  chmod +x "$bin_dir/comfygit"
  if [ -f "$shell_asset_dir/cg.ps1" ]; then
    cp "$shell_asset_dir/cg.ps1" "$bin_dir/cg.ps1"
    chmod +x "$bin_dir/cg.ps1"
  fi
}

# pwsh on Linux/macOS: no Windows-only installer ran, so register a cg function in the user profile.
install_pwsh_user_profile() {
  if [ ! -f "$bin_dir/cg.ps1" ]; then
    return 0
  fi
  pwsh_profile_dir="$config_home/powershell"
  mkdir -p "$pwsh_profile_dir"
  pwsh_profile="$pwsh_profile_dir/Microsoft.PowerShell_profile.ps1"
  marker="# comfygit-install-shell (cg for pwsh)"
  if [ -f "$pwsh_profile" ] && grep -F "$marker" "$pwsh_profile" >/dev/null 2>&1; then
    return 0
  fi
  ps1_path="$bin_dir/cg.ps1"
  escaped_path=$(printf '%s' "$ps1_path" | sed "s/'/''/g")
  {
    printf '\n%s\n' "$marker"
    printf '%s\n' "function cg {"
    printf "%s\n" "  & '$escaped_path' @args"
    printf '%s\n' "}"
  } >>"$pwsh_profile"
}

install_user_shell_integration() {
  mkdir -p "$target_dir"
  copy_launchers
  cp "$shell_asset_dir/cg.sh" "$target_file"

  fish_conf_d="$config_home/fish/conf.d"
  mkdir -p "$fish_conf_d"
  cp "$shell_asset_dir/cg.fish" "$fish_conf_d/comfygit.fish"

  append_once "$HOME/.bashrc" ". \"$target_file\""
  append_once "$HOME/.zshrc" ". \"$target_file\""
  install_pwsh_user_profile

  printf '%s\n' "Installed ComfyGit shell integration to $target_file"
  printf '%s\n' "Installed fish integration to $fish_conf_d/comfygit.fish"
  printf '%s\n' "Installed the cg launcher wrapper to $bin_dir/cg"
  printf '%s\n' "Installed the comfygit launcher wrapper to $bin_dir/comfygit"
  if [ -f "$bin_dir/cg.ps1" ]; then
    printf '%s\n' "Installed cg.ps1 for PowerShell and updated $config_home/powershell/Microsoft.PowerShell_profile.ps1 (if needed)."
  fi
  printf '%s\n' "Open a new bash, zsh, fish, or pwsh session so 'cg' and 'cg cd <alias>' are available."
}

install_global_shell_integration() {
  global_target_dir=$(dirname "$global_target_file")
  mkdir -p "$global_target_dir"
  copy_launchers
  cp "$shell_asset_dir/cg.sh" "$global_target_file"
  chmod 0644 "$global_target_file"

  fish_global_conf_d=/etc/fish/conf.d
  if mkdir -p "$fish_global_conf_d" 2>/dev/null; then
    cp "$shell_asset_dir/cg.fish" "$fish_global_conf_d/comfygit.fish"
    chmod 0644 "$fish_global_conf_d/comfygit.fish"
  fi

  source_line=". \"$global_target_file\""
  append_once /etc/profile "$source_line"
  append_once /etc/bash.bashrc "$source_line"
  append_once /etc/bashrc "$source_line"
  append_once /etc/zsh/zshrc "$source_line"
  append_once /etc/zshrc "$source_line"

  printf '%s\n' "Installed ComfyGit shell integration system-wide at $global_target_file"
  if [ -f "$fish_global_conf_d/comfygit.fish" ]; then
    printf '%s\n' "Installed fish integration system-wide at $fish_global_conf_d/comfygit.fish"
  fi
  printf '%s\n' "Installed the cg launcher wrapper to $bin_dir/cg"
  printf '%s\n' "Installed the comfygit launcher wrapper to $bin_dir/comfygit"
  printf '%s\n' "Open a new shell session to enable real 'cg cd <alias>' support."
}

if [ -z "$bin_dir" ]; then
  if command -v ComfyGit >/dev/null 2>&1; then
    bin_dir=$(dirname "$(command -v ComfyGit)")
  else
    bin_dir="$HOME/.cargo/bin"
  fi
fi

if [ "$(id -u)" -eq 0 ] && [ "${COMFYGIT_INSTALL_AS_USER:-0}" != "1" ]; then
  install_global_shell_integration
  exit 0
fi

install_user_shell_integration