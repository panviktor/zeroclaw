//! IPC broker handlers for inter-agent communication.
//!
//! All IPC communication is broker-mediated: agents authenticate with bearer
//! tokens, and the broker resolves trust levels from token metadata. The broker
//! owns the SQLite database — agents never access it directly.

use super::{require_localhost, AppState};
use crate::config::TokenMetadata;
use crate::gateway::api::extract_bearer_token;
use axum::{
    extract::{ConnectInfo, Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use parking_lot::Mutex;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{info, warn};

// ── IpcDb (broker-owned SQLite) ─────────────────────────────────

/// Broker-owned SQLite database for IPC messages, agent registry, and shared state.
///
/// Initialized when `agents_ipc.enabled = true`. The database is WAL-mode
/// and only accessible by the broker process.
pub struct IpcDb {
    conn: Arc<Mutex<Connection>>,
}

impl IpcDb {
    /// Open (or create) the IPC database at the given path.
    pub fn open(path: &Path) -> Result<Self, rusqlite::Error> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "busy_timeout", 5000)?;
        Self::init_schema(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Open an in-memory database (for testing).
    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self, rusqlite::Error> {
        let conn = Connection::open_in_memory()?;
        Self::init_schema(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    fn init_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS agents (
                agent_id    TEXT PRIMARY KEY,
                role        TEXT,
                trust_level INTEGER NOT NULL DEFAULT 3,
                status      TEXT DEFAULT 'online',
                metadata    TEXT,
                last_seen   INTEGER
            );

            CREATE TABLE IF NOT EXISTS messages (
                id               INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id       TEXT,
                from_agent       TEXT NOT NULL,
                to_agent         TEXT NOT NULL,
                kind             TEXT NOT NULL DEFAULT 'text',
                payload          TEXT NOT NULL,
                priority         INTEGER DEFAULT 0,
                from_trust_level INTEGER NOT NULL,
                seq              INTEGER NOT NULL,
                blocked          INTEGER DEFAULT 0,
                block_reason     TEXT,
                created_at       INTEGER NOT NULL,
                read             INTEGER DEFAULT 0,
                expires_at       INTEGER
            );

            CREATE INDEX IF NOT EXISTS idx_messages_inbox
                ON messages(to_agent, read, created_at);
            CREATE INDEX IF NOT EXISTS idx_messages_session
                ON messages(session_id) WHERE session_id IS NOT NULL;

            CREATE TABLE IF NOT EXISTS shared_state (
                key        TEXT PRIMARY KEY,
                value      TEXT NOT NULL,
                owner      TEXT NOT NULL,
                updated_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS message_sequences (
                agent_id TEXT PRIMARY KEY,
                last_seq INTEGER NOT NULL DEFAULT 0
            );
            ",
        )
    }

    /// Upsert agent record and update `last_seen` timestamp.
    pub fn update_last_seen(&self, agent_id: &str, trust_level: u8, role: &str) {
        let now = unix_now();
        let conn = self.conn.lock();
        let _ = conn.execute(
            "INSERT INTO agents (agent_id, trust_level, role, last_seen)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(agent_id) DO UPDATE SET
                trust_level = ?2, role = ?3, last_seen = ?4, status = 'online'",
            params![agent_id, trust_level, role, now],
        );
    }

    /// Check whether a session contains a task directed at the given agent.
    pub fn session_has_task_for(&self, session_id: &str, agent_id: &str) -> bool {
        let conn = self.conn.lock();
        conn.query_row(
            "SELECT COUNT(*) FROM messages
             WHERE session_id = ?1 AND to_agent = ?2 AND kind = 'task' AND blocked = 0",
            params![session_id, agent_id],
            |row| row.get::<_, i64>(0),
        )
        .unwrap_or(0)
            > 0
    }

    /// Allocate the next monotonic sequence number for a sender.
    pub fn next_seq(&self, agent_id: &str) -> i64 {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO message_sequences (agent_id, last_seq) VALUES (?1, 1)
             ON CONFLICT(agent_id) DO UPDATE SET last_seq = last_seq + 1",
            params![agent_id],
        )
        .ok();
        conn.query_row(
            "SELECT last_seq FROM message_sequences WHERE agent_id = ?1",
            params![agent_id],
            |row| row.get(0),
        )
        .unwrap_or(1)
    }

    /// Insert a message into the database.
    pub fn insert_message(
        &self,
        from_agent: &str,
        to_agent: &str,
        kind: &str,
        payload: &str,
        from_trust_level: u8,
        session_id: Option<&str>,
        priority: i32,
        message_ttl_secs: Option<u64>,
    ) -> Result<i64, rusqlite::Error> {
        let now = unix_now();
        let seq = self.next_seq(from_agent);
        let expires_at = message_ttl_secs.map(|ttl| now + ttl as i64);
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO messages (session_id, from_agent, to_agent, kind, payload,
             priority, from_trust_level, seq, created_at, expires_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                session_id,
                from_agent,
                to_agent,
                kind,
                payload,
                priority,
                from_trust_level,
                seq,
                now,
                expires_at
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Fetch unread messages for an agent, optionally including quarantine.
    pub fn fetch_inbox(
        &self,
        agent_id: &str,
        include_quarantine: bool,
        limit: u32,
    ) -> Vec<InboxMessage> {
        let now = unix_now();
        let conn = self.conn.lock();
        // Lazy TTL cleanup
        let _ = conn.execute(
            "DELETE FROM messages WHERE expires_at IS NOT NULL AND expires_at < ?1",
            params![now],
        );
        let min_trust = if include_quarantine { 0 } else { -1 };
        let _ = min_trust; // quarantine = from_trust_level >= 4
        let query = if include_quarantine {
            "SELECT id, session_id, from_agent, to_agent, kind, payload, priority,
                    from_trust_level, seq, created_at
             FROM messages
             WHERE to_agent = ?1 AND read = 0 AND blocked = 0
             ORDER BY priority DESC, created_at ASC
             LIMIT ?2"
        } else {
            "SELECT id, session_id, from_agent, to_agent, kind, payload, priority,
                    from_trust_level, seq, created_at
             FROM messages
             WHERE to_agent = ?1 AND read = 0 AND blocked = 0 AND from_trust_level < 4
             ORDER BY priority DESC, created_at ASC
             LIMIT ?2"
        };
        let mut stmt = match conn.prepare(query) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = stmt
            .query_map(params![agent_id, limit], |row| {
                Ok(InboxMessage {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    from_agent: row.get(2)?,
                    to_agent: row.get(3)?,
                    kind: row.get(4)?,
                    payload: row.get(5)?,
                    priority: row.get(6)?,
                    from_trust_level: row.get(7)?,
                    seq: row.get(8)?,
                    created_at: row.get(9)?,
                })
            })
            .ok();
        let messages: Vec<InboxMessage> = rows
            .map(|r| r.filter_map(|m| m.ok()).collect())
            .unwrap_or_default();
        // Mark as read
        let ids: Vec<i64> = messages.iter().map(|m| m.id).collect();
        for id in &ids {
            let _ = conn.execute("UPDATE messages SET read = 1 WHERE id = ?1", params![id]);
        }
        messages
    }

    /// List known agents with staleness check.
    pub fn list_agents(&self, staleness_secs: u64) -> Vec<AgentInfo> {
        let now = unix_now();
        let conn = self.conn.lock();
        let mut stmt = match conn.prepare(
            "SELECT agent_id, role, trust_level, status, last_seen FROM agents ORDER BY agent_id",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        stmt.query_map([], |row| {
            let last_seen: Option<i64> = row.get(4)?;
            let status: String = row.get(3)?;
            let effective_status = if status == "online" {
                match last_seen {
                    Some(ts) if (now - ts) > staleness_secs as i64 => "stale".to_string(),
                    _ => status,
                }
            } else {
                status
            };
            Ok(AgentInfo {
                agent_id: row.get(0)?,
                role: row.get(1)?,
                trust_level: row.get(2)?,
                status: effective_status,
                last_seen,
            })
        })
        .ok()
        .map(|r| r.filter_map(|a| a.ok()).collect())
        .unwrap_or_default()
    }

    /// Get a shared state value.
    pub fn get_state(&self, key: &str) -> Option<StateEntry> {
        let conn = self.conn.lock();
        conn.query_row(
            "SELECT key, value, owner, updated_at FROM shared_state WHERE key = ?1",
            params![key],
            |row| {
                Ok(StateEntry {
                    key: row.get(0)?,
                    value: row.get(1)?,
                    owner: row.get(2)?,
                    updated_at: row.get(3)?,
                })
            },
        )
        .ok()
    }

    /// Upsert a shared state value.
    pub fn set_state(&self, key: &str, value: &str, owner: &str) {
        let now = unix_now();
        let conn = self.conn.lock();
        let _ = conn.execute(
            "INSERT INTO shared_state (key, value, owner, updated_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(key) DO UPDATE SET value = ?2, owner = ?3, updated_at = ?4",
            params![key, value, owner, now],
        );
    }

    /// Set agent status (for admin disable/quarantine).
    pub fn set_agent_status(&self, agent_id: &str, status: &str) -> bool {
        let conn = self.conn.lock();
        let changed = conn
            .execute(
                "UPDATE agents SET status = ?2 WHERE agent_id = ?1",
                params![agent_id, status],
            )
            .unwrap_or(0);
        changed > 0
    }

    /// Set agent trust level (for admin downgrade).
    pub fn set_agent_trust_level(&self, agent_id: &str, new_level: u8) -> Option<u8> {
        let conn = self.conn.lock();
        let current: u8 = conn
            .query_row(
                "SELECT trust_level FROM agents WHERE agent_id = ?1",
                params![agent_id],
                |row| row.get(0),
            )
            .ok()?;
        // Can only downgrade (increase number)
        if new_level <= current {
            return None;
        }
        conn.execute(
            "UPDATE agents SET trust_level = ?2 WHERE agent_id = ?1",
            params![agent_id, new_level],
        )
        .ok();
        Some(current)
    }

    /// Block pending messages for an agent (used by revoke/disable).
    pub fn block_pending_messages(&self, agent_id: &str, reason: &str) {
        let conn = self.conn.lock();
        let _ = conn.execute(
            "UPDATE messages SET blocked = 1, block_reason = ?2
             WHERE to_agent = ?1 AND read = 0 AND blocked = 0",
            params![agent_id, reason],
        );
    }
}

// ── Request/Response types ──────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct SendBody {
    pub to: String,
    #[serde(default = "default_kind")]
    pub kind: String,
    pub payload: String,
    pub session_id: Option<String>,
    #[serde(default)]
    pub priority: i32,
}

fn default_kind() -> String {
    "text".into()
}

#[derive(Debug, Deserialize)]
pub struct InboxQuery {
    #[serde(default)]
    pub quarantine: bool,
    #[serde(default = "default_inbox_limit")]
    pub limit: u32,
}

fn default_inbox_limit() -> u32 {
    50
}

#[derive(Debug, Serialize)]
pub struct InboxMessage {
    pub id: i64,
    pub session_id: Option<String>,
    pub from_agent: String,
    pub to_agent: String,
    pub kind: String,
    pub payload: String,
    pub priority: i32,
    pub from_trust_level: u8,
    pub seq: i64,
    pub created_at: i64,
}

#[derive(Debug, Serialize)]
pub struct AgentInfo {
    pub agent_id: String,
    pub role: Option<String>,
    pub trust_level: u8,
    pub status: String,
    pub last_seen: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct StateGetQuery {
    pub key: String,
}

#[derive(Debug, Deserialize)]
pub struct StateSetBody {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Serialize)]
pub struct StateEntry {
    pub key: String,
    pub value: String,
    pub owner: String,
    pub updated_at: i64,
}

#[derive(Debug, Deserialize)]
pub struct AdminAgentBody {
    pub agent_id: String,
}

#[derive(Debug, Deserialize)]
pub struct AdminDowngradeBody {
    pub agent_id: String,
    pub new_level: u8,
}

// ── ACL validation ──────────────────────────────────────────────

/// Allowed message kinds.
const VALID_KINDS: &[&str] = &["text", "task", "result", "query", "notify"];

/// Validate whether a send operation is permitted by the ACL rules.
///
/// Rules:
/// 0. Kind must be in the whitelist.
/// 1. L4 agents can only send `text`.
/// 2. `task` cannot be sent upward (to lower trust_level number = higher trust).
/// 3. `result` requires a correlated task in the same session.
/// 4. L4↔L4 direct messaging is denied (must go through a higher-trust agent).
/// 5. L3 lateral `text` requires an explicit allowlist entry.
pub fn validate_send(
    from_level: u8,
    to_level: u8,
    kind: &str,
    from_agent: &str,
    to_agent: &str,
    session_id: Option<&str>,
    lateral_text_pairs: &[[String; 2]],
    l4_destinations: &[String],
    db: &IpcDb,
) -> Result<(), IpcError> {
    // Rule 0: kind whitelist
    if !VALID_KINDS.contains(&kind) {
        return Err(IpcError {
            status: StatusCode::BAD_REQUEST,
            error: format!("Invalid message kind: {kind}"),
            code: "invalid_kind".into(),
            retryable: false,
        });
    }

    // Rule 1: L4 can only send text
    if from_level >= 4 && kind != "text" {
        return Err(IpcError {
            status: StatusCode::FORBIDDEN,
            error: "Restricted agents can only send text".into(),
            code: "l4_text_only".into(),
            retryable: false,
        });
    }

    // L4 destination whitelist
    if from_level >= 4 && !l4_destinations.contains(&to_agent.to_string()) {
        return Err(IpcError {
            status: StatusCode::FORBIDDEN,
            error: "Destination not in L4 allowlist".into(),
            code: "l4_destination_denied".into(),
            retryable: false,
        });
    }

    // Rule 2: task cannot be sent upward
    if kind == "task" && to_level < from_level {
        return Err(IpcError {
            status: StatusCode::FORBIDDEN,
            error: "Cannot assign tasks to higher-trust agents".into(),
            code: "task_upward_denied".into(),
            retryable: false,
        });
    }

    // Rule 2b: task cannot be sent to same level
    if kind == "task" && to_level == from_level {
        return Err(IpcError {
            status: StatusCode::FORBIDDEN,
            error: "Cannot assign tasks to same-trust agents".into(),
            code: "task_lateral_denied".into(),
            retryable: false,
        });
    }

    // Rule 3: result requires correlated task
    if kind == "result" {
        match session_id {
            Some(sid) if db.session_has_task_for(sid, from_agent) => {}
            _ => {
                return Err(IpcError {
                    status: StatusCode::FORBIDDEN,
                    error: "Result requires a correlated task in the same session".into(),
                    code: "result_no_task".into(),
                    retryable: false,
                });
            }
        }
    }

    // Rule 4: L4↔L4 denied
    if from_level >= 4 && to_level >= 4 {
        return Err(IpcError {
            status: StatusCode::FORBIDDEN,
            error: "L4 agents cannot message each other directly".into(),
            code: "l4_lateral_denied".into(),
            retryable: false,
        });
    }

    // Rule 5: L3 lateral text requires allowlist
    if from_level == 3 && to_level == 3 && kind == "text" {
        let pair_allowed = lateral_text_pairs.iter().any(|pair| {
            (pair[0] == from_agent && pair[1] == to_agent)
                || (pair[0] == to_agent && pair[1] == from_agent)
        });
        if !pair_allowed {
            return Err(IpcError {
                status: StatusCode::FORBIDDEN,
                error: "L3 lateral text requires allowlist entry".into(),
                code: "l3_lateral_denied".into(),
                retryable: false,
            });
        }
    }

    Ok(())
}

/// Validate whether a state write is permitted.
///
/// Key format: `{scope}:{owner}:{key}`
/// - L4: only `agent:{self}:*`
/// - L3: + `public:*`
/// - L2: + `team:*`
/// - L1: + `global:*`
/// - `secret:*` denied for all (reserved for Phase 2)
pub fn validate_state_set(trust_level: u8, agent_id: &str, key: &str) -> Result<(), IpcError> {
    let parts: Vec<&str> = key.splitn(3, ':').collect();
    if parts.len() < 2 {
        return Err(IpcError {
            status: StatusCode::BAD_REQUEST,
            error: "Key must be in format scope:owner:key".into(),
            code: "invalid_key_format".into(),
            retryable: false,
        });
    }

    let scope = parts[0];

    if scope == "secret" {
        return Err(IpcError {
            status: StatusCode::FORBIDDEN,
            error: "Secret namespace is reserved".into(),
            code: "secret_denied".into(),
            retryable: false,
        });
    }

    match scope {
        "agent" => {
            let owner = parts.get(1).unwrap_or(&"");
            if *owner != agent_id {
                return Err(IpcError {
                    status: StatusCode::FORBIDDEN,
                    error: "Can only write to own agent namespace".into(),
                    code: "agent_namespace_denied".into(),
                    retryable: false,
                });
            }
        }
        "public" => {
            if trust_level > 3 {
                return Err(IpcError {
                    status: StatusCode::FORBIDDEN,
                    error: "L4 agents cannot write to public namespace".into(),
                    code: "public_denied".into(),
                    retryable: false,
                });
            }
        }
        "team" => {
            if trust_level > 2 {
                return Err(IpcError {
                    status: StatusCode::FORBIDDEN,
                    error: "Only L1-L2 can write to team namespace".into(),
                    code: "team_denied".into(),
                    retryable: false,
                });
            }
        }
        "global" => {
            if trust_level > 1 {
                return Err(IpcError {
                    status: StatusCode::FORBIDDEN,
                    error: "Only L1 can write to global namespace".into(),
                    code: "global_denied".into(),
                    retryable: false,
                });
            }
        }
        _ => {
            return Err(IpcError {
                status: StatusCode::BAD_REQUEST,
                error: format!("Unknown scope: {scope}"),
                code: "unknown_scope".into(),
                retryable: false,
            });
        }
    }

    Ok(())
}

/// Validate whether a state read is permitted.
/// All agents can read all keys except `secret:*` (L0-L1 only).
pub fn validate_state_get(trust_level: u8, key: &str) -> Result<(), IpcError> {
    if key.starts_with("secret:") && trust_level > 1 {
        return Err(IpcError {
            status: StatusCode::FORBIDDEN,
            error: "Secret namespace requires L0-L1".into(),
            code: "secret_read_denied".into(),
            retryable: false,
        });
    }
    Ok(())
}

// ── Auth helper ─────────────────────────────────────────────────

/// Extract and verify bearer token, returning the agent's metadata.
fn require_ipc_auth(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<TokenMetadata, (StatusCode, Json<serde_json::Value>)> {
    let token = extract_bearer_token(headers).ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "error": "Missing Authorization header",
                "code": "missing_auth"
            })),
        )
    })?;

    state.pairing.authenticate(token).ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "error": "Invalid or unknown token",
                "code": "invalid_token"
            })),
        )
    })
}

// ── Structured error ────────────────────────────────────────────

/// Structured IPC error with machine-readable code and retryable hint.
#[derive(Debug, Clone, Serialize)]
pub struct IpcError {
    #[serde(skip)]
    pub status: StatusCode,
    pub error: String,
    pub code: String,
    pub retryable: bool,
}

impl IpcError {
    fn into_response_pair(self, caller_trust: u8) -> (StatusCode, Json<serde_json::Value>) {
        if caller_trust <= 2 {
            (
                self.status,
                Json(serde_json::json!({
                    "error": self.error,
                    "code": self.code,
                    "retryable": self.retryable,
                })),
            )
        } else {
            (
                self.status,
                Json(serde_json::json!({
                    "error": "Forbidden",
                    "code": self.code,
                    "retryable": self.retryable,
                })),
            )
        }
    }
}

// ── IPC endpoint handlers ───────────────────────────────────────

/// GET /api/ipc/agents — list known agents with their status and trust level.
pub async fn handle_ipc_agents(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let db = require_ipc_db(&state)?;
    let meta = require_ipc_auth(&state, &headers)?;
    db.update_last_seen(&meta.agent_id, meta.trust_level, &meta.role);

    let staleness = state.config.lock().agents_ipc.staleness_secs;
    let agents = db.list_agents(staleness);

    // L4 agents only see their configured destinations
    let agents = if meta.trust_level >= 4 {
        let l4_dests = &state.config.lock().agents_ipc.l4_destinations;
        agents
            .into_iter()
            .filter(|a| l4_dests.contains(&a.agent_id))
            .collect()
    } else {
        agents
    };

    Ok(Json(serde_json::json!({ "agents": agents })))
}

/// POST /api/ipc/send — send a message to another agent via the broker.
pub async fn handle_ipc_send(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<SendBody>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let db = require_ipc_db(&state)?;
    let meta = require_ipc_auth(&state, &headers)?;
    db.update_last_seen(&meta.agent_id, meta.trust_level, &meta.role);

    // Resolve recipient trust level
    let config = state.config.lock();
    let to_level = db
        .list_agents(config.agents_ipc.staleness_secs)
        .iter()
        .find(|a| a.agent_id == body.to)
        .map(|a| a.trust_level)
        .unwrap_or(3); // default to L3 for unknown agents

    // ACL check
    validate_send(
        meta.trust_level,
        to_level,
        &body.kind,
        &meta.agent_id,
        &body.to,
        body.session_id.as_deref(),
        &config.agents_ipc.lateral_text_pairs,
        &config.agents_ipc.l4_destinations,
        db,
    )
    .map_err(|e| e.into_response_pair(meta.trust_level))?;

    let message_ttl = config.agents_ipc.message_ttl_secs;
    drop(config);

    let msg_id = db
        .insert_message(
            &meta.agent_id,
            &body.to,
            &body.kind,
            &body.payload,
            meta.trust_level,
            body.session_id.as_deref(),
            body.priority,
            message_ttl,
        )
        .map_err(|e| {
            warn!(error = %e, "IPC insert_message failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "Failed to store message",
                    "code": "db_error"
                })),
            )
        })?;

    info!(
        from = meta.agent_id,
        to = body.to,
        kind = body.kind,
        msg_id = msg_id,
        "IPC message sent"
    );

    Ok(Json(serde_json::json!({ "ok": true, "id": msg_id })))
}

/// GET /api/ipc/inbox — retrieve messages for the authenticated agent.
pub async fn handle_ipc_inbox(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<InboxQuery>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let db = require_ipc_db(&state)?;
    let meta = require_ipc_auth(&state, &headers)?;
    db.update_last_seen(&meta.agent_id, meta.trust_level, &meta.role);

    let messages = db.fetch_inbox(&meta.agent_id, query.quarantine, query.limit);

    Ok(Json(serde_json::json!({ "messages": messages })))
}

/// GET /api/ipc/state — read a shared state key.
pub async fn handle_ipc_state_get(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<StateGetQuery>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let db = require_ipc_db(&state)?;
    let meta = require_ipc_auth(&state, &headers)?;
    db.update_last_seen(&meta.agent_id, meta.trust_level, &meta.role);

    validate_state_get(meta.trust_level, &query.key)
        .map_err(|e| e.into_response_pair(meta.trust_level))?;

    let entry = db.get_state(&query.key);
    Ok(Json(serde_json::json!({ "entry": entry })))
}

/// POST /api/ipc/state — write a shared state key.
pub async fn handle_ipc_state_set(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<StateSetBody>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let db = require_ipc_db(&state)?;
    let meta = require_ipc_auth(&state, &headers)?;
    db.update_last_seen(&meta.agent_id, meta.trust_level, &meta.role);

    validate_state_set(meta.trust_level, &meta.agent_id, &body.key)
        .map_err(|e| e.into_response_pair(meta.trust_level))?;

    db.set_state(&body.key, &body.value, &meta.agent_id);

    info!(agent = meta.agent_id, key = body.key, "IPC state set");

    Ok(Json(serde_json::json!({ "ok": true })))
}

// ── IPC admin endpoint handlers ─────────────────────────────────

/// GET /admin/ipc/agents — full agent list with metadata (localhost only).
pub async fn handle_admin_ipc_agents(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    require_localhost(&peer)?;
    let db = require_ipc_db(&state)?;
    let staleness = state.config.lock().agents_ipc.staleness_secs;
    let agents = db.list_agents(staleness);
    Ok(Json(serde_json::json!({ "agents": agents })))
}

/// POST /admin/ipc/revoke — revoke an agent (block messages, set status=revoked).
pub async fn handle_admin_ipc_revoke(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(body): Json<AdminAgentBody>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    require_localhost(&peer)?;
    let db = require_ipc_db(&state)?;
    db.block_pending_messages(&body.agent_id, "agent_revoked");
    let found = db.set_agent_status(&body.agent_id, "revoked");
    if found {
        info!(agent = body.agent_id, "IPC agent revoked");
    }
    Ok(Json(serde_json::json!({ "ok": true, "found": found })))
}

/// POST /admin/ipc/disable — disable an agent without revoking its token.
pub async fn handle_admin_ipc_disable(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(body): Json<AdminAgentBody>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    require_localhost(&peer)?;
    let db = require_ipc_db(&state)?;
    db.block_pending_messages(&body.agent_id, "agent_disabled");
    let found = db.set_agent_status(&body.agent_id, "disabled");
    if found {
        info!(agent = body.agent_id, "IPC agent disabled");
    }
    Ok(Json(serde_json::json!({ "ok": true, "found": found })))
}

/// POST /admin/ipc/quarantine — quarantine an agent (set trust_level=4).
pub async fn handle_admin_ipc_quarantine(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(body): Json<AdminAgentBody>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    require_localhost(&peer)?;
    let db = require_ipc_db(&state)?;
    let found = db.set_agent_status(&body.agent_id, "quarantined");
    // Force trust level to 4
    let _ = db.set_agent_trust_level(&body.agent_id, 4);
    if found {
        info!(agent = body.agent_id, "IPC agent quarantined");
    }
    Ok(Json(serde_json::json!({ "ok": true, "found": found })))
}

/// POST /admin/ipc/downgrade — downgrade an agent's trust level (only increases).
pub async fn handle_admin_ipc_downgrade(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(body): Json<AdminDowngradeBody>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    require_localhost(&peer)?;
    let db = require_ipc_db(&state)?;
    match db.set_agent_trust_level(&body.agent_id, body.new_level) {
        Some(old_level) => {
            info!(
                agent = body.agent_id,
                old_level = old_level,
                new_level = body.new_level,
                "IPC agent downgraded"
            );
            Ok(Json(serde_json::json!({
                "ok": true,
                "old_level": old_level,
                "new_level": body.new_level
            })))
        }
        None => Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "Agent not found or new_level is not a downgrade",
                "code": "downgrade_invalid"
            })),
        )),
    }
}

// ── Helpers ─────────────────────────────────────────────────────

fn require_ipc_db(state: &AppState) -> Result<&Arc<IpcDb>, (StatusCode, Json<serde_json::Value>)> {
    state.ipc_db.as_ref().ok_or_else(ipc_disabled_error)
}

fn ipc_disabled_error() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(serde_json::json!({
            "error": "IPC is not enabled",
            "code": "ipc_disabled"
        })),
    )
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

// ── Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> IpcDb {
        IpcDb::open_in_memory().expect("in-memory DB")
    }

    // ── validate_send tests ─────────────────────────────────────

    #[test]
    fn validate_send_invalid_kind() {
        let db = test_db();
        let result = validate_send(3, 1, "execute", "a", "b", None, &[], &[], &db);
        assert_eq!(result.unwrap_err().code, "invalid_kind");
    }

    #[test]
    fn validate_send_l4_text_only() {
        let db = test_db();
        let l4_dests = vec!["opus".to_string()];
        let result = validate_send(4, 1, "task", "kids", "opus", None, &[], &l4_dests, &db);
        assert_eq!(result.unwrap_err().code, "l4_text_only");
    }

    #[test]
    fn validate_send_l4_text_allowed() {
        let db = test_db();
        let l4_dests = vec!["opus".to_string()];
        let result = validate_send(4, 1, "text", "kids", "opus", None, &[], &l4_dests, &db);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_send_l4_destination_denied() {
        let db = test_db();
        let result = validate_send(4, 1, "text", "kids", "opus", None, &[], &[], &db);
        assert_eq!(result.unwrap_err().code, "l4_destination_denied");
    }

    #[test]
    fn validate_send_task_upward_denied() {
        let db = test_db();
        let result = validate_send(3, 1, "task", "worker", "opus", None, &[], &[], &db);
        assert_eq!(result.unwrap_err().code, "task_upward_denied");
    }

    #[test]
    fn validate_send_task_lateral_denied() {
        let db = test_db();
        let result = validate_send(2, 2, "task", "a", "b", None, &[], &[], &db);
        assert_eq!(result.unwrap_err().code, "task_lateral_denied");
    }

    #[test]
    fn validate_send_task_downward_ok() {
        let db = test_db();
        let result = validate_send(1, 3, "task", "opus", "worker", None, &[], &[], &db);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_send_result_no_task() {
        let db = test_db();
        let result = validate_send(
            3,
            1,
            "result",
            "worker",
            "opus",
            Some("session-1"),
            &[],
            &[],
            &db,
        );
        assert_eq!(result.unwrap_err().code, "result_no_task");
    }

    #[test]
    fn validate_send_result_without_session() {
        let db = test_db();
        let result = validate_send(3, 1, "result", "worker", "opus", None, &[], &[], &db);
        assert_eq!(result.unwrap_err().code, "result_no_task");
    }

    #[test]
    fn validate_send_l4_lateral_denied() {
        let db = test_db();
        let l4_dests = vec!["other_kid".to_string()];
        let result = validate_send(4, 4, "text", "kids", "other_kid", None, &[], &l4_dests, &db);
        assert_eq!(result.unwrap_err().code, "l4_lateral_denied");
    }

    #[test]
    fn validate_send_l3_lateral_text_denied() {
        let db = test_db();
        let result = validate_send(3, 3, "text", "agent_a", "agent_b", None, &[], &[], &db);
        assert_eq!(result.unwrap_err().code, "l3_lateral_denied");
    }

    #[test]
    fn validate_send_l3_lateral_text_allowed() {
        let db = test_db();
        let pairs = vec![["agent_a".to_string(), "agent_b".to_string()]];
        let result = validate_send(3, 3, "text", "agent_a", "agent_b", None, &pairs, &[], &db);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_send_l3_lateral_text_reverse() {
        let db = test_db();
        let pairs = vec![["agent_b".to_string(), "agent_a".to_string()]];
        let result = validate_send(3, 3, "text", "agent_a", "agent_b", None, &pairs, &[], &db);
        assert!(result.is_ok());
    }

    // ── validate_state_set tests ────────────────────────────────

    #[test]
    fn state_set_l4_own_namespace() {
        assert!(validate_state_set(4, "kids", "agent:kids:mood").is_ok());
    }

    #[test]
    fn state_set_l4_other_namespace_denied() {
        assert_eq!(
            validate_state_set(4, "kids", "agent:opus:x")
                .unwrap_err()
                .code,
            "agent_namespace_denied"
        );
    }

    #[test]
    fn state_set_l4_public_denied() {
        assert_eq!(
            validate_state_set(4, "kids", "public:status")
                .unwrap_err()
                .code,
            "public_denied"
        );
    }

    #[test]
    fn state_set_l3_public_ok() {
        assert!(validate_state_set(3, "worker", "public:status").is_ok());
    }

    #[test]
    fn state_set_l3_team_denied() {
        assert_eq!(
            validate_state_set(3, "worker", "team:config")
                .unwrap_err()
                .code,
            "team_denied"
        );
    }

    #[test]
    fn state_set_l2_team_ok() {
        assert!(validate_state_set(2, "sentinel", "team:config").is_ok());
    }

    #[test]
    fn state_set_l2_global_denied() {
        assert_eq!(
            validate_state_set(2, "sentinel", "global:flag")
                .unwrap_err()
                .code,
            "global_denied"
        );
    }

    #[test]
    fn state_set_l1_global_ok() {
        assert!(validate_state_set(1, "opus", "global:flag").is_ok());
    }

    #[test]
    fn state_set_secret_denied() {
        assert_eq!(
            validate_state_set(1, "opus", "secret:key")
                .unwrap_err()
                .code,
            "secret_denied"
        );
    }

    #[test]
    fn state_set_invalid_format() {
        assert_eq!(
            validate_state_set(1, "opus", "nocolon").unwrap_err().code,
            "invalid_key_format"
        );
    }

    // ── validate_state_get tests ────────────────────────────────

    #[test]
    fn state_get_public_all_levels() {
        for level in 0..=4 {
            assert!(validate_state_get(level, "public:status").is_ok());
        }
    }

    #[test]
    fn state_get_secret_l1_ok() {
        assert!(validate_state_get(1, "secret:api_key").is_ok());
    }

    #[test]
    fn state_get_secret_l2_denied() {
        assert_eq!(
            validate_state_get(2, "secret:api_key").unwrap_err().code,
            "secret_read_denied"
        );
    }

    // ── IpcDb tests ─────────────────────────────────────────────

    #[test]
    fn session_has_task_for_false() {
        let db = test_db();
        assert!(!db.session_has_task_for("s1", "worker"));
    }

    #[test]
    fn session_has_task_for_true() {
        let db = test_db();
        let conn = db.conn.lock();
        conn.execute(
            "INSERT INTO messages (session_id, from_agent, to_agent, kind, payload,
             from_trust_level, seq, created_at)
             VALUES ('s1', 'opus', 'worker', 'task', 'do work', 1, 1, 100)",
            [],
        )
        .unwrap();
        drop(conn);
        assert!(db.session_has_task_for("s1", "worker"));
    }

    #[test]
    fn session_has_task_for_blocked_ignored() {
        let db = test_db();
        let conn = db.conn.lock();
        conn.execute(
            "INSERT INTO messages (session_id, from_agent, to_agent, kind, payload,
             from_trust_level, seq, created_at, blocked)
             VALUES ('s1', 'opus', 'worker', 'task', 'do work', 1, 1, 100, 1)",
            [],
        )
        .unwrap();
        drop(conn);
        assert!(!db.session_has_task_for("s1", "worker"));
    }

    #[test]
    fn next_seq_monotonic() {
        let db = test_db();
        assert_eq!(db.next_seq("agent_a"), 1);
        assert_eq!(db.next_seq("agent_a"), 2);
        assert_eq!(db.next_seq("agent_a"), 3);
        assert_eq!(db.next_seq("agent_b"), 1);
    }

    #[test]
    fn update_last_seen_upsert() {
        let db = test_db();
        db.update_last_seen("opus", 1, "coordinator");
        db.update_last_seen("opus", 1, "coordinator");
        let conn = db.conn.lock();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM agents WHERE agent_id = 'opus'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    // ── Broker handler unit tests (Step 5-7) ────────────────────

    #[test]
    fn insert_and_fetch_message() {
        let db = test_db();
        db.update_last_seen("opus", 1, "coordinator");
        db.update_last_seen("worker", 3, "agent");

        let id = db
            .insert_message(
                "opus",
                "worker",
                "task",
                "do something",
                1,
                Some("s1"),
                0,
                None,
            )
            .unwrap();
        assert!(id > 0);

        let messages = db.fetch_inbox("worker", true, 50);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].from_agent, "opus");
        assert_eq!(messages[0].kind, "task");
        assert_eq!(messages[0].payload, "do something");

        // Second fetch should return empty (marked as read)
        let messages2 = db.fetch_inbox("worker", true, 50);
        assert!(messages2.is_empty());
    }

    #[test]
    fn fetch_inbox_excludes_quarantine() {
        let db = test_db();
        db.insert_message("kids", "opus", "text", "hello", 4, None, 0, None)
            .unwrap();
        db.insert_message("worker", "opus", "text", "report", 3, None, 0, None)
            .unwrap();

        // Without quarantine: only L3 message
        let messages = db.fetch_inbox("opus", false, 50);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].from_agent, "worker");
    }

    #[test]
    fn fetch_inbox_includes_quarantine() {
        let db = test_db();
        db.insert_message("kids", "opus", "text", "hello", 4, None, 0, None)
            .unwrap();
        db.insert_message("worker", "opus", "text", "report", 3, None, 0, None)
            .unwrap();

        let messages = db.fetch_inbox("opus", true, 50);
        assert_eq!(messages.len(), 2);
    }

    #[test]
    fn list_agents_staleness() {
        let db = test_db();
        db.update_last_seen("opus", 1, "coordinator");
        let agents = db.list_agents(120);
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].status, "online");
    }

    #[test]
    fn state_get_set_roundtrip() {
        let db = test_db();
        assert!(db.get_state("public:status").is_none());

        db.set_state("public:status", "ready", "worker");
        let entry = db.get_state("public:status").unwrap();
        assert_eq!(entry.value, "ready");
        assert_eq!(entry.owner, "worker");

        // Overwrite
        db.set_state("public:status", "busy", "opus");
        let entry = db.get_state("public:status").unwrap();
        assert_eq!(entry.value, "busy");
        assert_eq!(entry.owner, "opus");
    }

    #[test]
    fn admin_disable_blocks_messages() {
        let db = test_db();
        db.update_last_seen("worker", 3, "agent");
        db.insert_message("opus", "worker", "task", "do it", 1, None, 0, None)
            .unwrap();

        db.block_pending_messages("worker", "agent_disabled");
        let found = db.set_agent_status("worker", "disabled");
        assert!(found);

        // Messages should be blocked
        let messages = db.fetch_inbox("worker", true, 50);
        assert!(messages.is_empty());
    }

    #[test]
    fn admin_downgrade_only_increases() {
        let db = test_db();
        db.update_last_seen("worker", 2, "agent");

        // Upgrade attempt (2 → 1) should fail
        assert!(db.set_agent_trust_level("worker", 1).is_none());

        // Same level should fail
        assert!(db.set_agent_trust_level("worker", 2).is_none());

        // Downgrade (2 → 3) should succeed
        let old = db.set_agent_trust_level("worker", 3);
        assert_eq!(old, Some(2));
    }

    #[test]
    fn message_ttl_cleanup() {
        let db = test_db();
        // Insert a message with expired TTL
        let conn = db.conn.lock();
        conn.execute(
            "INSERT INTO messages (from_agent, to_agent, kind, payload,
             from_trust_level, seq, created_at, expires_at)
             VALUES ('opus', 'worker', 'task', 'old', 1, 1, 100, 101)",
            [],
        )
        .unwrap();
        drop(conn);

        // Fetch should clean up expired messages
        let messages = db.fetch_inbox("worker", true, 50);
        assert!(messages.is_empty());
    }
}
