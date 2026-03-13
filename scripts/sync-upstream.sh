#!/usr/bin/env bash
set -euo pipefail

UPSTREAM_REMOTE="${UPSTREAM_REMOTE:-upstream}"
FORK_REMOTE="${FORK_REMOTE:-origin}"
UPSTREAM_BRANCH="${UPSTREAM_BRANCH:-master}"
MAIN_BRANCH="${MAIN_BRANCH:-main}"
VENDOR_BRANCH="${VENDOR_BRANCH:-vendor/upstream-master}"
SYNC_BRANCH_PREFIX="${SYNC_BRANCH_PREFIX:-sync/upstream}"
REPORT_PATH="${REPORT_PATH:-.sync-conflict-report.md}"
SYNC_META_PATH="${SYNC_META_PATH:-.sync-meta}"
ROOT_DIR="$(git rev-parse --show-toplevel)"
DATE_TAG="$(date +%Y%m%d)"
SYNC_BRANCH="${SYNC_BRANCH_PREFIX}-${DATE_TAG}"

cd "$ROOT_DIR"

if ! git diff --quiet || ! git diff --cached --quiet; then
  echo "Worktree is not clean. Commit or stash changes first." >&2
  exit 2
fi

git config rerere.enabled true
git config rerere.autoupdate true

echo "==> Fetching remotes"
git fetch "$UPSTREAM_REMOTE" --prune
git fetch "$FORK_REMOTE" --prune

echo "==> Updating vendor branch: $VENDOR_BRANCH"
if git show-ref --verify --quiet "refs/heads/$VENDOR_BRANCH"; then
  git switch "$VENDOR_BRANCH"
else
  git switch --create "$VENDOR_BRANCH" "$UPSTREAM_REMOTE/$UPSTREAM_BRANCH"
fi
git merge --ff-only "$UPSTREAM_REMOTE/$UPSTREAM_BRANCH"

OLD_MAIN_SHA="$(git rev-parse "$FORK_REMOTE/$MAIN_BRANCH")"
UPSTREAM_SHA="$(git rev-parse "$VENDOR_BRANCH")"

echo "==> Updating local main from fork remote"
git switch "$MAIN_BRANCH"
git merge --ff-only "$FORK_REMOTE/$MAIN_BRANCH"

if git show-ref --verify --quiet "refs/heads/$SYNC_BRANCH"; then
  echo "Sync branch already exists: $SYNC_BRANCH" >&2
  cat > "$SYNC_META_PATH" <<META
SYNC_BRANCH=$SYNC_BRANCH
MAIN_BRANCH=$MAIN_BRANCH
UPSTREAM_SHA=$UPSTREAM_SHA
OLD_MAIN_SHA=$OLD_MAIN_SHA
MERGE_STATUS=branch_exists
REPORT_PATH=$REPORT_PATH
META
  exit 3
fi

echo "==> Creating sync branch: $SYNC_BRANCH"
git switch --create "$SYNC_BRANCH" "$MAIN_BRANCH"

set +e
git merge --no-ff -m "sync: merge $VENDOR_BRANCH into $MAIN_BRANCH ($DATE_TAG)" "$VENDOR_BRANCH"
MERGE_EXIT=$?
set -e

if [ "$MERGE_EXIT" -ne 0 ]; then
  echo "==> Merge conflicts detected"
  if [ -x "$ROOT_DIR/scripts/report-sync-conflicts.sh" ]; then
    "$ROOT_DIR/scripts/report-sync-conflicts.sh" "$REPORT_PATH"
  elif [ -x "$ROOT_DIR/report-sync-conflicts.sh" ]; then
    "$ROOT_DIR/report-sync-conflicts.sh" "$REPORT_PATH"
  else
    printf '# Upstream Sync Conflict Report\n\nNo report generator found.\n' > "$REPORT_PATH"
  fi

  cat > "$SYNC_META_PATH" <<META
SYNC_BRANCH=$SYNC_BRANCH
MAIN_BRANCH=$MAIN_BRANCH
UPSTREAM_SHA=$UPSTREAM_SHA
OLD_MAIN_SHA=$OLD_MAIN_SHA
MERGE_STATUS=conflict
REPORT_PATH=$REPORT_PATH
META
  exit 4
fi

cat > "$SYNC_META_PATH" <<META
SYNC_BRANCH=$SYNC_BRANCH
MAIN_BRANCH=$MAIN_BRANCH
UPSTREAM_SHA=$UPSTREAM_SHA
OLD_MAIN_SHA=$OLD_MAIN_SHA
MERGE_STATUS=clean
REPORT_PATH=$REPORT_PATH
META

echo "==> Sync branch ready: $SYNC_BRANCH"
echo "Metadata written to $SYNC_META_PATH"
