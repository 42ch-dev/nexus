//! The canonical host-tool catalog: pure metadata, no executable bindings.
//!
//! This module is the single declaration of the host tools the product
//! reserves and lists: identity, access classification, admission gates,
//! catalog descriptors (real schemas), failure mode and test identity. It
//! compiles in every cohort and imports no owner, capability, engine, provider
//! or simulator, so the reserved namespace never depends on which handlers a
//! build links.
//!
//! The runtime owner (`crate::owner::capabilities`) binds a real handler to
//! each [`HostToolId`] and borrows the metadata from here. It restates neither
//! the id strings nor the schemas.
// invariant: pure metadata — no executable bindings, and no owner/capability/engine/provider import.

use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

/// note: access classification for a capability row.
///
/// Used by admission gates and audit to determine the
/// risk profile of a capability at dispatch time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Access {
    /// note: read-only; no side effects.
    Read,
    /// note: mutation-capable; may write to DB, filesystem, or state.
    Write,
    /// note: access depends on runtime policy (e.g. `permissions.toml`
    /// or DA-005 `ContextPermissionGrant`).
    PolicyGated,
}

/// note: ordered fail-closed admission gate before handler dispatch.
///
/// Each gate must pass (or be explicitly skipped for a given
/// capability) before the handler is invoked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdmissionGate {
    /// invariant: Tool ID must be in the allowlist.
    Allowlist,
    /// invariant: active creator must exist (for `nexus.*` tools).
    ActiveCreator,
    /// invariant: operation must be within workspace bounds.
    WorkspaceBounds,
    /// invariant: `permissions.toml` / policy must grant the capability.
    PermissionPolicy,
    /// invariant: World must exist and be owned by the active creator.
    RequireWorldOwnership,
    /// invariant: audit log entry must be written (always last gate).
    AuditLog,
}

/// note: catalog descriptor for a capability (AR-78, DF-89).
///
/// Carries the authored tool summary plus real draft-2020-12 JSON-Schema
/// text for the input (and, where pinned, output) shape. `&'static str`
/// literals fit the static-row design exactly — no parse at catalog build,
/// no new dependency. The schema text is the single source of truth: it
/// flows catalog → route → MCP child parse.
#[derive(Debug, Clone)]
pub struct CatalogDescriptor {
    /// Authored tool summary for LLM/script consumers (replaces the
    /// `TestVector.description` reuse that ended with AR-78 #4).
    pub description: &'static str,
    /// note: real draft-2020-12 JSON-Schema text (root `"type":"object"`) for the
    /// tool's input. REQUIRED: every canonical row authors one, so there is no
    /// placeholder or omission path — a row without a schema does not compile.
    pub input_schema: &'static str,
    /// note: real draft-2020-12 JSON-Schema text for the success shape when the
    /// handler's output is a stable object worth pinning; `None` otherwise
    /// (omission is honest and rule-based — no ledger entry for outputs).
    pub output_schema: Option<&'static str>,
}

/// note: stable failure mode contract for a capability.
///
/// Defines the error surface a caller can expect when
/// the capability is denied or fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FailureMode {
    /// note: capability is not supported in this runtime configuration.
    NotSupported,
    /// note: policy (permissions or admission gate) blocked execution.
    PolicyBlocked,
    /// note: authentication/authorization failed.
    Forbidden,
    /// note: input validation failed.
    InvalidInput,
    /// note: internal error (database, filesystem, etc.).
    Internal,
}

/// note: test vector descriptor for a capability row.
///
/// Each row must have at least one success and one
/// failure test proving the handler works correctly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestVector {
    /// note: human-readable description of what the test covers.
    pub description: &'static str,
    /// note: expected outcome — "success", "`failure:policy_blocked`", etc.
    pub expected_outcome: &'static str,
    /// note: name of the test function (for grep-ability).
    pub test_fn_name: &'static str,
}

/// note: typed identity of one canonical host tool.
///
/// The runtime binder matches on this key, so a canonical tool without a real
/// handler binding is a compile error rather than a silent fallback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HostToolId {
    /// note: `nexus.context.whoami`
    ContextWhoami,
    /// note: `nexus.workspace.info`
    WorkspaceInfo,
    /// note: `nexus.work.get`
    WorkGet,
    /// note: `nexus.work.patch`
    WorkPatch,
    /// note: `nexus.orchestration.schedule_status`
    OrchestrationScheduleStatus,
    /// note: `nexus.strategy.generate`
    StrategyGenerate,
    /// note: `nexus.context.assemble`
    ContextAssemble,
    /// note: `nexus.world.snapshot.get`
    WorldSnapshotGet,
    /// note: `nexus.timeline.recent.get`
    TimelineRecentGet,
    /// note: `nexus.kb_snapshot.read`
    KbSnapshotRead,
    /// note: `nexus.manuscript.chapter.get`
    ManuscriptChapterGet,
    /// note: `nexus.observability.daemon.health`
    ObservabilityDaemonHealth,
    /// note: `nexus.kb_snapshot.write`
    KbSnapshotWrite,
    /// note: `nexus.manuscript.chapter.update`
    ManuscriptChapterUpdate,
    /// note: `nexus.world.configure`
    WorldConfigure,
    /// note: `nexus.work.schedule.set`
    WorkScheduleSet,
    /// note: `nexus.finding.resolve`
    FindingResolve,
    /// note: `nexus.pool.entry.manage`
    PoolEntryManage,
    /// note: `nexus.registry.refresh`
    RegistryRefresh,
    /// note: `nexus.manuscript.list`
    ManuscriptList,
    /// note: `nexus.manuscript.read_range`
    ManuscriptReadRange,
    /// note: `nexus.manuscript.write`
    ManuscriptWrite,
    /// note: `nexus.manuscript.phase.get`
    ManuscriptPhaseGet,
    /// note: `nexus.manuscript.phase.set`
    ManuscriptPhaseSet,
    /// note: `nexus.workspace.paths`
    WorkspacePaths,
    /// note: `nexus.runtime.health`
    RuntimeHealth,
    /// note: `nexus.trace.correlation`
    TraceCorrelation,
    /// note: `fs/read_text_file`
    FsReadTextFile,
    /// note: `fs/write_text_file`
    FsWriteTextFile,
}

impl HostToolId {
    /// note: the stable wire id.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ContextWhoami => "nexus.context.whoami",
            Self::WorkspaceInfo => "nexus.workspace.info",
            Self::WorkGet => "nexus.work.get",
            Self::WorkPatch => "nexus.work.patch",
            Self::OrchestrationScheduleStatus => "nexus.orchestration.schedule_status",
            Self::StrategyGenerate => "nexus.strategy.generate",
            Self::ContextAssemble => "nexus.context.assemble",
            Self::WorldSnapshotGet => "nexus.world.snapshot.get",
            Self::TimelineRecentGet => "nexus.timeline.recent.get",
            Self::KbSnapshotRead => "nexus.kb_snapshot.read",
            Self::ManuscriptChapterGet => "nexus.manuscript.chapter.get",
            Self::ObservabilityDaemonHealth => "nexus.observability.daemon.health",
            Self::KbSnapshotWrite => "nexus.kb_snapshot.write",
            Self::ManuscriptChapterUpdate => "nexus.manuscript.chapter.update",
            Self::WorldConfigure => "nexus.world.configure",
            Self::WorkScheduleSet => "nexus.work.schedule.set",
            Self::FindingResolve => "nexus.finding.resolve",
            Self::PoolEntryManage => "nexus.pool.entry.manage",
            Self::RegistryRefresh => "nexus.registry.refresh",
            Self::ManuscriptList => "nexus.manuscript.list",
            Self::ManuscriptReadRange => "nexus.manuscript.read_range",
            Self::ManuscriptWrite => "nexus.manuscript.write",
            Self::ManuscriptPhaseGet => "nexus.manuscript.phase.get",
            Self::ManuscriptPhaseSet => "nexus.manuscript.phase.set",
            Self::WorkspacePaths => "nexus.workspace.paths",
            Self::RuntimeHealth => "nexus.runtime.health",
            Self::TraceCorrelation => "nexus.trace.correlation",
            Self::FsReadTextFile => "fs/read_text_file",
            Self::FsWriteTextFile => "fs/write_text_file",
        }
    }
}

/// note: one canonical host tool — everything a caller may know before a handler is
/// bound.
#[derive(Debug, Clone)]
pub struct HostToolMetadata {
    /// note: stable typed id.
    pub id: HostToolId,
    /// note: access classification.
    pub access: Access,
    /// note: ordered fail-closed admission gates.
    pub admission: &'static [AdmissionGate],
    /// note: catalog descriptor (authored description + real draft-2020-12 schemas).
    pub catalog: CatalogDescriptor,
    /// note: expected failure mode when denied.
    pub failure_mode: FailureMode,
    /// note: test vector descriptor.
    pub handler_test_vector: TestVector,
}

/// note: static admission gate arrays (defined once, referenced by every row).
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

/// note: the agent tool's input contract — `GenerateRequest` rendered as JSON Schema, plus the address
/// the CLI and HTTP surfaces carry in arguments and the path.
///
/// The schema is the committed wire contract itself, so the tool description IS the contract (D8):
/// a model fills the request's fields instead of inventing a shape, and the schema cannot describe
/// a request the kernel would not accept. The tool adds only its own address field: `strategy_id`
/// is the one field a tool call names that the wire schema does not, because the HTTP surface
/// carries it in the path instead. Wire-level requiredness stays the committed schema's.
pub(crate) fn tool_input_schema() -> &'static str {
    static SCHEMA: LazyLock<String> = LazyLock::new(|| {
        let mut schema: serde_json::Value = serde_json::from_str(include_str!(
            "../../../schemas/daemon-api/orchestration/strategy-generate-request.schema.json"
        ))
        .expect("the generate request schema is committed JSON");
        let root = schema
            .as_object_mut()
            .expect("the generate request schema is a JSON object");
        root.insert(
            "title".to_owned(),
            serde_json::Value::String("NexusStrategyGenerate".to_owned()),
        );
        let properties = root["properties"]
            .as_object_mut()
            .expect("the generate request schema declares properties");
        properties.insert(
            "strategy_id".to_owned(),
            serde_json::json!({
                "type": "string",
                "minLength": 1,
                "pattern": "^[a-z][a-z0-9._-]*$",
                "description": "Strategy the candidate belongs to. It is the document id and the \
                                strategy the revision guard reads; the HTTP surface carries it in \
                                the path instead.",
            }),
        );
        // why: the tool's own address field is PREPENDED to the committed array — the
        // wire schema keeps owning which request fields are required, and the
        // rendered envelope stays what it was before the move.
        let required = root["required"]
            .as_array_mut()
            .expect("the generate request schema declares required");
        required.insert(0, serde_json::Value::String("strategy_id".to_owned()));
        schema.to_string()
    });
    SCHEMA.as_str()
}

/// note: the canonical rows, in declaration order.
///
/// `tool_input_schema` renders at first use, so the table is lazily built; the
/// returned slice is process-lifetime.
static HOST_TOOL_CATALOG: LazyLock<[HostToolMetadata; 29]> = LazyLock::new(|| {
    [
        HostToolMetadata {
            id: HostToolId::ContextWhoami,
            access: Access::Read,
            admission: ADMISSION_READ_CONTEXT,
            catalog: CatalogDescriptor {
                description: "Return the active creator id and workspace slug for the current session.",
                input_schema: r#"{"type":"object","properties":{}}"#,
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
        },
        HostToolMetadata {
            id: HostToolId::WorkspaceInfo,
            access: Access::Read,
            admission: ADMISSION_READ_CONTEXT,
            catalog: CatalogDescriptor {
                description: "Return workspace details: creator id, slug, path, runtime mode, and initialization state.",
                input_schema: r#"{"type":"object","properties":{}}"#,
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
        },
        HostToolMetadata {
            id: HostToolId::WorkGet,
            access: Access::Read,
            admission: ADMISSION_READ_WORKSPACE,
            catalog: CatalogDescriptor {
                description: "Return the Work record fields exposed by the catalog: status, title, current stage, and stage status.",
                input_schema: r#"{"type":"object","properties":{"work_id":{"type":"string"}},"required":["work_id"]}"#,
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
        },
        HostToolMetadata {
            id: HostToolId::WorkPatch,
            access: Access::Write,
            admission: ADMISSION_WRITE_WORKSPACE,
            catalog: CatalogDescriptor {
                description: "Patch a work's title, inspiration log, or stage metadata (stage field itself is rejected).",
                input_schema: r#"{"type":"object","properties":{"work_id":{"type":"string"},"title":{"type":"string"},"inspiration_log":{"type":"array","items":{"type":"object","properties":{"text":{"type":"string"},"note":{"type":"string"},"source":{"type":"string"}},"anyOf":[{"required":["text"]},{"required":["note"]}]}},"stage_metadata":{"type":"object","properties":{"agent_notes":{"type":"string"},"research_summary_ref":{"type":"string"},"draft_outline_ref":{"type":"string"},"review_summary_ref":{"type":"string"},"last_agent_tool_request_id":{"type":"string"}},"additionalProperties":false}},"required":["work_id"],"additionalProperties":false}"#,
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
        },
        HostToolMetadata {
            id: HostToolId::OrchestrationScheduleStatus,
            access: Access::Read,
            admission: ADMISSION_READ_WORKSPACE,
            catalog: CatalogDescriptor {
                description: "Return the schedule ids linked to a work and their count.",
                input_schema: r#"{"type":"object","properties":{"work_id":{"type":"string"}},"required":["work_id"]}"#,
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
        },
        HostToolMetadata {
            id: HostToolId::StrategyGenerate,
            access: Access::Write,
            admission: ADMISSION_WRITE_WORKSPACE,
            catalog: CatalogDescriptor {
                description: "Generate one committed strategy candidate from a declarative request \
                              (nodes, edges, nests, meta) through the same lint-correct kernel the CLI \
                              and HTTP surfaces use. Success means structurally legal and nothing \
                              more: the candidate still has to be simulated and approved. On lint \
                              exhaustion the result is the `lint_exhausted` payload with one \
                              actionable correction per violation, and nothing was persisted.",
                // note: the rendered `GenerateRequest` contract (D8) — the tool description IS the schema.
                input_schema: tool_input_schema(),
                // note: omitted — the success shape is the WorkflowDocument contract, which is authored as
                // its own `$ref`-addressed schema and cannot be carried here verbatim.
                output_schema: None,
            },
            failure_mode: FailureMode::Forbidden,
            handler_test_vector: TestVector {
                description: "the tool commits the requested candidate through the generate kernel",
                expected_outcome: "success",
                test_fn_name: "strategy_generate_tool_and_cli_and_http_reach_one_kernel",
            },
        },
        HostToolMetadata {
            id: HostToolId::ContextAssemble,
            access: Access::Read,
            admission: ADMISSION_READ_CONTEXT,
            catalog: CatalogDescriptor {
                description: "Assemble the current context moment; requires_platform requests platform-integrated assembly.",
                input_schema: r#"{"type":"object","properties":{"work_id":{"type":"string"},"requires_platform":{"type":"boolean"}}}"#,
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
        },
        HostToolMetadata {
            id: HostToolId::WorldSnapshotGet,
            access: Access::Read,
            admission: ADMISSION_READ_WORLD,
            catalog: CatalogDescriptor {
                description: "Return the world snapshot fields exposed by the catalog: title, slug, status, fork flag, and creation time.",
                input_schema: r#"{"type":"object","properties":{"world_id":{"type":"string"}},"required":["world_id"]}"#,
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
        },
        HostToolMetadata {
            id: HostToolId::TimelineRecentGet,
            access: Access::Read,
            admission: ADMISSION_READ_WORLD,
            catalog: CatalogDescriptor {
                description: "Return the most recent timeline events for a world (default 100, clamped to 500).",
                input_schema: r#"{"type":"object","properties":{"world_id":{"type":"string"},"limit":{"type":"integer","minimum":1,"maximum":500}},"required":["world_id"]}"#,
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
        },
        HostToolMetadata {
            id: HostToolId::KbSnapshotRead,
            access: Access::Read,
            admission: ADMISSION_READ_WORLD,
            catalog: CatalogDescriptor {
                description: "Return the knowledge-base key blocks for an owned world.",
                input_schema: r#"{"type":"object","properties":{"world_id":{"type":"string"}},"required":["world_id"]}"#,
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
        },
        HostToolMetadata {
            id: HostToolId::ManuscriptChapterGet,
            access: Access::Read,
            admission: ADMISSION_READ_WORKSPACE,
            catalog: CatalogDescriptor {
                description: "Return the manuscript chapter fields exposed by the catalog: status and planned word count.",
                input_schema: r#"{"type":"object","properties":{"work_id":{"type":"string"},"chapter":{"type":"integer","minimum":1},"volume":{"type":"integer","minimum":1}},"required":["work_id","chapter"]}"#,
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
        },
        HostToolMetadata {
            id: HostToolId::ObservabilityDaemonHealth,
            access: Access::Read,
            admission: ADMISSION_READ_CONTEXT,
            catalog: CatalogDescriptor {
                description: "Return daemon runtime health: uptime, lifecycle context, registry size and ids, pool health.",
                input_schema: r#"{"type":"object","properties":{}}"#,
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
        },
        HostToolMetadata {
            id: HostToolId::KbSnapshotWrite,
            access: Access::Write,
            admission: ADMISSION_WRITE_WORLD,
            catalog: CatalogDescriptor {
                description: "Upsert knowledge-base key blocks for an owned world.",
                input_schema: r#"{"type":"object","properties":{"world_id":{"type":"string"},"blocks":{"type":"array","items":{"type":"object","properties":{"schema_version":{"type":"integer"},"entry_id":{"type":"string"},"world_id":{"type":"string"},"block_type":{"type":"string","enum":["character","ability","scene","organization","item","conflict","info_point","event","species","faction","magic_system","technology","deity","level","economy_tier","dialogue","beat","act","era"]},"canonical_name":{"type":"string"},"status":{"type":"string"},"revision":{"type":"integer"},"body":{"type":"object"},"source_anchor":{"type":"object"},"created_from_command_id":{"type":"string"},"created_at":{"type":"string"},"updated_at":{"type":"string"},"source_work_id":{"type":"string"},"source_chapter":{"type":"integer"},"source_provenance_kind":{"type":"string"},"extensions_nexus_extras":{"type":"object"},"modules":{"type":"object"}},"required":["schema_version","entry_id","world_id","block_type","canonical_name","status","created_at"]}}},"required":["world_id","blocks"]}"#,
                output_schema: Some(
                    r#"{"type":"object","properties":{"written":{"type":"integer"},"world_id":{"type":"string"},"suppressed_writes":{"type":"array","items":{"type":"object","properties":{"entry_id":{"type":"string"}},"required":["entry_id"]}}},"required":["written","world_id"]}"#,
                ),
            },
            failure_mode: FailureMode::Forbidden,
            handler_test_vector: TestVector {
                description: "kb snapshot write upserts key blocks for owned world",
                expected_outcome: "success",
                test_fn_name: "simulated_knowledge_write_records_and_preserves_the_store",
            },
        },
        HostToolMetadata {
            id: HostToolId::ManuscriptChapterUpdate,
            access: Access::Write,
            admission: ADMISSION_WRITE_WORKSPACE,
            catalog: CatalogDescriptor {
                description: "Update a manuscript chapter's content for a work.",
                input_schema: r#"{"type":"object","properties":{"work_id":{"type":"string"},"chapter":{"type":"integer","minimum":1},"volume":{"type":"integer","minimum":1},"content":{"type":"string"}},"required":["work_id","chapter"]}"#,
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
        },
        HostToolMetadata {
            id: HostToolId::WorldConfigure,
            access: Access::Write,
            admission: ADMISSION_WRITE_WORLD,
            catalog: CatalogDescriptor {
                description: "Update metadata (title, visibility, time policy) for an owned world.",
                input_schema: r#"{"type":"object","properties":{"world_id":{"type":"string"},"title":{"type":"string"},"visibility":{"type":"string","enum":["public","private","invited"]},"time_policy":{"type":"string","enum":["manual","auto_advance"]}},"required":["world_id"]}"#,
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
        },
        HostToolMetadata {
            id: HostToolId::WorkScheduleSet,
            access: Access::Write,
            admission: ADMISSION_WRITE_WORKSPACE,
            catalog: CatalogDescriptor {
                description: "Link schedule ids to a work.",
                input_schema: r#"{"type":"object","properties":{"work_id":{"type":"string"},"schedule_ids":{"type":"array","items":{"type":"string"}}},"required":["work_id","schedule_ids"]}"#,
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
        },
        HostToolMetadata {
            id: HostToolId::FindingResolve,
            access: Access::Write,
            admission: ADMISSION_WRITE_WORKSPACE,
            catalog: CatalogDescriptor {
                description: "Mark a finding as resolved, optionally with a resolution note.",
                input_schema: r#"{"type":"object","properties":{"finding_id":{"type":"string"},"resolution":{"type":"string"}},"required":["finding_id"]}"#,
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
        },
        HostToolMetadata {
            id: HostToolId::PoolEntryManage,
            access: Access::Write,
            admission: ADMISSION_POOL_WRITE,
            catalog: CatalogDescriptor {
                description: "Add, remove, promote, or archive a work entry in the selection pool.",
                input_schema: r#"{"type":"object","properties":{"work_id":{"type":"string"},"action":{"type":"string","enum":["add","remove","promote","archive"]}},"required":["work_id","action"]}"#,
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
        },
        HostToolMetadata {
            id: HostToolId::RegistryRefresh,
            access: Access::Read,
            admission: ADMISSION_READ_CONTEXT,
            catalog: CatalogDescriptor {
                description: "Return the capability registry snapshot (synthetic or CDN-backed).",
                input_schema: r#"{"type":"object","properties":{},"required":[],"additionalProperties":false}"#,
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
        },
        HostToolMetadata {
            id: HostToolId::ManuscriptList,
            access: Access::Read,
            admission: ADMISSION_READ_WORKSPACE,
            catalog: CatalogDescriptor {
                description: "List all manuscripts (works) for the active creator.",
                input_schema: r#"{"type":"object","properties":{}}"#,
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
        },
        HostToolMetadata {
            id: HostToolId::ManuscriptReadRange,
            access: Access::Read,
            admission: ADMISSION_READ_WORKSPACE,
            catalog: CatalogDescriptor {
                description: "Read a bounded line range from a manuscript chapter body.",
                input_schema: r#"{"type":"object","properties":{"work_id":{"type":"string"},"chapter":{"type":"integer","minimum":1},"volume":{"type":"integer","minimum":1},"start_line":{"type":"integer","minimum":1},"end_line":{"type":"integer","minimum":1}},"required":["work_id","chapter"]}"#,
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
        },
        HostToolMetadata {
            id: HostToolId::ManuscriptWrite,
            access: Access::Write,
            admission: ADMISSION_WRITE_WORKSPACE,
            catalog: CatalogDescriptor {
                description: "Write manuscript body content for a chapter within the size quota.",
                input_schema: r#"{"type":"object","properties":{"work_id":{"type":"string"},"chapter":{"type":"integer","minimum":1},"volume":{"type":"integer","minimum":1},"content":{"type":"string"}},"required":["work_id","chapter","content"]}"#,
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
        },
        HostToolMetadata {
            id: HostToolId::ManuscriptPhaseGet,
            access: Access::Read,
            admission: ADMISSION_READ_WORKSPACE,
            catalog: CatalogDescriptor {
                description: "Return the current manuscript phase and stage status for a work.",
                input_schema: r#"{"type":"object","properties":{"work_id":{"type":"string"}},"required":["work_id"]}"#,
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
        },
        HostToolMetadata {
            id: HostToolId::ManuscriptPhaseSet,
            access: Access::Write,
            admission: ADMISSION_WRITE_WORKSPACE,
            catalog: CatalogDescriptor {
                description: "Move a work forward to the next manuscript phase.",
                input_schema: r#"{"type":"object","properties":{"work_id":{"type":"string"},"phase":{"type":"string","enum":["brainstorm","draft","review","finalize"]},"force":{"type":"boolean"}},"required":["work_id","phase"]}"#,
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
        },
        HostToolMetadata {
            id: HostToolId::WorkspacePaths,
            access: Access::Read,
            admission: ADMISSION_READ_CONTEXT,
            catalog: CatalogDescriptor {
                description: "Return the workspace root and the allowed roots (Works, Worlds, References, .nexus42).",
                input_schema: r#"{"type":"object","properties":{}}"#,
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
        },
        HostToolMetadata {
            id: HostToolId::RuntimeHealth,
            access: Access::Read,
            admission: ADMISSION_READ_CONTEXT,
            catalog: CatalogDescriptor {
                description: "Return agent-visible runtime health: mode, registry reachability, sync context, cloud flag.",
                input_schema: r#"{"type":"object","properties":{}}"#,
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
        },
        HostToolMetadata {
            id: HostToolId::TraceCorrelation,
            access: Access::Read,
            admission: ADMISSION_READ_CONTEXT,
            catalog: CatalogDescriptor {
                description: "Propagate a correlation id (and optional session id) across tool calls.",
                input_schema: r#"{"type":"object","properties":{"correlation_id":{"type":"string"},"session_id":{"type":"string"}}}"#,
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
        },
        HostToolMetadata {
            id: HostToolId::FsReadTextFile,
            access: Access::Read,
            admission: ADMISSION_FS_READ,
            catalog: CatalogDescriptor {
                description: "Read a text file within the workspace root and return its content.",
                input_schema: r#"{"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}"#,
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
        },
        HostToolMetadata {
            id: HostToolId::FsWriteTextFile,
            access: Access::Write,
            admission: ADMISSION_FS_WRITE,
            catalog: CatalogDescriptor {
                description: "Write text content to a file within the workspace root.",
                input_schema: r#"{"type":"object","properties":{"path":{"type":"string"},"content":{"type":"string"}},"required":["path","content"]}"#,
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
        },
    ]
});

/// note: the full canonical host-tool catalog.
///
/// Complete and nonempty in every build: reservation uses this set, never the
/// subset a particular graph binds.
#[must_use]
pub fn host_tool_catalog() -> &'static [HostToolMetadata] {
    HOST_TOOL_CATALOG.as_slice()
}

/// note: the stable ids the host namespace reserves, including rows this build does
/// not bind.
pub fn reserved_host_tool_ids() -> impl Iterator<Item = &'static str> {
    host_tool_catalog().iter().map(|row| row.id.as_str())
}

/// note: look up one canonical row by its wire id.
#[must_use]
pub fn lookup_host_tool(id: &str) -> Option<&'static HostToolMetadata> {
    host_tool_catalog().iter().find(|row| row.id.as_str() == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// note: the Generate tool's rendered contract adds ONLY its address field — the
    /// committed wire schema keeps owning the rest of the `required` list, in
    /// its own order. Replacing the array (the pre-fix shape) fails here the
    /// moment the wire schema gains or reorders a required entry.
    #[test]
    fn generate_schema_prepends_strategy_id_to_the_wire_required_list() {
        let wire: serde_json::Value = serde_json::from_str(include_str!(
            "../../../schemas/daemon-api/orchestration/strategy-generate-request.schema.json"
        ))
        .expect("the generate request schema is committed JSON");
        let rendered: serde_json::Value =
            serde_json::from_str(tool_input_schema()).expect("the rendered schema is JSON");

        let mut expected = wire["required"]
            .as_array()
            .expect("the wire schema declares required")
            .clone();
        assert!(
            !expected.contains(&serde_json::Value::String("strategy_id".to_owned())),
            "the wire schema carries no strategy_id; the tool adds it"
        );
        expected.insert(0, serde_json::Value::String("strategy_id".to_owned()));

        assert_eq!(rendered["required"], serde_json::Value::Array(expected));
    }

    /// note: the canonical-input requirement is a type, not a convention — every row
    /// authors one, so no placeholder path exists.
    #[test]
    fn every_canonical_row_authors_an_input_schema() {
        for row in host_tool_catalog() {
            assert!(
                !row.catalog.input_schema.is_empty(),
                "canonical tool '{}' must author its input schema",
                row.id.as_str()
            );
            let parsed: serde_json::Value = serde_json::from_str(row.catalog.input_schema)
                .unwrap_or_else(|e| {
                    panic!(
                        "canonical tool '{}' has an unparseable input schema: {e}",
                        row.id.as_str()
                    )
                });
            assert_eq!(
                parsed["type"],
                "object",
                "canonical tool '{}' must declare a root object schema",
                row.id.as_str()
            );
        }
    }
}
