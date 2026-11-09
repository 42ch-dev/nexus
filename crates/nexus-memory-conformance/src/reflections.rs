//! Reflection admission (§6.5, §7.25–§7.27): the `OpTarget` family for
//! `memory.reflections.{state,list,generate}`, its freeze classification and
//! the honest cognition refusal.
//!
//! All three reflection ops require the `reflect` capability covering the
//! request's selection (`reflect` never unions across grants, MEM-R006). The
//! §6.5 facets follow the selection exactly as in recall: `subject_all` needs
//! an unrestricted grant (C-075), `selected_contexts` needs coverage of every
//! listed context, and a shared read needs the shared-access facet; `admin`
//! implies `reflect` inside the published evaluator. The provenance filter is
//! a read-time intersection, never a placement statement (§6.13), so it
//! narrows rows without changing the grant requirement.
//!
//! This slice serves no cognition family: a present `cognition_filter` is
//! refused `INVALID_INPUT` on all three ops — an honest refusal, never a
//! silent widening of the served surface. `state`/`list` are read-only and
//! stay admitted in a frozen space (§8.2 allowlist); `generate` is not
//! allowlisted and refuses `SPACE_FROZEN`. Refusal constructors live in this
//! slice because the frozen slices keep theirs module-private; codes and
//! `details` shapes are the published protocol ones.

use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{json, Map, Value};
use spoke_operations::spoke_schemas::memory_reflections_generate_request::ReadSelection as GenerateSelection;
use spoke_operations::spoke_schemas::memory_reflections_list_request::ReadSelection as ListSelection;
use spoke_operations::spoke_schemas::memory_reflections_state_request::ReadSelection as StateSelection;
use spoke_operations::spoke_schemas::reflection::ReflectionCostUnitKind;
use spoke_operations::spoke_schemas::{
    ExtensionKey, MemoryReceipt, MemoryReflectionsGenerateRequest, MemoryReflectionsListRequest,
    MemoryReflectionsStateRequest,
};
use spoke_operations::{
    is_freeze_allowlisted, jcs_sha256_hex, validate_memory_reflections_generate,
    validate_memory_reflections_list, validate_memory_reflections_state, GrantRequirement,
    SpokeReject, SpokeRejectCode, SpokeResult,
};
use sqlx::{Row, SqliteConnection};
use std::num::NonZeroU64;
use std::str::FromStr;
use uuid::Uuid;

use crate::candidates::{
    receipt_id, release_expired_binding_in_tx, required_capabilities, resolve_replay,
    settle_receipt_in_tx, stage_receipt_in_tx, Admitted, Mutation, ReceiptState, ReceiptTarget,
    ReplayResolution,
};
use crate::faults::FaultPoint;
use crate::policy::describe_body;
use crate::provider::FixtureMemoryProvider;
use crate::spaces::{
    bump_snapshot, internal, internal_failure, is_authorized, load_grants, snapshot_marker,
    ENGINE_POLICY_VERSION, MAX_PAGE_SIZE,
};
use crate::storage::FixtureStore;
use crate::FixtureError;

/// The grant facets one reflection request's selection needs (§6.5).
///
/// `state`/`generate` selections are required; `list` may omit its selection
/// to list the whole subject, which is the same whole-subject requirement as
/// `subject_all`.
#[derive(Debug, Clone)]
pub(crate) struct ReflectionTarget {
    subject_all: bool,
    context_ids: Vec<String>,
    reads_shared: bool,
}

impl ReflectionTarget {
    fn new(subject_all: bool, context_ids: &[String], reads_shared: bool) -> Self {
        Self {
            subject_all,
            context_ids: context_ids.to_vec(),
            reads_shared,
        }
    }

    pub(crate) fn from_state(selection: &StateSelection) -> Self {
        match selection {
            StateSelection::SubjectAll => Self::new(true, &[], true),
            StateSelection::SharedOnly => Self::new(false, &[], true),
            StateSelection::SelectedContexts {
                context_ids,
                include_shared,
            } => Self::new(false, context_ids, include_shared.unwrap_or(false)),
        }
    }

    pub(crate) fn from_list(selection: Option<&ListSelection>) -> Self {
        match selection {
            None | Some(ListSelection::SubjectAll) => Self::new(true, &[], true),
            Some(ListSelection::SharedOnly) => Self::new(false, &[], true),
            Some(ListSelection::SelectedContexts {
                context_ids,
                include_shared,
            }) => Self::new(false, context_ids, include_shared.unwrap_or(false)),
        }
    }

    pub(crate) fn from_generate(selection: &GenerateSelection) -> Self {
        match selection {
            GenerateSelection::SubjectAll => Self::new(true, &[], true),
            GenerateSelection::SharedOnly => Self::new(false, &[], true),
            GenerateSelection::SelectedContexts {
                context_ids,
                include_shared,
            } => Self::new(false, context_ids, include_shared.unwrap_or(false)),
        }
    }

    /// The §6.5 requirement one grant must cover: `reflect` over this
    /// selection's facets.
    fn requirement<'a>(&'a self, subject_id: &'a str) -> GrantRequirement<'a> {
        GrantRequirement {
            capability: "reflect",
            subject_id: Some(subject_id),
            context_ids: self.context_ids.clone(),
            reads_shared: self.reads_shared,
            subject_all: self.subject_all,
        }
    }
}

// ---------------------------------------------------------------------------
// Protocol errors
// ---------------------------------------------------------------------------

/// Same `INVALID_INPUT` envelope the published validators emit: the offending
/// request field in `details.field`.
fn invalid_input(field: &str, message: impl Into<String>) -> SpokeReject {
    let mut details = serde_json::Map::new();
    details.insert(
        "field".to_owned(),
        serde_json::Value::String(field.to_owned()),
    );
    SpokeReject {
        code: SpokeRejectCode::InvalidInput,
        message: message.into(),
        details: Some(details),
    }
}

fn not_authorized(message: impl Into<String>) -> SpokeReject {
    SpokeReject {
        code: SpokeRejectCode::NotAuthorized,
        message: message.into(),
        details: None,
    }
}

fn space_frozen(space_id: &str, op: &str) -> SpokeReject {
    SpokeReject {
        code: SpokeRejectCode::SpaceFrozen,
        message: format!("space {space_id} is frozen; {op} is outside the §8.2 allowlist"),
        details: None,
    }
}

fn not_found(message: impl Into<String>) -> SpokeReject {
    SpokeReject {
        code: SpokeRejectCode::NotFound,
        message: message.into(),
        details: None,
    }
}

/// This fixture serves no cognition family, so a present `cognition_filter`
/// is refused on every reflection op before any storage is touched.
fn cognition_filter_unsupported() -> SpokeReject {
    invalid_input(
        "cognition_filter",
        "this provider does not serve cognition-filtered reflections",
    )
}

async fn space_status(conn: &mut SqliteConnection, space_id: &str) -> Result<String, SpokeReject> {
    let row = sqlx::query("SELECT status FROM spaces WHERE space_id = ?")
        .bind(space_id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(internal_failure)?
        .ok_or_else(|| not_found(format!("space {space_id} is not served by this fixture")))?;
    row.try_get("status").map_err(internal_failure)
}

// ---------------------------------------------------------------------------
// Admission
// ---------------------------------------------------------------------------

impl FixtureMemoryProvider {
    /// One admission decision for a reflection op: cognition refusal, served
    /// space, §8.2 freeze classification, then §6.5 grant coverage — the
    /// whole decision on the caller's single snapshot. `state`/`list` pass
    /// `is_freeze_allowlisted`; `generate` refuses `SPACE_FROZEN` when frozen.
    /// The admission decision's inputs arrive per-op from the serving slices;
    /// the count is the protocol surface, not a design smell. The cognition
    /// filter arrives as presence only: each request type generates its own
    /// filter shape and every shape is refused identically.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn require_reflection_admitted(
        &self,
        conn: &mut SqliteConnection,
        principal_id: &str,
        op: &str,
        space_id: &str,
        subject_id: &str,
        target: &ReflectionTarget,
        cognition_filter_present: bool,
        now: DateTime<Utc>,
    ) -> Result<(), SpokeReject> {
        if cognition_filter_present {
            return Err(cognition_filter_unsupported());
        }
        if space_id != self.identity.space_id {
            return Err(not_found(format!(
                "space {space_id} is not served by this fixture"
            )));
        }
        let status = space_status(conn, space_id).await?;
        if status == "frozen" && !is_freeze_allowlisted(op) {
            return Err(space_frozen(space_id, op));
        }
        // §6.5/MEM-R006: one current grant must cover the whole target.
        let grants = load_grants(conn, space_id).await?;
        if !is_authorized(
            &grants,
            principal_id,
            space_id,
            now,
            &target.requirement(subject_id),
        ) {
            return Err(not_authorized(
                "the authenticated principal holds no grant covering this reflection selection",
            ));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Read ops: `memory.reflections.state` and `memory.reflections.list`
// ---------------------------------------------------------------------------

/// The fixture-private `basis_json` member a generate run pins beyond the four
/// published `ReflectionBasis` fields. C-077's read-time exclusion needs the
/// pinned source record ids and the published shape carries only their digest,
/// so storage keeps the id list beside them; the served basis strips it.
const PINNED_SOURCE_IDS_KEY: &str = "source_ids";

fn checked(result: SpokeResult<()>) -> Result<(), SpokeReject> {
    match result {
        SpokeResult::Ok(()) => Ok(()),
        SpokeResult::Reject(reject) => Err(reject),
    }
}

fn cursor_invalid(message: impl Into<String>) -> SpokeReject {
    SpokeReject {
        code: SpokeRejectCode::CursorInvalid,
        message: message.into(),
        details: None,
    }
}

async fn subject_status(
    conn: &mut SqliteConnection,
    space_id: &str,
    subject_id: &str,
) -> Result<String, SpokeReject> {
    let row = sqlx::query("SELECT status FROM subjects WHERE space_id = ? AND subject_id = ?")
        .bind(space_id)
        .bind(subject_id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(internal_failure)?
        .ok_or_else(|| not_found(format!("subject {subject_id} does not exist in this space")))?;
    row.try_get("status").map_err(internal_failure)
}

/// §6.5 placement parity with recall: `subject_all` admits every placement,
/// `shared_only` only shared records, and a context selection its named
/// contexts plus shared records when `include_shared`.
fn target_admits(target: &ReflectionTarget, visibility: &str, context_id: Option<&str>) -> bool {
    if target.subject_all {
        return true;
    }
    if visibility == "shared" {
        return target.reads_shared;
    }
    context_id.is_some_and(|context_id| {
        target
            .context_ids
            .iter()
            .any(|named| named == context_id)
    })
}

/// §6.13: the provenance filter intersects the record's recorded claim and
/// combines with the selection by AND; both sides arrive as protocol JSON.
fn filter_admits(provenance: &Value, filter: Option<&Value>) -> bool {
    let Some(filter) = filter else {
        return true;
    };
    provenance
        .get("source_context_ids")
        .and_then(Value::as_array)
        .is_some_and(|claims| {
            claims.iter().filter_map(Value::as_str).any(|claim| {
                filter
                    .get("source_context_ids")
                    .and_then(Value::as_array)
                    .is_some_and(|wanted| {
                        wanted.iter().filter_map(Value::as_str).any(|named| named == claim)
                    })
            })
        })
}

/// One live source record feeding a basis digest: the tombstone predicate
/// keeps dead rows out (MEM-R014), ordered by id for a total digest input.
struct BasisRecord {
    record_id: String,
    revision: i64,
    visibility: String,
    context_id: Option<String>,
    provenance: Value,
}

async fn load_basis_records(
    conn: &mut SqliteConnection,
    space_id: &str,
    subject_id: &str,
) -> Result<Vec<BasisRecord>, SpokeReject> {
    let rows = sqlx::query(
        "SELECT record_id, revision, scope_visibility, scope_context_id, provenance_json \
         FROM records WHERE space_id = ? AND subject_id = ? AND tombstone_json IS NULL \
         ORDER BY record_id",
    )
    .bind(space_id)
    .bind(subject_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(internal_failure)?;
    rows.into_iter()
        .map(|row| {
            Ok(BasisRecord {
                record_id: row.try_get("record_id").map_err(internal_failure)?,
                revision: row.try_get("revision").map_err(internal_failure)?,
                visibility: row.try_get("scope_visibility").map_err(internal_failure)?,
                context_id: row.try_get("scope_context_id").map_err(internal_failure)?,
                provenance: json_field(
                    &row.try_get::<String, _>("provenance_json").map_err(internal_failure)?,
                    "record provenance",
                )?,
            })
        })
        .collect()
}

async fn load_profile_revisions(
    conn: &mut SqliteConnection,
    space_id: &str,
    subject_id: &str,
) -> Result<Vec<(String, i64)>, SpokeReject> {
    let rows = sqlx::query(
        "SELECT profile_kind, revision FROM profiles \
         WHERE space_id = ? AND subject_id = ? ORDER BY profile_kind",
    )
    .bind(space_id)
    .bind(subject_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(internal_failure)?;
    rows.iter()
        .map(|row| {
            Ok((
                row.try_get::<String, _>("profile_kind").map_err(internal_failure)?,
                row.try_get::<i64, _>("revision").map_err(internal_failure)?,
            ))
        })
        .collect()
}

/// The subject's tombstoned record ids (C-077's read-time exclusion set).
async fn tombstoned_record_ids(
    conn: &mut SqliteConnection,
    space_id: &str,
    subject_id: &str,
) -> Result<Vec<String>, SpokeReject> {
    let rows = sqlx::query(
        "SELECT record_id FROM records \
         WHERE space_id = ? AND subject_id = ? AND tombstone_json IS NOT NULL",
    )
    .bind(space_id)
    .bind(subject_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(internal_failure)?;
    rows.iter()
        .map(|row| row.try_get("record_id").map_err(internal_failure))
        .collect()
}

fn json_field(text: &str, name: &str) -> Result<Value, SpokeReject> {
    serde_json::from_str(text)
        .map_err(|error| internal(format!("stored {name} is not valid JSON: {error}")))
}

/// The §6.5 facets a stored (or received) selection resolves to, so pinned
/// rows and live records meet in one predicate without new protocol types.
fn target_from_value(selection: &Value) -> Result<ReflectionTarget, SpokeReject> {
    let mode = selection
        .get("mode")
        .and_then(Value::as_str)
        .ok_or_else(|| internal("a stored reflection selection carries no mode"))?;
    match mode {
        "subject_all" => Ok(ReflectionTarget::new(true, &[], true)),
        "shared_only" => Ok(ReflectionTarget::new(false, &[], true)),
        "selected_contexts" => {
            let ids: Vec<String> = selection
                .get("context_ids")
                .and_then(Value::as_array)
                .map(|ids| {
                    ids.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default();
            let reads_shared = selection
                .get("include_shared")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            Ok(ReflectionTarget::new(false, &ids, reads_shared))
        }
        other => Err(internal(format!("unknown selection mode {other:?}"))),
    }
}

/// The fixture's canonical basis object over one resolved selection/filter:
/// sorted source `[record_id, revision]` pairs, the profile revisions, the
/// selection/filter resolution and the engine policy. `jcs_sha256_hex` over
/// this object is the whole C-042 mechanism: computed at read time, never
/// stored, so every basis-affecting change alters the digest at the next read.
fn canonical_basis_object(
    selection: &Value,
    filter: Option<&Value>,
    profiles: &[(String, i64)],
    source_set: &[(&str, i64)],
) -> Value {
    let mut profile_kinds: Vec<&str> = profiles.iter().map(|(kind, _)| kind.as_str()).collect();
    profile_kinds.sort_unstable();
    let mut profile_revisions = Map::new();
    for (kind, revision) in profiles {
        profile_revisions.insert(kind.clone(), json!(revision));
    }
    json!({
        "selection": selection,
        "filter": filter,
        "profile_kinds": profile_kinds,
        "profile_revisions": profile_revisions,
        "record_count": source_set.len(),
        "source_set": source_set,
        "engine_policy_version": ENGINE_POLICY_VERSION,
    })
}

fn basis_digest(object: &Value) -> Result<String, SpokeReject> {
    match jcs_sha256_hex(object) {
        SpokeResult::Ok(digest) => Ok(digest),
        SpokeResult::Reject(reject) => Err(reject),
    }
}

/// The current digest over one pinned selection/filter: the same composer as
/// `state`, fed by that row's pinned pair (§7.26 staleness at read time).
fn current_digest_for(
    selection: &Value,
    filter: Option<&Value>,
    records: &[BasisRecord],
    profiles: &[(String, i64)],
) -> Result<String, SpokeReject> {
    let target = target_from_value(selection)?;
    let source_set: Vec<(&str, i64)> = records
        .iter()
        .filter(|record| {
            target_admits(&target, &record.visibility, record.context_id.as_deref())
                && filter_admits(&record.provenance, filter)
        })
        .map(|record| (record.record_id.as_str(), record.revision))
        .collect();
    basis_digest(&canonical_basis_object(selection, filter, profiles, &source_set))
}

/// The pinned source record ids one stored basis carries (C-077).
fn pinned_source_ids(basis: &Value) -> Vec<String> {
    basis
        .get(PINNED_SOURCE_IDS_KEY)
        .and_then(Value::as_array)
        .map(|ids| {
            ids.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// C-077: a reflection whose *whole* pinned source set has been tombstoned is
/// excluded from list/get at read time; the row itself is never deleted. A
/// partially tombstoned set keeps the reflection (stale already reports it).
fn source_set_tombstoned(source_ids: &[String], tombstoned: &[String]) -> bool {
    !source_ids.is_empty() && source_ids.iter().all(|id| tombstoned.contains(id))
}

fn pinned_state_digest(basis: &Value) -> Result<String, SpokeReject> {
    basis
        .get("state_digest")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| internal("a stored reflection basis carries no state_digest"))
}

/// The resume index one list cursor names on exactly this surviving set: a
/// different binding digest, a missing entry or a malformed cursor is refused
/// `CURSOR_INVALID` instead of silently resuming elsewhere (MEM-R026).
fn list_cursor_start(
    cursor: Option<&str>,
    binding: &str,
    served: &[ServedReflection],
) -> Result<usize, SpokeReject> {
    let Some(cursor) = cursor else {
        return Ok(0);
    };
    let (digest, last_id) = cursor
        .split_once(':')
        .ok_or_else(|| cursor_invalid("cursor is malformed"))?;
    if digest != binding {
        return Err(cursor_invalid(
            "cursor was issued for a different query or provider state",
        ));
    }
    served
        .iter()
        .position(|row| row.reflection_id == last_id)
        .map(|position| position + 1)
        .ok_or_else(|| cursor_invalid("cursor no longer matches the listed set"))
}

/// The fixture's provider-variable sufficiency verdict: an authorized basis
/// with at least one source record is sufficient (C-079 asserts the outcome
/// shape, not shared thresholds; the same verdict gates generate in Task 4).
const fn sufficient(record_count: usize) -> bool {
    record_count > 0
}

/// `REQUIRED_PROFILE_KINDS` read from the describe body's single source of
/// truth; the fixture requires no authored kinds, so this stays empty.
fn required_profile_kinds() -> Result<Vec<String>, SpokeReject> {
    serde_json::from_value(describe_body()["required_profile_kinds"].clone())
        .map_err(|error| internal(format!("describe body carries malformed profile kinds: {error}")))
}

fn missing_required_profile_kinds(
    profiles: &[(String, i64)],
) -> Result<Vec<String>, SpokeReject> {
    let required = required_profile_kinds()?;
    Ok(required
        .into_iter()
        .filter(|kind| !profiles.iter().any(|(present, _)| present == kind))
        .collect())
}

/// One reflection row as it serves: pinned columns verbatim plus the parsed
/// basis, with the fixture-private `source_ids` member still attached.
struct ServedReflection {
    reflection_id: String,
    space_id: String,
    subject_id: String,
    created_at: String,
    revision: i64,
    schema_version: i64,
    selection: Value,
    filter: Option<Value>,
    cognition_filter: Option<Value>,
    basis: Value,
    body_text: String,
    body_format: String,
    cost: Option<(String, i64)>,
    engine_policy_version: String,
    model: Option<String>,
    extensions: Value,
}

/// One subject's reflections oldest-first, the stable paging order. Rows are
/// immutable and never deleted, so the scan order is total.
async fn load_served_reflections(
    conn: &mut SqliteConnection,
    space_id: &str,
    subject_id: &str,
) -> Result<Vec<ServedReflection>, SpokeReject> {
    let rows = sqlx::query(
        "SELECT reflection_id, space_id, subject_id, created_at, revision, schema_version, \
         selection_json, filter_json, cognition_filter_json, basis_json, body_text, body_format, \
         cost_unit_kind, cost_units, engine_policy_version, model, extensions_json \
         FROM reflections WHERE space_id = ? AND subject_id = ? \
         ORDER BY created_at, reflection_id",
    )
    .bind(space_id)
    .bind(subject_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(internal_failure)?;
    rows.into_iter()
        .map(|row| {
            let cost_unit_kind: Option<String> =
                row.try_get("cost_unit_kind").map_err(internal_failure)?;
            let cost_units: Option<i64> = row.try_get("cost_units").map_err(internal_failure)?;
            Ok(ServedReflection {
                reflection_id: row.try_get("reflection_id").map_err(internal_failure)?,
                space_id: row.try_get("space_id").map_err(internal_failure)?,
                subject_id: row.try_get("subject_id").map_err(internal_failure)?,
                created_at: row.try_get("created_at").map_err(internal_failure)?,
                revision: row.try_get("revision").map_err(internal_failure)?,
                schema_version: row.try_get("schema_version").map_err(internal_failure)?,
                selection: json_field(
                    &row.try_get::<String, _>("selection_json").map_err(internal_failure)?,
                    "reflection selection",
                )?,
                filter: row
                    .try_get::<Option<String>, _>("filter_json")
                    .map_err(internal_failure)?
                    .map(|text| json_field(&text, "reflection filter"))
                    .transpose()?,
                cognition_filter: row
                    .try_get::<Option<String>, _>("cognition_filter_json")
                    .map_err(internal_failure)?
                    .map(|text| json_field(&text, "reflection cognition filter"))
                    .transpose()?,
                basis: json_field(
                    &row.try_get::<String, _>("basis_json").map_err(internal_failure)?,
                    "reflection basis",
                )?,
                body_text: row.try_get("body_text").map_err(internal_failure)?,
                body_format: row.try_get("body_format").map_err(internal_failure)?,
                cost: cost_unit_kind.zip(cost_units),
                engine_policy_version: row
                    .try_get("engine_policy_version")
                    .map_err(internal_failure)?,
                model: row.try_get("model").map_err(internal_failure)?,
                extensions: json_field(
                    &row.try_get::<String, _>("extensions_json").map_err(internal_failure)?,
                    "reflection extensions",
                )?,
            })
        })
        .collect()
}

/// The published `Reflection` wire object: pinned columns verbatim, the
/// four-member basis (the fixture-private `source_ids` stripped), and `stale`
/// recomputed by the caller at read time (§7.26).
fn reflection_wire(row: &ServedReflection, stale: bool) -> Result<Value, SpokeReject> {
    let mut wire_basis = row
        .basis
        .as_object()
        .cloned()
        .ok_or_else(|| internal("a stored reflection basis is not an object"))?;
    wire_basis.remove(PINNED_SOURCE_IDS_KEY);
    let mut object = Map::new();
    object.insert("reflection_id".into(), json!(row.reflection_id));
    object.insert("space_id".into(), json!(row.space_id));
    object.insert("subject_id".into(), json!(row.subject_id));
    object.insert(
        "revision".into(),
        json!(u64::try_from(row.revision).map_err(internal_failure)?),
    );
    object.insert(
        "schema_version".into(),
        json!(u64::try_from(row.schema_version).map_err(internal_failure)?),
    );
    object.insert("created_at".into(), json!(row.created_at));
    object.insert("selection".into(), row.selection.clone());
    if let Some(filter) = &row.filter {
        object.insert("filter".into(), filter.clone());
    }
    if let Some(cognition_filter) = &row.cognition_filter {
        object.insert("cognition_filter".into(), cognition_filter.clone());
    }
    object.insert("basis".into(), Value::Object(wire_basis));
    object.insert(
        "body".into(),
        json!({ "body": row.body_text, "format": row.body_format }),
    );
    if let Some((unit_kind, units)) = &row.cost {
        object.insert(
            "cost".into(),
            json!({ "unit_kind": unit_kind, "units": f64::from(u32::try_from(*units).map_err(internal_failure)?), }),
        );
    }
    let mut generator = Map::new();
    generator.insert(
        "engine_policy_version".into(),
        json!(row.engine_policy_version),
    );
    if let Some(model) = &row.model {
        generator.insert("model".into(), json!(model));
    }
    object.insert("generator".into(), Value::Object(generator));
    object.insert("extensions".into(), row.extensions.clone());
    object.insert("stale".into(), json!(stale));
    Ok(Value::Object(object))
}

/// The `memory.reflections.state` view: content-free basis, the fixture's
/// sufficiency verdict, honest staleness and the digest CAS token.
pub(crate) struct ReflectionStateView {
    pub(crate) basis: Value,
    pub(crate) sufficient: bool,
    pub(crate) missing_required_profile_kinds: Vec<String>,
    pub(crate) stale: bool,
    pub(crate) state_digest: String,
    pub(crate) last_reflection_id: Option<String>,
}

pub(crate) struct ReflectionListPage {
    pub(crate) reflections: Vec<Value>,
    pub(crate) next_cursor: Option<String>,
}

impl FixtureMemoryProvider {
    /// [`Self::reflection_state_in_tx`] on one read snapshot: the whole read
    /// runs inside one transaction so the digest, the staleness verdict and
    /// the last-reflection lookup agree.
    pub(crate) async fn reflection_state(
        &self,
        principal_id: &str,
        request: &MemoryReflectionsStateRequest,
        now: DateTime<Utc>,
    ) -> Result<ReflectionStateView, SpokeReject> {
        checked(validate_memory_reflections_state(request))?;
        let mut transaction = self
            .store()
            .pool()
            .begin()
            .await
            .map_err(internal_failure)?;
        let view = self
            .reflection_state_in_tx(&mut transaction, principal_id, request, now)
            .await?;
        transaction.commit().await.map_err(internal_failure)?;
        Ok(view)
    }

    /// [`Self::reflection_state`] on a caller-owned connection; never commits.
    pub(crate) async fn reflection_state_in_tx(
        &self,
        conn: &mut SqliteConnection,
        principal_id: &str,
        request: &MemoryReflectionsStateRequest,
        now: DateTime<Utc>,
    ) -> Result<ReflectionStateView, SpokeReject> {
        let space_id = request.space_id.as_str();
        let subject_id = request.subject_id.as_str();
        let target = ReflectionTarget::from_state(&request.selection);
        self.require_reflection_admitted(
            conn,
            principal_id,
            "memory.reflections.state",
            space_id,
            subject_id,
            &target,
            request.cognition_filter.is_some(),
            now,
        )
        .await?;
        let status = subject_status(conn, space_id, subject_id).await?;
        let selection_value =
            serde_json::to_value(&request.selection).map_err(internal_failure)?;
        let filter_value = request
            .filter
            .as_ref()
            .map(|filter| serde_json::to_value(filter).map_err(internal_failure))
            .transpose()?;
        let records = self.live_basis_records(
            conn,
            space_id,
            subject_id,
            &status,
            &target,
            filter_value.as_ref(),
        )
        .await?;
        let profiles = load_profile_revisions(conn, space_id, subject_id).await?;
        let source_set: Vec<(&str, i64)> = records
            .iter()
            .map(|record| (record.record_id.as_str(), record.revision))
            .collect();
        let canonical = canonical_basis_object(&selection_value, filter_value.as_ref(), &profiles, &source_set);
        let state_digest = basis_digest(&canonical)?;

        // The most recent surviving reflection pinned to exactly this
        // selection/filter: staleness compares against that pinned basis
        // (§7.26); with nothing published there is nothing to be stale
        // against, so `stale` is false.
        let tombstoned = tombstoned_record_ids(conn, space_id, subject_id).await?;
        let last = load_served_reflections(conn, space_id, subject_id)
            .await?
            .into_iter()
            .rev()
            .find(|row| {
                !source_set_tombstoned(&pinned_source_ids(&row.basis), &tombstoned)
                    && row.selection == selection_value
                    && row.filter.as_ref() == filter_value.as_ref()
            });
        let stale = match &last {
            Some(row) => pinned_state_digest(&row.basis)? != state_digest,
            None => false,
        };
        Ok(ReflectionStateView {
            basis: json!({
                "profile_kinds": canonical["profile_kinds"],
                "record_count": canonical["record_count"],
            }),
            sufficient: sufficient(source_set.len()),
            missing_required_profile_kinds: missing_required_profile_kinds(&profiles)?,
            stale,
            state_digest,
            last_reflection_id: last.map(|row| row.reflection_id),
        })
    }

    /// [`Self::reflection_list_in_tx`] on one read snapshot.
    pub(crate) async fn reflection_list(
        &self,
        principal_id: &str,
        request: &MemoryReflectionsListRequest,
        now: DateTime<Utc>,
    ) -> Result<ReflectionListPage, SpokeReject> {
        checked(validate_memory_reflections_list(request, MAX_PAGE_SIZE))?;
        let mut transaction = self
            .store()
            .pool()
            .begin()
            .await
            .map_err(internal_failure)?;
        let page = self
            .reflection_list_in_tx(&mut transaction, principal_id, request, now)
            .await?;
        transaction.commit().await.map_err(internal_failure)?;
        Ok(page)
    }

    /// [`Self::reflection_list`] on a caller-owned connection; never commits.
    pub(crate) async fn reflection_list_in_tx(
        &self,
        conn: &mut SqliteConnection,
        principal_id: &str,
        request: &MemoryReflectionsListRequest,
        now: DateTime<Utc>,
    ) -> Result<ReflectionListPage, SpokeReject> {
        let space_id = request.space_id.as_str();
        let subject_id = request.subject_id.as_str();
        let target = ReflectionTarget::from_list(request.selection.as_ref());
        self.require_reflection_admitted(
            conn,
            principal_id,
            "memory.reflections.list",
            space_id,
            subject_id,
            &target,
            request.cognition_filter.is_some(),
            now,
        )
        .await?;
        let status = subject_status(conn, space_id, subject_id).await?;
        let selection_value = request
            .selection
            .as_ref()
            .map(|selection| serde_json::to_value(selection).map_err(internal_failure))
            .transpose()?;
        let filter_value = request
            .filter
            .as_ref()
            .map(|filter| serde_json::to_value(filter).map_err(internal_failure))
            .transpose()?;
        let records = self.live_basis_records(
            conn,
            space_id,
            subject_id,
            &status,
            &target,
            filter_value.as_ref(),
        )
        .await?;
        let profiles = load_profile_revisions(conn, space_id, subject_id).await?;
        let tombstoned = tombstoned_record_ids(conn, space_id, subject_id).await?;

        // C-077 read-time exclusion plus the optional restriction: a supplied
        // selection (or filter) keeps exactly the reflections pinned to it.
        let served: Vec<ServedReflection> = load_served_reflections(conn, space_id, subject_id)
            .await?
            .into_iter()
            .filter(|row| {
                !source_set_tombstoned(&pinned_source_ids(&row.basis), &tombstoned)
                    && selection_value
                        .as_ref()
                        .is_none_or(|wanted| &row.selection == wanted)
                    && filter_value
                        .as_ref()
                        .is_none_or(|wanted| row.filter.as_ref() == Some(wanted))
            })
            .collect();

        // §7.0.7/MEM-R026: the cursor binds the caller, the query and the
        // provider snapshot marker plus the engine policy, so any grant or
        // state change invalidates it even when the page's rows did not move.
        let snapshot = snapshot_marker(conn, space_id).await?;
        let binding = match jcs_sha256_hex(&json!({
            "principal": principal_id,
            "op": "memory.reflections.list",
            "space": space_id,
            "subject": subject_id,
            "selection": selection_value,
            "filter": filter_value,
            "snapshot": snapshot,
            "policy": ENGINE_POLICY_VERSION,
        })) {
            SpokeResult::Ok(digest) => digest,
            SpokeResult::Reject(reject) => return Err(reject),
        };
        let start = list_cursor_start(request.cursor.as_deref().map(String::as_str), &binding, &served)?;

        let limit =
            usize::try_from(request.limit.map_or(MAX_PAGE_SIZE.get(), NonZeroU64::get))
                .map_err(internal_failure)?;
        let remaining = &served[start..];
        let page = &remaining[..remaining.len().min(limit)];
        let next_cursor = if remaining.len() > limit {
            page.last()
                .map(|row| format!("{binding}:{}", row.reflection_id))
        } else {
            None
        };
        let mut reflections = Vec::with_capacity(page.len());
        for row in page {
            // §7.26: authoritative staleness computed at read time against the
            // pinned basis; nothing stored is trusted for it.
            let digest =
                current_digest_for(&row.selection, row.filter.as_ref(), &records, &profiles)?;
            let stale = pinned_state_digest(&row.basis)? != digest;
            reflections.push(reflection_wire(row, stale)?);
        }
        Ok(ReflectionListPage {
            reflections,
            next_cursor,
        })
    }

    /// The live source set one read's digest is computed over: the subject's
    /// non-tombstoned records, emptied for an archived subject (§7.7/§11) and
    /// narrowed by the request's selection and provenance filter.
    async fn live_basis_records(
        &self,
        conn: &mut SqliteConnection,
        space_id: &str,
        subject_id: &str,
        status: &str,
        target: &ReflectionTarget,
        filter: Option<&Value>,
    ) -> Result<Vec<BasisRecord>, SpokeReject> {
        let mut records = load_basis_records(conn, space_id, subject_id).await?;
        if status == "archived" {
            records.clear();
        }
        records.retain(|record| {
            target_admits(target, &record.visibility, record.context_id.as_deref())
                && filter_admits(&record.provenance, filter)
        });
        Ok(records)
    }
}

// ---------------------------------------------------------------------------
// Generate: `memory.reflections.generate` — two-transaction pending→terminal
// receipt flow with the GenerationPause/GenerationFailure fault points
// ---------------------------------------------------------------------------

/// The fixture's content-free generation cost: one published-pattern unit kind
/// charged exactly once, on the terminal committed receipt (C-078). The kind
/// is validated through the published pattern types at the only place one is
/// minted; the reflection's `extensions` stay the empty map, so no extension
/// key is ever invented here.
const GENERATION_COST_UNIT_KIND: &str = "fixture_reflection_unit";
const GENERATION_COST_UNITS: i64 = 1;

/// §14.3: the caller cannot tell whether the mutation committed.
fn outcome_uncertain() -> SpokeReject {
    SpokeReject {
        code: SpokeRejectCode::OutcomeUncertain,
        message: "the mutation's commit state is unknown; resolve with the same idempotency key \
                  or memory.receipts.get"
            .to_owned(),
        details: None,
    }
}

/// A commit that failed may still have taken effect (§14.3).
fn commit_unknown() -> SpokeReject {
    SpokeReject {
        code: SpokeRejectCode::OutcomeUncertain,
        message: "the transaction's commit state is unknown; resolve with the same idempotency \
                  key or memory.receipts.get"
            .to_owned(),
        details: None,
    }
}

/// A fixture fault that ended before this operation committed anything. §14.2
/// allows the retryable internal class only when no effect committed, which
/// holds at every site this is raised.
fn fault_before_effect(point: FaultPoint) -> SpokeReject {
    SpokeReject {
        code: SpokeRejectCode::InternalError,
        message: format!("fixture fault hold at {point:?} ended before any effect was committed"),
        details: None,
    }
}

fn insufficient_data() -> SpokeReject {
    SpokeReject {
        code: SpokeRejectCode::InsufficientData,
        message: "no authorized source records for the pinned selection".to_owned(),
        details: None,
    }
}

fn revision_conflict() -> SpokeReject {
    SpokeReject {
        code: SpokeRejectCode::RevisionConflict,
        message: "expected_state_digest does not match the current basis".to_owned(),
        details: None,
    }
}

/// §7.27 admission, fail-closed: the required-profile and sufficiency
/// preconditions, in the local provider's order (profiles first, then
/// sufficiency, then the CAS digest). Returns the live source records and the
/// profile revisions the committed basis pins.
#[allow(clippy::too_many_arguments)]
async fn check_generate_policy(
    conn: &mut SqliteConnection,
    space_id: &str,
    subject_id: &str,
    status: &str,
    target: &ReflectionTarget,
    selection_value: &Value,
    filter: Option<&Value>,
    expected_state_digest: &str,
) -> Result<(Vec<BasisRecord>, Vec<(String, i64)>), SpokeReject> {
    let profiles = load_profile_revisions(conn, space_id, subject_id).await?;
    let missing = missing_required_profile_kinds(&profiles)?;
    if !missing.is_empty() {
        return Err(SpokeReject {
            code: SpokeRejectCode::ProfileIncomplete,
            message: format!("required profile kinds absent: {missing:?}"),
            details: Some(Map::from_iter([("missing".to_owned(), json!(missing))])),
        });
    }
    let records = load_basis_records(conn, space_id, subject_id).await?;
    let mut records: Vec<BasisRecord> = if status == "archived" {
        Vec::new()
    } else {
        records
    };
    records.retain(|record| {
        target_admits(target, &record.visibility, record.context_id.as_deref())
            && filter_admits(&record.provenance, filter)
    });
    if !sufficient(records.len()) {
        return Err(insufficient_data());
    }
    let source_set: Vec<(&str, i64)> = records
        .iter()
        .map(|record| (record.record_id.as_str(), record.revision))
        .collect();
    let canonical = canonical_basis_object(selection_value, filter, &profiles, &source_set);
    if basis_digest(&canonical)? != expected_state_digest {
        return Err(revision_conflict());
    }
    Ok((records, profiles))
}

/// What transaction 1 of a generate run decided.
enum PendingGenerate {
    /// The identical admitted replay: return the stored receipt, no effect.
    Replayed(Box<MemoryReceipt>),
    /// The pending receipt is staged on the caller's uncommitted transaction.
    Staged(Box<StagedGenerate>),
}

/// The facts transaction 2 needs about the staged pending receipt.
struct StagedGenerate {
    space_id: String,
    subject_id: String,
    receipt_id: String,
    filter_value: Option<Value>,
    records: Vec<BasisRecord>,
    profiles: Vec<(String, i64)>,
}

impl FixtureMemoryProvider {
    /// Transaction 1 of [`Self::reflection_generate`], on the caller's
    /// uncommitted transaction: current authorization, replay resolution
    /// (with the expired-key release), the §7.27 policy + `expected_state_digest`
    /// CAS, then the pending receipt staging. Never commits.
    async fn stage_pending_generate_in_tx(
        &self,
        conn: &mut SqliteConnection,
        principal_id: &str,
        op: &'static str,
        admitted: &Admitted<MemoryReflectionsGenerateRequest>,
        now: DateTime<Utc>,
    ) -> Result<PendingGenerate, SpokeReject> {
        let request = admitted.request();
        let space_id = request.space_id.as_str();
        let subject_id = request.subject_id.as_str();
        let target = ReflectionTarget::from_generate(&request.selection);
        self.require_reflection_admitted(
            conn,
            principal_id,
            op,
            space_id,
            subject_id,
            &target,
            request.cognition_filter.is_some(),
            now,
        )
        .await?;
        let mutation = Mutation::keyed(op, request.idempotency_key.as_str(), admitted);
        match resolve_replay(conn, space_id, principal_id, &mutation, now).await? {
            ReplayResolution::Replayed(receipt) => return Ok(PendingGenerate::Replayed(receipt)),
            ReplayResolution::KeyReusable { receipt_id } => {
                release_expired_binding_in_tx(conn, space_id, principal_id, &mutation, &receipt_id)
                    .await?;
            }
            ReplayResolution::Fresh => {}
        }
        let status = subject_status(conn, space_id, subject_id).await?;
        let selection_value =
            serde_json::to_value(&request.selection).map_err(internal_failure)?;
        let filter_value = request
            .filter
            .as_ref()
            .map(|filter| serde_json::to_value(filter).map_err(internal_failure))
            .transpose()?;
        let (records, profiles) = check_generate_policy(
            conn,
            space_id,
            subject_id,
            &status,
            &target,
            &selection_value,
            filter_value.as_ref(),
            request.expected_state_digest.as_str(),
        )
        .await?;
        let capabilities = required_capabilities(op)
            .ok_or_else(|| internal(format!("{op} has no published capability list")))?;
        let contexts: Vec<&str> = target.context_ids.iter().map(String::as_str).collect();
        let receipt_target = ReceiptTarget::new(
            capabilities,
            Some(subject_id),
            &contexts,
            target.reads_shared,
            target.subject_all,
        )?;
        let receipt_id = receipt_id();
        stage_receipt_in_tx(
            conn,
            space_id,
            principal_id,
            &mutation,
            &receipt_id,
            &receipt_target,
            ReceiptState::Pending,
            now,
        )
        .await?;
        Ok(PendingGenerate::Staged(Box::new(StagedGenerate {
            space_id: space_id.to_owned(),
            subject_id: subject_id.to_owned(),
            receipt_id,
            filter_value,
            records,
            profiles,
        })))
    }

    /// [`Self::reflection_generate`] on the whole provider: the fault points
    /// bracket both transactions of the two-transaction flow.
    pub(crate) async fn reflection_generate(
        &self,
        principal_id: &str,
        admitted: &Admitted<MemoryReflectionsGenerateRequest>,
        now: DateTime<Utc>,
    ) -> Result<MemoryReceipt, SpokeReject> {
        let request = admitted.request();
        checked(validate_memory_reflections_generate(request))?;
        // Bounded test-only admission pause before the writer transaction: a
        // competing identical admission can commit terminally while this one
        // waits, and this one then replays that terminal receipt (C-070/C-076).
        self.faults()
            .hold(FaultPoint::AdmissionPause)
            .await
            .map_err(|_| fault_before_effect(FaultPoint::AdmissionPause))?;

        // Transaction 1: current authorization, replay resolution, §7.27
        // policy + `expected_state_digest` CAS, then the pending receipt.
        let mut transaction = self.begin_write().await?;
        let pending = match self
            .stage_pending_generate_in_tx(
                &mut transaction,
                principal_id,
                "memory.reflections.generate",
                admitted,
                now,
            )
            .await?
        {
            PendingGenerate::Replayed(receipt) => {
                let _ = transaction.rollback().await;
                return Ok(*receipt);
            }
            PendingGenerate::Staged(pending) => pending,
        };
        // Pre-commit loss: dropping the staged transaction leaves no pending
        // receipt, dedup row or target behind.
        self.faults()
            .hold(FaultPoint::BeforeCommit)
            .await
            .map_err(|_| fault_before_effect(FaultPoint::BeforeCommit))?;
        if self.faults().trip(FaultPoint::BeforeCommit) {
            transaction.rollback().await.map_err(internal_failure)?;
            return Err(outcome_uncertain());
        }
        transaction.commit().await.map_err(|_| commit_unknown())?;
        // Armed before the operation: the pending receipt is durable and the
        // generation never ran (alternate C-015 shape).
        if self.faults().trip(FaultPoint::AfterCommitBeforeResponse) {
            return Err(outcome_uncertain());
        }
        // Bounded hold with the pending receipt observable (`status = pending`
        // over `memory.receipts.get`); it is also the arming window that
        // targets transaction 2 with the one-shot points.
        self.faults()
            .hold(FaultPoint::GenerationPause)
            .await
            .map_err(|_| outcome_uncertain())?;

        // The fixture-deterministic generation step (C-078 one-shot fail):
        // an honest failure is a terminal `failed` receipt with an error
        // envelope and retry class, and no cost is ever charged for it.
        let generation_failed = self.faults().trip(FaultPoint::GenerationFailure);

        // Transaction 2: settle the pending receipt to exactly one terminal
        // outcome (the reflection row commits with it, MEM-R009). An abort
        // here leaves the pending receipt recoverable; the
        // never-return-to-pending rule keeps the settle one-way.
        let mut transaction = self.begin_write().await?;
        let terminal = if generation_failed {
            ReceiptState::Failed(internal("the fixture's deterministic generation step failed"))
        } else {
            let reflection_id = format!("refl_{}", Uuid::new_v4().simple());
            insert_generated_reflection(
                &mut transaction,
                &pending.space_id,
                &pending.subject_id,
                principal_id,
                request,
                pending.filter_value.as_ref(),
                &pending.records,
                &pending.profiles,
                now,
                &reflection_id,
            )
            .await?;
            ReceiptState::Committed(json!({
                "reflection_id": reflection_id,
                "cost": {
                    "unit_kind": GENERATION_COST_UNIT_KIND,
                    "units": GENERATION_COST_UNITS,
                },
            }))
        };
        let receipt =
            match settle_receipt_in_tx(&mut transaction, &pending.space_id, &pending.receipt_id, terminal, now).await
            {
                Ok(receipt) => receipt,
                Err(reject) => {
                    let _ = transaction.rollback().await;
                    return Err(reject);
                }
            };
        self.faults()
            .hold(FaultPoint::BeforeCommit)
            .await
            .map_err(|_| outcome_uncertain())?;
        if self.faults().trip(FaultPoint::BeforeCommit) {
            transaction.rollback().await.map_err(internal_failure)?;
            return Err(outcome_uncertain());
        }
        transaction.commit().await.map_err(|_| commit_unknown())?;
        // Armed during the generation pause: the terminal receipt is durable
        // but the response is lost; a same-key replay returns it (C-070/C-071).
        if self.faults().trip(FaultPoint::AfterCommitBeforeResponse) {
            return Err(outcome_uncertain());
        }
        self.faults()
            .hold(FaultPoint::ResponseDrop)
            .await
            .map_err(|_| outcome_uncertain())?;
        Ok(receipt)
    }
}

/// Insert the immutable reflection row a committed generate run publishes, on
/// the caller's transaction 2 (MEM-R009: the row and its terminal receipt
/// commit together). The pinned basis carries exactly the four published
/// `ReflectionBasis` members plus the fixture-private `source_ids`; the body
/// is the fixture's deterministic text, never an emulated model answer.
#[allow(clippy::too_many_arguments)]
async fn insert_generated_reflection(
    conn: &mut SqliteConnection,
    space_id: &str,
    subject_id: &str,
    principal_id: &str,
    request: &MemoryReflectionsGenerateRequest,
    filter: Option<&Value>,
    records: &[BasisRecord],
    profiles: &[(String, i64)],
    now: DateTime<Utc>,
    reflection_id: &str,
) -> Result<(), SpokeReject> {
    let selection_value = serde_json::to_value(&request.selection).map_err(internal_failure)?;
    let source_set: Vec<(&str, i64)> = records
        .iter()
        .map(|record| (record.record_id.as_str(), record.revision))
        .collect();
    let canonical =
        canonical_basis_object(&selection_value, filter, profiles, &source_set);
    let state_digest = basis_digest(&canonical)?;
    let source_set_digest = basis_digest(&json!({ "source_set": source_set }))?;
    let source_ids: Vec<String> = records
        .iter()
        .map(|record| record.record_id.clone())
        .collect();
    let mut profile_revisions = Map::new();
    for (kind, revision) in profiles {
        profile_revisions.insert(kind.clone(), json!(revision));
    }
    let basis = json!({
        "source_set_digest": source_set_digest,
        "record_count": source_set.len(),
        "profile_revisions": profile_revisions,
        "state_digest": state_digest,
        PINNED_SOURCE_IDS_KEY: source_ids,
    });
    // The published pattern types validate the only cost kind this fixture
    // mints; the extensions stay the empty map, so no extension key is minted.
    if ReflectionCostUnitKind::from_str(GENERATION_COST_UNIT_KIND).is_err()
        || ExtensionKey::from_str(GENERATION_COST_UNIT_KIND).is_err()
    {
        return Err(internal(
            "the generated cost unit kind is not publishable under the protocol patterns",
        ));
    }
    let kinds: Vec<&str> = profiles.iter().map(|(kind, _)| kind.as_str()).collect();
    let body_text = format!(
        "Fixture reflection over {} authorized source record(s) (profile kinds: {}); \
         the pinned basis digest is {}.",
        source_set.len(),
        kinds.join(", "),
        state_digest,
    );
    sqlx::query(
        "INSERT INTO reflections (reflection_id, space_id, subject_id, \
         requester_principal_id, selection_json, filter_json, cognition_filter_json, \
         basis_json, body_text, body_format, cost_unit_kind, cost_units, \
         engine_policy_version, model, schema_version, revision, extensions_json, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(reflection_id)
    .bind(space_id)
    .bind(subject_id)
    .bind(principal_id)
    .bind(selection_value.to_string())
    .bind(filter.map(std::string::ToString::to_string))
    .bind(
        request
            .cognition_filter
            .as_ref()
            .map(|filter| serde_json::to_string(filter).map_err(internal_failure))
            .transpose()?,
    )
    .bind(basis.to_string())
    .bind(&body_text)
    .bind("markdown")
    .bind(GENERATION_COST_UNIT_KIND)
    .bind(GENERATION_COST_UNITS)
    .bind(ENGINE_POLICY_VERSION)
    .bind(Option::<String>::None)
    .bind(1_i64)
    .bind(1_i64)
    .bind("{}")
    .bind(now.to_rfc3339_opts(SecondsFormat::Micros, true))
    .execute(&mut *conn)
    .await
    .map_err(internal_failure)?;
    // One bump covers both the new row and its terminal receipt: they commit
    // in this same transaction, and cursors bind to this marker (MEM-R026).
    bump_snapshot(conn, space_id).await
}

/// Restart recovery (§8.5): a generate receipt left pending by a lost session
/// can never complete — the fixture's generation is in-process state — so
/// reopening the database terminalizes every orphaned pending generate
/// receipt `failed` (retryable `INTERNAL_ERROR`, no cost). The same rule the
/// local provider applies, so cross-provider rows observe identical
/// semantics.
pub(crate) async fn terminalize_orphaned_generate_receipts(
    store: &FixtureStore,
) -> Result<(), FixtureError> {
    let pool = store.pool();
    let orphans = sqlx::query(
        "SELECT space_id, receipt_id FROM receipts \
         WHERE op = 'memory.reflections.generate' AND status = 'pending'",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| {
        Ok((
            row.try_get::<String, _>("space_id")?,
            row.try_get::<String, _>("receipt_id")?,
        ))
    })
    .collect::<Result<Vec<_>, sqlx::Error>>()?;
    for (space_id, receipt_id) in orphans {
        let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
        let settled = settle_receipt_in_tx(
            &mut transaction,
            &space_id,
            &receipt_id,
            ReceiptState::Failed(internal(
                "the provider restarted before the reflection generation completed",
            )),
            Utc::now(),
        )
        .await;
        match settled {
            Ok(_) => transaction.commit().await?,
            Err(reject) => {
                return Err(FixtureError::Bootstrap(format!(
                    "orphaned generate receipt {receipt_id} could not be terminalized: {}",
                    reject.message
                )))
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Admission-rule tests (the §6.5 facet matrix and the honest refusals; the
// serving selectors that drive these rules over loopback land in Task 6)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use spoke_operations::spoke_schemas::MemoryGrant;
    use spoke_operations::SpokeRejectCode;

    fn now() -> DateTime<Utc> {
        Utc::now()
    }

    fn grant_from(value: serde_json::Value) -> MemoryGrant {
        serde_json::from_value(value).expect("test grant deserializes")
    }

    fn restricted(capabilities: &[&str], context_ids: &[&str], shared_access: bool) -> MemoryGrant {
        grant_from(json!({
            "grant_id": "g-1",
            "principal_id": "client",
            "space_id": "space-one",
            "subject_id": null,
            "capabilities": capabilities,
            "context_ids": context_ids,
            "shared_access": shared_access,
            "created_at": "2026-01-01T00:00:00Z",
            "revision": 1,
            "schema_version": 1,
            "extensions": {},
        }))
    }

    fn unrestricted(capabilities: &[&str]) -> MemoryGrant {
        grant_from(json!({
            "grant_id": "g-2",
            "principal_id": "client",
            "space_id": "space-one",
            "subject_id": null,
            "capabilities": capabilities,
            "created_at": "2026-01-01T00:00:00Z",
            "revision": 1,
            "schema_version": 1,
            "extensions": {},
        }))
    }

    fn admits(grants: &[MemoryGrant], target: &ReflectionTarget) -> bool {
        is_authorized(
            grants,
            "client",
            "space-one",
            now(),
            &target.requirement("subject-one"),
        )
    }

    fn selection_all() -> ReflectionTarget {
        ReflectionTarget::from_state(&StateSelection::SubjectAll)
    }

    fn selection_contexts(contexts: &[&str], include_shared: bool) -> ReflectionTarget {
        ReflectionTarget::from_state(&StateSelection::SelectedContexts {
            context_ids: contexts.iter().map(|c| (*c).to_owned()).collect(),
            include_shared: Some(include_shared),
        })
    }

    fn selection_shared_only() -> ReflectionTarget {
        ReflectionTarget::from_state(&StateSelection::SharedOnly)
    }

    /// C-075: `subject_all` needs an unrestricted grant; a context-limited or
    /// shared-only grant never covers it.
    #[test]
    fn c_075_subject_all_requires_unrestricted_grant() {
        assert!(admits(&[unrestricted(&["reflect"])], &selection_all()));
        // `admin` implies `reflect` (§6.5).
        assert!(admits(&[unrestricted(&["admin"])], &selection_all()));
        // An empty `context_ids` restriction is no restriction (the published
        // evaluator reads it as full coverage), so the negative cases here are
        // context-limited grants.
        for narrow in [
            restricted(&["reflect"], &["ctx-a"], false),
            restricted(&["reflect"], &["ctx-a"], true),
        ] {
            assert!(
                !admits(&[narrow], &selection_all()),
                "a restricted grant must not cover subject_all"
            );
        }
    }

    /// `selected_contexts` needs coverage of every listed context; a shared
    /// read needs the shared-access facet.
    #[test]
    fn selected_contexts_need_full_coverage_and_shared_reads_need_shared_access() {
        let both = selection_contexts(&["ctx-a", "ctx-b"], false);
        assert!(admits(
            &[restricted(&["reflect"], &["ctx-a", "ctx-b"], false)],
            &both
        ));
        assert!(!admits(
            &[restricted(&["reflect"], &["ctx-a"], false)],
            &both
        ));

        let with_shared = selection_contexts(&["ctx-a"], true);
        assert!(admits(
            &[restricted(&["reflect"], &["ctx-a"], true)],
            &with_shared
        ));
        assert!(!admits(
            &[restricted(&["reflect"], &["ctx-a"], false)],
            &with_shared
        ));

        assert!(admits(
            &[restricted(&["reflect"], &[], true)],
            &selection_shared_only()
        ));
        assert!(!admits(
            &[restricted(&["reflect"], &["ctx-a"], false)],
            &selection_shared_only()
        ));
    }

    /// The list slice's absent selection is the whole-subject requirement.
    #[test]
    fn absent_list_selection_is_subject_all() {
        assert!(admits(
            &[unrestricted(&["reflect"])],
            &ReflectionTarget::from_list(None)
        ));
        assert!(!admits(
            &[restricted(&["reflect"], &["ctx-a"], true)],
            &ReflectionTarget::from_list(None)
        ));
    }

    /// A present `cognition_filter` is an honest `INVALID_INPUT` refusal.
    #[test]
    fn cognition_filter_is_refused_invalid_input() {
        let refusal = cognition_filter_unsupported();
        assert_eq!(refusal.code, SpokeRejectCode::InvalidInput);
        assert_eq!(
            refusal
                .details
                .as_ref()
                .and_then(|d| d.get("field"))
                .and_then(serde_json::Value::as_str),
            Some("cognition_filter")
        );
    }

    /// §8.2 classification: `state`/`list` are allowlisted reads; `generate`
    /// is not, so only generate refuses `SPACE_FROZEN`.
    #[test]
    fn freeze_classification_admits_reads_only() {
        assert!(is_freeze_allowlisted("memory.reflections.state"));
        assert!(is_freeze_allowlisted("memory.reflections.list"));
        assert!(!is_freeze_allowlisted("memory.reflections.generate"));
    }

    // -- read-op helpers ------------------------------------------------------

    fn profiles() -> Vec<(String, i64)> {
        vec![("personality".to_owned(), 2)]
    }

    fn digest_of(records: &[(&str, i64)], filter: Option<&Value>) -> String {
        let selection = json!({ "mode": "subject_all" });
        basis_digest(&canonical_basis_object(&selection, filter, &profiles(), records))
            .expect("canonical digest computes")
    }

    /// C-042's mechanism: every basis-affecting fact moves the digest.
    #[test]
    fn canonical_digest_tracks_every_basis_fact() {
        let base = digest_of(&[("r-1", 1)], None);
        assert_ne!(base, digest_of(&[("r-1", 2)], None), "record revision");
        assert_ne!(base, digest_of(&[("r-1", 1), ("r-2", 1)], None), "record set");
        let selection = json!({ "mode": "subject_all" });
        let filtered = Some(json!({ "source_context_ids": ["ctx-a"] }));
        assert_ne!(base, {
            let digest = basis_digest(&canonical_basis_object(
                &selection,
                filtered.as_ref(),
                &profiles(),
                &[("r-1", 1)],
            ));
            digest.expect("canonical digest computes")
        }, "filter");
        let mut profiles = profiles();
        profiles[0].1 = 3;
        let other = basis_digest(&canonical_basis_object(
            &selection,
            None,
            &profiles,
            &[("r-1", 1)],
        ));
        assert_ne!(base, other.expect("canonical digest computes"), "profile revision");
    }

    /// The digest input is total: the same facts in the same order produce the
    /// same digest regardless of call ordering.
    #[test]
    fn canonical_digest_is_deterministic() {
        assert_eq!(
            digest_of(&[("r-1", 1), ("r-2", 2)], None),
            digest_of(&[("r-1", 1), ("r-2", 2)], None),
        );
    }

    /// C-077: only a fully tombstoned non-empty source set is excluded; a
    /// partial tombstone stays listed (already stale) and an empty pin never
    /// excludes.
    #[test]
    fn whole_pinned_source_set_tombstones_the_row() {
        let tombstoned = ["r-1".to_owned(), "r-2".to_owned()];
        assert!(source_set_tombstoned(&["r-1".into(), "r-2".into()], &tombstoned));
        // Partial tombstone: the pinned set still has a live source.
        assert!(!source_set_tombstoned(
            &["r-1".into(), "r-2".into(), "r-3".into()],
            &tombstoned
        ));
        assert!(!source_set_tombstoned(&[], &tombstoned));
    }

    /// Placement parity with recall, via the stored-selection resolver.
    #[test]
    fn stored_selection_resolves_to_recall_parity() {
        let admits = |selection: &Value, visibility: &str, context: Option<&str>| {
            let target = target_from_value(selection).expect("selection resolves");
            target_admits(&target, visibility, context)
        };
        let all = json!({ "mode": "subject_all" });
        let shared = json!({ "mode": "shared_only" });
        let contexts = json!({ "mode": "selected_contexts", "context_ids": ["ctx-a"] });
        let with_shared =
            json!({ "mode": "selected_contexts", "context_ids": ["ctx-a"], "include_shared": true });
        assert!(admits(&all, "shared", Some("ctx-b")));
        assert!(admits(&all, "context", Some("ctx-b")));
        assert!(admits(&shared, "shared", None));
        assert!(!admits(&shared, "context", Some("ctx-a")));
        assert!(admits(&contexts, "context", Some("ctx-a")));
        assert!(!admits(&contexts, "context", Some("ctx-b")));
        assert!(!admits(&contexts, "shared", None));
        assert!(admits(&with_shared, "shared", None));
    }
}
