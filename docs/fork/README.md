# Fork Documentation

This directory contains the strategy, plans, and operational docs for the ZeroClaw fork.

## Documents

| Document | Purpose | Who reads it |
|----------|---------|-------------|
| [sync-strategy.md](sync-strategy.md) | Long-lived fork maintenance: vendor branch, merge-based sync, delta registry | Everyone |
| [delta-registry.md](delta-registry.md) | What is fork-only vs candidate-upstream, merge risk per item | Administrator, Opus |
| [sync-review-rubric.md](sync-review-rubric.md) | Approve / Request changes / Escalate policy for sync PRs | Administrator |
| [ipc-plan.md](ipc-plan.md) | Full IPC design: trust model, ACL, quarantine, approvals, phases | Everyone |
| [ipc-progress.md](ipc-progress.md) | Step-by-step execution checklist (11 steps, Phase 1 — DONE) | Opus |
| [ipc-phase2-plan.md](ipc-phase2-plan.md) | Phase 2: Hardened Security — PromptGuard, structured output, audit, replay, session limits | Everyone |
| [ipc-phase2-progress.md](ipc-phase2-progress.md) | Phase 2 execution checklist (8 steps) | Opus |
| [ipc-quickstart.md](ipc-quickstart.md) | Minimal configs, pairing flow, smoke-test curl commands | Everyone |
| [ipc-phase3-plan.md](ipc-phase3-plan.md) | Phase 3: Trusted Execution — ephemeral agents, fail-closed sandbox, subprocess isolation, crypto provenance | Everyone |
| [ipc-phase3-progress.md](ipc-phase3-progress.md) | Phase 3A/3B execution checklist (all steps — DONE) | Opus |
| [ipc-phase3_5-plan.md](ipc-phase3_5-plan.md) | Phase 3.5: Human Control Plane — IPC operator UI (6 screens, 10 steps) | Everyone |
| [ipc-phase3_6-plan.md](ipc-phase3_6-plan.md) | Phase 3.6: Agent Provisioning — add agents from UI, 5 presets, config generator, pairing flow | Everyone |

## Reading order

**New to the fork?** Start with `ipc-plan.md` → `sync-strategy.md` → `delta-registry.md`.

**Starting IPC work?** Phases 1-3.5 are complete. Next up: `ipc-phase3_6-plan.md` — agent provisioning UI with presets and config generator.

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
