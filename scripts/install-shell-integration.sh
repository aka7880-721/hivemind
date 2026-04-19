#!/usr/bin/env sh
set -eu

shell_asset_dir=${1:-/usr/local/share/comfygit/shell}
bin_dir=${2:-}
config_home=${XDG_CONFIG_HOME:-"$HOME/.config"}
target_dir="$config_home/comfygit"
target_file="$target_dir/cg.sh"

detect_target_user() {
  if [ -n "${SUDO_USER:-}" ] && [ "${SUDO_USER}" != "root" ]; then
    printf '%s' "$SUDO_USER"
    return
  fi

  console_user=$(stat -f '%Su' /dev/console 2>/dev/null || true)
  if [ -n "$console_user" ] && [ "$console_user" != "root" ]; then
    printf '%s' "$console_user"
    return
  fi

  logname 2>/dev/null || true
}

if [ "$(id -u)" -eq 0 ] && [ "${COMFYGIT_INSTALL_AS_USER:-0}" != "1" ]; then
  target_user=$(detect_target_user)
  if [ -z "$target_user" ] || [ "$target_user" = "root" ]; then
    printf '%s\n' 'ComfyGit could not detect a target user for shell activation. Run this script again as the intended user.' >&2
    exit 0
  fi

  if [ -z "$bin_dir" ]; then
    bin_dir=/usr/local/bin
  fi

  su - "$target_user" -c "COMFYGIT_INSTALL_AS_USER=1 '$0' '$shell_asset_dir' '$bin_dir'"
  exit $?
fi

if [ -z "$bin_dir" ]; then
  if command -v ComfyGit >/dev/null 2>&1; then
    bin_dir=$(dirname "$(command -v ComfyGit)")
  else
    bin_dir="$HOME/.cargo/bin"
  fi
fi

mkdir -p "$target_dir"
mkdir -p "$bin_dir"
cp "$shell_asset_dir/cg" "$bin_dir/cg"
chmod +x "$bin_dir/cg"
cp "$shell_asset_dir/cg.sh" "$target_file"

append_once() {
  profile_path=$1
  line=$2

  if [ ! -f "$profile_path" ]; then
    touch "$profile_path"
  fi

  if ! grep -F "$line" "$profile_path" >/dev/null 2>&1; then
    printf '\n%s\n' "$line" >> "$profile_path"
  fi
}

append_once "$HOME/.bashrc" ". \"$target_file\""
append_once "$HOME/.zshrc" ". \"$target_file\""

printf '%s\n' "Installed ComfyGit shell integration to $target_file"
printf '%s\n' "Installed the cg launcher wrapper to $bin_dir/cg"
printf '%s\n' "Open a new bash or zsh session to enable real 'cg cd <alias>' support."