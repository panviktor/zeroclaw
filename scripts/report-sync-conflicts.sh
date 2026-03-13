#!/usr/bin/env bash
set -euo pipefail

OUTPUT_PATH="${1:-.sync-conflict-report.md}"
ROOT_DIR="$(git rev-parse --show-toplevel)"
cd "$ROOT_DIR"

mapfile -t CONFLICTS < <(git diff --name-only --diff-filter=U | sort)

classify_path() {
  local path="$1"
  case "$path" in
    src/gateway/ipc.rs|src/tools/agents_ipc.rs)
      echo "fork-owned"
      ;;
    src/config/schema.rs|src/config/mod.rs|src/gateway/mod.rs|src/gateway/api.rs|src/security/pairing.rs|src/tools/mod.rs|src/onboard/wizard.rs|src/cron/*|src/channels/*|src/agent/*)
      echo "shared-hotspot"
      ;;
    *)
      echo "upstream-owned"
      ;;
  esac
}

security_sensitive() {
  local path="$1"
  case "$path" in
    src/security/*|src/gateway/*|src/config/*|src/channels/*|src/agent/*)
      echo "yes"
      ;;
    *)
      echo "no"
      ;;
  esac
}

recommended_tests() {
  local path="$1"
  case "$path" in
    src/security/*|src/gateway/*)
      echo "cargo test -- ipc ; security/pairing tests ; gateway handler tests"
      ;;
    src/config/*)
      echo "cargo test config/schema ; mask/hydrate smoke ; default config regression"
      ;;
    src/tools/*)
      echo "cargo test -- agents_ipc ; tool registration smoke"
      ;;
    src/cron/*|src/agent/*)
      echo "spawn/cron smoke ; integration tests"
      ;;
    *)
      echo "cargo test"
      ;;
  esac
}

{
  echo "# Upstream Sync Conflict Report"
  echo
  echo "Generated: $(date -Iseconds)"
  echo
  if [ "${#CONFLICTS[@]}" -eq 0 ]; then
    echo "No conflicted files detected."
    exit 0
  fi

  echo "## Summary"
  echo
  echo "- Conflicted files: ${#CONFLICTS[@]}"
  echo "- Manual review required: yes"
  echo
  echo "## Conflicted Paths"
  echo
  echo "| Path | Class | Security-sensitive | Recommended checks |"
  echo "|------|-------|--------------------|--------------------|"
  for path in "${CONFLICTS[@]}"; do
    printf '| `%s` | `%s` | `%s` | %s |\n' \
      "$path" \
      "$(classify_path "$path")" \
      "$(security_sensitive "$path")" \
      "$(recommended_tests "$path")"
  done
  echo
  echo "## Required Reviewers"
  echo
  echo "- Opus"
  echo '- Additional security/architecture review if any `shared-hotspot` path touches auth, approval, pairing, gateway or channels'
  echo
  echo "## Notes"
  echo
  echo "- Re-run fork invariants after conflict resolution"
  echo '- Update `fork-delta.md` if a hotspot boundary changed'
  echo "- If upstream semantics changed in shared-hotspot files, prefer restoring fork behavior through isolated hooks instead of deeper patching"
} > "$OUTPUT_PATH"

echo "Conflict report written to $OUTPUT_PATH"
