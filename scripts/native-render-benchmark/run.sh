#!/usr/bin/env bash
# Nonshipping, disposable VM only. No root operation precedes these guards.
set -euo pipefail
[[ "${GITHUB_ACTIONS:-}" == true && "${RUNNER_ENVIRONMENT:-}" == github-hosted && "${RUNNER_OS:-}" == Linux ]]
[[ $(uname -s) == Linux && $# == 2 && ( "$1" == smoke || "$1" == pilot || "$1" == pilot3 || "$1" == pilot5 ) ]]
[[ -f "$2" && -x "$2" && ! -L "$2" && "$2" == /* ]]
: "${RUNNER_TEMP:?}"
[[ "$RUNNER_TEMP" == /* && -d "$RUNNER_TEMP" && ! -L "$RUNNER_TEMP" ]]
script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
exec sudo /usr/bin/env -i PATH=/usr/sbin:/usr/bin:/sbin:/bin \
  GITHUB_ACTIONS=true RUNNER_ENVIRONMENT=github-hosted RUNNER_OS=Linux \
  RUNNER_TEMP="$RUNNER_TEMP" /usr/bin/python3 "$script_dir/runner.py" --mode "$1" --binary "$2"
