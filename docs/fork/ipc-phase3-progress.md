# IPC Phase 3A Progress — Trusted Execution

Plan: [`ipc-phase3-plan.md`](ipc-phase3-plan.md)

## Status: DONE

All 6 implementation steps completed and merged.

## Steps

| Step | Title | PR | Status |
|------|-------|----|--------|
| 0 | Subprocess execution path in scheduler | #40 | Done |
| 1 | Ephemeral identity provisioning | #41 | Done |
| 2 | `agents_spawn` upgrade — wait, timeout, workload | #42 | Done |
| 3 | Child process IPC bootstrap | #43 | Done |
| 4 | Result delivery path + auto-revoke | #44 | Done |
| 5 | Execution profiles — fail-closed sandbox | #45 | Done |
| 6 | Integration wiring + docs | #46 | Done |

## What was built

### Subprocess execution (Step 0)
- `ExecutionMode` enum (`InProcess` | `Subprocess`) on `CronJob`
- `add_agent_job_full()` with `execution_mode` and `env_overlay`
- `run_agent_job_subprocess()` — launches `zeroclaw agent -m` via `tokio::process::Command`

### Ephemeral identity (Step 1)
- `spawn_runs` table in IPC DB (status tracking, result payload, expiry)
- `POST /api/ipc/provision-ephemeral` — generates agent_id, token, session_id
- `GET /api/ipc/spawn-status` — poll spawn session status
- `PairingGuard.register_ephemeral_token()` — runtime-only token (never persisted)
- `revoke_ephemeral_agent()` — token removal + DB status update
- `IpcDb.interrupt_all_ephemeral_spawn_runs()` — broker restart recovery

### agents_spawn upgrade (Step 2)
- `wait` (bool), `timeout` (10-3600s), `workload` (string) parameters
- Broker-backed mode: provision → subprocess → poll with exponential backoff
- Legacy mode preserved when no broker_token configured

### Child bootstrap (Step 3)
- `ZEROCLAW_BROKER_TOKEN` env var → auto-enable IPC in config
- `EphemeralAgentSection` in system prompt — session context + agents_reply instructions

### Result delivery (Step 4)
- `kind=result` with matching `session_id` → complete spawn_run + auto-revoke child
- Message delivered to parent inbox simultaneously

### Execution profiles (Step 5)
- `ExecutionBoundary` — trust-level → sandbox/autonomy/tools mapping
- `require_sandbox()` — fail-closed enforcement (L2-L4 refuse NoopSandbox)
- `WorkloadProfile` — model, tools subset, output limits (can only narrow)
- `apply_workload()` — validate tools ⊆ boundary ceiling

### Integration (Step 6)
- Wire `require_sandbox` + `apply_workload` into `agents_spawn` broker-backed path
- Update `ipc-quickstart.md` with spawn workflow examples
- Update `delta-registry.md` with Phase 3A entries (IPC-013, IPC-014, IPC-015)

## Files touched

| File | Steps |
|------|-------|
| `src/cron/types.rs` | 0 |
| `src/cron/store.rs` | 0 |
| `src/cron/scheduler.rs` | 0, 5 |
| `src/cron/mod.rs` | 0 |
| `src/gateway/ipc.rs` | 1, 4 |
| `src/gateway/mod.rs` | 1 |
| `src/security/pairing.rs` | 1 |
| `src/tools/agents_ipc.rs` | 2, 6 |
| `src/tools/mod.rs` | 2 |
| `src/config/schema.rs` | 3, 5 |
| `src/agent/prompt.rs` | 3 |
| `src/security/execution.rs` | 5 |
| `src/security/mod.rs` | 5 |

## What's next: Phase 3B (deferred)

Phase 3B adds cryptographic provenance — not started, depends on Phase 3A stability.

| Step | Title | Status |
|------|-------|--------|
| 7 | Ed25519 agent identity | Not started |
| 8 | Signed messages | Not started |
| 9 | HMAC audit chain | Not started |
| 10 | Sender-side replay protection | Not started |
