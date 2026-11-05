#!/usr/bin/env bash
# Sourced by the community packaging task.
timeout_command=$(command -v timeout || true)
platform_check() {
  [[ -n $timeout_command && -x $timeout_command ]] || {
    echo 'GNU timeout is required; install coreutils on Linux.' >&2
    return 1
  }
}

platform_shell_path() {
  printf '%s\n' "$1"
}

platform_environment() {
  : # The shared POSIX environment needs no additional variables.
}
