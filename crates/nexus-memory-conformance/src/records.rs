//! Retained records: create/get/update/delete/list/promote
//! (spoke-memory §§6.6, 7.16–7.21, MEM-R007/R011/R014/R026).
//!
//! A record is the durable authored object: untrusted content, write placement
//! in its own columns, and provenance that is independent of placement. Every
//! mutation is split into an outer `*` convenience method that opens one
//! `BEGIN IMMEDIATE` transaction and a `*_in_tx` sibling that stages the same
//! authorized effect on a caller-owned connection without committing, so the
//! assembly task's single effect + receipt/dedup commit can host these effects
//! (MEM-R022). Reads are split the same way: the outer method opens one read
//! transaction and the `*_in_tx` sibling pairs authorization with the rows and
//! the cursor marker inside the caller's snapshot.
//!
//! Every operation authorizes at the single admission instant the caller
//! supplies; no operation samples the wall clock. The requester is always the
//! admission-supplied principal id: the payload never selects it, and the
//! provider — not the client — stamps `captured_by`/`captured_at`.
//!
//! Refusals are constructed in this slice because the frozen spaces slice
//! keeps its constructors module-private; codes and `details` shapes are the
//! published protocol ones. Receipt bodies and idempotent replay are absent by
//! design: the receipt slice owns them, and these methods return only the
//! content-free result each op reports (MEM-R022).

use std::num::NonZeroU64;

use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{json, Map, Value};
use sqlx::{Row, SqliteConnection};
use uuid::Uuid;

use spoke_operations::spoke_schemas::memory_records_create_request::PlacementScope as CreateScope;
use spoke_operations::spoke_schemas::memory_records_list_request::ReadSelection;
use spoke_operations::spoke_schemas::memory_records_promote_request::PlacementScope as PromoteScope;
use spoke_operations::spoke_schemas::memory_records_update_request::MemoryRecordsUpdateRequest;
use spoke_operations::spoke_schemas::retained_record::{ContextId, PlacementScope};
use spoke_operations::spoke_schemas::{
    MemoryRecordsCreateRequest, MemoryRecordsDeleteRequest, MemoryRecordsGetRequest,
    MemoryRecordsListRequest, MemoryRecordsPromoteRequest, RetainedRecord,
};
use spoke_operations::{
    grant_covers, jcs_sha256_hex, validate_memory_records_create, validate_memory_records_list,
    validate_memory_records_update, GrantRequirement, SpokeReject, SpokeRejectCode, SpokeResult,
};

use crate::provider::FixtureMemoryProvider;
use crate::spaces::{
    bump_snapshot, internal, internal_failure, is_authorized, load_grants, snapshot_marker,
    whole_scope_requirement, ENGINE_POLICY_VERSION, MAX_PAGE_SIZE,
};

/// Wire schema version carried by every fixture-served object (§6.1).
const SCHEMA_VERSION: u64 = 1;

/// Provider content bound (`memory.describe.limits.max_content_bytes`, §7.1);
/// the assembly task advertises this same value.
pub const MAX_CONTENT_BYTES: usize = 65_536;

/// Content-free result of `memory.records.create` / `memory.records.update`
/// (§7.16/§7.18).
#[derive(Debug)]
pub struct RecordRevision {
    pub(crate) record_id: String,
    pub(crate) revision: u64,
}

/// Content-free result of `memory.records.delete` (§7.19).
#[derive(Debug)]
pub struct RecordDeleted {
    pub(crate) record_id: String,
    pub(crate) tombstoned: bool,
}

/// Content-free result of `memory.records.promote` (§7.21).
#[derive(Debug)]
pub struct RecordPromoted {
    pub(crate) record_id: String,
    pub(crate) revision: u64,
    pub(crate) scope: PlacementScope,
}

/// `memory.records.list` payload (§7.20).
#[derive(Debug)]
pub struct RecordPage {
    pub(crate) records: Vec<RetainedRecord>,
    pub(crate) total_authorized: u64,
    pub(crate) next_cursor: Option<String>,
}

// ---------------------------------------------------------------------------
// Protocol errors
// ---------------------------------------------------------------------------

fn not_found(message: impl Into<String>) -> SpokeReject {
    SpokeReject {
        code: SpokeRejectCode::NotFound,
        message: message.into(),
        details: None,
    }
}

fn not_authorized(message: impl Into<String>) -> SpokeReject {
    SpokeReject {
        code: SpokeRejectCode::NotAuthorized,
        message: message.into(),
        details: None,
    }
}

/// The single public refusal for a record the caller cannot see: an absent id,
/// a placement outside the caller's grants, a deleted record and a tombstone
/// hidden from non-admin all answer this same `NOT_FOUND`, so by-id probes
/// cannot distinguish existence (matrix C-102, MEM-R026). The caller-supplied
/// id is deliberately not echoed back, keeping the refusal byte-identical.
fn record_absent() -> SpokeReject {
    not_found("the requested record does not exist in this space")
}

fn space_frozen(space_id: &str) -> SpokeReject {
    SpokeReject {
        code: SpokeRejectCode::SpaceFrozen,
        message: format!(
            "space {space_id} is frozen; record mutations are outside the §8.2 allowlist"
        ),
        details: None,
    }
}

fn subject_fenced(subject_id: &str, scope: &str) -> SpokeReject {
    let mut details = Map::new();
    details.insert("scope".to_owned(), Value::String(scope.to_owned()));
    SpokeReject {
        code: SpokeRejectCode::SubjectFenced,
        message: format!("subject {subject_id} is fenced for writes ({scope} scope)"),
        details: Some(details),
    }
}

fn revision_conflict(expected: NonZeroU64, actual: u64) -> SpokeReject {
    let mut details = Map::new();
    details.insert("expected_revision".to_owned(), json!(expected.get()));
    details.insert("actual_revision".to_owned(), json!(actual));
    SpokeReject {
        code: SpokeRejectCode::RevisionConflict,
        message: format!(
            "expected revision {} does not match the stored revision {actual}",
            expected.get()
        ),
        details: Some(details),
    }
}

fn cursor_invalid(message: impl Into<String>) -> SpokeReject {
    SpokeReject {
        code: SpokeRejectCode::CursorInvalid,
        message: message.into(),
        details: None,
    }
}

/// Flatten the published validator envelope into the crate's result shape.
fn checked(result: SpokeResult<()>) -> Result<(), SpokeReject> {
    match result {
        SpokeResult::Ok(()) => Ok(()),
        SpokeResult::Reject(reject) => Err(reject),
    }
}

// ---------------------------------------------------------------------------
// Stored row and conversions
// ---------------------------------------------------------------------------

struct StoredRecord {
    record_id: String,
    space_id: String,
    subject_id: String,
    kind: String,
    content_json: String,
    scope_visibility: String,
    scope_context_id: Option<String>,
    provenance_json: String,
    import_ref_json: Option<String>,
    occurred_at: Option<DateTime<Utc>>,
    tombstone_json: Option<String>,
    revision: u64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    extensions_json: String,
}

fn timestamp(raw: &str) -> Result<DateTime<Utc>, SpokeReject> {
    DateTime::parse_from_rfc3339(raw)
        .map(|parsed| parsed.with_timezone(&Utc))
        .map_err(internal_failure)
}

fn timestamp_text(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Micros, true)
}

fn optional_timestamp(raw: Option<String>) -> Result<Option<DateTime<Utc>>, SpokeReject> {
    raw.map(|value| timestamp(&value)).transpose()
}

fn json_field(raw: &str, field: &str) -> Result<Value, SpokeReject> {
    serde_json::from_str(raw).map_err(|_| internal(format!("stored {field} is not valid JSON")))
}

fn row_u64(row: &sqlx::sqlite::SqliteRow, column: &str) -> Result<u64, SpokeReject> {
    let raw: i64 = row.try_get(column).map_err(internal_failure)?;
    u64::try_from(raw).map_err(internal_failure)
}

fn row_i64(value: u64) -> Result<i64, SpokeReject> {
    i64::try_from(value).map_err(internal_failure)
}

fn scope_json(visibility: &str, context_id: Option<&str>) -> Value {
    match (visibility, context_id) {
        ("shared", _) => json!({ "visibility": "shared" }),
        ("context", Some(context_id)) => {
            json!({ "visibility": "context", "context_id": context_id })
        }
        _ => json!({ "visibility": visibility }),
    }
}

fn wire_placement(
    visibility: &str,
    context_id: Option<&str>,
) -> Result<PlacementScope, SpokeReject> {
    match (visibility, context_id) {
        ("shared", None) => Ok(PlacementScope::Shared),
        ("context", Some(context_id)) => Ok(PlacementScope::Context(
            ContextId::try_from(context_id).map_err(internal_failure)?,
        )),
        _ => Err(internal(format!(
            "stored record placement {visibility}/{context_id:?} is not protocol-shaped"
        ))),
    }
}

/// One stored row as the published `RetainedRecord` variant it describes.
fn record_to_wire(record: &StoredRecord) -> Result<RetainedRecord, SpokeReject> {
    let mut value = json!({
        "schema_version": SCHEMA_VERSION,
        "record_id": record.record_id,
        "space_id": record.space_id,
        "subject_id": record.subject_id,
        "kind": record.kind,
        "content": json_field(&record.content_json, "record content")?,
        "scope": scope_json(&record.scope_visibility, record.scope_context_id.as_deref()),
        "provenance": json_field(&record.provenance_json, "record provenance")?,
        "created_at": timestamp_text(record.created_at),
        "updated_at": timestamp_text(record.updated_at),
        "revision": record.revision,
        "extensions": json_field(&record.extensions_json, "record extensions")?,
    });
    if let Some(occurred_at) = record.occurred_at {
        value["occurred_at"] = json!(timestamp_text(occurred_at));
    }
    match &record.tombstone_json {
        // A tombstoned record keeps its identity and scope but carries the
        // tombstone instead of import attribution (MEM-R014); the published
        // tombstoned variant has no import_ref field.
        Some(raw) => value["tombstone"] = json_field(raw, "record tombstone")?,
        None => {
            if let Some(raw) = &record.import_ref_json {
                value["import_ref"] = json_field(raw, "record import ref")?;
            }
        }
    }
    serde_json::from_value(value).map_err(internal_failure)
}

fn content_json(text: &str, structured: Option<&Value>) -> Result<String, SpokeReject> {
    let mut value = json!({ "text": text });
    if let Some(structured) = structured {
        value["structured"] = structured.clone();
    }
    serde_json::to_string(&value).map_err(internal_failure)
}

async fn fetch_record(
    conn: &mut SqliteConnection,
    space_id: &str,
    record_id: &str,
) -> Result<StoredRecord, SpokeReject> {
    let row = sqlx::query(
        "SELECT record_id, space_id, subject_id, kind, content_json, scope_visibility, \
         scope_context_id, provenance_json, import_ref_json, occurred_at, tombstone_json, \
         revision, created_at, updated_at, extensions_json \
         FROM records WHERE space_id = ? AND record_id = ?",
    )
    .bind(space_id)
    .bind(record_id)
    .fetch_optional(conn)
    .await
    .map_err(internal_failure)?
    .ok_or_else(record_absent)?;
    Ok(StoredRecord {
        record_id: row.try_get("record_id").map_err(internal_failure)?,
        space_id: row.try_get("space_id").map_err(internal_failure)?,
        subject_id: row.try_get("subject_id").map_err(internal_failure)?,
        kind: row.try_get("kind").map_err(internal_failure)?,
        content_json: row.try_get("content_json").map_err(internal_failure)?,
        scope_visibility: row.try_get("scope_visibility").map_err(internal_failure)?,
        scope_context_id: row.try_get("scope_context_id").map_err(internal_failure)?,
        provenance_json: row.try_get("provenance_json").map_err(internal_failure)?,
        import_ref_json: row.try_get("import_ref_json").map_err(internal_failure)?,
        occurred_at: optional_timestamp(row.try_get("occurred_at").map_err(internal_failure)?)?,
        tombstone_json: row.try_get("tombstone_json").map_err(internal_failure)?,
        revision: row_u64(&row, "revision")?,
        created_at: timestamp(
            &row.try_get::<String, _>("created_at")
                .map_err(internal_failure)?,
        )?,
        updated_at: timestamp(
            &row.try_get::<String, _>("updated_at")
                .map_err(internal_failure)?,
        )?,
        extensions_json: row.try_get("extensions_json").map_err(internal_failure)?,
    })
}

// ---------------------------------------------------------------------------
// Admission state: freeze and fence
// ---------------------------------------------------------------------------

/// Every record mutation is outside the §8.2 maintenance allowlist, so a
/// frozen space admits none of them and the space row is the whole decision.
async fn require_active_space(
    conn: &mut SqliteConnection,
    space_id: &str,
) -> Result<(), SpokeReject> {
    let row = sqlx::query("SELECT status FROM spaces WHERE space_id = ?")
        .bind(space_id)
        .fetch_optional(conn)
        .await
        .map_err(internal_failure)?
        .ok_or_else(|| not_found(format!("space {space_id} is not served by this fixture")))?;
    let status: String = row.try_get("status").map_err(internal_failure)?;
    if status == "frozen" {
        return Err(space_frozen(space_id));
    }
    Ok(())
}

/// Subject-level write admission for erasure: a fenced or archived subject
/// rejects every write for that subject (§7.7/MEM-R017). Context fences are
/// deliberately not consulted here — they gate placement into a context.
async fn require_subject_live(
    conn: &mut SqliteConnection,
    space_id: &str,
    subject_id: &str,
) -> Result<(), SpokeReject> {
    let row = sqlx::query("SELECT status FROM subjects WHERE space_id = ? AND subject_id = ?")
        .bind(space_id)
        .bind(subject_id)
        .fetch_optional(conn)
        .await
        .map_err(internal_failure)?;
    let Some(row) = row else {
        return Ok(());
    };
    let status: String = row.try_get("status").map_err(internal_failure)?;
    if status != "active" {
        return Err(subject_fenced(subject_id, "subject"));
    }
    Ok(())
}

/// Provider-enforced subject/context fence at admission (§8.3/MEM-R024). A
/// subject with no lifecycle row carries no fence state, so it admits the
/// write; a fenced subject or a fenced named placement context refuses it.
async fn require_subject_writable(
    conn: &mut SqliteConnection,
    space_id: &str,
    subject_id: &str,
    contexts: &[&str],
) -> Result<(), SpokeReject> {
    let row = sqlx::query(
        "SELECT status, fenced_contexts_json FROM subjects \
         WHERE space_id = ? AND subject_id = ?",
    )
    .bind(space_id)
    .bind(subject_id)
    .fetch_optional(conn)
    .await
    .map_err(internal_failure)?;
    let Some(row) = row else {
        return Ok(());
    };
    let status: String = row.try_get("status").map_err(internal_failure)?;
    if status != "active" {
        return Err(subject_fenced(subject_id, "subject"));
    }
    let fenced_raw: String = row
        .try_get("fenced_contexts_json")
        .map_err(internal_failure)?;
    let fenced: Vec<String> = serde_json::from_str(&fenced_raw).map_err(internal_failure)?;
    if contexts
        .iter()
        .any(|context| fenced.iter().any(|fenced| fenced == context))
    {
        return Err(subject_fenced(subject_id, "context"));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Authorization requirements (§6.5)
// ---------------------------------------------------------------------------

fn placement_requirement<'a>(
    capability: &'a str,
    subject_id: &'a str,
    visibility: &str,
    context_id: Option<&str>,
) -> GrantRequirement<'a> {
    GrantRequirement {
        capability,
        subject_id: Some(subject_id),
        context_ids: context_id.map(str::to_owned).into_iter().collect(),
        reads_shared: visibility == "shared",
        subject_all: false,
    }
}

/// `records_read` or `recall` must cover the whole declared access: one grant
/// per capability, never a union of coverage facets across grants.
fn authorized_for(
    grants: &[spoke_operations::spoke_schemas::MemoryGrant],
    principal_id: &str,
    space_id: &str,
    now: DateTime<Utc>,
    base: &GrantRequirement<'_>,
    capabilities: &[&str],
) -> bool {
    capabilities.iter().any(|capability| {
        is_authorized(
            grants,
            principal_id,
            space_id,
            now,
            &GrantRequirement {
                capability,
                ..base.clone()
            },
        )
    })
}

impl FixtureMemoryProvider {
    fn require_identity_space(&self, requested: &str) -> Result<(), SpokeReject> {
        if requested == self.identity.space_id {
            return Ok(());
        }
        Err(not_found(format!(
            "space {requested} is not served by this fixture"
        )))
    }

    // -----------------------------------------------------------------------
    // memory.records.create (§7.16)
    // -----------------------------------------------------------------------

    /// Direct authored write under `records_write` covering the declared
    /// placement.
    pub(crate) async fn records_create(
        &self,
        principal_id: &str,
        request: &MemoryRecordsCreateRequest,
        now: DateTime<Utc>,
    ) -> Result<RecordRevision, SpokeReject> {
        let mut transaction = self
            .store()
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(internal_failure)?;
        let created = self
            .records_create_in_tx(&mut transaction, principal_id, request, now)
            .await?;
        transaction.commit().await.map_err(internal_failure)?;
        Ok(created)
    }

    /// [`Self::records_create`] on a caller-owned connection; never commits.
    pub(crate) async fn records_create_in_tx(
        &self,
        conn: &mut SqliteConnection,
        principal_id: &str,
        request: &MemoryRecordsCreateRequest,
        now: DateTime<Utc>,
    ) -> Result<RecordRevision, SpokeReject> {
        checked(validate_memory_records_create(request, MAX_CONTENT_BYTES))?;
        let space_id = request.space_id.as_str();
        self.require_identity_space(space_id)?;
        require_active_space(conn, space_id).await?;
        let subject_id = request.subject_id.as_str();
        let (visibility, context_id) = match &request.scope {
            CreateScope::Shared => ("shared", None),
            CreateScope::Context(context) => ("context", Some(context.as_str())),
        };
        let placement_contexts: Vec<&str> = context_id.into_iter().collect();
        let grants = load_grants(conn, space_id).await?;
        if !authorized_for(
            &grants,
            principal_id,
            space_id,
            now,
            &placement_requirement("records_write", subject_id, visibility, context_id),
            &["records_write"],
        ) {
            return Err(not_authorized(
                "the authenticated principal holds no records_write grant covering the placement",
            ));
        }
        require_subject_writable(conn, space_id, subject_id, &placement_contexts).await?;

        let record_id = format!("rec_{}", Uuid::new_v4().simple());
        let content = content_json(&request.content.text, request.content.structured.as_ref())?;
        let provenance = json!({
            "source_context_ids": &request.source_context_ids,
            "anchors": &request.anchors,
            "captured_by": principal_id,
            "captured_at": timestamp_text(now),
        });
        let provenance_json = serde_json::to_string(&provenance).map_err(internal_failure)?;
        let extensions_json = match &request.extensions {
            Some(extensions) => serde_json::to_string(extensions).map_err(internal_failure)?,
            None => "{}".to_owned(),
        };
        sqlx::query(
            "INSERT INTO records (record_id, space_id, subject_id, kind, content_json, \
             scope_visibility, scope_context_id, provenance_json, import_ref_json, occurred_at, \
             tombstone_json, revision, created_at, updated_at, extensions_json) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, NULL, NULL, NULL, 1, ?, ?, ?)",
        )
        .bind(&record_id)
        .bind(space_id)
        .bind(subject_id)
        .bind(request.kind.as_str())
        .bind(content)
        .bind(visibility)
        .bind(context_id)
        .bind(provenance_json)
        .bind(timestamp_text(now))
        .bind(timestamp_text(now))
        .bind(extensions_json)
        .execute(&mut *conn)
        .await
        .map_err(internal_failure)?;
        bump_snapshot(conn, space_id).await?;
        Ok(RecordRevision {
            record_id,
            revision: 1,
        })
    }

    // -----------------------------------------------------------------------
    // memory.records.get (§7.17)
    // -----------------------------------------------------------------------

    /// Read one record under current authorization (MEM-R026), pairing the
    /// grant evaluation and the row inside one read snapshot.
    pub(crate) async fn records_get(
        &self,
        principal_id: &str,
        request: &MemoryRecordsGetRequest,
        now: DateTime<Utc>,
    ) -> Result<RetainedRecord, SpokeReject> {
        let mut transaction = self
            .store()
            .pool()
            .begin()
            .await
            .map_err(internal_failure)?;
        let record = self
            .records_get_in_tx(&mut transaction, principal_id, request, now)
            .await?;
        transaction.commit().await.map_err(internal_failure)?;
        Ok(record)
    }

    /// [`Self::records_get`] on a caller-owned connection; never begins or
    /// commits a transaction.
    pub(crate) async fn records_get_in_tx(
        &self,
        conn: &mut SqliteConnection,
        principal_id: &str,
        request: &MemoryRecordsGetRequest,
        now: DateTime<Utc>,
    ) -> Result<RetainedRecord, SpokeReject> {
        let space_id = request.space_id.as_str();
        self.require_identity_space(space_id)?;
        let record = fetch_record(conn, space_id, request.record_id.as_str()).await?;
        let grants = load_grants(conn, space_id).await?;
        let placement = placement_requirement(
            "records_read",
            &record.subject_id,
            &record.scope_visibility,
            record.scope_context_id.as_deref(),
        );
        if record.tombstone_json.is_some() {
            // Audit visibility of a tombstone belongs to admin only; every
            // other caller sees the same NOT_FOUND as an absent id.
            let admin = is_authorized(
                &grants,
                principal_id,
                space_id,
                now,
                &GrantRequirement {
                    capability: "admin",
                    ..placement
                },
            );
            if !admin {
                return Err(record_absent());
            }
        } else if !authorized_for(
            &grants,
            principal_id,
            space_id,
            now,
            &placement,
            &["records_read", "recall"],
        ) {
            // A record outside the caller's coverage answers exactly like an
            // absent id: the grant check is not an existence oracle.
            return Err(record_absent());
        }
        record_to_wire(&record)
    }

    // -----------------------------------------------------------------------
    // memory.records.update (§7.18)
    // -----------------------------------------------------------------------

    /// Update content and/or kind under `records_write` covering the current
    /// placement. Placement is not updatable here (§7.21 owns that).
    pub(crate) async fn records_update(
        &self,
        principal_id: &str,
        request: &MemoryRecordsUpdateRequest,
        now: DateTime<Utc>,
    ) -> Result<RecordRevision, SpokeReject> {
        let mut transaction = self
            .store()
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(internal_failure)?;
        let updated = self
            .records_update_in_tx(&mut transaction, principal_id, request, now)
            .await?;
        transaction.commit().await.map_err(internal_failure)?;
        Ok(updated)
    }

    /// [`Self::records_update`] on a caller-owned connection; never commits.
    #[allow(clippy::too_many_lines)] // one linear CAS update: precheck, revision and receipt
    pub(crate) async fn records_update_in_tx(
        &self,
        conn: &mut SqliteConnection,
        principal_id: &str,
        request: &MemoryRecordsUpdateRequest,
        now: DateTime<Utc>,
    ) -> Result<RecordRevision, SpokeReject> {
        checked(validate_memory_records_update(request, MAX_CONTENT_BYTES))?;
        let (space_id, record_id, expected_revision, content_text, content_structured, kind) =
            match request {
                MemoryRecordsUpdateRequest::Variant0 {
                    space_id,
                    record_id,
                    expected_revision,
                    content,
                    ..
                } => (
                    space_id.as_str(),
                    record_id.as_str(),
                    *expected_revision,
                    Some(content.text.clone()),
                    content.structured.clone(),
                    None,
                ),
                MemoryRecordsUpdateRequest::Variant1 {
                    space_id,
                    record_id,
                    expected_revision,
                    content,
                    kind,
                    ..
                } => (
                    space_id.as_str(),
                    record_id.as_str(),
                    *expected_revision,
                    content.as_ref().map(|content| content.text.clone()),
                    content
                        .as_ref()
                        .and_then(|content| content.structured.clone()),
                    Some(kind.as_str()),
                ),
            };
        self.require_identity_space(space_id)?;
        require_active_space(conn, space_id).await?;
        let record = fetch_record(conn, space_id, record_id).await?;
        if record.tombstone_json.is_some() {
            // A tombstone is audit-only: deleted content can never be
            // rewritten or regrown (MEM-R014/§6.6).
            return Err(record_absent());
        }
        let grants = load_grants(conn, space_id).await?;
        if !authorized_for(
            &grants,
            principal_id,
            space_id,
            now,
            &placement_requirement(
                "records_write",
                &record.subject_id,
                &record.scope_visibility,
                record.scope_context_id.as_deref(),
            ),
            &["records_write"],
        ) {
            return Err(record_absent());
        }
        let placement_contexts: Vec<&str> =
            record.scope_context_id.as_deref().into_iter().collect();
        require_subject_writable(conn, space_id, &record.subject_id, &placement_contexts).await?;
        if expected_revision.get() != record.revision {
            return Err(revision_conflict(expected_revision, record.revision));
        }

        // §7.0.4: content updates are never no-ops, but an update that names
        // only the already-stored kind changes nothing observable.
        let changes_content = content_text.is_some();
        let changes_kind = kind.is_some_and(|kind| kind != record.kind);
        if !changes_content && !changes_kind {
            return Ok(RecordRevision {
                record_id: record.record_id,
                revision: record.revision,
            });
        }
        let next_content = match content_text {
            Some(text) => content_json(&text, content_structured.as_ref())?,
            None => record.content_json.clone(),
        };
        let next_kind = kind.unwrap_or(&record.kind);
        let revision = record.revision + 1;
        let updated = sqlx::query(
            "UPDATE records SET content_json = ?, kind = ?, updated_at = ?, revision = ? \
             WHERE space_id = ? AND record_id = ? AND revision = ?",
        )
        .bind(&next_content)
        .bind(next_kind)
        .bind(timestamp_text(now))
        .bind(row_i64(revision)?)
        .bind(space_id)
        .bind(record_id)
        .bind(row_i64(record.revision)?)
        .execute(&mut *conn)
        .await
        .map_err(internal_failure)?;
        if updated.rows_affected() != 1 {
            return Err(internal(format!(
                "record {record_id} changed under its revision-CAS write"
            )));
        }
        bump_snapshot(conn, space_id).await?;
        Ok(RecordRevision {
            record_id: record.record_id,
            revision,
        })
    }

    // -----------------------------------------------------------------------
    // memory.records.delete (§7.19)
    // -----------------------------------------------------------------------

    /// Erase the content and keep the tombstone under `records_write` covering
    /// the current placement (MEM-R014).
    pub(crate) async fn records_delete(
        &self,
        principal_id: &str,
        request: &MemoryRecordsDeleteRequest,
        now: DateTime<Utc>,
    ) -> Result<RecordDeleted, SpokeReject> {
        let mut transaction = self
            .store()
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(internal_failure)?;
        let deleted = self
            .records_delete_in_tx(&mut transaction, principal_id, request, now)
            .await?;
        transaction.commit().await.map_err(internal_failure)?;
        Ok(deleted)
    }

    /// [`Self::records_delete`] on a caller-owned connection; never commits.
    pub(crate) async fn records_delete_in_tx(
        &self,
        conn: &mut SqliteConnection,
        principal_id: &str,
        request: &MemoryRecordsDeleteRequest,
        now: DateTime<Utc>,
    ) -> Result<RecordDeleted, SpokeReject> {
        let space_id = request.space_id.as_str();
        self.require_identity_space(space_id)?;
        require_active_space(conn, space_id).await?;
        let record = fetch_record(conn, space_id, request.record_id.as_str()).await?;
        let grants = load_grants(conn, space_id).await?;
        if !authorized_for(
            &grants,
            principal_id,
            space_id,
            now,
            &placement_requirement(
                "records_write",
                &record.subject_id,
                &record.scope_visibility,
                record.scope_context_id.as_deref(),
            ),
            &["records_write"],
        ) {
            return Err(record_absent());
        }
        // §7.7/MEM-R017: a fenced or archived subject rejects every write for
        // that subject, so erasure admission obeys the subject lifecycle before
        // any new effect. A context fence is placement-scoped (§7.7/MEM-R024):
        // it bans writes *into* that context, not removing a record already
        // placed there.
        require_subject_live(conn, space_id, &record.subject_id).await?;
        if request.expected_revision.get() != record.revision {
            return Err(revision_conflict(
                request.expected_revision,
                record.revision,
            ));
        }
        if record.tombstone_json.is_some() {
            // A repeat delete at the current revision observes exactly the
            // stored tombstone: a genuine no-op success (§7.0.4).
            return Ok(RecordDeleted {
                record_id: record.record_id,
                tombstoned: true,
            });
        }
        let tombstone = json!({
            "deleted_at": timestamp_text(now),
            "deleted_by": principal_id,
            "reason": request.reason,
        });
        let tombstone_json = serde_json::to_string(&tombstone).map_err(internal_failure)?;
        let revision = record.revision + 1;
        let updated = sqlx::query(
            "UPDATE records SET content_json = '{\"text\":\"\"}', tombstone_json = ?, \
             updated_at = ?, revision = ? \
             WHERE space_id = ? AND record_id = ? AND revision = ?",
        )
        .bind(tombstone_json)
        .bind(timestamp_text(now))
        .bind(row_i64(revision)?)
        .bind(space_id)
        .bind(&record.record_id)
        .bind(row_i64(record.revision)?)
        .execute(&mut *conn)
        .await
        .map_err(internal_failure)?;
        if updated.rows_affected() != 1 {
            return Err(internal(format!(
                "record {} changed under its revision-CAS delete",
                record.record_id
            )));
        }
        bump_snapshot(conn, space_id).await?;
        Ok(RecordDeleted {
            record_id: record.record_id,
            tombstoned: true,
        })
    }

    // -----------------------------------------------------------------------
    // memory.records.promote (§7.21)
    // -----------------------------------------------------------------------

    /// Explicit placement change. One grant must carry both `promote` and
    /// `records_write` and cover the whole source-plus-target placement; two
    /// disjoint grants never compose into promotion authority (§6.5/§7.21).
    pub(crate) async fn records_promote(
        &self,
        principal_id: &str,
        request: &MemoryRecordsPromoteRequest,
        now: DateTime<Utc>,
    ) -> Result<RecordPromoted, SpokeReject> {
        let mut transaction = self
            .store()
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(internal_failure)?;
        let promoted = self
            .records_promote_in_tx(&mut transaction, principal_id, request, now)
            .await?;
        transaction.commit().await.map_err(internal_failure)?;
        Ok(promoted)
    }

    /// [`Self::records_promote`] on a caller-owned connection; never commits.
    pub(crate) async fn records_promote_in_tx(
        &self,
        conn: &mut SqliteConnection,
        principal_id: &str,
        request: &MemoryRecordsPromoteRequest,
        now: DateTime<Utc>,
    ) -> Result<RecordPromoted, SpokeReject> {
        let space_id = request.space_id.as_str();
        self.require_identity_space(space_id)?;
        require_active_space(conn, space_id).await?;
        let record = fetch_record(conn, space_id, request.record_id.as_str()).await?;
        if record.tombstone_json.is_some() {
            // A deleted record keeps its audit placement and can never be
            // promoted into a new state (MEM-R014).
            return Err(record_absent());
        }
        let (target_visibility, target_context) = match &request.target_scope {
            PromoteScope::Shared => ("shared", None),
            PromoteScope::Context(context) => ("context", Some(context.as_str())),
        };
        let mut contexts: Vec<String> = Vec::new();
        for context in [record.scope_context_id.as_deref(), target_context]
            .into_iter()
            .flatten()
        {
            if !contexts.iter().any(|known| known == context) {
                contexts.push(context.to_owned());
            }
        }
        let reads_shared = record.scope_visibility == "shared" || target_visibility == "shared";
        let promote_requirement = GrantRequirement {
            capability: "promote",
            subject_id: Some(&record.subject_id),
            context_ids: contexts.clone(),
            reads_shared,
            subject_all: false,
        };
        let write_requirement = GrantRequirement {
            capability: "records_write",
            ..promote_requirement.clone()
        };
        let grants = load_grants(conn, space_id).await?;
        let authorized = grants.iter().any(|grant| {
            grant_covers(grant, principal_id, space_id, now, &promote_requirement)
                && grant_covers(grant, principal_id, space_id, now, &write_requirement)
        });
        if !authorized {
            return Err(record_absent());
        }
        let placement_contexts: Vec<&str> = contexts.iter().map(String::as_str).collect();
        require_subject_writable(conn, space_id, &record.subject_id, &placement_contexts).await?;
        if request.expected_revision.get() != record.revision {
            return Err(revision_conflict(
                request.expected_revision,
                record.revision,
            ));
        }
        let target_scope = wire_placement(target_visibility, target_context)?;
        let unchanged = record.scope_visibility == target_visibility
            && record.scope_context_id.as_deref() == target_context;
        if unchanged {
            // Promoting to the placement the record already has changes
            // nothing observable: a no-op success (§7.0.4).
            return Ok(RecordPromoted {
                record_id: record.record_id,
                revision: record.revision,
                scope: target_scope,
            });
        }
        let revision = record.revision + 1;
        let updated = sqlx::query(
            "UPDATE records SET scope_visibility = ?, scope_context_id = ?, updated_at = ?, \
             revision = ? WHERE space_id = ? AND record_id = ? AND revision = ?",
        )
        .bind(target_visibility)
        .bind(target_context)
        .bind(timestamp_text(now))
        .bind(row_i64(revision)?)
        .bind(space_id)
        .bind(&record.record_id)
        .bind(row_i64(record.revision)?)
        .execute(&mut *conn)
        .await
        .map_err(internal_failure)?;
        if updated.rows_affected() != 1 {
            return Err(internal(format!(
                "record {} changed under its revision-CAS promotion",
                record.record_id
            )));
        }
        bump_snapshot(conn, space_id).await?;
        Ok(RecordPromoted {
            record_id: record.record_id,
            revision,
            scope: target_scope,
        })
    }

    // -----------------------------------------------------------------------
    // memory.records.list (§7.20)
    // -----------------------------------------------------------------------

    /// Authorized, bounded, snapshot-stable page of a subject's records.
    pub(crate) async fn records_list(
        &self,
        principal_id: &str,
        request: &MemoryRecordsListRequest,
        now: DateTime<Utc>,
    ) -> Result<RecordPage, SpokeReject> {
        let mut transaction = self
            .store()
            .pool()
            .begin()
            .await
            .map_err(internal_failure)?;
        let page = self
            .records_list_in_tx(&mut transaction, principal_id, request, now)
            .await?;
        transaction.commit().await.map_err(internal_failure)?;
        Ok(page)
    }

    /// [`Self::records_list`] on a caller-owned connection: authorization, the
    /// rows and the cursor marker come from the caller's one read snapshot.
    #[allow(clippy::too_many_lines)] // one linear authorized page: rows and marker from one snapshot
    pub(crate) async fn records_list_in_tx(
        &self,
        conn: &mut SqliteConnection,
        principal_id: &str,
        request: &MemoryRecordsListRequest,
        now: DateTime<Utc>,
    ) -> Result<RecordPage, SpokeReject> {
        checked(validate_memory_records_list(request, MAX_PAGE_SIZE))?;
        let space_id = request.space_id.as_str();
        self.require_identity_space(space_id)?;
        let subject_id = request.subject_id.as_str();
        let selection_requirement = match &request.selection {
            ReadSelection::SubjectAll => whole_scope_requirement("records_read", Some(subject_id)),
            ReadSelection::SharedOnly => GrantRequirement {
                capability: "records_read",
                subject_id: Some(subject_id),
                context_ids: Vec::new(),
                reads_shared: true,
                subject_all: false,
            },
            ReadSelection::SelectedContexts {
                context_ids,
                include_shared,
            } => GrantRequirement {
                capability: "records_read",
                subject_id: Some(subject_id),
                context_ids: context_ids.clone(),
                reads_shared: include_shared.unwrap_or(false),
                subject_all: false,
            },
        };
        let grants = load_grants(conn, space_id).await?;
        if !authorized_for(
            &grants,
            principal_id,
            space_id,
            now,
            &selection_requirement,
            &["records_read", "recall"],
        ) {
            return Err(not_authorized(
                "the authenticated principal holds no read grant covering the selection",
            ));
        }
        let sees_tombstones = request.include_tombstoned
            && is_authorized(
                &grants,
                principal_id,
                space_id,
                now,
                &GrantRequirement {
                    capability: "admin",
                    ..selection_requirement
                },
            );
        if request.include_tombstoned && !sees_tombstones {
            return Err(not_authorized(
                "only an admin grant covering the selection may include tombstoned records",
            ));
        }

        let rows = sqlx::query(
            "SELECT record_id, space_id, subject_id, kind, content_json, scope_visibility, \
             scope_context_id, provenance_json, import_ref_json, occurred_at, tombstone_json, \
             revision, created_at, updated_at, extensions_json \
             FROM records WHERE space_id = ? AND subject_id = ? \
             ORDER BY created_at DESC, record_id ASC",
        )
        .bind(space_id)
        .bind(subject_id)
        .fetch_all(&mut *conn)
        .await
        .map_err(internal_failure)?;
        let mut records = Vec::with_capacity(rows.len());
        for row in rows {
            let record = StoredRecord {
                record_id: row.try_get("record_id").map_err(internal_failure)?,
                space_id: row.try_get("space_id").map_err(internal_failure)?,
                subject_id: row.try_get("subject_id").map_err(internal_failure)?,
                kind: row.try_get("kind").map_err(internal_failure)?,
                content_json: row.try_get("content_json").map_err(internal_failure)?,
                scope_visibility: row.try_get("scope_visibility").map_err(internal_failure)?,
                scope_context_id: row.try_get("scope_context_id").map_err(internal_failure)?,
                provenance_json: row.try_get("provenance_json").map_err(internal_failure)?,
                import_ref_json: row.try_get("import_ref_json").map_err(internal_failure)?,
                occurred_at: optional_timestamp(
                    row.try_get("occurred_at").map_err(internal_failure)?,
                )?,
                tombstone_json: row.try_get("tombstone_json").map_err(internal_failure)?,
                revision: row_u64(&row, "revision")?,
                created_at: timestamp(
                    &row.try_get::<String, _>("created_at")
                        .map_err(internal_failure)?,
                )?,
                updated_at: timestamp(
                    &row.try_get::<String, _>("updated_at")
                        .map_err(internal_failure)?,
                )?,
                extensions_json: row.try_get("extensions_json").map_err(internal_failure)?,
            };
            if !placement_matches(&request.selection, &record) {
                continue;
            }
            if !request.kinds.is_empty() && !request.kinds.iter().any(|kind| kind == &record.kind) {
                continue;
            }
            if let Some(filter) = &request.filter {
                let provenance = json_field(&record.provenance_json, "record provenance")?;
                let claims = provenance["source_context_ids"].as_array();
                let matches = claims.is_some_and(|claims| {
                    claims.iter().any(|claim| {
                        claim.as_str().is_some_and(|claim| {
                            filter
                                .source_context_ids
                                .iter()
                                .any(|context| context == claim)
                        })
                    })
                });
                if !matches {
                    continue;
                }
            }
            if record.tombstone_json.is_some() && !sees_tombstones {
                continue;
            }
            records.push(record);
        }
        let total_authorized = u64::try_from(records.len()).map_err(internal_failure)?;

        // §7.0.7/MEM-R026: the cursor is bound to the provider snapshot marker,
        // the policy identity and the query itself, and the marker is read in
        // the same snapshot as the authorization and rows above.
        let snapshot = snapshot_marker(conn, space_id).await?;
        let binding = jcs_sha256_hex(&json!({
            "principal": principal_id,
            "space": space_id,
            "subject": subject_id,
            "selection": &request.selection,
            "filter": &request.filter,
            "kinds": &request.kinds,
            "tombstoned": request.include_tombstoned,
            "snapshot": snapshot,
            "policy": ENGINE_POLICY_VERSION,
        }));
        let binding_digest = match binding {
            SpokeResult::Ok(digest) => digest,
            SpokeResult::Reject(reject) => return Err(reject),
        };
        let start = match request.cursor.as_deref() {
            None => 0,
            Some(cursor) => {
                let (digest, last_id) = cursor
                    .split_once(':')
                    .ok_or_else(|| cursor_invalid("cursor is malformed"))?;
                if digest != binding_digest {
                    return Err(cursor_invalid(
                        "cursor was issued for a different query or provider state",
                    ));
                }
                let position = records
                    .iter()
                    .position(|record| record.record_id == last_id)
                    .ok_or_else(|| cursor_invalid("cursor no longer matches the record set"))?;
                position + 1
            }
        };
        let limit = usize::try_from(request.limit.map_or(MAX_PAGE_SIZE.get(), NonZeroU64::get))
            .map_err(internal_failure)?;
        let remaining = &records[start..];
        let page = &remaining[..remaining.len().min(limit)];
        let next_cursor = if remaining.len() > limit {
            page.last()
                .map(|record| format!("{binding_digest}:{}", record.record_id))
        } else {
            None
        };
        let records = page
            .iter()
            .map(record_to_wire)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(RecordPage {
            records,
            total_authorized,
            next_cursor,
        })
    }
}

/// Does the stored placement fall inside the read selection (§6.12)?
fn placement_matches(selection: &ReadSelection, record: &StoredRecord) -> bool {
    match selection {
        ReadSelection::SubjectAll => true,
        ReadSelection::SharedOnly => record.scope_visibility == "shared",
        ReadSelection::SelectedContexts {
            context_ids,
            include_shared,
        } => {
            if record.scope_visibility == "shared" {
                return include_shared.unwrap_or(false);
            }
            record
                .scope_context_id
                .as_deref()
                .is_some_and(|context| context_ids.iter().any(|selected| selected == context))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use chrono::{DateTime, Duration};
    use tempfile::TempDir;

    use spoke_operations::spoke_schemas::retained_record::{LiveRecord, TombstonedRecord};
    use spoke_operations::spoke_schemas::{
        MemoryGrantsGrantRequest, MemoryGrantsRevokeRequest, MemorySpacesFreezeRequest,
        MemorySpacesUnfreezeRequest, MemorySubjectsEnsureRequest, MemorySubjectsFenceRequest,
    };

    const OPERATOR: &str = "principal-operator";
    const CLIENT: &str = "principal-client-b";
    const WRITER: &str = "principal-writer";
    const PROMOTER: &str = "principal-promoter";

    struct Scenario {
        _home: TempDir,
        path: PathBuf,
        provider: FixtureMemoryProvider,
        space_id: String,
    }

    impl Scenario {
        async fn open() -> Self {
            let home = tempfile::tempdir().expect("scenario home");
            let path = home.path().join("records.sqlite");
            let provider = FixtureMemoryProvider::open(&path)
                .await
                .expect("fixture opens");
            let space_id = provider.identity().space_id.clone();
            provider
                .bootstrap_operator_admin(OPERATOR)
                .await
                .expect("operator bootstrap");
            Self {
                _home: home,
                path,
                provider,
                space_id,
            }
        }

        async fn reopen(self) -> Self {
            self.provider.close().await;
            let provider = FixtureMemoryProvider::open(&self.path)
                .await
                .expect("fixture reopens");
            Self {
                _home: self._home,
                path: self.path,
                provider,
                space_id: self.space_id,
            }
        }
    }

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2020-01-01T00:00:00Z")
            .expect("a valid scenario instant")
            .with_timezone(&Utc)
    }

    fn ensure_request(space: &str, subject: &str) -> MemorySubjectsEnsureRequest {
        serde_json::from_value(json!({
            "space_id": space,
            "subject_id": subject,
        }))
        .expect("valid subjects.ensure request")
    }

    fn freeze_request(space: &str, expected: u64) -> MemorySpacesFreezeRequest {
        serde_json::from_value(json!({
            "space_id": space,
            "expected_revision": expected,
            "idempotency_key": format!("freeze-{expected}"),
        }))
        .expect("valid spaces.freeze request")
    }

    fn unfreeze_request(space: &str, expected: u64) -> MemorySpacesUnfreezeRequest {
        serde_json::from_value(json!({
            "space_id": space,
            "expected_revision": expected,
            "idempotency_key": format!("unfreeze-{expected}"),
        }))
        .expect("valid spaces.unfreeze request")
    }

    fn grant_request(
        space: &str,
        grantee: &str,
        capabilities: &[&str],
        subject: Option<&str>,
        contexts: Option<&[&str]>,
    ) -> MemoryGrantsGrantRequest {
        let mut value = json!({
            "space_id": space,
            "principal_id": grantee,
            "capabilities": capabilities,
            "idempotency_key": format!("grant-{grantee}-{}", capabilities.join("-")),
        });
        if let Some(subject) = subject {
            value["subject_id"] = json!(subject);
        }
        if let Some(contexts) = contexts {
            value["context_ids"] = json!(contexts);
            value["shared_access"] = json!(false);
        }
        serde_json::from_value(value).expect("valid grants.grant request")
    }

    fn shared_grant_request(
        space: &str,
        grantee: &str,
        capabilities: &[&str],
        contexts: &[&str],
    ) -> MemoryGrantsGrantRequest {
        serde_json::from_value(json!({
            "space_id": space,
            "principal_id": grantee,
            "capabilities": capabilities,
            "context_ids": contexts,
            "shared_access": true,
            "idempotency_key": format!("grant-shared-{grantee}"),
        }))
        .expect("valid shared grants.grant request")
    }

    fn create_request(
        space: &str,
        subject: &str,
        kind: &str,
        text: &str,
        context: Option<&str>,
        key: &str,
    ) -> MemoryRecordsCreateRequest {
        serde_json::from_value(json!({
            "space_id": space,
            "subject_id": subject,
            "kind": kind,
            "content": { "text": text },
            "scope": match context {
                Some(context) => json!({ "visibility": "context", "context_id": context }),
                None => json!({ "visibility": "shared" }),
            },
            "source_context_ids": [],
            "anchors": [],
            "idempotency_key": key,
        }))
        .expect("valid records.create request")
    }

    fn update_content_request(
        space: &str,
        record_id: &str,
        expected: u64,
        text: &str,
        key: &str,
    ) -> MemoryRecordsUpdateRequest {
        serde_json::from_value(json!({
            "space_id": space,
            "record_id": record_id,
            "expected_revision": expected,
            "content": { "text": text },
            "idempotency_key": key,
        }))
        .expect("valid records.update request")
    }

    fn update_kind_request(
        space: &str,
        record_id: &str,
        expected: u64,
        kind: &str,
        key: &str,
    ) -> MemoryRecordsUpdateRequest {
        serde_json::from_value(json!({
            "space_id": space,
            "record_id": record_id,
            "expected_revision": expected,
            "kind": kind,
            "idempotency_key": key,
        }))
        .expect("valid records.update request")
    }

    fn get_request(space: &str, record_id: &str) -> MemoryRecordsGetRequest {
        serde_json::from_value(json!({
            "space_id": space,
            "record_id": record_id,
        }))
        .expect("valid records.get request")
    }

    fn create_with_content_request(
        space: &str,
        subject: &str,
        kind: &str,
        content: Value,
        context: Option<&str>,
        key: &str,
    ) -> MemoryRecordsCreateRequest {
        serde_json::from_value(json!({
            "space_id": space,
            "subject_id": subject,
            "kind": kind,
            "content": content,
            "scope": match context {
                Some(context) => json!({ "visibility": "context", "context_id": context }),
                None => json!({ "visibility": "shared" }),
            },
            "source_context_ids": [],
            "anchors": [],
            "idempotency_key": key,
        }))
        .expect("valid records.create request")
    }

    fn expiring_grant_request(
        space: &str,
        grantee: &str,
        capabilities: &[&str],
        subject: &str,
        contexts: &[&str],
        expires_at: DateTime<Utc>,
    ) -> MemoryGrantsGrantRequest {
        serde_json::from_value(json!({
            "space_id": space,
            "principal_id": grantee,
            "capabilities": capabilities,
            "subject_id": subject,
            "context_ids": contexts,
            "shared_access": false,
            "expires_at": timestamp_text(expires_at),
            "idempotency_key": format!("grant-expiring-{grantee}"),
        }))
        .expect("valid expiring grants.grant request")
    }

    fn revoke_request(space: &str, grant_id: &str, expected: u64) -> MemoryGrantsRevokeRequest {
        serde_json::from_value(json!({
            "space_id": space,
            "grant_id": grant_id,
            "expected_revision": expected,
            "idempotency_key": format!("revoke-{grant_id}-{expected}"),
        }))
        .expect("valid grants.revoke request")
    }

    /// The whole public refusal, for comparing two refusals field by field
    /// without pinning literal wording as the oracle.
    fn refusal<T: std::fmt::Debug>(result: Result<T, SpokeReject>) -> SpokeReject {
        match result {
            Ok(value) => panic!("expected a protocol reject, observed {value:?}"),
            Err(reject) => reject,
        }
    }

    fn assert_same_refusal<T: std::fmt::Debug, U: std::fmt::Debug>(
        foreign: Result<T, SpokeReject>,
        absent: Result<U, SpokeReject>,
    ) {
        let foreign = refusal(foreign);
        let absent = refusal(absent);
        assert_eq!(foreign.code, absent.code);
        assert_eq!(foreign.message, absent.message);
        assert_eq!(foreign.details, absent.details);
    }

    /// The raw stored bytes and CAS/tombstone columns for one record.
    async fn stored_row(scenario: &Scenario, record_id: &str) -> (String, u64, Option<String>) {
        let mut conn = scenario
            .provider
            .store()
            .pool()
            .acquire()
            .await
            .expect("scenario connection");
        let row = sqlx::query(
            "SELECT content_json, revision, tombstone_json FROM records WHERE record_id = ?",
        )
        .bind(record_id)
        .fetch_one(&mut *conn)
        .await
        .expect("stored record row");
        let revision: i64 = row.try_get("revision").expect("stored revision");
        (
            row.try_get("content_json").expect("stored content"),
            u64::try_from(revision).expect("positive revision"),
            row.try_get("tombstone_json").expect("stored tombstone"),
        )
    }

    fn delete_request(
        space: &str,
        record_id: &str,
        expected: u64,
        key: &str,
    ) -> MemoryRecordsDeleteRequest {
        serde_json::from_value(json!({
            "space_id": space,
            "record_id": record_id,
            "expected_revision": expected,
            "reason": "scenario erasure",
            "idempotency_key": key,
        }))
        .expect("valid records.delete request")
    }

    fn promote_request(
        space: &str,
        record_id: &str,
        expected: u64,
        target: Value,
        key: &str,
    ) -> MemoryRecordsPromoteRequest {
        serde_json::from_value(json!({
            "space_id": space,
            "record_id": record_id,
            "expected_revision": expected,
            "target_scope": target,
            "idempotency_key": key,
        }))
        .expect("valid records.promote request")
    }

    fn list_request(
        space: &str,
        subject: &str,
        selection: Value,
        kinds: &[&str],
        include_tombstoned: bool,
        limit: Option<u64>,
        cursor: Option<&str>,
    ) -> MemoryRecordsListRequest {
        let mut value = json!({
            "space_id": space,
            "subject_id": subject,
            "selection": selection,
            "kinds": kinds,
            "include_tombstoned": include_tombstoned,
        });
        if let Some(limit) = limit {
            value["limit"] = json!(limit);
        }
        if let Some(cursor) = cursor {
            value["cursor"] = json!(cursor);
        }
        serde_json::from_value(value).expect("valid records.list request")
    }

    fn fence_request(
        space: &str,
        subject: &str,
        action: &str,
        target: Value,
        expected: u64,
    ) -> MemorySubjectsFenceRequest {
        serde_json::from_value(json!({
            "space_id": space,
            "subject_id": subject,
            "target": target,
            "action": action,
            "expected_revision": expected,
            "idempotency_key": format!("fence-{action}-{expected}"),
        }))
        .expect("valid subjects.fence request")
    }

    fn reject_code<T: std::fmt::Debug>(result: Result<T, SpokeReject>) -> SpokeRejectCode {
        match result {
            Ok(value) => panic!("expected a protocol reject, observed {value:?}"),
            Err(reject) => reject.code,
        }
    }

    fn live(record: &RetainedRecord) -> &LiveRecord {
        match record {
            RetainedRecord::LiveRecord(record) => record,
            other => panic!("expected a live record, observed {other:?}"),
        }
    }

    fn tombstoned(record: &RetainedRecord) -> &TombstonedRecord {
        match record {
            RetainedRecord::TombstonedRecord(record) => record,
            other => panic!("expected a tombstoned record, observed {other:?}"),
        }
    }

    fn scope_of(scope: &PlacementScope) -> (String, Option<String>) {
        match scope {
            PlacementScope::Shared => ("shared".to_owned(), None),
            PlacementScope::Context(context) => ("context".to_owned(), Some(context.to_string())),
        }
    }

    async fn marker(scenario: &Scenario) -> u64 {
        let mut conn = scenario
            .provider
            .store()
            .pool()
            .acquire()
            .await
            .expect("scenario connection");
        snapshot_marker(&mut conn, &scenario.space_id)
            .await
            .expect("stored snapshot marker")
    }

    #[tokio::test]
    async fn create_read_update_delete_with_revision_cas() {
        let scenario = Scenario::open().await;
        let space = scenario.space_id.clone();
        let subject = "subject-one";
        scenario
            .provider
            .subjects_ensure(OPERATOR, &ensure_request(&space, subject), now())
            .await
            .expect("subject exists");

        let created = scenario
            .provider
            .records_create(
                OPERATOR,
                &create_request(&space, subject, "note", "first", Some("ctx-a"), "create-1"),
                now(),
            )
            .await
            .expect("operator creates a record");
        assert_eq!(created.revision, 1);

        let record = scenario
            .provider
            .records_get(OPERATOR, &get_request(&space, &created.record_id), now())
            .await
            .expect("operator reads the record");
        let live_record = live(&record);
        assert_eq!(live_record.content.text, "first");
        assert_eq!(live_record.kind.to_string(), "note");
        assert_eq!(
            scope_of(&live_record.scope),
            ("context".to_owned(), Some("ctx-a".to_owned()))
        );
        // Provenance is provider-populated from the authenticated context.
        assert_eq!(live_record.provenance.captured_by.as_str(), OPERATOR);
        assert_eq!(*live_record.provenance.captured_at, now());
        assert_eq!(live_record.record_id.as_str(), created.record_id);

        let updated = scenario
            .provider
            .records_update(
                OPERATOR,
                &update_content_request(&space, &created.record_id, 1, "second", "update-1"),
                now(),
            )
            .await
            .expect("authorized content update");
        assert_eq!(updated.revision, 2);
        let record = scenario
            .provider
            .records_get(OPERATOR, &get_request(&space, &created.record_id), now())
            .await
            .expect("operator reads the updated record");
        let live_record = live(&record);
        assert_eq!(live_record.content.text, "second");
        assert_eq!(live_record.revision.get(), 2);

        // A stale writer is rejected and the stored row does not move.
        let conflict = scenario
            .provider
            .records_update(
                OPERATOR,
                &update_content_request(&space, &created.record_id, 1, "lost", "update-stale"),
                now(),
            )
            .await
            .expect_err("stale revision is rejected");
        assert_eq!(conflict.code, SpokeRejectCode::RevisionConflict);
        let details = conflict.details.expect("CAS details");
        assert_eq!(details["expected_revision"], json!(1));
        assert_eq!(details["actual_revision"], json!(2));
        let record = scenario
            .provider
            .records_get(OPERATOR, &get_request(&space, &created.record_id), now())
            .await
            .expect("operator reads the unchanged record");
        assert_eq!(live(&record).content.text, "second");
        assert_eq!(live(&record).revision.get(), 2);

        let deleted = scenario
            .provider
            .records_delete(
                OPERATOR,
                &delete_request(&space, &created.record_id, 2, "delete-1"),
                now(),
            )
            .await
            .expect("authorized delete");
        assert!(deleted.tombstoned);
        let record = scenario
            .provider
            .records_get(OPERATOR, &get_request(&space, &created.record_id), now())
            .await
            .expect("admin still sees the tombstone");
        let tombstone = tombstoned(&record);
        assert_eq!(tombstone.content.text, "");
        assert_eq!(tombstone.revision.get(), 3);
        assert_eq!(tombstone.tombstone.deleted_by.as_str(), OPERATOR);
        assert_eq!(
            scope_of(&tombstone.scope),
            ("context".to_owned(), Some("ctx-a".to_owned()))
        );

        // A non-admin reader gets the absent-id refusal for a tombstone, so
        // deletion is not observable as a distinct existence.
        scenario
            .provider
            .grants_grant(
                OPERATOR,
                &grant_request(
                    &space,
                    CLIENT,
                    &["records_read"],
                    Some(subject),
                    Some(&["ctx-a"]),
                ),
                now(),
            )
            .await
            .expect("reader grant");
        let hidden = scenario
            .provider
            .records_get(CLIENT, &get_request(&space, &created.record_id), now())
            .await
            .expect_err("tombstone is hidden from non-admin");
        assert_eq!(hidden.code, SpokeRejectCode::NotFound);
        let page = scenario
            .provider
            .records_list(
                OPERATOR,
                &list_request(
                    &space,
                    subject,
                    json!({ "mode": "subject_all" }),
                    &[],
                    true,
                    None,
                    None,
                ),
                now(),
            )
            .await
            .expect("admin lists tombstones");
        assert_eq!(page.total_authorized, 1);
        assert_eq!(
            tombstoned(&page.records[0]).record_id.as_str(),
            created.record_id
        );
    }

    #[tokio::test]
    async fn genuine_noops_do_not_advance_revision_or_snapshot() {
        let scenario = Scenario::open().await;
        let space = scenario.space_id.clone();
        let subject = "subject-one";
        scenario
            .provider
            .subjects_ensure(OPERATOR, &ensure_request(&space, subject), now())
            .await
            .expect("subject exists");
        let created = scenario
            .provider
            .records_create(
                OPERATOR,
                &create_request(&space, subject, "note", "same", Some("ctx-a"), "create-1"),
                now(),
            )
            .await
            .expect("record exists");

        let after_create = marker(&scenario).await;
        let same_scope = scenario
            .provider
            .records_promote(
                OPERATOR,
                &promote_request(
                    &space,
                    &created.record_id,
                    1,
                    json!({ "visibility": "context", "context_id": "ctx-a" }),
                    "promote-same",
                ),
                now(),
            )
            .await
            .expect("promotion to the current placement is a no-op");
        assert_eq!(same_scope.revision, 1);
        assert_eq!(marker(&scenario).await, after_create);

        // Restating the stored kind changes nothing observable either.
        let same_kind = scenario
            .provider
            .records_update(
                OPERATOR,
                &update_kind_request(&space, &created.record_id, 1, "note", "update-kind-same"),
                now(),
            )
            .await
            .expect("kind-only update to the stored kind is a no-op");
        assert_eq!(same_kind.revision, 1);
        assert_eq!(marker(&scenario).await, after_create);

        // §7.0.4: content updates are never no-ops, even when the text repeats.
        let identical = scenario
            .provider
            .records_update(
                OPERATOR,
                &update_content_request(&space, &created.record_id, 1, "same", "update-same"),
                now(),
            )
            .await
            .expect("content update is accepted");
        assert_eq!(identical.revision, 2);
        assert_eq!(marker(&scenario).await, after_create + 1);

        // A real kind change is an observable update.
        let restyled = scenario
            .provider
            .records_update(
                OPERATOR,
                &update_kind_request(&space, &created.record_id, 2, "fact", "update-kind"),
                now(),
            )
            .await
            .expect("kind change is accepted");
        assert_eq!(restyled.revision, 3);
        assert_eq!(marker(&scenario).await, after_create + 2);

        let deleted = scenario
            .provider
            .records_delete(
                OPERATOR,
                &delete_request(&space, &created.record_id, 3, "delete-1"),
                now(),
            )
            .await
            .expect("first delete tombstones");
        assert!(deleted.tombstoned);
        let after_delete = marker(&scenario).await;
        assert_eq!(after_delete, after_create + 3);

        // A repeat delete at the current revision observes the stored
        // tombstone and advances neither revision nor the private snapshot.
        let repeated = scenario
            .provider
            .records_delete(
                OPERATOR,
                &delete_request(&space, &created.record_id, 4, "delete-2"),
                now(),
            )
            .await
            .expect("repeat delete is a no-op success");
        assert!(repeated.tombstoned);
        assert_eq!(marker(&scenario).await, after_delete);
        let record = scenario
            .provider
            .records_get(OPERATOR, &get_request(&space, &created.record_id), now())
            .await
            .expect("admin reads the tombstone");
        let tombstone = tombstoned(&record);
        assert_eq!(tombstone.revision.get(), 4);
        assert_eq!(tombstone.kind.to_string(), "fact");
    }

    #[tokio::test]
    async fn pages_are_bounded_stable_and_authorized() {
        let scenario = Scenario::open().await;
        let space = scenario.space_id.clone();
        let subject = "subject-one";
        scenario
            .provider
            .subjects_ensure(OPERATOR, &ensure_request(&space, subject), now())
            .await
            .expect("subject exists");
        let mut created = Vec::new();
        for (text, context) in [
            ("a1", Some("ctx-a")),
            ("a2", Some("ctx-a")),
            ("a3", Some("ctx-a")),
            ("b1", Some("ctx-b")),
            ("s1", None),
        ] {
            let record = scenario
                .provider
                .records_create(
                    OPERATOR,
                    &create_request(&space, subject, "note", text, context, text),
                    now(),
                )
                .await
                .expect("operator writes a record");
            created.push(record);
        }
        let ctx_a: Vec<String> = created[..3]
            .iter()
            .map(|record| record.record_id.clone())
            .collect();

        scenario
            .provider
            .grants_grant(
                OPERATOR,
                &grant_request(
                    &space,
                    CLIENT,
                    &["records_read"],
                    Some(subject),
                    Some(&["ctx-a"]),
                ),
                now(),
            )
            .await
            .expect("context-scoped reader grant");

        let selection = json!({ "mode": "selected_contexts", "context_ids": ["ctx-a"] });
        let first_page = scenario
            .provider
            .records_list(
                CLIENT,
                &list_request(
                    &space,
                    subject,
                    selection.clone(),
                    &[],
                    false,
                    Some(2),
                    None,
                ),
                now(),
            )
            .await
            .expect("scoped first page");
        assert_eq!(first_page.records.len(), 2);
        assert_eq!(first_page.total_authorized, 3);
        let cursor = first_page
            .next_cursor
            .clone()
            .expect("a second page exists");
        let second_page = scenario
            .provider
            .records_list(
                CLIENT,
                &list_request(
                    &space,
                    subject,
                    selection.clone(),
                    &[],
                    false,
                    Some(2),
                    Some(&cursor),
                ),
                now(),
            )
            .await
            .expect("scoped second page");
        assert_eq!(second_page.records.len(), 1);
        assert_eq!(second_page.total_authorized, 3);
        assert!(second_page.next_cursor.is_none());
        let mut paged: Vec<String> = first_page
            .records
            .iter()
            .chain(second_page.records.iter())
            .map(|record| match record {
                RetainedRecord::LiveRecord(record) => record.record_id.to_string(),
                other => panic!("unexpected record variant {other:?}"),
            })
            .collect();
        paged.sort_unstable();
        let mut expected = ctx_a.clone();
        expected.sort_unstable();
        assert_eq!(
            paged, expected,
            "authorized rows are exact and no other placement leaks"
        );

        // The same query started again yields the same order and totals.
        let replay = scenario
            .provider
            .records_list(
                CLIENT,
                &list_request(&space, subject, selection.clone(), &[], false, None, None),
                now(),
            )
            .await
            .expect("full scoped page");
        assert_eq!(replay.total_authorized, 3);
        assert_eq!(replay.records.len(), 3);
        let replayed: Vec<String> = replay
            .records
            .iter()
            .map(|record| match record {
                RetainedRecord::LiveRecord(record) => record.record_id.to_string(),
                other => panic!("unexpected record variant {other:?}"),
            })
            .collect();
        let first_two: Vec<String> = first_page
            .records
            .iter()
            .map(|record| match record {
                RetainedRecord::LiveRecord(record) => record.record_id.to_string(),
                other => panic!("unexpected record variant {other:?}"),
            })
            .collect();
        assert_eq!(replayed[..2], first_two[..], "page order is stable");

        // Selection modes outside the grant's coverage are refused, and the
        // page bound is the published one.
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .records_list(
                        CLIENT,
                        &list_request(
                            &space,
                            subject,
                            json!({ "mode": "shared_only" }),
                            &[],
                            false,
                            None,
                            None,
                        ),
                        now(),
                    )
                    .await
            ),
            SpokeRejectCode::NotAuthorized
        );
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .records_list(
                        CLIENT,
                        &list_request(
                            &space,
                            subject,
                            json!({ "mode": "subject_all" }),
                            &[],
                            false,
                            None,
                            None,
                        ),
                        now(),
                    )
                    .await
            ),
            SpokeRejectCode::NotAuthorized
        );
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .records_list(
                        CLIENT,
                        &list_request(
                            &space,
                            subject,
                            selection.clone(),
                            &[],
                            false,
                            Some(MAX_PAGE_SIZE.get() + 1),
                            None,
                        ),
                        now(),
                    )
                    .await
            ),
            SpokeRejectCode::InvalidInput
        );

        // Any committed record change advances the snapshot and invalidates an
        // outstanding cursor instead of silently re-filtering the next page.
        scenario
            .provider
            .records_create(
                OPERATOR,
                &create_request(&space, subject, "note", "a4", Some("ctx-a"), "create-a4"),
                now(),
            )
            .await
            .expect("state change after the cursor was issued");
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .records_list(
                        CLIENT,
                        &list_request(
                            &space,
                            subject,
                            selection.clone(),
                            &[],
                            false,
                            Some(2),
                            Some(&cursor)
                        ),
                        now(),
                    )
                    .await
            ),
            SpokeRejectCode::CursorInvalid
        );

        // A shared-access grant widens the selection to shared records only
        // because the selection asks for them.
        scenario
            .provider
            .grants_grant(
                OPERATOR,
                &shared_grant_request(&space, CLIENT, &["records_read"], &["ctx-a"]),
                now(),
            )
            .await
            .expect("shared-access reader grant");
        let with_shared = scenario
            .provider
            .records_list(
                CLIENT,
                &list_request(
                    &space,
                    subject,
                    json!({
                        "mode": "selected_contexts",
                        "context_ids": ["ctx-a"],
                        "include_shared": true,
                    }),
                    &[],
                    false,
                    None,
                    None,
                ),
                now(),
            )
            .await
            .expect("shared-inclusive selection");
        assert_eq!(
            with_shared.total_authorized, 5,
            "four ctx-a records (including the post-cursor one) plus the shared one"
        );
    }

    #[tokio::test]
    async fn promote_needs_one_grant_over_both_capabilities_and_placements() {
        let scenario = Scenario::open().await;
        let space = scenario.space_id.clone();
        let subject = "subject-one";
        scenario
            .provider
            .subjects_ensure(OPERATOR, &ensure_request(&space, subject), now())
            .await
            .expect("subject exists");
        let created = scenario
            .provider
            .records_create(
                OPERATOR,
                &create_request(
                    &space,
                    subject,
                    "note",
                    "movable",
                    Some("ctx-a"),
                    "create-1",
                ),
                now(),
            )
            .await
            .expect("record exists");
        let target_ctx_b = json!({ "visibility": "context", "context_id": "ctx-b" });

        scenario
            .provider
            .grants_grant(
                OPERATOR,
                &grant_request(
                    &space,
                    CLIENT,
                    &["records_write"],
                    Some(subject),
                    Some(&["ctx-a"]),
                ),
                now(),
            )
            .await
            .expect("write-only grant");
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .records_promote(
                        CLIENT,
                        &promote_request(&space, &created.record_id, 1, target_ctx_b.clone(), "p1"),
                        now(),
                    )
                    .await
            ),
            SpokeRejectCode::NotFound,
            "records_write without promote cannot move placement"
        );

        scenario
            .provider
            .grants_grant(
                OPERATOR,
                &grant_request(
                    &space,
                    PROMOTER,
                    &["promote"],
                    Some(subject),
                    Some(&["ctx-a", "ctx-b"]),
                ),
                now(),
            )
            .await
            .expect("promote-only grant");
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .records_promote(
                        PROMOTER,
                        &promote_request(&space, &created.record_id, 1, target_ctx_b.clone(), "p2"),
                        now(),
                    )
                    .await
            ),
            SpokeRejectCode::NotFound,
            "promote without records_write cannot move placement"
        );

        scenario
            .provider
            .grants_grant(
                OPERATOR,
                &grant_request(
                    &space,
                    WRITER,
                    &["promote", "records_write"],
                    Some(subject),
                    Some(&["ctx-a"]),
                ),
                now(),
            )
            .await
            .expect("dual-capability grant limited to the source context");
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .records_promote(
                        WRITER,
                        &promote_request(&space, &created.record_id, 1, target_ctx_b.clone(), "p3"),
                        now(),
                    )
                    .await
            ),
            SpokeRejectCode::NotFound,
            "the target placement is outside the grant"
        );

        // Two disjoint grants (one per capability) never compose into
        // promotion authority.
        scenario
            .provider
            .grants_grant(
                OPERATOR,
                &grant_request(
                    &space,
                    "principal-split",
                    &["records_write"],
                    Some(subject),
                    Some(&["ctx-a", "ctx-b"]),
                ),
                now(),
            )
            .await
            .expect("split write grant");
        scenario
            .provider
            .grants_grant(
                OPERATOR,
                &grant_request(
                    &space,
                    "principal-split",
                    &["promote"],
                    Some(subject),
                    Some(&["ctx-a", "ctx-b"]),
                ),
                now(),
            )
            .await
            .expect("split promote grant");
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .records_promote(
                        "principal-split",
                        &promote_request(&space, &created.record_id, 1, target_ctx_b.clone(), "p4"),
                        now(),
                    )
                    .await
            ),
            SpokeRejectCode::NotFound,
            "capabilities split across grants are not unioned"
        );

        scenario
            .provider
            .grants_grant(
                OPERATOR,
                &grant_request(
                    &space,
                    "principal-dual",
                    &["promote", "records_write"],
                    Some(subject),
                    Some(&["ctx-a", "ctx-b"]),
                ),
                now(),
            )
            .await
            .expect("dual-capability grant over both placements");
        let promoted = scenario
            .provider
            .records_promote(
                "principal-dual",
                &promote_request(&space, &created.record_id, 1, target_ctx_b.clone(), "p5"),
                now(),
            )
            .await
            .expect("covered promotion succeeds");
        assert_eq!(promoted.revision, 2);
        assert!(matches!(
            &promoted.scope,
            PlacementScope::Context(context) if context.as_str() == "ctx-b"
        ));
        let record = scenario
            .provider
            .records_get(OPERATOR, &get_request(&space, &created.record_id), now())
            .await
            .expect("operator reads the promoted record");
        assert_eq!(
            scope_of(&live(&record).scope),
            ("context".to_owned(), Some("ctx-b".to_owned()))
        );

        // Promoting into shared visibility needs shared access in the same
        // grant; the ctx-only dual grant is refused.
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .records_promote(
                        "principal-dual",
                        &promote_request(
                            &space,
                            &created.record_id,
                            2,
                            json!({ "visibility": "shared" }),
                            "p6",
                        ),
                        now(),
                    )
                    .await
            ),
            SpokeRejectCode::NotFound
        );
        let shared = scenario
            .provider
            .records_promote(
                OPERATOR,
                &promote_request(
                    &space,
                    &created.record_id,
                    2,
                    json!({ "visibility": "shared" }),
                    "p7",
                ),
                now(),
            )
            .await
            .expect("admin promotes to shared");
        assert_eq!(shared.revision, 3);
        assert!(matches!(shared.scope, PlacementScope::Shared));
    }

    #[tokio::test]
    async fn freeze_blocks_record_mutations_but_admits_reads() {
        let scenario = Scenario::open().await;
        let space = scenario.space_id.clone();
        let subject = "subject-one";
        scenario
            .provider
            .subjects_ensure(OPERATOR, &ensure_request(&space, subject), now())
            .await
            .expect("subject exists");
        let created = scenario
            .provider
            .records_create(
                OPERATOR,
                &create_request(&space, subject, "note", "frozen", Some("ctx-a"), "create-1"),
                now(),
            )
            .await
            .expect("record exists");
        scenario
            .provider
            .spaces_freeze(OPERATOR, &freeze_request(&space, 1), now())
            .await
            .expect("space freezes");

        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .records_create(
                        OPERATOR,
                        &create_request(
                            &space,
                            subject,
                            "note",
                            "blocked",
                            Some("ctx-a"),
                            "create-2"
                        ),
                        now(),
                    )
                    .await
            ),
            SpokeRejectCode::SpaceFrozen
        );
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .records_update(
                        OPERATOR,
                        &update_content_request(
                            &space,
                            &created.record_id,
                            1,
                            "blocked",
                            "update-1"
                        ),
                        now(),
                    )
                    .await
            ),
            SpokeRejectCode::SpaceFrozen
        );
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .records_promote(
                        OPERATOR,
                        &promote_request(
                            &space,
                            &created.record_id,
                            1,
                            json!({ "visibility": "shared" }),
                            "promote-1",
                        ),
                        now(),
                    )
                    .await
            ),
            SpokeRejectCode::SpaceFrozen
        );
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .records_delete(
                        OPERATOR,
                        &delete_request(&space, &created.record_id, 1, "delete-1"),
                        now()
                    )
                    .await
            ),
            SpokeRejectCode::SpaceFrozen
        );

        // Read-only ops stay admitted while frozen (MEM-R011).
        let record = scenario
            .provider
            .records_get(OPERATOR, &get_request(&space, &created.record_id), now())
            .await
            .expect("read is admitted while frozen");
        assert_eq!(live(&record).content.text, "frozen");
        let page = scenario
            .provider
            .records_list(
                OPERATOR,
                &list_request(
                    &space,
                    subject,
                    json!({ "mode": "subject_all" }),
                    &[],
                    false,
                    None,
                    None,
                ),
                now(),
            )
            .await
            .expect("list is admitted while frozen");
        assert_eq!(page.total_authorized, 1);

        scenario
            .provider
            .spaces_unfreeze(OPERATOR, &unfreeze_request(&space, 2), now())
            .await
            .expect("space unfreezes");
        let updated = scenario
            .provider
            .records_update(
                OPERATOR,
                &update_content_request(&space, &created.record_id, 1, "writable", "update-2"),
                now(),
            )
            .await
            .expect("write resumes after unfreeze");
        assert_eq!(updated.revision, 2);
    }

    #[tokio::test]
    async fn subject_and_context_fence_refuse_record_writes() {
        let scenario = Scenario::open().await;
        let space = scenario.space_id.clone();
        let subject = "subject-one";
        scenario
            .provider
            .subjects_ensure(OPERATOR, &ensure_request(&space, subject), now())
            .await
            .expect("subject exists");
        let ctx_a = scenario
            .provider
            .records_create(
                OPERATOR,
                &create_request(&space, subject, "note", "a", Some("ctx-a"), "create-a"),
                now(),
            )
            .await
            .expect("ctx-a record");
        let ctx_b = scenario
            .provider
            .records_create(
                OPERATOR,
                &create_request(&space, subject, "note", "b", Some("ctx-b"), "create-b"),
                now(),
            )
            .await
            .expect("ctx-b record");

        scenario
            .provider
            .subjects_fence(
                OPERATOR,
                &fence_request(&space, subject, "fence", json!({ "type": "subject" }), 1),
                now(),
            )
            .await
            .expect("subject fences");
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .records_create(
                        OPERATOR,
                        &create_request(
                            &space,
                            subject,
                            "note",
                            "blocked",
                            Some("ctx-b"),
                            "create-c"
                        ),
                        now(),
                    )
                    .await
            ),
            SpokeRejectCode::SubjectFenced
        );
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .records_update(
                        OPERATOR,
                        &update_content_request(&space, &ctx_b.record_id, 1, "blocked", "update-b"),
                        now(),
                    )
                    .await
            ),
            SpokeRejectCode::SubjectFenced
        );
        // Reads are not fenced.
        scenario
            .provider
            .records_get(OPERATOR, &get_request(&space, &ctx_b.record_id), now())
            .await
            .expect("read stays admitted under a subject fence");

        scenario
            .provider
            .subjects_fence(
                OPERATOR,
                &fence_request(&space, subject, "unfence", json!({ "type": "subject" }), 2),
                now(),
            )
            .await
            .expect("subject unfences");
        scenario
            .provider
            .subjects_fence(
                OPERATOR,
                &fence_request(
                    &space,
                    subject,
                    "fence",
                    json!({ "type": "context", "context_id": "ctx-a" }),
                    3,
                ),
                now(),
            )
            .await
            .expect("ctx-a fences");
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .records_update(
                        OPERATOR,
                        &update_content_request(&space, &ctx_a.record_id, 1, "blocked", "update-a"),
                        now(),
                    )
                    .await
            ),
            SpokeRejectCode::SubjectFenced
        );
        // A sibling context stays writable.
        let sibling = scenario
            .provider
            .records_update(
                OPERATOR,
                &update_content_request(&space, &ctx_b.record_id, 1, "still-writable", "update-b2"),
                now(),
            )
            .await
            .expect("sibling context stays writable");
        assert_eq!(sibling.revision, 2);
        // Promoting out of a fenced context is refused as well.
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .records_promote(
                        OPERATOR,
                        &promote_request(
                            &space,
                            &ctx_a.record_id,
                            1,
                            json!({ "visibility": "context", "context_id": "ctx-b" }),
                            "promote-fenced",
                        ),
                        now(),
                    )
                    .await
            ),
            SpokeRejectCode::SubjectFenced
        );
    }

    #[tokio::test]
    async fn records_and_revisions_survive_reopen() {
        let scenario = Scenario::open().await;
        let space = scenario.space_id.clone();
        let subject = "subject-one";
        scenario
            .provider
            .subjects_ensure(OPERATOR, &ensure_request(&space, subject), now())
            .await
            .expect("subject exists");
        let created = scenario
            .provider
            .records_create(
                OPERATOR,
                &create_request(
                    &space,
                    subject,
                    "note",
                    "durable",
                    Some("ctx-a"),
                    "create-1",
                ),
                now(),
            )
            .await
            .expect("record exists");
        let before = scenario
            .provider
            .records_get(OPERATOR, &get_request(&space, &created.record_id), now())
            .await
            .expect("record before reopen");
        let before_live = live(&before);

        let scenario = scenario.reopen().await;
        let after = scenario
            .provider
            .records_get(OPERATOR, &get_request(&space, &created.record_id), now())
            .await
            .expect("record after reopen");
        let after_live = live(&after);
        assert_eq!(after_live.content.text, "durable");
        assert_eq!(
            scope_of(&after_live.scope),
            ("context".to_owned(), Some("ctx-a".to_owned()))
        );
        assert_eq!(after_live.revision.get(), 1);
        assert_eq!(after_live.created_at, before_live.created_at);
        assert_eq!(after_live.provenance.captured_by.as_str(), OPERATOR);

        // The durable revision remains a usable CAS token across sessions.
        let updated = scenario
            .provider
            .records_update(
                OPERATOR,
                &update_content_request(&space, &created.record_id, 1, "after-reopen", "update-1"),
                now(),
            )
            .await
            .expect("revision CAS works after reopen");
        assert_eq!(updated.revision, 2);
        let page = scenario
            .provider
            .records_list(
                OPERATOR,
                &list_request(
                    &space,
                    subject,
                    json!({ "mode": "subject_all" }),
                    &[],
                    false,
                    None,
                    None,
                ),
                now(),
            )
            .await
            .expect("durable list");
        assert_eq!(page.total_authorized, 1);
        assert_eq!(live(&page.records[0]).content.text, "after-reopen");
    }

    fn page_ids(page: &RecordPage) -> Vec<String> {
        page.records
            .iter()
            .map(|record| match record {
                RetainedRecord::LiveRecord(record) => record.record_id.to_string(),
                other => panic!("unexpected record variant {other:?}"),
            })
            .collect()
    }

    const FOREIGN: &str = "principal-foreign";
    const EXPIRING: &str = "principal-expiring";

    #[tokio::test]
    async fn foreign_and_missing_record_ids_have_no_existence_oracle() {
        let scenario = Scenario::open().await;
        let space = scenario.space_id.clone();
        let subject = "subject-one";
        scenario
            .provider
            .subjects_ensure(OPERATOR, &ensure_request(&space, subject), now())
            .await
            .expect("subject exists");
        let created = scenario
            .provider
            .records_create(
                OPERATOR,
                &create_request(
                    &space,
                    subject,
                    "note",
                    "private",
                    Some("ctx-a"),
                    "create-1",
                ),
                now(),
            )
            .await
            .expect("record exists");
        scenario
            .provider
            .grants_grant(
                OPERATOR,
                &grant_request(
                    &space,
                    FOREIGN,
                    &["records_read", "records_write"],
                    Some(subject),
                    Some(&["ctx-b"]),
                ),
                now(),
            )
            .await
            .expect("foreign placement grant");
        let before = marker(&scenario).await;
        let missing = "rec_00000000000000000000000000000000";
        let target_ctx_b = json!({ "visibility": "context", "context_id": "ctx-b" });

        // Every by-id operation answers a foreign placement exactly like an
        // absent id: same code, message and details.
        assert_same_refusal(
            scenario
                .provider
                .records_get(FOREIGN, &get_request(&space, &created.record_id), now())
                .await,
            scenario
                .provider
                .records_get(FOREIGN, &get_request(&space, missing), now())
                .await,
        );
        assert_same_refusal(
            scenario
                .provider
                .records_update(
                    FOREIGN,
                    &update_content_request(&space, &created.record_id, 1, "probe", "u-foreign"),
                    now(),
                )
                .await,
            scenario
                .provider
                .records_update(
                    FOREIGN,
                    &update_content_request(&space, missing, 1, "probe", "u-missing"),
                    now(),
                )
                .await,
        );
        assert_same_refusal(
            scenario
                .provider
                .records_delete(
                    FOREIGN,
                    &delete_request(&space, &created.record_id, 1, "d-foreign"),
                    now(),
                )
                .await,
            scenario
                .provider
                .records_delete(
                    FOREIGN,
                    &delete_request(&space, missing, 1, "d-missing"),
                    now(),
                )
                .await,
        );
        assert_same_refusal(
            scenario
                .provider
                .records_promote(
                    FOREIGN,
                    &promote_request(
                        &space,
                        &created.record_id,
                        1,
                        target_ctx_b.clone(),
                        "p-foreign",
                    ),
                    now(),
                )
                .await,
            scenario
                .provider
                .records_promote(
                    FOREIGN,
                    &promote_request(&space, missing, 1, target_ctx_b.clone(), "p-missing"),
                    now(),
                )
                .await,
        );

        // A principal with no grants observes the same single refusal.
        assert_same_refusal(
            scenario
                .provider
                .records_get(
                    "principal-outsider",
                    &get_request(&space, &created.record_id),
                    now(),
                )
                .await,
            scenario
                .provider
                .records_get("principal-outsider", &get_request(&space, missing), now())
                .await,
        );

        // Refusals leave the record and the private snapshot untouched.
        assert_eq!(marker(&scenario).await, before);
        let (_, revision, tombstone) = stored_row(&scenario, &created.record_id).await;
        assert_eq!(revision, 1);
        assert!(tombstone.is_none());

        // Legitimate reads and CAS still answer precisely to the admin.
        let record = scenario
            .provider
            .records_get(OPERATOR, &get_request(&space, &created.record_id), now())
            .await
            .expect("admin reads the record");
        assert_eq!(live(&record).content.text, "private");
        let updated = scenario
            .provider
            .records_update(
                OPERATOR,
                &update_content_request(&space, &created.record_id, 1, "authorized", "u-admin"),
                now(),
            )
            .await
            .expect("authorized CAS update");
        assert_eq!(updated.revision, 2);
        let stale = refusal(
            scenario
                .provider
                .records_update(
                    OPERATOR,
                    &update_content_request(&space, &created.record_id, 1, "lost", "u-stale"),
                    now(),
                )
                .await,
        );
        assert_eq!(stale.code, SpokeRejectCode::RevisionConflict);
    }

    #[tokio::test]
    async fn tombstoned_records_cannot_regain_content_or_placement() {
        let scenario = Scenario::open().await;
        let space = scenario.space_id.clone();
        let subject = "subject-one";
        scenario
            .provider
            .subjects_ensure(OPERATOR, &ensure_request(&space, subject), now())
            .await
            .expect("subject exists");
        let created = scenario
            .provider
            .records_create(
                OPERATOR,
                &create_request(&space, subject, "note", "erased", Some("ctx-a"), "create-1"),
                now(),
            )
            .await
            .expect("record exists");
        scenario
            .provider
            .records_delete(
                OPERATOR,
                &delete_request(&space, &created.record_id, 1, "delete-1"),
                now(),
            )
            .await
            .expect("record is tombstoned");
        let after_delete = marker(&scenario).await;
        let (scrubbed, revision, tombstone) = stored_row(&scenario, &created.record_id).await;
        assert_eq!(scrubbed, "{\"text\":\"\"}");
        assert_eq!(revision, 2);
        assert!(tombstone.is_some());

        // No update can write content back onto a deleted record, and no
        // promotion can move it, whatever revision the caller claims.
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .records_update(
                        OPERATOR,
                        &update_content_request(
                            &space,
                            &created.record_id,
                            2,
                            "resurrected",
                            "u-1"
                        ),
                        now(),
                    )
                    .await
            ),
            SpokeRejectCode::NotFound
        );
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .records_update(
                        OPERATOR,
                        &update_content_request(
                            &space,
                            &created.record_id,
                            1,
                            "resurrected",
                            "u-2"
                        ),
                        now(),
                    )
                    .await
            ),
            SpokeRejectCode::NotFound
        );
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .records_promote(
                        OPERATOR,
                        &promote_request(
                            &space,
                            &created.record_id,
                            2,
                            json!({ "visibility": "shared" }),
                            "p-1",
                        ),
                        now(),
                    )
                    .await
            ),
            SpokeRejectCode::NotFound
        );

        // Durable scrub, revision and snapshot are preserved.
        assert_eq!(marker(&scenario).await, after_delete);
        let (scrubbed, revision, tombstone) = stored_row(&scenario, &created.record_id).await;
        assert_eq!(scrubbed, "{\"text\":\"\"}");
        assert_eq!(revision, 2);
        assert!(tombstone.is_some());

        // The authorized audit view still decodes the tombstone.
        let record = scenario
            .provider
            .records_get(OPERATOR, &get_request(&space, &created.record_id), now())
            .await
            .expect("admin audit view");
        let record = tombstoned(&record);
        assert_eq!(record.content.text, "");
        assert_eq!(record.revision.get(), 2);
        assert_eq!(record.tombstone.deleted_by.as_str(), OPERATOR);

        // Repeat delete at the current revision remains the accepted no-op.
        let repeated = scenario
            .provider
            .records_delete(
                OPERATOR,
                &delete_request(&space, &created.record_id, 2, "delete-2"),
                now(),
            )
            .await
            .expect("repeat delete is a no-op success");
        assert!(repeated.tombstoned);
        assert_eq!(marker(&scenario).await, after_delete);
    }

    #[tokio::test]
    async fn delete_obeys_subject_fence_but_not_context_fence() {
        let scenario = Scenario::open().await;
        let space = scenario.space_id.clone();
        let subject = "subject-one";
        scenario
            .provider
            .subjects_ensure(OPERATOR, &ensure_request(&space, subject), now())
            .await
            .expect("subject exists");
        let ctx_a = scenario
            .provider
            .records_create(
                OPERATOR,
                &create_request(&space, subject, "note", "a", Some("ctx-a"), "create-a"),
                now(),
            )
            .await
            .expect("ctx-a record");
        let ctx_b = scenario
            .provider
            .records_create(
                OPERATOR,
                &create_request(&space, subject, "note", "b", Some("ctx-b"), "create-b"),
                now(),
            )
            .await
            .expect("ctx-b record");

        scenario
            .provider
            .subjects_fence(
                OPERATOR,
                &fence_request(&space, subject, "fence", json!({ "type": "subject" }), 1),
                now(),
            )
            .await
            .expect("subject fences");
        let fenced_marker = marker(&scenario).await;
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .records_delete(
                        OPERATOR,
                        &delete_request(&space, &ctx_a.record_id, 1, "d-fenced"),
                        now()
                    )
                    .await
            ),
            SpokeRejectCode::SubjectFenced
        );
        // No tombstone, revision or snapshot change from the refused erase.
        assert_eq!(marker(&scenario).await, fenced_marker);
        let (_, revision, tombstone) = stored_row(&scenario, &ctx_a.record_id).await;
        assert_eq!((revision, tombstone.is_none()), (1, true));

        scenario
            .provider
            .subjects_fence(
                OPERATOR,
                &fence_request(&space, subject, "unfence", json!({ "type": "subject" }), 2),
                now(),
            )
            .await
            .expect("subject unfences");
        scenario
            .provider
            .subjects_fence(
                OPERATOR,
                &fence_request(
                    &space,
                    subject,
                    "fence",
                    json!({ "type": "context", "context_id": "ctx-a" }),
                    3,
                ),
                now(),
            )
            .await
            .expect("ctx-a fences");

        // A context fence is placement-scoped: it does not invent an erasure
        // ban for records already placed in that context.
        let erased = scenario
            .provider
            .records_delete(
                OPERATOR,
                &delete_request(&space, &ctx_a.record_id, 1, "d-ctx"),
                now(),
            )
            .await
            .expect("context fence does not ban erasure");
        assert!(erased.tombstoned);
        let erased = scenario
            .provider
            .records_delete(
                OPERATOR,
                &delete_request(&space, &ctx_b.record_id, 1, "d-sibling"),
                now(),
            )
            .await
            .expect("sibling context erasure");
        assert!(erased.tombstoned);

        // An archived subject rejects erasure like a fence does.
        let later = scenario
            .provider
            .records_create(
                OPERATOR,
                &create_request(&space, subject, "note", "later", Some("ctx-b"), "create-c"),
                now(),
            )
            .await
            .expect("record before archive");
        scenario
            .provider
            .subjects_fence(
                OPERATOR,
                &fence_request(&space, subject, "archive", json!({ "type": "subject" }), 4),
                now(),
            )
            .await
            .expect("subject archives");
        let archived_marker = marker(&scenario).await;
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .records_delete(
                        OPERATOR,
                        &delete_request(&space, &later.record_id, 1, "d-archived"),
                        now()
                    )
                    .await
            ),
            SpokeRejectCode::SubjectFenced
        );
        assert_eq!(marker(&scenario).await, archived_marker);
        let (_, revision, tombstone) = stored_row(&scenario, &later.record_id).await;
        assert_eq!((revision, tombstone.is_none()), (1, true));
    }

    #[tokio::test]
    async fn content_bound_is_exact_jcs_bytes_with_utf8_and_structured() {
        assert_eq!(MAX_CONTENT_BYTES, 65_536);
        let scenario = Scenario::open().await;
        let space = scenario.space_id.clone();
        let subject = "subject-one";
        scenario
            .provider
            .subjects_ensure(OPERATOR, &ensure_request(&space, subject), now())
            .await
            .expect("subject exists");
        let text_prefix = "{\"text\":\"\"}".len();

        // Exactly max_content_bytes of JCS-serialized ASCII content is admitted.
        let boundary_text = "a".repeat(MAX_CONTENT_BYTES - text_prefix);
        let created_ascii = scenario
            .provider
            .records_create(
                OPERATOR,
                &create_with_content_request(
                    &space,
                    subject,
                    "note",
                    json!({ "text": boundary_text }),
                    Some("ctx-a"),
                    "create-boundary",
                ),
                now(),
            )
            .await
            .expect("content at the exact bound is admitted");
        let record = scenario
            .provider
            .records_get(
                OPERATOR,
                &get_request(&space, &created_ascii.record_id),
                now(),
            )
            .await
            .expect("boundary record reads back");
        assert_eq!(live(&record).content.text, boundary_text);
        assert_eq!(
            live(&record).content.text.len(),
            MAX_CONTENT_BYTES - text_prefix
        );

        // One byte over is refused with the published limit details.
        let overflow = refusal(
            scenario
                .provider
                .records_create(
                    OPERATOR,
                    &create_with_content_request(
                        &space,
                        subject,
                        "note",
                        json!({ "text": "a".repeat(MAX_CONTENT_BYTES - text_prefix + 1) }),
                        Some("ctx-a"),
                        "create-overflow",
                    ),
                    now(),
                )
                .await,
        );
        assert_eq!(overflow.code, SpokeRejectCode::InvalidInput);
        let details = overflow.details.expect("limit details");
        assert_eq!(details["field"], json!("content"));
        assert_eq!(details["limit"], json!("max_content_bytes"));
        let page = scenario
            .provider
            .records_list(
                OPERATOR,
                &list_request(
                    &space,
                    subject,
                    json!({ "mode": "subject_all" }),
                    &[],
                    false,
                    None,
                    None,
                ),
                now(),
            )
            .await
            .expect("list after refused create");
        assert_eq!(
            page.total_authorized, 1,
            "the refused create stored nothing"
        );

        // The bound counts UTF-8 bytes, not characters: 32,763 two-byte
        // characters are 65,526 bytes and overflow even though the scalar
        // count alone is far below the limit.
        let utf8_boundary = format!("{}{}", "é".repeat(32_762), "a");
        assert_eq!(utf8_boundary.len(), MAX_CONTENT_BYTES - text_prefix);
        let created_utf8 = scenario
            .provider
            .records_create(
                OPERATOR,
                &create_with_content_request(
                    &space,
                    subject,
                    "note",
                    json!({ "text": utf8_boundary }),
                    Some("ctx-a"),
                    "create-utf8",
                ),
                now(),
            )
            .await
            .expect("multi-byte content at the exact bound is admitted");
        let record = scenario
            .provider
            .records_get(
                OPERATOR,
                &get_request(&space, &created_utf8.record_id),
                now(),
            )
            .await
            .expect("multi-byte boundary record reads back");
        assert_eq!(live(&record).content.text, utf8_boundary);
        let utf8_overflow = "é".repeat(32_763);
        assert_eq!(utf8_overflow.len(), MAX_CONTENT_BYTES - text_prefix + 1);
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .records_create(
                        OPERATOR,
                        &create_with_content_request(
                            &space,
                            subject,
                            "note",
                            json!({ "text": utf8_overflow }),
                            Some("ctx-a"),
                            "create-utf8-overflow",
                        ),
                        now(),
                    )
                    .await
            ),
            SpokeRejectCode::InvalidInput
        );

        // Structured payloads count through canonical (key-sorted) JCS bytes.
        let structured_base = "{\"structured\":{\"pad\":\"\"},\"text\":\"\"}";
        let pad = "x".repeat(MAX_CONTENT_BYTES - structured_base.len());
        let structured = json!({ "text": "", "structured": { "pad": pad } });
        let created_structured = scenario
            .provider
            .records_create(
                OPERATOR,
                &create_with_content_request(
                    &space,
                    subject,
                    "note",
                    structured.clone(),
                    Some("ctx-a"),
                    "create-structured",
                ),
                now(),
            )
            .await
            .expect("structured content at the exact bound is admitted");
        let record = scenario
            .provider
            .records_get(
                OPERATOR,
                &get_request(&space, &created_structured.record_id),
                now(),
            )
            .await
            .expect("structured record reads back");
        assert_eq!(
            live(&record).content.structured.as_ref(),
            Some(&structured["structured"])
        );
        let structured_overflow = json!({
            "text": "",
            "structured": { "pad": "x".repeat(MAX_CONTENT_BYTES - structured_base.len() + 1) },
        });
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .records_create(
                        OPERATOR,
                        &create_with_content_request(
                            &space,
                            subject,
                            "note",
                            structured_overflow,
                            Some("ctx-a"),
                            "create-structured-overflow",
                        ),
                        now(),
                    )
                    .await
            ),
            SpokeRejectCode::InvalidInput
        );

        // The update path applies the same serialized-byte bound. Each exact
        // update uses the record's true current revision and is read back; each
        // overflow uses that record's *new* current revision, so INVALID_INPUT
        // cannot be a mislabelled CAS refusal.
        let before_updates = marker(&scenario).await;

        let ascii_updated = "b".repeat(MAX_CONTENT_BYTES - text_prefix);
        let updated = scenario
            .provider
            .records_update(
                OPERATOR,
                &update_content_request(
                    &space,
                    &created_ascii.record_id,
                    1,
                    &ascii_updated,
                    "update-ascii-exact",
                ),
                now(),
            )
            .await
            .expect("ASCII update at the exact bound succeeds");
        assert_eq!(updated.revision, 2);
        let record = scenario
            .provider
            .records_get(
                OPERATOR,
                &get_request(&space, &created_ascii.record_id),
                now(),
            )
            .await
            .expect("ASCII update reads back");
        assert_eq!(live(&record).content.text, ascii_updated);
        assert_eq!(
            live(&record).content.text.len(),
            MAX_CONTENT_BYTES - text_prefix
        );
        assert_eq!(live(&record).revision.get(), 2);
        assert_eq!(marker(&scenario).await, before_updates + 1);
        let overflow = refusal(
            scenario
                .provider
                .records_update(
                    OPERATOR,
                    &update_content_request(
                        &space,
                        &created_ascii.record_id,
                        2,
                        &"c".repeat(MAX_CONTENT_BYTES - text_prefix + 1),
                        "update-ascii-overflow",
                    ),
                    now(),
                )
                .await,
        );
        assert_eq!(overflow.code, SpokeRejectCode::InvalidInput);
        let details = overflow.details.expect("limit details");
        assert_eq!(details["field"], json!("content"));
        assert_eq!(details["limit"], json!("max_content_bytes"));
        assert_eq!(
            marker(&scenario).await,
            before_updates + 1,
            "refusal has no effect"
        );
        let record = scenario
            .provider
            .records_get(
                OPERATOR,
                &get_request(&space, &created_ascii.record_id),
                now(),
            )
            .await
            .expect("ASCII record after refused update");
        assert_eq!(live(&record).content.text, ascii_updated);
        assert_eq!(live(&record).revision.get(), 2);

        let utf8_updated = format!("{}{}", "é".repeat(32_762), "b");
        assert_eq!(utf8_updated.len(), MAX_CONTENT_BYTES - text_prefix);
        let updated = scenario
            .provider
            .records_update(
                OPERATOR,
                &update_content_request(
                    &space,
                    &created_utf8.record_id,
                    1,
                    &utf8_updated,
                    "update-utf8-exact",
                ),
                now(),
            )
            .await
            .expect("multi-byte update at the exact bound succeeds");
        assert_eq!(updated.revision, 2);
        let record = scenario
            .provider
            .records_get(
                OPERATOR,
                &get_request(&space, &created_utf8.record_id),
                now(),
            )
            .await
            .expect("multi-byte update reads back");
        assert_eq!(live(&record).content.text, utf8_updated);
        assert_eq!(
            live(&record).content.text.len(),
            MAX_CONTENT_BYTES - text_prefix
        );
        assert_eq!(live(&record).revision.get(), 2);
        assert_eq!(marker(&scenario).await, before_updates + 2);
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .records_update(
                        OPERATOR,
                        &update_content_request(
                            &space,
                            &created_utf8.record_id,
                            2,
                            &"é".repeat(32_763),
                            "update-utf8-overflow",
                        ),
                        now(),
                    )
                    .await
            ),
            SpokeRejectCode::InvalidInput
        );
        assert_eq!(
            marker(&scenario).await,
            before_updates + 2,
            "refusal has no effect"
        );
        let record = scenario
            .provider
            .records_get(
                OPERATOR,
                &get_request(&space, &created_utf8.record_id),
                now(),
            )
            .await
            .expect("multi-byte record after refused update");
        assert_eq!(live(&record).content.text, utf8_updated);
        assert_eq!(live(&record).revision.get(), 2);

        let updated_pad = "y".repeat(MAX_CONTENT_BYTES - structured_base.len());
        let structured_update: MemoryRecordsUpdateRequest = serde_json::from_value(json!({
            "space_id": &space,
            "record_id": &created_structured.record_id,
            "expected_revision": 1,
            "content": { "text": "", "structured": { "pad": &updated_pad } },
            "idempotency_key": "update-structured-exact",
        }))
        .expect("valid structured records.update request");
        let updated = scenario
            .provider
            .records_update(OPERATOR, &structured_update, now())
            .await
            .expect("structured update at the exact bound succeeds");
        assert_eq!(updated.revision, 2);
        let record = scenario
            .provider
            .records_get(
                OPERATOR,
                &get_request(&space, &created_structured.record_id),
                now(),
            )
            .await
            .expect("structured update reads back");
        assert_eq!(live(&record).content.text, "");
        assert_eq!(
            live(&record).content.structured.as_ref(),
            Some(&json!({ "pad": &updated_pad }))
        );
        assert_eq!(live(&record).revision.get(), 2);
        assert_eq!(marker(&scenario).await, before_updates + 3);
        let structured_overflow_update: MemoryRecordsUpdateRequest = serde_json::from_value(json!({
            "space_id": &space,
            "record_id": &created_structured.record_id,
            "expected_revision": 2,
            "content": {
                "text": "",
                "structured": { "pad": "z".repeat(MAX_CONTENT_BYTES - structured_base.len() + 1) },
            },
            "idempotency_key": "update-structured-overflow",
        }))
        .expect("valid structured records.update request");
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .records_update(OPERATOR, &structured_overflow_update, now())
                    .await
            ),
            SpokeRejectCode::InvalidInput
        );
        assert_eq!(
            marker(&scenario).await,
            before_updates + 3,
            "refusal has no effect"
        );
        let record = scenario
            .provider
            .records_get(
                OPERATOR,
                &get_request(&space, &created_structured.record_id),
                now(),
            )
            .await
            .expect("structured record after refused update");
        // The complete content is unchanged, not only the structured member.
        assert_eq!(live(&record).content.text, "");
        assert_eq!(
            live(&record).content.structured.as_ref(),
            Some(&json!({ "pad": &updated_pad }))
        );
        assert_eq!(live(&record).revision.get(), 2);
    }

    #[tokio::test]
    async fn by_id_reads_recheck_revocation_and_expiry() {
        let scenario = Scenario::open().await;
        let space = scenario.space_id.clone();
        let subject = "subject-one";
        scenario
            .provider
            .subjects_ensure(OPERATOR, &ensure_request(&space, subject), now())
            .await
            .expect("subject exists");
        let created = scenario
            .provider
            .records_create(
                OPERATOR,
                &create_request(
                    &space,
                    subject,
                    "note",
                    "guarded",
                    Some("ctx-a"),
                    "create-1",
                ),
                now(),
            )
            .await
            .expect("record exists");

        let issued = scenario
            .provider
            .grants_grant(
                OPERATOR,
                &grant_request(
                    &space,
                    CLIENT,
                    &["records_read"],
                    Some(subject),
                    Some(&["ctx-a"]),
                ),
                now(),
            )
            .await
            .expect("reader grant");
        scenario
            .provider
            .records_get(CLIENT, &get_request(&space, &created.record_id), now())
            .await
            .expect("reader sees the record before revocation");

        scenario
            .provider
            .grants_revoke(
                OPERATOR,
                &revoke_request(&space, &issued.grant_id, 1),
                now(),
            )
            .await
            .expect("reader grant revokes");
        let revoked = marker(&scenario).await;
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .records_get(CLIENT, &get_request(&space, &created.record_id), now())
                    .await
            ),
            SpokeRejectCode::NotFound
        );
        assert_eq!(marker(&scenario).await, revoked, "refusal has no effect");
        let (_, revision, _) = stored_row(&scenario, &created.record_id).await;
        assert_eq!(revision, 1);

        // Expiry is evaluated at the supplied admission instant.
        let expiry = now() + Duration::hours(1);
        scenario
            .provider
            .grants_grant(
                OPERATOR,
                &expiring_grant_request(
                    &space,
                    EXPIRING,
                    &["records_read", "records_write"],
                    subject,
                    &["ctx-a"],
                    expiry,
                ),
                now(),
            )
            .await
            .expect("expiring grant");
        scenario
            .provider
            .records_get(EXPIRING, &get_request(&space, &created.record_id), now())
            .await
            .expect("grant is valid before its expiry");
        let updated = scenario
            .provider
            .records_update(
                EXPIRING,
                &update_content_request(&space, &created.record_id, 1, "before-expiry", "u-1"),
                now(),
            )
            .await
            .expect("write grant is valid before its expiry");
        assert_eq!(updated.revision, 2);

        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .records_get(EXPIRING, &get_request(&space, &created.record_id), expiry)
                    .await
            ),
            SpokeRejectCode::NotFound
        );
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .records_update(
                        EXPIRING,
                        &update_content_request(
                            &space,
                            &created.record_id,
                            2,
                            "after-expiry",
                            "u-2"
                        ),
                        expiry,
                    )
                    .await
            ),
            SpokeRejectCode::NotFound
        );
        let (_, revision, _) = stored_row(&scenario, &created.record_id).await;
        assert_eq!(revision, 2, "expired refusals leave the record untouched");
    }

    #[tokio::test]
    async fn caller_owned_transactions_compose_or_discard_record_effects() {
        let scenario = Scenario::open().await;
        let space = scenario.space_id.clone();
        let subject = "subject-one";
        scenario
            .provider
            .subjects_ensure(OPERATOR, &ensure_request(&space, subject), now())
            .await
            .expect("subject exists");
        let before = marker(&scenario).await;

        // Rolled back caller transaction: neither record nor snapshot survives.
        let mut transaction = scenario
            .provider
            .store()
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .expect("caller transaction");
        let created = scenario
            .provider
            .records_create_in_tx(
                &mut transaction,
                OPERATOR,
                &create_request(&space, subject, "note", "staged", Some("ctx-a"), "create-1"),
                now(),
            )
            .await
            .expect("staged create");
        assert_eq!(created.revision, 1);
        let updated = scenario
            .provider
            .records_update_in_tx(
                &mut transaction,
                OPERATOR,
                &update_content_request(&space, &created.record_id, 1, "staged-update", "u-1"),
                now(),
            )
            .await
            .expect("staged update composes in the same transaction");
        assert_eq!(updated.revision, 2);
        // Uncommitted effects are invisible to another connection.
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .records_get(OPERATOR, &get_request(&space, &created.record_id), now())
                    .await
            ),
            SpokeRejectCode::NotFound
        );
        transaction.rollback().await.expect("caller rollback");
        assert_eq!(marker(&scenario).await, before);
        let mut conn = scenario
            .provider
            .store()
            .pool()
            .acquire()
            .await
            .expect("scenario connection");
        let row = sqlx::query("SELECT revision FROM records WHERE record_id = ?")
            .bind(&created.record_id)
            .fetch_optional(&mut *conn)
            .await
            .expect("rollback query");
        assert!(row.is_none(), "rollback discarded the staged record");

        // Committed caller transaction: both owned effects and both snapshot
        // bumps land in C7's single commit.
        let mut transaction = scenario
            .provider
            .store()
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .expect("caller transaction");
        let created = scenario
            .provider
            .records_create_in_tx(
                &mut transaction,
                OPERATOR,
                &create_request(
                    &space,
                    subject,
                    "note",
                    "committed",
                    Some("ctx-a"),
                    "create-2",
                ),
                now(),
            )
            .await
            .expect("staged create");
        let updated = scenario
            .provider
            .records_update_in_tx(
                &mut transaction,
                OPERATOR,
                &update_content_request(&space, &created.record_id, 1, "committed-update", "u-2"),
                now(),
            )
            .await
            .expect("staged update");
        assert_eq!(updated.revision, 2);
        transaction.commit().await.expect("caller commit");
        assert_eq!(marker(&scenario).await, before + 2);
        let record = scenario
            .provider
            .records_get(OPERATOR, &get_request(&space, &created.record_id), now())
            .await
            .expect("committed record reads back");
        assert_eq!(live(&record).content.text, "committed-update");
        assert_eq!(live(&record).revision.get(), 2);
    }

    #[tokio::test]
    async fn read_snapshot_is_stable_under_committed_state_change() {
        let scenario = Scenario::open().await;
        let space = scenario.space_id.clone();
        let subject = "subject-one";
        scenario
            .provider
            .subjects_ensure(OPERATOR, &ensure_request(&space, subject), now())
            .await
            .expect("subject exists");
        let first = scenario
            .provider
            .records_create(
                OPERATOR,
                &create_request(&space, subject, "note", "first", Some("ctx-a"), "create-1"),
                now(),
            )
            .await
            .expect("first record");
        let issued = scenario
            .provider
            .grants_grant(
                OPERATOR,
                &grant_request(
                    &space,
                    CLIENT,
                    &["records_read"],
                    Some(subject),
                    Some(&["ctx-a"]),
                ),
                now(),
            )
            .await
            .expect("reader grant");
        let selection = json!({ "mode": "selected_contexts", "context_ids": ["ctx-a"] });

        let mut transaction = scenario
            .provider
            .store()
            .pool()
            .begin()
            .await
            .expect("read transaction");
        let before_change = scenario
            .provider
            .records_list_in_tx(
                &mut transaction,
                CLIENT,
                &list_request(&space, subject, selection.clone(), &[], false, None, None),
                now(),
            )
            .await
            .expect("first snapshot page");
        assert_eq!(page_ids(&before_change), vec![first.record_id.clone()]);

        // A committed write from another connection is invisible to the open
        // snapshot, including the marker the page is bound to.
        scenario
            .provider
            .records_create(
                OPERATOR,
                &create_request(&space, subject, "note", "second", Some("ctx-a"), "create-2"),
                now(),
            )
            .await
            .expect("concurrent state change");
        let after_change = scenario
            .provider
            .records_list_in_tx(
                &mut transaction,
                CLIENT,
                &list_request(&space, subject, selection.clone(), &[], false, None, None),
                now(),
            )
            .await
            .expect("same snapshot page");
        assert_eq!(after_change.total_authorized, 1);
        assert_eq!(page_ids(&after_change), page_ids(&before_change));
        transaction.rollback().await.expect("read transaction ends");

        let fresh = scenario
            .provider
            .records_list(
                CLIENT,
                &list_request(&space, subject, selection.clone(), &[], false, None, None),
                now(),
            )
            .await
            .expect("fresh read sees the commit");
        assert_eq!(fresh.total_authorized, 2);

        // The same snapshot also pins the authorization decision: a committed
        // revoke does not re-filter an already-open page.
        let mut transaction = scenario
            .provider
            .store()
            .pool()
            .begin()
            .await
            .expect("read transaction");
        let before_revoke = scenario
            .provider
            .records_list_in_tx(
                &mut transaction,
                CLIENT,
                &list_request(&space, subject, selection.clone(), &[], false, None, None),
                now(),
            )
            .await
            .expect("authorized page before revoke");
        assert_eq!(before_revoke.total_authorized, 2);
        scenario
            .provider
            .grants_revoke(
                OPERATOR,
                &revoke_request(&space, &issued.grant_id, 1),
                now(),
            )
            .await
            .expect("reader grant revokes");
        let after_revoke = scenario
            .provider
            .records_list_in_tx(
                &mut transaction,
                CLIENT,
                &list_request(&space, subject, selection.clone(), &[], false, None, None),
                now(),
            )
            .await
            .expect("same snapshot stays authorized");
        assert_eq!(after_revoke.total_authorized, 2);
        transaction.rollback().await.expect("read transaction ends");

        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .records_list(
                        CLIENT,
                        &list_request(&space, subject, selection, &[], false, None, None),
                        now(),
                    )
                    .await
            ),
            SpokeRejectCode::NotAuthorized
        );
    }
}
