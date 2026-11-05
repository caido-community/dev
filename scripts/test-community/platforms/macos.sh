#!/usr/bin/env bash
# Sourced by the community packaging task.
timeout_command=$(command -v gtimeout || true)
platform_check() {
  [[ -n $timeout_command && -x $timeout_command ]] || {
    echo 'GNU timeout is required; run brew install coreutils on macOS.' >&2
    return 1
  }
}

platform_shell_path() {
  printf '%s\n' "$1"
}

platform_environment() {
  : # The shared POSIX environment needs no additional variables.
}
