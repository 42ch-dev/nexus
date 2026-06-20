//! Capability Registry — runtime SSOT for `nexus.*` host tool dispatch.
//!
//! V1.53 P0: Introduces a unified registry with 7-field row shape
//! (id → access → admission → handler → ACP wire → failure mode → test vector).
//! Migrated from `HostToolExecutor`'s `dispatch_tool()` match table via an
//! adapter-first approach: introduce → cutover → cleanup.
//!
//! # Architecture
//!
//! ```text
//! HostToolExecutor::execute()
//!   → admission_pipeline()     (5 gates: allowlist → creator → workspace → policy → audit)
//!   → CapabilityRegistry::dispatch()  (lookup → invoke handler)
//! ```
//!
//! # Sub-phase tracker (P0)
//!
//! - **Sub-phase 1 (introduce)**: Registry exists; old dispatch is primary path.
//!   `HostToolExecutor::registry_dispatch()` is a parallel path for parity testing.
//! - **Sub-phase 2 (cutover)**: `HostToolExecutor::execute()` routes through registry.
//!   Old `dispatch_tool()` is present but unused.
//! - **Sub-phase 3 (cleanup)**: Old `dispatch_tool()` removed. No lingering parallel paths.

use crate::api::errors::NexusApiError;
use crate::api::handlers::host_tool_executor::ToolExecuteRequest;
use crate::workspace::WorkspaceState;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

// ─── Registry types ────────────────────────────────────────────────────────

/// Unified handler function signature for all registered capabilities.
///
/// Takes references to the tool request, workspace state, and creator id,
/// returns a boxed future resolving to `Result<serde_json::Value, NexusApiError>`.
pub type RegistryHandlerFn = for<'a> fn(
    &'a ToolExecuteRequest,
    &'a WorkspaceState,
    &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>>;

// ─── Field types ───────────────────────────────────────────────────────────

/// Access classification for a capability row.
///
/// Used by admission gates and audit to determine the
/// risk profile of a capability at dispatch time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Access {
    /// Read-only; no side effects.
    Read,
    /// Mutation-capable; may write to DB, filesystem, or state.
    Write,
    /// Access depends on runtime policy (e.g. `permissions.toml`
    /// or DA-005 `ContextPermissionGrant`).
    PolicyGated,
}

/// Ordered fail-closed admission gate before handler dispatch.
///
/// Each gate must pass (or be explicitly skipped for a given
/// capability) before the handler is invoked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdmissionGate {
    /// Tool ID must be in the allowlist.
    Allowlist,
    /// Active creator must exist (for `nexus.*` tools).
    ActiveCreator,
    /// Operation must be within workspace bounds.
    WorkspaceBounds,
    /// `permissions.toml` / policy must grant the capability.
    PermissionPolicy,
    /// Audit log entry must be written (always last gate).
    AuditLog,
}

/// ACP wire contract reference for a capability.
///
/// Points to the request, response, and error schema shapes
/// that the capability exposes to ACP-facing callers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcpWire {
    /// JSON Schema reference (or inline) for the request shape.
    pub request_schema_ref: &'static str,
    /// JSON Schema reference (or inline) for the success response shape.
    pub response_schema_ref: &'static str,
    /// JSON Schema reference (or inline) for the error response shape.
    pub error_schema_ref: &'static str,
}

/// Stable failure mode contract for a capability.
///
/// Defines the error surface a caller can expect when
/// the capability is denied or fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FailureMode {
    /// Capability is not supported in this runtime configuration.
    NotSupported,
    /// Policy (permissions or admission gate) blocked execution.
    PolicyBlocked,
    /// Authentication/authorization failed.
    Forbidden,
    /// Input validation failed.
    InvalidInput,
    /// Internal error (database, filesystem, etc.).
    Internal,
}

/// Test vector descriptor for a capability row.
///
/// Each row must have at least one success and one
/// failure test proving the handler works correctly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestVector {
    /// Human-readable description of what the test covers.
    pub description: &'static str,
    /// Expected outcome: "success", "failure:policy_blocked", etc.
    pub expected_outcome: &'static str,
    /// Name of the test function (for grep-ability).
    pub test_fn_name: &'static str,
}

// ─── Capability row ────────────────────────────────────────────────────────

/// A single row in the capability registry.
///
/// Bundles all 7 fields: id, access, admission gates,
/// handler binding, ACP wire contract, failure mode contract,
/// and test vector.
#[derive(Clone)]
pub struct CapabilityRow {
    /// Stable `nexus.*` capability id (e.g. `"nexus.work.get"`).
    pub id: &'static str,
    /// Access classification.
    pub access: Access,
    /// Ordered fail-closed admission gates.
    pub admission: Vec<AdmissionGate>,
    /// Handler function binding.
    pub handler: RegistryHandlerFn,
    /// ACP wire schema references.
    pub acp_wire: AcpWire,
    /// Expected failure mode when denied.
    pub failure_mode: FailureMode,
    /// Test vector descriptor.
    pub handler_test_vector: TestVector,
}

// ─── Registry ──────────────────────────────────────────────────────────────

/// Central registry for `nexus.*` host tool capabilities.
///
/// Built once at daemon startup. Provides O(1) lookup by
/// capability id and a unified `dispatch()` method that
/// mirrors the old `dispatch_tool()` behavior.
pub struct CapabilityRegistry {
    rows: HashMap<&'static str, CapabilityRow>,
}

impl CapabilityRegistry {
    /// Create an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            rows: HashMap::new(),
        }
    }

    /// Register a capability row.
    ///
    /// # Panics
    ///
    /// Panics if a row with the same `id` is already registered
    /// (duplicate capability ids are a programmer error).
    pub fn register(&mut self, row: CapabilityRow) {
        if self.rows.contains_key(row.id) {
            panic!("duplicate capability id in registry: {}", row.id);
        }
        self.rows.insert(row.id, row);
    }

    /// Look up a capability row by id.
    #[must_use]
    pub fn lookup(&self, id: &str) -> Option<&CapabilityRow> {
        self.rows.get(id)
    }

    /// Iterate over all registered capability ids.
    pub fn ids(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.rows.keys().copied()
    }

    /// Number of registered capabilities.
    #[must_use]
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Return whether the registry is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Dispatch a tool request through the registry.
    ///
    /// Looks up the capability by `tool_name`, then invokes the
    /// registered handler. Returns `NotSupported` if the tool
    /// is not registered.
    pub async fn dispatch(
        &self,
        req: &ToolExecuteRequest,
        state: &WorkspaceState,
        creator_id: &str,
    ) -> Result<serde_json::Value, NexusApiError> {
        match self.lookup(&req.tool_name) {
            Some(row) => (row.handler)(req, state, creator_id).await,
            None => Err(NexusApiError::BadRequest {
                code: "NOT_SUPPORTED".to_string(),
                message: format!("unsupported tool: {}", req.tool_name),
            }),
        }
    }
}

impl Default for CapabilityRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Registry constructor ──────────────────────────────────────────────────

/// Create a registry pre-populated with all 8 V1.34 host tools.
///
/// Each handler is wired to the corresponding `pub(crate)` wrapper
/// function in `host_tool_executor.rs`. The wrapper functions exist
/// solely to bridge the existing handler implementations to the
/// unified `RegistryHandlerFn` signature.
#[must_use]
pub fn host_tool_registry() -> CapabilityRegistry {
    use crate::api::handlers::host_tool_executor as hte;
    let mut reg = CapabilityRegistry::new();

    // ── nexus.* tools (V1.34) ──
    reg.register(CapabilityRow {
        id: "nexus.context.whoami",
        access: Access::Read,
        admission: vec![
            AdmissionGate::Allowlist,
            AdmissionGate::ActiveCreator,
            AdmissionGate::PermissionPolicy,
            AdmissionGate::AuditLog,
        ],
        handler: hte::registry_context_whoami,
        acp_wire: AcpWire {
            request_schema_ref: "{}",
            response_schema_ref: r#"{"creator_id":"string","workspace_slug":"string"}"#,
            error_schema_ref: r#"{"code":"FORBIDDEN|POLICY_BLOCKED|NOT_SUPPORTED"}"#,
        },
        failure_mode: FailureMode::Forbidden,
        handler_test_vector: TestVector {
            description: "whoami returns active creator_id and workspace_slug",
            expected_outcome: "success",
            test_fn_name: "whoami_returns_active_creator",
        },
    });

    reg.register(CapabilityRow {
        id: "nexus.workspace.info",
        access: Access::Read,
        admission: vec![
            AdmissionGate::Allowlist,
            AdmissionGate::ActiveCreator,
            AdmissionGate::PermissionPolicy,
            AdmissionGate::AuditLog,
        ],
        handler: hte::registry_workspace_info,
        acp_wire: AcpWire {
            request_schema_ref: "{}",
            response_schema_ref: r#"{"creator_id":"string","workspace_slug":"string","workspace_path":"string","runtime_mode":"string","initialized":"bool"}"#,
            error_schema_ref: r#"{"code":"FORBIDDEN|POLICY_BLOCKED|NOT_SUPPORTED"}"#,
        },
        failure_mode: FailureMode::Forbidden,
        handler_test_vector: TestVector {
            description: "workspace info returns workspace details",
            expected_outcome: "success",
            test_fn_name: "workspace_info_returns_details",
        },
    });

    reg.register(CapabilityRow {
        id: "nexus.work.get",
        access: Access::Read,
        admission: vec![
            AdmissionGate::Allowlist,
            AdmissionGate::ActiveCreator,
            AdmissionGate::WorkspaceBounds,
            AdmissionGate::PermissionPolicy,
            AdmissionGate::AuditLog,
        ],
        handler: hte::registry_work_get,
        acp_wire: AcpWire {
            request_schema_ref: r#"{"work_id":"string"}"#,
            response_schema_ref: "WorkApiDto",
            error_schema_ref: r#"{"code":"FORBIDDEN|INVALID_INPUT|NOT_SUPPORTED"}"#,
        },
        failure_mode: FailureMode::Forbidden,
        handler_test_vector: TestVector {
            description: "work get returns Work row for active creator",
            expected_outcome: "success",
            test_fn_name: "work_get_happy_path",
        },
    });

    reg.register(CapabilityRow {
        id: "nexus.work.patch",
        access: Access::Write,
        admission: vec![
            AdmissionGate::Allowlist,
            AdmissionGate::ActiveCreator,
            AdmissionGate::WorkspaceBounds,
            AdmissionGate::PermissionPolicy,
            AdmissionGate::AuditLog,
        ],
        handler: hte::registry_work_patch,
        acp_wire: AcpWire {
            request_schema_ref: r#"{"work_id":"string","title?":"string","inspiration_log?":"array","stage_metadata?":"object"}"#,
            response_schema_ref: "WorkApiDto",
            error_schema_ref: r#"{"code":"FORBIDDEN|INVALID_INPUT|POLICY_BLOCKED|NOT_SUPPORTED"}"#,
        },
        failure_mode: FailureMode::Forbidden,
        handler_test_vector: TestVector {
            description: "work patch rejects stage field per spec §4.4",
            expected_outcome: "failure:invalid_input",
            test_fn_name: "work_patch_rejects_stage_field",
        },
    });

    reg.register(CapabilityRow {
        id: "nexus.orchestration.schedule_status",
        access: Access::Read,
        admission: vec![
            AdmissionGate::Allowlist,
            AdmissionGate::ActiveCreator,
            AdmissionGate::WorkspaceBounds,
            AdmissionGate::PermissionPolicy,
            AdmissionGate::AuditLog,
        ],
        handler: hte::registry_schedule_status,
        acp_wire: AcpWire {
            request_schema_ref: r#"{"work_id":"string"}"#,
            response_schema_ref: r#"{"work_id":"string","schedule_ids":"array","count":"int"}"#,
            error_schema_ref: r#"{"code":"FORBIDDEN|INVALID_INPUT|NOT_SUPPORTED"}"#,
        },
        failure_mode: FailureMode::Forbidden,
        handler_test_vector: TestVector {
            description: "schedule status returns schedule ids for work",
            expected_outcome: "success",
            test_fn_name: "schedule_status_returns_ids",
        },
    });

    reg.register(CapabilityRow {
        id: "nexus.context.assemble",
        access: Access::Read,
        admission: vec![
            AdmissionGate::Allowlist,
            AdmissionGate::ActiveCreator,
            AdmissionGate::PermissionPolicy,
            AdmissionGate::AuditLog,
        ],
        handler: hte::registry_context_assemble,
        acp_wire: AcpWire {
            request_schema_ref: r#"{"work_id?":"string","requires_platform?":"bool"}"#,
            response_schema_ref: r#"{"mode":"string","creator_id":"string","assembled_at":"string"}"#,
            error_schema_ref: r#"{"code":"POLICY_BLOCKED|FORBIDDEN|NOT_SUPPORTED"}"#,
        },
        failure_mode: FailureMode::PolicyBlocked,
        handler_test_vector: TestVector {
            description: "context assemble returns POLICY_BLOCKED in local-only mode with requires_platform",
            expected_outcome: "failure:policy_blocked",
            test_fn_name: "context_assemble_policy_blocked_when_local_only",
        },
    });

    // ── fs/* baseline (V1.33) ──
    reg.register(CapabilityRow {
        id: "fs/read_text_file",
        access: Access::Read,
        admission: vec![
            AdmissionGate::Allowlist,
            AdmissionGate::WorkspaceBounds,
            AdmissionGate::PermissionPolicy,
            AdmissionGate::AuditLog,
        ],
        handler: hte::registry_read_file,
        acp_wire: AcpWire {
            request_schema_ref: r#"{"path":"string"}"#,
            response_schema_ref: r#"{"content":"string"}"#,
            error_schema_ref: r#"{"code":"INVALID_INPUT|FORBIDDEN|NOT_SUPPORTED"}"#,
        },
        failure_mode: FailureMode::InvalidInput,
        handler_test_vector: TestVector {
            description: "read file returns content for valid path",
            expected_outcome: "success",
            test_fn_name: "execute_read_file_succeeds",
        },
    });

    reg.register(CapabilityRow {
        id: "fs/write_text_file",
        access: Access::Write,
        admission: vec![
            AdmissionGate::Allowlist,
            AdmissionGate::WorkspaceBounds,
            AdmissionGate::PermissionPolicy,
            AdmissionGate::AuditLog,
        ],
        handler: hte::registry_write_file,
        acp_wire: AcpWire {
            request_schema_ref: r#"{"path":"string","content":"string"}"#,
            response_schema_ref: r#"{"written":"bool"}"#,
            error_schema_ref: r#"{"code":"INVALID_INPUT|FORBIDDEN|NOT_SUPPORTED"}"#,
        },
        failure_mode: FailureMode::InvalidInput,
        handler_test_vector: TestVector {
            description: "write file writes content and returns success",
            expected_outcome: "success",
            test_fn_name: "execute_write_file_succeeds",
        },
    });

    reg
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::create_test_workspace;

    #[test]
    fn registry_has_eight_host_tools() {
        let reg = host_tool_registry();
        assert_eq!(reg.len(), 8);
    }

    #[test]
    fn registry_lookup_each_tool() {
        let reg = host_tool_registry();
        for id in [
            "nexus.context.whoami",
            "nexus.workspace.info",
            "nexus.work.get",
            "nexus.work.patch",
            "nexus.orchestration.schedule_status",
            "nexus.context.assemble",
            "fs/read_text_file",
            "fs/write_text_file",
        ] {
            assert!(
                reg.lookup(id).is_some(),
                "expected tool '{id}' to be registered"
            );
        }
    }

    #[test]
    fn registry_lookup_unknown_returns_none() {
        let reg = host_tool_registry();
        assert!(reg.lookup("nonexistent.tool").is_none());
    }

    #[test]
    fn registry_all_rows_have_seven_fields() {
        let reg = host_tool_registry();
        for id in reg.ids() {
            let row = reg.lookup(id).expect("row must exist");
            // Verify all 7 fields are populated
            assert!(!row.id.is_empty(), "id must not be empty for {id}");
            assert!(
                !row.admission.is_empty(),
                "admission must not be empty for {id}"
            );
            assert!(
                !row.handler_test_vector.description.is_empty(),
                "test vector description must not be empty for {id}"
            );
            assert!(
                !row.handler_test_vector.test_fn_name.is_empty(),
                "test fn name must not be empty for {id}"
            );
        }
    }

    #[test]
    fn registry_cross_validates_prefix() {
        // Every registry row id must use the "nexus." or "fs/" prefix.
        let reg = host_tool_registry();
        for id in reg.ids() {
            assert!(
                id.starts_with("nexus.") || id.starts_with("fs/"),
                "registry id '{id}' must use nexus.* or fs/* prefix"
            );
        }
    }

    #[tokio::test]
    async fn registry_dispatch_rejects_unknown_tool() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
        let reg = host_tool_registry();
        let req = ToolExecuteRequest {
            tool_name: "unknown/tool".to_string(),
            parameters: serde_json::json!({}),
            session_id: None,
            request_id: None,
            caller_kind: None,
        };
        let result = reg.dispatch(&req, &state, "").await;
        assert!(result.is_err());
        match result {
            Err(NexusApiError::BadRequest { code, .. }) => {
                assert_eq!(code, "NOT_SUPPORTED");
            }
            other => panic!("Expected BadRequest(NOT_SUPPORTED), got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn registry_dispatch_whoami_returns_creator() {
        let (_tmp, nexus_home, db_path) = create_test_workspace().await;
        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
        let reg = host_tool_registry();
        let req = ToolExecuteRequest {
            tool_name: "nexus.context.whoami".to_string(),
            parameters: serde_json::json!({}),
            session_id: None,
            request_id: None,
            caller_kind: None,
        };
        let result = reg.dispatch(&req, &state, "test_creator").await;
        assert!(result.is_ok());
        let val = result.expect("result");
        assert_eq!(val["creator_id"], "test_creator");
        assert_eq!(val["workspace_slug"], "default");
    }
}
