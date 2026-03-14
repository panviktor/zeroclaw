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
| 3 | Structured output: trust_warning + quarantine label | gateway/ipc.rs, tools/agents_ipc.rs | TODO | — |
| 4 | Credential leak scanning via LeakDetector (send + state_set) | gateway/ipc.rs, gateway/mod.rs | TODO | 1 |
| 5 | Sequence integrity check | gateway/ipc.rs | TODO | — |
| 6 | Session length limits + provenance-preserving escalation | gateway/ipc.rs, config/schema.rs | TODO | 1 |
| 7 | Promote-to-task: quarantine -> provenance-preserving envelope | gateway/ipc.rs, gateway/mod.rs | TODO | 1, 3 |
| 8 | Final validation: fmt + clippy + test + docs | — | TODO | all |

> **Deferred to Phase 3**: synchronous spawn (`wait_for_result`), PromptGuard sanitize mode, HMAC audit signing, sender-side replay protection. See plan for rationale.

## Session Log

| Date | Session | Steps done | Notes |
|------|---------|------------|-------|
| 2026-03-14 | 1 | 1 | Audit trail: 7 event types, AuditEvent::ipc() builder, wired into all handlers. PR #26 |
| 2026-03-14 | 1 | 2 | PromptGuard: IpcPromptGuardConfig, scan in handle_ipc_send, block/warn, exempt levels. |
