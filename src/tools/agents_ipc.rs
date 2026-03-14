//! IPC tools for inter-agent communication.
//!
//! Tools use an HTTP client (`IpcClient`) to communicate with the IPC broker
//! running in the gateway. Agents never access the broker database directly.

use super::traits::{Tool, ToolResult};
use async_trait::async_trait;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;

// ── IpcClient ───────────────────────────────────────────────────

/// HTTP client for communicating with the IPC broker gateway.
///
/// Proxy-aware: respects the runtime proxy configuration for service key
/// `"tool.agents_ipc"`.
pub struct IpcClient {
    client: reqwest::Client,
    broker_url: String,
    bearer_token: String,
}

impl IpcClient {
    /// Create a new IPC client.
    pub fn new(broker_url: &str, bearer_token: &str, timeout_secs: u64) -> Self {
        let builder = reqwest::Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .connect_timeout(Duration::from_secs(5));
        let builder = crate::config::apply_runtime_proxy_to_builder(builder, "tool.agents_ipc");
        let client = builder.build().unwrap_or_else(|err| {
            tracing::warn!("Failed to build IPC client: {err}");
            reqwest::Client::new()
        });
        Self {
            client,
            broker_url: broker_url.trim_end_matches('/').to_string(),
            bearer_token: bearer_token.to_string(),
        }
    }

    async fn get(&self, path: &str) -> Result<reqwest::Response, reqwest::Error> {
        self.client
            .get(format!("{}{path}", self.broker_url))
            .bearer_auth(&self.bearer_token)
            .send()
            .await
    }

    async fn post(
        &self,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<reqwest::Response, reqwest::Error> {
        self.client
            .post(format!("{}{path}", self.broker_url))
            .bearer_auth(&self.bearer_token)
            .json(body)
            .send()
            .await
    }
}

// ── AgentsListTool ──────────────────────────────────────────────

/// Tool for listing known agents and their status.
pub struct AgentsListTool {
    client: Arc<IpcClient>,
}

impl AgentsListTool {
    pub fn new(client: Arc<IpcClient>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl Tool for AgentsListTool {
    fn name(&self) -> &str {
        "agents_list"
    }

    fn description(&self) -> &str {
        "List all known agents in the IPC mesh with their status, role, and trust level. \
         Use this to discover available agents before sending messages."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        })
    }

    async fn execute(&self, _args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let resp = self
            .client
            .get("/api/ipc/agents")
            .await
            .map_err(|e| anyhow::anyhow!("Failed to connect to IPC broker: {e}"))?;

        let status = resp.status();
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to parse broker response: {e}"))?;

        if status.is_success() {
            Ok(ToolResult {
                success: true,
                output: serde_json::to_string_pretty(&body)?,
                error: None,
            })
        } else {
            let error_msg = body["error"].as_str().unwrap_or("Unknown error");
            Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("Broker returned {status}: {error_msg}")),
            })
        }
    }
}

// ── AgentsSendTool ──────────────────────────────────────────────

/// Tool for sending a message to another agent via the IPC broker.
pub struct AgentsSendTool {
    client: Arc<IpcClient>,
}

impl AgentsSendTool {
    pub fn new(client: Arc<IpcClient>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl Tool for AgentsSendTool {
    fn name(&self) -> &str {
        "agents_send"
    }

    fn description(&self) -> &str {
        "Send a message to another agent through the IPC broker. \
         The broker enforces trust-level ACL: you cannot assign tasks to \
         higher-trust agents or send restricted message types."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "to": {
                    "type": "string",
                    "description": "Target agent ID"
                },
                "kind": {
                    "type": "string",
                    "enum": ["text", "task", "result", "query"],
                    "description": "Message kind (default: text)"
                },
                "payload": {
                    "type": "string",
                    "description": "Message content"
                },
                "session_id": {
                    "type": "string",
                    "description": "Session ID for task/result correlation (required for kind=result)"
                },
                "priority": {
                    "type": "integer",
                    "description": "Message priority (higher = more urgent, default: 0)"
                }
            },
            "required": ["to", "payload"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let to = args["to"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'to' parameter"))?;
        let payload = args["payload"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'payload' parameter"))?;
        let kind = args["kind"].as_str().unwrap_or("text");
        let session_id = args["session_id"].as_str();
        let priority = args["priority"].as_i64().unwrap_or(0);

        let mut body = json!({
            "to": to,
            "kind": kind,
            "payload": payload,
            "priority": priority,
        });
        if let Some(sid) = session_id {
            body["session_id"] = json!(sid);
        }

        let resp = self
            .client
            .post("/api/ipc/send", &body)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to connect to IPC broker: {e}"))?;

        let status = resp.status();
        let resp_body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to parse broker response: {e}"))?;

        if status.is_success() {
            Ok(ToolResult {
                success: true,
                output: serde_json::to_string_pretty(&resp_body)?,
                error: None,
            })
        } else {
            let error_msg = resp_body["error"].as_str().unwrap_or("Unknown error");
            let code = resp_body["code"].as_str().unwrap_or("unknown");
            Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("[{code}] {error_msg}")),
            })
        }
    }
}

// ── AgentsInboxTool ─────────────────────────────────────────────

/// Maximum payload length returned in inbox results to avoid flooding context.
const INBOX_PAYLOAD_TRUNCATE: usize = 4000;

/// Tool for retrieving messages from the agent's inbox.
pub struct AgentsInboxTool {
    client: Arc<IpcClient>,
}

impl AgentsInboxTool {
    pub fn new(client: Arc<IpcClient>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl Tool for AgentsInboxTool {
    fn name(&self) -> &str {
        "agents_inbox"
    }

    fn description(&self) -> &str {
        "Check your inbox for messages from other agents. Messages are marked as read \
         after retrieval. Use quarantine=true to review messages from restricted (L4) agents \
         separately."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "quarantine": {
                    "type": "boolean",
                    "description": "If true, fetch only quarantined messages from restricted agents (default: false)"
                },
                "limit": {
                    "type": "integer",
                    "description": "Max messages to retrieve (default: 50)"
                }
            },
            "additionalProperties": false
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let quarantine = args["quarantine"].as_bool().unwrap_or(false);
        let limit = args["limit"].as_u64().unwrap_or(50);

        let path = format!("/api/ipc/inbox?quarantine={quarantine}&limit={limit}");
        let resp = self
            .client
            .get(&path)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to connect to IPC broker: {e}"))?;

        let status = resp.status();
        let mut body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to parse broker response: {e}"))?;

        if status.is_success() {
            // Truncate long payloads to avoid flooding the LLM context
            if let Some(messages) = body["messages"].as_array_mut() {
                for msg in messages.iter_mut() {
                    if let Some(payload) = msg["payload"].as_str() {
                        if payload.len() > INBOX_PAYLOAD_TRUNCATE {
                            let truncated = format!(
                                "{}… [truncated, {} chars total]",
                                &payload[..INBOX_PAYLOAD_TRUNCATE],
                                payload.len()
                            );
                            msg["payload"] = json!(truncated);
                        }
                    }
                }
            }

            Ok(ToolResult {
                success: true,
                output: serde_json::to_string_pretty(&body)?,
                error: None,
            })
        } else {
            let error_msg = body["error"].as_str().unwrap_or("Unknown error");
            Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("Broker returned {status}: {error_msg}")),
            })
        }
    }
}

// ── AgentsReplyTool ─────────────────────────────────────────────

/// Tool for replying to a task or query with a result in the same session.
pub struct AgentsReplyTool {
    client: Arc<IpcClient>,
}

impl AgentsReplyTool {
    pub fn new(client: Arc<IpcClient>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl Tool for AgentsReplyTool {
    fn name(&self) -> &str {
        "agents_reply"
    }

    fn description(&self) -> &str {
        "Reply to a task or query from another agent with a result. \
         Automatically uses kind=result and requires the session_id from the \
         original task/query message."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "to": {
                    "type": "string",
                    "description": "Agent ID to reply to (the original sender)"
                },
                "session_id": {
                    "type": "string",
                    "description": "Session ID from the original task/query message"
                },
                "payload": {
                    "type": "string",
                    "description": "Result content"
                }
            },
            "required": ["to", "session_id", "payload"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let to = args["to"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'to' parameter"))?;
        let session_id = args["session_id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'session_id' parameter"))?;
        let payload = args["payload"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'payload' parameter"))?;

        let body = json!({
            "to": to,
            "kind": "result",
            "payload": payload,
            "session_id": session_id,
            "priority": 0,
        });

        let resp = self
            .client
            .post("/api/ipc/send", &body)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to connect to IPC broker: {e}"))?;

        let status = resp.status();
        let resp_body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to parse broker response: {e}"))?;

        if status.is_success() {
            Ok(ToolResult {
                success: true,
                output: serde_json::to_string_pretty(&resp_body)?,
                error: None,
            })
        } else {
            let error_msg = resp_body["error"].as_str().unwrap_or("Unknown error");
            let code = resp_body["code"].as_str().unwrap_or("unknown");
            Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("[{code}] {error_msg}")),
            })
        }
    }
}

// ── StateGetTool ────────────────────────────────────────────────

/// Tool for reading a shared state key from the IPC broker.
pub struct StateGetTool {
    client: Arc<IpcClient>,
}

impl StateGetTool {
    pub fn new(client: Arc<IpcClient>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl Tool for StateGetTool {
    fn name(&self) -> &str {
        "state_get"
    }

    fn description(&self) -> &str {
        "Read a shared state key from the IPC broker. Keys use namespace format: \
         scope:owner:key (e.g. public:status, agent:myid:mood, team:config). \
         Access is controlled by trust level."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "key": {
                    "type": "string",
                    "description": "State key in scope:owner:key format"
                }
            },
            "required": ["key"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let key = args["key"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'key' parameter"))?;

        let path = format!("/api/ipc/state?key={}", urlencoding::encode(key));
        let resp = self
            .client
            .get(&path)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to connect to IPC broker: {e}"))?;

        let status = resp.status();
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to parse broker response: {e}"))?;

        if status.is_success() {
            Ok(ToolResult {
                success: true,
                output: serde_json::to_string_pretty(&body)?,
                error: None,
            })
        } else {
            let error_msg = body["error"].as_str().unwrap_or("Unknown error");
            let code = body["code"].as_str().unwrap_or("unknown");
            Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("[{code}] {error_msg}")),
            })
        }
    }
}

// ── StateSetTool ────────────────────────────────────────────────

/// Tool for writing a shared state key to the IPC broker.
pub struct StateSetTool {
    client: Arc<IpcClient>,
}

impl StateSetTool {
    pub fn new(client: Arc<IpcClient>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl Tool for StateSetTool {
    fn name(&self) -> &str {
        "state_set"
    }

    fn description(&self) -> &str {
        "Write a shared state key to the IPC broker. Keys use namespace format: \
         scope:owner:key. Write access depends on trust level: \
         L4=agent:{self}:* only, L3=+public:*, L2=+team:*, L1=+global:*. \
         secret:* namespace is reserved."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "key": {
                    "type": "string",
                    "description": "State key in scope:owner:key format"
                },
                "value": {
                    "type": "string",
                    "description": "Value to store"
                }
            },
            "required": ["key", "value"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let key = args["key"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'key' parameter"))?;
        let value = args["value"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'value' parameter"))?;

        let body = json!({
            "key": key,
            "value": value,
        });

        let resp = self
            .client
            .post("/api/ipc/state", &body)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to connect to IPC broker: {e}"))?;

        let status = resp.status();
        let resp_body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to parse broker response: {e}"))?;

        if status.is_success() {
            Ok(ToolResult {
                success: true,
                output: serde_json::to_string_pretty(&resp_body)?,
                error: None,
            })
        } else {
            let error_msg = resp_body["error"].as_str().unwrap_or("Unknown error");
            let code = resp_body["code"].as_str().unwrap_or("unknown");
            Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("[{code}] {error_msg}")),
            })
        }
    }
}

// ── AgentsSpawnTool ──────────────────────────────────────────────

/// Tool for spawning a new agent process via the cron scheduler.
///
/// This is a local tool — it does not use the IPC HTTP client. Instead it
/// creates a one-shot cron job that runs immediately. Trust propagation:
/// child trust_level >= parent trust_level (cannot escalate).
pub struct AgentsSpawnTool {
    config: Arc<crate::config::Config>,
    security: Arc<crate::security::SecurityPolicy>,
    parent_trust_level: u8,
}

impl AgentsSpawnTool {
    pub fn new(
        config: Arc<crate::config::Config>,
        security: Arc<crate::security::SecurityPolicy>,
        parent_trust_level: u8,
    ) -> Self {
        Self {
            config,
            security,
            parent_trust_level,
        }
    }
}

#[async_trait]
impl Tool for AgentsSpawnTool {
    fn name(&self) -> &str {
        "agents_spawn"
    }

    fn description(&self) -> &str {
        "Spawn a new agent process with a given prompt. The child agent runs as a \
         one-shot cron job and inherits your trust level or lower. You cannot spawn \
         agents with higher trust than your own."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "prompt": {
                    "type": "string",
                    "description": "System prompt / instructions for the spawned agent"
                },
                "name": {
                    "type": "string",
                    "description": "Human-readable name for the spawned agent (optional)"
                },
                "model": {
                    "type": "string",
                    "description": "Model to use (optional, defaults to parent's model)"
                },
                "trust_level": {
                    "type": "integer",
                    "description": "Trust level for child (0-4). Must be >= parent's level. Default: parent's level."
                }
            },
            "required": ["prompt"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        // Security check
        if !self.security.can_act() {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some("Security policy denied spawn".into()),
            });
        }

        let prompt = args["prompt"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'prompt' parameter"))?;
        let name = args["name"].as_str().map(String::from);
        let model = args["model"].as_str().map(String::from);
        let requested_level = args["trust_level"]
            .as_u64()
            .and_then(|v| u8::try_from(v).ok());

        // Trust propagation: child >= parent
        let child_level = requested_level
            .map(|r| r.max(self.parent_trust_level))
            .unwrap_or(self.parent_trust_level);

        if let Some(requested) = requested_level {
            if requested < self.parent_trust_level {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(format!(
                        "Cannot escalate: requested trust L{requested} but parent is L{}. \
                         Child will be L{child_level} at minimum.",
                        self.parent_trust_level
                    )),
                });
            }
        }

        // Create one-shot job that fires ~1 second from now
        let run_at = chrono::Utc::now() + chrono::Duration::seconds(1);
        let schedule = crate::cron::Schedule::At { at: run_at };

        let job_name = name.unwrap_or_else(|| format!("ipc-spawn-L{child_level}"));
        let spawn_prompt = format!("[IPC spawned agent | trust_level={child_level}]\n\n{prompt}");

        match crate::cron::add_agent_job(
            &self.config,
            Some(job_name.clone()),
            schedule,
            &spawn_prompt,
            crate::cron::SessionTarget::Isolated,
            model,
            None, // delivery
            true, // delete_after_run
        ) {
            Ok(job) => Ok(ToolResult {
                success: true,
                output: serde_json::to_string_pretty(&json!({
                    "spawned": true,
                    "job_id": job.id,
                    "name": job_name,
                    "trust_level": child_level,
                    "next_run": job.next_run.to_rfc3339(),
                }))?,
                error: None,
            }),
            Err(e) => Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("Failed to spawn agent: {e}")),
            }),
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ipc_client_trims_trailing_slash() {
        let client = IpcClient::new("http://localhost:42617/", "token", 10);
        assert_eq!(client.broker_url, "http://localhost:42617");
    }

    #[test]
    fn ipc_client_preserves_clean_url() {
        let client = IpcClient::new("http://localhost:42617", "token", 10);
        assert_eq!(client.broker_url, "http://localhost:42617");
    }

    #[test]
    fn agents_list_tool_spec() {
        let client = Arc::new(IpcClient::new("http://localhost:42617", "t", 10));
        let tool = AgentsListTool::new(client);
        let spec = tool.spec();
        assert_eq!(spec.name, "agents_list");
        assert_eq!(spec.parameters["type"], "object");
    }

    #[test]
    fn agents_send_tool_spec() {
        let client = Arc::new(IpcClient::new("http://localhost:42617", "t", 10));
        let tool = AgentsSendTool::new(client);
        let spec = tool.spec();
        assert_eq!(spec.name, "agents_send");
        let required = spec.parameters["required"].as_array().unwrap();
        assert!(required.contains(&json!("to")));
        assert!(required.contains(&json!("payload")));
    }

    #[test]
    fn agents_inbox_tool_spec() {
        let client = Arc::new(IpcClient::new("http://localhost:42617", "t", 10));
        let tool = AgentsInboxTool::new(client);
        let spec = tool.spec();
        assert_eq!(spec.name, "agents_inbox");
    }

    #[test]
    fn agents_reply_tool_spec() {
        let client = Arc::new(IpcClient::new("http://localhost:42617", "t", 10));
        let tool = AgentsReplyTool::new(client);
        let spec = tool.spec();
        assert_eq!(spec.name, "agents_reply");
        let required = spec.parameters["required"].as_array().unwrap();
        assert!(required.contains(&json!("to")));
        assert!(required.contains(&json!("session_id")));
        assert!(required.contains(&json!("payload")));
    }

    #[test]
    fn state_get_tool_spec() {
        let client = Arc::new(IpcClient::new("http://localhost:42617", "t", 10));
        let tool = StateGetTool::new(client);
        let spec = tool.spec();
        assert_eq!(spec.name, "state_get");
        let required = spec.parameters["required"].as_array().unwrap();
        assert!(required.contains(&json!("key")));
    }

    #[test]
    fn agents_spawn_tool_spec() {
        let config = Arc::new(crate::config::Config::default());
        let security = Arc::new(crate::security::SecurityPolicy::default());
        let tool = AgentsSpawnTool::new(config, security, 2);
        let spec = tool.spec();
        assert_eq!(spec.name, "agents_spawn");
        let required = spec.parameters["required"].as_array().unwrap();
        assert!(required.contains(&json!("prompt")));
    }

    #[test]
    fn state_set_tool_spec() {
        let client = Arc::new(IpcClient::new("http://localhost:42617", "t", 10));
        let tool = StateSetTool::new(client);
        let spec = tool.spec();
        assert_eq!(spec.name, "state_set");
        let required = spec.parameters["required"].as_array().unwrap();
        assert!(required.contains(&json!("key")));
        assert!(required.contains(&json!("value")));
    }

    #[test]
    fn payload_truncation_logic() {
        let long_payload = "x".repeat(5000);
        let mut msg = json!({
            "payload": long_payload,
            "from_agent": "test"
        });

        if let Some(payload) = msg["payload"].as_str() {
            if payload.len() > INBOX_PAYLOAD_TRUNCATE {
                let truncated = format!(
                    "{}… [truncated, {} chars total]",
                    &payload[..INBOX_PAYLOAD_TRUNCATE],
                    payload.len()
                );
                msg["payload"] = json!(truncated);
            }
        }

        let result = msg["payload"].as_str().unwrap();
        assert!(result.len() < 5000);
        assert!(result.contains("[truncated, 5000 chars total]"));
    }
}
