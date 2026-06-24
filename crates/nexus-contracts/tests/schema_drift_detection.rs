//! Schema Drift Detection (`WS-D`)
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

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
enum CheckMode {
    /// All schema fields must exist in Rust (bidirectional exact match)
    Strict,
    /// Required schema fields must exist in Rust; Rust may have extra fields
    #[allow(dead_code)]
    Subset,
}

/// A function pointer that deserializes a JSON Value into a concrete Rust type,
/// serializes it back, and returns `(type_name, serialized_value)`.
type CheckFn = Box<dyn Fn(Value) -> Result<(String, Value), String>>;

struct SchemaEntry {
    /// Path to the JSON Schema file, relative to repo root
    schema_path: &'static str,
    /// Validation mode (Strict or Subset)
    mode: CheckMode,
    /// One checker per Rust struct type generated from this schema
    checkers: Vec<CheckFn>,
}

// ---------------------------------------------------------------------------
// Macros
// ---------------------------------------------------------------------------

/// Build a checker for a single Rust struct type.
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

/// Build a `SchemaEntry`. Accepts either a single type or a list of types in brackets.
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

// ---------------------------------------------------------------------------
// Schema Inventory Registry (T1)
// ---------------------------------------------------------------------------

/// Build the complete inventory of schema → Rust struct mappings.
///
/// This is the single source of truth for what gets checked by drift detection.
/// Add new entries here when you add new schemas or structs.
#[allow(clippy::too_many_lines)]
fn build_schema_map() -> Vec<SchemaEntry> {
    vec![
        // ── domain/ ──────────────────────────────────────────────────────
        entry!("schemas/domain/world.schema.json", Strict, World),
        entry!("schemas/domain/memory.schema.json", Strict, Memory),
        entry!("schemas/domain/creator.schema.json", Strict, Creator),
        entry!("schemas/domain/fork-branch.schema.json", Strict, ForkBranch),
        entry!("schemas/domain/key-block.schema.json", Strict, KeyBlock),
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
        // ── common/ ──────────────────────────────────────────────────────
        entry!("schemas/common/version-ref.schema.json", Strict, VersionRef),
        // SourceAnchor lives in common_types.rs, generated from source-anchor.schema.json
        entry!(
            "schemas/common/source-anchor.schema.json",
            Strict,
            SourceAnchor
        ),
        // ── platform/sync/ ───────────────────────────────────────────────
        // V1.62 reorganization: bundle/delta/sync-command moved here from domain/;
        // conflict-response/sync-pull-* moved here from cloud-sync/.
        // Bundle: generated ONLY from platform/sync/bundle.schema.json
        // (platform/sync/bundle-refinement.schema.json is allOf-only, skipped by
        // codegen per SKIP_STRUCT_GENERATION_REL_PATHS).
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
        // ── local-api/compute/ ───────────────────────────────────────────
        // V1.62 reorganization: compute envelopes moved here from compute/.
        // V1.61 WASM compute ABI envelopes (compass Q3/Q8). Only the top-level
        // struct of each schema is registered; inline/definition structs
        // (ComputeOutputStateDelta) are emitted by codegen but validated
        // indirectly via their parent schema.
        // (entity-attributes/entity-state schemas were DELETED in V1.62 P0;
        // per-module shapes now live in modules/<id>/manifest.json per P1.)
        entry!(
            "schemas/local-api/compute/compute-input.schema.json",
            Strict,
            ComputeInput
        ),
        entry!(
            "schemas/local-api/compute/compute-output.schema.json",
            Strict,
            ComputeOutput
        ),
        // ── local-api/works/ (V1.63 P1) ──────────────────────────────────
        entry!(
            "schemas/local-api/works/create-work-request.schema.json",
            Strict,
            CreateWorkRequest
        ),
        entry!(
            "schemas/local-api/works/create-work-response.schema.json",
            Strict,
            CreateWorkResponse
        ),
        entry!(
            "schemas/local-api/works/list-works-query.schema.json",
            Strict,
            ListWorksQuery
        ),
        entry!(
            "schemas/local-api/works/work-summary.schema.json",
            Strict,
            WorkSummary
        ),
        entry!(
            "schemas/local-api/works/list-works-response.schema.json",
            Strict,
            ListWorksResponse
        ),
        entry!(
            "schemas/local-api/works/work-detail-response.schema.json",
            Strict,
            WorkDetailResponse
        ),
        entry!(
            "schemas/local-api/works/patch-work-request.schema.json",
            Strict,
            PatchWorkRequest
        ),
        entry!(
            "schemas/local-api/works/append-inspiration-request.schema.json",
            Strict,
            AppendInspirationRequest
        ),
        entry!(
            "schemas/local-api/works/append-inspiration-response.schema.json",
            Strict,
            AppendInspirationResponse
        ),
        entry!(
            "schemas/local-api/works/release-completion-lock-request.schema.json",
            Strict,
            ReleaseCompletionLockRequest
        ),
        // ── local-api/kb/ (V1.63 P1) ─────────────────────────────────────
        entry!(
            "schemas/local-api/kb/list-kb-entries-query.schema.json",
            Strict,
            ListKbEntriesQuery
        ),
        entry!(
            "schemas/local-api/kb/kb-entry-summary.schema.json",
            Strict,
            KbEntrySummary
        ),
        entry!(
            "schemas/local-api/kb/pagination-info.schema.json",
            Strict,
            PaginationInfo
        ),
        entry!(
            "schemas/local-api/kb/list-kb-entries-response.schema.json",
            Strict,
            ListKbEntriesResponse
        ),
        entry!(
            "schemas/local-api/kb/add-kb-entry-request.schema.json",
            Strict,
            AddKbEntryRequest
        ),
        entry!(
            "schemas/local-api/kb/add-kb-entry-response.schema.json",
            Strict,
            AddKbEntryResponse
        ),
        entry!(
            "schemas/local-api/kb/get-kb-entry-response.schema.json",
            Strict,
            GetKbEntryResponse
        ),
        entry!(
            "schemas/local-api/kb/delete-kb-entry-response.schema.json",
            Strict,
            DeleteKbEntryResponse
        ),
        // ── local-api/findings/ (V1.63 P1) ───────────────────────────────
        entry!(
            "schemas/local-api/findings/create-finding-request.schema.json",
            Strict,
            CreateFindingRequest
        ),
        entry!(
            "schemas/local-api/findings/finding-detail-response.schema.json",
            Strict,
            FindingDetailResponse
        ),
        entry!(
            "schemas/local-api/findings/update-finding-request.schema.json",
            Strict,
            UpdateFindingRequest
        ),
        entry!(
            "schemas/local-api/findings/list-findings-query.schema.json",
            Strict,
            ListFindingsQuery
        ),
        entry!(
            "schemas/local-api/findings/stale-findings-response.schema.json",
            Strict,
            StaleFindingsResponse
        ),
        // ── local-api/schedule/ (V1.63 P1) ───────────────────────────────
        entry!(
            "schemas/local-api/schedule/add-schedule-request.schema.json",
            Strict,
            AddScheduleRequest
        ),
        entry!(
            "schemas/local-api/schedule/add-schedule-response.schema.json",
            Strict,
            AddScheduleResponse
        ),
        entry!(
            "schemas/local-api/schedule/list-schedules-query.schema.json",
            Strict,
            ListSchedulesQuery
        ),
        entry!(
            "schemas/local-api/schedule/schedule-summary.schema.json",
            Strict,
            ScheduleSummary
        ),
        entry!(
            "schemas/local-api/schedule/list-schedules-response.schema.json",
            Strict,
            ListSchedulesResponse
        ),
        entry!(
            "schemas/local-api/schedule/inspect-schedule-response.schema.json",
            Strict,
            InspectScheduleResponse
        ),
        entry!(
            "schemas/local-api/schedule/edit-core-context-request.schema.json",
            Strict,
            EditCoreContextRequest
        ),
        entry!(
            "schemas/local-api/schedule/edit-core-context-response.schema.json",
            Strict,
            EditCoreContextResponse
        ),
        entry!(
            "schemas/local-api/schedule/core-context-response.schema.json",
            Strict,
            CoreContextResponse
        ),
        entry!(
            "schemas/local-api/schedule/core-context-history-entry.schema.json",
            Strict,
            CoreContextHistoryEntry
        ),
        entry!(
            "schemas/local-api/schedule/core-context-history-response.schema.json",
            Strict,
            CoreContextHistoryResponse
        ),
        entry!(
            "schemas/local-api/schedule/signal-schedule-request.schema.json",
            Strict,
            SignalScheduleRequest
        ),
        entry!(
            "schemas/local-api/schedule/signal-schedule-response.schema.json",
            Strict,
            SignalScheduleResponse
        ),
        entry!(
            "schemas/local-api/schedule/delete-schedule-response.schema.json",
            Strict,
            DeleteScheduleResponse
        ),
        // ── local-api/workspace/ (V1.63 P1) ──────────────────────────────
        entry!(
            "schemas/local-api/workspace/list-workspaces-query.schema.json",
            Strict,
            ListWorkspacesQuery
        ),
        entry!(
            "schemas/local-api/workspace/workspace-summary.schema.json",
            Strict,
            WorkspaceSummary
        ),
        entry!(
            "schemas/local-api/workspace/list-workspaces-response.schema.json",
            Strict,
            ListWorkspacesResponse
        ),
        entry!(
            "schemas/local-api/workspace/create-workspace-request.schema.json",
            Strict,
            CreateWorkspaceRequest
        ),
        entry!(
            "schemas/local-api/workspace/create-workspace-response.schema.json",
            Strict,
            CreateWorkspaceResponse
        ),
        entry!(
            "schemas/local-api/workspace/active-workspace-response.schema.json",
            Strict,
            ActiveWorkspaceResponse
        ),
        entry!(
            "schemas/local-api/workspace/set-active-workspace-request.schema.json",
            Strict,
            SetActiveWorkspaceRequest
        ),
        entry!(
            "schemas/local-api/workspace/set-active-workspace-response.schema.json",
            Strict,
            SetActiveWorkspaceResponse
        ),
        // ── local-api/creators/ (V1.63 P1) ───────────────────────────────
        entry!(
            "schemas/local-api/creators/creator-info.schema.json",
            Strict,
            CreatorInfo
        ),
        entry!(
            "schemas/local-api/creators/list-creators-query.schema.json",
            Strict,
            ListCreatorsQuery
        ),
        entry!(
            "schemas/local-api/creators/list-creators-response.schema.json",
            Strict,
            ListCreatorsResponse
        ),
        entry!(
            "schemas/local-api/creators/creator-detail.schema.json",
            Strict,
            CreatorDetail
        ),
        entry!(
            "schemas/local-api/creators/set-active-creator-request.schema.json",
            Strict,
            SetActiveCreatorRequest
        ),
        entry!(
            "schemas/local-api/creators/active-creator-response.schema.json",
            Strict,
            ActiveCreatorResponse
        ),
        entry!(
            "schemas/local-api/creators/set-active-creator-response.schema.json",
            Strict,
            SetActiveCreatorResponse
        ),
        entry!(
            "schemas/local-api/creators/logout-response.schema.json",
            Strict,
            LogoutResponse
        ),
        // ── platform/http-bff/ ───────────────────────────────────────────
        // V1.62 reorganization: platform HTTP bodies moved here from platform/.
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
    ]
}

// ---------------------------------------------------------------------------
// Schema File Loading and Caching (T2)
// ---------------------------------------------------------------------------

/// Derive the complete set of schema file paths from two sources:
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

    // Collect from the provided schema map
    for entry in entries {
        paths.insert(entry.schema_path.to_string());
    }

    // Supplement with glob discovery for any schemas not in the map
    let root = workspace_root();
    let schemas_dir = root.join("schemas");
    collect_schema_files_recursive(&schemas_dir, &root, &mut paths);

    paths
}

/// Recursively collect `.schema.json` files under a directory.
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

/// Workspace root: traverse up from `CARGO_MANIFEST_DIR` (`crates/nexus-contracts`)
/// to the workspace root (2 levels up).
fn workspace_root() -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    // CARGO_MANIFEST_DIR = .../crates/nexus-contracts
    // workspace root = .../ (parent of crates/)
    dir.parent().unwrap().parent().unwrap().to_path_buf()
}

/// Load a JSON file from a path relative to the workspace root.
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
            // Key by relative path
            cache.insert(path.clone(), val.clone());
            // Also key by the `$id` URL if present
            if let Some(id) = val.get("$id").and_then(|v| v.as_str()) {
                // Strip any #fragment from $id before using as key
                let clean_id = id.split('#').next().unwrap_or(id);
                cache.insert(clean_id.to_string(), val);
            }
        }
    }
    cache
}

// ---------------------------------------------------------------------------
// Schema Property Extraction (T2)
// ---------------------------------------------------------------------------

/// Extract property names and their schema definitions from a JSON Schema object.
fn extract_properties(schema: &Value) -> Vec<(String, Value)> {
    let mut props = Vec::new();
    if let Some(properties) = schema.get("properties").and_then(|p| p.as_object()) {
        for (name, prop_def) in properties {
            props.push((name.clone(), prop_def.clone()));
        }
    }
    props
}

/// Extract the set of required field names from a JSON Schema object.
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

/// Describe a property's type for error messages.
fn describe_type(properties: &[(String, Value)], name: &str) -> String {
    properties.iter().find(|(n, _)| n == name).map_or_else(
        || "unknown".to_string(),
        |(_, def)| describe_schema_type(def),
    )
}

/// Get a human-readable type description from a schema property definition.
fn describe_schema_type(def: &Value) -> String {
    if let Some(ref_path) = def.get("$ref").and_then(|r| r.as_str()) {
        // Extract the definition name from the ref path
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

// ---------------------------------------------------------------------------
// Dummy Value Generation (T2)
// ---------------------------------------------------------------------------

/// Build a JSON test payload from schema properties using dummy values.
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
        let dummy = make_dummy_value(prop_def, schema_cache, current_dir);
        map.insert(name.clone(), dummy);
    }
    Value::Object(map)
}

/// Generate a dummy JSON value for a schema property definition.
fn make_dummy_value(
    prop_def: &Value,
    schema_cache: &HashMap<String, Value>,
    current_dir: &str,
) -> Value {
    // Handle $ref (must be first to resolve enum references)
    if let Some(ref_path) = prop_def.get("$ref").and_then(|r| r.as_str()) {
        return make_dummy_from_ref(ref_path, schema_cache, current_dir);
    }

    // Handle const value (e.g., "const": 1)
    if let Some(const_val) = prop_def.get("const") {
        return const_val.clone();
    }

    // Handle inline enum (must precede type-based dispatch so we get the correct value)
    if let Some(enum_vals) = prop_def.get("enum").and_then(|e| e.as_array()) {
        if let Some(first) = enum_vals.first() {
            return first.clone();
        }
    }

    // Handle type
    let type_val = prop_def.get("type");
    let type_str = match type_val {
        Some(Value::String(s)) => Some(s.as_str()),
        Some(Value::Array(arr)) => {
            // Pick the first non-null type from a union type like ["integer", "null"]
            arr.iter().filter_map(|v| v.as_str()).find(|s| *s != "null")
        }
        _ => None,
    };

    match type_str {
        Some("string") => Value::String("dummy".to_string()),
        Some("integer") => Value::Number(serde_json::Number::from(0)),
        Some("number") => Value::Number(serde_json::Number::from_f64(0.0).unwrap()),
        Some("boolean") => Value::Bool(false),
        Some("array") => {
            // Check for items schema
            prop_def.get("items").map_or_else(
                || Value::Array(vec![]),
                |items| {
                    // Generate a single dummy item
                    let item_val = make_dummy_value(items, schema_cache, current_dir);
                    Value::Array(vec![item_val])
                },
            )
        }
        Some("object") => {
            // Build a dummy from sub-properties if available
            prop_def
                .get("properties")
                .and_then(|p| p.as_object())
                .map_or_else(
                    || Value::Object(serde_json::Map::new()),
                    |sub_props| {
                        let mut map = serde_json::Map::new();
                        for (sub_name, sub_def) in sub_props {
                            map.insert(
                                sub_name.clone(),
                                make_dummy_value(sub_def, schema_cache, current_dir),
                            );
                        }
                        Value::Object(map)
                    },
                )
        }
        _ => {
            // Try allOf, oneOf, anyOf
            for key in &["allOf", "oneOf", "anyOf"] {
                if let Some(subs) = prop_def.get(*key).and_then(|a| a.as_array()) {
                    if let Some(first) = subs.first() {
                        return make_dummy_value(first, schema_cache, current_dir);
                    }
                }
            }
            // Last resort
            Value::String("dummy".to_string())
        }
    }
}

/// Generate a dummy value by resolving a `$ref` reference.
fn make_dummy_from_ref(
    ref_path: &str,
    schema_cache: &HashMap<String, Value>,
    current_dir: &str,
) -> Value {
    // Split on # to separate file part from fragment
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

    // Resolve the file path
    let resolved_path = resolve_ref_file(file_part, current_dir);

    // Use the referenced file's directory for resolving nested relative refs
    let ref_dir = Path::new(&resolved_path)
        .parent()
        .and_then(|p| p.to_str())
        .unwrap_or(current_dir);

    // Look up in cache; try both the resolved path and the raw ref_path
    let schema = schema_cache
        .get(&resolved_path)
        .or_else(|| schema_cache.get(ref_path));

    if let Some(s) = schema {
        if let Some(frag) = fragment {
            // Navigate the fragment path like "/definitions/SchemaVersion"
            // Skip leading empty element from "/" prefix
            let parts: Vec<&str> = frag.split('/').filter(|p| !p.is_empty()).collect();
            let mut current = s;
            for part in &parts {
                if let Some(next) = current.get(*part) {
                    current = next;
                } else {
                    return Value::String("dummy".to_string());
                }
            }
            // Now 'current' is the referenced definition
            make_dummy_value(current, schema_cache, ref_dir)
        } else {
            // No fragment: the entire schema is the referenced value
            make_dummy_value(s, schema_cache, ref_dir)
        }
    } else {
        // If we can't resolve, try using the raw property definition as a fallback
        // This happens with relative refs to files that might not be in our cache
        Value::String("dummy".to_string())
    }
}

/// Resolve a `$ref` file path to an absolute schema path relative to the repo root.
fn resolve_ref_file(file_part: &str, current_dir: &str) -> String {
    if file_part.is_empty() || file_part.starts_with("https://") {
        // URL like https://nexus42.invalid/schemas/common/...
        file_part
            .strip_prefix("https://nexus42.invalid/")
            .or_else(|| file_part.strip_prefix("https://nexus42.invalid"))
            .map_or_else(|| file_part.to_string(), ToString::to_string)
    } else if file_part.starts_with('/') {
        // Absolute path in repo
        file_part.trim_start_matches('/').to_string()
    } else {
        // Relative path: resolve against current schema directory
        let current = Path::new(current_dir);
        let resolved = current.join(file_part);
        resolved.to_str().unwrap_or(file_part).to_string()
    }
}

// ---------------------------------------------------------------------------
// Drift Detection Test (T3, T4, T5)
// ---------------------------------------------------------------------------

/// Check whether a Rust struct's serialized fields match the schema's properties.
fn check_fields_match(
    schema_path: &str,
    struct_name: &str,
    serialized: &Value,
    schema_properties: &[(String, Value)],
    schema_required: &BTreeSet<String>,
    mode: CheckMode,
) -> Vec<String> {
    let mut errors = Vec::new();

    // Get the serialized field names from the Rust struct
    let rust_fields: BTreeSet<String> = match serialized {
        Value::Object(map) => map.keys().cloned().collect(),
        _other => {
            // Not an object (e.g., serialized as a string or number)
            // This can happen for unit structs or wrapper types
            return errors; // nothing to compare
        }
    };

    // Get schema property names
    let schema_fields: BTreeSet<String> =
        schema_properties.iter().map(|(n, _)| n.clone()).collect();

    match mode {
        CheckMode::Strict => {
            // Check 1: every schema field must exist in Rust serialized output
            for prop_name in &schema_fields {
                if !rust_fields.contains(prop_name) {
                    let prop_type = describe_type(schema_properties, prop_name);
                    errors.push(format!(
                        "  [{schema_path}] {struct_name}: MISSING field \
                         '{prop_name}' (type: {prop_type})",
                    ));
                }
            }
            // Check 2: every Rust serialized field must exist in schema
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
            // Only required schema fields must exist in Rust
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

// ---------------------------------------------------------------------------
// Main Test (T5, T6)
// ---------------------------------------------------------------------------

/// Maximum acceptable wall-clock time for drift detection in milliseconds.
///
/// Defaults to 500ms (~25x headroom over typical ~18ms runtime). Override via
/// `NEXUS_DRIFT_LIMIT_MS` env var for slow CI runners.
///
/// The elapsed time is always printed so regressions are observable even when
/// the threshold is not exceeded.
const DRIFT_DETECTION_TIME_LIMIT_MS_DEFAULT: u64 = 500;

/// Returns the drift time limit, checking `NEXUS_DRIFT_LIMIT_MS` env var first.
fn drift_time_limit_ms() -> u64 {
    std::env::var("NEXUS_DRIFT_LIMIT_MS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DRIFT_DETECTION_TIME_LIMIT_MS_DEFAULT)
}

#[test]
fn schema_drift_detection() {
    let start = std::time::Instant::now();

    let entries = build_schema_map();
    let schema_cache = build_schema_cache(&entries);
    let mut all_errors: Vec<String> = Vec::new();
    let mut checked_count = 0;

    for entry in &entries {
        // Load the schema file
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

        // Extract properties and required fields from the schema
        let properties = extract_properties(&schema);
        let required = extract_required(&schema);

        if properties.is_empty() {
            // Skip schemas with no properties (definitions-only schemas like common.schema.json)
            continue;
        }

        // Build a JSON test payload with dummy values
        let test_json = build_test_json(&properties, &schema_cache, entry.schema_path);

        // Run each checker on this schema's struct types
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
        // Build a detailed error report
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

    // Log elapsed time for observability
    let limit_ms = drift_time_limit_ms();
    eprintln!(
        "  drift detection timing: {elapsed_ms}ms for {} schemas, {} structs (limit: {limit_ms}ms)",
        entries.len(),
        checked_count,
    );

    // Assert time threshold
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

// ---------------------------------------------------------------------------
// Deliberate Drift Tests (T7)
// ---------------------------------------------------------------------------

/// Helper: create a schema with specific properties for deliberate-drift testing.
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
    // Default: all properties required
    let required: Vec<Value> = properties
        .iter()
        .map(|(n, _)| Value::String(n.to_string()))
        .collect();
    schema.insert("required".to_string(), Value::Array(required));
    Value::Object(schema)
}

/// A struct with fields matching a known subset of schema properties.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
struct TestFixture {
    id: String,
    name: String,
}

#[test]
fn drift_detection_known_matched_passes() {
    // Verify that a known in-sync schema+struct pair passes.
    // The TestFixture above has 'id' and 'name'. Schema should match exactly.
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
    // Schema has a field ('extra_field') that the Rust struct doesn't have.
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

    // Verify the error mentions the missing field
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
    // Schema has a field that expects integer, but TestFixture has String.
    // The deserialization should fail because we pass JSON number for a String field.
    let schema = make_test_schema(&[("id", "integer"), ("name", "string")]);
    let properties = extract_properties(&schema);
    let cache = HashMap::new();
    let test_json = build_test_json(&properties, &cache, "test");

    // TestFixture.id is String, but schema says integer -> deserialization fails
    let result: Result<TestFixture, _> = serde_json::from_value(test_json);
    assert!(
        result.is_err(),
        "Expected deserialization to fail for type mismatch (schema says integer, Rust has string)"
    );
}
