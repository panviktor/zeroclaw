# Fork Sync Strategy: ZeroClaw IPC Fork

## Purpose

This document describes the **operational strategy for maintaining a long-lived fork**, not the workflow of a single feature branch.

Related documents:
- `ipc-implementation-plan.md` — what exactly we are building
- `ipc-progress.md` — step-by-step execution of the IPC work
- `fork-sync-strategy.md` — how not to drown in upstream sync
- `fork-sync-automation.md` — concrete templates for scripts, workflows, and PR/issue templates
- `sync-pr-review-rubric.md` — review rules for `sync/upstream-*` PRs

**Sync process executor**: Opus  
**Architecture owner**: fork team / Administrator

## Initial Conditions

- upstream evolves quickly; prolonged drift is expensive
- IPC changes several hot areas of the core
- some IPC logic is generic and potentially upstreamable
- some logic is strictly fork-specific:
  - trust matrix L0-L4
  - quarantine lane
  - approval broker through Opus
  - sparse mesh lateral policy

Conclusion: we need not a one-time merge, but a **sustainable process**.

## Core Principles

1. **The fork uses merge-based sync, not rebase-based sync.**
2. **Upstream arrives through a dedicated sync PR, not directly into `main`.**
3. **`fork/main` is the source of truth for the integration history. We do not rewrite it.**
4. **Recurring conflicts should be learned through `git rerere`.**
5. **Without fork invariants CI, a sync cannot be considered safe.**
6. **We must reduce the size of the fork, not only automate conflicts.**

## Remote Name Normalization

Preferred scheme:
- `upstream` — the original repository
- `origin` — our fork

If the local clone uses the reverse names, **scripts must not rely on default names**. Variables must be set explicitly:

```bash
UPSTREAM_REMOTE=upstream
FORK_REMOTE=origin
```

## Branch Model

### Persistent Branches

- `vendor/upstream-master`
  Mirror of `upstream/master`. Read-only branch. Do not commit to it manually.

- `main`
  Main branch of the fork. Releases come from it, it is protected, and we **do not rebase it**.

### Temporary Branches

- `sync/upstream-YYYYMMDD`
  Branch for the next sync PR. Created from `main`; `vendor/upstream-master` is merged into it.

- `feat/*`
  Working branches of the fork. Branched from `main`, not from `vendor/upstream-master`.

- `hotfix/upstream-*`
  Temporary branches for urgent backports of upstream security fixes without waiting for a full sync.

## Why Not Rebase-on-Sync

The old approach, `fork/master = mirror of upstream`, with `feat/*` rebased on every sync, only works when:
- the fork is short-lived
- the changes are small
- there is no published history relied on by other people and CI

That is no longer true in the current situation.

### Reasons to Move to Merge-Based Sync

- the published integration history of the fork cannot be rewritten constantly
- `git rerere` works better on recurring merge conflicts than on endless rebases
- sync PRs are easier to read and discuss separately from feature PRs
- it is easier to keep an audit trail: which upstream range was integrated, when, and with which conflicts
- there is less risk of breaking someone else’s working branches with force-pushes after a rebase

### Where Rebase Is Still Acceptable

Only on **personal unpublished** feature branches before opening a PR.  
Do not rebase:
- `main`
- `vendor/upstream-master`
- `sync/*`
- already published shared branches

## Merge Workflow

### Daily Automation Loop

1. `git fetch upstream --prune`
2. fast-forward `vendor/upstream-master` to `upstream/master`
3. save metadata: upstream HEAD, date, commit range
4. if today matches the sync cadence or there is a security fix, open/update `sync/upstream-YYYYMMDD`

### Sync PR Workflow

1. create `sync/upstream-YYYYMMDD` from `main`
2. merge `vendor/upstream-master` into `sync/...`
3. if the merge is clean:
   - push the branch
   - open a PR into `main`
   - run CI
4. if there are conflicts:
   - push a draft PR or create an issue
   - attach a conflict report
   - assign owners of hotspot files
5. after review and green CI, merge the PR into `main`

## Cadence

- **Daily**: fetch + update vendor branch
- **Weekly**: mandatory sync PR, even if “nothing urgent is happening”
- **Immediate**: separate hotfix path for upstream security fixes
- **Max drift budget**: no more than 7-10 days

If drift exceeds 10 days, treat it as a process incident, not a “normal delay.”

## Automation: What Exactly Must Be Built

### 1. Persistent `git rerere`

Must be enabled:

```bash
git config rerere.enabled true
git config rerere.autoupdate true
```

Requirement: the `rerere` cache must survive between sync runs instead of being lost on every machine or CI job.

### 2. Scheduled Sync Job

The job must be able to:
- update `vendor/upstream-master`
- create or update `sync/upstream-YYYYMMDD`
- attempt the merge
- if the merge fails, generate a conflict report
- if the merge succeeds, open the sync PR automatically

### 3. Conflict Report

The automated report must include:
- upstream range: `old_vendor..new_vendor`
- the list of conflicting files
- file classification:
  - `fork-owned`
  - `shared-hotspot`
  - `upstream-owned`
- suggested reviewers / owners
- required test subset
- flag: `security-sensitive = yes/no`

### 4. Fork Invariants CI

Every sync PR needs a separate set of checks in addition to normal CI:
- IPC ACL invariants
- correlated `result` semantics
- legacy tokens = no IPC
- quarantine = read-only for execution
- approval routing through Opus/control plane
- revoke / disable / downgrade / quarantine semantics
- lateral messaging restrictions
- no bypass through channel auto-approve for IPC-originated dangerous actions

A clean merge guarantees nothing without these checks.

## Path Ownership

### Fork-Owned Paths

Here an “ours-first” mode is usually acceptable because these are our overlay modules:
- `src/gateway/ipc.rs`
- `src/tools/agents_ipc.rs`
- fork-specific docs in `~/logs/`

### Shared Hotspots

These always require manual review:
- `src/config/schema.rs`
- `src/config/mod.rs`
- `src/gateway/mod.rs`
- `src/gateway/api.rs`
- `src/security/pairing.rs`
- `src/tools/mod.rs`
- `src/onboard/wizard.rs`
- potentially later: `src/cron/*`, `src/agent/*`, `src/channels/*`

### Upstream-Owned

All other paths that the fork does not touch. The default strategy there is to accept upstream as the base.

## Merge Policy by File Class

| Class | Policy | Who reviews |
|------|----------|-------------|
| `fork-owned` | Preserve fork behavior; only verify compile/test impact | Opus |
| `shared-hotspot` | Manual merge; validate architectural invariants | Opus + architecture review |
| `upstream-owned` | Prefer upstream with no local modifications | Opus |

## Delta Registry

A separate artifact already exists: `fork-delta.md`. It lists **all intentional delta** of the fork. Minimal format:
- change
- why it exists
- fork-only or candidate for upstream
- affected files
- merge risk (`low` / `medium` / `high`)
