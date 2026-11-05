//! Recall selections, budgeted ranked reads and trusted profile documents
//! (spoke-memory §6.12, §7.22–§7.24, §9, §10).
//!
//! Recall authorizes first, then applies the placement selection and the
//! provenance filter, ranks the authorized set and only then spends the
//! item/byte budget (§9). It reads the durable `records` table; captured
//! candidates never enter it and tombstoned rows are excluded by the row
//! predicate, so recall can never return a candidate or a dead record.
//! Profiles are the separate trusted `profiles` table: authored text never
//! flows through recall, and captured text never appears in a profile.
//!
//! Every read path takes the caller-owned connection. Authorization, the page
//! rows and the snapshot marker a cursor is bound to are therefore read in one
//! snapshot, so a change committed by another connection mid-read cannot pair
//! old rows with a new marker (§7.0.7/MEM-R026). `memory.profile.update`
//! follows the crate's mutation seam: `*_in_tx` stages the effect and its
//! snapshot bump on a caller-owned connection without beginning or committing,
//! and the outer wrapper owns the real transaction, so the receipt/idempotency
//! writer can commit both together (MEM-R022/R024). Every authorization
//! decision uses the single admission instant `now`; nothing here samples the
//! wall clock.

use std::num::NonZeroU64;

use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{json, Map, Value};
use sqlx::{Row, SqliteConnection};
use spoke_operations::spoke_schemas::memory_recall_request::{ProvenanceFilter, ReadSelection};
use spoke_operations::spoke_schemas::{
    MemoryGrant, MemoryProfileGetRequest, MemoryProfileUpdateRequest, MemoryRecallRequest,
    ProfileDocument, RetainedRecord,
};
use spoke_operations::{
    grant_covers, is_freeze_allowlisted, jcs_sha256_hex,
    validate_memory_recall, GrantRequirement, SpokeReject, SpokeRejectCode, SpokeResult,
};

use crate::provider::FixtureMemoryProvider;
use crate::spaces::{
    bump_snapshot, internal, is_authorized, load_grants, snapshot_marker,
    ENGINE_POLICY_VERSION, MAX_PAGE_SIZE,
};

/// Wire schema version carried by every fixture-served object (§6.1).
const SCHEMA_VERSION: u64 = 1;

/// Counts bytes without buffering them, for the §9 budget.
#[derive(Default)]
struct ByteCounter(usize);

impl std::io::Write for ByteCounter {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.0 += buffer.len();
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// The §9 byte budget of one record's content, measured in the representation
/// the response actually emits.
///
/// The published JCS helper counts canonical ECMAScript number spelling
/// (`1.0` as `1`), while the served response is serialized by serde_json and
/// emits `1.0`. Counting JCS bytes would understate the returned material, so
/// the budget is measured by streaming the *same* `Value` the response carries
/// through the serving serializer into a counting sink: no content is altered,
/// no ranking input changes, and no temporary buffer is allocated.
fn emitted_content_bytes(content: &Value) -> Result<usize, SpokeReject> {
    let mut counter = ByteCounter::default();
    serde_json::to_writer(&mut counter, content)
        .map_err(|error| internal(format!("recalled content is not serializable: {error}")))?;
    Ok(counter.0)
}

/// `memory.recall` payload (§7.22/§9).
#[derive(Debug)]
pub(crate) struct RecallPage {
    pub(crate) records: Vec<RetainedRecord>,
    pub(crate) total_authorized: u64,
    pub(crate) truncated: bool,
    pub(crate) overflow_record_id: Option<String>,
    pub(crate) next_cursor: Option<String>,
    pub(crate) engine_policy_version: String,
}

/// `memory.profile.get` payload (§7.23).
#[derive(Debug)]
pub(crate) struct ProfilePage {
    pub(crate) profiles: Vec<ProfileDocument>,
}

/// Content-free result of `memory.profile.update` (§7.24).
#[derive(Debug)]
pub(crate) struct ProfileChange {
    pub(crate) profile_kind: String,
    pub(crate) revision: u64,
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

fn space_frozen(space_id: &str, op: &str) -> SpokeReject {
    SpokeReject {
        code: SpokeRejectCode::SpaceFrozen,
        message: format!("space {space_id} is frozen; {op} is outside the §8.2 allowlist"),
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

fn cursor_invalid(message: impl Into<String>) -> SpokeReject {
    SpokeReject {
        code: SpokeRejectCode::CursorInvalid,
        message: message.into(),
        details: None,
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

/// `expected_revision: null` admits creation only (§7.24), so an existing
/// document is the same lost-update refusal with a `null` expectation.
fn profile_create_conflict(actual: u64) -> SpokeReject {
    let mut details = Map::new();
    details.insert("expected_revision".to_owned(), Value::Null);
    details.insert("actual_revision".to_owned(), json!(actual));
    SpokeReject {
        code: SpokeRejectCode::RevisionConflict,
        message: format!(
            "expected_revision null admits creation only; the document already exists at \
             revision {actual}"
        ),
        details: Some(details),
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
// Stored row helpers
// ---------------------------------------------------------------------------

fn timestamp(raw: &str) -> Result<DateTime<Utc>, SpokeReject> {
    DateTime::parse_from_rfc3339(raw)
        .map(|parsed| parsed.with_timezone(&Utc))
        .map_err(|error| internal(format!("stored timestamp {raw} is not RFC 3339: {error}")))
}

fn timestamp_text(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Micros, true)
}

fn optional_timestamp(raw: Option<String>) -> Result<Option<DateTime<Utc>>, SpokeReject> {
    raw.map(|value| timestamp(&value)).transpose()
}

fn json_field(raw: &str, field: &str) -> Result<Value, SpokeReject> {
    serde_json::from_str(raw)
        .map_err(|error| internal(format!("stored {field} is not valid JSON: {error}")))
}

fn row_u64(row: &sqlx::sqlite::SqliteRow, column: &str) -> Result<u64, SpokeReject> {
    let raw: i64 = row.try_get(column).map_err(internal)?;
    u64::try_from(raw).map_err(internal)
}

// ---------------------------------------------------------------------------
// Stored rows
// ---------------------------------------------------------------------------

/// One stored record row projected into what recall needs. `content` stays a
/// JSON value because the budget is measured over its serialized bytes (§9)
/// and the same value is the wire `content` object.
struct RecallRow {
    record_id: String,
    kind: String,
    content: Value,
    visibility: String,
    context_id: Option<String>,
    provenance: Value,
    import_ref: Option<Value>,
    occurred_at: Option<DateTime<Utc>>,
    revision: u64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    extensions: Value,
    /// Set per request: does the record's text carry the optional query hint?
    query_rank: bool,
}

impl RecallRow {
    fn content_text(&self) -> &str {
        self.content
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or_default()
    }
}

struct StoredProfile {
    space_id: String,
    subject_id: String,
    profile_kind: String,
    document: Value,
    updated_by: String,
    revision: u64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    extensions: Value,
}

// ---------------------------------------------------------------------------
// Loaders
// ---------------------------------------------------------------------------

async fn space_status(
    conn: &mut SqliteConnection,
    space_id: &str,
) -> Result<String, SpokeReject> {
    let row = sqlx::query("SELECT status FROM spaces WHERE space_id = ?")
        .bind(space_id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(internal)?
        .ok_or_else(|| not_found(format!("space {space_id} is not served by this fixture")))?;
    row.try_get("status").map_err(internal)
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
        .map_err(internal)?
        .ok_or_else(|| not_found(format!("subject {subject_id} does not exist in this space")))?;
    row.try_get("status").map_err(internal)
}

/// Read the live records of one subject: the tombstone predicate keeps dead
/// rows out, and candidates live in their own table so they cannot appear
/// (MEM-R004/MEM-R014).
async fn load_recall_rows(
    conn: &mut SqliteConnection,
    space_id: &str,
    subject_id: &str,
) -> Result<Vec<RecallRow>, SpokeReject> {
    let rows = sqlx::query(
        "SELECT record_id, kind, content_json, scope_visibility, scope_context_id, \
         provenance_json, import_ref_json, occurred_at, revision, created_at, updated_at, \
         extensions_json FROM records \
         WHERE space_id = ? AND subject_id = ? AND tombstone_json IS NULL \
         ORDER BY record_id",
    )
    .bind(space_id)
    .bind(subject_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(internal)?;
    rows.into_iter()
        .map(|row| {
            let import_ref_json: Option<String> = row.try_get("import_ref_json").map_err(internal)?;
            Ok(RecallRow {
                record_id: row.try_get("record_id").map_err(internal)?,
                kind: row.try_get("kind").map_err(internal)?,
                content: json_field(
                    &row.try_get::<String, _>("content_json").map_err(internal)?,
                    "record content",
                )?,
                visibility: row.try_get("scope_visibility").map_err(internal)?,
                context_id: row.try_get("scope_context_id").map_err(internal)?,
                provenance: json_field(
                    &row.try_get::<String, _>("provenance_json")
                        .map_err(internal)?,
                    "record provenance",
                )?,
                import_ref: optional_json(import_ref_json.as_deref(), "record import_ref")?,
                occurred_at: optional_timestamp(row.try_get("occurred_at").map_err(internal)?)?,
                revision: row_u64(&row, "revision")?,
                created_at: timestamp(
                    &row.try_get::<String, _>("created_at").map_err(internal)?,
                )?,
                updated_at: timestamp(
                    &row.try_get::<String, _>("updated_at").map_err(internal)?,
                )?,
                extensions: json_field(
                    &row.try_get::<String, _>("extensions_json")
                        .map_err(internal)?,
                    "record extensions",
                )?,
                query_rank: false,
            })
        })
        .collect()
}

fn optional_json(raw: Option<&str>, field: &str) -> Result<Option<Value>, SpokeReject> {
    raw.map(|value| json_field(value, field)).transpose()
}

async fn load_profiles(
    conn: &mut SqliteConnection,
    space_id: &str,
    subject_id: &str,
) -> Result<Vec<StoredProfile>, SpokeReject> {
    let rows = sqlx::query(
        "SELECT space_id, subject_id, profile_kind, document_json, updated_by, revision, \
         created_at, updated_at, extensions_json FROM profiles \
         WHERE space_id = ? AND subject_id = ? ORDER BY profile_kind",
    )
    .bind(space_id)
    .bind(subject_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(internal)?;
    rows.into_iter()
        .map(|row| {
            Ok(StoredProfile {
                space_id: row.try_get("space_id").map_err(internal)?,
                subject_id: row.try_get("subject_id").map_err(internal)?,
                profile_kind: row.try_get("profile_kind").map_err(internal)?,
                document: json_field(
                    &row.try_get::<String, _>("document_json").map_err(internal)?,
                    "profile document",
                )?,
                updated_by: row.try_get("updated_by").map_err(internal)?,
                revision: row_u64(&row, "revision")?,
                created_at: timestamp(
                    &row.try_get::<String, _>("created_at").map_err(internal)?,
                )?,
                updated_at: timestamp(
                    &row.try_get::<String, _>("updated_at").map_err(internal)?,
                )?,
                extensions: json_field(
                    &row.try_get::<String, _>("extensions_json").map_err(internal)?,
                    "profile extensions",
                )?,
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Authorization facets (§6.5, §7.22–§7.24)
// ---------------------------------------------------------------------------

/// The grant facets a recall selection needs. `subject_all` needs a grant with
/// no context restriction at all; a shared read needs the shared facet or no
/// restriction; a context selection needs coverage of every named context.
fn selection_requirement<'a>(
    selection: &'a ReadSelection,
    subject_id: &'a str,
) -> GrantRequirement<'a> {
    match selection {
        ReadSelection::SubjectAll => GrantRequirement {
            capability: "recall",
            subject_id: Some(subject_id),
            context_ids: Vec::new(),
            reads_shared: true,
            subject_all: true,
        },
        ReadSelection::SharedOnly => GrantRequirement {
            capability: "recall",
            subject_id: Some(subject_id),
            context_ids: Vec::new(),
            reads_shared: true,
            subject_all: false,
        },
        ReadSelection::SelectedContexts {
            context_ids,
            include_shared,
        } => GrantRequirement {
            capability: "recall",
            subject_id: Some(subject_id),
            context_ids: context_ids.clone(),
            reads_shared: include_shared.unwrap_or(false),
            subject_all: false,
        },
    }
}

fn grant_capabilities(grant: &MemoryGrant) -> &[String] {
    match grant {
        MemoryGrant::RestrictedGrant(grant) => &grant.capabilities,
        MemoryGrant::UnrestrictedGrant(grant) => &grant.capabilities,
    }
}

/// §7.23: a profile read needs *any* capability on the subject with shared
/// access or unrestricted coverage. The published evaluator takes one
/// capability name, so the "any" qualifier is the only facet composed here —
/// every other rule still comes from `grant_covers`.
fn any_capability_admits(
    grants: &[MemoryGrant],
    principal_id: &str,
    space_id: &str,
    now: DateTime<Utc>,
    subject_id: &str,
) -> bool {
    grants.iter().any(|grant| {
        grant_capabilities(grant).iter().any(|capability| {
            grant_covers(
                grant,
                principal_id,
                space_id,
                now,
                &GrantRequirement {
                    capability,
                    subject_id: Some(subject_id),
                    context_ids: Vec::new(),
                    reads_shared: true,
                    subject_all: false,
                },
            )
        })
    })
}

/// §8.2: a frozen space admits exactly the maintenance allowlist. The decision
/// is the published predicate; only the stored status is read here.
async fn require_op_admitted(
    conn: &mut SqliteConnection,
    op: &str,
    space_id: &str,
) -> Result<(), SpokeReject> {
    let status = space_status(conn, space_id).await?;
    if status != "frozen" || is_freeze_allowlisted(op) {
        return Ok(());
    }
    Err(space_frozen(space_id, op))
}

// ---------------------------------------------------------------------------
// Selection, filter, ranking, budget (§6.12, §9)
// ---------------------------------------------------------------------------

fn selection_admits(selection: &ReadSelection, row: &RecallRow) -> bool {
    match selection {
        ReadSelection::SubjectAll => true,
        ReadSelection::SharedOnly => row.visibility == "shared",
        ReadSelection::SelectedContexts {
            context_ids,
            include_shared,
        } => {
            if row.visibility == "shared" {
                include_shared.unwrap_or(false)
            } else {
                // The schema requires a context placement to carry its id.
                row.context_id
                    .as_deref()
                    .is_some_and(|context_id| context_ids.iter().any(|named| named == context_id))
            }
        }
    }
}

/// §6.13: a filter, never a placement statement — it intersects the record's
/// recorded provenance claim and combines with the selection by AND.
fn provenance_filter_admits(row: &RecallRow, filter: Option<&ProvenanceFilter>) -> bool {
    let Some(filter) = filter else {
        return true;
    };
    row.provenance
        .get("source_context_ids")
        .and_then(Value::as_array)
        .is_some_and(|claims| {
            claims.iter().filter_map(Value::as_str).any(|claim| {
                filter
                    .source_context_ids
                    .iter()
                    .any(|wanted| wanted == claim)
            })
        })
}

/// Fixture-owned ranking policy (§9 item 2 is provider-variable, so this is
/// deliberately not Nexus policy):
///
/// 1. records whose untrusted text carries the optional query hint first —
///    the hint orders, it never filters;
/// 2. then the most recent original event, undated last;
/// 3. then the most recently written record;
/// 4. then the record id, so the order is total for every input.
fn rank(rows: &mut [RecallRow], query: Option<&str>) {
    let needle = query.map(str::to_lowercase);
    for row in rows.iter_mut() {
        row.query_rank = needle
            .as_deref()
            .is_some_and(|needle| row.content_text().to_lowercase().contains(needle));
    }
    rows.sort_by(|left, right| {
        right
            .query_rank
            .cmp(&left.query_rank)
            .then_with(|| right.occurred_at.cmp(&left.occurred_at))
            .then_with(|| right.created_at.cmp(&left.created_at))
            .then_with(|| left.record_id.cmp(&right.record_id))
    });
}

fn record_to_wire(
    space_id: &str,
    subject_id: &str,
    row: &RecallRow,
) -> Result<RetainedRecord, SpokeReject> {
    let scope = match (row.visibility.as_str(), row.context_id.as_deref()) {
        ("shared", None) => json!({ "visibility": "shared" }),
        ("context", Some(context_id)) => {
            json!({ "visibility": "context", "context_id": context_id })
        }
        (visibility, context_id) => {
            return Err(internal(format!(
                "stored record {} has an inconsistent placement ({visibility}, {context_id:?})",
                row.record_id
            )))
        }
    };
    let mut value = json!({
        "schema_version": SCHEMA_VERSION,
        "record_id": row.record_id,
        "space_id": space_id,
        "subject_id": subject_id,
        "kind": row.kind,
        "content": row.content,
        "scope": scope,
        "provenance": row.provenance,
        "created_at": timestamp_text(row.created_at),
        "updated_at": timestamp_text(row.updated_at),
        "revision": row.revision,
        "extensions": row.extensions,
    });
    if let Some(import_ref) = &row.import_ref {
        value["import_ref"] = import_ref.clone();
    }
    if let Some(occurred_at) = row.occurred_at {
        value["occurred_at"] = json!(timestamp_text(occurred_at));
    }
    let record = serde_json::from_value(value)
        .map_err(|error| internal(format!("stored record is not protocol-shaped: {error}")))?;
    Ok(RetainedRecord::LiveRecord(record))
}

fn profile_to_wire(row: &StoredProfile) -> Result<ProfileDocument, SpokeReject> {
    serde_json::from_value(json!({
        "schema_version": SCHEMA_VERSION,
        "space_id": row.space_id,
        "subject_id": row.subject_id,
        "profile_kind": row.profile_kind,
        "document": row.document,
        "updated_by": row.updated_by,
        "created_at": timestamp_text(row.created_at),
        "updated_at": timestamp_text(row.updated_at),
        "revision": row.revision,
        "extensions": row.extensions,
    }))
    .map_err(|error| internal(format!("stored profile row is not protocol-shaped: {error}")))
}

// ---------------------------------------------------------------------------
// Operations
// ---------------------------------------------------------------------------

impl FixtureMemoryProvider {
    /// The fixture serves exactly one durable space; a request naming another
    /// one is refused before any authorization decision touches storage. The
    /// private helper on the spaces slice is same-crate but not visible here,
    /// so this slice keeps its own two-line equality guard over the public
    /// provider identity instead of widening that frozen module.
    fn require_served_fixture_space(&self, requested: &str) -> Result<(), SpokeReject> {
        if requested == self.identity.space_id {
            return Ok(());
        }
        Err(not_found(format!(
            "space {requested} is not served by this fixture"
        )))
    }

    // -----------------------------------------------------------------------
    // memory.recall (§7.22/§9)
    // -----------------------------------------------------------------------

    /// Authorized, ranked, budgeted read for execution-time context.
    pub(crate) async fn recall(
        &self,
        principal_id: &str,
        request: &MemoryRecallRequest,
        now: DateTime<Utc>,
    ) -> Result<RecallPage, SpokeReject> {
        let mut transaction = self.store().pool().begin().await.map_err(internal)?;
        let page = self
            .recall_in_tx(&mut transaction, principal_id, request, now)
            .await?;
        transaction.commit().await.map_err(internal)?;
        Ok(page)
    }

    /// [`Self::recall`] on a caller-owned connection: the whole read uses the
    /// caller's snapshot and never begins or commits a transaction.
    pub(crate) async fn recall_in_tx(
        &self,
        conn: &mut SqliteConnection,
        principal_id: &str,
        request: &MemoryRecallRequest,
        now: DateTime<Utc>,
    ) -> Result<RecallPage, SpokeReject> {
        checked(validate_memory_recall(request, MAX_PAGE_SIZE))?;
        self.require_served_fixture_space(&request.space_id)?;
        let space_id = request.space_id.as_str();
        let subject_id = request.subject_id.as_str();
        require_op_admitted(conn, "memory.recall", space_id).await?;

        // §6.5/MEM-R006: authorization before rows, counts, pages and cursors.
        let grants = load_grants(conn, space_id).await?;
        if !is_authorized(
            &grants,
            principal_id,
            space_id,
            now,
            &selection_requirement(&request.selection, subject_id),
        ) {
            return Err(not_authorized(
                "the authenticated principal holds no grant covering this recall selection",
            ));
        }

        let subject = subject_status(conn, space_id, subject_id).await?;
        let snapshot = snapshot_marker(conn, space_id).await?;
        let binding_digest = recall_binding(
            principal_id,
            space_id,
            subject_id,
            request,
            snapshot,
        )?;

        // §7.7/§11: an archived subject's records are excluded from recall;
        // reads on a merely fenced subject stay admitted. The exclusion is a
        // row-set rule, so the cursor/budget path below stays single.
        let mut rows = load_recall_rows(conn, space_id, subject_id).await?;
        if subject == "archived" {
            rows.clear();
        }
        rows.retain(|row| {
            selection_admits(&request.selection, row)
                && provenance_filter_admits(row, request.filter.as_ref())
        });
        rank(&mut rows, request.query.as_deref());
        let total_authorized = u64::try_from(rows.len()).map_err(internal)?;

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
                let position = rows
                    .iter()
                    .position(|row| row.record_id == last_id)
                    .ok_or_else(|| cursor_invalid("cursor no longer matches the recalled set"))?;
                position + 1
            }
        };

        // §9 item 3: the item cap is a hard cap, and the byte budget is spent
        // in UTF-8 bytes of the serialized content. An oversized first item is
        // surfaced whole instead of being silently dropped; any later item that
        // does not fit is deferred to the next page.
        let limit = usize::try_from(request.budget.max_items.get()).map_err(internal)?;
        let max_bytes = match request.budget.max_bytes {
            Some(budget) => Some(usize::try_from(budget.get()).map_err(internal)?),
            None => None,
        };
        let mut used = 0_usize;
        let mut returned = 0_usize;
        let mut overflow_record_id = None;
        for row in &rows[start..] {
            if returned == limit {
                break;
            }
            let size = emitted_content_bytes(&row.content)?;
            let fits = max_bytes.is_none_or(|budget| used.saturating_add(size) <= budget);
            if !fits {
                if returned == 0 {
                    overflow_record_id = Some(row.record_id.clone());
                    returned += 1;
                }
                break;
            }
            returned += 1;
            used += size;
        }

        let records = rows[start..start + returned]
            .iter()
            .map(|row| record_to_wire(space_id, subject_id, row))
            .collect::<Result<Vec<_>, _>>()?;
        let more_remaining = start + returned < rows.len();
        let next_cursor = if more_remaining {
            rows[start..start + returned]
                .last()
                .map(|row| format!("{binding_digest}:{}", row.record_id))
        } else {
            None
        };

        Ok(RecallPage {
            records,
            total_authorized,
            truncated: more_remaining,
            overflow_record_id,
            next_cursor,
            engine_policy_version: ENGINE_POLICY_VERSION.to_owned(),
        })
    }

    // -----------------------------------------------------------------------
    // memory.profile.get (§7.23)
    // -----------------------------------------------------------------------

    /// Read the trusted documents present for one subject. Absent kinds are
    /// honest-empty, never an error (§4.2).
    pub(crate) async fn profile_get(
        &self,
        principal_id: &str,
        request: &MemoryProfileGetRequest,
        now: DateTime<Utc>,
    ) -> Result<ProfilePage, SpokeReject> {
        let mut transaction = self.store().pool().begin().await.map_err(internal)?;
        let page = self
            .profile_get_in_tx(&mut transaction, principal_id, request, now)
            .await?;
        transaction.commit().await.map_err(internal)?;
        Ok(page)
    }

    /// [`Self::profile_get`] on a caller-owned connection; never begins or
    /// commits a transaction.
    pub(crate) async fn profile_get_in_tx(
        &self,
        conn: &mut SqliteConnection,
        principal_id: &str,
        request: &MemoryProfileGetRequest,
        now: DateTime<Utc>,
    ) -> Result<ProfilePage, SpokeReject> {
        self.require_served_fixture_space(&request.space_id)?;
        let space_id = request.space_id.as_str();
        let subject_id = request.subject_id.as_str();
        require_op_admitted(conn, "memory.profile.get", space_id).await?;

        // §7.23/MEM-R003: the authenticated principal decides coverage; the
        // payload never names a requester.
        let grants = load_grants(conn, space_id).await?;
        if !any_capability_admits(&grants, principal_id, space_id, now, subject_id) {
            return Err(not_authorized(
                "the authenticated principal holds no grant covering this subject's profiles",
            ));
        }
        subject_status(conn, space_id, subject_id).await?;

        let rows = load_profiles(conn, space_id, subject_id).await?;
        let wanted = &request.profile_kinds;
        let profiles = rows
            .iter()
            .filter(|row| {
                wanted.is_empty() || wanted.iter().any(|kind| kind == &row.profile_kind)
            })
            .map(profile_to_wire)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ProfilePage { profiles })
    }

    // -----------------------------------------------------------------------
    // memory.profile.update (§7.24)
    // -----------------------------------------------------------------------

    /// Create or replace one authored document under revision CAS. The
    /// document body is stored and served byte for byte: profiles are trusted
    /// authored content (§10), never derived from captured text.
    pub(crate) async fn profile_update(
        &self,
        principal_id: &str,
        request: &MemoryProfileUpdateRequest,
        now: DateTime<Utc>,
    ) -> Result<ProfileChange, SpokeReject> {
        let mut transaction = self
            .store()
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(internal)?;
        let change = self
            .profile_update_in_tx(&mut transaction, principal_id, request, now)
            .await?;
        transaction.commit().await.map_err(internal)?;
        Ok(change)
    }

    /// [`Self::profile_update`] on a caller-owned connection: the document
    /// write and its snapshot bump are staged on `conn` and never committed,
    /// so a receipt/dedup writer can commit both in one commit (MEM-R022).
    pub(crate) async fn profile_update_in_tx(
        &self,
        conn: &mut SqliteConnection,
        principal_id: &str,
        request: &MemoryProfileUpdateRequest,
        now: DateTime<Utc>,
    ) -> Result<ProfileChange, SpokeReject> {
        self.require_served_fixture_space(&request.space_id)?;
        let space_id = request.space_id.as_str();
        let subject_id = request.subject_id.as_str();
        require_op_admitted(conn, "memory.profile.update", space_id).await?;

        let grants = load_grants(conn, space_id).await?;
        // §6.5/§7.24: a profile is one subject-level trusted document, not a
        // per-context record, so its write must cover shared document access.
        // `reads_shared` is the published facet for exactly that; the plain
        // `requirement()` helper (no contexts, no shared facet) would accept a
        // context-restricted grant without `shared_access`, letting a principal
        // replace trusted text it cannot even read.
        let profile_write = GrantRequirement {
            capability: "profile_write",
            subject_id: Some(subject_id),
            context_ids: Vec::new(),
            reads_shared: true,
            subject_all: false,
        };
        if !is_authorized(&grants, principal_id, space_id, now, &profile_write) {
            return Err(not_authorized(
                "the authenticated principal holds no profile_write grant covering shared \
                 document access on this subject",
            ));
        }
        // A fenced or archived subject rejects every write (§7.7).
        if subject_status(conn, space_id, subject_id).await? != "active" {
            return Err(subject_fenced(subject_id, "subject"));
        }

        let profile_kind = request.profile_kind.to_string();
        let document = json!({
            "format": &request.document.format,
            "body": &request.document.body,
        });
        let document_json = serde_json::to_string(&document).map_err(internal)?;
        let stored: Option<i64> = sqlx::query(
            "SELECT revision FROM profiles \
             WHERE space_id = ? AND subject_id = ? AND profile_kind = ?",
        )
        .bind(space_id)
        .bind(subject_id)
        .bind(&profile_kind)
        .fetch_optional(&mut *conn)
        .await
        .map_err(internal)?
        .map(|row| row.try_get("revision").map_err(internal))
        .transpose()?;
        let stored = stored.map(u64::try_from).transpose().map_err(internal)?;

        let revision = match (request.expected_revision, stored) {
            (None, None) => {
                sqlx::query(
                    "INSERT INTO profiles (space_id, subject_id, profile_kind, document_json, \
                     updated_by, revision, created_at, updated_at) \
                     VALUES (?, ?, ?, ?, ?, 1, ?, ?)",
                )
                .bind(space_id)
                .bind(subject_id)
                .bind(&profile_kind)
                .bind(&document_json)
                .bind(principal_id)
                .bind(timestamp_text(now))
                .bind(timestamp_text(now))
                .execute(&mut *conn)
                .await
                .map_err(internal)?;
                1
            }
            (None, Some(actual)) => return Err(profile_create_conflict(actual)),
            (Some(expected), None) => return Err(revision_conflict(expected, 0)),
            (Some(expected), Some(actual)) => {
                if expected.get() != actual {
                    return Err(revision_conflict(expected, actual));
                }
                let updated = sqlx::query(
                    "UPDATE profiles SET document_json = ?, updated_by = ?, updated_at = ?, \
                     revision = revision + 1 \
                     WHERE space_id = ? AND subject_id = ? AND profile_kind = ? AND revision = ?",
                )
                .bind(&document_json)
                .bind(principal_id)
                .bind(timestamp_text(now))
                .bind(space_id)
                .bind(subject_id)
                .bind(&profile_kind)
                .bind(i64::try_from(actual).map_err(internal)?)
                .execute(&mut *conn)
                .await
                .map_err(internal)?;
                if updated.rows_affected() != 1 {
                    return Err(internal(format!(
                        "profile {profile_kind} vanished inside its own transaction"
                    )));
                }
                actual + 1
            }
        };

        // Only an accepted write is an observable change; refusals above left
        // no effect and therefore leave the marker alone.
        bump_snapshot(conn, space_id).await?;
        Ok(ProfileChange {
            profile_kind,
            revision,
        })
    }
}

/// Cursor binding (§7.0.7/MEM-R026): principal + space + subject + the exact
/// selection/filter/hint + the snapshot marker + the engine policy version. Any
/// state or grant change moves the marker, so an outstanding cursor is refused
/// instead of silently resuming over a different result set.
fn recall_binding(
    principal_id: &str,
    space_id: &str,
    subject_id: &str,
    request: &MemoryRecallRequest,
    snapshot: u64,
) -> Result<String, SpokeReject> {
    match jcs_sha256_hex(&json!({
        "principal": principal_id,
        "space": space_id,
        "subject": subject_id,
        "selection": &request.selection,
        "filter": &request.filter,
        "query": &request.query,
        "snapshot": snapshot,
        "policy": ENGINE_POLICY_VERSION,
    })) {
        SpokeResult::Ok(digest) => Ok(digest),
        SpokeResult::Reject(reject) => Err(reject),
    }
}

#[cfg(test)]
mod tests {
    use spoke_operations::content_serialized_bytes;
    use super::*;
    use std::path::PathBuf;

    use spoke_operations::spoke_schemas::{
        MemoryGrantsGrantRequest, MemoryProfileGetRequest, MemoryProfileUpdateRequest,
        MemorySpacesFreezeRequest, MemorySubjectsEnsureRequest, MemorySubjectsFenceRequest,
    };
    use tempfile::TempDir;

    const OPERATOR: &str = "principal-operator";
    const CLIENT: &str = "principal-client-b";
    const LIMITED: &str = "principal-limited";
    const STRANGER: &str = "principal-stranger";
    const SUBJECT: &str = "subj-ada";

    fn instant(text: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(text)
            .expect("valid instant")
            .with_timezone(&Utc)
    }

    /// The single admission instant every call in these cases supplies.
    fn now() -> DateTime<Utc> {
        instant("2026-11-04T10:00:00Z")
    }

    fn reject_code<T: std::fmt::Debug>(result: Result<T, SpokeReject>) -> SpokeRejectCode {
        match result {
            Ok(value) => panic!("expected a protocol reject, observed {value:?}"),
            Err(reject) => reject.code,
        }
    }

    struct Scenario {
        _home: TempDir,
        path: PathBuf,
        provider: FixtureMemoryProvider,
        space_id: String,
    }

    impl Scenario {
        async fn open() -> Self {
            let home = tempfile::tempdir().expect("scenario home");
            let path = home.path().join("scenario.sqlite");
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

        async fn ensure_subject(&self) {
            let request: MemorySubjectsEnsureRequest = serde_json::from_value(json!({
                "space_id": self.space_id,
                "subject_id": SUBJECT,
            }))
            .expect("valid subjects.ensure request");
            self.provider
                .subjects_ensure(OPERATOR, &request, now())
                .await
                .expect("subject ensured");
        }

        async fn grant(
            &self,
            grantee: &str,
            capabilities: &[&str],
            contexts: Option<&[&str]>,
            shared_access: Option<bool>,
        ) -> String {
            let mut value = json!({
                "space_id": self.space_id,
                "principal_id": grantee,
                "capabilities": capabilities,
                "subject_id": SUBJECT,
                "idempotency_key": format!(
                    "grant-{grantee}-{}-{:?}-{:?}",
                    capabilities.join("-"),
                    contexts,
                    shared_access
                ),
            });
            if let Some(contexts) = contexts {
                value["context_ids"] = json!(contexts);
            }
            if let Some(shared) = shared_access {
                value["shared_access"] = json!(shared);
            }
            let request: MemoryGrantsGrantRequest =
                serde_json::from_value(value).expect("valid grants.grant request");
            self.provider
                .grants_grant(OPERATOR, &request, now())
                .await
                .expect("grant issued")
                .grant_id
        }

        /// Arrange one durable record row. A real writer advances the private
        /// snapshot in the same commit, so the arrangement inserts the row and
        /// its marker step together.
        async fn insert_record(
            &self,
            record_id: &str,
            visibility: &str,
            context_id: Option<&str>,
            text: &str,
            source_contexts: &[&str],
            occurred_at: Option<&str>,
            created_at: &str,
        ) {
            let content = serde_json::to_string(&json!({ "text": text })).expect("content json");
            let provenance = serde_json::to_string(&json!({
                "source_context_ids": source_contexts,
                "anchors": [],
                "captured_by": CLIENT,
                "captured_at": created_at,
            }))
            .expect("provenance json");
            sqlx::query(
                "INSERT INTO records (record_id, space_id, subject_id, kind, content_json, \
                 scope_visibility, scope_context_id, provenance_json, import_ref_json, \
                 occurred_at, tombstone_json, revision, created_at, updated_at, extensions_json) \
                 VALUES (?, ?, ?, 'note', ?, ?, ?, ?, NULL, ?, NULL, 1, ?, ?, '{}')",
            )
            .bind(record_id)
            .bind(&self.space_id)
            .bind(SUBJECT)
            .bind(content)
            .bind(visibility)
            .bind(context_id)
            .bind(provenance)
            .bind(occurred_at)
            .bind(created_at)
            .bind(created_at)
            .execute(self.provider.store().pool())
            .await
            .expect("record row inserted");
            sqlx::query("UPDATE provider_snapshot SET marker = marker + 1 WHERE space_id = ?")
                .bind(&self.space_id)
                .execute(self.provider.store().pool())
                .await
                .expect("snapshot advance");
        }

        async fn marker(&self) -> u64 {
            let mut conn = self
                .provider
                .store()
                .pool()
                .acquire()
                .await
                .expect("connection");
            snapshot_marker(&mut conn, &self.space_id)
                .await
                .expect("snapshot marker")
        }

        async fn raw_profile(&self, profile_kind: &str) -> Option<String> {
            let row = sqlx::query(
                "SELECT document_json FROM profiles \
                 WHERE space_id = ? AND subject_id = ? AND profile_kind = ?",
            )
            .bind(&self.space_id)
            .bind(SUBJECT)
            .bind(profile_kind)
            .fetch_optional(self.provider.store().pool())
            .await
            .expect("profile row");
            row.map(|row| {
                row.try_get::<String, _>("document_json")
                    .expect("stored document")
            })
        }

        async fn raw_record(&self, record_id: &str) -> String {
            let row = sqlx::query("SELECT content_json FROM records WHERE record_id = ?")
                .bind(record_id)
                .fetch_one(self.provider.store().pool())
                .await
                .expect("record row");
            row.try_get::<String, _>("content_json")
                .expect("stored content")
        }
    }

    fn budget(max_items: u64, max_bytes: Option<u64>) -> Value {
        let mut value = json!({ "max_items": max_items });
        if let Some(max_bytes) = max_bytes {
            value["max_bytes"] = json!(max_bytes);
        }
        value
    }

    fn recall_request(
        space: &str,
        selection: Value,
        budget: Value,
        cursor: Option<&str>,
        filter: Option<Value>,
        query: Option<&str>,
    ) -> MemoryRecallRequest {
        let mut value = json!({
            "space_id": space,
            "subject_id": SUBJECT,
            "selection": selection,
            "budget": budget,
        });
        if let Some(cursor) = cursor {
            value["cursor"] = json!(cursor);
        }
        if let Some(filter) = filter {
            value["filter"] = filter;
        }
        if let Some(query) = query {
            value["query"] = json!(query);
        }
        serde_json::from_value(value).expect("valid memory.recall request")
    }

    fn subject_all() -> Value {
        json!({ "mode": "subject_all" })
    }

    fn shared_only() -> Value {
        json!({ "mode": "shared_only" })
    }

    fn selected_contexts(context_ids: &[&str], include_shared: Option<bool>) -> Value {
        let mut value = json!({ "mode": "selected_contexts", "context_ids": context_ids });
        if let Some(include_shared) = include_shared {
            value["include_shared"] = json!(include_shared);
        }
        value
    }

    fn profile_get_request(space: &str, kinds: &[&str]) -> MemoryProfileGetRequest {
        serde_json::from_value(json!({
            "space_id": space,
            "subject_id": SUBJECT,
            "profile_kinds": kinds,
        }))
        .expect("valid profile.get request")
    }

    fn profile_update_request(
        space: &str,
        profile_kind: &str,
        body: &str,
        format: &str,
        expected: Option<u64>,
    ) -> MemoryProfileUpdateRequest {
        serde_json::from_value(json!({
            "space_id": space,
            "subject_id": SUBJECT,
            "profile_kind": profile_kind,
            "document": { "format": format, "body": body },
            "expected_revision": expected,
            "idempotency_key": format!("profile-{profile_kind}-{expected:?}"),
        }))
        .expect("valid profile.update request")
    }

    fn recalled_ids(page: &RecallPage) -> Vec<&str> {
        page.records
            .iter()
            .map(|record| match record {
                RetainedRecord::LiveRecord(record) => record.record_id.as_str(),
                RetainedRecord::TombstonedRecord(_) => {
                    panic!("recall must never return a tombstoned record")
                }
            })
            .collect()
    }

    fn sorted_ids(page: &RecallPage) -> Vec<&str> {
        let mut ids = recalled_ids(page);
        ids.sort_unstable();
        ids
    }

    fn recalled_text<'a>(record: &'a RetainedRecord) -> &'a str {
        match record {
            RetainedRecord::LiveRecord(record) => record.content.text.as_str(),
            RetainedRecord::TombstonedRecord(_) => {
                panic!("recall must never return a tombstoned record")
            }
        }
    }

    // -----------------------------------------------------------------------
    // Selection (§6.12, C-050/C-051/C-052/C-053)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn selection_modes_admit_exactly_the_placed_rows() {
        let scenario = Scenario::open().await;
        scenario.ensure_subject().await;
        scenario.grant(CLIENT, &["recall"], None, None).await;
        scenario
            .insert_record(
                "rec_shared",
                "shared",
                None,
                "shared row",
                &[],
                None,
                "2026-01-01T00:00:00Z",
            )
            .await;
        scenario
            .insert_record(
                "rec_a",
                "context",
                Some("ctx-a"),
                "a row",
                &[],
                None,
                "2026-01-02T00:00:00Z",
            )
            .await;
        scenario
            .insert_record(
                "rec_b",
                "context",
                Some("ctx-b"),
                "b row",
                &[],
                None,
                "2026-01-03T00:00:00Z",
            )
            .await;

        let page = scenario
            .provider
            .recall(
                CLIENT,
                &recall_request(
                    &scenario.space_id,
                    subject_all(),
                    budget(10, None),
                    None,
                    None,
                    None,
                ),
                now(),
            )
            .await
            .expect("subject_all");
        assert_eq!(page.total_authorized, 3);
        assert!(!page.truncated);
        assert_eq!(sorted_ids(&page), vec!["rec_a", "rec_b", "rec_shared"]);

        let page = scenario
            .provider
            .recall(
                CLIENT,
                &recall_request(
                    &scenario.space_id,
                    shared_only(),
                    budget(10, None),
                    None,
                    None,
                    None,
                ),
                now(),
            )
            .await
            .expect("shared_only");
        assert_eq!(page.total_authorized, 1);
        assert_eq!(sorted_ids(&page), vec!["rec_shared"]);

        let page = scenario
            .provider
            .recall(
                CLIENT,
                &recall_request(
                    &scenario.space_id,
                    selected_contexts(&["ctx-a"], None),
                    budget(10, None),
                    None,
                    None,
                    None,
                ),
                now(),
            )
            .await
            .expect("selected_contexts without shared");
        assert_eq!(sorted_ids(&page), vec!["rec_a"]);

        let page = scenario
            .provider
            .recall(
                CLIENT,
                &recall_request(
                    &scenario.space_id,
                    selected_contexts(&["ctx-a"], Some(true)),
                    budget(10, None),
                    None,
                    None,
                    None,
                ),
                now(),
            )
            .await
            .expect("selected_contexts including shared");
        assert_eq!(sorted_ids(&page), vec!["rec_a", "rec_shared"]);

        let page = scenario
            .provider
            .recall(
                CLIENT,
                &recall_request(
                    &scenario.space_id,
                    selected_contexts(&["ctx-a", "ctx-b"], None),
                    budget(10, None),
                    None,
                    None,
                    None,
                ),
                now(),
            )
            .await
            .expect("two contexts");
        assert_eq!(sorted_ids(&page), vec!["rec_a", "rec_b"]);
    }

    #[tokio::test]
    async fn scoped_grant_cannot_widen_the_selection() {
        let scenario = Scenario::open().await;
        scenario.ensure_subject().await;
        scenario
            .insert_record(
                "rec_shared",
                "shared",
                None,
                "shared row",
                &[],
                None,
                "2026-01-01T00:00:00Z",
            )
            .await;
        scenario
            .insert_record(
                "rec_a",
                "context",
                Some("ctx-a"),
                "a row",
                &[],
                None,
                "2026-01-02T00:00:00Z",
            )
            .await;
        scenario
            .insert_record(
                "rec_b",
                "context",
                Some("ctx-b"),
                "b row",
                &[],
                None,
                "2026-01-03T00:00:00Z",
            )
            .await;

        scenario
            .grant(LIMITED, &["recall"], Some(&["ctx-a"]), None)
            .await;

        let request = recall_request(
            &scenario.space_id,
            subject_all(),
            budget(10, None),
            None,
            None,
            None,
        );
        assert_eq!(
            reject_code(scenario.provider.recall(LIMITED, &request, now()).await),
            SpokeRejectCode::NotAuthorized
        );
        let request = recall_request(
            &scenario.space_id,
            shared_only(),
            budget(10, None),
            None,
            None,
            None,
        );
        assert_eq!(
            reject_code(scenario.provider.recall(LIMITED, &request, now()).await),
            SpokeRejectCode::NotAuthorized
        );
        let request = recall_request(
            &scenario.space_id,
            selected_contexts(&["ctx-a", "ctx-b"], None),
            budget(10, None),
            None,
            None,
            None,
        );
        assert_eq!(
            reject_code(scenario.provider.recall(LIMITED, &request, now()).await),
            SpokeRejectCode::NotAuthorized
        );
        let request = recall_request(
            &scenario.space_id,
            selected_contexts(&["ctx-a"], Some(true)),
            budget(10, None),
            None,
            None,
            None,
        );
        assert_eq!(
            reject_code(scenario.provider.recall(LIMITED, &request, now()).await),
            SpokeRejectCode::NotAuthorized
        );

        // The named context alone is inside the coverage.
        let request = recall_request(
            &scenario.space_id,
            selected_contexts(&["ctx-a"], None),
            budget(10, None),
            None,
            None,
            None,
        );
        let page = scenario
            .provider
            .recall(LIMITED, &request, now())
            .await
            .expect("covered context");
        assert_eq!(sorted_ids(&page), vec!["rec_a"]);

        // Shared access widens the same grant only to shared visibility, and
        // still never to subject_all.
        scenario
            .grant(LIMITED, &["recall"], Some(&["ctx-a"]), Some(true))
            .await;
        let request = recall_request(
            &scenario.space_id,
            selected_contexts(&["ctx-a"], Some(true)),
            budget(10, None),
            None,
            None,
            None,
        );
        let page = scenario
            .provider
            .recall(LIMITED, &request, now())
            .await
            .expect("shared access");
        assert_eq!(sorted_ids(&page), vec!["rec_a", "rec_shared"]);
        let request = recall_request(
            &scenario.space_id,
            subject_all(),
            budget(10, None),
            None,
            None,
            None,
        );
        assert_eq!(
            reject_code(scenario.provider.recall(LIMITED, &request, now()).await),
            SpokeRejectCode::NotAuthorized
        );
    }

    #[tokio::test]
    async fn invalid_selection_filter_and_budget_are_invalid_input_with_no_effects() {
        let scenario = Scenario::open().await;
        scenario.ensure_subject().await;
        scenario.grant(CLIENT, &["recall"], None, None).await;
        let marker = scenario.marker().await;

        let request = recall_request(
            &scenario.space_id,
            selected_contexts(&[], None),
            budget(10, None),
            None,
            None,
            None,
        );
        assert_eq!(
            reject_code(scenario.provider.recall(CLIENT, &request, now()).await),
            SpokeRejectCode::InvalidInput
        );

        let request = recall_request(
            &scenario.space_id,
            subject_all(),
            budget(10, None),
            None,
            Some(json!({ "source_context_ids": [] })),
            None,
        );
        assert_eq!(
            reject_code(scenario.provider.recall(CLIENT, &request, now()).await),
            SpokeRejectCode::InvalidInput
        );

        let request = recall_request(
            &scenario.space_id,
            subject_all(),
            budget(MAX_PAGE_SIZE.get() + 1, None),
            None,
            None,
            None,
        );
        assert_eq!(
            reject_code(scenario.provider.recall(CLIENT, &request, now()).await),
            SpokeRejectCode::InvalidInput
        );

        assert_eq!(scenario.marker().await, marker);
        assert_eq!(
            scenario
                .provider
                .profile_get(CLIENT, &profile_get_request(&scenario.space_id, &[]), now())
                .await
                .expect("honest-empty profile read")
                .profiles
                .len(),
            0
        );
    }

    // -----------------------------------------------------------------------
    // Budget and cursor (§9, C-055/C-056)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn utf8_budget_counts_bytes_and_surfaces_an_oversized_first_item() {
        let scenario = Scenario::open().await;
        scenario.ensure_subject().await;
        scenario.grant(CLIENT, &["recall"], None, None).await;
        scenario
            .insert_record(
                "rec_two",
                "shared",
                None,
                "zz",
                &[],
                None,
                "2026-01-01T00:00:00Z",
            )
            .await;
        scenario
            .insert_record(
                "rec_one",
                "shared",
                None,
                "é",
                &[],
                None,
                "2026-01-02T00:00:00Z",
            )
            .await;

        // rec_one ranks first (most recently written). A one-byte budget cannot
        // hold its serialized content, but the first item is surfaced whole
        // instead of being dropped.
        let page = scenario
            .provider
            .recall(
                CLIENT,
                &recall_request(
                    &scenario.space_id,
                    subject_all(),
                    budget(10, Some(1)),
                    None,
                    None,
                    None,
                ),
                now(),
            )
            .await
            .expect("oversized first item");
        assert_eq!(recalled_ids(&page), vec!["rec_one"]);
        assert_eq!(page.overflow_record_id.as_deref(), Some("rec_one"));
        assert!(page.truncated);
        assert!(page.next_cursor.is_some());

        // The item cap is a hard cap; the cursor continues the same ranked set.
        let page = scenario
            .provider
            .recall(
                CLIENT,
                &recall_request(
                    &scenario.space_id,
                    subject_all(),
                    budget(1, None),
                    None,
                    None,
                    None,
                ),
                now(),
            )
            .await
            .expect("item cap");
        assert_eq!(recalled_ids(&page), vec!["rec_one"]);
        assert!(page.truncated);
        assert!(page.overflow_record_id.is_none());
        let cursor = page.next_cursor.expect("a truncated page issues a cursor");
        let page = scenario
            .provider
            .recall(
                CLIENT,
                &recall_request(
                    &scenario.space_id,
                    subject_all(),
                    budget(1, None),
                    Some(&cursor),
                    None,
                    None,
                ),
                now(),
            )
            .await
            .expect("second page");
        assert_eq!(recalled_ids(&page), vec!["rec_two"]);
        assert!(!page.truncated);
        assert!(page.next_cursor.is_none());

        // The budget is measured in UTF-8 bytes of the serialized content, not
        // in characters: a one-character multi-byte body is larger than one
        // character of budget.
        let one = content_serialized_bytes(&json!({ "text": "é" }));
        let two = content_serialized_bytes(&json!({ "text": "zz" }));
        assert!(one > 1, "a multi-byte body must serialize to more bytes");
        let page = scenario
            .provider
            .recall(
                CLIENT,
                &recall_request(
                    &scenario.space_id,
                    subject_all(),
                    budget(10, Some(u64::try_from(one).expect("size fits"))),
                    None,
                    None,
                    None,
                ),
                now(),
            )
            .await
            .expect("exact byte fit");
        assert_eq!(recalled_ids(&page), vec!["rec_one"]);
        assert!(page.overflow_record_id.is_none());
        assert!(page.truncated);
        let page = scenario
            .provider
            .recall(
                CLIENT,
                &recall_request(
                    &scenario.space_id,
                    subject_all(),
                    budget(10, Some(u64::try_from(one + two).expect("size fits"))),
                    None,
                    None,
                    None,
                ),
                now(),
            )
            .await
            .expect("both fit");
        assert_eq!(recalled_ids(&page), vec!["rec_one", "rec_two"]);
        assert!(!page.truncated);
        assert!(page.next_cursor.is_none());
    }

    #[tokio::test]
    async fn cursor_pages_are_bound_and_state_change_invalidates_them() {
        let scenario = Scenario::open().await;
        scenario.ensure_subject().await;
        scenario.grant(CLIENT, &["recall"], None, None).await;
        for index in 1..=4 {
            scenario
                .insert_record(
                    &format!("rec_{index}"),
                    "shared",
                    None,
                    &format!("row {index}"),
                    &[],
                    None,
                    &format!("2026-01-0{}T00:00:00Z", index + 1),
                )
                .await;
        }

        let first = scenario
            .provider
            .recall(
                CLIENT,
                &recall_request(
                    &scenario.space_id,
                    subject_all(),
                    budget(2, None),
                    None,
                    None,
                    None,
                ),
                now(),
            )
            .await
            .expect("first page");
        assert_eq!(first.total_authorized, 4);
        assert_eq!(recalled_ids(&first), vec!["rec_4", "rec_3"]);
        assert!(first.truncated);
        let cursor = first.next_cursor.expect("a truncated page issues a cursor");

        let second = scenario
            .provider
            .recall(
                CLIENT,
                &recall_request(
                    &scenario.space_id,
                    subject_all(),
                    budget(2, None),
                    Some(&cursor),
                    None,
                    None,
                ),
                now(),
            )
            .await
            .expect("second page");
        assert_eq!(recalled_ids(&second), vec!["rec_2", "rec_1"]);
        assert!(!second.truncated);
        assert!(second.next_cursor.is_none());

        // A cursor issued for another selection never resumes this one.
        let request = recall_request(
            &scenario.space_id,
            shared_only(),
            budget(2, None),
            Some(&cursor),
            None,
            None,
        );
        assert_eq!(
            reject_code(scenario.provider.recall(CLIENT, &request, now()).await),
            SpokeRejectCode::CursorInvalid
        );

        // A committed state change moves the snapshot marker, so an outstanding
        // cursor is refused instead of silently resuming over new state.
        scenario.grant(STRANGER, &["recall"], None, None).await;
        let request = recall_request(
            &scenario.space_id,
            subject_all(),
            budget(2, None),
            Some(&cursor),
            None,
            None,
        );
        assert_eq!(
            reject_code(scenario.provider.recall(CLIENT, &request, now()).await),
            SpokeRejectCode::CursorInvalid
        );

        let request = recall_request(
            &scenario.space_id,
            subject_all(),
            budget(2, None),
            Some("not-a-cursor"),
            None,
            None,
        );
        assert_eq!(
            reject_code(scenario.provider.recall(CLIENT, &request, now()).await),
            SpokeRejectCode::CursorInvalid
        );
    }

    // -----------------------------------------------------------------------
    // Ranking and filter (§9, C-058/C-059)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn ranking_is_deterministic_and_the_query_hint_only_orders() {
        let scenario = Scenario::open().await;
        scenario.ensure_subject().await;
        scenario.grant(CLIENT, &["recall"], None, None).await;
        let records: [(&str, &str, Option<&str>, &str); 7] = [
            (
                "rec_alpaca",
                "an alpaca fact",
                Some("2026-01-01T00:00:00Z"),
                "2026-01-01T00:00:00Z",
            ),
            (
                "rec_new",
                "newest event",
                Some("2026-03-01T00:00:00Z"),
                "2026-01-02T00:00:00Z",
            ),
            (
                "rec_fresh",
                "fresh event",
                Some("2026-03-01T00:00:00Z"),
                "2026-01-07T00:00:00Z",
            ),
            (
                "rec_tie_b",
                "tie b",
                Some("2026-02-01T00:00:00Z"),
                "2026-01-05T00:00:00Z",
            ),
            (
                "rec_tie_a",
                "tie a",
                Some("2026-02-01T00:00:00Z"),
                "2026-01-05T00:00:00Z",
            ),
            (
                "rec_old",
                "old event",
                Some("2025-01-01T00:00:00Z"),
                "2026-01-03T00:00:00Z",
            ),
            ("rec_none", "undated", None, "2026-01-04T00:00:00Z"),
        ];
        for (record_id, text, occurred_at, created_at) in records {
            scenario
                .insert_record(record_id, "shared", None, text, &[], occurred_at, created_at)
                .await;
        }

        let request = recall_request(
            &scenario.space_id,
            subject_all(),
            budget(10, None),
            None,
            None,
            None,
        );
        let first = scenario
            .provider
            .recall(CLIENT, &request, now())
            .await
            .expect("ranked");
        assert_eq!(first.total_authorized, 7);
        assert_eq!(
            recalled_ids(&first),
            vec![
                "rec_fresh",
                "rec_new",
                "rec_tie_a",
                "rec_tie_b",
                "rec_alpaca",
                "rec_old",
                "rec_none",
            ]
        );
        let again = scenario
            .provider
            .recall(CLIENT, &request, now())
            .await
            .expect("ranked again");
        assert_eq!(recalled_ids(&again), recalled_ids(&first));

        // The hint orders matching records first and never filters.
        let request = recall_request(
            &scenario.space_id,
            subject_all(),
            budget(10, None),
            None,
            None,
            Some("alpaca"),
        );
        let hinted = scenario
            .provider
            .recall(CLIENT, &request, now())
            .await
            .expect("hinted");
        assert_eq!(hinted.total_authorized, 7);
        assert_eq!(
            recalled_ids(&hinted),
            vec![
                "rec_alpaca",
                "rec_fresh",
                "rec_new",
                "rec_tie_a",
                "rec_tie_b",
                "rec_old",
                "rec_none",
            ]
        );

        // The provenance filter is ANDed with the selection and never rewrites
        // placement: a record placed in ctx-a is filtered by its origin.
        scenario
            .insert_record(
                "rec_origin",
                "shared",
                None,
                "originated",
                &["ctx-a"],
                None,
                "2026-01-08T00:00:00Z",
            )
            .await;
        scenario
            .insert_record(
                "rec_foreign",
                "shared",
                None,
                "elsewhere",
                &["ctx-z"],
                None,
                "2026-01-09T00:00:00Z",
            )
            .await;
        let request = recall_request(
            &scenario.space_id,
            subject_all(),
            budget(10, None),
            None,
            Some(json!({ "source_context_ids": ["ctx-a"] })),
            None,
        );
        let filtered = scenario
            .provider
            .recall(CLIENT, &request, now())
            .await
            .expect("filtered");
        let mut ids = recalled_ids(&filtered);
        ids.sort_unstable();
        assert_eq!(ids, vec!["rec_origin"]);
    }

    #[tokio::test]
    async fn archived_subject_is_excluded_from_recall_without_losing_its_rows() {
        let scenario = Scenario::open().await;
        scenario.ensure_subject().await;
        scenario.grant(CLIENT, &["recall"], None, None).await;
        scenario
            .insert_record(
                "rec_archived",
                "shared",
                None,
                "still stored",
                &[],
                None,
                "2026-01-01T00:00:00Z",
            )
            .await;

        let request = recall_request(
            &scenario.space_id,
            subject_all(),
            budget(10, None),
            None,
            None,
            None,
        );
        assert_eq!(
            recalled_ids(&scenario.provider.recall(CLIENT, &request, now()).await.expect("live")),
            vec!["rec_archived"]
        );

        let archive: MemorySubjectsFenceRequest = serde_json::from_value(json!({
            "space_id": scenario.space_id,
            "subject_id": SUBJECT,
            "target": { "type": "subject" },
            "action": "archive",
            "expected_revision": 1,
            "idempotency_key": "archive-subject",
        }))
        .expect("valid subjects.fence request");
        scenario
            .provider
            .subjects_fence(OPERATOR, &archive, now())
            .await
            .expect("subject archived");

        let page = scenario
            .provider
            .recall(CLIENT, &request, now())
            .await
            .expect("archived subject still answers");
        assert!(page.records.is_empty());
        assert_eq!(page.total_authorized, 0);
        assert!(!page.truncated);
        assert!(page.overflow_record_id.is_none());

        // Exclusion is a read rule, not a deletion: the row is still stored.
        let rows = sqlx::query("SELECT COUNT(*) AS stored FROM records WHERE record_id = ?")
            .bind("rec_archived")
            .fetch_one(scenario.provider.store().pool())
            .await
            .expect("record count");
        let stored: i64 = rows.try_get("stored").expect("count");
        assert_eq!(stored, 1);
    }

    // -----------------------------------------------------------------------
    // Profiles (§7.23/§7.24, C-060/C-061)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn profile_documents_preserve_bytes_and_enforce_revision_cas() {
        let scenario = Scenario::open().await;
        scenario.ensure_subject().await;
        scenario
            .grant(CLIENT, &["profile_write"], None, None)
            .await;
        let first_body = "Naïve draft — \"quoted\", backslash \\ and a newline\nend 🐙";
        let second_body = "Second body\nwith two lines\tand a tab";

        let change = scenario
            .provider
            .profile_update(
                CLIENT,
                &profile_update_request(
                    &scenario.space_id,
                    "personality",
                    first_body,
                    "markdown",
                    None,
                ),
                now(),
            )
            .await
            .expect("create");
        assert_eq!(change.profile_kind, "personality");
        assert_eq!(change.revision, 1);

        let page = scenario
            .provider
            .profile_get(CLIENT, &profile_get_request(&scenario.space_id, &[]), now())
            .await
            .expect("get");
        assert_eq!(page.profiles.len(), 1);
        let stored = &page.profiles[0];
        assert_eq!(stored.document.body.as_bytes(), first_body.as_bytes());
        assert_eq!(stored.updated_by.to_string(), CLIENT);
        assert_eq!(stored.created_at, now());
        assert_eq!(stored.updated_at.0, now());
        assert_eq!(stored.revision.get(), 1);

        let change = scenario
            .provider
            .profile_update(
                CLIENT,
                &profile_update_request(
                    &scenario.space_id,
                    "personality",
                    second_body,
                    "markdown",
                    Some(1),
                ),
                now(),
            )
            .await
            .expect("update");
        assert_eq!(change.revision, 2);

        // A stale expectation is refused and leaves the stored bytes alone.
        let stale = scenario
            .provider
            .profile_update(
                CLIENT,
                &profile_update_request(
                    &scenario.space_id,
                    "personality",
                    "stale body",
                    "markdown",
                    Some(1),
                ),
                now(),
            )
            .await;
        assert_eq!(reject_code(stale), SpokeRejectCode::RevisionConflict);
        assert_eq!(
            scenario.raw_profile("personality").await.as_deref(),
            Some(
                serde_json::to_string(&json!({ "format": "markdown", "body": second_body }))
                    .expect("document json")
                    .as_str()
            )
        );

        // `null` admits creation only.
        let create_only = scenario
            .provider
            .profile_update(
                CLIENT,
                &profile_update_request(
                    &scenario.space_id,
                    "personality",
                    "second create",
                    "markdown",
                    None,
                ),
                now(),
            )
            .await;
        assert_eq!(
            reject_code(create_only),
            SpokeRejectCode::RevisionConflict
        );

        // An expectation on an absent document is the same lost-update refusal.
        let absent = scenario
            .provider
            .profile_update(
                CLIENT,
                &profile_update_request(
                    &scenario.space_id,
                    "preferences",
                    "no document",
                    "json",
                    Some(3),
                ),
                now(),
            )
            .await;
        assert_eq!(
            reject_code(absent),
            SpokeRejectCode::RevisionConflict
        );
        assert!(scenario.raw_profile("preferences").await.is_none());

        // The accepted body survives a real close and reopen of the file.
        let scenario = scenario.reopen().await;
        let page = scenario
            .provider
            .profile_get(CLIENT, &profile_get_request(&scenario.space_id, &[]), now())
            .await
            .expect("get after reopen");
        assert_eq!(page.profiles.len(), 1);
        assert_eq!(page.profiles[0].document.body, second_body);
        assert_eq!(page.profiles[0].revision.get(), 2);
    }

    #[tokio::test]
    async fn profile_reads_need_shared_or_unrestricted_coverage_and_writes_need_profile_write() {
        let scenario = Scenario::open().await;
        scenario.ensure_subject().await;
        for (kind, body) in [
            ("personality", "calm"),
            ("preferences", "concise"),
        ] {
            scenario
                .provider
                .profile_update(
                    OPERATOR,
                    &profile_update_request(&scenario.space_id, kind, body, "markdown", None),
                    now(),
                )
                .await
                .expect("operator authors a profile");
        }

        // No grant at all: neither read nor write is admitted.
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .profile_get(STRANGER, &profile_get_request(&scenario.space_id, &[]), now())
                    .await
            ),
            SpokeRejectCode::NotAuthorized
        );
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .profile_update(
                        STRANGER,
                        &profile_update_request(
                            &scenario.space_id,
                            "personality",
                            "intruder",
                            "markdown",
                            None,
                        ),
                        now(),
                    )
                    .await
            ),
            SpokeRejectCode::NotAuthorized
        );

        // Any capability with unrestricted coverage admits the read (§7.23),
        // but only profile_write admits the write (§7.24).
        scenario.grant(CLIENT, &["recall"], None, None).await;
        let page = scenario
            .provider
            .profile_get(CLIENT, &profile_get_request(&scenario.space_id, &[]), now())
            .await
            .expect("recall-only read");
        assert_eq!(page.profiles.len(), 2);
        assert_eq!(
            page.profiles
                .iter()
                .map(|profile| profile.profile_kind.as_str())
                .collect::<Vec<_>>(),
            vec!["personality", "preferences"]
        );
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .profile_update(
                        CLIENT,
                        &profile_update_request(
                            &scenario.space_id,
                            "personality",
                            "without profile_write",
                            "markdown",
                            Some(1),
                        ),
                        now(),
                    )
                    .await
            ),
            SpokeRejectCode::NotAuthorized
        );

        // A context-limited grant without shared access cannot read a profile;
        // the shared facet is what widens it.
        scenario
            .grant(LIMITED, &["recall"], Some(&["ctx-a"]), None)
            .await;
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .profile_get(LIMITED, &profile_get_request(&scenario.space_id, &[]), now())
                    .await
            ),
            SpokeRejectCode::NotAuthorized
        );
        scenario
            .grant(LIMITED, &["recall"], Some(&["ctx-a"]), Some(true))
            .await;
        assert_eq!(
            scenario
                .provider
                .profile_get(LIMITED, &profile_get_request(&scenario.space_id, &[]), now())
                .await
                .expect("shared access reads profiles")
                .profiles
                .len(),
            2
        );

        // Kind filtering is exact, and an absent kind is honest-empty.
        let page = scenario
            .provider
            .profile_get(
                CLIENT,
                &profile_get_request(&scenario.space_id, &["preferences"]),
                now(),
            )
            .await
            .expect("filtered get");
        assert_eq!(page.profiles.len(), 1);
        assert_eq!(page.profiles[0].profile_kind.as_str(), "preferences");
        let page = scenario
            .provider
            .profile_get(
                CLIENT,
                &profile_get_request(&scenario.space_id, &["absent"]),
                now(),
            )
            .await
            .expect("absent kind");
        assert!(page.profiles.is_empty());

        // profile_write on the same principal admits the update.
        scenario
            .grant(CLIENT, &["profile_write"], None, None)
            .await;
        let change = scenario
            .provider
            .profile_update(
                CLIENT,
                &profile_update_request(
                    &scenario.space_id,
                    "personality",
                    "revised",
                    "markdown",
                    Some(1),
                ),
                now(),
            )
            .await
            .expect("authorized update");
        assert_eq!(change.revision, 2);

        // §6.5/§7.24: a profile is a subject-level trusted document, so the
        // write grant must cover shared document access exactly as the read
        // does. A context-restricted grant without shared access never reaches
        // the CAS: no create, no replace, and no bytes, revision or marker
        // change.
        scenario
            .grant(LIMITED, &["profile_write"], Some(&["ctx-a"]), None)
            .await;
        let marker = scenario.marker().await;
        let document = scenario
            .raw_profile("personality")
            .await
            .expect("stored document");
        let refused = scenario
            .provider
            .profile_update(
                LIMITED,
                &profile_update_request(
                    &scenario.space_id,
                    "personality",
                    "context-only overwrite",
                    "markdown",
                    Some(2),
                ),
                now(),
            )
            .await;
        assert_eq!(reject_code(refused), SpokeRejectCode::NotAuthorized);
        let refused = scenario
            .provider
            .profile_update(
                LIMITED,
                &profile_update_request(
                    &scenario.space_id,
                    "style",
                    "context-only create",
                    "markdown",
                    None,
                ),
                now(),
            )
            .await;
        assert_eq!(reject_code(refused), SpokeRejectCode::NotAuthorized);
        assert_eq!(scenario.marker().await, marker);
        assert_eq!(
            scenario.raw_profile("personality").await.as_deref(),
            Some(document.as_str())
        );
        assert!(scenario.raw_profile("style").await.is_none());
        assert_eq!(
            scenario
                .provider
                .profile_get(CLIENT, &profile_get_request(&scenario.space_id, &[]), now())
                .await
                .expect("get after refusals")
                .profiles
                .iter()
                .find(|profile| profile.profile_kind.as_str() == "personality")
                .expect("personality profile")
                .revision
                .get(),
            2
        );

        // Two partial grants on disjoint contexts are never unioned into
        // shared coverage.
        scenario
            .grant(LIMITED, &["profile_write"], Some(&["ctx-b"]), None)
            .await;
        let union_marker = scenario.marker().await;
        let refused = scenario
            .provider
            .profile_update(
                LIMITED,
                &profile_update_request(
                    &scenario.space_id,
                    "personality",
                    "union overwrite",
                    "markdown",
                    Some(2),
                ),
                now(),
            )
            .await;
        assert_eq!(reject_code(refused), SpokeRejectCode::NotAuthorized);
        assert_eq!(scenario.marker().await, union_marker);
        assert_eq!(
            scenario.raw_profile("personality").await.as_deref(),
            Some(document.as_str())
        );

        // The shared facet on the same context restriction is the legal
        // control: the document is read and written again.
        scenario
            .grant(LIMITED, &["profile_write"], Some(&["ctx-a"]), Some(true))
            .await;
        let change = scenario
            .provider
            .profile_update(
                LIMITED,
                &profile_update_request(
                    &scenario.space_id,
                    "personality",
                    "shared context write",
                    "markdown",
                    Some(2),
                ),
                now(),
            )
            .await
            .expect("shared access admits the write");
        assert_eq!(change.revision, 3);
        assert_eq!(
            scenario
                .provider
                .profile_get(LIMITED, &profile_get_request(&scenario.space_id, &[]), now())
                .await
                .expect("shared access reads the written document")
                .profiles
                .iter()
                .find(|profile| profile.profile_kind.as_str() == "personality")
                .expect("personality profile")
                .document
                .body,
            "shared context write"
        );
    }

    #[tokio::test]
    async fn captured_text_and_authored_profiles_stay_separate() {
        let scenario = Scenario::open().await;
        scenario.ensure_subject().await;
        scenario.grant(CLIENT, &["recall"], None, None).await;
        scenario
            .grant(CLIENT, &["profile_write"], None, None)
            .await;
        scenario
            .insert_record(
                "rec_captured",
                "shared",
                None,
                "captured-marker-7f3",
                &[],
                None,
                "2026-01-01T00:00:00Z",
            )
            .await;
        scenario
            .provider
            .profile_update(
                CLIENT,
                &profile_update_request(
                    &scenario.space_id,
                    "personality",
                    "authored-marker-9c1",
                    "markdown",
                    None,
                ),
                now(),
            )
            .await
            .expect("authored profile");

        let record_before = scenario.raw_record("rec_captured").await;
        let profile_before = scenario
            .raw_profile("personality")
            .await
            .expect("stored profile");

        let page = scenario
            .provider
            .recall(
                CLIENT,
                &recall_request(
                    &scenario.space_id,
                    subject_all(),
                    budget(10, None),
                    None,
                    None,
                    None,
                ),
                now(),
            )
            .await
            .expect("recall");
        assert_eq!(page.records.len(), 1);
        assert_eq!(recalled_text(&page.records[0]), "captured-marker-7f3");
        let serialized = serde_json::to_string(&page.records).expect("records json");
        assert!(!serialized.contains("authored-marker-9c1"));

        let profiles = scenario
            .provider
            .profile_get(CLIENT, &profile_get_request(&scenario.space_id, &[]), now())
            .await
            .expect("profile get");
        assert_eq!(profiles.profiles.len(), 1);
        assert_eq!(
            profiles.profiles[0].document.body,
            "authored-marker-9c1"
        );
        let serialized = serde_json::to_string(&profiles.profiles).expect("profiles json");
        assert!(!serialized.contains("captured-marker-7f3"));

        // Authoring a new profile document never rewrote captured text, and a
        // recall never rewrote an authored document.
        scenario
            .provider
            .profile_update(
                CLIENT,
                &profile_update_request(
                    &scenario.space_id,
                    "personality",
                    "authored-marker-9c1-revised",
                    "markdown",
                    Some(1),
                ),
                now(),
            )
            .await
            .expect("profile update");
        assert_eq!(scenario.raw_record("rec_captured").await, record_before);
        let page = scenario
            .provider
            .recall(
                CLIENT,
                &recall_request(
                    &scenario.space_id,
                    subject_all(),
                    budget(10, None),
                    None,
                    None,
                    None,
                ),
                now(),
            )
            .await
            .expect("recall again");
        assert_eq!(recalled_ids(&page), vec!["rec_captured"]);
        assert_eq!(
            scenario.raw_profile("personality").await,
            Some(
                serde_json::to_string(&json!({
                    "format": "markdown",
                    "body": "authored-marker-9c1-revised",
                }))
                .expect("document json")
            )
        );
        assert_ne!(
            scenario.raw_profile("personality").await.expect("profile"),
            profile_before
        );
    }

    // -----------------------------------------------------------------------
    // Mutation seam and admission gate (§7.0.4/§8.2, MEM-R022/R024)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn profile_mutation_composes_with_the_caller_transaction_and_the_admission_gate() {
        let scenario = Scenario::open().await;
        scenario.ensure_subject().await;
        scenario
            .grant(CLIENT, &["recall", "profile_write"], None, None)
            .await;
        scenario
            .insert_record(
                "rec_shared",
                "shared",
                None,
                "row",
                &[],
                None,
                "2026-01-01T00:00:00Z",
            )
            .await;
        let baseline = scenario.marker().await;

        // A caller rollback discards both the staged document and its marker
        // step: the mutation owns no commit of its own.
        let mut transaction = scenario
            .provider
            .store()
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .expect("caller transaction");
        let change = scenario
            .provider
            .profile_update_in_tx(
                &mut transaction,
                CLIENT,
                &profile_update_request(
                    &scenario.space_id,
                    "personality",
                    "staged then rolled back",
                    "markdown",
                    None,
                ),
                now(),
            )
            .await
            .expect("staged create");
        assert_eq!(change.revision, 1);
        transaction.rollback().await.expect("rollback");
        assert_eq!(scenario.marker().await, baseline);
        assert!(scenario.raw_profile("personality").await.is_none());

        // A caller commit applies the document and its marker step together.
        let mut transaction = scenario
            .provider
            .store()
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .expect("caller transaction");
        scenario
            .provider
            .profile_update_in_tx(
                &mut transaction,
                CLIENT,
                &profile_update_request(
                    &scenario.space_id,
                    "personality",
                    "committed by the caller",
                    "markdown",
                    None,
                ),
                now(),
            )
            .await
            .expect("staged create");
        transaction.commit().await.expect("caller commit");
        assert_eq!(scenario.marker().await, baseline + 1);
        assert_eq!(
            scenario
                .provider
                .profile_get(CLIENT, &profile_get_request(&scenario.space_id, &[]), now())
                .await
                .expect("get")
                .profiles
                .len(),
            1
        );

        // A refused CAS is not an observable effect, so the marker stays put.
        let after_commit = scenario.marker().await;
        let stale = scenario
            .provider
            .profile_update(
                CLIENT,
                &profile_update_request(
                    &scenario.space_id,
                    "personality",
                    "loser",
                    "markdown",
                    Some(2),
                ),
                now(),
            )
            .await;
        assert_eq!(reject_code(stale), SpokeRejectCode::RevisionConflict);
        assert_eq!(scenario.marker().await, after_commit);

        // §8.2: a frozen space still admits the read-only ops and refuses the
        // write, leaving no effect behind.
        let freeze: MemorySpacesFreezeRequest = serde_json::from_value(json!({
            "space_id": scenario.space_id,
            "expected_revision": 1,
            "idempotency_key": "freeze-space",
        }))
        .expect("valid spaces.freeze request");
        scenario
            .provider
            .spaces_freeze(OPERATOR, &freeze, now())
            .await
            .expect("space frozen");
        let frozen_marker = scenario.marker().await;
        let page = scenario
            .provider
            .recall(
                CLIENT,
                &recall_request(
                    &scenario.space_id,
                    subject_all(),
                    budget(10, None),
                    None,
                    None,
                    None,
                ),
                now(),
            )
            .await
            .expect("recall is allowlisted while frozen");
        assert_eq!(recalled_ids(&page), vec!["rec_shared"]);
        assert_eq!(
            scenario
                .provider
                .profile_get(CLIENT, &profile_get_request(&scenario.space_id, &[]), now())
                .await
                .expect("profile get is allowlisted while frozen")
                .profiles
                .len(),
            1
        );
        let refused = scenario
            .provider
            .profile_update(
                CLIENT,
                &profile_update_request(
                    &scenario.space_id,
                    "personality",
                    "written while frozen",
                    "markdown",
                    Some(1),
                ),
                now(),
            )
            .await;
        assert_eq!(reject_code(refused), SpokeRejectCode::SpaceFrozen);
        assert_eq!(scenario.marker().await, frozen_marker);
        assert_eq!(
            scenario.raw_profile("personality").await,
            Some(
                serde_json::to_string(&json!({
                    "format": "markdown",
                    "body": "committed by the caller",
                }))
                .expect("document json")
            )
        );
    }
}
