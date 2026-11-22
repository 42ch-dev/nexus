//! Schema Drift Detection (`WS-D`)
//!
//! see crates/nexus-contracts/AGENTS.md
//!
//! Detects drift between JSON Schema wire contracts in `schemas/` and their
//! corresponding Rust struct definitions in `crates/nexus-contracts/src/`.
//!
//! ## How it works
//!
//! For each registered schema → Rust struct pair:
//! 1. Parse the JSON Schema to extract property names and types
//! 2. Build a JSON test payload with dummy values derived from the schema
//! 3. Deserialize the payload into the Rust struct (verifies structural compatibility)
//! 4. Serialize back to JSON and compare field names against schema properties
//!
//! ## Adding a new schema
//!
//! 1. Create the `.schema.json` file under `schemas/`
//! 2. Run `pnpm run codegen` to generate Rust and TypeScript types
//! 3. Add a new entry to `build_schema_map()` in this file
//! 4. For local-only types (subset of wire), use `CheckMode::Subset`; for wire types, use `Strict`
//!
//! ## Modes
//!
//! - `Strict`: All schema fields must exist in Rust, and all Rust fields must exist in schema
//! - `Subset`: All required schema fields must exist in Rust; Rust may have extra internal fields

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use nexus_contracts::*;
// why: typify barrel only re-exports the root type for single-word-title
// schemas; these definition types live inside the module but are not
// glob-re-exported.
use nexus_contracts::generated::platform::http_bff::context_assembly_v1::{
    ContextAssembleRequestV1, ContextAssembleResponseV1,
};

#[derive(Debug, Clone, Copy, PartialEq)]
enum CheckMode {
    /// note: all schema fields must exist in Rust (bidirectional exact match)
    Strict,
    /// note: required schema fields must exist in Rust; Rust may have extra
    /// fields
    #[allow(dead_code)]
    Subset,
}

/// note: a function pointer that deserializes a JSON Value into a concrete Rust
/// type, serializes it back, and returns `(type_name, serialized_value)`.
type CheckFn = Box<dyn Fn(Value) -> Result<(String, Value), String>>;

struct SchemaEntry {
    /// note: path to the JSON Schema file, relative to repo root
    schema_path: &'static str,
    /// note: validation mode (Strict or Subset)
    mode: CheckMode,
    /// note: one checker per Rust struct type generated from this schema
    checkers: Vec<CheckFn>,
}

/// note: build a checker for a single Rust struct type.
macro_rules! make_checker {
    ($t:ty) => {{
        let type_name: &'static str = stringify!($t);
        Box::new(move |json_val: Value| -> Result<(String, Value), String> {
            let v: $t =
                serde_json::from_value(json_val).map_err(|e| format!("{type_name}: {e}"))?;
            let serialized =
                serde_json::to_value(&v).map_err(|e| format!("{type_name} serialize: {e}"))?;
            Ok((type_name.to_string(), serialized))
        }) as CheckFn
    }};
}

/// note: build a `SchemaEntry` from either a single type or a list of types in
/// brackets.
macro_rules! entry {
    ($path:expr, $mode:ident, [$($t:ty),+ $(,)?]) => {
        SchemaEntry {
            schema_path: $path,
            mode: CheckMode::$mode,
            checkers: vec![$(make_checker!($t)),+],
        }
    };
    ($path:expr, $mode:ident, $t:ty) => {
        SchemaEntry {
            schema_path: $path,
            mode: CheckMode::$mode,
            checkers: vec![make_checker!($t)],
        }
    };
}

/// note: build the complete inventory of schema → Rust struct mappings.
///
/// This is the single source of truth for what gets checked by drift detection.
/// Add new entries here when you add new schemas or structs.
#[allow(clippy::too_many_lines)]
fn build_schema_map() -> Vec<SchemaEntry> {
    vec![
        entry!("schemas/domain/world.schema.json", Strict, World),
        entry!("schemas/domain/memory.schema.json", Strict, Memory),
        entry!("schemas/domain/creator.schema.json", Strict, Creator),
        entry!("schemas/domain/fork-branch.schema.json", Strict, ForkBranch),
        // note: `key-block.schema.json` was deleted — KnowledgeEntry is now the
        // atomic KB type, sourced from spoke. No local drift entry; spoke
        // version conformance is checked by check-wire-drift.sh.
        entry!("schemas/domain/pairing.schema.json", Strict, Pairing),
        entry!(
            "schemas/domain/story-manifest.schema.json",
            Strict,
            StoryManifest
        ),
        entry!(
            "schemas/domain/timeline-event.schema.json",
            Strict,
            TimelineEvent
        ),
        entry!("schemas/domain/user.schema.json", Strict, User),
        entry!(
            "schemas/domain/world-membership.schema.json",
            Strict,
            WorldMembership
        ),
        entry!("schemas/domain/actor-ref.schema.json", Strict, ActorRef),
        entry!("schemas/domain/character.schema.json", Strict, Character),
        entry!(
            "schemas/domain/actor-world-binding.schema.json",
            Strict,
            ActorWorldBinding
        ),
        entry!(
            "schemas/core/findings/findings-prune-response.schema.json",
            Strict,
            FindingsPruneResponse
        ),
        entry!(
            "schemas/core/narrative/narrative-world-response.schema.json",
            Strict,
            NarrativeWorldResponse
        ),
        entry!(
            "schemas/core/narrative/narrative-world-state.schema.json",
            Strict,
            NarrativeWorldState
        ),
        entry!(
            "schemas/core/narrative/narrative-worlds-list-response.schema.json",
            Strict,
            NarrativeWorldsListResponse
        ),
        entry!(
            "schemas/core/orchestration-presets/orchestration-preset-list-response.schema.json",
            Strict,
            OrchestrationPresetListResponse
        ),
        entry!(
            "schemas/core/orchestration-presets/preset-profile-conditional-rule.schema.json",
            Strict,
            PresetProfileConditionalRule
        ),
        entry!(
            "schemas/core/orchestration-presets/preset-profile-enter-action.schema.json",
            Strict,
            PresetProfileEnterAction
        ),
        entry!(
            "schemas/core/orchestration-presets/preset-profile-exit-when.schema.json",
            Strict,
            PresetProfileExitWhen
        ),
        entry!(
            "schemas/core/orchestration-presets/preset-profile-labeled-next.schema.json",
            Strict,
            PresetProfileLabeledNext
        ),
        entry!(
            "schemas/core/orchestration-presets/preset-profile-lanes.schema.json",
            Strict,
            PresetProfileLanes
        ),
        entry!(
            "schemas/core/orchestration-presets/preset-profile-next.schema.json",
            Strict,
            PresetProfileNext
        ),
        entry!(
            "schemas/core/orchestration-presets/preset-profile-response.schema.json",
            Strict,
            PresetProfileResponse
        ),
        entry!(
            "schemas/core/orchestration-presets/preset-profile-role.schema.json",
            Strict,
            PresetProfileRole
        ),
        entry!(
            "schemas/core/orchestration-presets/preset-profile-signal.schema.json",
            Strict,
            PresetProfileSignal
        ),
        entry!(
            "schemas/core/orchestration-presets/preset-profile-state.schema.json",
            Strict,
            PresetProfileState
        ),
        entry!(
            "schemas/core/works/work-inspiration-add-request.schema.json",
            Strict,
            WorkInspirationAddRequest
        ),
        entry!(
            "schemas/core/works/work-inspiration-add-response.schema.json",
            Strict,
            WorkInspirationAddResponse
        ),
        entry!(
            "schemas/core/works/work-inspiration-archive-request.schema.json",
            Strict,
            WorkInspirationArchiveRequest
        ),
        entry!(
            "schemas/core/works/work-inspiration-item.schema.json",
            Strict,
            WorkInspirationItem
        ),
        entry!(
            "schemas/core/works/work-inspiration-list-query.schema.json",
            Strict,
            WorkInspirationListQuery
        ),
        entry!(
            "schemas/core/works/work-inspiration-list-response.schema.json",
            Strict,
            WorkInspirationListResponse
        ),
        entry!(
            "schemas/core/works/work-inspiration-promote-request.schema.json",
            Strict,
            WorkInspirationPromoteRequest
        ),
        entry!(
            "schemas/core/works/work-inspiration-promote-response.schema.json",
            Strict,
            WorkInspirationPromoteResponse
        ),
        entry!(
            "schemas/core/works/work-pool-archive-request.schema.json",
            Strict,
            WorkPoolArchiveRequest
        ),
        entry!(
            "schemas/core/works/work-pool-entry.schema.json",
            Strict,
            WorkPoolEntry
        ),
        entry!(
            "schemas/core/works/work-pool-list-query.schema.json",
            Strict,
            WorkPoolListQuery
        ),
        entry!(
            "schemas/core/works/work-pool-list-response.schema.json",
            Strict,
            WorkPoolListResponse
        ),
        entry!(
            "schemas/core/works/work-pool-promote-request.schema.json",
            Strict,
            WorkPoolPromoteRequest
        ),
        entry!(
            "schemas/core/works/work-pool-set-active-request.schema.json",
            Strict,
            WorkPoolSetActiveRequest
        ),
        entry!(
            "schemas/core/works/work-reconcile-report.schema.json",
            Strict,
            WorkReconcileReport
        ),
        // note: the tools-api DTO replaces the handwritten
        // `ToolExecuteRequest`/`ToolExecuteResponse` pair.
        entry!("schemas/core/tools-api.schema.json", Strict, ToolsApi),
        // note: Connect replay/gap + capability negotiation and the durable
        // operation receipt. These four DTOs are the wire contracts
        // the peer-tools event lane serves (subscribe request/response and the
        // cursorless gap event) plus the receipt `CoreOperationReceipt`.
        // Registered Strict like every other core DTO so a field rename on
        // either side fails the drift gate.
        entry!(
            "schemas/core/core-connect-subscribe-request.schema.json",
            Strict,
            CoreConnectSubscribeRequest
        ),
        entry!(
            "schemas/core/core-connect-subscribe-response.schema.json",
            Strict,
            CoreConnectSubscribeResponse
        ),
        entry!(
            "schemas/core/core-connect-gap-event.schema.json",
            Strict,
            CoreConnectGapEvent
        ),
        entry!(
            "schemas/core/core-operation-receipt.schema.json",
            Strict,
            CoreOperationReceipt
        ),
        // why: `schemas/core/findings-api.schema.json` is NOT
        // registered here on purpose: it is an `allOf` alias to the
        // daemon-api update-finding-request definition (single tri-state
        // authority), and `extract_properties` does not follow `allOf` (same
        // reason `core/chapter-content-query` is absent above). Its shape is
        // covered by that definition's own entry below, plus the generated
        // `FindingsApi` parity pinned in `tri_state_presence.rs`.
        entry!("schemas/common/version-ref.schema.json", Strict, VersionRef),
        // note: SourceAnchor is generated by typify from source-anchor.schema.json
        // (un-skipped alongside common.schema.json).
        entry!(
            "schemas/common/source-anchor.schema.json",
            Strict,
            SourceAnchor
        ),
        // why: bundle is generated ONLY from platform/sync/bundle.schema.json —
        // platform/sync/bundle-refinement.schema.json is allOf-only and skipped
        // by codegen per SKIP_STRUCT_GENERATION_REL_PATHS.
        entry!("schemas/platform/sync/bundle.schema.json", Strict, Bundle),
        entry!("schemas/platform/sync/delta.schema.json", Strict, Delta),
        entry!(
            "schemas/platform/sync/sync-command.schema.json",
            Strict,
            SyncCommand
        ),
        entry!(
            "schemas/platform/sync/conflict-response.schema.json",
            Strict,
            ConflictResponse
        ),
        entry!(
            "schemas/platform/sync/sync-pull-request.schema.json",
            Strict,
            SyncPullRequest
        ),
        entry!(
            "schemas/platform/sync/sync-pull-response.schema.json",
            Strict,
            SyncPullResponse
        ),
        // note: canonical Daemon API error detail (the `error` object inside the
        // daemon's `{ success, false, error: ErrorResponse }` wire body).
        entry!(
            "schemas/daemon-api/common/error-response.schema.json",
            Strict,
            ErrorResponse
        ),
        entry!(
            "schemas/daemon-api/runtime/cert-fingerprint-response.schema.json",
            Strict,
            CertFingerprintResponse
        ),
        // note: CheckRequest is a plain object → Strict. CheckResponse is a
        // oneOf-root schema (mirrors spoke CheckResponse) with no top-level
        // `properties`.
        // The drift loop checks each non-empty root oneOf arm and skips empty
        // arms (no field comparison possible).
        entry!(
            "schemas/daemon-api/check/check-request.schema.json",
            Strict,
            CheckRequest
        ),
        entry!(
            "schemas/daemon-api/check/check-response.schema.json",
            Strict,
            CheckResponse
        ),
        // note: all three are plain-object root schemas → Strict. The response
        // packet mirrors build_inspector_packet 1:1 (AC-I3: moment_directive
        // is status-only, no body field by construction).
        entry!(
            "schemas/daemon-api/inspector/moment-inspect-request.schema.json",
            Strict,
            MomentInspectRequest
        ),
        entry!(
            "schemas/daemon-api/inspector/moment-inspect-response.schema.json",
            Strict,
            MomentInspectResponse
        ),
        entry!(
            "schemas/daemon-api/inspector/moment-directive-request.schema.json",
            Strict,
            MomentDirectiveRequest
        ),
        // note: moment-directive RESPONSE schema. oneOf-root (directive row |
        // empty `{}`) with no top-level `properties`; non-empty arms are
        // field-checked, empty arms skipped. Wire exactness is also enforced by
        // nexus-core/tests/context_services.rs
        // (`retained_moment_directive_set_show_clear_and_scope_ownership`,
        // migrated from the deleted nexus-daemon-runtime directive_api.rs
        // fixture in v1.193 P2-T5).
        entry!(
            "schemas/daemon-api/inspector/moment-directive-response.schema.json",
            Strict,
            MomentDirectiveResponse
        ),
        // note: WASM compute ABI envelopes (compass Q3/Q8). Only the top-level
        // struct of each schema is registered; inline/definition structs
        // (ComputeOutputStateDelta) are emitted by codegen but validated
        // indirectly via their parent schema.
        // note: entity-attributes/entity-state schemas were deleted;
        // per-module shapes now live in modules/<id>/manifest.json.
        entry!(
            "schemas/daemon-api/compute/compute-input.schema.json",
            Strict,
            ComputeInput
        ),
        entry!(
            "schemas/daemon-api/compute/compute-output.schema.json",
            Strict,
            ComputeOutput
        ),
        entry!(
            "schemas/daemon-api/compute/module-summary.schema.json",
            Strict,
            ModuleSummary
        ),
        entry!(
            "schemas/daemon-api/compute/module-detail.schema.json",
            Strict,
            ModuleDetail
        ),
        entry!(
            "schemas/daemon-api/compute/list-modules-response.schema.json",
            Strict,
            ListModulesResponse
        ),
        // note: the query/response shapes the native/service Compute facade
        // transports, promoted from the app-local declarations.
        entry!(
            "schemas/daemon-api/compute/list-runs-query.schema.json",
            Strict,
            ListRunsQuery
        ),
        entry!(
            "schemas/daemon-api/compute/discard-run-response.schema.json",
            Strict,
            DiscardRunResponse
        ),
        entry!(
            "schemas/daemon-api/compute/clear-runs-query.schema.json",
            Strict,
            ClearRunsQuery
        ),
        entry!(
            "schemas/daemon-api/compute/clear-runs-response.schema.json",
            Strict,
            ClearRunsResponse
        ),
        entry!(
            "schemas/daemon-api/works/create-work-request.schema.json",
            Strict,
            CreateWorkRequest
        ),
        entry!(
            "schemas/daemon-api/works/create-work-response.schema.json",
            Strict,
            CreateWorkResponse
        ),
        entry!(
            "schemas/daemon-api/works/list-works-query.schema.json",
            Strict,
            ListWorksQuery
        ),
        entry!(
            "schemas/daemon-api/works/work-summary.schema.json",
            Strict,
            WorkSummary
        ),
        entry!(
            "schemas/daemon-api/works/list-works-response.schema.json",
            Strict,
            ListWorksResponse
        ),
        entry!(
            "schemas/daemon-api/works/work-detail-response.schema.json",
            Strict,
            WorkDetailResponse
        ),
        entry!(
            "schemas/daemon-api/works/patch-work-request.schema.json",
            Strict,
            PatchWorkRequest
        ),
        entry!(
            "schemas/daemon-api/works/append-inspiration-request.schema.json",
            Strict,
            AppendInspirationRequest
        ),
        entry!(
            "schemas/daemon-api/works/append-inspiration-response.schema.json",
            Strict,
            AppendInspirationResponse
        ),
        entry!(
            "schemas/daemon-api/works/release-completion-lock-request.schema.json",
            Strict,
            ReleaseCompletionLockRequest
        ),
        entry!(
            "schemas/daemon-api/works/chapters/chapter-status.schema.json",
            Strict,
            ChapterStatus
        ),
        entry!(
            "schemas/daemon-api/works/chapters/chapter-protection.schema.json",
            Strict,
            ChapterProtection
        ),
        entry!(
            "schemas/daemon-api/works/chapters/chapter-summary.schema.json",
            Strict,
            ChapterSummary
        ),
        entry!(
            "schemas/daemon-api/works/chapters/chapter-detail.schema.json",
            Strict,
            ChapterDetail
        ),
        entry!(
            "schemas/daemon-api/works/chapters/chapter-outline.schema.json",
            Strict,
            ChapterOutline
        ),
        entry!(
            "schemas/daemon-api/works/chapters/patch-chapter-request.schema.json",
            Strict,
            PatchChapterRequest
        ),
        entry!(
            "schemas/daemon-api/works/chapters/chapter-body.schema.json",
            Strict,
            ChapterBody
        ),
        entry!(
            "schemas/daemon-api/works/chapters/list-chapters-query.schema.json",
            Strict,
            ListChaptersQuery
        ),
        entry!(
            "schemas/daemon-api/works/chapters/chapter-content-query.schema.json",
            Strict,
            ChapterContentQuery
        ),
        entry!(
            "schemas/daemon-api/works/chapters/list-chapters-response.schema.json",
            Strict,
            ListChaptersResponse
        ),
        entry!(
            "schemas/daemon-api/kb/pagination-info.schema.json",
            Strict,
            PaginationInfo
        ),
        entry!(
            "schemas/daemon-api/findings/create-finding-request.schema.json",
            Strict,
            CreateFindingRequest
        ),
        entry!(
            "schemas/daemon-api/findings/finding-detail-response.schema.json",
            Strict,
            FindingDetailResponse
        ),
        entry!(
            "schemas/daemon-api/findings/update-finding-request.schema.json",
            Strict,
            UpdateFindingRequest
        ),
        entry!(
            "schemas/daemon-api/findings/list-findings-query.schema.json",
            Strict,
            ListFindingsQuery
        ),
        // note: cursor-paginated findings list response.
        entry!(
            "schemas/daemon-api/findings/list-findings-response.schema.json",
            Strict,
            ListFindingsResponse
        ),
        entry!(
            "schemas/daemon-api/findings/stale-findings-response.schema.json",
            Strict,
            StaleFindingsResponse
        ),
        entry!(
            "schemas/daemon-api/findings/batch-update-findings-request.schema.json",
            Strict,
            BatchUpdateFindingsRequest
        ),
        entry!(
            "schemas/daemon-api/findings/batch-update-findings-response.schema.json",
            Strict,
            BatchUpdateFindingsResponse
        ),
        // note: extracted inline patch from batch-update-findings.
        entry!(
            "schemas/daemon-api/findings/finding-batch-patch.schema.json",
            Strict,
            FindingBatchPatch
        ),
        entry!(
            "schemas/daemon-api/memory/create-pending-review-request.schema.json",
            Strict,
            CreatePendingReviewRequest
        ),
        entry!(
            "schemas/daemon-api/memory/create-pending-review-response.schema.json",
            Strict,
            CreatePendingReviewResponse
        ),
        entry!(
            "schemas/daemon-api/memory/pending-review-info.schema.json",
            Strict,
            PendingReviewInfo
        ),
        entry!(
            "schemas/daemon-api/memory/list-pending-reviews-query.schema.json",
            Strict,
            ListPendingReviewsQuery
        ),
        entry!(
            "schemas/daemon-api/memory/list-pending-reviews-response.schema.json",
            Strict,
            ListPendingReviewsResponse
        ),
        entry!(
            "schemas/daemon-api/memory/count-pending-reviews-query.schema.json",
            Strict,
            CountPendingReviewsQuery
        ),
        entry!(
            "schemas/daemon-api/memory/count-pending-reviews-response.schema.json",
            Strict,
            CountPendingReviewsResponse
        ),
        entry!(
            "schemas/daemon-api/memory/delete-pending-review-query.schema.json",
            Strict,
            DeletePendingReviewQuery
        ),
        entry!(
            "schemas/daemon-api/memory/delete-pending-review-response.schema.json",
            Strict,
            DeletePendingReviewResponse
        ),
        entry!(
            "schemas/daemon-api/memory/review-request.schema.json",
            Strict,
            ReviewRequest
        ),
        entry!(
            "schemas/daemon-api/memory/review-response.schema.json",
            Strict,
            ReviewResponse
        ),
        entry!(
            "schemas/daemon-api/memory/memory-fragment-info.schema.json",
            Strict,
            MemoryFragmentInfo
        ),
        entry!(
            "schemas/daemon-api/memory/list-memory-fragments-query.schema.json",
            Strict,
            ListMemoryFragmentsQuery
        ),
        entry!(
            "schemas/daemon-api/memory/list-memory-fragments-response.schema.json",
            Strict,
            ListMemoryFragmentsResponse
        ),
        entry!(
            "schemas/daemon-api/memory/soul-narrative-request.schema.json",
            Strict,
            SoulNarrativeRequest
        ),
        entry!(
            "schemas/daemon-api/memory/soul-narrative-response.schema.json",
            Strict,
            SoulNarrativeResponse
        ),
        entry!(
            "schemas/daemon-api/reading/reading-progress-query.schema.json",
            Strict,
            ReadingProgressQuery
        ),
        entry!(
            "schemas/daemon-api/reading/reading-progress-request.schema.json",
            Strict,
            ReadingProgressRequest
        ),
        entry!(
            "schemas/daemon-api/reading/reading-progress-response.schema.json",
            Strict,
            ReadingProgressResponse
        ),
        entry!(
            "schemas/daemon-api/reading/reading-annotation.schema.json",
            Strict,
            ReadingAnnotation
        ),
        entry!(
            "schemas/daemon-api/reading/reading-annotation-create-request.schema.json",
            Strict,
            ReadingAnnotationCreateRequest
        ),
        entry!(
            "schemas/daemon-api/reading/reading-annotation-patch-request.schema.json",
            Strict,
            ReadingAnnotationPatchRequest
        ),
        entry!(
            "schemas/daemon-api/reading/reading-annotation-list-query.schema.json",
            Strict,
            ReadingAnnotationListQuery
        ),
        entry!(
            "schemas/daemon-api/reading/reading-annotation-list-response.schema.json",
            Strict,
            ReadingAnnotationListResponse
        ),
        entry!(
            "schemas/daemon-api/schedule/add-schedule-request.schema.json",
            Strict,
            AddScheduleRequest
        ),
        entry!(
            "schemas/daemon-api/schedule/add-schedule-response.schema.json",
            Strict,
            AddScheduleResponse
        ),
        entry!(
            "schemas/daemon-api/schedule/list-schedules-query.schema.json",
            Strict,
            ListSchedulesQuery
        ),
        entry!(
            "schemas/daemon-api/schedule/schedule-summary.schema.json",
            Strict,
            ScheduleSummary
        ),
        entry!(
            "schemas/daemon-api/schedule/list-schedules-response.schema.json",
            Strict,
            ListSchedulesResponse
        ),
        entry!(
            "schemas/daemon-api/schedule/inspect-schedule-response.schema.json",
            Strict,
            InspectScheduleResponse
        ),
        entry!(
            "schemas/daemon-api/schedule/edit-core-context-request.schema.json",
            Strict,
            EditCoreContextRequest
        ),
        entry!(
            "schemas/daemon-api/schedule/edit-core-context-response.schema.json",
            Strict,
            EditCoreContextResponse
        ),
        entry!(
            "schemas/daemon-api/schedule/core-context-response.schema.json",
            Strict,
            CoreContextResponse
        ),
        entry!(
            "schemas/daemon-api/schedule/core-context-history-entry.schema.json",
            Strict,
            CoreContextHistoryEntry
        ),
        entry!(
            "schemas/daemon-api/schedule/core-context-history-response.schema.json",
            Strict,
            CoreContextHistoryResponse
        ),
        entry!(
            "schemas/daemon-api/schedule/signal-schedule-request.schema.json",
            Strict,
            SignalScheduleRequest
        ),
        entry!(
            "schemas/daemon-api/schedule/signal-schedule-response.schema.json",
            Strict,
            SignalScheduleResponse
        ),
        entry!(
            "schemas/daemon-api/schedule/delete-schedule-response.schema.json",
            Strict,
            DeleteScheduleResponse
        ),
        entry!(
            "schemas/daemon-api/workspace/list-workspaces-query.schema.json",
            Strict,
            ListWorkspacesQuery
        ),
        entry!(
            "schemas/daemon-api/workspace/workspace-summary.schema.json",
            Strict,
            WorkspaceSummary
        ),
        entry!(
            "schemas/daemon-api/workspace/list-workspaces-response.schema.json",
            Strict,
            ListWorkspacesResponse
        ),
        entry!(
            "schemas/daemon-api/workspace/create-workspace-request.schema.json",
            Strict,
            CreateWorkspaceRequest
        ),
        entry!(
            "schemas/daemon-api/workspace/create-workspace-response.schema.json",
            Strict,
            CreateWorkspaceResponse
        ),
        entry!(
            "schemas/daemon-api/workspace/active-workspace-response.schema.json",
            Strict,
            ActiveWorkspaceResponse
        ),
        entry!(
            "schemas/daemon-api/workspace/set-active-workspace-request.schema.json",
            Strict,
            SetActiveWorkspaceRequest
        ),
        entry!(
            "schemas/daemon-api/workspace/set-active-workspace-response.schema.json",
            Strict,
            SetActiveWorkspaceResponse
        ),
        entry!(
            "schemas/daemon-api/creators/creator-info.schema.json",
            Strict,
            CreatorInfo
        ),
        entry!(
            "schemas/daemon-api/creators/list-creators-query.schema.json",
            Strict,
            ListCreatorsQuery
        ),
        entry!(
            "schemas/daemon-api/creators/list-creators-response.schema.json",
            Strict,
            ListCreatorsResponse
        ),
        entry!(
            "schemas/daemon-api/creators/creator-detail.schema.json",
            Strict,
            CreatorDetail
        ),
        entry!(
            "schemas/daemon-api/creators/set-active-creator-request.schema.json",
            Strict,
            SetActiveCreatorRequest
        ),
        entry!(
            "schemas/daemon-api/creators/active-creator-response.schema.json",
            Strict,
            ActiveCreatorResponse
        ),
        entry!(
            "schemas/daemon-api/creators/set-active-creator-response.schema.json",
            Strict,
            SetActiveCreatorResponse
        ),
        entry!(
            "schemas/daemon-api/creators/logout-response.schema.json",
            Strict,
            LogoutResponse
        ),
        entry!(
            "schemas/daemon-api/orchestration/sessions/session-summary.schema.json",
            Strict,
            SessionSummary
        ),
        entry!(
            "schemas/daemon-api/orchestration/sessions/list-sessions-query.schema.json",
            Strict,
            ListSessionsQuery
        ),
        entry!(
            "schemas/daemon-api/orchestration/sessions/list-sessions-response.schema.json",
            Strict,
            ListSessionsResponse
        ),
        entry!(
            "schemas/daemon-api/orchestration/sessions/session-detail-response.schema.json",
            Strict,
            SessionDetailResponse
        ),
        entry!(
            "schemas/daemon-api/orchestration/capabilities/capability-info.schema.json",
            Strict,
            CapabilityInfo
        ),
        entry!(
            "schemas/daemon-api/orchestration/capabilities/list-capabilities-response.schema.json",
            Strict,
            ListCapabilitiesResponse
        ),
        entry!(
            "schemas/daemon-api/orchestration/workflow-document.schema.json",
            Strict,
            WorkflowDocument
        ),
        entry!(
            "schemas/daemon-api/orchestration/simulation-trajectory.schema.json",
            Strict,
            SimulationTrajectory
        ),
        entry!(
            "schemas/daemon-api/orchestration/strategy-simulation-request.schema.json",
            Strict,
            StrategySimulationRequest
        ),
        entry!(
            "schemas/daemon-api/orchestration/strategy-compose-request.schema.json",
            Strict,
            StrategyComposeRequest
        ),
        entry!(
            "schemas/daemon-api/orchestration/strategy-compose-response.schema.json",
            Strict,
            StrategyComposeResponse
        ),
        entry!(
            "schemas/daemon-api/orchestration/approved-strategy-snapshot.schema.json",
            Strict,
            ApprovedStrategySnapshot
        ),
        entry!(
            "schemas/daemon-api/orchestration/strategy-approve-request.schema.json",
            Strict,
            StrategyApproveRequest
        ),
        entry!(
            "schemas/daemon-api/orchestration/strategy-apply-request.schema.json",
            Strict,
            StrategyApplyRequest
        ),
        entry!(
            "schemas/daemon-api/orchestration/strategy-applied-status.schema.json",
            Strict,
            StrategyAppliedStatus
        ),
        entry!(
            "schemas/daemon-api/orchestration/strategy-approvals.schema.json",
            Strict,
            StrategyApprovals
        ),
        entry!(
            "schemas/daemon-api/patterns/pattern-slot.schema.json",
            Strict,
            PatternSlot
        ),
        entry!(
            "schemas/daemon-api/patterns/pattern-manifest.schema.json",
            Strict,
            PatternManifest
        ),
        entry!(
            "schemas/daemon-api/patterns/pattern-summary.schema.json",
            Strict,
            PatternSummary
        ),
        entry!(
            "schemas/daemon-api/patterns/list-patterns-query.schema.json",
            Strict,
            ListPatternsQuery
        ),
        entry!(
            "schemas/daemon-api/patterns/list-patterns-response.schema.json",
            Strict,
            ListPatternsResponse
        ),
        entry!(
            "schemas/daemon-api/preset-management/preset-summary.schema.json",
            Strict,
            PresetSummary
        ),
        entry!(
            "schemas/daemon-api/preset-management/list-presets-response.schema.json",
            Strict,
            ListPresetsResponse
        ),
        entry!(
            "schemas/daemon-api/preset-management/scaffold-preset-request.schema.json",
            Strict,
            ScaffoldPresetRequest
        ),
        entry!(
            "schemas/daemon-api/preset-management/scaffold-preset-response.schema.json",
            Strict,
            ScaffoldPresetResponse
        ),
        entry!(
            "schemas/daemon-api/preset-management/validate-preset-request.schema.json",
            Strict,
            ValidatePresetRequest
        ),
        entry!(
            "schemas/daemon-api/preset-management/validate-preset-response.schema.json",
            Strict,
            ValidatePresetResponse
        ),
        entry!(
            "schemas/daemon-api/preset-management/reload-preset-response.schema.json",
            Strict,
            ReloadPresetResponse
        ),
        entry!(
            "schemas/daemon-api/preset-management/get-preset-response.schema.json",
            Strict,
            GetPresetResponse
        ),
        entry!(
            "schemas/daemon-api/preset-management/update-preset-request.schema.json",
            Strict,
            UpdatePresetRequest
        ),
        entry!(
            "schemas/daemon-api/preset-management/update-preset-response.schema.json",
            Strict,
            UpdatePresetResponse
        ),
        entry!(
            "schemas/daemon-api/canvas/strategy/strategy-patch-state-request.schema.json",
            Strict,
            StrategyPatchStateRequest
        ),
        entry!(
            "schemas/daemon-api/canvas/strategy/strategy-patch-transition-request.schema.json",
            Strict,
            StrategyPatchTransitionRequest
        ),
        entry!(
            "schemas/daemon-api/canvas/strategy/strategy-patch-prompt-template-request.schema.json",
            Strict,
            StrategyPatchPromptTemplateRequest
        ),
        entry!(
            "schemas/daemon-api/canvas/strategy/strategy-patch-response.schema.json",
            Strict,
            StrategyPatchResponse
        ),
        entry!(
            "schemas/daemon-api/canvas/strategy/strategy-conflict-error.schema.json",
            Strict,
            StrategyConflictError
        ),
        entry!(
            "schemas/daemon-api/canvas/outline/outline-patch-structure-request.schema.json",
            Strict,
            OutlinePatchStructureRequest
        ),
        entry!(
            "schemas/daemon-api/canvas/outline/outline-patch-chapter-request.schema.json",
            Strict,
            OutlinePatchChapterRequest
        ),
        entry!(
            "schemas/daemon-api/canvas/outline/timeline-patch-event-request.schema.json",
            Strict,
            TimelinePatchEventRequest
        ),
        entry!(
            "schemas/daemon-api/canvas/outline/outline-patch-response.schema.json",
            Strict,
            OutlinePatchResponse
        ),
        entry!(
            "schemas/daemon-api/canvas/outline/outline-conflict-error.schema.json",
            Strict,
            OutlineConflictError
        ),
        entry!(
            "schemas/daemon-api/canvas/outline/outline-validation-error.schema.json",
            Strict,
            OutlineValidationError
        ),
        entry!(
            "schemas/daemon-api/canvas/outline/work-outline.schema.json",
            Strict,
            WorkOutline
        ),
        entry!(
            "schemas/daemon-api/canvas/world-kb/world-kb-patch-entity-request.schema.json",
            Strict,
            WorldKbPatchEntityRequest
        ),
        entry!(
            "schemas/daemon-api/canvas/world-kb/world-kb-patch-entity-response.schema.json",
            Strict,
            WorldKbPatchEntityResponse
        ),
        entry!(
            "schemas/daemon-api/canvas/world-kb/world-kb-promote-candidate-request.schema.json",
            Strict,
            WorldKbPromoteCandidateRequest
        ),
        entry!(
            "schemas/daemon-api/canvas/world-kb/world-kb-promote-candidate-response.schema.json",
            Strict,
            WorldKbPromoteCandidateResponse
        ),
        entry!(
            "schemas/daemon-api/canvas/world-kb/world-kb-graph-response.schema.json",
            Strict,
            WorldKbGraphResponse
        ),
        entry!(
            "schemas/daemon-api/canvas/world-kb/world-kb-key-block-state-response.schema.json",
            Strict,
            WorldKbKeyBlockStateResponse
        ),
        entry!(
            "schemas/daemon-api/canvas/world-kb/world-kb-candidates-response.schema.json",
            Strict,
            WorldKbCandidatesResponse
        ),
        entry!(
            "schemas/daemon-api/canvas/world-kb/world-kb-conflict-error.schema.json",
            Strict,
            WorldKbConflictError
        ),
        entry!(
            "schemas/daemon-api/canvas/world-kb/world-kb-validation-error.schema.json",
            Strict,
            WorldKbValidationError
        ),
        entry!(
            "schemas/daemon-api/canvas/world-kb/world-kb-entity-patch.schema.json",
            Strict,
            WorldKbEntityPatch
        ),
        entry!(
            "schemas/daemon-api/canvas/world-kb/world-kb-entity-projection.schema.json",
            Strict,
            WorldKbEntityProjection
        ),
        entry!(
            "schemas/daemon-api/canvas/world-kb/world-kb-candidate-projection.schema.json",
            Strict,
            WorldKbCandidateProjection
        ),
        entry!(
            "schemas/daemon-api/canvas/world-kb/world-kb-source-anchor-projection.schema.json",
            Strict,
            WorldKbSourceAnchorProjection
        ),
        entry!(
            "schemas/daemon-api/canvas/world-kb/world-kb-extract-job-projection.schema.json",
            Strict,
            WorldKbExtractJobProjection
        ),
        entry!(
            "schemas/platform/http-bff/context-assembly-v1.schema.json",
            Strict,
            [ContextAssembleRequestV1, ContextAssembleResponseV1,]
        ),
        entry!(
            "schemas/platform/http-bff/creator-runtime-policy-response.schema.json",
            Strict,
            CreatorRuntimePolicyResponse
        ),
        entry!(
            "schemas/platform/http-bff/explore-ai-answer-request.schema.json",
            Strict,
            ExploreAiAnswerRequest
        ),
        entry!(
            "schemas/platform/http-bff/explore-ai-answer-response.schema.json",
            Strict,
            ExploreAiAnswerResponse
        ),
        entry!(
            "schemas/platform/http-bff/explore-ai-summary-request.schema.json",
            Strict,
            ExploreAiSummaryRequest
        ),
        entry!(
            "schemas/platform/http-bff/explore-ai-summary-response.schema.json",
            Strict,
            ExploreAiSummaryResponse
        ),
        entry!(
            "schemas/platform/http-bff/explore-browse-request.schema.json",
            Strict,
            ExploreBrowseRequest
        ),
        entry!(
            "schemas/platform/http-bff/explore-creator-card.schema.json",
            Strict,
            ExploreCreatorCard
        ),
        entry!(
            "schemas/platform/http-bff/explore-feed-response.schema.json",
            Strict,
            ExploreFeedResponse
        ),
        entry!(
            "schemas/platform/http-bff/explore-hit.schema.json",
            Strict,
            ExploreHit
        ),
        entry!(
            "schemas/platform/http-bff/explore-search-request.schema.json",
            Strict,
            ExploreSearchRequest
        ),
        entry!(
            "schemas/platform/http-bff/me-entitlements-response.schema.json",
            Strict,
            MeEntitlementsResponse
        ),
        entry!(
            "schemas/platform/http-bff/memory-web-list-request.schema.json",
            Strict,
            MemoryWebListRequest
        ),
        entry!(
            "schemas/platform/http-bff/memory-web-list-response.schema.json",
            Strict,
            MemoryWebListResponse
        ),
        entry!(
            "schemas/platform/http-bff/notifications-inbox-item.schema.json",
            Strict,
            NotificationsInboxItem
        ),
        entry!(
            "schemas/platform/http-bff/notifications-list-request.schema.json",
            Strict,
            NotificationsListRequest
        ),
        entry!(
            "schemas/platform/http-bff/notifications-list-response.schema.json",
            Strict,
            NotificationsListResponse
        ),
        entry!(
            "schemas/platform/http-bff/notifications-mark-read-request.schema.json",
            Strict,
            NotificationsMarkReadRequest
        ),
        entry!(
            "schemas/platform/http-bff/notifications-mark-read-response.schema.json",
            Strict,
            NotificationsMarkReadResponse
        ),
        entry!(
            "schemas/platform/http-bff/official-creator-quota-response.schema.json",
            Strict,
            OfficialCreatorQuotaResponse
        ),
        entry!(
            "schemas/platform/http-bff/publish-chapter-request.schema.json",
            Strict,
            PublishChapterRequest
        ),
        entry!(
            "schemas/platform/http-bff/publish-history-entry.schema.json",
            Strict,
            PublishHistoryEntry
        ),
        entry!(
            "schemas/platform/http-bff/publish-history-request.schema.json",
            Strict,
            PublishHistoryRequest
        ),
        entry!(
            "schemas/platform/http-bff/publish-history-response.schema.json",
            Strict,
            PublishHistoryResponse
        ),
        entry!(
            "schemas/platform/http-bff/publish-story-request.schema.json",
            Strict,
            PublishStoryRequest
        ),
        entry!(
            "schemas/platform/http-bff/publish-story-response.schema.json",
            Strict,
            PublishStoryResponse
        ),
        entry!(
            "schemas/platform/http-bff/social-graph-feed-request.schema.json",
            Strict,
            SocialGraphFeedRequest
        ),
        entry!(
            "schemas/platform/http-bff/social-graph-feed-response.schema.json",
            Strict,
            SocialGraphFeedResponse
        ),
        entry!(
            "schemas/platform/http-bff/social-graph-relationship-request.schema.json",
            Strict,
            SocialGraphRelationshipRequest
        ),
        entry!(
            "schemas/platform/http-bff/social-graph-relationship-response.schema.json",
            Strict,
            SocialGraphRelationshipResponse
        ),
        entry!(
            "schemas/platform/http-bff/world-fork-request.schema.json",
            Strict,
            WorldForkRequest
        ),
        entry!(
            "schemas/platform/http-bff/world-fork-response.schema.json",
            Strict,
            WorldForkResponse
        ),
        entry!(
            "schemas/platform/http-bff/world-snapshot-request.schema.json",
            Strict,
            WorldSnapshotRequest
        ),
        entry!(
            "schemas/platform/http-bff/world-snapshot-response.schema.json",
            Strict,
            WorldSnapshotResponse
        ),
        entry!(
            "schemas/daemon-api/timeline/timeline-overview-response.schema.json",
            Strict,
            TimelineOverviewResponse
        ),
        entry!(
            "schemas/daemon-api/timeline/timeline-event-info.schema.json",
            Strict,
            TimelineEventInfo
        ),
        entry!(
            "schemas/daemon-api/timeline/list-timeline-events-response.schema.json",
            Strict,
            ListTimelineEventsResponse
        ),
        // note: daemon HTTP surface for the `nexus.fork.create` capability:
        // POST /v1/daemon/worlds/:world_id/forks. Both are plain-object root
        // schemas → Strict.
        entry!(
            "schemas/daemon-api/worlds/create-fork-request.schema.json",
            Strict,
            CreateForkRequest
        ),
        entry!(
            "schemas/daemon-api/worlds/create-fork-response.schema.json",
            Strict,
            CreateForkResponse
        ),
        // note: world findings read surface DTO. Plain-object root
        // (`findings` + `truncated`) → Strict; the inline
        // item shape is validated indirectly via the parent schema (same
        // convention as the check-response findings item).
        entry!(
            "schemas/daemon-api/worlds/world-findings-list-response.schema.json",
            Strict,
            WorldFindingsListResponse
        ),
        // note: world rules read surface DTO. Plain-object root (`rules` +
        // `truncated`) → Strict; item mirrors
        // the spoke Rule author metadata + the first-class constraint
        // carrier (extensions bag not exposed).
        entry!(
            "schemas/daemon-api/worlds/world-rules-list-response.schema.json",
            Strict,
            WorldRulesListResponse
        ),
        // note: world rules write surface DTOs. Plain-object
        // roots → Strict. Type structure only (no minLength/enum/format):
        // value validation lives in the daemon handler so form-producible
        // failures surface through the field-level envelope, never
        // the axum Json extractor.
        entry!(
            "schemas/daemon-api/worlds/world-rule-create-request.schema.json",
            Strict,
            WorldRuleCreateRequest
        ),
        entry!(
            "schemas/daemon-api/worlds/world-rule-update-request.schema.json",
            Strict,
            WorldRuleUpdateRequest
        ),
        // note: single-item response schema serving both POST
        // 201 and PATCH 200 — the WorldRulesListResponseRulesItem shape
        // verbatim → Strict.
        entry!(
            "schemas/daemon-api/worlds/world-rule-response.schema.json",
            Strict,
            WorldRuleResponse
        ),
        entry!(
            "schemas/daemon-api/characters/create-character-request.schema.json",
            Strict,
            CreateCharacterRequest
        ),
        entry!(
            "schemas/daemon-api/characters/create-character-response.schema.json",
            Strict,
            CreateCharacterResponse
        ),
        entry!(
            "schemas/daemon-api/characters/character-detail.schema.json",
            Strict,
            CharacterDetail
        ),
        entry!(
            "schemas/daemon-api/characters/list-characters-query.schema.json",
            Strict,
            ListCharactersQuery
        ),
        entry!(
            "schemas/daemon-api/characters/list-characters-response.schema.json",
            Strict,
            ListCharactersResponse
        ),
        entry!(
            "schemas/daemon-api/characters/add-character-binding-request.schema.json",
            Strict,
            AddCharacterBindingRequest
        ),
        entry!(
            "schemas/daemon-api/characters/add-character-binding-response.schema.json",
            Strict,
            AddCharacterBindingResponse
        ),
        entry!(
            "schemas/daemon-api/characters/list-character-bindings-query.schema.json",
            Strict,
            ListCharacterBindingsQuery
        ),
        entry!(
            "schemas/daemon-api/characters/list-character-bindings-response.schema.json",
            Strict,
            ListCharacterBindingsResponse
        ),
        entry!(
            "schemas/daemon-api/characters/update-character-request.schema.json",
            Strict,
            UpdateCharacterRequest
        ),
        entry!(
            "schemas/daemon-api/characters/character-lifecycle-request.schema.json",
            Strict,
            CharacterLifecycleRequest
        ),
        entry!(
            "schemas/daemon-api/characters/memory/character-pending-review-info.schema.json",
            Strict,
            CharacterPendingReviewInfo
        ),
        entry!(
            "schemas/daemon-api/characters/character-binding-detail.schema.json",
            Strict,
            CharacterBindingDetail
        ),
        entry!(
            "schemas/daemon-api/characters/update-character-binding-request.schema.json",
            Strict,
            UpdateCharacterBindingRequest
        ),
        entry!(
            "schemas/domain/knowledge-owner-ref.schema.json",
            Strict,
            KnowledgeOwnerRef
        ),
        entry!(
            "schemas/daemon-api/actor-knowledge/add-knowledge-entry-request.schema.json",
            Strict,
            AddKnowledgeEntryRequest
        ),
        entry!(
            "schemas/daemon-api/actor-knowledge/add-knowledge-entry-response.schema.json",
            Strict,
            AddKnowledgeEntryResponse
        ),
        entry!(
            "schemas/daemon-api/actor-knowledge/knowledge-view-item.schema.json",
            Strict,
            KnowledgeViewItem
        ),
        entry!(
            "schemas/daemon-api/actor-knowledge/list-character-knowledge-query.schema.json",
            Strict,
            ListCharacterKnowledgeQuery
        ),
        entry!(
            "schemas/daemon-api/actor-knowledge/list-character-knowledge-response.schema.json",
            Strict,
            ListCharacterKnowledgeResponse
        ),
        entry!(
            "schemas/daemon-api/actor-knowledge/view-request.schema.json",
            Strict,
            ViewRequest
        ),
        entry!(
            "schemas/daemon-api/actor-knowledge/view-response.schema.json",
            Strict,
            ViewResponse
        ),
        entry!(
            "schemas/daemon-api/actor-knowledge/knowledge-entry-detail.schema.json",
            Strict,
            KnowledgeEntryDetail
        ),
        entry!(
            "schemas/daemon-api/actor-knowledge/update-knowledge-entry-request.schema.json",
            Strict,
            UpdateKnowledgeEntryRequest
        ),
        entry!(
            "schemas/daemon-api/actor-knowledge/delete-knowledge-entry-query.schema.json",
            Strict,
            DeleteKnowledgeEntryQuery
        ),
        entry!(
            "schemas/daemon-api/agent-host/agent-host-config.schema.json",
            Strict,
            AgentHostConfig
        ),
        entry!(
            "schemas/daemon-api/agent-host/agent-host-list-sessions-query.schema.json",
            Strict,
            AgentHostListSessionsQuery
        ),
        entry!(
            "schemas/daemon-api/agent-host/agent-scan-entry.schema.json",
            Strict,
            AgentScanEntry
        ),
        entry!(
            "schemas/daemon-api/agent-host/character-operation-result.schema.json",
            Strict,
            CharacterOperationResult
        ),
        entry!(
            "schemas/daemon-api/agent-host/character-run-capture-outcome.schema.json",
            Strict,
            CharacterRunCaptureOutcome
        ),
        entry!(
            "schemas/daemon-api/agent-host/cancel-operation-response.schema.json",
            Strict,
            CancelOperationResponse
        ),
        entry!(
            "schemas/daemon-api/agent-host/create-session-request.schema.json",
            Strict,
            CreateSessionRequest
        ),
        entry!(
            "schemas/daemon-api/agent-host/execute-operation-request.schema.json",
            Strict,
            ExecuteOperationRequest
        ),
        entry!(
            "schemas/daemon-api/agent-host/operation-response.schema.json",
            Strict,
            OperationResponse
        ),
        entry!(
            "schemas/daemon-api/agent-host/scan-request.schema.json",
            Strict,
            ScanRequest
        ),
        entry!(
            "schemas/daemon-api/agent-host/scan-response.schema.json",
            Strict,
            ScanResponse
        ),
        entry!(
            "schemas/daemon-api/agent-host/session-list-response.schema.json",
            Strict,
            SessionListResponse
        ),
        entry!(
            "schemas/daemon-api/agent-host/session-response.schema.json",
            Strict,
            SessionResponse
        ),
        entry!(
            "schemas/daemon-api/agent-host/session-viewpoint.schema.json",
            Strict,
            SessionViewpoint
        ),
        entry!(
            "schemas/daemon-api/agent-host/shutdown-session-response.schema.json",
            Strict,
            ShutdownSessionResponse
        ),
    ]
}

/// note: derive the complete set of schema file paths from two sources:
/// 1. All paths registered in `build_schema_map()` (the checked schemas)
/// 2. A deterministic glob over `schemas/**/*.schema.json` (catches schemas
///    without checkers, e.g., definitions-only or allOf-only schemas)
///
/// This replaces the former manually-maintained `ALL_SCHEMA_PATHS` list,
/// ensuring new schemas are automatically discovered.
///
/// Accepts a pre-built schema map to avoid redundant construction (each call
/// to `build_schema_map()` allocates ~51 boxed closures).
fn collect_all_schema_paths(entries: &[SchemaEntry]) -> BTreeSet<String> {
    let mut paths = BTreeSet::new();

    for entry in entries {
        paths.insert(entry.schema_path.to_string());
    }

    // why: glob discovery catches schemas with no registered checker.
    let root = workspace_root();
    let schemas_dir = root.join("schemas");
    collect_schema_files_recursive(&schemas_dir, &root, &mut paths);

    paths
}

/// note: recursively collect `.schema.json` files under a directory.
fn collect_schema_files_recursive(dir: &Path, root: &Path, paths: &mut BTreeSet<String>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_schema_files_recursive(&path, root, paths);
            } else if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.ends_with(".schema.json") {
                    if let Ok(relative) = path.strip_prefix(root) {
                        paths.insert(relative.to_string_lossy().to_string());
                    }
                }
            }
        }
    }
}

/// note: workspace root — traverse up from `CARGO_MANIFEST_DIR`
/// (`crates/nexus-contracts`) to the workspace root (2 levels up).
fn workspace_root() -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    dir.parent().unwrap().parent().unwrap().to_path_buf()
}

/// note: load a JSON file from a path relative to the workspace root.
fn load_json(relative_path: &str) -> Result<Value, String> {
    let full_path = workspace_root().join(relative_path);
    let content = std::fs::read_to_string(&full_path)
        .map_err(|e| format!("Cannot read {relative_path}: {e}"))?;
    serde_json::from_str(&content).map_err(|e| format!("Cannot parse {relative_path}: {e}"))
}

/// Build a cache of all schema files keyed by their relative path.
/// Also keyed by the `https://nexus42.invalid/...` URL for $ref resolution.
///
/// Accepts a pre-built schema map to avoid double construction of boxed checkers.
fn build_schema_cache(entries: &[SchemaEntry]) -> HashMap<String, Value> {
    let mut cache = HashMap::new();
    for path in collect_all_schema_paths(entries) {
        if let Ok(val) = load_json(&path) {
            cache.insert(path.clone(), val.clone());
            if let Some(id) = val.get("$id").and_then(|v| v.as_str()) {
                let clean_id = id.split('#').next().unwrap_or(id);
                cache.insert(clean_id.to_string(), val);
            }
        }
    }
    cache
}

/// note: extract property names and their schema definitions from a JSON
/// Schema object.
fn extract_properties(schema: &Value) -> Vec<(String, Value)> {
    let mut props = Vec::new();
    if let Some(properties) = schema.get("properties").and_then(|p| p.as_object()) {
        for (name, prop_def) in properties {
            props.push((name.clone(), prop_def.clone()));
        }
    }
    props
}

/// note: extract the set of required field names from a JSON Schema object.
fn extract_required(schema: &Value) -> BTreeSet<String> {
    let mut required = BTreeSet::new();
    if let Some(req) = schema.get("required").and_then(|r| r.as_array()) {
        for val in req {
            if let Some(name) = val.as_str() {
                required.insert(name.to_string());
            }
        }
    }
    required
}

/// note: describe a property's type for error messages.
fn describe_type(properties: &[(String, Value)], name: &str) -> String {
    properties.iter().find(|(n, _)| n == name).map_or_else(
        || "unknown".to_string(),
        |(_, def)| describe_schema_type(def),
    )
}

/// note: get a human-readable type description from a schema property
/// definition.
fn describe_schema_type(def: &Value) -> String {
    if let Some(ref_path) = def.get("$ref").and_then(|r| r.as_str()) {
        if let Some(fragment) = ref_path.split('#').nth(1) {
            if let Some(def_name) = fragment.rsplit('/').next() {
                return format!("ref({def_name})");
            }
            return format!("ref({ref_path})");
        }
    }
    if let Some(type_val) = def.get("type") {
        match type_val {
            Value::String(s) => return s.clone(),
            Value::Array(arr) => {
                let types: Vec<&str> = arr.iter().filter_map(|v| v.as_str()).collect();
                return types.join(" | ");
            }
            _ => {}
        }
    }
    if def.get("enum").is_some() {
        return "enum".to_string();
    }
    if def.get("properties").is_some() || def.get("additionalProperties").is_some() {
        return "object".to_string();
    }
    "unknown".to_string()
}

/// note: build a JSON test payload from schema properties using dummy values.
/// The returned JSON object has all schema properties filled with type-appropriate
/// dummy values, so it can be deserialized into the corresponding Rust struct.
fn build_test_json(
    properties: &[(String, Value)],
    schema_cache: &HashMap<String, Value>,
    current_schema_path: &str,
) -> Value {
    let mut map = serde_json::Map::new();
    let current_dir = Path::new(current_schema_path)
        .parent()
        .and_then(|p| p.to_str())
        .unwrap_or("");
    for (name, prop_def) in properties {
        let dummy = make_dummy_value(prop_def, schema_cache, current_dir, current_schema_path);
        map.insert(name.clone(), dummy);
    }
    Value::Object(map)
}

/// note: generate a dummy string value that satisfies typify-generated type
/// constraints.
///
/// typify enforces:
/// - `format: "date-time"` → `chrono::DateTime<Utc>` (needs valid RFC 3339 string)
/// - `format: "uuid"` → `::uuid::Uuid` (needs a valid UUID string)
/// - `pattern: "^prefix_[a-zA-Z0-9]+$"` → newtype with pattern-validated deserializer
///
/// For unconstrained strings, returns `"dummy"`.
fn make_dummy_string(prop_def: &Value) -> Value {
    if let Some(format) = prop_def.get("format").and_then(|f| f.as_str()) {
        if format == "date-time" {
            return Value::String("2024-01-01T00:00:00Z".to_string());
        }
        // note: `CoreConnectSubscribeResponse.epoch` is `format: "uuid"`,
        // which typify maps to `::uuid::Uuid`; the nil UUID round-trips it.
        if format == "uuid" {
            return Value::String("00000000-0000-0000-0000-000000000000".to_string());
        }
    }

    // note: Nexus patterns look like `^prefix_[a-zA-Z0-9]+$`; the literal
    // prefix plus a valid suffix satisfies them.
    if let Some(pattern) = prop_def.get("pattern").and_then(|p| p.as_str()) {
        // why: SemVer 2.0.0 (`pattern-manifest` `version`) is
        // alternation-heavy, so the generic prefix extraction below cannot
        // build a conforming value.
        if pattern.contains("(0|[1-9]") {
            return Value::String("1.0.0".to_string());
        }
        let inner = pattern.trim_start_matches('^').trim_end_matches('$');
        let first_alternative = inner
            .strip_suffix(')')
            .and_then(|inner| inner.split_once("(?:"))
            .and_then(|(prefix, group)| {
                group
                    .split_once('|')
                    .map(|(first, _)| format!("{prefix}{first}"))
            });
        let inner = first_alternative.as_deref().unwrap_or(inner);
        if !inner.is_empty()
            && !inner.chars().any(|c| {
                matches!(
                    c,
                    '[' | '\\' | '.' | '{' | '*' | '+' | '?' | '(' | ')' | '|'
                )
            })
        {
            return Value::String(inner.to_string());
        }
        let prefix = inner.split(['[', '\\', '.']).next().unwrap_or("");
        if !prefix.is_empty() {
            // note: exact-length hex ids (`chr_<32hex>`, `awb_<32hex>`) use
            // `{N}` after a character class; pad with `a` so dummy values
            // satisfy the pattern.
            if let Some(n) = inner
                .rsplit_once('{')
                .and_then(|(_, rest)| rest.split('}').next())
                .and_then(|digits| digits.parse::<usize>().ok())
            {
                return Value::String(format!("{prefix}{}", "a".repeat(n)));
            }
            // note: the prefix typically ends with `_` (e.g. `ctr_`); append
            // `0a` to satisfy `[a-zA-Z0-9]+` quantifiers that follow.
            return Value::String(format!("{prefix}0a"));
        }
    }

    Value::String("dummy".to_string())
}

/// note: generate a dummy JSON value for a schema property definition.
#[allow(clippy::too_many_lines)] // why: one recursive schema-shape dispatch table
fn make_dummy_value(
    prop_def: &Value,
    schema_cache: &HashMap<String, Value>,
    current_dir: &str,
    current_schema_path: &str,
) -> Value {
    // why: $ref must be first to resolve enum references.
    if let Some(ref_path) = prop_def.get("$ref").and_then(|r| r.as_str()) {
        return make_dummy_from_ref(ref_path, schema_cache, current_dir, current_schema_path);
    }

    if let Some(const_val) = prop_def.get("const") {
        return const_val.clone();
    }

    // why: inline enums must precede type-based dispatch to get the correct value.
    if let Some(enum_vals) = prop_def.get("enum").and_then(|e| e.as_array()) {
        if let Some(first) = enum_vals.first() {
            return first.clone();
        }
    }

    let type_val = prop_def.get("type");
    let type_str = match type_val {
        Some(Value::String(s)) => Some(s.as_str()),
        Some(Value::Array(arr)) => {
            arr.iter().filter_map(|v| v.as_str()).find(|s| *s != "null")
        }
        _ => None,
    };

    match type_str {
        Some("string") => make_dummy_string(prop_def),
        Some("integer") => {
            // why: typify maps `minimum: 1` to NonZeroU64; generate a value at
            // least 1 so deserialization succeeds for both u64 and NonZeroU64
            // fields.
            let min = prop_def
                .get("minimum")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0);
            Value::Number(serde_json::Number::from(min.max(1)))
        }
        Some("number") => Value::Number(serde_json::Number::from_f64(1.0).unwrap()),
        Some("boolean") => Value::Bool(false),
        Some("array") => {
            prop_def.get("items").map_or_else(
                || Value::Array(vec![]),
                |items| {
                    let item_val =
                        make_dummy_value(items, schema_cache, current_dir, current_schema_path);
                    Value::Array(vec![item_val])
                },
            )
        }
        Some("object") => {
            prop_def
                .get("properties")
                .and_then(|p| p.as_object())
                .map_or_else(
                    // why: a freeform object (no `properties`) that is a
                    // root/nested `oneOf` (e.g. the closed ActorRef /
                    // KnowledgeOwnerRef unions referenced from daemon-api
                    // schemas) takes its first arm; otherwise, if
                    // `additionalProperties` specifies a value schema, generate
                    // one matching entry so typify's typed maps (e.g.
                    // `Map<String, Map<…>>`) accept it.
                    || {
                        if let Some(first_arm) = prop_def
                            .get("oneOf")
                            .and_then(|a| a.as_array())
                            .and_then(|a| a.first())
                        {
                            return make_dummy_value(
                                first_arm,
                                schema_cache,
                                current_dir,
                                current_schema_path,
                            );
                        }
                        let val = prop_def.get("additionalProperties").map_or_else(
                            || Value::String("_".to_string()),
                            |ap| {
                                make_dummy_value(ap, schema_cache, current_dir, current_schema_path)
                            },
                        );
                        let mut m = serde_json::Map::new();
                        // note: `propertyNames.pattern` constrains the map
                        // keys — typify emits a pattern-validated key newtype
                        // (e.g. the spoke ModuleMap key `^[a-z][a-z0-9_-]*$` on
                        // the patch-DTO `modules` field). The `"_"` default
                        // fails that pattern, so use a lowercase key when the
                        // pattern's first class is `[a-z]` (the only
                        // propertyNames shape in the schema corpus today;
                        // same heuristic spirit as `make_dummy_string`).
                        let key = if prop_def
                            .get("propertyNames")
                            .and_then(|pn| pn.get("pattern"))
                            .and_then(|p| p.as_str())
                            .is_some_and(|p| p.starts_with("^[a-z]"))
                        {
                            "key"
                        } else {
                            "_"
                        };
                        m.insert(key.to_string(), val);
                        Value::Object(m)
                    },
                    |sub_props| {
                        let mut map = serde_json::Map::new();
                        for (sub_name, sub_def) in sub_props {
                            map.insert(
                                sub_name.clone(),
                                make_dummy_value(
                                    sub_def,
                                    schema_cache,
                                    current_dir,
                                    current_schema_path,
                                ),
                            );
                        }
                        Value::Object(map)
                    },
                )
        }
        _ => {
            for key in &["allOf", "oneOf", "anyOf"] {
                if let Some(subs) = prop_def.get(*key).and_then(|a| a.as_array()) {
                    if let Some(first) = subs.first() {
                        return make_dummy_value(
                            first,
                            schema_cache,
                            current_dir,
                            current_schema_path,
                        );
                    }
                }
            }
            Value::String("dummy".to_string())
        }
    }
}

/// note: generate a dummy value by resolving a `$ref` reference.
fn make_dummy_from_ref(
    ref_path: &str,
    schema_cache: &HashMap<String, Value>,
    current_dir: &str,
    current_schema_path: &str,
) -> Value {
    let (file_part, fragment) = ref_path.find('#').map_or_else(
        || (ref_path, None),
        |pos| {
            let file = &ref_path[..pos];
            let frag = &ref_path[pos + 1..];
            if file.is_empty() {
                ("", Some(frag))
            } else {
                (file, Some(frag))
            }
        },
    );

    // why: for a same-file ref (`#/definitions/...`) the cache key IS the
    // current schema's relative path, so use it directly instead of letting
    // `resolve_ref_file` return "" (which made every cache lookup miss and
    // hit the silent "dummy" fallback — e.g. StaleFindingsResponse.items).
    let resolved_path = if file_part.is_empty() {
        current_schema_path.to_string()
    } else {
        resolve_ref_file(file_part, current_dir)
    };

    // why: nested relative refs resolve against the referenced file's directory.
    let ref_dir = Path::new(&resolved_path)
        .parent()
        .and_then(|p| p.to_str())
        .unwrap_or(current_dir);

    let schema = schema_cache
        .get(&resolved_path)
        .or_else(|| schema_cache.get(ref_path));

    if let Some(s) = schema {
        if let Some(frag) = fragment {
            // note: navigate the fragment path like
            // "/definitions/SchemaVersion", skipping the leading empty element
            // from the "/" prefix.
            let parts: Vec<&str> = frag.split('/').filter(|p| !p.is_empty()).collect();
            let mut current = s;
            for part in &parts {
                if let Some(next) = current.get(*part) {
                    current = next;
                } else {
                    return Value::String("dummy".to_string());
                }
            }
            make_dummy_value(current, schema_cache, ref_dir, &resolved_path)
        } else {
            make_dummy_value(s, schema_cache, ref_dir, &resolved_path)
        }
    } else {
        // note: an unresolvable ref falls back to the raw property definition;
        // this happens with relative refs to files not in the cache.
        Value::String("dummy".to_string())
    }
}

/// note: resolve a `$ref` file path to an absolute schema path relative to the
/// repo root.
fn resolve_ref_file(file_part: &str, current_dir: &str) -> String {
    if file_part.is_empty() || file_part.starts_with("https://") {
        // URL like https://nexus42.invalid/schemas/common/...
        file_part
            .strip_prefix("https://nexus42.invalid/")
            .or_else(|| file_part.strip_prefix("https://nexus42.invalid"))
            .map_or_else(|| file_part.to_string(), ToString::to_string)
    } else if file_part.starts_with('/') {
        file_part.trim_start_matches('/').to_string()
    } else {
        let current = Path::new(current_dir);
        let resolved = current.join(file_part);
        resolved.to_str().unwrap_or(file_part).to_string()
    }
}

/// note: check whether a Rust struct's serialized fields match the schema's
/// properties.
fn check_fields_match(
    schema_path: &str,
    struct_name: &str,
    serialized: &Value,
    schema_properties: &[(String, Value)],
    schema_required: &BTreeSet<String>,
    mode: CheckMode,
) -> Vec<String> {
    let mut errors = Vec::new();

    let rust_fields: BTreeSet<String> = match serialized {
        Value::Object(map) => map.keys().cloned().collect(),
        _other => {
            // note: non-object serialization (a unit struct or wrapper type)
            // leaves nothing to compare.
            return errors;
        }
    };

    let schema_fields: BTreeSet<String> =
        schema_properties.iter().map(|(n, _)| n.clone()).collect();

    match mode {
        CheckMode::Strict => {
            for prop_name in &schema_fields {
                if !rust_fields.contains(prop_name) {
                    let prop_type = describe_type(schema_properties, prop_name);
                    errors.push(format!(
                        "  [{schema_path}] {struct_name}: MISSING field \
                         '{prop_name}' (type: {prop_type})",
                    ));
                }
            }
            for field_name in &rust_fields {
                if !schema_fields.contains(field_name) {
                    errors.push(format!(
                        "  [{schema_path}] {struct_name}: EXTRA field \
                         '{field_name}' not in schema",
                    ));
                }
            }
        }
        CheckMode::Subset => {
            for prop_name in schema_required {
                if !rust_fields.contains(prop_name) {
                    let prop_type = describe_type(schema_properties, prop_name);
                    errors.push(format!(
                        "  [{schema_path}] {struct_name}: MISSING required field \
                         '{prop_name}' (type: {prop_type})",
                    ));
                }
            }
        }
    }

    errors
}

/// note: maximum acceptable wall-clock time for drift detection in milliseconds.
///
/// Defaults to 500ms (~25x headroom over typical ~18ms runtime). Override via
/// `NEXUS_DRIFT_LIMIT_MS` env var for slow CI runners.
///
/// The elapsed time is always printed so regressions are observable even when
/// the threshold is not exceeded.
const DRIFT_DETECTION_TIME_LIMIT_MS_DEFAULT: u64 = 500;

/// note: return the drift time limit, checking `NEXUS_DRIFT_LIMIT_MS` env var
/// first.
fn drift_time_limit_ms() -> u64 {
    std::env::var("NEXUS_DRIFT_LIMIT_MS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DRIFT_DETECTION_TIME_LIMIT_MS_DEFAULT)
}

#[test]
// why: one exhaustive per-schema drift sweep over every registered contract.
#[allow(clippy::too_many_lines)]
fn schema_drift_detection() {
    let start = std::time::Instant::now();

    let entries = build_schema_map();
    let schema_cache = build_schema_cache(&entries);
    let mut all_errors: Vec<String> = Vec::new();
    let mut checked_count = 0;

    for entry in &entries {
        let schema = match load_json(entry.schema_path) {
            Ok(s) => s,
            Err(e) => {
                all_errors.push(format!(
                    "  [{path}] ERROR loading schema: {e}",
                    path = entry.schema_path
                ));
                continue;
            }
        };

        let properties = extract_properties(&schema);
        let required = extract_required(&schema);

        if properties.is_empty() {
            if let Some(arms) = schema.get("oneOf").and_then(|v| v.as_array()) {
                for arm in arms {
                    let arm_properties = extract_properties(arm);
                    let arm_required = extract_required(arm);
                    if arm_properties.is_empty() {
                        continue;
                    }
                    let test_json =
                        build_test_json(&arm_properties, &schema_cache, entry.schema_path);
                    for checker in &entry.checkers {
                        checked_count += 1;
                        match checker(test_json.clone()) {
                            Ok((struct_name, serialized)) => {
                                let errors = check_fields_match(
                                    entry.schema_path,
                                    &struct_name,
                                    &serialized,
                                    &arm_properties,
                                    &arm_required,
                                    entry.mode,
                                );
                                all_errors.extend(errors);
                            }
                            Err(e) => {
                                all_errors.push(format!(
                                    "  [{path}] DESERIALIZATION ERROR: {e}",
                                    path = entry.schema_path
                                ));
                            }
                        }
                    }
                }
                continue;
            }
            // note: skip definitions-only schemas like common.schema.json.
            continue;
        }

        let test_json = build_test_json(&properties, &schema_cache, entry.schema_path);

        for checker in &entry.checkers {
            checked_count += 1;
            match checker(test_json.clone()) {
                Ok((struct_name, serialized)) => {
                    let errors = check_fields_match(
                        entry.schema_path,
                        &struct_name,
                        &serialized,
                        &properties,
                        &required,
                        entry.mode,
                    );
                    all_errors.extend(errors);
                }
                Err(e) => {
                    all_errors.push(format!(
                        "  [{path}] DESERIALIZATION ERROR: {e}",
                        path = entry.schema_path
                    ));
                }
            }
        }
    }

    let elapsed = start.elapsed();
    let elapsed_ms = elapsed.as_millis();

    if !all_errors.is_empty() {
        let unique_errors: HashSet<&str> = all_errors.iter().map(String::as_str).collect();
        let summary = format!(
            "\n========================================================\n\
             SCHEMA DRIFT DETECTED\n\
             ========================================================\n\
             {} schemas checked, {} structs tested\n\
             {} drift error(s) found:\n\
             \n{}",
            entries.len(),
            checked_count,
            unique_errors.len(),
            unique_errors.into_iter().collect::<Vec<_>>().join("\n"),
        );
        panic!("{summary}");
    }

    let limit_ms = drift_time_limit_ms();
    eprintln!(
        "  drift detection timing: {elapsed_ms}ms for {} schemas, {} structs (limit: {limit_ms}ms)",
        entries.len(),
        checked_count,
    );

    assert!(
        elapsed < std::time::Duration::from_millis(limit_ms),
        "Schema drift detection took {elapsed_ms}ms, exceeding {limit_ms}ms \
         limit. This may indicate schema growth needs optimization. \
         {entries_len} schemas, {checked_count} structs checked.",
        entries_len = entries.len(),
    );

    println!(
        "✓ Schema drift detection passed: {} schemas, {} structs checked in {}ms",
        entries.len(),
        checked_count,
        elapsed_ms,
    );
}

/// note: create a schema with specific properties for deliberate-drift testing.
fn make_test_schema(properties: &[(&str, &str)]) -> Value {
    let mut props = serde_json::Map::new();
    for (name, type_str) in properties {
        let mut prop = serde_json::Map::new();
        prop.insert("type".to_string(), Value::String(type_str.to_string()));
        props.insert(name.to_string(), Value::Object(prop));
    }
    let mut schema = serde_json::Map::new();
    schema.insert("type".to_string(), Value::String("object".to_string()));
    schema.insert("properties".to_string(), Value::Object(props));
    let required: Vec<Value> = properties
        .iter()
        .map(|(n, _)| Value::String(n.to_string()))
        .collect();
    schema.insert("required".to_string(), Value::Array(required));
    Value::Object(schema)
}

/// note: a struct with fields matching a known subset of schema properties.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
struct TestFixture {
    id: String,
    name: String,
}

#[test]
fn drift_detection_known_matched_passes() {
    let schema = make_test_schema(&[("id", "string"), ("name", "string")]);
    let properties = extract_properties(&schema);
    let required = extract_required(&schema);
    let cache = HashMap::new();
    let test_json = build_test_json(&properties, &cache, "test");

    let v: TestFixture =
        serde_json::from_value(test_json).expect("TestFixture should deserialize from test schema");
    let serialized = serde_json::to_value(&v).expect("TestFixture should serialize");

    let errors = check_fields_match(
        "test",
        "TestFixture",
        &serialized,
        &properties,
        &required,
        CheckMode::Strict,
    );

    assert!(
        errors.is_empty(),
        "Expected no drift errors for matched schema, got: {errors:?}"
    );
}

#[test]
fn drift_detection_deliberate_missing_field_fails() {
    // note: the schema has an 'extra_field' the Rust struct doesn't have.
    let schema = make_test_schema(&[
        ("id", "string"),
        ("name", "string"),
        ("extra_field", "string"),
    ]);
    let properties = extract_properties(&schema);
    let required = extract_required(&schema);
    let cache = HashMap::new();
    let test_json = build_test_json(&properties, &cache, "test");

    let v: TestFixture = serde_json::from_value(test_json)
        .expect("TestFixture should deserialize (serde ignores unknown fields by default)");
    let serialized = serde_json::to_value(&v).expect("TestFixture should serialize");

    let errors = check_fields_match(
        "test",
        "TestFixture",
        &serialized,
        &properties,
        &required,
        CheckMode::Strict,
    );

    assert!(
        !errors.is_empty(),
        "Expected drift errors for missing field but got none"
    );

    let error_text = errors.join("\n");
    assert!(
        error_text.contains("extra_field"),
        "Expected error to mention 'extra_field', got: {error_text}"
    );
    assert!(
        error_text.contains("MISSING"),
        "Expected error to mention 'MISSING', got: {error_text}"
    );
}

#[test]
fn drift_detection_type_mismatch_fails() {
    // note: the schema expects an integer where TestFixture has a String, so
    // the dummy JSON number must fail deserialization.
    let schema = make_test_schema(&[("id", "integer"), ("name", "string")]);
    let properties = extract_properties(&schema);
    let cache = HashMap::new();
    let test_json = build_test_json(&properties, &cache, "test");

    let result: Result<TestFixture, _> = serde_json::from_value(test_json);
    assert!(
        result.is_err(),
        "Expected deserialization to fail for type mismatch (schema says integer, Rust has string)"
    );
}

#[test]
fn actor_ref_one_of_arms_roundtrip_and_reject() {
    let creator = serde_json::json!({
        "actor_kind": "creator",
        "creator_id": "ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    });
    let character = serde_json::json!({
        "actor_kind": "character",
        "character_id": "chr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    });
    let creator_v: ActorRef = serde_json::from_value(creator).expect("creator arm");
    let character_v: ActorRef = serde_json::from_value(character).expect("character arm");
    assert_eq!(
        serde_json::to_value(&creator_v).unwrap()["actor_kind"],
        "creator"
    );
    assert_eq!(
        serde_json::to_value(&character_v).unwrap()["actor_kind"],
        "character"
    );
    assert!(serde_json::from_value::<ActorRef>(serde_json::json!({
        "actor_kind": "npc",
        "creator_id": "ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    }))
    .is_err());
    assert!(serde_json::from_value::<ActorRef>(serde_json::json!({
        "actor_kind": "creator",
        "creator_id": "ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "character_id": "chr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    }))
    .is_err());
}
