# Fork Delta Registry: ZeroClaw IPC Fork

## Purpose

This file records the **intentional delta of the fork** relative to upstream.  
It serves three purposes:
- understand exactly what we maintain ourselves
- simplify sync with upstream
- separate fork-only policy from code that can eventually be extracted upstream

Related documents:
- [`ipc-plan.md`](ipc-plan.md) — full implementation plan
- [`ipc-progress.md`](ipc-progress.md) — step-by-step execution tracker
- [`sync-strategy.md`](sync-strategy.md) — fork sync strategy

## Statuses

- `fork-only` — product logic that is not planned to go upstream as a whole
- `candidate-upstream` — a neutral primitive / extension point
- `temporary-backport` — a temporarily backported upstream fix that should disappear after sync

## Merge Risk

- `low` — isolated, conflicts are rare
- `medium` — periodic conflicts in shared files
- `high` — security/gateway/config hotspots; manual review is mandatory

## Current Delta

| ID | Change | Status | Merge risk | Main files | Owner | Notes |
|----|-----------|--------|------------|----------------|-------|-------|
| IPC-001 | Broker-mediated IPC endpoints + SQLite store | `candidate-upstream` | `high` | `src/gateway/ipc.rs`, `src/gateway/mod.rs` | Opus | The base substrate can be split into neutral PRs |
| IPC-002 | Token metadata, IPC eligibility, revoke/disable/downgrade hooks | `candidate-upstream` | `high` | `src/security/pairing.rs`, `src/config/schema.rs`, `src/gateway/ipc.rs` | Opus | Strong candidate for upstream as primitives |
| IPC-003 | Correlated `result` only + session validation | `candidate-upstream` | `medium` | `src/gateway/ipc.rs` | Opus | Useful as a generic safety rule |
| IPC-004 | L0-L4 trust hierarchy and directional ACL matrix | `fork-only` | `high` | `src/gateway/ipc.rs`, `src/config/schema.rs` | Opus | Tightly coupled to the product model |
| IPC-005 | L4 quarantine lane (read-only for execution) | `fork-only` | `high` | `src/gateway/ipc.rs`, tools/inbox behavior, audit events | Opus | Unlikely to go upstream as-is |
| IPC-006 | Sparse mesh lateral policy (`L2↔L2`, `L3↔L3`, allowlisted FYI text) | `fork-only` | `medium` | `src/gateway/ipc.rs`, config allowlists | Opus | Policy-specific; keep it separate from generic transport |
| IPC-007 | Logical destinations for L4 (`supervisor`, `escalation`) | `fork-only` | `medium` | `src/gateway/ipc.rs`, config schema | Opus | Routing abstraction tied to the low-trust model |
| IPC-008 | Approval broker via Opus / control plane / `#approvals` | `fork-only` | `high` | orchestration policy, channel integrations, audit events | Administrator + Opus | Authority boundary; critical not to mix with generic IPC |
| IPC-009 | Structured IPC tracing events | `candidate-upstream` | `medium` | `src/gateway/ipc.rs`, tracing integration | Opus | Neutral observability layer |
| IPC-010 | Agent IPC tools (`agents_list/send/inbox/reply/state/spawn`) | `candidate-upstream` | `medium` | `src/tools/agents_ipc.rs`, `src/tools/mod.rs` | Opus | Some parts may be upstreamable; the policy surface is not |
| IPC-011 | ~~Same-process `agents_spawn`~~ → Subprocess spawn with broker-backed identity | `fork-only` | `high` | `src/tools/agents_ipc.rs`, `src/cron/*` | Opus | Phase 3A: subprocess execution, ephemeral identity, wait/poll |
| IPC-012 | Config masking/encryption for IPC secrets | `candidate-upstream` | `medium` | `src/gateway/api.rs`, `src/config/schema.rs` | Opus | Good generic hardening |
| IPC-013 | Ephemeral identity provisioning + spawn_runs table | `fork-only` | `high` | `src/gateway/ipc.rs`, `src/security/pairing.rs` | Opus | Phase 3A: runtime-only tokens, spawn session tracking, auto-revoke |
| IPC-014 | Child process IPC bootstrap via env vars | `candidate-upstream` | `low` | `src/config/schema.rs`, `src/agent/prompt.rs` | Opus | Env-based IPC auto-config — useful as generic mechanism |
| IPC-015 | Fail-closed execution profiles + workload profiles | `fork-only` | `high` | `src/security/execution.rs`, `src/config/schema.rs` | Opus | Phase 3A: trust-derived sandbox enforcement, no NoopSandbox fallback for L2+ |

## Shared Hotspots

These files should automatically go onto the manual review list for every sync PR:
- `src/config/schema.rs`
- `src/config/mod.rs`
- `src/gateway/mod.rs`
- `src/gateway/api.rs`
- `src/security/pairing.rs`
- `src/tools/mod.rs`
- `src/onboard/wizard.rs`
- `src/security/execution.rs`
- if IPC touches scheduler / channels: `src/cron/*`, `src/channels/*`, `src/agent/*`

## Review Rules

### For `candidate-upstream`

On every sync and every noticeable redesign, ask:
- can this be separated from our trust/policy model?
- can it be extracted into a separate hook, trait, helper, or neutral API?
- can we prepare a small upstream PR instead of growing the fork further?

### For `fork-only`

On every sync, verify:
- has the logic spread further across shared-hotspot files?
- can it be moved behind an overlay/module boundary?
- has a hidden dependency on upstream internals appeared that will make the next merge harder?

## Updating This File

Update this registry when:
- a new shared hotspot is added
- the `fork-only` ↔ `candidate-upstream` status changes
- a temporary backport appears
- the architectural boundary shifts

## Current Conclusion

The main maintenance task is not just “merge more often,” but **systematically reduce the volume of intentional delta**.  
Everything neutral should gradually be extracted into upstream primitives. Everything policy-specific should be tightly isolated and explicitly marked as fork-only.
