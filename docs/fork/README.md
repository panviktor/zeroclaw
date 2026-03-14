# Fork Documentation

This directory contains the strategy, plans, and operational docs for the ZeroClaw fork.

## Documents

| Document | Purpose | Who reads it |
|----------|---------|-------------|
| [sync-strategy.md](sync-strategy.md) | Long-lived fork maintenance: vendor branch, merge-based sync, delta registry | Everyone |
| [delta-registry.md](delta-registry.md) | What is fork-only vs candidate-upstream, merge risk per item | Administrator, Opus |
| [sync-review-rubric.md](sync-review-rubric.md) | Approve / Request changes / Escalate policy for sync PRs | Administrator |
| [ipc-plan.md](ipc-plan.md) | Full IPC design: trust model, ACL, quarantine, approvals, phases | Everyone |
| [ipc-progress.md](ipc-progress.md) | Step-by-step execution checklist (11 steps, Phase 1) | Opus |
| [ipc-quickstart.md](ipc-quickstart.md) | Minimal configs, pairing flow, smoke-test curl commands | Everyone |

## Reading order

**New to the fork?** Start with `ipc-plan.md` → `sync-strategy.md` → `delta-registry.md`.

**Starting IPC work?** Read `ipc-progress.md` first — find the next TODO step, then read the matching section in `ipc-plan.md`.

**Setting up IPC locally?** Follow `ipc-quickstart.md` — configs, pairing, smoke tests.

**Reviewing a sync PR?** Open `sync-review-rubric.md` and `delta-registry.md`.

## Branch model

| Branch | Role | Tracks |
|--------|------|--------|
| `main` | Fork's default branch | `origin/main` |
| `vendor/upstream-master` | Read-only upstream mirror | `upstream/master` |
| `sync/upstream-YYYYMMDD` | Temporary sync PR branch | — |
| `feat/*` | Feature work, branched from `main` | — |

## Automation

- **Weekly sync workflow**: `.github/workflows/upstream-sync.yml`
- **Sync scripts**: `scripts/sync-upstream.sh`, `scripts/report-sync-conflicts.sh`, `scripts/render-sync-pr-body.sh`
- **Templates**: `.github/pull_request_template/sync-pr.md`, `.github/ISSUE_TEMPLATE/upstream-sync-conflict.md`

## Related

- [CLAUDE.md](../../CLAUDE.md) — project-wide coding conventions
- [docs/contributing/](../contributing/) — PR discipline, change playbooks
