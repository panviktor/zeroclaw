//! IPC broker handlers for inter-agent communication.
//!
//! All IPC communication is broker-mediated: agents authenticate with bearer
//! tokens, and the broker resolves trust levels from token metadata. The broker
//! owns the SQLite database — agents never access it directly.

use super::AppState;
use axum::{
    extract::{ConnectInfo, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use std::net::SocketAddr;

// ── IpcDb (broker-owned SQLite) ─────────────────────────────────

/// Broker-owned SQLite database for IPC messages, agent registry, and shared state.
///
/// Initialized when `agents_ipc.enabled = true`. The database is WAL-mode
/// and only accessible by the broker process.
pub struct IpcDb {
    // Will be filled in Step 4: parking_lot::Mutex<rusqlite::Connection>
    _private: (),
}

// ── IPC endpoint handlers (stubs) ───────────────────────────────

/// GET /api/ipc/agents — list known agents with their status and trust level.
pub async fn handle_ipc_agents(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    if state.ipc_db.is_none() {
        return Err(ipc_disabled_error());
    }
    Ok(Json(serde_json::json!({ "agents": [] })))
}

/// POST /api/ipc/send — send a message to another agent via the broker.
pub async fn handle_ipc_send(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    if state.ipc_db.is_none() {
        return Err(ipc_disabled_error());
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// GET /api/ipc/inbox — retrieve messages for the authenticated agent.
pub async fn handle_ipc_inbox(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    if state.ipc_db.is_none() {
        return Err(ipc_disabled_error());
    }
    Ok(Json(serde_json::json!({ "messages": [] })))
}

/// GET /api/ipc/state — read a shared state key.
pub async fn handle_ipc_state_get(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    if state.ipc_db.is_none() {
        return Err(ipc_disabled_error());
    }
    Ok(Json(serde_json::json!({ "value": null })))
}

/// POST /api/ipc/state — write a shared state key.
pub async fn handle_ipc_state_set(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    if state.ipc_db.is_none() {
        return Err(ipc_disabled_error());
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}

// ── IPC admin endpoint handlers (stubs) ─────────────────────────

/// GET /admin/ipc/agents — full agent list with metadata (localhost only).
pub async fn handle_admin_ipc_agents(
    State(state): State<AppState>,
    ConnectInfo(_peer): ConnectInfo<SocketAddr>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    if state.ipc_db.is_none() {
        return Err(ipc_disabled_error());
    }
    Ok(Json(serde_json::json!({ "agents": [] })))
}

/// POST /admin/ipc/revoke — revoke an agent's token (localhost only).
pub async fn handle_admin_ipc_revoke(
    State(state): State<AppState>,
    ConnectInfo(_peer): ConnectInfo<SocketAddr>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    if state.ipc_db.is_none() {
        return Err(ipc_disabled_error());
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// POST /admin/ipc/disable — disable an agent without revoking its token (localhost only).
pub async fn handle_admin_ipc_disable(
    State(state): State<AppState>,
    ConnectInfo(_peer): ConnectInfo<SocketAddr>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    if state.ipc_db.is_none() {
        return Err(ipc_disabled_error());
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// POST /admin/ipc/quarantine — quarantine an agent (localhost only).
pub async fn handle_admin_ipc_quarantine(
    State(state): State<AppState>,
    ConnectInfo(_peer): ConnectInfo<SocketAddr>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    if state.ipc_db.is_none() {
        return Err(ipc_disabled_error());
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// POST /admin/ipc/downgrade — downgrade an agent's trust level (localhost only).
pub async fn handle_admin_ipc_downgrade(
    State(state): State<AppState>,
    ConnectInfo(_peer): ConnectInfo<SocketAddr>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    if state.ipc_db.is_none() {
        return Err(ipc_disabled_error());
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}

// ── Helpers ─────────────────────────────────────────────────────

fn ipc_disabled_error() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(serde_json::json!({
            "error": "IPC is not enabled",
            "code": "ipc_disabled"
        })),
    )
}
