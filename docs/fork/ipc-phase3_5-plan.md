# IPC Phase 3.5: Human Control Plane

Phase 3A: trusted execution | Phase 3B: crypto hardening | **Phase 3.5: human control plane** | Phase 4: federated execution

---

## What Phase 3.5 gives

Three promises to the operator:

1. **Visibility** — see the entire IPC system state at a glance: who's alive, who's stuck, who's quarantined, who spawned whom.
2. **Safe intervention** — revoke, quarantine, promote, kill, inspect — through a web UI, not curl or raw SQLite.
3. **Provenance understanding** — trace any message, decision, or block to its source agent, trust level, and audit chain.

---

## Why Phase 3.5 exists

Phase 3A made trusted execution **usable for agents**. Phase 3B made it **cryptographically verifiable**. But humans still interact with the system through `curl`, `sqlite3`, and tracing logs. This is fine for development — not for production.

Without Phase 3.5:
- Operator cannot see which agents are alive without `curl /admin/ipc/agents | jq`
- Quarantine review requires manual `POST /admin/ipc/promote` per message
- Spawn lifecycle is invisible unless you poll `spawn_runs` by hand
- Audit chain verification requires `zeroclaw audit verify` CLI
- Incident investigation requires reading JSONL files

With Phase 3.5:
- One screen shows system health
- Quarantine has a review queue with promote/dismiss
- Every spawn has a lifecycle timeline
- Every message is traceable to source + policy decision
- Destructive actions require confirmation

---

## Non-goals

- Not a visual policy editor (defer to Phase 4)
- Not a multi-host topology dashboard (Phase 4)
- Not a replacement for the existing chat UI (`/agent` page handles that)
- Not a full SOC platform — operator-first, not analyst-first
- Not a graph/canvas visualization — tables and timelines are sufficient

---

## Architectural Decisions

### AD-1: Extend existing web UI, localhost-only access model

The gateway already serves a React 19 + Vite + Tailwind SPA via `rust-embed`. 10 pages exist. Phase 3.5 adds a new "IPC" sidebar section with 6 pages. Same design system (glass-card, electric theme), same API patterns (`apiFetch`).

**Access model**: Phase 3.5 IPC admin pages work **only when the browser connects to localhost** (or via SSH tunnel / reverse proxy to localhost). This matches the existing `/admin/ipc/*` security model where every handler calls `require_localhost(&peer)` before processing — there is **no bearer token check** on admin endpoints, only peer address validation.

The frontend uses `apiFetch()` which sends the bearer token, but the backend admin handlers ignore it — they enforce localhost origin only. This is intentional: admin operations (revoke, quarantine, etc.) are too sensitive for bearer-only auth. A leaked gateway token should not grant admin access.

**Deployment contract**:
- **Local development**: browser at `http://localhost:{port}` → works directly
- **Remote server**: SSH tunnel (`ssh -L 8080:localhost:{port} server`) → browser at `http://localhost:8080` → works
- **Public internet / reverse proxy**: admin endpoints return 403 — `require_localhost()` checks `peer.ip().is_loopback()` and does **not** honor forwarded headers

**Why**: building a separate admin auth model (admin tokens, RBAC, session management) is significant scope. The localhost-only model is already proven in the codebase and sufficient for the target deployment (family multi-agent system on a home server).

**Consequence**: the frontend conditionally shows IPC admin pages — if `/admin/ipc/agents` returns 403, the IPC sidebar section is hidden with a tooltip "Admin pages require localhost access".

### AD-2: Read endpoints are side-effect-free

Viewing messages, agents, or audit events in the UI **must not** change state. No `read=1` flag set on view, no `last_seen` updated from admin browsing, no implicit acknowledgment.

**Why**: the existing `GET /api/ipc/inbox` has consumptive semantics — `fetch_inbox()` automatically marks messages as read on retrieval. Admin read endpoints use separate query methods that don't modify message state.

**Consequence**: admin message listing uses `db.list_messages_admin()` (new), not `db.fetch_inbox()` (existing). Agent `last_seen` is updated only by agent API calls, not admin views.

### AD-3: Quarantine is a separate review queue, not a filtered inbox

Quarantine messages are shown on a dedicated page with explicit promote/dismiss workflow. They are **never** mixed into normal message lists without explicit operator action.

**Why**: if quarantine is "just a filter on the inbox", operators will accidentally treat quarantined content as normal. The separate page + explicit promote action creates a deliberate friction point.

**Consequence**: `GET /admin/ipc/quarantine` returns only quarantine-lane messages. Promote requires confirmation dialog. Dismissed messages are marked but retained for audit.

### AD-4: Trust level is always visible

Every agent reference, every message row, every spawn — trust level is shown as a color-coded badge. L0-L1 (coordinator) green, L2 (privileged) blue, L3 (worker) yellow, L4+ (restricted) red.

**Why**: trust level determines what an agent can do, what sandbox it runs in, and what ACL rules apply. Hiding it deep in detail views means operators miss escalation or demotion.

### AD-5: Backend endpoints before frontend pages

Each implementation step delivers **backend endpoint + tests first**, then frontend page. This ensures the data contract is correct before the UI is built, and allows curl-based testing before the frontend exists.

---

## Existing Infrastructure

### Backend — already implemented (reuse as-is)

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/admin/ipc/agents` | GET | List all agents with metadata |
| `/admin/ipc/revoke` | POST | Revoke agent (block + token removal) |
| `/admin/ipc/disable` | POST | Disable agent (block messages, keep token) |
| `/admin/ipc/quarantine` | POST | Quarantine agent (trust→L4, move messages) |
| `/admin/ipc/downgrade` | POST | Downgrade trust level |
| `/admin/ipc/promote` | POST | Promote quarantine message to normal inbox |

### Backend — needs extension

| Endpoint | Method | Purpose | Why new |
|----------|--------|---------|---------|
| `/admin/ipc/agents/:id/detail` | GET | Agent + recent messages + active spawns | `list_agents` doesn't include message/spawn data |
| `/admin/ipc/messages` | GET | Paginated message list with filters | `fetch_inbox` is consumptive, agent-facing |
| `/admin/ipc/spawn-runs` | GET | Paginated spawn run list with filters | `spawn-status` is agent-facing, single session |
| `/admin/ipc/audit` | GET | Paginated audit event list with filters | No read endpoint exists, only CLI verify |
| `/admin/ipc/audit/verify` | POST | Verify HMAC chain integrity | CLI-only today |
| `/admin/ipc/dismiss-message` | POST | Mark quarantine message as dismissed | No soft-dismiss exists |

### Frontend — existing patterns to reuse

| Pattern | Location | Reuse in Phase 3.5 |
|---------|----------|---------------------|
| `apiFetch<T>()` | `web/src/lib/api.ts` | All admin API calls |
| `useAuth()` hook | `web/src/hooks/useAuth.ts` | Auth state + logout |
| `useSSE()` hook | `web/src/hooks/useSSE.ts` | Real-time status updates |
| Glass card styling | `web/src/index.css` (.glass-card) | All page containers |
| Error/loading states | Every existing page | Consistent UX |
| Sidebar navigation | `web/src/components/layout/Sidebar.tsx` | Add IPC section |
| TypeScript types | `web/src/types/api.ts` | Extend with IPC types |

### IPC Database — existing tables

```sql
-- agents: agent registry
CREATE TABLE agents (
    agent_id    TEXT PRIMARY KEY,
    trust_level INTEGER NOT NULL DEFAULT 1,
    role        TEXT NOT NULL DEFAULT 'agent',
    status      TEXT NOT NULL DEFAULT 'online',
    last_seen   INTEGER NOT NULL,
    metadata    TEXT,         -- JSON blob
    public_key  TEXT          -- Ed25519 hex (Phase 3B)
);

-- messages: IPC message store with quarantine lanes
CREATE TABLE messages (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    from_agent      TEXT NOT NULL,
    to_agent        TEXT NOT NULL,
    kind            TEXT NOT NULL DEFAULT 'text',
    payload         TEXT NOT NULL,
    from_trust_level INTEGER NOT NULL DEFAULT 0,
    session_id      TEXT,
    priority        INTEGER DEFAULT 0,
    read            INTEGER NOT NULL DEFAULT 0,
    promoted        INTEGER NOT NULL DEFAULT 0,
    blocked         INTEGER NOT NULL DEFAULT 0,
    blocked_reason  TEXT,
    seq             INTEGER,
    created_at      INTEGER NOT NULL,
    expires_at      INTEGER
);

-- spawn_runs: ephemeral agent lifecycle tracking
CREATE TABLE spawn_runs (
    id           TEXT PRIMARY KEY,   -- session_id
    parent_id    TEXT NOT NULL,
    child_id     TEXT NOT NULL,
    status       TEXT NOT NULL DEFAULT 'running',
    result       TEXT,
    created_at   INTEGER NOT NULL,
    expires_at   INTEGER NOT NULL,
    completed_at INTEGER
);

-- shared_state: key-value store
-- message_sequences: per-agent monotonic seq
-- sender_sequences: per-agent sender seq (Phase 3B)
```

---

## User Stories

1. **"I open the dashboard and see which agents are alive, stuck, or quarantined."**
   - Operator opens `/ipc/fleet` → sees table of all agents with status, trust, last_seen
   - Quarantined agents have red badge, ephemeral agents have "eph" marker
   - Last_seen > staleness threshold → "stale" indicator

2. **"I see a stuck spawn and kill the child agent."**
   - Operator opens `/ipc/spawns` → sees running spawn that's been going for 20 minutes
   - Clicks "Revoke" → confirmation dialog → child token revoked, spawn_run → timeout
   - Parent's next poll gets error, can re-spawn

3. **"A quarantined message arrives. I review it and promote to the target agent's inbox."**
   - Operator opens `/ipc/quarantine` → sees message from L4 agent
   - Reads payload preview, clicks "Inspect" for full view
   - Clicks "Promote" → confirmation: "This will deliver to {target}'s inbox" → confirms
   - Message appears in target agent's normal inbox

4. **"After an incident, I trace a suspicious message back to its origin."**
   - Operator opens `/ipc/sessions` → filters by session_id from the incident
   - Sees full message timeline: who sent what, trust levels, which were blocked
   - Clicks message → sees seq number, delivery lane, trust levels
   - Jumps to agent detail → sees spawn history, trust changes

5. **"I verify the audit chain hasn't been tampered with."**
   - Operator opens `/ipc/audit` → clicks "Verify Chain"
   - Backend recomputes HMAC chain → returns OK or shows first break point
   - Operator exports filtered events as JSON for external review

---

## Primary Workflow

```
Operator                           Web UI                         Backend
────────                           ──────                         ───────

Opens browser ──────────────────> /ipc/fleet
                                   │
                                   │ GET /admin/ipc/agents ──────> list_agents()
                                   │
                                   │ <── agents table ────────────┘
                                   │
Sees "eph-opus-a1b2" quarantined  │
                                   │
Clicks agent ───────────────────> /ipc/fleet/eph-opus-a1b2
                                   │
                                   │ GET /admin/ipc/agents/
                                   │   eph-opus-a1b2/detail ─────> agent_detail()
                                   │                               → recent messages
                                   │                               → active spawns
                                   │                               → quarantine count
                                   │ <── detail response ─────────┘
                                   │
Sees suspicious message in queue  │
                                   │
Clicks "View Quarantine" ───────> /ipc/quarantine
                                   │
                                   │ GET /admin/ipc/messages
                                   │   ?quarantine=true ─────────> list_messages_admin()
                                   │
                                   │ <── quarantine messages ─────┘
                                   │
Reviews message, clicks "Promote" │
                                   │ [Confirmation dialog]
                                   │
Confirms ──────────────────────> POST /admin/ipc/promote
                                   │   { message_id, to_agent } ──> promote_message()
                                   │
                                   │ <── { promoted: true } ──────┘
                                   │
Sees success toast                │
```

---

## Screens

### 1. Fleet Overview (`/ipc/fleet`)

**Purpose**: Answer "is the system healthy?" in one glance.

**Data source**: `GET /admin/ipc/agents` (existing)

**Table columns**:

| Column | Source | Rendering |
|--------|--------|-----------|
| agent_id | agents.agent_id | Clickable link → Agent Detail |
| role | agents.role | Text |
| trust | agents.trust_level | Color badge: L0-1=green, L2=blue, L3=yellow, L4+=red |
| status | agents.status | Badge: online=green, disabled=gray, revoked=red, quarantined=orange, ephemeral=purple |
| last_seen | agents.last_seen | Relative time ("3m ago"), red if stale |
| key | agents.public_key | Icon: shield-check (registered) or shield-off (unsigned) |

**Actions per row**:
- Revoke → `POST /admin/ipc/revoke` (confirmation)
- Quarantine → `POST /admin/ipc/quarantine` (confirmation)
- Disable → `POST /admin/ipc/disable` (confirmation)
- Downgrade → `POST /admin/ipc/downgrade` (level picker + confirmation)

**Empty state**: "No agents registered. Pair an agent to get started."

**Refresh**: Poll every 10s or SSE subscription.

### 2. Agent Detail (`/ipc/fleet/:agentId`)

**Purpose**: Deep-dive into one agent.

**Data source**: `GET /admin/ipc/agents/:id/detail` (new endpoint)

**Sections**:

**Identity card** (top):
- agent_id, role, trust_level badge, status badge
- public_key (first 16 chars + "..."), "registered" / "unsigned"
- First seen / last seen (absolute + relative)
- Ephemeral marker (if applicable): parent, session_id, expires_at

**Recent messages** (middle, collapsible):
- Last 20 messages sent or received by this agent
- Columns: timestamp, direction (→/←), peer, kind, payload preview, lane
- Click → expand full payload

**Active spawn runs** (bottom, collapsible):
- Spawn runs where this agent is parent or child
- Columns: session_id, role (parent/child), peer, status, created_at, expires_at

**Actions**: Same as Fleet Overview (revoke, quarantine, disable, downgrade).

### 3. Session Inspector (`/ipc/sessions`)

**Purpose**: Debug IPC message flow. Primary investigation tool.

**Data source**: `GET /admin/ipc/messages` (new endpoint)

**Filters** (top bar):
- agent_id (text input, filters from or to)
- session_id (text input)
- kind (dropdown: all / task / query / result / text)
- lane (dropdown: all / normal / quarantine / blocked)
- time range (date pickers or quick: 1h / 24h / 7d)

**Timeline** (main area):

| Column | Rendering |
|--------|-----------|
| timestamp | Absolute + relative |
| from → to | agent_id badges with trust color |
| kind | Color badge: task=blue, query=purple, result=green, text=gray |
| lane | normal=none, quarantine=orange dot, blocked=red dot |
| seq | Mono font, small |
| payload | First 200 chars, click to expand |

**Note on signatures**: The broker verifies Ed25519 signatures on message receipt but does **not persist** signature data or verification results in the messages table. Historical messages cannot show "verified/unsigned/failed" status without a schema migration. Phase 3.5 v1 does not add signature columns — if provenance per-message is needed in the UI, a future step should add `signature_verified BOOLEAN` column to the messages table and populate it at INSERT time.

**Expand** (click row):
- Full payload (redacted by default, raw toggle)
- Metadata: session_id, priority, blocked_reason, promoted flag
- Sender's public key status (from agents table: registered / not registered)
- Links: jump to from-agent detail, to-agent detail, parent spawn

**Pagination**: 50 per page, load more.

### 4. Spawn Monitor (`/ipc/spawns`)

**Purpose**: Track ephemeral agent lifecycle.

**Data source**: `GET /admin/ipc/spawn-runs` (new endpoint)

**Filters**:
- status (dropdown: all / running / completed / timeout / revoked / interrupted)
- parent (text input)
- time range

**Table**:

| Column | Rendering |
|--------|-----------|
| session_id | Clickable → Session Inspector filtered by this session |
| parent | agent_id badge with trust |
| child | agent_id badge with trust + ephemeral marker |
| status | Badge: running=blue pulse, completed=green, timeout=orange, revoked=red, interrupted=gray |
| created_at | Absolute |
| expires_at | Relative ("in 3m" or "expired 5m ago") |
| completed_at | Absolute or "-" |
| result | First 100 chars preview, click to expand |

**Actions per row**:
- Revoke child (if running) → `POST /admin/ipc/revoke` (confirmation)
- View result (if completed) → expand panel
- View messages → jump to Session Inspector with session_id filter

**Running spawns** highlighted at top.

### 5. Quarantine Review (`/ipc/quarantine`)

**Purpose**: Operator review queue for L4+ messages. Separate from normal flow.

**Data source**: `GET /admin/ipc/messages?quarantine=true&dismissed=false` (new endpoint)

**Queue display**:

| Column | Rendering |
|--------|-----------|
| message_id | # |
| from | agent_id badge with trust (always L4+ = red) |
| to | target agent_id |
| kind | Badge |
| payload | First 200 chars, **redacted by default** |
| age | Relative time since created_at |
| status | pending / promoted / dismissed |

**Actions per message**:
- **Inspect** → modal with full payload (raw toggle for secrets)
- **Promote** → `POST /admin/ipc/promote` (confirmation: "Deliver to {to}'s inbox?")
- **Dismiss** → `POST /admin/ipc/dismiss-message` (confirmation: "Mark as reviewed without delivering?")

**UX rule**: No bulk actions. Each quarantine message reviewed individually.

**Counts**: Show "N pending" in sidebar badge for quick visibility.

### 6. Audit Viewer (`/ipc/audit`)

**Purpose**: Investigation and compliance.

**Data source**: `GET /admin/ipc/audit` (new endpoint)

**Filters**:
- agent_id (text input)
- event_type (dropdown: all / IpcSent / IpcReceived / IpcBlocked / IpcAdminAction)
- time range
- full-text search in detail field

**Event stream** (reverse chronological):

| Column | Rendering |
|--------|-----------|
| timestamp | Absolute |
| event_type | Color badge |
| actor | agent_id or "broker" / "admin" |
| target | affected agent_id (if applicable) |
| detail | Human-readable summary |
| hmac | Chain icon: intact (green) or first entry (gray) |

**Expand** (click row):
- Full event JSON
- Previous/next in chain
- Links to related agent, session, message

**Actions**:
- **Verify Chain** button → `POST /admin/ipc/audit/verify` → result toast (OK / break at event N)
- **Export** button → download filtered events as JSON

---

## Implementation Steps

### Step 0: Backend — admin read endpoints

**Files**: `src/gateway/ipc.rs`, `src/gateway/mod.rs`

**What**:
- Add `IpcDb::list_messages_admin()` — paginated, filterable query:
  - Params: agent_id, session_id, kind, quarantine (bool), dismissed (bool), limit, offset
  - Does NOT set `read=1` or update `last_seen`
  - Returns messages with computed field: lane (normal/quarantine/blocked)
  - **Quarantine queue contract**: a message is in the quarantine lane when `from_trust_level >= 4`. Within quarantine:
    - **pending** = `promoted=0 AND blocked=0` (not yet reviewed)
    - **promoted** = `promoted=1` (delivered to target inbox via `/admin/ipc/promote`)
    - **dismissed** = `blocked=1 AND blocked_reason='dismissed'` (reviewed but not delivered via `/admin/ipc/dismiss-message`)
  - Filter `dismissed=false` returns pending + promoted (excluding dismissed). Default for Quarantine Review page.
  - Filter `dismissed=true` returns only dismissed items (for audit trail)
- Add `IpcDb::list_spawn_runs_admin()` — paginated, filterable:
  - Params: status, parent_id, limit, offset
- Add `IpcDb::agent_detail()` — single agent + recent messages + active spawn runs
- Add `IpcDb::list_audit_events()` — read from audit log file, paginated:
  - Params: agent_id, event_type, from_ts, to_ts, search, limit, offset
- Add `IpcDb::dismiss_message()` — set `blocked=1, blocked_reason="dismissed"` on quarantine message

**Handlers**:
- `handle_admin_ipc_agent_detail()` → `GET /admin/ipc/agents/:id/detail`
- `handle_admin_ipc_messages()` → `GET /admin/ipc/messages`
- `handle_admin_ipc_spawn_runs()` → `GET /admin/ipc/spawn-runs`
- `handle_admin_ipc_audit()` → `GET /admin/ipc/audit`
- `handle_admin_ipc_audit_verify()` → `POST /admin/ipc/audit/verify`
- `handle_admin_ipc_dismiss_message()` → `POST /admin/ipc/dismiss-message`

**Routes** (add to `mod.rs`):
```rust
.route("/admin/ipc/agents/:id/detail", get(ipc::handle_admin_ipc_agent_detail))
.route("/admin/ipc/messages", get(ipc::handle_admin_ipc_messages))
.route("/admin/ipc/spawn-runs", get(ipc::handle_admin_ipc_spawn_runs))
.route("/admin/ipc/audit", get(ipc::handle_admin_ipc_audit))
.route("/admin/ipc/audit/verify", post(ipc::handle_admin_ipc_audit_verify))
.route("/admin/ipc/dismiss-message", post(ipc::handle_admin_ipc_dismiss_message))
```

**Tests**: Unit tests for each IpcDb query method. HTTP handler tests using existing test harness (test_app_state + start_test_server pattern).

**Verify**: `cargo check`, `cargo test`, `cargo clippy`

---

### Step 1: Frontend — types, API client, sidebar

**Files**: `web/src/types/ipc.ts` (new), `web/src/lib/ipc-api.ts` (new), `web/src/components/layout/Sidebar.tsx`, `web/src/App.tsx`

**What**:
- TypeScript interfaces for all IPC entities:
  ```typescript
  interface IpcAgent {
    agent_id: string;
    role: string;
    trust_level: number;
    status: string;
    last_seen: number;
    public_key: string | null;
    metadata: Record<string, string> | null;
  }

  interface IpcMessage {
    id: number;
    from_agent: string;
    to_agent: string;
    kind: string;
    payload: string;
    from_trust_level: number;
    session_id: string | null;
    priority: number;
    read: boolean;
    promoted: boolean;
    blocked: boolean;
    blocked_reason: string | null;
    seq: number | null;
    created_at: number;
    lane: 'normal' | 'quarantine' | 'blocked';
  }

  interface IpcSpawnRun {
    id: string;           // session_id
    parent_id: string;
    child_id: string;
    status: string;
    result: string | null;
    created_at: number;
    expires_at: number;
    completed_at: number | null;
  }

  interface IpcAuditEvent {
    timestamp: number;
    event_type: string;
    actor: string;
    target: string | null;
    detail: string;
    hmac: string | null;
  }

  interface IpcAgentDetail {
    agent: IpcAgent;
    recent_messages: IpcMessage[];
    active_spawns: IpcSpawnRun[];
    quarantine_count: number;
  }
  ```
- API client functions: `fetchFleet()`, `fetchAgentDetail(id)`, `fetchMessages(filters)`, `fetchSpawnRuns(filters)`, `fetchAudit(filters)`, `revokeAgent(id)`, `quarantineAgent(id)`, `disableAgent(id)`, `downgradeAgent(id, level)`, `promoteMessage(id, to)`, `dismissMessage(id)`, `verifyAuditChain()`
- Sidebar: add "IPC" section with 5 links (Fleet, Sessions, Spawns, Quarantine, Audit)
  - Quarantine link shows pending count badge
- Router: add routes for all 6 IPC pages

**Verify**: `cd web && npm run build`

---

### Step 2: Frontend — shared IPC components

**Files**: `web/src/components/ipc/` (new directory)

**What**:
- `TrustBadge.tsx` — color-coded trust level badge (L0-1=green, L2=blue, L3=yellow, L4+=red)
- `StatusBadge.tsx` — agent status badge (online=green, disabled=gray, revoked=red, quarantined=orange, ephemeral=purple)
- `KindBadge.tsx` — message kind badge (task=blue, query=purple, result=green, text=gray)
- `LaneDot.tsx` — delivery lane indicator (normal=none, quarantine=orange, blocked=red)
- `KeyStatusIcon.tsx` — agent has registered public key or not (from agents table)
- `ConfirmDialog.tsx` — reusable confirmation modal for destructive actions
- `MessageDetail.tsx` — expandable message view with redacted/raw toggle
- `TimeAgo.tsx` — relative timestamp component ("3m ago", "2h ago")
- `AgentLink.tsx` — clickable agent_id with trust badge, links to Agent Detail

These components are shared across all IPC pages for consistent rendering.

**Verify**: `cd web && npm run build`

---

### Step 3: Frontend — Fleet Overview page

**Files**: `web/src/pages/ipc/Fleet.tsx`

**What**:
- Page component that fetches `GET /admin/ipc/agents` on mount
- Renders agent table with all columns from Screen 1 spec
- Action buttons per row (revoke, quarantine, disable, downgrade)
- Each action opens ConfirmDialog before executing
- Auto-refresh every 10s via `setInterval` + refetch
- Error/loading/empty states following existing page patterns
- Glass-card container with "Fleet Overview" title + agent count

**Verify**: `cd web && npm run build`, manual browser test

---

### Step 4: Frontend — Agent Detail page

**Files**: `web/src/pages/ipc/AgentDetail.tsx`

**What**:
- Route: `/ipc/fleet/:agentId`
- Fetches `GET /admin/ipc/agents/:id/detail` on mount
- Identity card section (top): all agent fields + key status
- Recent messages section (collapsible): table with expand-on-click
- Active spawns section (collapsible): table with status badges
- Action buttons (same as Fleet)
- Cross-links: message rows link to Session Inspector, spawn rows link to Spawn Monitor

**Verify**: `cd web && npm run build`, manual browser test

---

### Step 5: Frontend — Session Inspector page

**Files**: `web/src/pages/ipc/Sessions.tsx`

**What**:
- Filter bar (top): agent_id input, session_id input, kind dropdown, lane dropdown, time range
- Fetches `GET /admin/ipc/messages` with filter params on filter change
- Timeline table with all columns from Screen 3 spec
- Click row → expand inline with MessageDetail component
- Cross-links: agent_ids link to Agent Detail, session_ids to Spawn Monitor
- Pagination: "Load more" button at bottom
- URL query params sync (shareable filtered views)

**Verify**: `cd web && npm run build`, manual browser test

---

### Step 6: Frontend — Spawn Monitor page

**Files**: `web/src/pages/ipc/Spawns.tsx`

**What**:
- Filter bar: status dropdown, parent input, time range
- Fetches `GET /admin/ipc/spawn-runs` with filter params
- Table with all columns from Screen 4 spec
- Running spawns highlighted (pulsing blue badge)
- Revoke action (if status=running) with confirmation
- Expand row → result payload preview
- Cross-links: parent/child → Agent Detail, session_id → Session Inspector

**Verify**: `cd web && npm run build`, manual browser test

---

### Step 7: Frontend — Quarantine Review page

**Files**: `web/src/pages/ipc/Quarantine.tsx`

**What**:
- Fetches `GET /admin/ipc/messages?quarantine=true&dismissed=false`
- Queue display with all columns from Screen 5 spec
- Payload **redacted by default** (first 200 chars of sanitized text)
- "Inspect" button → modal with full payload + raw toggle
- "Promote" button → ConfirmDialog → `POST /admin/ipc/promote`
- "Dismiss" button → ConfirmDialog → `POST /admin/ipc/dismiss-message`
- Pending count shown in sidebar badge (polled)
- No bulk actions — each message reviewed individually
- Success/error toasts after actions

**Verify**: `cd web && npm run build`, manual browser test

---

### Step 8: Frontend — Audit Viewer page

**Files**: `web/src/pages/ipc/Audit.tsx`

**What**:
- Filter bar: agent_id, event_type dropdown, time range, search input
- Fetches `GET /admin/ipc/audit` with filter params
- Event stream table (reverse chronological) with all columns from Screen 6 spec
- Click row → expand full event JSON
- "Verify Chain" button → `POST /admin/ipc/audit/verify` → toast with result
- "Export JSON" button → download filtered events as .json file
- Pagination: load more

**Verify**: `cd web && npm run build`, manual browser test

---

### Step 9: Cross-links, polish, integration tests

**Files**: all IPC pages + components

**What**:
- Ensure all agent_id references are clickable AgentLink components
- Ensure all session_id references link to Session Inspector with filter
- Loading skeletons instead of spinners for tables
- Keyboard navigation: Escape closes modals
- Mobile-responsive: stack table columns on small screens
- Integration test: start gateway, open each IPC page, verify data renders
- Screenshot comparison (manual) against design spec

**Verify**: Full CI (`./dev/ci.sh all`), manual browser walkthrough of all 6 pages

---

## File Structure

```
src/gateway/
├── ipc.rs              # Add: 6 new handlers, 5 new IpcDb query methods
└── mod.rs              # Add: 6 new routes

web/src/
├── pages/
│   └── ipc/            # NEW
│       ├── Fleet.tsx
│       ├── AgentDetail.tsx
│       ├── Sessions.tsx
│       ├── Spawns.tsx
│       ├── Quarantine.tsx
│       └── Audit.tsx
├── components/
│   └── ipc/            # NEW
│       ├── TrustBadge.tsx
│       ├── StatusBadge.tsx
│       ├── KindBadge.tsx
│       ├── LaneDot.tsx
│       ├── KeyStatusIcon.tsx
│       ├── ConfirmDialog.tsx
│       ├── MessageDetail.tsx
│       ├── TimeAgo.tsx
│       └── AgentLink.tsx
├── types/
│   └── ipc.ts          # NEW
├── lib/
│   └── ipc-api.ts      # NEW
└── App.tsx             # Add IPC routes
```

---

## Verification

### Per step
1. `cargo fmt --all -- --check` — Rust formatting
2. `cargo clippy --all-targets -- -D warnings` — Rust lints
3. `cargo test` — backend unit tests pass
4. `cd web && npm run build` — frontend compiles
5. Manual browser verification of implemented page

### Final (after Step 9)
1. All 6 IPC pages accessible from sidebar
2. Fleet Overview shows all agents with correct status/trust
3. Agent Detail shows messages + spawns + actions work
4. Session Inspector filters work, messages expand correctly
5. Spawn Monitor shows lifecycle, revoke works on running spawns
6. Quarantine Review: inspect → promote/dismiss workflow complete
7. Audit Viewer: filters, expand, verify chain, export work
8. All destructive actions have confirmation dialogs
9. Trust badges visible everywhere
10. Cross-links navigate correctly between pages
11. No secrets visible without explicit raw toggle
12. Quarantine count badge in sidebar updates

---

## Risk

| Risk | Impact | Mitigation |
|------|--------|------------|
| Admin reads mutate state | Data corruption, unexpected inbox consumption | AD-2: separate `_admin` query methods, no `read=1` |
| Quarantine leaks into normal flow | Untrusted content auto-processed | AD-3: separate page, explicit promote with confirmation |
| Secrets leak in UI | Token/payload exposure | Server-side redaction + client-side raw toggle |
| Scope creep into policy editor | Delays delivery | Non-goal, defer to Phase 4 |
| Admin endpoints exposed remotely | Unauthorized access | `require_localhost()` guard on every admin handler |
| Frontend bundle size bloat | Slow initial load | 6 pages ≈ 30KB gzipped, negligible vs existing 10 pages |

---

## Dependencies

**Required (done)**:
- Phase 1: brokered coordination (agents, messages, shared_state)
- Phase 2: broker-side safety (ACL, quarantine lanes, rate limiting)
- Phase 3A: trusted execution (spawn_runs, ephemeral lifecycle)
- Phase 3B: crypto hardening (signatures, audit chain, replay protection)

**Not required**:
- Phase 4 (federated execution)
- External services or databases

---

## Recommended order

1. **Step 0** — backend endpoints (Rust, ~300 lines)
2. **Step 1** — types + API client + sidebar (TypeScript, ~200 lines)
3. **Step 2** — shared components (React, ~300 lines)
4. **Step 3** — Fleet Overview (first visible result)
5. **Step 4** — Agent Detail (drill-down from Fleet)
6. **Step 5** — Session Inspector (primary debug tool)
7. **Step 6** — Spawn Monitor
8. **Step 7** — Quarantine Review (critical for operations)
9. **Step 8** — Audit Viewer
10. **Step 9** — Cross-links + polish

Steps 0-3 can be done in one PR. Steps 4-8 can be individual PRs or grouped by 2-3.

---

## What's NOT in Phase 3.5

- Real-time WebSocket push for agent status changes (can add later, polling is fine for v1)
- Visual policy editor / rule builder
- Multi-host fleet view (Phase 4)
- Approval workflow UI (future — currently approvals go through agent chat)
- Mobile app
- Dark/light theme toggle (dark only, matching existing UI)
