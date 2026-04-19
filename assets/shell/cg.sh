#!/usr/bin/env sh

cg() {
  if [ "$#" -gt 0 ] && [ "$1" = "cd" ]; then
    if [ "$#" -ne 2 ]; then
      printf '%s\n' "usage: cg cd <alias>" >&2
      return 2
    fi

    target_dir="$(command cg-bin pwd "$2")" || return $?
    cd "$target_dir" || return $?
    return 0
  fi

  command cg-bin "$@"
}