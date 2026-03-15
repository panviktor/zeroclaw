//! Audit logging for security events

use crate::config::AuditConfig;
use anyhow::Result;
use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use uuid::Uuid;

/// Audit event types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditEventType {
    CommandExecution,
    FileAccess,
    ConfigChange,
    AuthSuccess,
    AuthFailure,
    PolicyViolation,
    SecurityEvent,
    IpcSend,
    IpcBlocked,
    IpcRateLimited,
    IpcReceived,
    IpcStateChange,
    IpcAdminAction,
    IpcLeakDetected,
}

/// Actor information (who performed the action)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Actor {
    pub channel: String,
    pub user_id: Option<String>,
    pub username: Option<String>,
}

/// Action information (what was done)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    pub command: Option<String>,
    pub risk_level: Option<String>,
    pub approved: bool,
    pub allowed: bool,
}

/// Execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub success: bool,
    pub exit_code: Option<i32>,
    pub duration_ms: Option<u64>,
    pub error: Option<String>,
}

/// Security context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityContext {
    pub policy_violation: bool,
    pub rate_limit_remaining: Option<u32>,
    pub sandbox_backend: Option<String>,
}

/// Complete audit event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub timestamp: DateTime<Utc>,
    pub event_id: String,
    pub event_type: AuditEventType,
    pub actor: Option<Actor>,
    pub action: Option<Action>,
    pub result: Option<ExecutionResult>,
    pub security: SecurityContext,
    /// HMAC-SHA256 chain value: HMAC(key, "{prev_hmac}|{event_json}").
    /// Present when HMAC audit chain is enabled (Phase 3B).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hmac: Option<String>,
}

impl AuditEvent {
    /// Create a new audit event
    pub fn new(event_type: AuditEventType) -> Self {
        Self {
            timestamp: Utc::now(),
            event_id: Uuid::new_v4().to_string(),
            event_type,
            actor: None,
            action: None,
            result: None,
            security: SecurityContext {
                policy_violation: false,
                rate_limit_remaining: None,
                sandbox_backend: None,
            },
            hmac: None,
        }
    }

    /// Set the actor
    pub fn with_actor(
        mut self,
        channel: String,
        user_id: Option<String>,
        username: Option<String>,
    ) -> Self {
        self.actor = Some(Actor {
            channel,
            user_id,
            username,
        });
        self
    }

    /// Set the action
    pub fn with_action(
        mut self,
        command: String,
        risk_level: String,
        approved: bool,
        allowed: bool,
    ) -> Self {
        self.action = Some(Action {
            command: Some(command),
            risk_level: Some(risk_level),
            approved,
            allowed,
        });
        self
    }

    /// Set the result
    pub fn with_result(
        mut self,
        success: bool,
        exit_code: Option<i32>,
        duration_ms: u64,
        error: Option<String>,
    ) -> Self {
        self.result = Some(ExecutionResult {
            success,
            exit_code,
            duration_ms: Some(duration_ms),
            error,
        });
        self
    }

    /// Build an IPC audit event. `to_agent` and IPC-specific context are
    /// encoded into the `action.command` field as a structured string.
    pub fn ipc(
        event_type: AuditEventType,
        from_agent: &str,
        to_agent: Option<&str>,
        detail: &str,
    ) -> Self {
        let command = match to_agent {
            Some(to) => format!("ipc: from={from_agent} to={to} {detail}"),
            None => format!("ipc: from={from_agent} {detail}"),
        };
        Self::new(event_type)
            .with_actor("ipc".to_string(), Some(from_agent.to_string()), None)
            .with_action(command, "high".to_string(), false, true)
    }

    /// Set security context
    pub fn with_security(mut self, sandbox_backend: Option<String>) -> Self {
        self.security.sandbox_backend = sandbox_backend;
        self
    }
}

/// Audit logger with optional HMAC-SHA256 chain for tamper detection.
pub struct AuditLogger {
    log_path: PathBuf,
    config: AuditConfig,
    buffer: Mutex<Vec<AuditEvent>>,
    /// HMAC key for chain computation (loaded from audit.key file).
    hmac_key: Option<Vec<u8>>,
    /// Previous HMAC in the chain (hex-encoded). Updated on each log().
    prev_hmac: Mutex<String>,
}

/// Structured command execution details for audit logging.
#[derive(Debug, Clone)]
pub struct CommandExecutionLog<'a> {
    pub channel: &'a str,
    pub command: &'a str,
    pub risk_level: &'a str,
    pub approved: bool,
    pub allowed: bool,
    pub success: bool,
    pub duration_ms: u64,
}

impl AuditLogger {
    /// Create a new audit logger.
    ///
    /// If the HMAC key file (`audit.key`) exists in the zeroclaw dir, loads it.
    /// If not, generates a new 32-byte key and saves it. HMAC chain is enabled
    /// automatically when the key is available.
    pub fn new(config: AuditConfig, zeroclaw_dir: PathBuf) -> Result<Self> {
        let log_path = zeroclaw_dir.join(&config.log_path);
        let key_path = zeroclaw_dir.join("audit.key");

        let hmac_key = if config.sign_events {
            match load_or_generate_hmac_key(&key_path) {
                Ok(key) => {
                    tracing::info!("HMAC audit chain enabled");
                    Some(key)
                }
                Err(e) => {
                    tracing::warn!("HMAC audit chain disabled: {e}");
                    None
                }
            }
        } else {
            None
        };

        // Read the last HMAC from the existing log file (if any) to continue the chain
        let prev_hmac = if hmac_key.is_some() {
            read_last_hmac(&log_path).unwrap_or_default()
        } else {
            String::new()
        };

        Ok(Self {
            log_path,
            config,
            buffer: Mutex::new(Vec::new()),
            hmac_key,
            prev_hmac: Mutex::new(prev_hmac),
        })
    }

    /// Log an event, computing HMAC chain if key is available.
    pub fn log(&self, event: &AuditEvent) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        // Check log size and rotate if needed
        self.rotate_if_needed()?;

        let mut event = event.clone();

        // Compute HMAC chain: HMAC(key, "{prev_hmac}|{event_json_without_hmac}")
        if let Some(ref key) = self.hmac_key {
            // Serialize event without hmac field for signing
            event.hmac = None;
            let event_json = serde_json::to_string(&event)?;

            let prev = self.prev_hmac.lock().clone();
            let chain_input = format!("{prev}|{event_json}");
            let hmac_hex = compute_hmac_sha256(key, chain_input.as_bytes());

            event.hmac = Some(hmac_hex.clone());
            *self.prev_hmac.lock() = hmac_hex;
        }

        // Serialize and write
        let line = serde_json::to_string(&event)?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_path)?;

        writeln!(file, "{line}")?;
        file.sync_all()?;

        Ok(())
    }

    /// Log a command execution event.
    pub fn log_command_event(&self, entry: CommandExecutionLog<'_>) -> Result<()> {
        let event = AuditEvent::new(AuditEventType::CommandExecution)
            .with_actor(entry.channel.to_string(), None, None)
            .with_action(
                entry.command.to_string(),
                entry.risk_level.to_string(),
                entry.approved,
                entry.allowed,
            )
            .with_result(entry.success, None, entry.duration_ms, None);

        self.log(&event)
    }

    /// Backward-compatible helper to log a command execution event.
    #[allow(clippy::too_many_arguments)]
    pub fn log_command(
        &self,
        channel: &str,
        command: &str,
        risk_level: &str,
        approved: bool,
        allowed: bool,
        success: bool,
        duration_ms: u64,
    ) -> Result<()> {
        self.log_command_event(CommandExecutionLog {
            channel,
            command,
            risk_level,
            approved,
            allowed,
            success,
            duration_ms,
        })
    }

    /// Rotate log if it exceeds max size
    fn rotate_if_needed(&self) -> Result<()> {
        if let Ok(metadata) = std::fs::metadata(&self.log_path) {
            let current_size_mb = metadata.len() / (1024 * 1024);
            if current_size_mb >= u64::from(self.config.max_size_mb) {
                self.rotate()?;
            }
        }
        Ok(())
    }

    /// Rotate the log file
    fn rotate(&self) -> Result<()> {
        for i in (1..10).rev() {
            let old_name = format!("{}.{}.log", self.log_path.display(), i);
            let new_name = format!("{}.{}.log", self.log_path.display(), i + 1);
            let _ = std::fs::rename(&old_name, &new_name);
        }

        let rotated = format!("{}.1.log", self.log_path.display());
        std::fs::rename(&self.log_path, &rotated)?;
        Ok(())
    }
}

// ── HMAC chain helpers ──────────────────────────────────────────

/// Compute HMAC-SHA256 and return hex-encoded result.
fn compute_hmac_sha256(key: &[u8], data: &[u8]) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    type HmacSha256 = Hmac<Sha256>;
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC can take key of any size");
    mac.update(data);
    hex::encode(mac.finalize().into_bytes())
}

/// Load HMAC key from file, or generate and save a new one (32 random bytes).
fn load_or_generate_hmac_key(path: &std::path::Path) -> Result<Vec<u8>> {
    if path.exists() {
        let data = std::fs::read(path)?;
        if data.len() < 16 {
            anyhow::bail!("HMAC key file too short ({} bytes)", data.len());
        }
        Ok(data)
    } else {
        let key: [u8; 32] = rand::random();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, key)?;
        Ok(key.to_vec())
    }
}

/// Read the last HMAC value from an existing audit log file.
/// Returns empty string if file doesn't exist or has no HMAC entries.
fn read_last_hmac(log_path: &std::path::Path) -> Result<String> {
    use std::io::BufRead;

    if !log_path.exists() {
        return Ok(String::new());
    }

    let file = std::fs::File::open(log_path)?;
    let reader = std::io::BufReader::new(file);
    let mut last_hmac = String::new();

    for line in reader.lines() {
        let line = line?;
        if let Ok(event) = serde_json::from_str::<AuditEvent>(&line) {
            if let Some(hmac) = event.hmac {
                last_hmac = hmac;
            }
        }
    }

    Ok(last_hmac)
}

/// Verify the HMAC chain in an audit log file.
///
/// Returns `Ok(count)` with the number of verified entries, or `Err` with
/// details about the first broken link.
pub fn verify_audit_chain(log_path: &std::path::Path, key_path: &std::path::Path) -> Result<usize> {
    use std::io::BufRead;

    let key = std::fs::read(key_path)
        .map_err(|e| anyhow::anyhow!("Failed to read HMAC key at {}: {e}", key_path.display()))?;

    let file = std::fs::File::open(log_path)
        .map_err(|e| anyhow::anyhow!("Failed to open audit log at {}: {e}", log_path.display()))?;
    let reader = std::io::BufReader::new(file);

    let mut prev_hmac = String::new();
    let mut verified = 0usize;

    for (line_num, line) in reader.lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        let event: AuditEvent = serde_json::from_str(&line)
            .map_err(|e| anyhow::anyhow!("Line {}: invalid JSON: {e}", line_num + 1))?;

        let stored_hmac = match &event.hmac {
            Some(h) => h.clone(),
            None => {
                // Entry without HMAC — skip (pre-chain entries)
                continue;
            }
        };

        // Recompute: serialize event without hmac, then HMAC("{prev}|{json}")
        let mut event_for_hash = event;
        event_for_hash.hmac = None;
        let event_json = serde_json::to_string(&event_for_hash)?;
        let chain_input = format!("{prev_hmac}|{event_json}");
        let expected_hmac = compute_hmac_sha256(&key, chain_input.as_bytes());

        if stored_hmac != expected_hmac {
            anyhow::bail!(
                "HMAC chain broken at line {} (event_id={}): \
                 expected={}, stored={}",
                line_num + 1,
                event_for_hash.event_id,
                &expected_hmac[..16],
                &stored_hmac[..16],
            );
        }

        prev_hmac = stored_hmac;
        verified += 1;
    }

    Ok(verified)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn audit_event_new_creates_unique_id() {
        let event1 = AuditEvent::new(AuditEventType::CommandExecution);
        let event2 = AuditEvent::new(AuditEventType::CommandExecution);
        assert_ne!(event1.event_id, event2.event_id);
    }

    #[test]
    fn audit_event_with_actor() {
        let event = AuditEvent::new(AuditEventType::CommandExecution).with_actor(
            "telegram".to_string(),
            Some("123".to_string()),
            Some("@alice".to_string()),
        );

        assert!(event.actor.is_some());
        let actor = event.actor.as_ref().unwrap();
        assert_eq!(actor.channel, "telegram");
        assert_eq!(actor.user_id, Some("123".to_string()));
        assert_eq!(actor.username, Some("@alice".to_string()));
    }

    #[test]
    fn audit_event_with_action() {
        let event = AuditEvent::new(AuditEventType::CommandExecution).with_action(
            "ls -la".to_string(),
            "low".to_string(),
            false,
            true,
        );

        assert!(event.action.is_some());
        let action = event.action.as_ref().unwrap();
        assert_eq!(action.command, Some("ls -la".to_string()));
        assert_eq!(action.risk_level, Some("low".to_string()));
    }

    #[test]
    fn audit_event_serializes_to_json() {
        let event = AuditEvent::new(AuditEventType::CommandExecution)
            .with_actor("telegram".to_string(), None, None)
            .with_action("ls".to_string(), "low".to_string(), false, true)
            .with_result(true, Some(0), 15, None);

        let json = serde_json::to_string(&event);
        assert!(json.is_ok());
        let json = json.expect("serialize");
        let parsed: AuditEvent = serde_json::from_str(json.as_str()).expect("parse");
        assert!(parsed.actor.is_some());
        assert!(parsed.action.is_some());
        assert!(parsed.result.is_some());
    }

    #[test]
    fn audit_logger_disabled_does_not_create_file() -> Result<()> {
        let tmp = TempDir::new()?;
        let config = AuditConfig {
            enabled: false,
            ..Default::default()
        };
        let logger = AuditLogger::new(config, tmp.path().to_path_buf())?;
        let event = AuditEvent::new(AuditEventType::CommandExecution);

        logger.log(&event)?;

        // File should not exist since logging is disabled
        assert!(!tmp.path().join("audit.log").exists());
        Ok(())
    }

    // ── §8.1 Log rotation tests ─────────────────────────────

    #[tokio::test]
    async fn audit_logger_writes_event_when_enabled() -> Result<()> {
        let tmp = TempDir::new()?;
        let config = AuditConfig {
            enabled: true,
            max_size_mb: 10,
            ..Default::default()
        };
        let logger = AuditLogger::new(config, tmp.path().to_path_buf())?;
        let event = AuditEvent::new(AuditEventType::CommandExecution)
            .with_actor("cli".to_string(), None, None)
            .with_action("ls".to_string(), "low".to_string(), false, true);

        logger.log(&event)?;

        let log_path = tmp.path().join("audit.log");
        assert!(log_path.exists(), "audit log file must be created");

        let content = tokio::fs::read_to_string(&log_path).await?;
        assert!(!content.is_empty(), "audit log must not be empty");

        let parsed: AuditEvent = serde_json::from_str(content.trim())?;
        assert!(parsed.action.is_some());
        Ok(())
    }

    #[tokio::test]
    async fn audit_log_command_event_writes_structured_entry() -> Result<()> {
        let tmp = TempDir::new()?;
        let config = AuditConfig {
            enabled: true,
            max_size_mb: 10,
            ..Default::default()
        };
        let logger = AuditLogger::new(config, tmp.path().to_path_buf())?;

        logger.log_command_event(CommandExecutionLog {
            channel: "telegram",
            command: "echo test",
            risk_level: "low",
            approved: false,
            allowed: true,
            success: true,
            duration_ms: 42,
        })?;

        let log_path = tmp.path().join("audit.log");
        let content = tokio::fs::read_to_string(&log_path).await?;
        let parsed: AuditEvent = serde_json::from_str(content.trim())?;

        let action = parsed.action.unwrap();
        assert_eq!(action.command, Some("echo test".to_string()));
        assert_eq!(action.risk_level, Some("low".to_string()));
        assert!(action.allowed);

        let result = parsed.result.unwrap();
        assert!(result.success);
        assert_eq!(result.duration_ms, Some(42));
        Ok(())
    }

    #[test]
    fn audit_rotation_creates_numbered_backup() -> Result<()> {
        let tmp = TempDir::new()?;
        let config = AuditConfig {
            enabled: true,
            max_size_mb: 0, // Force rotation on first write
            ..Default::default()
        };
        let logger = AuditLogger::new(config, tmp.path().to_path_buf())?;

        // Write initial content that triggers rotation
        let log_path = tmp.path().join("audit.log");
        std::fs::write(&log_path, "initial content\n")?;

        let event = AuditEvent::new(AuditEventType::CommandExecution);
        logger.log(&event)?;

        let rotated = format!("{}.1.log", log_path.display());
        assert!(
            std::path::Path::new(&rotated).exists(),
            "rotation must create .1.log backup"
        );
        Ok(())
    }

    // ── HMAC chain tests ────────────────────────────────────────

    #[test]
    fn hmac_chain_write_and_verify() -> Result<()> {
        let tmp = TempDir::new().unwrap();
        let config = AuditConfig {
            enabled: true,
            log_path: "audit.log".into(),
            max_size_mb: 100,
            sign_events: true,
        };

        let logger = AuditLogger::new(config, tmp.path().to_path_buf())?;

        // Log 3 events
        for _ in 0..3 {
            logger.log(&AuditEvent::new(AuditEventType::SecurityEvent))?;
        }

        // Verify chain
        let log_path = tmp.path().join("audit.log");
        let key_path = tmp.path().join("audit.key");
        let verified = verify_audit_chain(&log_path, &key_path)?;
        assert_eq!(verified, 3);
        Ok(())
    }

    #[test]
    fn hmac_chain_detects_tampered_entry() -> Result<()> {
        let tmp = TempDir::new().unwrap();
        let config = AuditConfig {
            enabled: true,
            log_path: "audit.log".into(),
            max_size_mb: 100,
            sign_events: true,
        };

        let logger = AuditLogger::new(config, tmp.path().to_path_buf())?;
        logger.log(&AuditEvent::new(AuditEventType::SecurityEvent))?;
        logger.log(&AuditEvent::new(AuditEventType::CommandExecution))?;
        drop(logger);

        // Tamper with the log: modify a payload
        let log_path = tmp.path().join("audit.log");
        let content = std::fs::read_to_string(&log_path)?;
        let tampered = content.replace("command_execution", "policy_violation");
        std::fs::write(&log_path, tampered)?;

        let key_path = tmp.path().join("audit.key");
        let result = verify_audit_chain(&log_path, &key_path);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("chain broken"));
        Ok(())
    }

    #[test]
    fn hmac_chain_continues_after_restart() -> Result<()> {
        let tmp = TempDir::new().unwrap();
        let config = AuditConfig {
            enabled: true,
            log_path: "audit.log".into(),
            max_size_mb: 100,
            sign_events: true,
        };

        // First logger session
        {
            let logger = AuditLogger::new(config.clone(), tmp.path().to_path_buf())?;
            logger.log(&AuditEvent::new(AuditEventType::SecurityEvent))?;
            logger.log(&AuditEvent::new(AuditEventType::IpcSend))?;
        }

        // Second logger session (simulates restart)
        {
            let logger = AuditLogger::new(config, tmp.path().to_path_buf())?;
            logger.log(&AuditEvent::new(AuditEventType::IpcBlocked))?;
        }

        // All 3 entries should form a valid chain
        let log_path = tmp.path().join("audit.log");
        let key_path = tmp.path().join("audit.key");
        let verified = verify_audit_chain(&log_path, &key_path)?;
        assert_eq!(verified, 3);
        Ok(())
    }

    #[test]
    fn hmac_chain_detects_deleted_entry() -> Result<()> {
        let tmp = TempDir::new().unwrap();
        let config = AuditConfig {
            enabled: true,
            log_path: "audit.log".into(),
            max_size_mb: 100,
            sign_events: true,
        };

        let logger = AuditLogger::new(config, tmp.path().to_path_buf())?;
        for _ in 0..3 {
            logger.log(&AuditEvent::new(AuditEventType::SecurityEvent))?;
        }
        drop(logger);

        // Delete the second line
        let log_path = tmp.path().join("audit.log");
        let content = std::fs::read_to_string(&log_path)?;
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 3);
        let tampered = format!("{}\n{}\n", lines[0], lines[2]);
        std::fs::write(&log_path, tampered)?;

        let key_path = tmp.path().join("audit.key");
        let result = verify_audit_chain(&log_path, &key_path);
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn hmac_key_generated_on_first_use() {
        let tmp = TempDir::new().unwrap();
        let key_path = tmp.path().join("audit.key");
        assert!(!key_path.exists());

        let key = load_or_generate_hmac_key(&key_path).unwrap();
        assert_eq!(key.len(), 32);
        assert!(key_path.exists());

        // Second load returns the same key
        let key2 = load_or_generate_hmac_key(&key_path).unwrap();
        assert_eq!(key, key2);
    }
}
