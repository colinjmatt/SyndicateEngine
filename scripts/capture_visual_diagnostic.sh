#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out_dir="$repo_root/visual_diagnostics"
mkdir -p "$out_dir"

stamp="$(date +"%Y%m%d-%H%M%S")"
out_path="$out_dir/syndicate_engine-$stamp.png"

osascript -e 'tell application "System Events" to tell process "syndicate_engine" to set frontmost to true' >/dev/null 2>&1 || true
sleep 1
screencapture -x "$out_path"

printf 'Wrote local-only visual diagnostic: %s\n' "$out_path"
printf 'Do not commit this screenshot; visual_diagnostics/ is ignored.\n'
