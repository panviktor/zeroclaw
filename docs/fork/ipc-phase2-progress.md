# IPC Phase 2: Hardened Security — Progress

Full plan: [`ipc-phase2-plan.md`](ipc-phase2-plan.md)
Phase 1 plan: [`ipc-plan.md`](ipc-plan.md) | Phase 1 progress: [`ipc-progress.md`](ipc-progress.md)
Base branch: `main`
Working branch: feature branch off `main` (e.g. `feat/ipc-phase2-*`)
Execution owner: `Opus`

## Steps Overview

| # | Step | Files | Status | Depends on |
|---|------|-------|--------|------------|
| 1 | Audit trail: IPC event types + AuditLogger wiring | security/audit.rs, gateway/ipc.rs, gateway/mod.rs | DONE (2026-03-14) | — |
| 2 | PromptGuard: broker payload scanning (block/warn only) | gateway/ipc.rs, config/schema.rs, gateway/mod.rs | DONE (2026-03-14) | 1 |
| 3 | Structured output: trust_warning + quarantine label | gateway/ipc.rs, tools/agents_ipc.rs | DONE (2026-03-14) | — |
| 4 | Credential leak scanning via LeakDetector (send + state_set) | gateway/ipc.rs, gateway/mod.rs | DONE (2026-03-14) | 1 |
| 5 | Sequence integrity check | gateway/ipc.rs | DONE (2026-03-14) | — |
| 6 | Session length limits + provenance-preserving escalation | gateway/ipc.rs, config/schema.rs | DONE (2026-03-14) | 1 |
| 7 | Promote-to-task: quarantine -> provenance-preserving envelope | gateway/ipc.rs, gateway/mod.rs | DONE (2026-03-14) | 1, 3 |
| 8 | Final validation: fmt + clippy + test + docs | — | DONE (2026-03-14) | all |

> **Deferred to Phase 3**: synchronous spawn (`wait_for_result`), PromptGuard sanitize mode, HMAC audit signing, sender-side replay protection. See plan for rationale.

## Session Log

| Date | Session | Steps done | Notes |
|------|---------|------------|-------|
| 2026-03-14 | 1 | 1 | Audit trail: 7 event types, AuditEvent::ipc() builder, wired into all handlers. PR #26 |
| 2026-03-14 | 1 | 2 | PromptGuard: IpcPromptGuardConfig, scan in handle_ipc_send, block/warn, exempt levels. PR #27 |
| 2026-03-14 | 1 | 3 | Structured output: trust_warning + quarantine label in InboxMessage, tool description updated. PR #28 |
| 2026-03-14 | 1 | 4 | LeakDetector: scan send + state_set, audit IpcLeakDetected, 4 tests. PR #29 |
| 2026-03-14 | 1 | 5 | Sequence integrity: per-pair monotonicity check in insert_message, corruption detection, 3 tests. PR #30 |
| 2026-03-14 | 1 | 6 | Session limits: session_max_exchanges + coordinator_agent config, ESCALATION_KIND, session_message_count, 4 tests. PR #31 |
| 2026-03-14 | 1 | 7, 8 | Promote-to-task: promoted column migration, StoredMessage, get_message, insert_promoted_message, handle_admin_ipc_promote, /admin/ipc/promote route, fetch_inbox quarantine queries updated, 7 tests. Final validation: fmt/clippy/test clean, 7296 passed. |
