#!/usr/bin/env bash
# Offline checks for the platform interface; run with Bash 3.2 or newer.
set -euo pipefail
platforms=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)

# Exercise POSIX setup without requiring another host's timeout binary.
for platform in linux macos; do
  (
    command() {
      case "$*" in
        '-v timeout'|'-v gtimeout') printf '/usr/bin/true\n' ;;
        *) builtin command "$@" ;;
      esac
    }
    source "$platforms/$platform.sh"
    platform_check
    [[ $timeout_command == /usr/bin/true ]]
    [[ $(platform_shell_path '/tmp/path with spaces') == '/tmp/path with spaces' ]]
    clean_home='/tmp/home with spaces' clean_work=/tmp/work
    package_env=(CI=true)
    platform_environment
    [[ $clean_home == '/tmp/home with spaces' && $clean_work == /tmp/work ]]
    [[ ${#package_env[@]} == 1 ]]
  )
done

# Simulate only Windows path conversion; no native Windows processes run here.
(
  cygpath() {
    case "$1" in
      -u) printf '/c/Tools/pnpm.exe\n' ;;
      -m) printf 'C:/converted%s\n' "$2" ;;
      *) return 1 ;;
    esac
  }
  source "$platforms/windows.sh"
  [[ $timeout_command == /usr/bin/timeout ]]
  [[ $(platform_shell_path 'C:\Tools\pnpm.exe') == /c/Tools/pnpm.exe ]]
  home='/home with spaces' work=/work
  clean_home=$home clean_work=$work
  package_env=(CI=true)
  platform_environment
  [[ $clean_home == 'C:/converted/home with spaces' && $clean_work == C:/converted/work ]]
  [[ ${#package_env[@]} == 10 ]]
  [[ ${package_env[5]} == 'USERPROFILE=C:/converted/home with spaces' ]]
  [[ ${package_env[8]} == TEMP=C:/converted/work ]]
  [[ ${package_env[9]} == TMP=C:/converted/work ]]
)
printf 'Platform interface checks passed (Windows conversion simulated).\n'
