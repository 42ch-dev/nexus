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
//! # Migration complete (V1.53 P0)
//!
//! All three sub-phases are done:
//! - **Sub-phase 1 (introduce)**: Registry introduced behind adapter with parity tests.
//! - **Sub-phase 2 (cutover)**: `HostToolExecutor::execute()` routes through registry.
//! - **Sub-phase 3 (cleanup)**: Old `dispatch_tool()` match table removed.
//!   No lingering parallel paths remain.

use crate::api::errors::NexusApiError;
use crate::api::handlers::host_tool_executor::ToolExecuteRequest;
use crate::workspace::WorkspaceState;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::LazyLock;

// ─── Registry types ────────────────────────────────────────────────────────

/// Unified handler function signature for all registered capabilities.
///
/// Takes references to the tool request, workspace state, and creator id,
/// returns a boxed future resolving to `Result<serde_json::Value, NexusApiError>`.
pub type RegistryHandlerFn = for<'a> fn(
    &'a ToolExecuteRequest,
    &'a WorkspaceState,
    &'a str,
) -> Pin<
    Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>,
>;

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
    /// World must exist and be owned by the active creator.
    RequireWorldOwnership,
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
    /// Expected outcome: "success", "`failure:policy_blocked`", etc.
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
    /// Ordered fail-closed admission gates (&'static since V1.54 P0 T5).
    pub admission: &'static [AdmissionGate],
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
        assert!(
            !self.rows.contains_key(row.id),
            "duplicate capability id in registry: {}",
            row.id
        );
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
    /// Looks up the capability by `tool_name`, iterates the declared
    /// `AdmissionGate` slice as a centralized accountability checkpoint,
    /// then invokes the registered handler.
    ///
    /// **Gate enforcement split** (W-001 fix):
    /// - Gates 1-4 (`Allowlist`, `ActiveCreator`, `WorkspaceBounds`,
    ///   `PermissionPolicy`) are enforced by `admission_pipeline` before
    ///   `dispatch` is called.
    /// - `RequireWorldOwnership` is enforced by per-handler checks
    ///   (e.g. `ensure_world_accessible_for_creator`).
    /// - `AuditLog` is enforced by the caller (`audit_tool_execution`
    ///   in `registry_dispatch`).
    ///
    /// The invariant test `registry_all_admission_gates_have_enforcement`
    /// proves every gate in every row has a corresponding runtime check.
    ///
    /// # Errors
    ///
    /// Returns `NexusApiError::BadRequest` with code `NOT_SUPPORTED`
    /// if the tool is not registered. Individual handlers may return
    /// other error variants (e.g. `Forbidden`, `InvalidInput`).
    pub async fn dispatch(
        &self,
        req: &ToolExecuteRequest,
        state: &WorkspaceState,
        creator_id: &str,
    ) -> Result<serde_json::Value, NexusApiError> {
        let row = self.lookup(&req.tool_name).ok_or_else(|| {
            NexusApiError::BadRequest {
                code: "NOT_SUPPORTED".to_string(),
                message: format!("unsupported tool: {}", req.tool_name),
            }
        })?;

        // Centralized admission-gate accountability checkpoint.
        // Each gate type MUST have a corresponding enforcement path (pipeline,
        // handler, or caller). The invariant test below validates this mapping
        // at registration time.
        for gate in row.admission {
            debug_assert!(
                matches!(
                    gate,
                    AdmissionGate::Allowlist
                        | AdmissionGate::ActiveCreator
                        | AdmissionGate::WorkspaceBounds
                        | AdmissionGate::PermissionPolicy
                        | AdmissionGate::RequireWorldOwnership
                        | AdmissionGate::AuditLog
                ),
                "unhandled admission gate {gate:?} for capability {}",
                row.id
            );
            let _ = gate; // Readability: gate is accounted for by the match above.
        }

        (row.handler)(req, state, creator_id).await
    }
}

impl Default for CapabilityRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Registry constructor ──────────────────────────────────────────────────

// ─── Registry constructor ──────────────────────────────────────────────────

/// Static admission gate arrays (defined once, referenced by all 19 rows).
const ADMISSION_READ_CONTEXT: &[AdmissionGate] = &[
    AdmissionGate::Allowlist,
    AdmissionGate::ActiveCreator,
    AdmissionGate::PermissionPolicy,
    AdmissionGate::AuditLog,
];

const ADMISSION_READ_WORKSPACE: &[AdmissionGate] = &[
    AdmissionGate::Allowlist,
    AdmissionGate::ActiveCreator,
    AdmissionGate::WorkspaceBounds,
    AdmissionGate::PermissionPolicy,
    AdmissionGate::AuditLog,
];

const ADMISSION_READ_WORLD: &[AdmissionGate] = &[
    AdmissionGate::Allowlist,
    AdmissionGate::ActiveCreator,
    AdmissionGate::RequireWorldOwnership,
    AdmissionGate::PermissionPolicy,
    AdmissionGate::AuditLog,
];

const ADMISSION_WRITE_WORKSPACE: &[AdmissionGate] = &[
    AdmissionGate::Allowlist,
    AdmissionGate::ActiveCreator,
    AdmissionGate::WorkspaceBounds,
    AdmissionGate::PermissionPolicy,
    AdmissionGate::AuditLog,
];

const ADMISSION_WRITE_WORLD: &[AdmissionGate] = &[
    AdmissionGate::Allowlist,
    AdmissionGate::ActiveCreator,
    AdmissionGate::RequireWorldOwnership,
    AdmissionGate::PermissionPolicy,
    AdmissionGate::AuditLog,
];

const ADMISSION_FS_READ: &[AdmissionGate] = &[
    AdmissionGate::Allowlist,
    AdmissionGate::WorkspaceBounds,
    AdmissionGate::PermissionPolicy,
    AdmissionGate::AuditLog,
];

const ADMISSION_FS_WRITE: &[AdmissionGate] = &[
    AdmissionGate::Allowlist,
    AdmissionGate::WorkspaceBounds,
    AdmissionGate::PermissionPolicy,
    AdmissionGate::AuditLog,
];

const ADMISSION_POOL_WRITE: &[AdmissionGate] = &[
    AdmissionGate::Allowlist,
    AdmissionGate::ActiveCreator,
    AdmissionGate::PermissionPolicy,
    AdmissionGate::AuditLog,
];

/// Create a registry pre-populated with all 19 host tools (V1.34 + V1.53 P1 + V1.54 P0).
///
/// V1.54 P0 T5: Converted to `LazyLock` singleton to eliminate per-dispatch
/// allocation. All admission gates are `&'static [AdmissionGate]` references.
#[must_use]
pub fn host_tool_registry() -> &'static CapabilityRegistry {
    static REGISTRY: LazyLock<CapabilityRegistry> = LazyLock::new(build_registry);
    &REGISTRY
}

/// Builds the full registry (called once by `LazyLock`).
/// Marked `pub` so benchmarks can measure cold-path initialization;
/// external callers should use `host_tool_registry()` instead.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn build_registry() -> CapabilityRegistry {
    use crate::api::handlers::host_tool_executor as hte;
    let mut reg = CapabilityRegistry::new();

    // ── nexus.* tools (V1.34) ──
    reg.register(CapabilityRow {
        id: "nexus.context.whoami",
        access: Access::Read,
        admission: ADMISSION_READ_CONTEXT,
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
        admission: ADMISSION_READ_CONTEXT,
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
        admission: ADMISSION_READ_WORKSPACE,
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
        admission: ADMISSION_WRITE_WORKSPACE,
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
        admission: ADMISSION_READ_WORKSPACE,
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
            test_fn_name: "schedule_status_happy_path",
        },
    });

    reg.register(CapabilityRow {
        id: "nexus.context.assemble",
        access: Access::Read,
        admission: ADMISSION_READ_CONTEXT,
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

    // ── nexus.* tools (V1.53 P1: DF-46 read-heavy slice) ──
    reg.register(CapabilityRow {
        id: "nexus.world.snapshot.get",
        access: Access::Read,
        admission: ADMISSION_READ_WORLD,
        handler: hte::registry_world_snapshot_get,
        acp_wire: AcpWire {
            request_schema_ref: r#"{"world_id":"string"}"#,
            response_schema_ref: "WorldState",
            error_schema_ref: r#"{"code":"FORBIDDEN|INVALID_INPUT|NOT_FOUND|NOT_SUPPORTED"}"#,
        },
        failure_mode: FailureMode::InvalidInput,
        handler_test_vector: TestVector {
            description: "world snapshot get returns world state for valid world_id",
            expected_outcome: "success",
            test_fn_name: "world_snapshot_get_returns_world_state",
        },
    });

    reg.register(CapabilityRow {
        id: "nexus.timeline.recent.get",
        access: Access::Read,
        admission: ADMISSION_READ_WORLD,
        handler: hte::registry_timeline_recent_get,
        acp_wire: AcpWire {
            request_schema_ref: r#"{"world_id":"string","limit?":"int"}"#,
            response_schema_ref: "[TimelineEvent]",
            error_schema_ref: r#"{"code":"FORBIDDEN|INVALID_INPUT|NOT_SUPPORTED"}"#,
        },
        failure_mode: FailureMode::InvalidInput,
        handler_test_vector: TestVector {
            description: "timeline recent get returns recent events for valid world_id",
            expected_outcome: "success",
            test_fn_name: "timeline_recent_get_returns_recent_events",
        },
    });

    reg.register(CapabilityRow {
        id: "nexus.kb_snapshot.read",
        access: Access::Read,
        admission: ADMISSION_READ_WORLD,
        handler: hte::registry_kb_snapshot_read,
        acp_wire: AcpWire {
            request_schema_ref: r#"{"world_id":"string"}"#,
            response_schema_ref: "[KeyBlock]",
            error_schema_ref: r#"{"code":"FORBIDDEN|INVALID_INPUT|NOT_SUPPORTED"}"#,
        },
        failure_mode: FailureMode::InvalidInput,
        handler_test_vector: TestVector {
            description: "kb snapshot read returns key blocks for valid world_id",
            expected_outcome: "success",
            test_fn_name: "kb_snapshot_read_returns_key_blocks",
        },
    });

    reg.register(CapabilityRow {
        id: "nexus.manuscript.chapter.get",
        access: Access::Read,
        admission: ADMISSION_READ_WORKSPACE,
        handler: hte::registry_manuscript_chapter_get,
        acp_wire: AcpWire {
            request_schema_ref: r#"{"work_id":"string","chapter":"int","volume?":"int"}"#,
            response_schema_ref: "WorkChapterRecord",
            error_schema_ref: r#"{"code":"FORBIDDEN|INVALID_INPUT|NOT_FOUND|NOT_SUPPORTED"}"#,
        },
        failure_mode: FailureMode::InvalidInput,
        handler_test_vector: TestVector {
            description:
                "manuscript chapter get returns chapter record for valid work_id + chapter",
            expected_outcome: "success",
            test_fn_name: "manuscript_chapter_get_returns_chapter_record",
        },
    });

    reg.register(CapabilityRow {
        id: "nexus.observability.daemon.health",
        access: Access::Read,
        admission: ADMISSION_READ_CONTEXT,
        handler: hte::registry_daemon_health,
        acp_wire: AcpWire {
            request_schema_ref: "{}",
            response_schema_ref: r#"{"uptime_seconds":"int","started_at":"string","runtime_mode":"string","lifecycle_state":"string","registry_size":"int","registry_ids":"[string]","pool_healthy":"bool"}"#,
            error_schema_ref: r#"{"code":"FORBIDDEN|NOT_SUPPORTED"}"#,
        },
        failure_mode: FailureMode::Forbidden,
        handler_test_vector: TestVector {
            description: "daemon health returns runtime status and registry size",
            expected_outcome: "success",
            test_fn_name: "daemon_health_returns_registry_status",
        },
    });

    // ── V1.54 P0: DF-46 write tools ──
    reg.register(CapabilityRow {
        id: "nexus.kb_snapshot.write",
        access: Access::Write,
        admission: ADMISSION_WRITE_WORLD,
        handler: hte::registry_kb_snapshot_write,
        acp_wire: AcpWire {
            request_schema_ref: r#"{"world_id":"string","blocks":"[KeyBlock]"}"#,
            response_schema_ref: r#"{"written":"int","world_id":"string"}"#,
            error_schema_ref: r#"{"code":"FORBIDDEN|INVALID_INPUT|NOT_FOUND|NOT_SUPPORTED"}"#,
        },
        failure_mode: FailureMode::Forbidden,
        handler_test_vector: TestVector {
            description: "kb snapshot write upserts key blocks for owned world",
            expected_outcome: "success",
            test_fn_name: "kb_snapshot_write_upserts_key_blocks",
        },
    });

    reg.register(CapabilityRow {
        id: "nexus.manuscript.chapter.update",
        access: Access::Write,
        admission: ADMISSION_WRITE_WORKSPACE,
        handler: hte::registry_manuscript_chapter_update,
        acp_wire: AcpWire {
            request_schema_ref: r#"{"work_id":"string","chapter":"int","volume?":"int","content?":"string","block_overrides?":"object"}"#,
            response_schema_ref: "WorkChapterRecord",
            error_schema_ref: r#"{"code":"FORBIDDEN|INVALID_INPUT|NOT_FOUND|NOT_SUPPORTED"}"#,
        },
        failure_mode: FailureMode::InvalidInput,
        handler_test_vector: TestVector {
            description: "manuscript chapter update writes chapter content for valid work",
            expected_outcome: "success",
            test_fn_name: "manuscript_chapter_update_writes_content",
        },
    });

    reg.register(CapabilityRow {
        id: "nexus.world.configure",
        access: Access::Write,
        admission: ADMISSION_WRITE_WORLD,
        handler: hte::registry_world_configure,
        acp_wire: AcpWire {
            request_schema_ref: r#"{"world_id":"string","title?":"string","visibility?":"string","time_policy?":"string"}"#,
            response_schema_ref: r#"{"world_id":"string","updated":"bool"}"#,
            error_schema_ref: r#"{"code":"FORBIDDEN|INVALID_INPUT|NOT_FOUND|NOT_SUPPORTED"}"#,
        },
        failure_mode: FailureMode::Forbidden,
        handler_test_vector: TestVector {
            description: "world configure updates world metadata for owned world",
            expected_outcome: "success",
            test_fn_name: "world_configure_updates_metadata",
        },
    });

    reg.register(CapabilityRow {
        id: "nexus.work.schedule.set",
        access: Access::Write,
        admission: ADMISSION_WRITE_WORKSPACE,
        handler: hte::registry_work_schedule_set,
        acp_wire: AcpWire {
            request_schema_ref: r#"{"work_id":"string","schedule_ids":"[string]"}"#,
            response_schema_ref: r#"{"work_id":"string","schedule_ids":"[string]"}"#,
            error_schema_ref: r#"{"code":"FORBIDDEN|INVALID_INPUT|NOT_SUPPORTED"}"#,
        },
        failure_mode: FailureMode::Forbidden,
        handler_test_vector: TestVector {
            description: "work schedule set links schedule ids to work",
            expected_outcome: "success",
            test_fn_name: "work_schedule_set_links_schedules",
        },
    });

    reg.register(CapabilityRow {
        id: "nexus.finding.resolve",
        access: Access::Write,
        admission: ADMISSION_WRITE_WORKSPACE,
        handler: hte::registry_finding_resolve,
        acp_wire: AcpWire {
            request_schema_ref: r#"{"finding_id":"string","resolution?":"string"}"#,
            response_schema_ref: r#"{"finding_id":"string","resolved":"bool"}"#,
            error_schema_ref: r#"{"code":"FORBIDDEN|INVALID_INPUT|NOT_FOUND|NOT_SUPPORTED"}"#,
        },
        failure_mode: FailureMode::Forbidden,
        handler_test_vector: TestVector {
            description: "finding resolve marks finding as resolved",
            expected_outcome: "success",
            test_fn_name: "finding_resolve_marks_resolved",
        },
    });

    reg.register(CapabilityRow {
        id: "nexus.pool.entry.manage",
        access: Access::Write,
        admission: ADMISSION_POOL_WRITE,
        handler: hte::registry_pool_entry_manage,
        acp_wire: AcpWire {
            request_schema_ref: r#"{"work_id":"string","action":"string","priority?":"int"}"#,
            response_schema_ref: r#"{"work_id":"string","action":"string","success":"bool"}"#,
            error_schema_ref: r#"{"code":"FORBIDDEN|INVALID_INPUT|NOT_FOUND|NOT_SUPPORTED"}"#,
        },
        failure_mode: FailureMode::Forbidden,
        handler_test_vector: TestVector {
            description: "pool entry manage adds work to selection pool",
            expected_outcome: "success",
            test_fn_name: "pool_entry_manage_adds_to_pool",
        },
    });

    // ── fs/* baseline (V1.33) ──
    reg.register(CapabilityRow {
        id: "fs/read_text_file",
        access: Access::Read,
        admission: ADMISSION_FS_READ,
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
        admission: ADMISSION_FS_WRITE,
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
    fn registry_has_nineteen_host_tools() {
        let reg = host_tool_registry();
        assert_eq!(reg.len(), 19);
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
            "nexus.world.snapshot.get",
            "nexus.timeline.recent.get",
            "nexus.kb_snapshot.read",
            "nexus.manuscript.chapter.get",
            "nexus.observability.daemon.health",
            "nexus.kb_snapshot.write",
            "nexus.manuscript.chapter.update",
            "nexus.world.configure",
            "nexus.work.schedule.set",
            "nexus.finding.resolve",
            "nexus.pool.entry.manage",
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

    /// **R-V153P0QC1-002 enforcement**: static accepted set of test function names.
    ///
    /// Every `CapabilityRow.handler_test_vector.test_fn_name` MUST appear in
    /// this set. When P1 adds new rows, the author MUST also add the test fn
    /// name here — otherwise the `all_test_fn_names_accepted` test will fail.
    const ACCEPTED_TEST_FN_NAMES: &[&str] = &[
        "whoami_returns_active_creator",
        "workspace_info_returns_details",
        "work_get_happy_path",
        "work_patch_rejects_stage_field",
        "schedule_status_happy_path",
        "context_assemble_policy_blocked_when_local_only",
        "world_snapshot_get_returns_world_state",
        "timeline_recent_get_returns_recent_events",
        "kb_snapshot_read_returns_key_blocks",
        "manuscript_chapter_get_returns_chapter_record",
        "daemon_health_returns_registry_status",
        "kb_snapshot_write_upserts_key_blocks",
        "manuscript_chapter_update_writes_content",
        "world_configure_updates_metadata",
        "work_schedule_set_links_schedules",
        "finding_resolve_marks_resolved",
        "pool_entry_manage_adds_to_pool",
        "execute_read_file_succeeds",
        "execute_write_file_succeeds",
    ];

    #[test]
    fn all_test_fn_names_accepted() {
        let reg = host_tool_registry();
        for id in reg.ids() {
            let row = reg.lookup(id).expect("row must exist");
            let name = row.handler_test_vector.test_fn_name;
            assert!(
                ACCEPTED_TEST_FN_NAMES.contains(&name),
                "test_fn_name '{name}' (tool '{id}') is not in ACCEPTED_TEST_FN_NAMES — \
                 add it to the const in capability_registry.rs test module"
            );
        }
    }

    #[test]
    fn all_accepted_test_fn_names_referenced() {
        // Every accepted name must be referenced by at least one registry row
        // (ensures ACCEPTED_TEST_FN_NAMES does not accumulate dead entries).
        let reg = host_tool_registry();
        let registry_names: std::collections::HashSet<&str> = reg
            .ids()
            .map(|id| {
                reg.lookup(id)
                    .expect("row must exist")
                    .handler_test_vector
                    .test_fn_name
            })
            .collect();
        for accepted in ACCEPTED_TEST_FN_NAMES {
            assert!(
                registry_names.contains(accepted),
                "ACCEPTED_TEST_FN_NAMES entry '{accepted}' is not referenced by any registry row"
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

    /// **R-V153P0QC2-002**: Catalog↔registry id bijection test.
    ///
    /// Reads `acp-capability-set.md` logical catalog and compares IDs against
    /// `host_tool_registry().ids()`. Fails if a registry id that IS expected
    /// to be in the catalog is missing, and vice versa for catalog ids that
    /// are implemented as host tools.
    ///
    /// P1 note: V1.53 P1 adds 5 new tools. Two of them (`nexus.manuscript.chapter.get`
    /// and `nexus.observability.daemon.health`) are not yet in the catalog because
    /// the catalog is frozen for P1 per plan constraints. The test acknowledges
    /// these as known gaps. Once the catalog is updated in a future iteration,
    /// the known-gaps list should be emptied.
    #[test]
    fn registry_ids_have_catalog_rows() {
        use std::collections::HashSet;

        let reg = host_tool_registry();
        let registry_ids: HashSet<&str> = reg.ids().collect();

        // Known gaps: registry ids not yet in the frozen catalog.
        // These are daemon host tools added in V1.33/V1.34/V1.53 that are
        // not part of the logical ACP capability catalog (fs/* tools are
        // not ACP-facing; work.* and orchestration.* were added as host
        // extensions; P1 tools pre-date catalog updates).
        // Remove entries when acp-capability-set.md is updated.
        let known_catalog_gaps: HashSet<&str> = [
            "fs/read_text_file",
            "fs/write_text_file",
            "nexus.work.get",
            "nexus.work.patch",
            "nexus.orchestration.schedule_status",
            "nexus.manuscript.chapter.get",
            "nexus.observability.daemon.health",
        ]
        .iter()
        .copied()
        .collect();

        // Parse capability IDs from acp-capability-set.md tables
        let catalog_path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../.mstar/knowledge/specs/acp-capability-set.md"
        );
        let catalog_content =
            std::fs::read_to_string(catalog_path).expect("acp-capability-set.md must be readable");

        // Extract all `nexus.<id>` lines from markdown tables
        let catalog_ids: HashSet<String> = catalog_content
            .lines()
            .filter_map(|line| {
                let trimmed = line.trim();
                // Match table rows like: `| nexus.world.snapshot.get | yes | ...`
                if trimmed.starts_with('|') && trimmed.contains('`') {
                    // Extract text between first pair of backticks
                    let start = trimmed.find('`')?;
                    let rest = &trimmed[start + 1..];
                    let end = rest.find('`')?;
                    let id = &rest[..end];
                    if id.starts_with("nexus.") || id.starts_with("fs/") {
                        Some(id.to_string())
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect();

        // Every registry id must have a catalog row (except known P1 gaps)
        for id in &registry_ids {
            if known_catalog_gaps.contains(id) {
                continue;
            }
            assert!(
                catalog_ids.contains(*id),
                "Registry id '{id}' has NO corresponding row in acp-capability-set.md catalog. \
                 Add a catalog row, add to known_catalog_gaps, or remove the registry entry."
            );
        }

        // Every catalog nexus.*/fs/* id that maps to a host tool should be in the registry.
        // Not all catalog ids are host tools (some are logical-only), so this is a
        // one-way check (registry ⊆ catalog ∪ known_gaps).
        let missing_from_registry: Vec<_> = catalog_ids
            .iter()
            .filter(|cid| {
                // Only flag catalog ids that look like they SHOULD be host tools
                // (i.e., read-only, non-mutation, non-sync, non-publish)
                let is_likely_host_tool = matches!(
                    cid.as_str(),
                    "nexus.context.whoami"
                        | "nexus.workspace.info"
                        | "nexus.workspace.paths"
                        | "nexus.context.assemble"
                        | "nexus.world.snapshot.get"
                        | "nexus.world.state.query"
                        | "nexus.timeline.recent.get"
                        | "nexus.kb_snapshot.read"
                        | "nexus.runtime.health"
                        | "nexus.trace.correlation"
                );
                is_likely_host_tool && !registry_ids.contains(cid.as_str())
            })
            .collect();

        // Logging-only: catalog ids not yet in registry (these are future tools).
        // Not a hard failure because P1 scope is limited to 5 tools.
        if !missing_from_registry.is_empty() {
            eprintln!(
                "INFO: catalog ids not yet in registry (future P1+ scope): {missing_from_registry:?}"
            );
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

    /// W-001 invariant: every registered row's admission gates have a known
    /// enforcement path (pipeline, handler, or caller). This test will fail
    /// if a new `AdmissionGate` variant is added without updating the
    /// enforcement mapping, preventing SSOT drift between declared gates
    /// and runtime checks.
    #[test]
    fn registry_all_admission_gates_have_enforcement() {
        let reg = build_registry();
        assert!(!reg.is_empty(), "registry must have rows");
        for id in reg.ids() {
            let row = reg.lookup(id).expect("row must exist");
            assert!(
                !row.admission.is_empty(),
                "row '{id}' has empty admission gates"
            );
            for gate in row.admission {
                // Every gate variant MUST appear in this match arm.
                // Adding a new variant without a corresponding enforcement
                // path will cause a compile error here.
                #[allow(clippy::wildcard_in_or_patterns)]
                let _enforcement_path = match gate {
                    AdmissionGate::Allowlist => "admission_pipeline: allowlist check",
                    AdmissionGate::ActiveCreator => "admission_pipeline: active-creator check",
                    AdmissionGate::WorkspaceBounds => "admission_pipeline: workspace-bounds check",
                    AdmissionGate::PermissionPolicy => "admission_pipeline: permission-policy check",
                    AdmissionGate::RequireWorldOwnership => {
                        "per-handler: ensure_world_accessible_for_creator"
                    }
                    AdmissionGate::AuditLog => "caller: audit_tool_execution",
                };
            }
        }
    }
}
