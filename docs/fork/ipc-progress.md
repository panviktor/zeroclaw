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
| 3 | Gateway plumbing: AppState + routes + IpcDb init | gateway/mod.rs, gateway/api.rs | TODO | 1, 2 |
| 4 | Broker core: IpcDb + schema + ACL | gateway/ipc.rs (new) | TODO | 3 |
| 5 | Broker handlers: send, inbox, agents_list | gateway/ipc.rs | TODO | 4 |
| 6 | Broker handlers: state_get, state_set | gateway/ipc.rs | TODO | 4 |
| 7 | Admin endpoints: revoke, disable, quarantine, downgrade | gateway/ipc.rs | TODO | 4 |
| 8 | Tools: IpcClient + agents_list, agents_send, agents_inbox | tools/agents_ipc.rs (new) | TODO | 5 |
| 9 | Tools: agents_reply, state_get, state_set | tools/agents_ipc.rs | TODO | 6, 8 |
| 10 | Tools: agents_spawn | tools/agents_ipc.rs | TODO | 1 |
| 11 | Tool registration + wizard | tools/mod.rs, onboard/wizard.rs | TODO | 8, 9, 10 |
| 12 | Tests: ACL unit tests | gateway/ipc.rs #[cfg(test)] | TODO | 4 |
| 13 | Tests: broker handler tests | gateway/ipc.rs #[cfg(test)] | TODO | 5, 6 |
| 14 | Tests: tool HTTP client tests | tools/agents_ipc.rs #[cfg(test)] | TODO | 8, 9 |
| 15 | Integration: cargo fmt + clippy + test | — | TODO | all |

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
- `IpcDb::session_has_task_for(session_id, agent_id) -> bool`
- `IpcDb::update_last_seen(agent_id)`
- Per-agent rate limiting helper
- Structured error: `IpcErrorResponse { error, code, retryable }` + detail for L1-L2
- Tracing events: structured IPC event logging

**Verify**: `cargo check`

**Notes**: This is the largest step (~300 lines). Can split if needed.

---

### Step 5: Broker handlers — send, inbox, agents_list

**What**:
- `handle_ipc_send()`: auth → rate limit → ACL → quarantine check → INSERT message → tracing event
- `handle_ipc_inbox()`: auth → read rate limit → SELECT messages (quarantine param) → mark read → update last_seen → lazy TTL cleanup
- `handle_ipc_agents()`: auth → L4 logical destinations vs full list → staleness check

**Verify**: `cargo check`

**Notes**: —

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
- `handle_admin_ipc_revoke()`: localhost check → remove token → block pending messages → close sessions → audit
- `handle_admin_ipc_disable()`: localhost → status=disabled, messages blocked, token preserved
- `handle_admin_ipc_quarantine()`: localhost → trust_level→4, messages → quarantine
- `handle_admin_ipc_downgrade()`: localhost → only downgrade (new_level > current)
- `handle_admin_ipc_agents()`: localhost → full agent list with metadata

**Verify**: `cargo check`

**Notes**: —

---

### Step 8: Tools (HTTP) — agents_list, agents_send, agents_inbox

**What**:
- `IpcClient` struct: reqwest::Client + broker_url + bearer_token, proxy-aware (`apply_runtime_proxy_to_builder`)
- `AgentsListTool`: GET /api/ipc/agents → JSON
- `AgentsSendTool`: POST /api/ipc/send → { to, kind, payload, session_id?, priority? }
- `AgentsInboxTool`: GET /api/ipc/inbox?quarantine=bool → messages + payload truncation (4000 chars)
- All implement `Tool` trait: name, description, parameters (JsonSchema), execute

**Verify**: `cargo check`

**Notes**: —

---

### Step 9: Tools (HTTP) — agents_reply, state_get, state_set

**What**:
- `AgentsReplyTool`: wrapper around POST /api/ipc/send with kind=result + auto session_id
- `StateGetTool`: GET /api/ipc/state?key=...
- `StateSetTool`: POST /api/ipc/state { key, value }

**Verify**: `cargo check`

**Notes**: —

---

### Step 10: Tool — agents_spawn

**What**:
- `AgentsSpawnTool`: local (no IpcClient), uses `cron::add_agent_job()`
- Parameters: prompt, model, session_id, wait_for_result, timeout_secs
- Trust propagation: `child_level = max(requested, parent_level)` (convention-based Phase 1)
- security.can_act() + record_action()

**Verify**: `cargo check`

**Notes**: —

---

### Step 11: Registration + wizard

**What**:
- `src/tools/mod.rs`: `pub mod agents_ipc;` + conditional registration in `all_tools_with_runtime()`
- `src/onboard/wizard.rs`: `agents_ipc: AgentsIpcConfig::default()`

**Verify**: `cargo check`, `cargo test` (enabled=false → no tools registered → no impact)

**Notes**: —

---

### Step 12: Tests — ACL unit tests

**What** (in `gateway/ipc.rs` `#[cfg(test)]`):
- validate_send: kind whitelist, L4 text-only, task upward denied, task same-level denied, correlated result, L4↔L4 denied, L3 text allowlist
- validate_state_set: L4 own namespace, L3 public, L2 team, L1 global, secret denied
- validate_state_get: secret read ACL
- IpcDb::session_has_task_for: true/false cases

**Verify**: `cargo test -- ipc`

**Notes**: —

---

### Step 13: Tests — broker handler tests

**What**:
- Integration-style tests with test AppState + IpcDb (in-memory SQLite)
- send → inbox round-trip
- Quarantine flag for L4 messages
- Rate limiting behavior
- Error response format (no hints for low-trust)

**Verify**: `cargo test -- ipc`

**Notes**: —

---

### Step 14: Tests — tool HTTP client tests

**What**:
- Mock HTTP server (or integration with real broker)
- IpcClient construction with proxy
- Tool parameter validation (JsonSchema)
- Payload truncation in inbox tool

**Verify**: `cargo test -- agents_ipc`

**Notes**: —

---

### Step 15: Final validation

**What**:
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test`
- `enabled: false` by default — all existing tests pass
- latest sync with `vendor/upstream-master` is merged or current sync PR is green
- fork invariants pass after latest upstream sync (ACL, quarantine, approval routing, revoke/disable)
- if touched file set expanded: update `fork-sync-strategy.md` hotspot list / delta registry
- Manual test flow (if time permits)

**Verify**: CI-equivalent

**Notes**: —

---

## Session Log

| Date | Session | Steps done | Notes |
|------|---------|------------|-------|
| — | — | — | — |
