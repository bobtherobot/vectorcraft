#!/usr/bin/env bash
# What the "VectorCraft (dev)" launcher runs, in a terminal: brings the app up to date (rebuilds
# this checkout when the sources changed, or downloads a newer release AppImage without Rust),
# then starts it and closes. When that fails, the terminal stays open on the error.
#
# Usage: dev-launcher/linux/launch.sh [--local | --download] [--build | --no-build]
#                                  [--repo OWNER/NAME] [--] [FILE…]
#   Options as for install.sh, which writes the ones you gave it into the launcher.
#   FILE…  documents to open
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"

# Keep the terminal open on a failure so it can be read (when there is a terminal to read).
on_exit() {
  local status=$?
  if [ "$status" -ne 0 ] && [ -t 0 ]; then
    echo
    echo "VectorCraft didn't start (exit status $status)."
    read -r -p "Press Enter to close. " _ || true
  fi
}
trap on_exit EXIT

while [ $# -gt 0 ]; do
  case "$1" in
    -h | --help) awk 'NR > 1 && !/^#/ { exit } NR > 1' "$0"; exit 0 ;;
    --) shift; break ;;
  esac
  n=0; parse_app_option "$@" || n=$?
  [ "$n" -gt 0 ] || break # the documents to open
  shift "$n"
done

prepare_app
start_app "$@"
