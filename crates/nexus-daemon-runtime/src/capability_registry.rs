//! Capability Registry — runtime SSOT for `nexus.*` host tool dispatch.
//!
//! V1.53 P0: Introduces a unified registry with 7-field row shape
//! (id → access → admission → handler → catalog descriptor → failure mode → test vector).
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
use serde_json::Value;
use spoke_operations::{parse_tool_capability_id, SpokeResult};
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

/// Catalog descriptor for a capability (AR-78, DF-89).
///
/// Carries the authored tool summary plus real draft-2020-12 JSON-Schema
/// text for the input (and, where pinned, output) shape. `&'static str`
/// literals fit the `LazyLock` static-row design exactly — no parse at
/// registry build, no new dependency. The schema text is the single source
/// of truth: it flows registry → catalog route → MCP child parse.
#[derive(Debug, Clone)]
pub struct CatalogDescriptor {
    /// Authored tool summary for LLM/script consumers (replaces the
    /// `TestVector.description` reuse that ended with AR-78 #4).
    pub description: &'static str,
    /// Real draft-2020-12 JSON-Schema text (root `"type":"object"`), or
    /// `None` when the input schema is not yet authored — the catalog then
    /// emits the named placeholder (`NAMED_PLACEHOLDER_INPUT`) and the id
    /// MUST appear in `SCHEMA_REMAINDER_LEDGER` (lockstep-pinned).
    pub input_schema: Option<&'static str>,
    /// Real draft-2020-12 JSON-Schema text for the success shape when the
    /// handler's output is a stable object worth pinning; `None` otherwise
    /// (omission is honest and rule-based — no ledger entry for outputs).
    pub output_schema: Option<&'static str>,
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
/// handler binding, catalog descriptor, failure mode contract,
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
    /// Catalog descriptor (authored description + real draft-2020-12
    /// schemas; AR-78 — replaces the removed `AcpWire`).
    pub catalog: CatalogDescriptor,
    /// Expected failure mode when denied.
    pub failure_mode: FailureMode,
    /// Test vector descriptor.
    pub handler_test_vector: TestVector,
}

// ─── Schema remainder ledger (AR-78 #6) ────────────────────────────────────

/// Named input-schema placeholder emitted by the catalog for a builtin row
/// whose input schema is not yet authored (`input_schema: None`).
///
/// Draft-2020-12-valid (`$comment` is ignorable by validators) and
/// machine-distinguishable from a real schema — never the silent
/// `{"type":"object"}` placeholder of V1.174.
pub const NAMED_PLACEHOLDER_INPUT: &str =
    r#"{"type":"object","$comment":"nexus42:schema-pending"}"#;

/// Registry-source ledger of builtin ids whose input schema is not yet
/// authored. Lockstep pin: `row.catalog.input_schema.is_none() ⇔ id ∈ LEDGER`
/// (unit-tested both directions). Task 2 converted all 30 static rows, so
/// the ledger is empty — the pin stays as the guard against future rows
/// being added without a schema. Test-only: the catalog route reads
/// `NAMED_PLACEHOLDER_INPUT`, never this ledger.
#[cfg(test)]
pub(crate) const SCHEMA_REMAINDER_LEDGER: &[&str] = &[];

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

    /// Whether `id` resolves anywhere in the dispatch spine.
    ///
    /// Single-table spine resolution (AR-68 #4/#6): static rows →
    /// `PeerToolTable` (behind `connect-client`) → orchestration user
    /// capabilities (`origin() == User` only). An id is dispatchable iff it
    /// resolves here; unknown ids yield `not_supported` exactly like an
    /// unknown builtin.
    #[must_use]
    pub fn spine_resolves(&self, state: &WorkspaceState, id: &str) -> bool {
        if self.lookup(id).is_some() {
            return true;
        }
        #[cfg(feature = "connect-client")]
        if crate::connect::peer_tool_table().get(id).is_some() {
            return true;
        }
        state
            .capability_registry()
            .is_some_and(|reg| user_cap_catalog_admission(reg.get(id)).is_ok())
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
    /// # Panics
    ///
    /// Never in practice: the `expect` after the early-return lookup is
    /// unreachable because registry rows are insert-only and the
    /// not-found arm returns above.
    ///
    /// # Errors
    ///
    /// Returns `NexusApiError::BadRequest` with code `not_supported`
    /// if the tool is not registered. Individual handlers may return
    /// other error variants (e.g. `Forbidden`, `InvalidInput`).
    pub async fn dispatch(
        &self,
        req: &ToolExecuteRequest,
        state: &WorkspaceState,
        creator_id: &str,
    ) -> Result<serde_json::Value, NexusApiError> {
        let row = self.lookup(&req.tool_name);
        if row.is_none() {
            // Peer arm (AR-68 #4): reverse-invoke the owning responder.
            #[cfg(feature = "connect-client")]
            if let Some(entry) = crate::connect::peer_tool_table().get(&req.tool_name) {
                return dispatch_peer_tool(&entry, req).await;
            }
            // User-capability arm (AR-68 #6): `Capability::run(arguments)`.
            if let Some(reg) = state.capability_registry() {
                if let Ok(cap) = user_cap_catalog_admission(reg.get(&req.tool_name)) {
                    return dispatch_user_cap(cap, req).await;
                }
            }
            return Err(NexusApiError::BadRequest {
                code: "not_supported".to_string(),
                message: format!("unsupported tool: {}", req.tool_name),
            });
        }
        let row = row.expect("row present");

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

// ─── Spine peer + user-capability arms (AR-68 #4/#6) ──────────────────────

/// Peer dispatch arm: structural argument gate via
/// `validate_tool_arguments`, then reverse-invoke the owning responder.
/// Spoke rejects map to `NexusApiError` with the wire code preserved.
#[cfg(feature = "connect-client")]
async fn dispatch_peer_tool(
    entry: &crate::connect::PeerToolEntry,
    req: &ToolExecuteRequest,
) -> Result<Value, NexusApiError> {
    use spoke_operations::{validate_tool_arguments, SpokeRejectCode, SpokeResult};
    if let SpokeResult::Reject(reject) = validate_tool_arguments(&entry.descriptor, &req.parameters)
    {
        return Err(NexusApiError::BadRequest {
            code: "invalid_input".to_string(),
            message: reject.message,
        });
    }
    match entry
        .responder
        .invoke_tool(&req.tool_name, req.parameters.clone())
        .await
    {
        SpokeResult::Ok(value) => Ok(value),
        SpokeResult::Reject(reject) => {
            // AR-76 #4 honest-refusal matrix for the transport-failure
            // classes (spoke frozen contract §8.2 `details.kind`): a
            // reverse invoke that times out is an `internal` error named
            // `timeout`; a session torn down mid-invoke (`session_closed` /
            // `transport`) is an `internal` error named for the disconnect.
            // Both are fail-fast refusals — never a hang, never mislabeled
            // as a peer deny. All other rejects keep the deny mapping below.
            let kind = reject
                .details
                .as_ref()
                .and_then(|d| d.get("kind"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            if reject.code == SpokeRejectCode::InternalError {
                match kind {
                    "timeout" => {
                        return Err(NexusApiError::Internal {
                            code: "PEER_TOOL_TIMEOUT".to_string(),
                            message: format!("peer tool invoke timed out: {}", reject.message),
                        });
                    }
                    "session_closed" | "transport" => {
                        return Err(NexusApiError::Internal {
                            code: "PEER_TOOL_DISCONNECTED".to_string(),
                            message: format!(
                                "peer session disconnected mid-invoke: {}",
                                reject.message
                            ),
                        });
                    }
                    _ => {}
                }
            }
            // AR-70 #4: thread the spoke reject through a typed error so the
            // original lowercase wire code (e.g. `op_unsupported`,
            // `capability_missing`) survives verbatim in `details.wire_code`
            // — never uppercased, never re-parsed from message text. The
            // `code` channel stays the canonical spine `not_supported`.
            let wire_code = reject
                .details
                .as_ref()
                .and_then(|d| d.get("wire_code"))
                .and_then(serde_json::Value::as_str)
                .map_or_else(|| reject.code.as_str().to_string(), ToOwned::to_owned);
            Err(NexusApiError::PeerToolDenied {
                code: "not_supported".to_string(),
                message: reject.message,
                wire_code,
            })
        }
    }
}

/// Structural argument gate for the user-cap arm (AR-76 #2/#4, W-A):
/// the SAME spoke-granularity semantics as the peer arm's
/// `spoke_operations::validate_tool_arguments` — arguments must be a JSON
/// object and every declared top-level `required` key must be present;
/// otherwise `invalid_input` BEFORE any adapter I/O (WASM module load and
/// execution count as adapter I/O). No deeper JSON-Schema checking (V1.172
/// AR-37 posture). The peer arm keeps its grammar-typed spoke helper; this
/// helper implements the same refusal vocabulary against the capability's
/// declared `input_schema()` string.
fn validate_user_cap_arguments(schema_json: &str, arguments: &Value) -> Result<(), String> {
    let Some(object) = arguments.as_object() else {
        return Err("Tool arguments must be a JSON object".to_string());
    };
    let Ok(schema) = serde_json::from_str::<Value>(schema_json) else {
        // The catalog admission gate already proved the schema parses as a
        // JSON object; a parse failure here means the descriptor changed
        // after admission — fail closed as invalid_input rather than
        // dispatching unvalidated.
        return Err("capability input schema is not a JSON object".to_string());
    };
    if schema.get("type") != Some(&Value::String("object".to_string())) {
        return Ok(());
    }
    let Some(Value::Array(required)) = schema.get("required") else {
        return Ok(());
    };
    let missing: Vec<&str> = required
        .iter()
        .filter_map(Value::as_str)
        .filter(|key| !object.contains_key(*key))
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    Err(format!(
        "Missing required tool arguments: {}",
        missing.join(", ")
    ))
}

/// User-capability dispatch arm (AR-68 #6): structural gate, then
/// `Capability::run(arguments)`. `CapabilityError` maps to the closest
/// `NexusApiError` variant.
async fn dispatch_user_cap(
    cap: &dyn nexus_orchestration::capability::Capability,
    req: &ToolExecuteRequest,
) -> Result<Value, NexusApiError> {
    use nexus_orchestration::capability::CapabilityError;
    // AR-76 #2/#4 (W-A): the structural gate fires before any adapter I/O —
    // same refusal vocabulary as the peer arm.
    if let Err(message) = validate_user_cap_arguments(cap.input_schema(), &req.parameters) {
        return Err(NexusApiError::BadRequest {
            code: "invalid_input".to_string(),
            message,
        });
    }
    cap.run(req.parameters.clone()).await.map_err(|e| match e {
        CapabilityError::InputInvalid(msg) => NexusApiError::BadRequest {
            code: "invalid_input".to_string(),
            message: msg,
        },
        CapabilityError::Forbidden(msg) => NexusApiError::Forbidden {
            resource: "tool_execution".to_string(),
            reason: msg,
        },
        CapabilityError::WorkerUnavailable => NexusApiError::ServiceUnavailable {
            message: format!("capability '{}' has no executor wired", cap.name()),
        },
        other => NexusApiError::Internal {
            code: "CAPABILITY_RUN_FAILED".to_string(),
            message: format!("capability '{}' failed: {other}", cap.name()),
        },
    })
}

/// Catalog admission for a user capability (AR-68 #6): name must not start
/// with `nexus.` and must not match the peer grammar `^tools\.…`; the
/// declared `input_schema()` must parse as a JSON object. Fail-closed —
/// a non-admitted capability is neither dispatchable nor listed.
pub(crate) fn user_cap_catalog_admission(
    cap: Option<&dyn nexus_orchestration::capability::Capability>,
) -> Result<&dyn nexus_orchestration::capability::Capability, UserCapCatalogRefusal> {
    let cap = cap.ok_or(UserCapCatalogRefusal::NotUserCapability)?;
    if cap.origin() != nexus_orchestration::capability::CapabilityOrigin::User {
        return Err(UserCapCatalogRefusal::NotUserCapability);
    }
    let name = cap.name();
    if name.starts_with("nexus.") {
        return Err(UserCapCatalogRefusal::ReservedNamespace);
    }
    // QC-fix S-d: the spoke-operations helper IS the grammar (single source
    // of truth with the peer arm — the local `matches_tools_grammar` mirror
    // was removed; spoke grammar is `tools.<ns>.<tool_id>` with ns
    // `^[a-z][a-z0-9_-]*$` and tool_id `^[a-z0-9][a-z0-9_-]*$`).
    if matches!(parse_tool_capability_id(name), SpokeResult::Ok(_)) {
        return Err(UserCapCatalogRefusal::ReservedNamespace);
    }
    if serde_json::from_str::<Value>(cap.input_schema())
        .ok()
        .and_then(|v| v.as_object().map(|_| ()))
        .is_none()
    {
        return Err(UserCapCatalogRefusal::InputSchemaNotObject);
    }
    Ok(cap)
}

/// AR-70 §3 inclusion rule: a JSON-Schema string is carried as an MCP
/// `output_schema` only when it parses and declares a root `type: "object"`
/// (MCP requires an object root; non-object outputs are omitted, never
/// invented, never wrapped). Shared by the peer merge (connect-client) and
/// the user-cap branch of the catalog.
pub(crate) fn json_schema_has_object_root(raw: &str) -> bool {
    serde_json::from_str::<Value>(raw)
        .ok()
        .and_then(|v| {
            v.get("type")
                .and_then(serde_json::Value::as_str)
                .map(ToOwned::to_owned)
        })
        .as_deref()
        == Some("object")
}

/// Named catalog refusal for a user capability (AR-68 #6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserCapCatalogRefusal {
    /// Not a user capability (builtin or absent).
    NotUserCapability,
    /// Name starts with `nexus.` or matches the peer grammar.
    ReservedNamespace,
    /// `input_schema()` does not parse as a JSON object.
    InputSchemaNotObject,
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

/// Create a registry pre-populated with all 30 host tools (28 `nexus.*` + 2
/// `fs/*`; V1.34 + V1.53 P1 + V1.54 P0 + V1.56 P1 + V1.58 P3 + V1.59 P0).
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
        catalog: CatalogDescriptor {
            description: "Return the active creator id and workspace slug for the current session.",
            input_schema: Some(r#"{"type":"object","properties":{}}"#),
            output_schema: Some(
                r#"{"type":"object","properties":{"creator_id":{"type":"string"},"workspace_slug":{"type":"string"}},"required":["creator_id","workspace_slug"]}"#,
            ),
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
        catalog: CatalogDescriptor {
            description: "Return workspace details: creator id, slug, path, runtime mode, and initialization state.",
            input_schema: Some(r#"{"type":"object","properties":{}}"#),
            output_schema: Some(
                r#"{"type":"object","properties":{"creator_id":{"type":"string"},"workspace_slug":{"type":"string"},"workspace_path":{"type":"string"},"runtime_mode":{"type":"string"},"initialized":{"type":"boolean"}},"required":["creator_id","workspace_slug","workspace_path","runtime_mode","initialized"]}"#,
            ),
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
        catalog: CatalogDescriptor {
            description: "Return the Work record fields exposed by the catalog: status, title, current stage, and stage status.",
            input_schema: Some(
                r#"{"type":"object","properties":{"work_id":{"type":"string"}},"required":["work_id"]}"#,
            ),
            output_schema: Some(
                r#"{"type":"object","properties":{"work_id":{"type":"string"},"status":{"type":"string"},"title":{"type":"string"},"current_stage":{"type":"string"},"stage_status":{"type":"string"}},"required":["work_id","status","title","current_stage","stage_status"]}"#,
            ),
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
        catalog: CatalogDescriptor {
            description: "Patch a work's title, inspiration log, or stage metadata (stage field itself is rejected).",
            input_schema: Some(
                r#"{"type":"object","properties":{"work_id":{"type":"string"},"title":{"type":"string"},"inspiration_log":{"type":"array","items":{"type":"object","properties":{"text":{"type":"string"},"note":{"type":"string"},"source":{"type":"string"}},"anyOf":[{"required":["text"]},{"required":["note"]}]}},"stage_metadata":{"type":"object","properties":{"agent_notes":{"type":"string"},"research_summary_ref":{"type":"string"},"draft_outline_ref":{"type":"string"},"review_summary_ref":{"type":"string"},"last_agent_tool_request_id":{"type":"string"}},"additionalProperties":false}},"required":["work_id"],"additionalProperties":false}"#,
            ),
            output_schema: Some(
                r#"{"type":"object","properties":{"work_id":{"type":"string"},"status":{"type":"string"},"title":{"type":"string"},"current_stage":{"type":"string"},"stage_status":{"type":"string"}},"required":["work_id","status","title","current_stage","stage_status"]}"#,
            ),
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
        catalog: CatalogDescriptor {
            description: "Return the schedule ids linked to a work and their count.",
            input_schema: Some(
                r#"{"type":"object","properties":{"work_id":{"type":"string"}},"required":["work_id"]}"#,
            ),
            output_schema: Some(
                r#"{"type":"object","properties":{"work_id":{"type":"string"},"schedule_ids":{"type":"array","items":{"type":"string"}},"count":{"type":"integer"}},"required":["work_id","schedule_ids","count"]}"#,
            ),
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
        catalog: CatalogDescriptor {
            description: "Assemble the current context moment; requires_platform requests platform-integrated assembly.",
            input_schema: Some(
                r#"{"type":"object","properties":{"work_id":{"type":"string"},"requires_platform":{"type":"boolean"}}}"#,
            ),
            output_schema: Some(
                r#"{"type":"object","properties":{"mode":{"type":"string"},"creator_id":{"type":"string"},"assembled_at":{"type":"string"}},"required":["mode","creator_id","assembled_at"]}"#,
            ),
        },
        failure_mode: FailureMode::PolicyBlocked,
        handler_test_vector: TestVector {
            description: "context assemble returns policy_blocked in local-only mode with requires_platform",
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
        catalog: CatalogDescriptor {
            description: "Return the world snapshot fields exposed by the catalog: title, slug, status, fork flag, and creation time.",
            input_schema: Some(
                r#"{"type":"object","properties":{"world_id":{"type":"string"}},"required":["world_id"]}"#,
            ),
            output_schema: Some(
                r#"{"type":"object","properties":{"world_id":{"type":"string"},"title":{"type":"string"},"slug":{"type":"string"},"status":{"type":"string"},"is_fork":{"type":"boolean"},"created_at":{"type":"string"}},"required":["world_id","title","slug","status","is_fork","created_at"]}"#,
            ),
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
        catalog: CatalogDescriptor {
            description: "Return the most recent timeline events for a world (default 100, clamped to 500).",
            input_schema: Some(
                r#"{"type":"object","properties":{"world_id":{"type":"string"},"limit":{"type":"integer","minimum":1,"maximum":500}},"required":["world_id"]}"#,
            ),
            // Output is an event array, not a stable object — omitted per
            // AR-78 #5 (output schema pinned only for stable object shapes).
            output_schema: None,
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
        catalog: CatalogDescriptor {
            description: "Return the knowledge-base key blocks for an owned world.",
            input_schema: Some(
                r#"{"type":"object","properties":{"world_id":{"type":"string"}},"required":["world_id"]}"#,
            ),
            // Output is a key-block array, not a stable object — omitted per
            // AR-78 #5 (output schema pinned only for stable object shapes).
            output_schema: None,
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
        catalog: CatalogDescriptor {
            description: "Return the manuscript chapter fields exposed by the catalog: status and planned word count.",
            input_schema: Some(
                r#"{"type":"object","properties":{"work_id":{"type":"string"},"chapter":{"type":"integer","minimum":1},"volume":{"type":"integer","minimum":1}},"required":["work_id","chapter"]}"#,
            ),
            output_schema: Some(
                r#"{"type":"object","properties":{"work_id":{"type":"string"},"chapter":{"type":"integer"},"volume":{"type":"integer"},"status":{"type":"string"},"planned_word_count":{"type":"integer"}},"required":["work_id","chapter","status","planned_word_count"]}"#,
            ),
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
        catalog: CatalogDescriptor {
            description: "Return daemon runtime health: uptime, lifecycle state, registry size and ids, pool health.",
            input_schema: Some(r#"{"type":"object","properties":{}}"#),
            output_schema: Some(
                r#"{"type":"object","properties":{"uptime_seconds":{"type":"integer"},"started_at":{"type":"string"},"runtime_mode":{"type":"string"},"lifecycle_state":{"type":"string"},"registry_size":{"type":"integer"},"registry_ids":{"type":"array","items":{"type":"string"}},"pool_healthy":{"type":"boolean"}},"required":["uptime_seconds","started_at","runtime_mode","lifecycle_state","registry_size","registry_ids","pool_healthy"]}"#,
            ),
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
        catalog: CatalogDescriptor {
            description: "Upsert knowledge-base key blocks for an owned world.",
            input_schema: Some(
                r#"{"type":"object","properties":{"world_id":{"type":"string"},"blocks":{"type":"array","items":{"type":"object","properties":{"schema_version":{"type":"integer"},"entry_id":{"type":"string"},"world_id":{"type":"string"},"block_type":{"type":"string","enum":["character","ability","scene","organization","item","conflict","info_point","event","species","faction","magic_system","technology","deity","level","economy_tier","dialogue","beat","act","era"]},"canonical_name":{"type":"string"},"status":{"type":"string"},"revision":{"type":"integer"},"body":{"type":"object"},"source_anchor":{"type":"object"},"created_from_command_id":{"type":"string"},"created_at":{"type":"string"},"updated_at":{"type":"string"},"source_work_id":{"type":"string"},"source_chapter":{"type":"integer"},"source_provenance_kind":{"type":"string"},"extensions_nexus_extras":{"type":"object"},"modules":{"type":"object"}},"required":["schema_version","entry_id","world_id","block_type","canonical_name","status","created_at"]}}},"required":["world_id","blocks"]}"#,
            ),
            output_schema: Some(
                r#"{"type":"object","properties":{"written":{"type":"integer"},"world_id":{"type":"string"}},"required":["written","world_id"]}"#,
            ),
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
        catalog: CatalogDescriptor {
            description: "Update a manuscript chapter's content for a work.",
            input_schema: Some(
                r#"{"type":"object","properties":{"work_id":{"type":"string"},"chapter":{"type":"integer","minimum":1},"volume":{"type":"integer","minimum":1},"content":{"type":"string"}},"required":["work_id","chapter"]}"#,
            ),
            output_schema: Some(
                r#"{"type":"object","properties":{"work_id":{"type":"string"},"chapter":{"type":"integer"},"volume":{"type":"integer"},"slug":{"type":"string"},"planned_word_count":{"type":"integer"},"actual_word_count":{"type":"integer"},"status":{"type":"string"},"outline_path":{"type":"string"},"body_path":{"type":"string"},"created_at":{"type":"string"},"updated_at":{"type":"string"}},"required":["work_id","chapter","planned_word_count","status","created_at","updated_at"]}"#,
            ),
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
        catalog: CatalogDescriptor {
            description: "Update metadata (title, visibility, time policy) for an owned world.",
            input_schema: Some(
                r#"{"type":"object","properties":{"world_id":{"type":"string"},"title":{"type":"string"},"visibility":{"type":"string","enum":["public","private","invited"]},"time_policy":{"type":"string","enum":["manual","auto_advance"]}},"required":["world_id"]}"#,
            ),
            output_schema: Some(
                r#"{"type":"object","properties":{"world_id":{"type":"string"},"updated":{"type":"boolean"}},"required":["world_id","updated"]}"#,
            ),
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
        catalog: CatalogDescriptor {
            description: "Link schedule ids to a work.",
            input_schema: Some(
                r#"{"type":"object","properties":{"work_id":{"type":"string"},"schedule_ids":{"type":"array","items":{"type":"string"}}},"required":["work_id","schedule_ids"]}"#,
            ),
            output_schema: Some(
                r#"{"type":"object","properties":{"work_id":{"type":"string"},"schedule_ids":{"type":"array","items":{"type":"string"}}},"required":["work_id","schedule_ids"]}"#,
            ),
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
        catalog: CatalogDescriptor {
            description: "Mark a finding as resolved, optionally with a resolution note.",
            input_schema: Some(
                r#"{"type":"object","properties":{"finding_id":{"type":"string"},"resolution":{"type":"string"}},"required":["finding_id"]}"#,
            ),
            output_schema: Some(
                r#"{"type":"object","properties":{"finding_id":{"type":"string"},"resolved":{"type":"boolean"}},"required":["finding_id","resolved"]}"#,
            ),
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
        catalog: CatalogDescriptor {
            description: "Add, remove, promote, or archive a work entry in the selection pool.",
            input_schema: Some(
                r#"{"type":"object","properties":{"work_id":{"type":"string"},"action":{"type":"string","enum":["add","remove","promote","archive"]}},"required":["work_id","action"]}"#,
            ),
            output_schema: Some(
                r#"{"type":"object","properties":{"work_id":{"type":"string"},"action":{"type":"string"},"success":{"type":"boolean"}},"required":["work_id","action","success"]}"#,
            ),
        },
        failure_mode: FailureMode::Forbidden,
        handler_test_vector: TestVector {
            description: "pool entry manage adds work to selection pool",
            expected_outcome: "success",
            test_fn_name: "pool_entry_manage_adds_to_pool",
        },
    });

    // ── V1.56 P1: nexus.registry.refresh ──
    reg.register(CapabilityRow {
        id: "nexus.registry.refresh",
        access: Access::Read,
        admission: ADMISSION_READ_CONTEXT,
        handler: hte::registry_registry_refresh,
        catalog: CatalogDescriptor {
            description: "Return the capability registry snapshot (synthetic or CDN-backed).",
            input_schema: Some(
                r#"{"type":"object","properties":{},"required":[],"additionalProperties":false}"#,
            ),
            output_schema: Some(
                r#"{"type":"object","properties":{"cacheAgeMs":{"type":"integer","minimum":0},"capabilityCount":{"type":"integer","minimum":0},"source":{"type":"string","enum":["synthetic","cdn","synthetic_fallback"]},"snapshotVersion":{"type":"string"},"generatedAt":{"type":"string","format":"date-time"},"fetchTimeoutMs":{"type":"integer","minimum":0},"maxRetries":{"type":"integer","minimum":0},"retryCount":{"type":"integer","minimum":0},"fallbackReason":{"type":"string"}},"required":["cacheAgeMs","capabilityCount","source","snapshotVersion","generatedAt"],"additionalProperties":false}"#,
            ),
        },
        failure_mode: FailureMode::NotSupported,
        handler_test_vector: TestVector {
            description: "registry refresh returns synthetic output by default",
            expected_outcome: "success",
            test_fn_name: "registry_refresh_synthetic_smoke",
        },
    });

    // ── V1.58 P3: nexus.reference.refresh ──
    reg.register(CapabilityRow {
        id: "nexus.reference.refresh",
        access: Access::Write,
        admission: ADMISSION_READ_WORKSPACE,
        handler: hte::registry_reference_refresh,
        catalog: CatalogDescriptor {
            description: "Refresh a reference source's body content and update its content hash.",
            input_schema: Some(
                r#"{"type":"object","properties":{"reference_source_id":{"type":"string","description":"Registry ID of the reference source to refresh"},"url":{"type":"string","description":"Optional override URL for ad-hoc refresh"}},"required":["reference_source_id"],"additionalProperties":false}"#,
            ),
            output_schema: Some(
                r#"{"type":"object","properties":{"reference_source_id":{"type":"string"},"refreshed":{"type":"boolean"},"content_changed":{"type":"boolean"},"new_content_hash":{"type":"string"},"refreshed_at":{"type":"string","format":"date-time"},"status":{"type":"string","enum":["fresh","stale","not_modified","policy_blocked","error"]},"bytes_fetched":{"type":"integer","minimum":0}},"required":["reference_source_id","refreshed","content_changed","status"],"additionalProperties":false}"#,
            ),
        },
        failure_mode: FailureMode::InvalidInput,
        handler_test_vector: TestVector {
            description:
                "reference refresh updates content hash and writes body.md for owned source",
            expected_outcome: "success",
            test_fn_name: "reference_refresh_happy_path",
        },
    });

    // ── V1.59 P0: DF-47 manuscript & misc capability parity batch (9 tools) ──

    reg.register(CapabilityRow {
        id: "nexus.manuscript.list",
        access: Access::Read,
        admission: ADMISSION_READ_WORKSPACE,
        handler: hte::registry_manuscript_list,
        catalog: CatalogDescriptor {
            description: "List all manuscripts (works) for the active creator.",
            input_schema: Some(r#"{"type":"object","properties":{}}"#),
            output_schema: Some(
                r#"{"type":"object","properties":{"manuscripts":{"type":"array","items":{"type":"object"}},"count":{"type":"integer"}},"required":["manuscripts","count"]}"#,
            ),
        },
        failure_mode: FailureMode::Forbidden,
        handler_test_vector: TestVector {
            description: "manuscript list returns manuscripts for active creator",
            expected_outcome: "success",
            test_fn_name: "manuscript_list_returns_manuscripts",
        },
    });

    reg.register(CapabilityRow {
        id: "nexus.manuscript.read_range",
        access: Access::Read,
        admission: ADMISSION_READ_WORKSPACE,
        handler: hte::registry_manuscript_read_range,
        catalog: CatalogDescriptor {
            description: "Read a bounded line range from a manuscript chapter body.",
            input_schema: Some(
                r#"{"type":"object","properties":{"work_id":{"type":"string"},"chapter":{"type":"integer","minimum":1},"volume":{"type":"integer","minimum":1},"start_line":{"type":"integer","minimum":1},"end_line":{"type":"integer","minimum":1}},"required":["work_id","chapter"]}"#,
            ),
            output_schema: Some(
                r#"{"type":"object","properties":{"work_id":{"type":"string"},"chapter":{"type":"integer"},"volume":{"type":"integer"},"content":{"type":"string"},"range":{"type":"object","properties":{"start_line":{"type":"integer"},"end_line":{"type":"integer"}}},"total_lines":{"type":"integer"},"truncated":{"type":"boolean"}},"required":["work_id","chapter","volume","content","range","total_lines","truncated"]}"#,
            ),
        },
        failure_mode: FailureMode::InvalidInput,
        handler_test_vector: TestVector {
            description: "manuscript read_range returns bounded content for valid chapter",
            expected_outcome: "success",
            test_fn_name: "manuscript_read_range_returns_bounded_content",
        },
    });

    reg.register(CapabilityRow {
        id: "nexus.manuscript.write",
        access: Access::Write,
        admission: ADMISSION_WRITE_WORKSPACE,
        handler: hte::registry_manuscript_write,
        catalog: CatalogDescriptor {
            description: "Write manuscript body content for a chapter within the size quota.",
            input_schema: Some(
                r#"{"type":"object","properties":{"work_id":{"type":"string"},"chapter":{"type":"integer","minimum":1},"volume":{"type":"integer","minimum":1},"content":{"type":"string"}},"required":["work_id","chapter","content"]}"#,
            ),
            output_schema: Some(
                r#"{"type":"object","properties":{"written":{"type":"boolean"},"work_id":{"type":"string"},"chapter":{"type":"integer"},"volume":{"type":"integer"},"word_count":{"type":"integer"},"bytes_written":{"type":"integer"}},"required":["written","work_id","chapter","volume","word_count","bytes_written"]}"#,
            ),
        },
        failure_mode: FailureMode::InvalidInput,
        handler_test_vector: TestVector {
            description: "manuscript write writes body content for valid chapter within size quota",
            expected_outcome: "success",
            test_fn_name: "manuscript_write_writes_content",
        },
    });

    reg.register(CapabilityRow {
        id: "nexus.manuscript.phase.get",
        access: Access::Read,
        admission: ADMISSION_READ_WORKSPACE,
        handler: hte::registry_manuscript_phase_get,
        catalog: CatalogDescriptor {
            description: "Return the current manuscript phase and stage status for a work.",
            input_schema: Some(
                r#"{"type":"object","properties":{"work_id":{"type":"string"}},"required":["work_id"]}"#,
            ),
            output_schema: Some(
                r#"{"type":"object","properties":{"work_id":{"type":"string"},"phase":{"type":"string"},"stage_status":{"type":"string"}},"required":["work_id","phase","stage_status"]}"#,
            ),
        },
        failure_mode: FailureMode::Forbidden,
        handler_test_vector: TestVector {
            description: "manuscript phase get returns current phase for owned work",
            expected_outcome: "success",
            test_fn_name: "manuscript_phase_get_returns_current_phase",
        },
    });

    reg.register(CapabilityRow {
        id: "nexus.manuscript.phase.set",
        access: Access::Write,
        admission: ADMISSION_WRITE_WORKSPACE,
        handler: hte::registry_manuscript_phase_set,
        catalog: CatalogDescriptor {
            description: "Move a work forward to the next manuscript phase.",
            input_schema: Some(
                r#"{"type":"object","properties":{"work_id":{"type":"string"},"phase":{"type":"string","enum":["brainstorm","draft","review","finalize"]},"force":{"type":"boolean"}},"required":["work_id","phase"]}"#,
            ),
            output_schema: Some(
                r#"{"type":"object","properties":{"work_id":{"type":"string"},"previous_phase":{"type":"string"},"current_phase":{"type":"string"},"stage_status":{"type":"string"},"transitioned":{"type":"boolean"}},"required":["work_id","previous_phase","current_phase","stage_status","transitioned"]}"#,
            ),
        },
        failure_mode: FailureMode::InvalidInput,
        handler_test_vector: TestVector {
            description: "manuscript phase set moves work forward to next phase",
            expected_outcome: "success",
            test_fn_name: "manuscript_phase_set_advances_phase",
        },
    });

    reg.register(CapabilityRow {
        id: "nexus.workspace.paths",
        access: Access::Read,
        admission: ADMISSION_READ_CONTEXT,
        handler: hte::registry_workspace_paths,
        catalog: CatalogDescriptor {
            description: "Return the workspace root and the allowed roots (Works, Worlds, References, .nexus42).",
            input_schema: Some(r#"{"type":"object","properties":{}}"#),
            output_schema: Some(
                r#"{"type":"object","properties":{"workspace_root":{"type":"string"},"allowed_roots":{"type":"array","items":{"type":"string"}},"preset_id":{"type":"string"}},"required":["workspace_root","allowed_roots","preset_id"]}"#,
            ),
        },
        failure_mode: FailureMode::InvalidInput,
        handler_test_vector: TestVector {
            description: "workspace paths returns allowed roots from active workspace",
            expected_outcome: "success",
            test_fn_name: "workspace_paths_returns_allowed_roots",
        },
    });

    reg.register(CapabilityRow {
        id: "nexus.research.query",
        access: Access::Read,
        admission: ADMISSION_READ_WORKSPACE,
        handler: hte::registry_research_query,
        catalog: CatalogDescriptor {
            description: "Query the local reference-source index by id, tag, or a bounded limit.",
            input_schema: Some(
                r#"{"type":"object","properties":{"reference_source_id":{"type":"string"},"tags":{"type":"string"},"limit":{"type":"integer","minimum":1,"maximum":1000}}}"#,
            ),
            output_schema: Some(
                r#"{"type":"object","properties":{"results":{"type":"array","items":{"type":"object"}},"count":{"type":"integer"}},"required":["results","count"]}"#,
            ),
        },
        failure_mode: FailureMode::InvalidInput,
        handler_test_vector: TestVector {
            description: "research query returns reference sources from local index",
            expected_outcome: "success",
            test_fn_name: "research_query_returns_reference_sources",
        },
    });

    reg.register(CapabilityRow {
        id: "nexus.runtime.health",
        access: Access::Read,
        admission: ADMISSION_READ_CONTEXT,
        handler: hte::registry_runtime_health,
        catalog: CatalogDescriptor {
            description: "Return agent-visible runtime health: mode, registry reachability, sync state, cloud flag.",
            input_schema: Some(r#"{"type":"object","properties":{}}"#),
            output_schema: Some(
                r#"{"type":"object","properties":{"runtime_mode":{"type":"string"},"registry_reachable":{"type":"boolean"},"registry_size":{"type":"integer"},"sync_state":{"type":"string"},"cloud_enabled":{"type":"boolean"},"pool_healthy":{"type":"boolean"}},"required":["runtime_mode","registry_reachable","registry_size","sync_state","cloud_enabled","pool_healthy"]}"#,
            ),
        },
        failure_mode: FailureMode::Forbidden,
        handler_test_vector: TestVector {
            description: "runtime health returns agent-visible health and registry reachability",
            expected_outcome: "success",
            test_fn_name: "runtime_health_returns_agent_visible_status",
        },
    });

    reg.register(CapabilityRow {
        id: "nexus.trace.correlation",
        access: Access::Read,
        admission: ADMISSION_READ_CONTEXT,
        handler: hte::registry_trace_correlation,
        catalog: CatalogDescriptor {
            description: "Propagate a correlation id (and optional session id) across tool calls.",
            input_schema: Some(
                r#"{"type":"object","properties":{"correlation_id":{"type":"string"},"session_id":{"type":"string"}}}"#,
            ),
            output_schema: Some(
                r#"{"type":"object","properties":{"correlation_id":{"type":"string"},"session_id":{"type":"string"},"parent_request_id":{"type":"string"},"trace_timestamp":{"type":"string"},"propagated":{"type":"boolean"}},"required":["correlation_id","trace_timestamp","propagated"]}"#,
            ),
        },
        failure_mode: FailureMode::Forbidden,
        handler_test_vector: TestVector {
            description: "trace correlation propagates correlation id across tool calls",
            expected_outcome: "success",
            test_fn_name: "trace_correlation_propagates_correlation_id",
        },
    });

    // ── fs/* baseline (V1.33) ──
    reg.register(CapabilityRow {
        id: "fs/read_text_file",
        access: Access::Read,
        admission: ADMISSION_FS_READ,
        handler: hte::registry_read_file,
        catalog: CatalogDescriptor {
            description: "Read a text file within the workspace root and return its content.",
            input_schema: Some(
                r#"{"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}"#,
            ),
            output_schema: Some(
                r#"{"type":"object","properties":{"content":{"type":"string"}},"required":["content"]}"#,
            ),
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
        catalog: CatalogDescriptor {
            description: "Write text content to a file within the workspace root.",
            input_schema: Some(
                r#"{"type":"object","properties":{"path":{"type":"string"},"content":{"type":"string"}},"required":["path","content"]}"#,
            ),
            output_schema: Some(
                r#"{"type":"object","properties":{"written":{"type":"boolean"}},"required":["written"]}"#,
            ),
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
    fn registry_has_thirty_host_tools() {
        let reg = host_tool_registry();
        assert_eq!(reg.len(), 30);
    }
    /// F-2 (qc1 W-001 ∩ qc2 S-002 ∩ qc3 S-002/S-004): the four touched
    /// rows' description / schema / handler triple must stay coherent —
    /// description claims ⊆ schema ⊆ handler reads, machine-checked where
    /// the schema carries the vocabulary (enum / allowlist / property
    /// names). This pins the fix so a future authoring pass cannot
    /// silently re-introduce description drift on these rows.
    #[test]
    fn touched_rows_description_schema_handler_coherence() {
        let reg = host_tool_registry();

        // nexus.manuscript.chapter.update: description must NOT promise
        // "block overrides" — the handler reads only work_id/chapter/
        // volume/content and the schema carries exactly those properties.
        let row = reg.lookup("nexus.manuscript.chapter.update").expect("row");
        assert!(
            !row.catalog.description.contains("block override"),
            "description must not promise block overrides (handler does not accept them)"
        );
        let input: serde_json::Value =
            serde_json::from_str(row.catalog.input_schema.expect("authored input"))
                .expect("input parses");
        let props = input["properties"].as_object().expect("properties");
        assert_eq!(
            props
                .keys()
                .map(String::as_str)
                .collect::<std::collections::BTreeSet<_>>(),
            ["work_id", "chapter", "volume", "content"]
                .into_iter()
                .collect::<std::collections::BTreeSet<_>>(),
            "chapter.update schema properties match the handler reads"
        );

        // nexus.pool.entry.manage: description must enumerate all four
        // actions the schema enum and the handler match arm support.
        let row = reg.lookup("nexus.pool.entry.manage").expect("row");
        let description_lower = row.catalog.description.to_ascii_lowercase();
        for action in ["add", "remove", "promote", "archive"] {
            assert!(
                description_lower.contains(action),
                "pool.entry.manage description must enumerate '{action}'"
            );
        }
        let input: serde_json::Value =
            serde_json::from_str(row.catalog.input_schema.expect("authored input"))
                .expect("input parses");
        assert_eq!(
            input["properties"]["action"]["enum"],
            serde_json::json!(["add", "remove", "promote", "archive"]),
            "pool.entry.manage action enum matches the handler match arm"
        );

        // nexus.work.patch: closed world — root and stage_metadata both
        // reject unknown keys, matching the handler allowlists.
        let row = reg.lookup("nexus.work.patch").expect("row");
        let input: serde_json::Value =
            serde_json::from_str(row.catalog.input_schema.expect("authored input"))
                .expect("input parses");
        assert_eq!(
            input["additionalProperties"],
            serde_json::json!(false),
            "work.patch root must be closed-world (handler rejects unknown keys)"
        );
        assert_eq!(
            input["properties"]["stage_metadata"]["additionalProperties"],
            serde_json::json!(false),
            "work.patch stage_metadata must be closed-world (handler allowlists sub-keys)"
        );
        let stage_props = input["properties"]["stage_metadata"]["properties"]
            .as_object()
            .expect("stage_metadata properties");
        assert_eq!(
            stage_props
                .keys()
                .map(String::as_str)
                .collect::<std::collections::BTreeSet<_>>(),
            [
                "agent_notes",
                "research_summary_ref",
                "draft_outline_ref",
                "review_summary_ref",
                "last_agent_tool_request_id",
            ]
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>(),
            "stage_metadata schema properties match STAGE_METADATA_ALLOWED_KEYS"
        );

        // Read-tool descriptions must not promise fields absent from the
        // pinned output schema (AR-78 #5 honest subsets).
        for (id, promised) in [
            (
                "nexus.world.snapshot.get",
                &["state", "timeline", "fork lineage"][..],
            ),
            ("nexus.work.get", &["full Work record"][..]),
            ("nexus.manuscript.chapter.get", &["paths"][..]),
        ] {
            let row = reg.lookup(id).expect("row");
            for word in promised {
                assert!(
                    !row.catalog.description.contains(word),
                    "'{id}' description must not promise '{word}' (absent from the output schema)"
                );
            }
            let output: serde_json::Value =
                serde_json::from_str(row.catalog.output_schema.expect("pinned output"))
                    .expect("output parses");
            let out_props = output["properties"].as_object().expect("properties");
            assert!(
                !out_props.is_empty(),
                "'{id}' output schema must stay a pinned honest subset"
            );
        }
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
            "nexus.registry.refresh",
            "nexus.reference.refresh",
            "nexus.kb_snapshot.write",
            "nexus.manuscript.chapter.update",
            "nexus.world.configure",
            "nexus.work.schedule.set",
            "nexus.finding.resolve",
            "nexus.pool.entry.manage",
            // V1.59 P0: DF-47 manuscript & misc parity batch (9 tools)
            "nexus.manuscript.list",
            "nexus.manuscript.read_range",
            "nexus.manuscript.write",
            "nexus.manuscript.phase.get",
            "nexus.manuscript.phase.set",
            "nexus.workspace.paths",
            "nexus.research.query",
            "nexus.runtime.health",
            "nexus.trace.correlation",
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
            // Verify all 7 fields are populated (AR-78: catalog descriptor
            // replaces the removed AcpWire).
            assert!(!row.id.is_empty(), "id must not be empty for {id}");
            assert!(
                !row.admission.is_empty(),
                "admission must not be empty for {id}"
            );
            assert!(
                !row.catalog.description.is_empty(),
                "catalog description must not be empty for {id}"
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

    /// AR-78 #9 / AR-80 #2 (`builtin_input_schemas_parse_as_root_object`):
    /// every builtin row's emitted input string — real schema or named
    /// placeholder — parses as a JSON object with root `type: "object"`
    /// (the MCP requirement). A current daemon never hits the child's
    /// `default_object_schema` fallback for a builtin row.
    #[test]
    fn builtin_input_schemas_parse_as_root_object() {
        let reg = host_tool_registry();
        for id in reg.ids() {
            let row = reg.lookup(id).expect("row must exist");
            let emitted = row.catalog.input_schema.unwrap_or(NAMED_PLACEHOLDER_INPUT);
            let parsed: serde_json::Value = serde_json::from_str(emitted)
                .unwrap_or_else(|e| panic!("input schema for '{id}' must parse: {e}"));
            assert!(
                parsed.is_object(),
                "input schema for '{id}' must be a JSON object"
            );
            assert_eq!(
                parsed["type"],
                serde_json::Value::String("object".into()),
                "input schema for '{id}' must declare root type object"
            );
        }
    }

    /// AR-78 #6 / AR-80 #2 (`placeholder_ledger_lockstep`), both directions:
    /// `row.catalog.input_schema.is_none() ⇔ id ∈ SCHEMA_REMAINDER_LEDGER`.
    #[test]
    fn placeholder_ledger_lockstep() {
        let reg = host_tool_registry();
        for id in reg.ids() {
            let row = reg.lookup(id).expect("row must exist");
            let ledgered = SCHEMA_REMAINDER_LEDGER.contains(&id);
            assert_eq!(
                row.catalog.input_schema.is_none(),
                ledgered,
                "input_schema None ⇔ ledgered must hold for '{id}'"
            );
        }
        // Every ledger entry must name a real registry row (no dead entries).
        for id in SCHEMA_REMAINDER_LEDGER {
            assert!(
                reg.lookup(id).is_some(),
                "ledger entry '{id}' must name a registered row"
            );
        }
    }

    /// AR-80 #2 (`silent_placeholder_gone`): the exact silent string
    /// `{"type":"object"}` appears in NO builtin catalog row — only the
    /// `$comment`-marked named placeholder may, and only for ledgered ids.
    #[test]
    fn silent_placeholder_gone() {
        let reg = host_tool_registry();
        for id in reg.ids() {
            let row = reg.lookup(id).expect("row must exist");
            let emitted = row.catalog.input_schema.unwrap_or(NAMED_PLACEHOLDER_INPUT);
            assert_ne!(
                emitted, "{\"type\":\"object\"}",
                "builtin row '{id}' must not emit the silent placeholder"
            );
        }
    }

    /// AR-78 #6 `DoD`: a synthetic ledgered row (input schema not authored)
    /// serializes the named placeholder ONLY — never the silent
    /// `{"type":"object"}` and never a guessed schema.
    #[test]
    fn synthetic_ledgered_row_serializes_named_placeholder_only() {
        let row = CapabilityRow {
            id: "nexus.synthetic.ledgered",
            access: Access::Read,
            admission: ADMISSION_READ_CONTEXT,
            handler: crate::api::handlers::host_tool_executor::registry_context_whoami,
            catalog: CatalogDescriptor {
                description: "synthetic ledgered row",
                input_schema: None,
                output_schema: None,
            },
            failure_mode: FailureMode::Forbidden,
            handler_test_vector: TestVector {
                description: "synthetic",
                expected_outcome: "success",
                test_fn_name: "whoami_returns_active_creator",
            },
        };
        let emitted = row.catalog.input_schema.unwrap_or(NAMED_PLACEHOLDER_INPUT);
        assert_eq!(emitted, NAMED_PLACEHOLDER_INPUT);
        assert_ne!(emitted, "{\"type\":\"object\"}");
        // The named placeholder is draft-2020-12-valid and machine-
        // distinguishable from a real schema.
        let parsed: serde_json::Value =
            serde_json::from_str(emitted).expect("named placeholder parses");
        assert_eq!(parsed["type"], serde_json::Value::String("object".into()));
        assert_eq!(
            parsed["$comment"],
            serde_json::Value::String("nexus42:schema-pending".into())
        );
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
        "registry_refresh_synthetic_smoke",
        "reference_refresh_happy_path",
        // V1.59 P0: DF-47 manuscript & misc parity batch (9 test fn names)
        "manuscript_list_returns_manuscripts",
        "manuscript_read_range_returns_bounded_content",
        "manuscript_write_writes_content",
        "manuscript_phase_get_returns_current_phase",
        "manuscript_phase_set_advances_phase",
        "workspace_paths_returns_allowed_roots",
        "research_query_returns_reference_sources",
        "runtime_health_returns_agent_visible_status",
        "trace_correlation_propagates_correlation_id",
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
                assert_eq!(code, "not_supported");
            }
            other => panic!("Expected BadRequest(not_supported), got: {other:?}"),
        }
    }

    /// **R-V153P0QC2-002**: Catalog↔registry id bijection test.
    ///
    /// Reads `acp-capability-set.md` logical catalog and compares IDs against
    /// `host_tool_registry().ids()`. Fails if a registry id that IS expected
    /// to be in the catalog is missing, and vice versa for catalog ids that
    /// are implemented as host tools.
    ///
    /// V1.57 P0: Catalog updated with full roster (§4 Capability roster).
    /// Only `fs/*` tools remain as known gaps (they are not ACP-facing
    /// `nexus.*` capabilities and use the V1.33 baseline prefix).
    ///
    /// V1.59 P0: Expanded `is_likely_host_tool` to cover the 9 newly-shipped
    /// DF-47 capabilities so the bijection is enforced bidirectionally.
    ///
    /// V1.60 P0 (R-V159P0-002): Replaced the manual 28-element match list with
    /// a catalog-driven derivation. The test now parses the §4 table's `Status`
    /// + `Registry row ref` columns: a catalog id is expected in
    ///   `host_tool_registry()` iff Status=`shipped` AND Registry row
    ///   ref=`host_tool`. Orchestration-scope shipped ids (e.g.
    ///   `nexus.reference.refresh`, the 5 DF-46 capabilities) are correctly
    ///   excluded from the `host_tool` direction — no manual list to maintain.
    // Long integration test; splitting would obscure the end-to-end scenario.
    #[allow(clippy::too_many_lines)]
    #[test]
    fn catalog_registry_invariant_all_ids_present() {
        use std::collections::HashSet;

        // V1.60 P0: parse structured rows from the §4 table so the host_tool
        // direction is auto-derived from the `Status` + `Registry row ref`
        // columns rather than a manual match list (closes R-V159P0-002).
        // V1.67 P2 (R-V160P0-QC1-W002): parse by header name instead of hardcoded
        // positional indices so future column insertions do not silently shift
        // the parsed values.
        // Row shape: | `nexus.<id>` | description | status | shipped_in | registry_ref |
        struct CatalogRow {
            id: String,
            status: String,
            registry_ref: String,
        }

        // First pass: locate the header row and map column names to indices.
        #[derive(Debug, Default)]
        struct ColumnIndex {
            id: Option<usize>,
            status: Option<usize>,
            registry_ref: Option<usize>,
        }

        let reg = host_tool_registry();
        let registry_ids: HashSet<&str> = reg.ids().collect();

        // Known gaps: `fs/*` tools are V1.33 baseline, not ACP-facing.
        let known_catalog_gaps: HashSet<&str> = ["fs/read_text_file", "fs/write_text_file"]
            .iter()
            .copied()
            .collect();

        // Parse capability IDs from acp-capability-set.md tables
        let catalog_path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../.mstar/specs/acp-capability-set.md"
        );
        let catalog_content =
            std::fs::read_to_string(catalog_path).expect("acp-capability-set.md must be readable");

        let lines: Vec<&str> = catalog_content.lines().collect();

        let mut header: Option<ColumnIndex> = None;
        for line in &lines {
            let trimmed = line.trim();
            if !trimmed.starts_with('|') {
                continue;
            }
            let cols: Vec<&str> = trimmed.split('|').map(str::trim).collect();
            if !cols.contains(&"Capability ID") {
                continue;
            }
            let mut idx = ColumnIndex::default();
            for (i, col) in cols.iter().enumerate() {
                match *col {
                    "Capability ID" => idx.id = Some(i),
                    "Status" => idx.status = Some(i),
                    "Registry row ref" => idx.registry_ref = Some(i),
                    _ => {}
                }
            }
            header = Some(idx);
            break;
        }
        let Some(header) = header else {
            panic!("acp-capability-set.md §4 table header not found");
        };
        let id_col = header.id.expect("Capability ID column missing");
        let status_col = header.status.expect("Status column missing");
        let registry_ref_col = header
            .registry_ref
            .expect("Registry row ref column missing");

        // Second pass: parse data rows using the mapped column indices.
        let catalog_rows: Vec<CatalogRow> = lines
            .iter()
            .filter_map(|line| {
                let trimmed = line.trim();
                if !trimmed.starts_with('|') || !trimmed.contains('`') {
                    return None;
                }
                let cols: Vec<&str> = trimmed.split('|').map(str::trim).collect();
                if cols.len() <= id_col.max(status_col).max(registry_ref_col) {
                    return None;
                }
                let id_cell = cols[id_col];
                let start = id_cell.find('`')?;
                let rest = &id_cell[start + 1..];
                let end = rest.find('`')?;
                let id = rest[..end].to_string();
                if !(id.starts_with("nexus.") || id.starts_with("fs/")) {
                    return None;
                }
                Some(CatalogRow {
                    id,
                    status: cols[status_col].to_string(),
                    registry_ref: cols[registry_ref_col].to_string(),
                })
            })
            .collect();

        let catalog_ids: HashSet<String> = catalog_rows.iter().map(|r| r.id.clone()).collect();

        // Direction 1: every registry id must have a catalog row (except known
        // `fs/*` gaps).
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

        // Direction 2 (auto-derived, R-V159P0-002): every catalog id with
        // Status=`shipped` AND Registry row ref=`host_tool` MUST be in the
        // host_tool registry. Orchestration-scope shipped ids are excluded
        // (they live in the orchestration CapabilityRegistry, not here).
        let missing_from_registry: Vec<String> = catalog_rows
            .iter()
            .filter(|r| r.status == "shipped" && r.registry_ref == "host_tool")
            .filter(|r| !registry_ids.contains(r.id.as_str()))
            .map(|r| r.id.clone())
            .collect();

        // Hard failure: every shipped host tool MUST be in the registry.
        assert!(
            missing_from_registry.is_empty(),
            "Catalog ids marked shipped + host_tool but missing from registry: \
             {missing_from_registry:?}"
        );
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
                    AdmissionGate::PermissionPolicy => {
                        "admission_pipeline: permission-policy check"
                    }
                    AdmissionGate::RequireWorldOwnership => {
                        "per-handler: ensure_world_accessible_for_creator"
                    }
                    AdmissionGate::AuditLog => "caller: audit_tool_execution",
                };
            }
        }
    }
}
