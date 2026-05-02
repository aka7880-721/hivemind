#!/usr/bin/env sh
set -eu

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

  printf '%s\n' "Installed ComfyGit shell integration to $target_file"
  printf '%s\n' "Installed fish integration to $fish_conf_d/comfygit.fish"
  printf '%s\n' "Installed the cg launcher wrapper to $bin_dir/cg"
  printf '%s\n' "Installed the comfygit launcher wrapper to $bin_dir/comfygit"
  printf '%s\n' "Open a new bash, zsh, or fish session to enable real 'cg cd <alias>' support."
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