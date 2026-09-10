//! Read-only checkpoint accessors for the daemon-free `nexus42 ops inspect`
//! surface (V1.182 P1 BL-04).
//!
//! Scoped so the public storage surface stays storage-neutral for the
//! daemon's `SessionStorage` contract: these row shapes are read-only
//! projections of `orchestration_sessions` and exist ONLY for the CLI
//! inspector (single consumer today; revisit when a second consumer appears,
//! qc1 W2). The accessors are `impl SqliteSessionStorage` methods defined in
//! `sqlite.rs` (the pool is private to that module).
//!
//! Honesty invariants:
//! - `status` is the raw DB column. For **v1 rows** (`execution_version == 1`)
//!   it is authoritative (written by `commit_transition`); for **v0 legacy
//!   rows** (`execution_version == 0`) it remains diagnostic only (every
//!   legacy save wrote `'running'`; ON CONFLICT never updated it; the
//!   schedule-cancel handler wrote `'cancelled'`). Negative/forward
//!   execution versions are unsupported and non-replayable. The v0/v1
//!   split is surfaced via the `execution_version` column (A2).
//! - The list view never loads `context_json` (it can embed chat history);
//!   it projects the resume-rule predicates in SQL instead.
//! - Timestamps are unix epoch seconds written on every save.

/// Detail-mode row: the full checkpoint projection of
/// `orchestration_sessions` (identity columns + persisted position +
/// timestamps + the raw context blob + the durable v1 execution metadata).
///
/// `context_json` is deliberately raw: detail mode parses it itself so it
/// can distinguish corrupt bytes from unexpected shapes and report
/// `_run_status`/`_run_error` verbatim. `execution_version` /
/// `state_revision` / `run_state_json` are the A2 durable columns; v1 rows
/// (`execution_version == 1`) carry the authoritative status/state, v0 rows
/// (`execution_version == 0`) stay legacy/unverified, and negative/forward
/// versions are unsupported.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CheckpointRow {
    /// Run id (PRIMARY KEY).
    pub session_id: String,
    /// Owning creator id.
    pub creator_id: String,
    /// Strategy / preset id.
    pub preset_id: String,
    /// Strategy / preset version.
    pub preset_version: i64,
    /// Persisted position (`None` = no position recorded yet).
    pub current_task_id: Option<String>,
    /// Raw DB status column — authoritative for v1 rows, diagnostic for v0.
    pub status: String,
    /// Raw serialized session context blob.
    pub context_json: Vec<u8>,
    /// Execution version: `0` = legacy/unverified, `1` = v1 authoritative
    /// (A2); negative/forward versions are unsupported.
    pub execution_version: i64,
    /// State revision (CAS anchor; `0` on v0 rows).
    pub state_revision: i64,
    /// Serialized [`crate::run_state::RunStateV1`] for v1 rows (`None` = corrupt/absent).
    pub run_state_json: Option<Vec<u8>>,
    /// Serialized [`crate::run_state::RunDescriptorV1`] for v1 rows.
    pub run_descriptor_json: Option<Vec<u8>>,
    /// First-save timestamp (unix epoch seconds).
    pub created_at: i64,
    /// Last-save timestamp (unix epoch seconds).
    pub updated_at: i64,
}

/// List-mode row: a lean, storage-neutral projection.
///
/// `context_json` is never loaded — the resume-rule predicates are
/// evaluated in SQL (JSON1: `json_valid` / `json_type` / `json_extract` /
/// `json_each`), so only the verdict inputs cross the boundary:
/// - `context_valid_json` — false ⇒ the row's context failed JSON parsing.
/// - `context_data_is_object` — false ⇒ JSON parsed but `data` is not an
///   object (schema-shape anomaly, distinct from byte corruption).
/// - `run_status` / `run_error` — the typed failure record, extracted as
///   text scalars (string-typed values only; `None` otherwise, mirroring
///   `graph_flow::Context::get` semantics).
/// - `live_join_keys` — legacy non-null join-tracker key names.
/// - `gate_park_live` — the current task's exact scheduler-park marker.
///
/// The v1 durable state and descriptor are projected raw. Rust structurally
/// deserializes both before assigning a replayable recovery class, so list,
/// detail, and boot fail closed on the same malformed metadata.
///
/// The daemon-side rules 1–4 (legacy v0 cascade) are exactly reproduced by
/// [`crate::resume_rules::classify_resumability_extracted`].
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CheckpointSummary {
    /// Run id (PRIMARY KEY).
    pub session_id: String,
    /// Owning creator id.
    pub creator_id: String,
    /// Strategy / preset id.
    pub preset_id: String,
    /// Strategy / preset version.
    pub preset_version: i64,
    /// Persisted position (`None` = no position recorded yet).
    pub current_task_id: Option<String>,
    /// Raw DB status column — authoritative for v1 rows, diagnostic for v0.
    pub status: String,
    /// Execution version: `0` = legacy/unverified, `1` = v1 authoritative;
    /// negative/forward versions are unsupported and non-replayable.
    pub execution_version: i64,
    /// State revision (CAS anchor; `0` on v0 rows).
    pub state_revision: i64,
    /// Serialized [`crate::run_state::RunStateV1`] for v1 rows, raw
    /// (`None` = absent on v0 rows or corrupt/absent blob on v1 rows).
    pub run_state_json: Option<Vec<u8>>,
    /// Serialized [`crate::run_state::RunDescriptorV1`] for v1 rows.
    pub run_descriptor_json: Option<Vec<u8>>,
    /// First-save timestamp (unix epoch seconds).
    pub created_at: i64,
    /// Last-save timestamp (unix epoch seconds).
    pub updated_at: i64,
    /// `context_json` parsed as JSON successfully.
    pub context_valid_json: bool,
    /// `data` key of the context root is a JSON object.
    pub context_data_is_object: bool,
    /// `_run_status` value when it is a JSON string.
    pub run_status: Option<String>,
    /// `_run_error` value when it is a JSON string.
    pub run_error: Option<String>,
    /// Comma-joined live join-key names (`None` when none live).
    pub live_join_keys: Option<String>,
    /// True only when the current task's `_gate_park_<task>` marker is live.
    pub gate_park_live: bool,
}
