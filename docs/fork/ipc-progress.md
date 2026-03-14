# IPC Implementation Progress

Full plan: [`ipc-plan.md`](ipc-plan.md)
Sync strategy: [`sync-strategy.md`](sync-strategy.md)
Delta registry: [`delta-registry.md`](delta-registry.md)
Base branch: `main`
Working branch: feature branch off `main` (e.g. `feat/ipc-*`)
Execution owner: `Opus`

## Steps Overview

| # | Step | Files | Status | Depends on |
|---|------|-------|--------|------------|
| 1 | Config: AgentsIpcConfig + schema | config/schema.rs, config/mod.rs | DONE (2026-03-13) | — |
| 2 | Pairing: TokenMetadata + authenticate() | security/pairing.rs | DONE (2026-03-13) | 1 |
| 3 | Gateway plumbing: AppState + routes + IpcDb init | gateway/mod.rs, gateway/api.rs, gateway/ipc.rs | DONE (2026-03-13) | 1, 2 |
| 4 | Broker core: IpcDb + schema + ACL + unit tests | gateway/ipc.rs (new) | DONE (2026-03-13) | 3 |
| 5 | Broker handlers: send, inbox, agents_list + tests | gateway/ipc.rs | DONE (2026-03-13) | 4 |
| 6 | Broker handlers: state_get, state_set | gateway/ipc.rs | DONE (2026-03-13) | 4 |
| 7 | Admin endpoints: revoke, disable, quarantine, downgrade | gateway/ipc.rs | DONE (2026-03-13) | 4 |
| 8 | Tools: IpcClient + agents_list, agents_send, agents_inbox + registration + tests | tools/agents_ipc.rs (new), tools/mod.rs | DONE (2026-03-13) | 5 |
| 9 | Tools: agents_reply, state_get, state_set | tools/agents_ipc.rs | DONE (2026-03-13) | 6, 8 |
| 10 | Tools: agents_spawn | tools/agents_ipc.rs | DONE (2026-03-13) | 1 |
| 11 | Final validation: fmt + clippy + test + sync | — | TODO | all |

> **Note on tests and registration**: Unit tests were written inline with each step (ACL tests in Step 4, handler tests in Steps 5-7, tool tests in Steps 8-10). Tool registration in `tools/mod.rs` and wizard defaults in `onboard/wizard.rs` were done as part of Steps 1 and 8. These are not separate steps.

## Step Details

### Step 1: Config — `src/config/schema.rs`, `src/config/mod.rs`

**What**:
- Add `AgentsIpcConfig` struct with `#[serde(default)]` + `Default` + `JsonSchema`
- Fields: enabled, broker_url, broker_token, staleness_secs, message_ttl_secs, trust_level, role, max_messages_per_hour, request_timeout_secs, lateral_text_pairs, l4_destinations
- Add `agents_ipc: AgentsIpcConfig` to root `Config` (concrete, not Option)
- Add `token_metadata: HashMap<String, TokenMetadata>` to `GatewayConfig`
- Export in `config/mod.rs`
- Add `broker_token` to `Config::save()` encryption path

**Verify**: `cargo check`, existing tests pass

**Notes**: Also added `agents_ipc` to wizard.rs (2 constructors). Commit: `731f9115` on `feat/ipc-config`.

---

### Step 2: Pairing — `src/security/pairing.rs`

**What**:
- Add `TokenMetadata` struct (agent_id, trust_level, role) + `effective_trust_level()` + `is_ipc_eligible()`
- Internal: `HashSet<String>` → `HashMap<String, TokenMetadata>`
- New method: `authenticate(&self, token: &str) -> Option<TokenMetadata>`
- Old `is_authenticated()` delegates to `authenticate().is_some()`
- Init: merge `config.gateway.paired_tokens` + `config.gateway.token_metadata`
- Add `pending_metadata: HashMap<String, TokenMetadata>` for paircode flow
- Extend `POST /admin/paircode/new`: optional `PaircodeNewBody { agent_id, trust_level, role }`
- `try_pair()`: transfer metadata from pending to token store

**Verify**: `cargo check`, existing pairing tests pass, no breaking changes to API responses

**Notes**: Also updated gateway: persist_pairing_tokens saves metadata, handle_admin_paircode_new accepts optional body, startup uses with_metadata(). 7 new tests. Commit: `343b2bc3` on `feat/ipc-pairing`.

---

### Step 3: Gateway plumbing — `src/gateway/mod.rs`, `src/gateway/api.rs`

**What**:
- AppState new fields: `ipc_db: Option<Arc<IpcDb>>`, `ipc_rate_limiter: Option<Arc<SlidingWindowRateLimiter>>`, `ipc_read_rate_limiter: Option<Arc<SlidingWindowRateLimiter>>`
- Conditional init (if `config.agents_ipc.enabled`)
- Route registration: 5 IPC + 5 admin endpoints
- `pub mod ipc;`
- `extract_bearer_token()` → `pub(crate)` in api.rs
- `mask_config()` / `hydrate_config()`: add agents_ipc.broker_token, gateway.token_metadata

**Verify**: `cargo check` (ipc.rs can be stub/empty at this point)

**Notes**: —

---

### Step 4: Broker core — `src/gateway/ipc.rs` (new file, part 1)

**What**:
- `IpcDb` struct: `Arc<parking_lot::Mutex<Connection>>`, WAL, init_schema()
- SQLite schema: agents, messages (with quarantine column), shared_state, message_sequences
- `require_ipc_auth()` helper
- `validate_send()` — Rules 0-5 (whitelist, L4 text-only, task-downward-only, correlated result, L4↔L4 denied, L3 text allowlist)
- `validate_state_set()`, `validate_state_get()`
- `IpcDb::session_has_request_for(session_id, agent_id) -> bool`
- `IpcDb::update_last_seen(agent_id)`
- Per-agent rate limiting helper
- Structured error: `IpcErrorResponse { error, code, retryable }` + detail for L1-L2
- Tracing events: structured IPC event logging
- Unit tests: validate_send (kind, L4, task direction, result correlation, L4↔L4, L3 allowlist), validate_state_set/get, session_has_request_for

**Verify**: `cargo check`

**Notes**: ~300 lines. PR #10 on `feat/ipc-broker-core`. 25 unit tests.

---

### Step 5: Broker handlers — send, inbox, agents_list

**What**:
- `handle_ipc_send()`: auth → rate limit → L4 alias resolution → ACL → quarantine check → INSERT message → tracing event
- `handle_ipc_inbox()`: auth → read rate limit → SELECT messages (quarantine param) → mark read → update last_seen → lazy TTL cleanup
- `handle_ipc_agents()`: auth → L4 logical destination aliases (masked metadata) vs full list → staleness check
- Tests: insert/fetch roundtrip, quarantine isolation, TTL cleanup

**Verify**: `cargo check`

**Notes**: Steps 5-7 implemented together. PR #12 on `feat/ipc-broker-handlers`. 40 tests total. Critical fix PR #13 followed: admin kill-switch effectiveness, query→result correlation, L4 topology masking, quarantine isolation. PR #18: rate limiting enforcement, L4 alias abstraction, retroactive quarantine, token revocation.

---

### Step 6: Broker handlers — state_get, state_set

**What**:
- `handle_ipc_state_get()`: auth → validate_state_get(trust, key) → SELECT from shared_state
- `handle_ipc_state_set()`: auth → validate_state_set(trust, agent_id, key) → UPSERT shared_state

**Verify**: `cargo check`

**Notes**: —

---

### Step 7: Admin endpoints

**What**:
- `handle_admin_ipc_revoke()`: localhost check → block pending messages → revoke bearer token via `PairingGuard::revoke_by_agent_id()` → set status=revoked → audit
- `handle_admin_ipc_disable()`: localhost → status=disabled, messages blocked, token preserved
- `handle_admin_ipc_quarantine()`: localhost → trust_level→4, retroactive `quarantine_pending_messages()` (moves unread messages to quarantine lane) → status=quarantined
- `handle_admin_ipc_downgrade()`: localhost → only downgrade (new_level > current)
- `handle_admin_ipc_agents()`: localhost → full agent list with metadata

**Verify**: `cargo check`

**Notes**: See Step 5 notes.

---

### Step 8: Tools (HTTP) — agents_list, agents_send, agents_inbox

**What**:
- `IpcClient` struct: reqwest::Client + broker_url + bearer_token, proxy-aware (`apply_runtime_proxy_to_builder`)
- `AgentsListTool`: GET /api/ipc/agents → JSON
- `AgentsSendTool`: POST /api/ipc/send → { to, kind, payload, session_id?, priority? }
- `AgentsInboxTool`: GET /api/ipc/inbox?quarantine=bool → messages + payload truncation (4000 chars)
- All implement `Tool` trait: name, description, parameters (JsonSchema), execute
- Tool registration: `pub mod agents_ipc;` + conditional registration in `all_tools_with_runtime()`
- Tests: client URL handling, tool specs, payload truncation

**Verify**: `cargo check`

**Notes**: Steps 8-10 implemented together. PR #15 on `feat/ipc-tools`. 10 tool tests. Registration: 6 HTTP tools require `broker_token`, `agents_spawn` only requires `enabled`.

---

### Step 9: Tools (HTTP) — agents_reply, state_get, state_set

**What**:
- `AgentsReplyTool`: wrapper around POST /api/ipc/send with kind=result + auto session_id
- `StateGetTool`: GET /api/ipc/state?key=...
- `StateSetTool`: POST /api/ipc/state { key, value }

**Verify**: `cargo check`

**Notes**: See Step 8 notes.

---

### Step 10: Tool — agents_spawn

**What**:
- `AgentsSpawnTool`: local (no IpcClient), uses `cron::add_agent_job()`
- Parameters: prompt (required), name (optional), model (optional), trust_level (optional, 0-4)
- Trust propagation: `child_level = max(requested, parent_level)` (convention-based Phase 1)
- security.can_act() check
- Phase 2 planned: session_id, wait_for_result, timeout_secs (not yet implemented)

**Verify**: `cargo check`

**Notes**: See Step 8 notes. Uses `cron::Schedule::At` for one-shot immediate execution with `delete_after_run=true`. Fire-and-forget in Phase 1; synchronous wait is deferred to Phase 2.

---

### Step 11: Final validation

**What**:
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test`
- `enabled: false` by default — all existing tests pass
- latest sync with `vendor/upstream-master` is merged or current sync PR is green
- fork invariants pass after latest upstream sync (ACL, quarantine, approval routing, revoke/disable)
- if touched file set expanded: update `sync-strategy.md` hotspot list / `delta-registry.md`
- Manual test flow (if time permits)

**Verify**: CI-equivalent

**Notes**: —

---

## Session Log

| Date | Session | Steps done | Notes |
|------|---------|------------|-------|
| 2026-03-13 | 1 | 1, 2, 3 | Config, pairing, gateway plumbing. PRs #5, #6, #7 |
| 2026-03-13 | 2 | 4 | Broker core: IpcDb, ACL, 25 tests. PR #10 |
| 2026-03-13 | 3 | 5, 6, 7 | All handlers + admin endpoints + 40 tests. PR #12 |
| 2026-03-13 | 3 | fix | Critical fixes: kill-switch, query→result, L4 masking, quarantine. PR #13 |
| 2026-03-13 | 3 | fix | Sync script fixes (sed delimiter, workflow failures). PR #14 |
| 2026-03-13 | 3 | 8, 9, 10 | All 7 IPC tools + registration. PR #15 |
| 2026-03-14 | 4 | fix | Docs cross-references, progress tracker. PR #16 |
| 2026-03-14 | 4 | sync | Upstream sync: 40 commits, 4 conflict resolutions. PR #17 |
| 2026-03-14 | 4 | fix | 5 review findings: rate limiting, L4 aliases, spawn contract, revoke/quarantine, notify. PR #18 |
