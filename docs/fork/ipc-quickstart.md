# IPC Quickstart

Minimal configs and smoke-test commands to get broker-mediated IPC running locally.

Full design: [`ipc-plan.md`](ipc-plan.md) | Progress: [`ipc-progress.md`](ipc-progress.md)

---

## 1. Broker Config

The broker is the ZeroClaw instance that runs the gateway with IPC routes.
Typically this is your primary (Opus / L1) agent.

```toml
# ── Gateway (already exists, just ensure it's on) ──
[gateway]
port = 42617
host = "127.0.0.1"
require_pairing = true

# ── IPC broker ──
[agents_ipc]
enabled = true
max_messages_per_hour = 60
message_ttl_secs = 86400
staleness_secs = 120

# L3 lateral text: which agent pairs can message each other directly
lateral_text_pairs = [
  ["code", "daily"],
  ["sentinel", "devops"],
]

# L4 alias → real agent_id (L4 agents see only aliases)
[agents_ipc.l4_destinations]
supervisor = "opus"
escalation = "sentinel"

# Phase 2: session limits
session_max_exchanges = 10
coordinator_agent = "opus"

# Phase 2: PromptGuard payload scanning
[agents_ipc.prompt_guard]
enabled = true
action = "block"
sensitivity = 0.55
exempt_levels = [0, 1]
```

> **Note**: The broker itself does not need `broker_url` or `broker_token` — it *is* the broker. These fields are only for agent instances that connect to the broker.

---

## 2. L3 Agent Config (worker)

A standard worker agent that connects to the broker. Trust level 3 — can send text laterally (if allowlisted), send queries/results upward, receive tasks downward.

```toml
[agents_ipc]
enabled = true
broker_url = "http://127.0.0.1:42617"
broker_token = "<token-from-pairing>"
trust_level = 3
role = "worker"
agent_id = "code"           # Must match the agent_id used in POST /admin/paircode/new
request_timeout_secs = 10
max_messages_per_hour = 60
```

> **Important**: `agent_id` must match the `agent_id` in the broker's `token_metadata` (set during pairing via `POST /admin/paircode/new`). This is used for Ed25519 message signing — a mismatch causes `signature_invalid`. If not set, falls back to `role`, which will break if the broker paired this agent with a different agent_id.

---

## 3. L4 Agent Config (restricted)

A restricted agent (e.g. kids). Cannot send tasks upward, can only send text to allowlisted aliases, sees masked agent list.

```toml
[agents_ipc]
enabled = true
broker_url = "http://127.0.0.1:42617"
broker_token = "<token-from-pairing>"
trust_level = 4
role = "restricted"
agent_id = "kids"           # Must match pairing agent_id
request_timeout_secs = 10
max_messages_per_hour = 30
```

---

## 4. Pairing Flow

### Step 1: Generate a paircode on the broker (localhost only)

```bash
# L1 coordinator
curl -sS -X POST http://127.0.0.1:42617/admin/paircode/new \
  -H 'Content-Type: application/json' \
  -d '{"agent_id": "opus", "trust_level": 1, "role": "coordinator"}'

# L3 worker
curl -sS -X POST http://127.0.0.1:42617/admin/paircode/new \
  -H 'Content-Type: application/json' \
  -d '{"agent_id": "code", "trust_level": 3, "role": "worker"}'

# L4 restricted
curl -sS -X POST http://127.0.0.1:42617/admin/paircode/new \
  -H 'Content-Type: application/json' \
  -d '{"agent_id": "kids", "trust_level": 4, "role": "restricted"}'
```

Response contains the one-time pairing code.

### Step 2: Exchange paircode for bearer token (from agent)

```bash
curl -sS -X POST http://127.0.0.1:42617/pair \
  -H 'X-Pairing-Code: <code-from-step-1>'
```

Response contains the bearer token. Put it in `agents_ipc.broker_token`.

---

## 5. Smoke Test

After pairing at least two agents, run these from a terminal to verify IPC works.

```bash
BROKER="http://127.0.0.1:42617"
TOKEN_L1="<opus-bearer-token>"
TOKEN_L3="<code-bearer-token>"
```

### 5.1 List agents

```bash
curl -sS "$BROKER/api/ipc/agents" \
  -H "Authorization: Bearer $TOKEN_L1" | jq .
```

### 5.2 Send a message (L1 → L3 task)

```bash
curl -sS -X POST "$BROKER/api/ipc/send" \
  -H "Authorization: Bearer $TOKEN_L1" \
  -H 'Content-Type: application/json' \
  -d '{
    "to": "code",
    "kind": "task",
    "payload": "run cargo test and report results"
  }' | jq .
```

### 5.3 Check inbox (as L3 agent)

```bash
curl -sS "$BROKER/api/ipc/inbox" \
  -H "Authorization: Bearer $TOKEN_L3" | jq .
```

### 5.4 Reply with result (L3 → L1)

```bash
curl -sS -X POST "$BROKER/api/ipc/send" \
  -H "Authorization: Bearer $TOKEN_L3" \
  -H 'Content-Type: application/json' \
  -d '{
    "to": "opus",
    "kind": "result",
    "payload": "all 7228 tests passed",
    "session_id": "<session_id-from-inbox-message>"
  }' | jq .
```

### 5.5 Shared state

```bash
# Set state (L1 can write global:*)
curl -sS -X POST "$BROKER/api/ipc/state" \
  -H "Authorization: Bearer $TOKEN_L1" \
  -H 'Content-Type: application/json' \
  -d '{"key": "global:status:deploy", "value": "in-progress"}'

# Get state
curl -sS "$BROKER/api/ipc/state?key=global:status:deploy" \
  -H "Authorization: Bearer $TOKEN_L3" | jq .
```

### 5.6 ACL denial (L3 → L1 task should fail)

```bash
curl -sS -X POST "$BROKER/api/ipc/send" \
  -H "Authorization: Bearer $TOKEN_L3" \
  -H 'Content-Type: application/json' \
  -d '{
    "to": "opus",
    "kind": "task",
    "payload": "this should be denied"
  }'
# Expected: 403 — "Cannot assign tasks to higher-trust agents"
```

---

## 6. Admin Operations (localhost only)

```bash
# List all agents with full metadata
curl -sS "$BROKER/admin/ipc/agents" | jq .

# Quarantine an agent (trust → L4, pending messages moved)
curl -sS -X POST "$BROKER/admin/ipc/quarantine" \
  -H 'Content-Type: application/json' \
  -d '{"agent_id": "kids"}'

# Disable an agent (blocks messages, preserves token)
curl -sS -X POST "$BROKER/admin/ipc/disable" \
  -H 'Content-Type: application/json' \
  -d '{"agent_id": "kids"}'

# Revoke an agent (removes bearer token entirely)
curl -sS -X POST "$BROKER/admin/ipc/revoke" \
  -H 'Content-Type: application/json' \
  -d '{"agent_id": "kids"}'

# Downgrade trust level (can only go down, not up)
curl -sS -X POST "$BROKER/admin/ipc/downgrade" \
  -H 'Content-Type: application/json' \
  -d '{"agent_id": "code", "new_level": 3}'

# Promote a quarantine message to normal inbox (Phase 2)
curl -sS -X POST "$BROKER/admin/ipc/promote" \
  -H 'Content-Type: application/json' \
  -d '{"message_id": 42, "to_agent": "opus"}'
```

---

## 7. Available IPC Tools (inside agent)

When `agents_ipc.enabled = true` and `broker_token` is set, these tools are registered:

| Tool | What it does |
|------|-------------|
| `agents_list` | List online agents (L4 sees aliases only) |
| `agents_send` | Send text/task/query/result to another agent |
| `agents_inbox` | Fetch unread messages (optional `quarantine=true` for L4 lane) |
| `agents_reply` | Reply with correlated result (auto `session_id`) |
| `state_get` | Read shared state key |
| `state_set` | Write shared state key (namespace ACL applies) |
| `agents_spawn` | Spawn a new agent via cron (fire-and-forget, Phase 1) |

`agents_spawn` is available even without `broker_token` (local operation).

---

## Trust Level Reference

| Level | Name | Can send | Can receive | State write scope |
|-------|------|----------|-------------|-------------------|
| 0 | Admin | Everything | Everything | All namespaces |
| 1 | Coordinator | Everything | Everything | `global:*` + below |
| 2 | Team lead | text/task/query/result | Everything | `team:*` + below |
| 3 | Worker | text ↑, query ↑, result ↑ (lateral text if allowlisted) | text/task/query | `public:*` + `agent:{self}:*` |
| 4 | Restricted | text only → aliases | text only (quarantine lane) | `agent:{self}:*` only |

---

## Phase 3A: Ephemeral Spawn Workflow

### Spawn and wait for result

From an agent with IPC configured (broker_token set):

```json
{
  "name": "agents_spawn",
  "arguments": {
    "prompt": "Analyze the top 5 Hacker News stories and summarize trends",
    "wait": true,
    "timeout": 300,
    "trust_level": 3
  }
}
```

The tool will:
1. Provision ephemeral identity from broker (`POST /api/ipc/provision-ephemeral`)
2. Verify sandbox is available for the trust level (fail-closed)
3. Launch child as a separate OS process with its own token
4. Poll `GET /api/ipc/spawn-status` until completed or timeout
5. Return the child's result payload

### Fire-and-forget spawn

```json
{
  "name": "agents_spawn",
  "arguments": {
    "prompt": "Clean up old log files in /tmp",
    "wait": false,
    "trust_level": 3
  }
}
```

Returns immediately with `session_id` — parent can poll status manually if needed.

### Spawn with workload profile

Define workload profiles in config:

```toml
[agents_ipc.workload_profiles.research]
model = "claude-sonnet-4-6"
allowed_tools = ["web_search", "web_fetch", "memory_read"]

[agents_ipc.workload_profiles.code_review]
model = "claude-opus-4-6"
allowed_tools = ["shell", "file_read", "file_write"]
```

Then use in spawn:

```json
{
  "name": "agents_spawn",
  "arguments": {
    "prompt": "Review the latest PR for security issues",
    "wait": true,
    "timeout": 600,
    "workload": "code_review"
  }
}
```

### Sandbox enforcement

Trust levels L2-L4 require a real sandbox backend. If the required sandbox is unavailable, spawn is **refused** — no fallback to NoopSandbox.

| Trust | Required sandbox | NoopSandbox |
|-------|-----------------|-------------|
| L0-L1 | Optional | Allowed |
| L2 | Landlock or Docker | Denied |
| L3 | Bubblewrap, Landlock, or Docker | Denied |
| L4 | Bubblewrap or Docker | Denied |

### Ephemeral agent lifecycle

```
Parent: agents_spawn(wait=true) → provision → subprocess → poll
Child:  [auto-configured via env vars] → runs → agents_reply(session_id, result)
Broker: deliver to parent inbox + complete spawn_run + auto-revoke child token
Parent: poll returns {status: "completed", result: "..."}
```

Child processes auto-configure IPC from environment variables set by the parent:
- `ZEROCLAW_BROKER_TOKEN` — ephemeral bearer token
- `ZEROCLAW_BROKER_URL` — broker gateway URL
- `ZEROCLAW_AGENT_ID` — ephemeral agent identity
- `ZEROCLAW_SESSION_ID` — spawn session for reply correlation
