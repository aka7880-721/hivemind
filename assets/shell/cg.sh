#!/usr/bin/env sh
# Bash/zsh integration: source this file from ~/.bashrc or ~/.zshrc (see `cg install-shell`).
# Fish users: do not source this file — use ~/.config/fish/conf.d/comfygit.fish instead.

# Many distros omit ~/.local/bin from the default PATH; AppImage `install-shell` installs wrappers there.
if [ -n "${HOME:-}" ] && [ -d "$HOME/.local/bin" ]; then
  case ":${PATH:-}:" in
    *:"$HOME/.local/bin":*) ;;
    *) PATH="$HOME/.local/bin${PATH:+:$PATH}" && export PATH ;;
  esac
fi

# Resolve the ComfyGit CLI even when ~/.local/bin is not on PATH (common in fish / minimal zsh).
comfygit_cli_exe() {
  if [ -n "${COMFYGIT_EXE:-}" ] && [ -x "${COMFYGIT_EXE}" ]; then
    printf '%s\n' "${COMFYGIT_EXE}"
    return 0
  fi
  if command -v ComfyGit >/dev/null 2>&1; then
    command -v ComfyGit
    return 0
  fi
  if [ -n "${HOME:-}" ] && [ -x "${HOME}/.local/bin/ComfyGit" ]; then
    printf '%s\n' "${HOME}/.local/bin/ComfyGit"
    return 0
  fi
  if [ -x /usr/local/bin/ComfyGit ]; then
    printf '%s\n' /usr/local/bin/ComfyGit
    return 0
  fi
  return 1
}

cg() {
  comfygit_exe="$(comfygit_cli_exe)" || {
    printf '%s\n' "ComfyGit executable not found. Run 'cg install-shell' (AppImage) or add ComfyGit to PATH (or set COMFYGIT_EXE)." >&2
    return 127
  }

  if [ "$#" -gt 0 ] && [ "$1" = "cd" ]; then
    if [ "$#" -eq 3 ]; then
      target_dir="$("$comfygit_exe" pwd "$2" "$3")" || return $?
    elif [ "$#" -eq 2 ]; then
      target_dir="$("$comfygit_exe" pwd "$2")" || return $?
    else
      printf '%s\n' "usage: cg cd <alias> [sub]" >&2
      return 2
    fi

    cd "$target_dir" || return $?
    return 0
  fi

  "$comfygit_exe" "$@"
}