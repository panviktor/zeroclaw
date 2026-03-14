# IPC Phase 3: Trusted Execution

Phase 1: brokered coordination | Phase 2: broker-side safety | **Phase 3: trusted execution**

Phase 1 plan: [`ipc-plan.md`](ipc-plan.md) | Phase 2 plan: [`ipc-phase2-plan.md`](ipc-phase2-plan.md)

---

## What Phase 3 gives

Three promises to the operator:

1. **Child = separate actor** — spawn a disposable worker with its own identity, inbox, and lifecycle.
2. **Trust level = real runtime boundary** — policy enforcement backed by OS-level isolation, not just ACL checks.
3. **Every message/result = provenance-preserving** — broker can prove who sent what, detect replay and tampering.

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

## User Stories

### Phase 3A

1. **"Opus can spawn a disposable worker and wait for its result."**
   - Operator action: Opus calls `agents_spawn(prompt="...", wait=true, timeout=300)`
   - System behavior: broker provisions ephemeral identity → child runs → sends result → identity auto-revoked
   - Security property: child cannot outlive its session; stale tokens are garbage-collected

2. **"Restricted agents run in a real sandbox, not trust-by-convention."**
   - Operator action: set `trust_level = 4` in pairing config
   - System behavior: child process inherits sandbox profile matching its trust level (L4 = strictest, L1 = permissive)
   - Security property: L4 agent physically cannot write outside `/tmp`, execute network calls, or escalate privileges

3. **"An operator can revoke an agent and know its future messages are rejected."**
   - Operator action: `POST /admin/ipc/revoke {agent_id: "..."}` or child session ends
   - System behavior: token removed, pending messages blocked, future auth attempts fail
   - Security property: revocation is immediate and irrecoverable (already implemented, but now covers ephemeral children)

### Phase 3B

4. **"An incident reviewer can verify who sent what and whether logs were tampered with."**
   - Operator action: `zeroclaw audit verify --since 2026-03-14`
   - System behavior: HMAC chain validated, signature on each message verified against agent's public key
   - Security property: any modification or deletion of audit entries is detectable

---

## Primary Workflow

```
Parent (Opus, L1)                          Broker                               Child (ephemeral, L3)
      │                                      │                                       │
      │ agents_spawn(                        │                                       │
      │   profile="research",                │                                       │
      │   prompt="analyze X",                │                                       │
      │   wait=true,                         │                                       │
      │   timeout=300)                       │                                       │
      │─────────────────────────────────────>│                                       │
      │                                      │ 1. Generate ephemeral agent_id        │
      │                                      │ 2. Create temporary bearer token      │
      │                                      │ 3. Register in agents table           │
      │                                      │ 4. Select sandbox profile for L3      │
      │                                      │ 5. Create session_id                  │
      │                                      │ 6. Launch child process               │
      │                                      │──────────────────────────────────────>│
      │                                      │                                       │
      │                                      │    (child runs with sandbox + token)   │
      │                                      │                                       │
      │                                      │ agents_send(kind=result,              │
      │                                      │   session_id=...,                     │
      │                                      │   payload="findings...")              │
      │                                      │<──────────────────────────────────────│
      │                                      │                                       │
      │                                      │ 7. Validate result (ACL + scan)       │
      │                                      │ 8. Insert into parent's inbox         │
      │                                      │ 9. Revoke child token                 │
      │                                      │ 10. Cleanup child process             │
      │                                      │                                       │
      │  { result: "findings..." }           │                                       │
      │<─────────────────────────────────────│                                       │
```

---

## Execution Profiles

Trust level determines what the child process can actually do at the OS level.

| Profile | Trust | Sandbox | Filesystem | Network | Tools | Shell |
|---------|-------|---------|------------|---------|-------|-------|
| `coordinator` | L0-L1 | None / Landlock (permissive) | Full workspace | Full | All | Unrestricted |
| `privileged` | L2 | Landlock | Workspace + allowed_roots | Full | All except admin | Classified commands |
| `worker` | L3 | Landlock + Bubblewrap | Workspace only, no dotdirs | Outbound only | Tool allowlist | Medium-risk max |
| `restricted` | L4 | Bubblewrap / Docker | `/tmp` + read-only workspace | None | Text-only (no shell, no file write) | Denied |

### Profile selection

```
trust_level from token_metadata
        │
        ▼
  ┌─────────────┐     ┌──────────────────────┐
  │ SandboxConfig│────>│ detect_best_sandbox() │
  │ per profile  │     │ (existing infra)      │
  └─────────────┘     └──────────────────────┘
                              │
                              ▼
                     sandbox.wrap_command()
                     (existing Sandbox trait)
```

Implementation reuses existing infrastructure:
- `src/security/detect.rs::create_sandbox()` — backend selection
- `src/security/traits.rs::Sandbox` trait — command wrapping
- `src/security/policy.rs::SecurityPolicy` — autonomy level, command classification, path validation
- `src/runtime/traits.rs::RuntimeAdapter` — native vs docker command building

What's new: a **profile registry** that maps `trust_level → (SandboxConfig, AutonomyConfig, allowed_tools)`. Currently both are global; Phase 3A makes them per-agent.

---

## Identity Model

### Persistent agents (existing)

Agents paired manually via `/admin/paircode/new` → `/pair`. Long-lived bearer token stored in config. Agent_id, trust_level, role in `token_metadata`.

No changes needed.

### Ephemeral agents (Phase 3A — new)

Created by `agents_spawn`. Lifecycle bound to a single session.

```
Provisioning:
  1. Parent calls agents_spawn(profile, prompt, wait, timeout)
  2. Broker generates:
     - agent_id: "eph-{parent_id}-{uuid_short}" (8 char suffix)
     - bearer_token: random 32 bytes → hex
     - session_id: uuid
  3. Broker registers in agents table:
     - trust_level = max(parent_level, requested_level)
     - role = profile name
     - status = "ephemeral"
     - metadata.parent = parent_agent_id
     - metadata.session_id = session_id
     - metadata.expires_at = now + timeout
  4. Broker inserts token_metadata for the ephemeral token
  5. Child process launched with:
     - ZEROCLAW_BROKER_URL=http://127.0.0.1:{port}
     - ZEROCLAW_BROKER_TOKEN={token}
     - ZEROCLAW_AGENT_ID={agent_id}
     - ZEROCLAW_SESSION_ID={session_id}
     - ZEROCLAW_REPLY_TO={parent_agent_id}
     - Config overlay: agents_ipc.enabled=true, sandbox profile per trust

Teardown:
  1. Child sends kind=result with session_id → broker delivers to parent
  2. Broker revokes ephemeral token (remove from token_metadata + paired_tokens)
  3. Broker sets agent status = "completed" (or "timeout" / "revoked")
  4. Parent's agents_spawn returns the result

  OR: timeout fires → broker revokes token, kills process, returns timeout error

  OR: operator calls /admin/ipc/revoke → immediate teardown
```

### Token lifecycle

```
                    ┌─────────────┐
                    │  generated   │
                    └──────┬──────┘
                           │ pair / auto-pair
                           ▼
                    ┌─────────────┐
        ┌──────────│   active     │──────────┐
        │          └──────┬──────┘           │
        │ revoke          │ session end       │ timeout
        ▼                 ▼                   ▼
  ┌──────────┐    ┌─────────────┐    ┌────────────┐
  │  revoked  │    │  completed   │    │  expired   │
  └──────────┘    └─────────────┘    └────────────┘
```

All terminal states: token removed from `paired_tokens`, future requests → 401.

---

## Operator Operations

### For operators (admin endpoints, localhost only)

| Operation | Endpoint | Phase |
|-----------|----------|-------|
| Spawn child | `agents_spawn` tool (via agent) or `POST /admin/ipc/spawn` | 3A |
| Wait / inspect | `GET /admin/ipc/agents` (shows ephemeral children + status) | 3A |
| Promote result | `POST /admin/ipc/promote` (existing) | Done |
| Revoke / kill | `POST /admin/ipc/revoke` (existing, extended for ephemeral) | 3A |
| Quarantine / downgrade | `POST /admin/ipc/quarantine`, `/downgrade` (existing) | Done |
| Show provenance | `zeroclaw audit verify` CLI | 3B |

### For agents (IPC tools)

| Operation | Tool | Phase |
|-----------|------|-------|
| Spawn child and wait | `agents_spawn(profile=..., wait=true, timeout=...)` | 3A |
| Reply with result | `agents_reply(session_id=..., payload=...)` (existing) | Done |
| Write shared state | `state_set(key=..., value=...)` (existing) | Done |
| Request approval | Route through Opus via `agents_send(kind=query)` (existing pattern) | Done |

The agent knows nothing about Unix users, sandbox backends, or signing keys. Those are implementation details under `agents_spawn`.

---

## Phase 3A: Implementation Steps

### Step 1: Ephemeral identity provisioning

**Files**: `src/gateway/ipc.rs`, `src/security/pairing.rs`

- Add `provision_ephemeral_agent()` to `IpcDb`:
  - Generate agent_id, token, session_id
  - Insert into agents table with `status = "ephemeral"`, expiry metadata
  - Insert into `paired_tokens` / `token_metadata` via pairing module
- Add `revoke_ephemeral_agent()`:
  - Remove token, set status = "completed" / "expired" / "revoked"
  - Block pending messages (existing `block_pending_messages`)
- Add expiry reaper: periodic check for expired ephemeral agents (in gateway tick or lazy on request)

### Step 2: `agents_spawn` upgrade — session binding + wait

**Files**: `src/tools/agents_ipc.rs`, `src/cron/store.rs`

- Extend `agents_spawn` tool schema:
  - `wait: bool` (default: false, backward compat)
  - `timeout: u32` (seconds, default: 300)
  - `profile: string` (execution profile name, default: inferred from trust_level)
- Extend `add_agent_job()`:
  - Accept `session_id`, `reply_to`, `broker_token`, `agent_id` fields
  - Pass to child via config overlay or env vars
- Wait mode implementation:
  - After spawning, poll `fetch_inbox(session_id, kind=result)` with backoff
  - Or: use a oneshot channel between scheduler and spawn caller
  - Timeout: revoke child + return error

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
- On successful result delivery:
  - Broker auto-revokes ephemeral child token
  - Broker sets agent status = "completed"
  - Audit: `IpcSend` + `IpcAdminAction(auto_revoke)`
- On timeout:
  - Broker revokes token, sets status = "expired"
  - Parent receives timeout error
  - Audit: `IpcAdminAction(timeout_revoke)`

### Step 5: Execution profiles

**Files**: `src/config/schema.rs`, `src/security/detect.rs`, `src/tools/agents_ipc.rs`

- Add `ExecutionProfile` config struct:
  ```rust
  pub struct ExecutionProfile {
      pub sandbox: SandboxConfig,
      pub autonomy: AutonomyConfig,
      pub allowed_tools: Option<Vec<String>>,
  }
  ```
- Add `[execution_profiles]` section to config:
  ```toml
  [execution_profiles.worker]
  sandbox.backend = "bubblewrap"
  autonomy.level = "supervised"
  autonomy.workspace_only = true

  [execution_profiles.restricted]
  sandbox.backend = "docker"
  autonomy.level = "readonly"
  ```
- Default profiles generated from trust_level if not explicitly configured
- `agents_spawn` passes profile to child via config overlay

### Step 6: Integration tests + docs

- Test: spawn ephemeral → send result → verify delivery → verify revoke
- Test: spawn with timeout → verify timeout revoke
- Test: L4 spawn gets restricted profile
- Test: ephemeral agent cannot outlive session
- Update `ipc-quickstart.md` with spawn workflow examples
- Update `delta-registry.md`

---

## Phase 3B: Implementation Steps

### Step 7: Ed25519 agent identity

**Files**: `src/security/identity.rs` (new), `src/gateway/ipc.rs`, `src/config/schema.rs`

- Each persistent agent generates Ed25519 keypair on first pairing
- Public key stored in broker's agents table
- Ephemeral agents get temporary keypair (provisioned by broker, discarded on revoke)
- Bearer token remains for transport auth; Ed25519 is for message signing

### Step 8: Signed messages

**Files**: `src/gateway/ipc.rs`, `src/tools/agents_ipc.rs`

- Agent signs `{from_agent}|{to_agent}|{seq}|{payload_hash}` with Ed25519 private key
- Signature sent as header or JSON field in `POST /api/ipc/send`
- Broker verifies signature against registered public key before INSERT
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
   - Child gets L3 execution profile (Bubblewrap sandbox, workspace-only filesystem)
   - Even if prompt contains destructive commands, sandbox blocks /home access
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
   - Timeout fires after configured seconds
   - Broker revokes ephemeral token
   - Process killed
   - Audit: IpcAdminAction(timeout_revoke)
   - Result: no resource leak
```

---

## What's NOT in Phase 3

- **Cross-host IPC via Tailscale** — separate initiative, requires mTLS or WireGuard auth, different trust model
- **PromptGuard sanitize mode** — useful but orthogonal to execution isolation
- **WebAssembly runtime** — interesting but premature; Docker + Bubblewrap cover the isolation need

---

## Verification

### Phase 3A
1. `cargo fmt --all -- --check`
2. `cargo clippy --all-targets -- -D warnings`
3. `cargo test` — unit tests for ephemeral identity, spawn workflow, profile selection
4. Manual: Opus spawns ephemeral worker → worker sends result → verify delivery + auto-revoke
5. Manual: spawn with L4 profile → verify sandbox restrictions apply
6. Manual: spawn → timeout → verify cleanup

### Phase 3B
7. Security tests:
   - Tampered audit entry → chain verification fails
   - Replayed message (same seq) → broker rejects
   - Forged signature → broker rejects
   - Stolen ephemeral token after revoke → 401

---

## Risk

**Phase 3A: Medium** — new plumbing (ephemeral identity, spawn-wait, profile selection) but builds on existing infrastructure (pairing, sandbox, cron scheduler). Feature flag: `agents_spawn.wait` is opt-in, fire-and-forget still works.

**Phase 3B: High** — Ed25519 key management is a new trust root. Key loss = agent identity loss. HMAC chain adds write-path overhead to every audit event. Recommend: ship 3A, stabilize, then 3B.
