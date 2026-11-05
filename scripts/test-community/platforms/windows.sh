#!/usr/bin/env bash
# Sourced by the community packaging task under Git Bash.
timeout_command=/usr/bin/timeout

platform_check() {
  [[ -x $timeout_command ]] || {
    echo 'GNU timeout is required; use Git for Windows Git Bash.' >&2
    return 1
  }
  command -v cygpath >/dev/null || { echo 'cygpath is required; use Git Bash.' >&2; return 1; }
}

platform_shell_path() {
  # Mise returns native paths; Bash needs POSIX paths for PATH and executable checks.
  cygpath -u "$1"
}

platform_environment() {
  clean_home=$(cygpath -m "$home")
  clean_work=$(cygpath -m "$work")
  # Native Node/pnpm need system variables, but never the user's profile.
  package_env+=("SystemRoot=${SYSTEMROOT:-${SystemRoot:-C:\\Windows}}"
    "WINDIR=${WINDIR:-C:\\Windows}" "COMSPEC=${COMSPEC:-C:\\Windows\\System32\\cmd.exe}"
    "PATHEXT=${PATHEXT:-.COM;.EXE;.BAT;.CMD}" "USERPROFILE=$clean_home"
    "APPDATA=$clean_home/AppData/Roaming" "LOCALAPPDATA=$clean_home/AppData/Local"
    "TEMP=$clean_work" "TMP=$clean_work")
}
