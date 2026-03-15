//! Execution profiles for Phase 3A ephemeral agent spawn.
//!
//! Defines trust-derived execution boundaries (sandbox, filesystem, network,
//! autonomy) and optional workload profiles that can only narrow — never
//! widen — the boundary.

use crate::config::SandboxBackend;
use crate::security::traits::Sandbox;
use serde::{Deserialize, Serialize};

/// Execution boundary derived from trust level.
///
/// Controls what the child process can actually do at the OS level.
/// **Cannot be weakened** by workload profiles or config.
#[derive(Debug, Clone)]
pub struct ExecutionBoundary {
    /// Human-readable name for this boundary.
    pub name: &'static str,
    /// Trust levels that map to this boundary.
    pub trust_range: (u8, u8),
    /// Required sandbox backend(s), in preference order.
    /// Empty = sandbox optional (NoopSandbox allowed).
    pub required_backends: Vec<SandboxBackend>,
    /// Whether NoopSandbox is acceptable (L0-L1 only).
    pub noop_allowed: bool,
    /// Autonomy level ceiling for this boundary.
    pub autonomy: BoundaryAutonomy,
    /// Tools ceiling — maximum set of tools available.
    pub tools_ceiling: Option<Vec<String>>,
}

/// Autonomy level for an execution boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundaryAutonomy {
    Full,
    Supervised,
    ReadOnly,
}

/// Error returned when sandbox requirements cannot be met.
#[derive(Debug, Clone)]
pub struct SpawnSandboxError {
    pub trust_level: u8,
    pub boundary: &'static str,
    pub required: Vec<String>,
    pub available: String,
}

impl std::fmt::Display for SpawnSandboxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Sandbox unavailable for L{} ({}): requires {:?}, have {}",
            self.trust_level, self.boundary, self.required, self.available
        )
    }
}

impl std::error::Error for SpawnSandboxError {}

/// Optional workload profile that can only narrow the execution boundary.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WorkloadProfile {
    /// LLM model override for the child.
    pub model: Option<String>,
    /// System prompt prefix/template.
    pub prompt_template: Option<String>,
    /// Tool subset available to the child. Must be ⊆ boundary tools ceiling.
    pub allowed_tools: Option<Vec<String>>,
    /// Maximum output tokens for the child.
    pub max_output_tokens: Option<u32>,
}

/// Resolved spawn configuration after applying boundary + workload.
#[derive(Debug, Clone)]
pub struct SpawnConfig {
    pub model: Option<String>,
    pub prompt_template: Option<String>,
    pub allowed_tools: Option<Vec<String>>,
    pub max_output_tokens: Option<u32>,
    pub autonomy: BoundaryAutonomy,
}

/// Get the execution boundary for a given trust level.
///
/// This mapping is hard-coded and not configurable — trust level controls
/// the cage, workload controls what happens inside.
pub fn execution_boundary(trust_level: u8) -> ExecutionBoundary {
    match trust_level {
        0..=1 => ExecutionBoundary {
            name: "coordinator",
            trust_range: (0, 1),
            required_backends: vec![],
            noop_allowed: true,
            autonomy: BoundaryAutonomy::Full,
            tools_ceiling: None, // all tools
        },
        2 => ExecutionBoundary {
            name: "privileged",
            trust_range: (2, 2),
            required_backends: vec![SandboxBackend::Landlock, SandboxBackend::Docker],
            noop_allowed: false,
            autonomy: BoundaryAutonomy::Supervised,
            tools_ceiling: None, // all tools, but supervised
        },
        3 => ExecutionBoundary {
            name: "worker",
            trust_range: (3, 3),
            required_backends: vec![
                SandboxBackend::Bubblewrap,
                SandboxBackend::Landlock,
                SandboxBackend::Docker,
            ],
            noop_allowed: false,
            autonomy: BoundaryAutonomy::Supervised,
            tools_ceiling: None, // narrowed by workload if set
        },
        _ => ExecutionBoundary {
            name: "restricted",
            trust_range: (4, 4),
            required_backends: vec![SandboxBackend::Bubblewrap, SandboxBackend::Docker],
            noop_allowed: false,
            autonomy: BoundaryAutonomy::ReadOnly,
            tools_ceiling: Some(vec![
                "memory_read".into(),
                "memory_write".into(),
                "web_search".into(),
                "web_fetch".into(),
            ]),
        },
    }
}

/// Verify that the required sandbox backend is available for the given boundary.
///
/// **Fail-closed**: for L2-L4, if no required backend is available, returns
/// `Err(SpawnSandboxError)`. Fallback is only allowed toward stricter
/// isolation (e.g. Docker instead of Bubblewrap), never toward NoopSandbox.
pub fn require_sandbox(
    trust_level: u8,
    boundary: &ExecutionBoundary,
    current_sandbox: &dyn Sandbox,
) -> Result<(), SpawnSandboxError> {
    // L0-L1: NoopSandbox is fine
    if boundary.noop_allowed {
        return Ok(());
    }

    let sandbox_name = current_sandbox.name();

    // Check if the current sandbox matches any of the required backends
    for required in &boundary.required_backends {
        let required_name = sandbox_backend_name(required);
        if sandbox_name == required_name {
            return Ok(());
        }
    }

    // Fail-closed: required sandbox not available
    Err(SpawnSandboxError {
        trust_level,
        boundary: boundary.name,
        required: boundary
            .required_backends
            .iter()
            .map(|b| sandbox_backend_name(b).to_string())
            .collect(),
        available: sandbox_name.to_string(),
    })
}

/// Apply a workload profile on top of an execution boundary.
///
/// The workload can only **narrow** the boundary — if it requests tools
/// outside the boundary's ceiling, returns an error.
pub fn apply_workload(
    boundary: &ExecutionBoundary,
    workload: &WorkloadProfile,
) -> Result<SpawnConfig, String> {
    // Validate tools subset
    let allowed_tools = if let Some(ref requested) = workload.allowed_tools {
        if let Some(ref ceiling) = boundary.tools_ceiling {
            // Verify requested ⊆ ceiling
            for tool in requested {
                if !ceiling.contains(tool) {
                    return Err(format!(
                        "Tool '{}' not allowed for {} boundary",
                        tool, boundary.name
                    ));
                }
            }
        }
        Some(requested.clone())
    } else {
        boundary.tools_ceiling.clone()
    };

    Ok(SpawnConfig {
        model: workload.model.clone(),
        prompt_template: workload.prompt_template.clone(),
        allowed_tools,
        max_output_tokens: workload.max_output_tokens,
        autonomy: boundary.autonomy,
    })
}

fn sandbox_backend_name(backend: &SandboxBackend) -> &'static str {
    match backend {
        SandboxBackend::Auto => "auto",
        SandboxBackend::Landlock => "landlock",
        SandboxBackend::Firejail => "firejail",
        SandboxBackend::Bubblewrap => "bubblewrap",
        SandboxBackend::Docker => "docker",
        SandboxBackend::None => "none",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::traits::NoopSandbox;

    #[test]
    fn execution_boundary_l0_allows_noop() {
        let boundary = execution_boundary(0);
        assert_eq!(boundary.name, "coordinator");
        assert!(boundary.noop_allowed);
        assert!(boundary.tools_ceiling.is_none());
        assert_eq!(boundary.autonomy, BoundaryAutonomy::Full);
    }

    #[test]
    fn execution_boundary_l1_allows_noop() {
        let boundary = execution_boundary(1);
        assert_eq!(boundary.name, "coordinator");
        assert!(boundary.noop_allowed);
    }

    #[test]
    fn execution_boundary_l2_requires_sandbox() {
        let boundary = execution_boundary(2);
        assert_eq!(boundary.name, "privileged");
        assert!(!boundary.noop_allowed);
        assert!(!boundary.required_backends.is_empty());
        assert_eq!(boundary.autonomy, BoundaryAutonomy::Supervised);
    }

    #[test]
    fn execution_boundary_l3_requires_sandbox() {
        let boundary = execution_boundary(3);
        assert_eq!(boundary.name, "worker");
        assert!(!boundary.noop_allowed);
        assert_eq!(boundary.autonomy, BoundaryAutonomy::Supervised);
    }

    #[test]
    fn execution_boundary_l4_is_restricted() {
        let boundary = execution_boundary(4);
        assert_eq!(boundary.name, "restricted");
        assert!(!boundary.noop_allowed);
        assert_eq!(boundary.autonomy, BoundaryAutonomy::ReadOnly);
        assert!(boundary.tools_ceiling.is_some());
        let ceiling = boundary.tools_ceiling.unwrap();
        assert!(ceiling.contains(&"memory_read".to_string()));
        assert!(!ceiling.contains(&"shell".to_string()));
    }

    #[test]
    fn require_sandbox_l0_accepts_noop() {
        let boundary = execution_boundary(0);
        let sandbox = NoopSandbox;
        assert!(require_sandbox(0, &boundary, &sandbox).is_ok());
    }

    #[test]
    fn require_sandbox_l2_rejects_noop() {
        let boundary = execution_boundary(2);
        let sandbox = NoopSandbox;
        let err = require_sandbox(2, &boundary, &sandbox).unwrap_err();
        assert_eq!(err.trust_level, 2);
        assert_eq!(err.boundary, "privileged");
        assert_eq!(err.available, "none");
    }

    #[test]
    fn require_sandbox_l3_rejects_noop() {
        let boundary = execution_boundary(3);
        let sandbox = NoopSandbox;
        assert!(require_sandbox(3, &boundary, &sandbox).is_err());
    }

    #[test]
    fn require_sandbox_l4_rejects_noop() {
        let boundary = execution_boundary(4);
        let sandbox = NoopSandbox;
        let err = require_sandbox(4, &boundary, &sandbox).unwrap_err();
        assert_eq!(err.boundary, "restricted");
    }

    #[test]
    fn apply_workload_empty_inherits_boundary() {
        let boundary = execution_boundary(3);
        let workload = WorkloadProfile::default();
        let config = apply_workload(&boundary, &workload).unwrap();
        assert!(config.model.is_none());
        assert!(config.allowed_tools.is_none());
        assert_eq!(config.autonomy, BoundaryAutonomy::Supervised);
    }

    #[test]
    fn apply_workload_narrows_tools() {
        let boundary = execution_boundary(4);
        let workload = WorkloadProfile {
            allowed_tools: Some(vec!["memory_read".into()]),
            ..Default::default()
        };
        let config = apply_workload(&boundary, &workload).unwrap();
        let tools = config.allowed_tools.unwrap();
        assert_eq!(tools, vec!["memory_read"]);
    }

    #[test]
    fn apply_workload_rejects_tools_outside_ceiling() {
        let boundary = execution_boundary(4);
        let workload = WorkloadProfile {
            allowed_tools: Some(vec!["shell".into()]),
            ..Default::default()
        };
        let err = apply_workload(&boundary, &workload).unwrap_err();
        assert!(err.contains("shell"));
        assert!(err.contains("restricted"));
    }

    #[test]
    fn apply_workload_with_model_override() {
        let boundary = execution_boundary(3);
        let workload = WorkloadProfile {
            model: Some("claude-haiku-4-5".into()),
            max_output_tokens: Some(1024),
            ..Default::default()
        };
        let config = apply_workload(&boundary, &workload).unwrap();
        assert_eq!(config.model.as_deref(), Some("claude-haiku-4-5"));
        assert_eq!(config.max_output_tokens, Some(1024));
    }

    #[test]
    fn apply_workload_unrestricted_boundary_allows_any_tools() {
        let boundary = execution_boundary(1); // coordinator — no ceiling
        let workload = WorkloadProfile {
            allowed_tools: Some(vec!["shell".into(), "file_write".into()]),
            ..Default::default()
        };
        let config = apply_workload(&boundary, &workload).unwrap();
        let tools = config.allowed_tools.unwrap();
        assert!(tools.contains(&"shell".into()));
    }

    #[test]
    fn spawn_sandbox_error_display() {
        let err = SpawnSandboxError {
            trust_level: 3,
            boundary: "worker",
            required: vec!["bubblewrap".into(), "docker".into()],
            available: "none".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("L3"));
        assert!(msg.contains("worker"));
        assert!(msg.contains("bubblewrap"));
    }
}
