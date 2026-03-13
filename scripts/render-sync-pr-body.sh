#!/usr/bin/env bash
set -euo pipefail

META_PATH="${1:-.sync-meta}"
TEMPLATE_PATH="${2:-weekly-sync-pr-template.md}"
OUTPUT_PATH="${3:-.sync-pr-body.md}"

if [ ! -f "$META_PATH" ]; then
  echo "Missing metadata file: $META_PATH" >&2
  exit 1
fi

# shellcheck disable=SC1090
source "$META_PATH"

sed \
  -e "s|{{SYNC_BRANCH}}|${SYNC_BRANCH}|g" \
  -e "s|{{UPSTREAM_SHA}}|${UPSTREAM_SHA}|g" \
  -e "s|{{OLD_MAIN_SHA}}|${OLD_MAIN_SHA}|g" \
  "$TEMPLATE_PATH" > "$OUTPUT_PATH"

echo "Rendered PR body to $OUTPUT_PATH"
