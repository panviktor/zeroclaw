# IPC Phase 3: Trusted Execution

Phase 1: brokered coordination | Phase 2: broker-side safety | **Phase 3: trusted execution**

Phase 1 plan: [`ipc-plan.md`](ipc-plan.md) | Phase 2 plan: [`ipc-phase2-plan.md`](ipc-phase2-plan.md)

---

## What Phase 3 gives

Three promises to the operator:

1. **Child = separate actor** — spawn a disposable worker with its own identity, inbox, and lifecycle.
2. **Trust level = real runtime boundary** — policy enforcement backed by OS-level isolation, not just ACL checks. **Fail-closed**: if the required sandbox cannot be applied, spawn refuses to start.
3. **Every message/result = provenance-preserving** — broker is the root of trust for transport auth (Phase 3A); agents hold their own signing keys for message-level provenance (Phase 3B).

---

## Phase 3A vs 3B

| | Phase 3A: Usable Isolation | Phase 3B: Hardening |
|--|---|---|
| **Goal** | Make `agents_spawn` a real product | Make the runtime cryptographically verifiable |
| **User sees** | spawn → wait → result → revoke | Signed messages, tamper-evident audit, replay rejection |
| **Prerequisite** | Phase 2 (done) | Phase 3A |
| **Risk** | Medium — new plumbing, existing infrastructure | High — crypto primitives, key management |

Phase 3A ships first. Phase 3B is only valuable after 3A works.

---

## Architectural Decisions

### AD-1: Spawn control plane — agent-local runtime + broker-issued identity

Child processes run **on the parent's host** as **separate OS processes**, launched by the parent's local scheduler. The broker's role is limited to **identity provisioning and lifecycle management** — it issues ephemeral tokens, tracks session state, and handles revocation. The broker does not launch processes.

**Breaking change from current scheduler**: the current `cron` scheduler runs agent jobs **in-process** via `crate::agent::run(config.clone(), ...)` (`scheduler.rs:175`). This means the child shares the parent's process, memory, and security context — it is not a "separate actor" in any meaningful sense. Phase 3A requires the scheduler to gain a **subprocess execution path**: `std::process::Command` / `tokio::process::Command` that launches `zeroclaw agent -m "..."` (or a dedicated `zeroclaw ephemeral` subcommand) as a separate OS process with its own PID, environment, and sandbox wrapping. The in-process path remains as legacy for `wait=false` fire-and-forget jobs without broker identity.

**Why**: `agents_spawn` is already a local tool on top of `cron` (`agents_ipc.rs:692`). The distributed model from Phase 1 (`ipc-plan.md:15`) has agents on different hosts connecting to a shared broker. Broker-side compute would force all children onto the broker host, breaking this model. Instead:

```
Parent host                                Broker host
┌─────────────────────┐                    ┌─────────────────────┐
│ Parent agent         │                    │ Gateway (IPC broker) │
│  ├─ agents_spawn()   │                    │  ├─ provision_eph()  │
│  │   └─ cron job ────┼── HTTP ───────────>│  │   └─ token+id     │
│  │                   │<───────────────────┼──┘                   │
│  └─ child process    │                    │                      │
│      ├─ sandbox      │                    │ agents.db            │
│      ├─ env vars     │                    │  ├─ agents table     │
│      └─ IPC tools ───┼── HTTP ───────────>│  ├─ messages table   │
│                      │                    │  └─ spawn_runs table │
└─────────────────────┘                    └─────────────────────┘
```

**Consequence**: no `POST /admin/ipc/spawn` endpoint. Spawn is always an agent operation via `agents_spawn` tool. Admin can only revoke/inspect/quarantine.

### AD-2: Sandbox enforcement is fail-closed

For trust levels L2-L4, spawn **refuses to start** if the required sandbox backend is not available on the host. There is no fallback to `NoopSandbox` for profile-required isolation.

**Rules**:
1. L0-L1 (`coordinator`): sandbox optional — `NoopSandbox` allowed
2. L2 (`privileged`): Landlock required. If unavailable → spawn error
3. L3 (`worker`): Landlock + Bubblewrap required. If unavailable → spawn error
4. L4 (`restricted`): Bubblewrap or Docker required. If unavailable → spawn error
5. Fallback is only allowed toward **stricter** isolation (e.g. Docker instead of Bubblewrap), never toward weaker

**Why**: current `detect_best_sandbox()` (`detect.rs:67`) silently falls back to `NoopSandbox` when no backend is available. With that behavior, "trust level = real runtime boundary" would be a lie. Phase 3A adds `require_sandbox_for_profile()` that returns `Err` instead of falling back to noop.

**Target platform**: Phase 3A sandbox enforcement targets **Linux hosts**. Landlock requires Linux 5.13+, Bubblewrap requires Linux user namespaces. On non-Linux platforms (macOS, Windows), only Docker backend is available for L2-L4 isolation. If Docker is also unavailable, L2-L4 spawn is denied. L0-L1 works everywhere (NoopSandbox allowed). This is an intentional operational constraint — the family multi-agent system runs on Linux servers.

| Platform | L0-L1 | L2 | L3 | L4 |
|----------|-------|----|----|----|
| Linux 5.13+ | Noop/any | Landlock | Landlock + Bubblewrap | Bubblewrap or Docker |
| Linux < 5.13 | Noop/any | Docker only | Docker only | Docker only |
| macOS | Noop/any | Docker only | Docker only | Docker only |
| Windows | Noop/any | Docker only | Docker only | Docker only |

### AD-3: Execution profile = trust-derived + workload overlay

Two separate concepts:

- **Execution boundary** — derived solely from `effective_trust_level`. Controls sandbox, filesystem, network, shell, autonomy. **Cannot be weakened** by any config or parameter.
- **Workload profile** — optional, user-facing. Controls model, prompt template, tool allowlist (can only **narrow**, not widen). Specified via `workload: "research"` parameter in `agents_spawn`.

```
effective_trust_level (from token_metadata)
        │
        ▼
┌───────────────────┐
│ Execution boundary │  ← sandbox, FS, network, shell — IMMUTABLE per trust
│ (fail-closed)      │
└────────┬──────────┘
         │
         ▼
┌───────────────────┐
│ Workload profile   │  ← model, prompt, tool subset — can only NARROW
│ (optional overlay) │
└───────────────────┘
```

**Rule**: `workload_profile.allowed_tools ⊆ execution_boundary.allowed_tools`. If a workload profile tries to grant a tool that the execution boundary forbids, spawn returns an error.

**Why**: a single `profile` field that mixes trust boundary with workload config lets an agent request a softer sandbox than its trust level permits. Splitting them makes the invariant explicit: trust controls the cage, workload controls what happens inside.

### AD-4: Result delivery via `spawn_runs` table (not inbox polling)

`agents_spawn(wait=true)` waits on a dedicated `spawn_runs` table, not on the general inbox.

```sql
CREATE TABLE spawn_runs (
    id          TEXT PRIMARY KEY,     -- same as session_id
    parent_id   TEXT NOT NULL,        -- parent agent_id
    child_id    TEXT NOT NULL,        -- ephemeral agent_id
    status      TEXT NOT NULL DEFAULT 'running',  -- running|completed|timeout|revoked|error
    result      TEXT,                 -- payload from child's kind=result
    created_at  INTEGER NOT NULL,
    expires_at  INTEGER NOT NULL,
    completed_at INTEGER
);
```

**Flow**:
1. `agents_spawn(wait=true)` creates a `spawn_runs` row with `status=running`
2. Child sends `kind=result` via normal IPC → broker inserts into parent's inbox AND updates `spawn_runs.status=completed, .result=payload`
3. `agents_spawn` polls `spawn_runs` by session_id (not inbox) with exponential backoff (100ms → 200ms → ... → 5s cap)
4. Timeout: broker sets `status=timeout`, revokes child
5. On parent restart: broker detects stale `running` rows with `expires_at < now`, transitions to `status=interrupted`

**Why**: inbox has consumptive read semantics (`read=1` on fetch). Polling inbox would either consume messages meant for other purposes, or require a separate peek mode. `spawn_runs` is purpose-built for this wait pattern and survives restart cleanly.

### AD-5: Ephemeral identities are runtime-only

Ephemeral tokens and metadata live **only in broker runtime state** (in-memory `paired_tokens` / `token_metadata` + IPC SQLite `agents` table). They are **never written to persistent config** (`config.toml`, `gateway.paired_tokens` on disk).

**Restart semantics**:
- On broker restart, all in-memory ephemeral tokens are lost
- Broker scans `agents` table for `status = "ephemeral"` rows, transitions them to `status = "interrupted"`
- Associated `spawn_runs` rows transition to `status = "interrupted"`
- Parent's next poll of `spawn_runs` sees `interrupted` → returns error to parent
- Child processes that are still running will get 401 on next IPC call → exit gracefully

**Why**: current pairing model (`pairing.rs:49, pairing.rs:266`) is designed for long-lived config-backed identities. Writing ephemeral tokens there would cause churn and garbage accumulation. Runtime-only ephemerals keep the persistent config clean.

### AD-6: Broker is root of trust for transport; agents own signing keys

Phase 3A and 3B have different trust models:

**Phase 3A** — broker is the sole root of trust:
- Bearer token proves identity (transport-level)
- Broker resolves agent_id + trust_level from token
- No message-level signatures
- Sufficient for single-broker deployments where the broker is trusted infrastructure

**Phase 3B** — agents hold their own signing keys:
- Persistent agents generate Ed25519 keypair **locally** on first pairing
- Agent sends public key to broker during pairing; broker stores it in `agents` table
- Broker **never sees** the agent's private key
- Ephemeral children generate their own keypair on startup, register public key via a one-time `POST /api/ipc/register-key` call (authenticated by ephemeral bearer token)
- Broker issues a short-lived **delegation certificate**: `{child_agent_id, child_pubkey, parent_agent_id, expires_at}` signed by broker's own key
- Message signature proves: "this payload was created by the runtime holding this private key"
- Delegation certificate proves: "this ephemeral identity was authorized by this parent via this broker"

**Why**: if broker generates and holds the child's private key, signatures only prove "broker could have signed this", not "child runtime actually produced this". Agent-generated keys with broker-signed delegation certificates give real non-repudiation.

---

## User Stories

### Phase 3A

1. **"Opus can spawn a disposable worker and wait for its result."**
   - Operator action: Opus calls `agents_spawn(prompt="...", wait=true, timeout=300)`
   - System behavior: parent requests ephemeral identity from broker → parent launches child locally with sandbox + token → child runs → sends result → identity auto-revoked
   - Security property: child cannot outlive its session; stale tokens are garbage-collected

2. **"Restricted agents run in a real sandbox, not trust-by-convention."**
   - Operator action: set `trust_level = 4` in pairing config
   - System behavior: child process gets execution boundary matching its trust level (L4 = strictest). If required sandbox backend is unavailable, **spawn fails** instead of running without isolation
   - Security property: L4 agent physically cannot write outside `/tmp`, execute network calls, or escalate privileges. This is an OS-level guarantee, not a policy suggestion

3. **"An operator can revoke an agent and know its future messages are rejected."**
   - Operator action: `POST /admin/ipc/revoke {agent_id: "..."}` or child session ends
   - System behavior: token removed from runtime state, pending messages blocked, future auth attempts fail
   - Security property: revocation is immediate and irrecoverable (already implemented, now covers ephemeral children)

### Phase 3B

4. **"An incident reviewer can verify who sent what and whether logs were tampered with."**
   - Operator action: `zeroclaw audit verify --since 2026-03-14`
   - System behavior: HMAC chain validated, signature on each message verified against agent's registered public key
   - Security property: any modification or deletion of audit entries is detectable; message authorship is cryptographically attributable to the signing runtime

---

## Primary Workflow

```
Parent host                                Broker host
─────────                                ───────────
Parent (Opus, L1)                          Broker                               Child (ephemeral, L3)
      │                                      │                                       │
      │ agents_spawn(                        │                                       │
      │   workload="research",               │                                       │
      │   prompt="analyze X",                │                                       │
      │   wait=true,                         │                                       │
      │   timeout=300)                       │                                       │
      │                                      │                                       │
      │ POST /api/ipc/provision-ephemeral    │                                       │
      │─────────────────────────────────────>│                                       │
      │                                      │ 1. Generate ephemeral agent_id        │
      │                                      │ 2. Create runtime-only bearer token   │
      │                                      │ 3. Register in agents table           │
      │                                      │ 4. Resolve execution boundary for L3  │
      │                                      │ 5. Create session_id + spawn_run row  │
      │  { token, agent_id, session_id }     │                                       │
      │<─────────────────────────────────────│                                       │
      │                                      │                                       │
      │ 6. Verify sandbox available (fail-closed)                                    │
      │ 7. Launch child via cron scheduler   │                                       │
      │    (local process, sandboxed)        │                                       │
      │─────────────────────────────────────────────────────────────────────────────>│
      │                                      │                                       │
      │                                      │    (child runs with sandbox + token)   │
      │                                      │                                       │
      │                                      │ agents_send(kind=result,              │
      │                                      │   session_id=...,                     │
      │                                      │   payload="findings...")              │
      │                                      │<──────────────────────────────────────│
      │                                      │                                       │
      │                                      │ 8. Validate result (ACL + scan)       │
      │                                      │ 9. Insert into parent's inbox         │
      │                                      │ 10. Update spawn_run = completed      │
      │                                      │ 11. Revoke ephemeral token (runtime)  │
      │                                      │                                       │
      │ poll spawn_runs → completed          │                                       │
      │<─────────────────────────────────────│                                       │
      │                                      │                                       │
      │ return { result: "findings..." }     │                                       │
```

---

## Execution Profiles

### Execution boundary (trust-derived, immutable)

Trust level determines what the child process can actually do at the OS level. **Cannot be weakened by workload profile or config.**

| Boundary | Trust | Sandbox (fail-closed) | Filesystem | Network | Shell | Autonomy |
|----------|-------|-----------------------|------------|---------|-------|----------|
| `coordinator` | L0-L1 | Optional (noop allowed) | Full workspace | Full | Unrestricted | Full |
| `privileged` | L2 | Landlock required | Workspace + allowed_roots | Full | Classified commands | Supervised |
| `worker` | L3 | Landlock + Bubblewrap required | Workspace only, no dotdirs | Outbound only | Medium-risk max | Supervised |
| `restricted` | L4 | Bubblewrap or Docker required | `/tmp` + read-only workspace | None | Denied | ReadOnly |

### Workload profile (optional overlay, can only narrow)

| Field | What it controls | Constraint |
|-------|------------------|------------|
| `model` | LLM model for child | No constraint |
| `prompt_template` | System prompt prefix | No constraint |
| `allowed_tools` | Tool subset available to child | Must be ⊆ execution boundary tools |
| `max_output_tokens` | Response size limit | No constraint |

```toml
# Example: custom workload profiles in config
[workload_profiles.research]
model = "claude-sonnet-4-6"
allowed_tools = ["web_search", "web_fetch", "memory_read"]

[workload_profiles.code_review]
model = "claude-opus-4-6"
allowed_tools = ["shell", "file_read", "file_write"]
```

### Profile selection flow

```
effective_trust_level (from token_metadata)
        │
        ▼
  ┌──────────────────────┐
  │ execution_boundary()  │──── returns (SandboxConfig, AutonomyConfig, tools_ceiling)
  └──────────┬───────────┘
             │
             ▼
  ┌──────────────────────┐
  │ require_sandbox()     │──── fail-closed: Err if backend unavailable for L2-L4
  └──────────┬───────────┘
             │
             ▼
  ┌──────────────────────┐
  │ apply_workload()      │──── intersect tools, apply model/prompt — can only narrow
  └──────────┬───────────┘
             │
             ▼
        spawn child
```

Implementation reuses existing infrastructure:
- `src/security/detect.rs::create_sandbox()` — backend selection
- `src/security/traits.rs::Sandbox` trait — command wrapping
- `src/security/policy.rs::SecurityPolicy` — autonomy level, command classification, path validation
- `src/runtime/traits.rs::RuntimeAdapter` — native vs docker command building

What's new:
- `execution_boundary(trust_level) → (SandboxConfig, AutonomyConfig, Vec<String>)` — hard-coded per trust level, not configurable
- `require_sandbox(boundary, available_backends) → Result<Arc<dyn Sandbox>, SpawnError>` — fail-closed enforcement
- `apply_workload(boundary, workload_profile) → Result<SpawnConfig, SpawnError>` — narrowing overlay

---

## Identity Model

### Persistent agents (existing)

Agents paired manually via `/admin/paircode/new` → `/pair`. Long-lived bearer token stored in config. Agent_id, trust_level, role in `token_metadata`.

No changes needed.

### Ephemeral agents (Phase 3A — new)

Created by `agents_spawn`. Lifecycle bound to a single session. **Runtime-only** — never written to persistent config.

```
Provisioning (broker side, via POST /api/ipc/provision-ephemeral):
  1. Parent sends: { trust_level, workload, timeout }
  2. Broker generates:
     - agent_id: "eph-{parent_id}-{uuid_short}" (8 char suffix)
     - bearer_token: random 32 bytes → hex
     - session_id: uuid
  3. Broker registers in IPC DB (agents table):
     - trust_level = max(parent_level, requested_level)
     - role = workload name
     - status = "ephemeral"
     - metadata.parent = parent_agent_id
     - metadata.session_id = session_id
     - metadata.expires_at = now + timeout
  4. Broker inserts token into runtime-only paired_tokens + token_metadata
     (NOT persisted to config — lives only in memory + IPC DB)
  5. Broker creates spawn_runs row: { session_id, parent_id, child_id, status=running }
  6. Broker returns: { agent_id, token, session_id }

Launching (parent side, local):
  7. Parent verifies sandbox available for trust_level (fail-closed)
  8. Parent launches child via cron::add_agent_job() with env vars:
     - ZEROCLAW_BROKER_URL, ZEROCLAW_BROKER_TOKEN, ZEROCLAW_AGENT_ID
     - ZEROCLAW_SESSION_ID, ZEROCLAW_REPLY_TO
  9. Parent polls spawn_runs by session_id (if wait=true)

Teardown:
  1. Child sends kind=result with session_id → broker delivers to parent inbox
     AND updates spawn_runs.status=completed, .result=payload
  2. Broker revokes ephemeral token (remove from runtime paired_tokens + token_metadata)
  3. Broker sets agent status = "completed" in IPC DB

  OR: timeout fires → broker sets spawn_runs.status=timeout, revokes token
  OR: operator calls /admin/ipc/revoke → immediate teardown, spawn_runs.status=revoked
  OR: broker restarts → ephemeral tokens lost from memory, agents table rows
      transitioned to "interrupted", spawn_runs.status=interrupted
```

### Token lifecycle

```
                    ┌─────────────┐
                    │  generated   │
                    └──────┬──────┘
                           │ provision-ephemeral (runtime-only)
                           ▼
                    ┌─────────────┐
        ┌──────────│   active     │──────────┐──────────┐
        │          └──────┬──────┘           │          │
        │ revoke          │ session end       │ timeout   │ broker restart
        ▼                 ▼                   ▼          ▼
  ┌──────────┐    ┌─────────────┐    ┌────────────┐  ┌──────────────┐
  │  revoked  │    │  completed   │    │  expired   │  │ interrupted  │
  └──────────┘    └─────────────┘    └────────────┘  └──────────────┘
```

All terminal states: token removed from runtime `paired_tokens`, future requests → 401. IPC DB `agents` row preserved for audit trail.

---

## Operator Operations

### For operators (admin endpoints, localhost only)

| Operation | Endpoint | Phase |
|-----------|----------|-------|
| Wait / inspect | `GET /admin/ipc/agents` (shows ephemeral children + status) | 3A |
| Promote result | `POST /admin/ipc/promote` (existing) | Done |
| Revoke / kill | `POST /admin/ipc/revoke` (existing, extended for ephemeral) | 3A |
| Quarantine / downgrade | `POST /admin/ipc/quarantine`, `/downgrade` (existing) | Done |
| Show provenance | `zeroclaw audit verify` CLI | 3B |

### For agents (IPC tools)

| Operation | Tool | Phase |
|-----------|------|-------|
| Spawn child and wait | `agents_spawn(workload=..., wait=true, timeout=...)` | 3A |
| Reply with result | `agents_reply(session_id=..., payload=...)` (existing) | Done |
| Write shared state | `state_set(key=..., value=...)` (existing) | Done |
| Request approval | Route through Opus via `agents_send(kind=query)` (existing pattern) | Done |

The agent knows nothing about Unix users, sandbox backends, or signing keys. Those are implementation details under `agents_spawn`.

**Broker requirement**: `wait=true` and ephemeral identity provisioning require `broker_url` + `broker_token` in the agent's config (broker-backed mode). Without broker connectivity, `agents_spawn` falls back to the legacy fire-and-forget local path (`wait=false` only, no ephemeral identity, no result delivery, no auto-revoke). The quickstart doc (`ipc-quickstart.md`) describes `agents_spawn` as a local operation — this remains true for the legacy path, but the Phase 3A workflow requires broker-backed mode.

---

## Phase 3A: Implementation Steps

### Step 0: Subprocess execution path in scheduler

**Files**: `src/cron/scheduler.rs`, `src/cron/store.rs`

**Prerequisite for all other steps.** The current scheduler runs agent jobs in-process via `crate::agent::run(config.clone(), ...)` (`scheduler.rs:175`). This must be extended with a subprocess path for broker-backed ephemeral agents.

- Add `ExecutionMode` enum to `CronJob`: `InProcess` (current default) | `Subprocess`
- `Subprocess` mode: launch child via `tokio::process::Command`:
  - Binary: `zeroclaw agent -m "{prompt}"` (or dedicated `zeroclaw ephemeral` subcommand)
  - Env vars from `env_overlay` (ZEROCLAW_BROKER_TOKEN, ZEROCLAW_AGENT_ID, etc.)
  - Sandbox wrapping via `sandbox.wrap_command()` (existing `Sandbox` trait)
  - Working directory: workspace-only for L3+
  - Timeout: kill child process on expiry
  - Capture exit code + stdout for audit
- `InProcess` mode: unchanged, used for legacy `wait=false` jobs without broker identity
- `add_agent_job()` extended with `execution_mode` and `env_overlay` parameters

This is the foundation that makes "child = separate actor" a real OS-level guarantee.

### Step 1: Ephemeral identity provisioning

**Files**: `src/gateway/ipc.rs`, `src/security/pairing.rs`

- Add `POST /api/ipc/provision-ephemeral` endpoint (bearer auth, parent must be L0-L3):
  - Generate agent_id, token, session_id
  - Insert into IPC DB `agents` table with `status = "ephemeral"`, expiry metadata
  - Insert into **runtime-only** `paired_tokens` / `token_metadata` (not persisted to config)
  - Create `spawn_runs` row with `status = "running"`
  - Return `{ agent_id, token, session_id }`
- Add `revoke_ephemeral_agent()`:
  - Remove token from runtime state, set status in IPC DB
  - Update `spawn_runs` row
  - Block pending messages (existing `block_pending_messages`)
- Add `spawn_runs` table to IPC DB schema
- Broker restart recovery: scan `agents` table for `status = "ephemeral"`, transition to `"interrupted"`

### Step 2: `agents_spawn` upgrade — session binding + wait

**Files**: `src/tools/agents_ipc.rs`, `src/cron/store.rs`

- Extend `agents_spawn` tool schema:
  - `wait: bool` (default: false, backward compat)
  - `timeout: u32` (seconds, default: 300)
  - `workload: string` (workload profile name, optional — narrows tools/model, cannot weaken sandbox)
- Spawn flow:
  1. Call `POST /api/ipc/provision-ephemeral` → get token + agent_id + session_id
  2. Resolve execution boundary from child's trust_level
  3. Verify sandbox available (fail-closed: `require_sandbox()`)
  4. Call `add_agent_job()` with env var overlay (ZEROCLAW_BROKER_TOKEN, etc.)
- Wait mode:
  - Poll `GET /api/ipc/spawn-status?session_id=...` with exponential backoff (100ms → 5s cap)
  - Endpoint reads `spawn_runs` table (not inbox)
  - On `completed`: return result payload
  - On `timeout`/`revoked`/`interrupted`/`error`: return error
- Extend `add_agent_job()`:
  - Accept `env_overlay: HashMap<String, String>` for passing broker credentials to child

### Step 3: Child process IPC bootstrap

**Files**: `src/agent/agent.rs` or `src/main.rs`

- On startup, if `ZEROCLAW_BROKER_TOKEN` env var is set:
  - Override `agents_ipc` config with env values
  - Set `agents_ipc.enabled = true`
  - Agent can immediately use IPC tools (agents_reply, state_set)
- Child system prompt injection:
  - Prepend session context: "You are an ephemeral agent spawned by {parent}. Reply with `agents_reply` when done."
  - Include `session_id` and `reply_to` in system prompt

### Step 4: Result delivery path

**Files**: `src/gateway/ipc.rs`, `src/tools/agents_ipc.rs`

- Child sends `agents_reply(payload=...)` → broker validates → inserts into parent inbox
- Broker simultaneously updates `spawn_runs`: `status = "completed"`, `result = payload`, `completed_at = now`
- On successful result delivery:
  - Broker auto-revokes ephemeral child token (runtime-only removal)
  - Broker sets agent status = "completed" in IPC DB
  - Audit: `IpcSend` + `IpcAdminAction(auto_revoke)`
- On timeout:
  - Broker sets `spawn_runs.status = "timeout"`, revokes token
  - Parent's next poll sees timeout → returns error
  - Audit: `IpcAdminAction(timeout_revoke)`
- On broker restart:
  - Stale `running` rows with `expires_at < now` → `status = "interrupted"`
  - Parent's next poll sees interrupted → returns error, can re-spawn

### Step 5: Execution profiles — fail-closed sandbox enforcement

**Files**: `src/config/schema.rs`, `src/security/detect.rs`, `src/tools/agents_ipc.rs`

- Add `execution_boundary(trust_level: u8) -> ExecutionBoundary`:
  - Hard-coded mapping, not configurable
  - Returns required sandbox backend(s), autonomy level, tools ceiling, FS/network policy
- Add `require_sandbox(boundary, host_backends) -> Result<Arc<dyn Sandbox>, SpawnError>`:
  - For L2-L4: if required backend unavailable → `Err(SpawnError::SandboxUnavailable)`
  - Fallback only toward stricter isolation (Docker instead of Bubblewrap = ok; Noop instead of anything = error)
- Add optional `[workload_profiles]` config section:
  ```rust
  pub struct WorkloadProfile {
      pub model: Option<String>,
      pub prompt_template: Option<String>,
      pub allowed_tools: Option<Vec<String>>,  // must be ⊆ boundary.tools_ceiling
      pub max_output_tokens: Option<u32>,
  }
  ```
- `apply_workload(boundary, workload) -> Result<SpawnConfig, SpawnError>`:
  - Intersects workload.allowed_tools with boundary.tools_ceiling
  - If workload requests tools outside ceiling → `Err(SpawnError::ToolNotAllowed)`

### Step 6: Integration tests + docs

- Test: spawn ephemeral → send result → verify delivery → verify revoke
- Test: spawn with timeout → verify timeout revoke
- Test: L4 spawn gets restricted boundary, fail-closed if no sandbox
- Test: ephemeral agent cannot outlive session
- Test: workload profile cannot widen execution boundary
- Test: broker restart → ephemeral sessions transition to interrupted
- Update `ipc-quickstart.md` with spawn workflow examples
- Update `delta-registry.md`

---

## Phase 3B: Implementation Steps

### Step 7: Ed25519 agent identity

**Files**: `src/security/identity.rs` (new), `src/gateway/ipc.rs`, `src/config/schema.rs`

- Persistent agents generate Ed25519 keypair **locally** on first pairing
- Agent sends public key to broker during pairing; broker stores it in `agents` table
- Broker **never sees** the agent's private key
- Ephemeral children generate their own keypair on startup, register public key via `POST /api/ipc/register-key` (authenticated by ephemeral bearer token)
- Broker issues a short-lived **delegation certificate**: `{child_agent_id, child_pubkey, parent_agent_id, expires_at}` signed by broker's own Ed25519 key
- Bearer token remains for transport auth; Ed25519 is for message-level signing

### Step 8: Signed messages

**Files**: `src/gateway/ipc.rs`, `src/tools/agents_ipc.rs`

- Agent signs `{from_agent}|{to_agent}|{seq}|{payload_hash}` with its own Ed25519 private key
- Signature sent as header or JSON field in `POST /api/ipc/send`
- Broker verifies signature against registered public key before INSERT
- For ephemeral agents: broker also verifies delegation certificate is valid and not expired
- Invalid signature → `IpcBlocked` audit event, 403 response

### Step 9: HMAC audit chain

**Files**: `src/security/audit.rs`

- Each audit event includes HMAC-SHA256 over `{prev_hmac}|{event_json}`
- Per-instance HMAC key (generated on first run, stored in `~/.zeroclaw/audit.key`)
- `zeroclaw audit verify` CLI command: reads audit.log, recomputes chain, reports breaks
- Tampered or deleted entries → chain break detected

### Step 10: Sender-side replay protection

**Files**: `src/tools/agents_ipc.rs`, `src/gateway/ipc.rs`

- Agent maintains local sequence counter (persisted in agent's config dir)
- Agent signs `{agent_id}|{seq}|{timestamp}` with Ed25519 key
- Broker verifies: seq > last_seen_seq for this agent, signature valid, timestamp within 5-min window
- Replayed message → reject with `sequence_violation`, audit `IpcBlocked`

---

## Attack Scenario: Phase 3A

```
ATTACK: Compromised L3 worker spawns a privileged child

1. Worker (L3) calls agents_spawn(trust_level=1, prompt="rm -rf /")

   Phase 3A response:
   - Trust propagation: child_level = max(3, 1) = 3
   - Execution boundary for L3: Bubblewrap sandbox, workspace-only FS
   - If Bubblewrap unavailable on host: spawn REFUSED (fail-closed)
   - If available: child runs sandboxed, cannot access /home
   - Result: attack contained at L3 sandbox boundary

2. Worker spawns L3 child, child tries to impersonate Opus

   Phase 3A response:
   - Child has ephemeral agent_id "eph-worker-a1b2c3d4", not "opus"
   - Broker resolves identity from bearer token, not from request body
   - Child cannot send kind=task upward (ACL: Phase 1)
   - Child cannot modify its own trust_level (broker-owned)
   - Result: impersonation impossible

3. Worker spawns child, child never replies (resource exhaustion)

   Phase 3A response:
   - spawn_runs row has expires_at
   - Timeout fires → broker sets status=timeout, revokes ephemeral token
   - Process killed by parent's scheduler
   - Audit: IpcAdminAction(timeout_revoke)
   - Result: no resource leak

4. Worker tries to use workload profile to bypass sandbox

   Phase 3A response:
   - Worker requests workload="permissive" with allowed_tools=["shell"]
   - apply_workload() checks: "shell" ⊆ boundary.tools_ceiling for L3? Yes.
   - But execution boundary still enforces: Bubblewrap + workspace-only FS + medium-risk shell
   - Workload cannot weaken the sandbox, only narrow the tool set
   - Result: sandbox boundary intact regardless of workload
```

---

## What's NOT in Phase 3

- **Cross-host IPC via Tailscale** — separate initiative, requires mTLS or WireGuard auth, different trust model
- **PromptGuard sanitize mode** — useful but orthogonal to execution isolation
- **WebAssembly runtime** — interesting but premature; Docker + Bubblewrap cover the isolation need
- **`POST /admin/ipc/spawn`** — spawn is agent-local, not broker-side compute (see AD-1)

---

## Verification

### Phase 3A
1. `cargo fmt --all -- --check`
2. `cargo clippy --all-targets -- -D warnings`
3. `cargo test` — unit tests for ephemeral identity, spawn workflow, profile selection, fail-closed sandbox
4. Manual: Opus spawns ephemeral worker → worker sends result → verify delivery + auto-revoke
5. Manual: spawn with L4 profile on host without Docker/Bubblewrap → verify spawn refused
6. Manual: spawn → timeout → verify cleanup + spawn_runs status
7. Manual: broker restart during active spawn → verify interrupted status

### Phase 3B
8. Security tests:
   - Tampered audit entry → chain verification fails
   - Replayed message (same seq) → broker rejects
   - Forged signature → broker rejects
   - Stolen ephemeral token after revoke → 401
   - Expired delegation certificate → broker rejects signed message

---

## Risk

**Phase 3A: Medium** — new plumbing (ephemeral identity, spawn-wait, fail-closed profiles) but builds on existing infrastructure (pairing, sandbox, cron scheduler). Feature flag: `agents_spawn.wait` is opt-in, fire-and-forget still works. Fail-closed sandbox may break setups that currently rely on NoopSandbox — documented as intentional.

**Phase 3B: High** — Ed25519 key management is a new trust root. Key loss = agent identity loss. Delegation certificates add complexity to ephemeral identity flow. HMAC chain adds write-path overhead to every audit event. Recommend: ship 3A, stabilize, then 3B.
