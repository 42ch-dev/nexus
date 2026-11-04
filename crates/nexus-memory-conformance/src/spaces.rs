//! Spaces, subjects, grants and the shared authorization seam
//! (spoke-memory §§6.2–6.5, 7.2–7.10, 8.2, 8.3).
//!
//! Every operation takes the authenticated principal id from the caller's
//! admission layer; payload principal fields only select grant targets
//! (MEM-R003). Each mutating op is split into an outer `*` convenience method
//! that opens one `BEGIN IMMEDIATE` transaction and a `*_in_tx` sibling that
//! stages the same authorized effect on a caller-owned connection without
//! committing, so a receipt/idempotency writer can share one commit with it
//! (MEM-R017/R022/R024). Every op authorizes at the single admission instant
//! the caller supplies; no operation samples the wall clock.
//!
//! Receipt bodies and idempotency dedup are deliberately absent here: the
//! receipt slice owns them, and these methods return only the content-free
//! receipt result each op reports (MEM-R022). A replay must therefore be
//! resolved before this layer is reached.

use std::num::NonZeroU64;

use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{json, Map, Value};
use sqlx::{Row, SqliteConnection};
use uuid::Uuid;

use spoke_operations::spoke_schemas::{
    MemoryGrant, MemoryGrantsGrantRequest, MemoryGrantsListRequest, MemoryGrantsRevokeRequest,
    MemorySpace, MemorySpacesFreezeRequest, MemorySpacesListRequest, MemorySpacesUnfreezeRequest,
    MemorySubjectsEnsureRequest, MemorySubjectsFenceRequest, MemorySubjectsGetRequest, Subject,
};
use spoke_operations::{
    grant_covers, is_freeze_allowlisted, jcs_sha256_hex, validate_grant_issuance,
    validate_memory_grants_grant, validate_memory_grants_list, validate_memory_spaces_list,
    validate_memory_subjects_fence, GrantRequirement, IssuedGrant, SpokeReject, SpokeRejectCode,
    SpokeResult,
};

use crate::provider::FixtureMemoryProvider;
use crate::FixtureError;

/// Wire schema version carried by every fixture-served object (§6.1).
const SCHEMA_VERSION: u64 = 1;

/// Provider page bound (`memory.describe.limits.max_page_size`, §7.0.7); the
/// assembly task advertises this same value.
pub(crate) const MAX_PAGE_SIZE: NonZeroU64 = match NonZeroU64::new(100) {
    Some(size) => size,
    None => panic!("the fixture page bound is non-zero"),
};

/// Algorithm-policy identity bound into every cursor (§7.0.7/MEM-R026) and
/// advertised by `memory.describe` when the assembly slice lands.
pub(crate) const ENGINE_POLICY_VERSION: &str = "nexus-fixture-spoke-memory/0.15.1";

/// `memory.spaces.list` payload (§7.2).
#[derive(Debug)]
pub(crate) struct SpacePage {
    pub(crate) spaces: Vec<MemorySpace>,
    pub(crate) next_cursor: Option<String>,
}

/// Content-free result of `memory.spaces.freeze` / `memory.spaces.unfreeze`
/// (§7.3/§7.4).
#[derive(Debug)]
pub(crate) struct SpaceChange {
    pub(crate) status: String,
    pub(crate) pending_receipts: u64,
    pub(crate) revision: u64,
}

/// Content-free result of `memory.subjects.ensure` (§7.5).
#[derive(Debug)]
pub(crate) struct SubjectChange {
    pub(crate) subject_id: String,
    pub(crate) created: bool,
    pub(crate) status: String,
    pub(crate) revision: u64,
}

/// Content-free result of `memory.subjects.fence` (§7.7).
#[derive(Debug)]
pub(crate) struct SubjectFenceChange {
    pub(crate) subject_id: String,
    pub(crate) status: String,
    pub(crate) fenced_contexts: Vec<String>,
    pub(crate) revision: u64,
}

/// `memory.grants.list` payload (§7.8).
#[derive(Debug)]
pub(crate) struct GrantPage {
    pub(crate) grants: Vec<MemoryGrant>,
    pub(crate) next_cursor: Option<String>,
}

/// Content-free result of `memory.grants.grant` (§7.9).
#[derive(Debug)]
pub(crate) struct GrantIssued {
    pub(crate) grant_id: String,
    pub(crate) revision: u64,
}

/// Content-free result of `memory.grants.revoke` (§7.10).
#[derive(Debug)]
pub(crate) struct GrantRevoked {
    pub(crate) grant_id: String,
    pub(crate) revoked: bool,
    pub(crate) revision: u64,
}

// ---------------------------------------------------------------------------
// Protocol errors
// ---------------------------------------------------------------------------

/// A storage or serialization failure: the caller's request was not at fault.
pub(crate) fn internal(error: impl std::fmt::Display) -> SpokeReject {
    SpokeReject {
        code: SpokeRejectCode::InternalError,
        message: format!("fixture storage failure: {error}"),
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

fn not_authorized(message: impl Into<String>) -> SpokeReject {
    SpokeReject {
        code: SpokeRejectCode::NotAuthorized,
        message: message.into(),
        details: None,
    }
}

fn invalid_input(field: &str, message: impl Into<String>) -> SpokeReject {
    let mut details = Map::new();
    details.insert("field".to_owned(), Value::String(field.to_owned()));
    SpokeReject {
        code: SpokeRejectCode::InvalidInput,
        message: message.into(),
        details: Some(details),
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

fn invalid_transition(message: impl Into<String>) -> SpokeReject {
    SpokeReject {
        code: SpokeRejectCode::InvalidStatusTransition,
        message: message.into(),
        details: None,
    }
}

fn cursor_invalid(message: impl Into<String>) -> SpokeReject {
    SpokeReject {
        code: SpokeRejectCode::CursorInvalid,
        message: message.into(),
        details: None,
    }
}

/// One indistinguishable refusal for a grant id the caller cannot act on: an
/// absent row and a row outside the caller's coverage must not be
/// distinguishable through code, details or message (§7.10/MEM-R026, no
/// existence oracle). The message deliberately names no grant id.
fn grant_not_revocable() -> SpokeReject {
    not_authorized("the grant is not revocable by the authenticated principal")
}

/// Flatten the published validator envelope into the crate's result shape.
fn checked(result: SpokeResult<()>) -> Result<(), SpokeReject> {
    match result {
        SpokeResult::Ok(()) => Ok(()),
        SpokeResult::Reject(reject) => Err(reject),
    }
}

// ---------------------------------------------------------------------------
// Shared authorization seam (§6.5)
// ---------------------------------------------------------------------------

/// §6.5 evaluation: one grant must admit the whole requirement; disjoint
/// grants are never unioned. Delegates to the published pure helper so every
/// slice evaluates coverage identically.
pub(crate) fn is_authorized(
    grants: &[MemoryGrant],
    principal_id: &str,
    space_id: &str,
    now: DateTime<Utc>,
    requirement: &GrantRequirement<'_>,
) -> bool {
    grants
        .iter()
        .any(|grant| grant_covers(grant, principal_id, space_id, now, requirement))
}

/// Requirement for an op that names no placement context and reads no shared
/// visibility; `capability` and the subject scope are the only facets.
pub(crate) fn requirement<'a>(
    capability: &'a str,
    subject_id: Option<&'a str>,
) -> GrantRequirement<'a> {
    GrantRequirement {
        capability,
        subject_id,
        context_ids: Vec::new(),
        reads_shared: false,
        subject_all: false,
    }
}

/// Requirement for an op whose effect — or whose grant view — covers a whole
/// subject or, with no subject scope, the whole space: every placement context
/// and shared visibility. `subject_all` is the published evaluator's only
/// unrestricted-only facet (§6.5), so a context-limited grant never admits a
/// whole-target action or a full grant view (MEM-R017/R026).
pub(crate) fn whole_scope_requirement<'a>(
    capability: &'a str,
    subject_id: Option<&'a str>,
) -> GrantRequirement<'a> {
    GrantRequirement {
        capability,
        subject_id,
        context_ids: Vec::new(),
        reads_shared: false,
        subject_all: true,
    }
}

fn require_authorized(
    grants: &[MemoryGrant],
    principal_id: &str,
    space_id: &str,
    now: DateTime<Utc>,
    requirement: &GrantRequirement<'_>,
) -> Result<(), SpokeReject> {
    if is_authorized(grants, principal_id, space_id, now, requirement) {
        return Ok(());
    }
    Err(not_authorized(
        "the authenticated principal holds no grant covering this operation",
    ))
}

// ---------------------------------------------------------------------------
// Stored rows
// ---------------------------------------------------------------------------

struct StoredSpace {
    space_id: String,
    display_name: Option<String>,
    authority_host_id: String,
    authority_generation: u64,
    status: String,
    revision: u64,
    created_at: DateTime<Utc>,
    extensions: Value,
}

struct StoredSubject {
    space_id: String,
    subject_id: String,
    kind_ns: Option<String>,
    kind_id: Option<String>,
    status: String,
    fenced_contexts: Vec<String>,
    revision: u64,
    created_at: DateTime<Utc>,
    extensions: Value,
}

struct StoredGrant {
    grant_id: String,
    space_id: String,
    subject_id: Option<String>,
    principal_id: String,
    capabilities: Vec<String>,
    context_ids: Vec<String>,
    shared_access: Option<bool>,
    expires_at: Option<DateTime<Utc>>,
    revoked_at: Option<DateTime<Utc>>,
    revision: u64,
    created_at: DateTime<Utc>,
    extensions: Value,
}

fn timestamp(raw: &str) -> Result<DateTime<Utc>, SpokeReject> {
    DateTime::parse_from_rfc3339(raw)
        .map(|parsed| parsed.with_timezone(&Utc))
        .map_err(|error| internal(format!("stored timestamp {raw} is not RFC 3339: {error}")))
}

fn timestamp_text(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Micros, true)
}

fn json_field(raw: &str, field: &str) -> Result<Value, SpokeReject> {
    serde_json::from_str(raw)
        .map_err(|error| internal(format!("stored {field} is not valid JSON: {error}")))
}

fn string_list_field(raw: &str, field: &str) -> Result<Vec<String>, SpokeReject> {
    serde_json::from_str(raw)
        .map_err(|error| internal(format!("stored {field} is not a string array: {error}")))
}

fn optional_timestamp(raw: Option<String>) -> Result<Option<DateTime<Utc>>, SpokeReject> {
    raw.map(|value| timestamp(&value)).transpose()
}

fn row_u64(row: &sqlx::sqlite::SqliteRow, column: &str) -> Result<u64, SpokeReject> {
    let raw: i64 = row.try_get(column).map_err(internal)?;
    u64::try_from(raw).map_err(internal)
}

fn row_i64(value: u64) -> Result<i64, SpokeReject> {
    i64::try_from(value).map_err(internal)
}

// ---------------------------------------------------------------------------
// Loaders
// ---------------------------------------------------------------------------

async fn fetch_space(
    conn: &mut SqliteConnection,
    space_id: &str,
) -> Result<StoredSpace, SpokeReject> {
    let row = sqlx::query(
        "SELECT space_id, display_name, authority_host_id, authority_generation, status, \
         revision, created_at, extensions_json FROM spaces WHERE space_id = ?",
    )
    .bind(space_id)
    .fetch_optional(conn)
    .await
    .map_err(internal)?
    .ok_or_else(|| not_found(format!("space {space_id} is not served by this fixture")))?;
    Ok(StoredSpace {
        space_id: row.try_get("space_id").map_err(internal)?,
        display_name: row.try_get("display_name").map_err(internal)?,
        authority_host_id: row.try_get("authority_host_id").map_err(internal)?,
        authority_generation: row_u64(&row, "authority_generation")?,
        status: row.try_get("status").map_err(internal)?,
        revision: row_u64(&row, "revision")?,
        created_at: timestamp(&row.try_get::<String, _>("created_at").map_err(internal)?)?,
        extensions: json_field(
            &row.try_get::<String, _>("extensions_json").map_err(internal)?,
            "space extensions",
        )?,
    })
}

async fn fetch_subject(
    conn: &mut SqliteConnection,
    space_id: &str,
    subject_id: &str,
) -> Result<StoredSubject, SpokeReject> {
    let row = sqlx::query(
        "SELECT space_id, subject_id, kind_ns, kind_id, status, fenced_contexts_json, revision, \
         created_at, extensions_json FROM subjects WHERE space_id = ? AND subject_id = ?",
    )
    .bind(space_id)
    .bind(subject_id)
    .fetch_optional(conn)
    .await
    .map_err(internal)?
    .ok_or_else(|| not_found(format!("subject {subject_id} does not exist in this space")))?;
    Ok(StoredSubject {
        space_id: row.try_get("space_id").map_err(internal)?,
        subject_id: row.try_get("subject_id").map_err(internal)?,
        kind_ns: row.try_get("kind_ns").map_err(internal)?,
        kind_id: row.try_get("kind_id").map_err(internal)?,
        status: row.try_get("status").map_err(internal)?,
        fenced_contexts: string_list_field(
            &row.try_get::<String, _>("fenced_contexts_json")
                .map_err(internal)?,
            "subject fenced contexts",
        )?,
        revision: row_u64(&row, "revision")?,
        created_at: timestamp(&row.try_get::<String, _>("created_at").map_err(internal)?)?,
        extensions: json_field(
            &row.try_get::<String, _>("extensions_json").map_err(internal)?,
            "subject extensions",
        )?,
    })
}

async fn load_grant_rows(
    conn: &mut SqliteConnection,
    space_id: &str,
) -> Result<Vec<StoredGrant>, SpokeReject> {
    let rows = sqlx::query(
        "SELECT grant_id, space_id, subject_id, principal_id, capabilities_json, \
         context_ids_json, shared_access, expires_at, revoked_at, revision, created_at, \
         extensions_json FROM grants WHERE space_id = ? ORDER BY grant_id",
    )
    .bind(space_id)
    .fetch_all(conn)
    .await
    .map_err(internal)?;
    rows.into_iter()
        .map(|row| {
            let shared_access: Option<i64> = row.try_get("shared_access").map_err(internal)?;
            Ok(StoredGrant {
                grant_id: row.try_get("grant_id").map_err(internal)?,
                space_id: row.try_get("space_id").map_err(internal)?,
                subject_id: row.try_get("subject_id").map_err(internal)?,
                principal_id: row.try_get("principal_id").map_err(internal)?,
                capabilities: string_list_field(
                    &row.try_get::<String, _>("capabilities_json")
                        .map_err(internal)?,
                    "grant capabilities",
                )?,
                context_ids: string_list_field(
                    &row.try_get::<String, _>("context_ids_json")
                        .map_err(internal)?,
                    "grant contexts",
                )?,
                shared_access: shared_access.map(|flag| flag != 0),
                expires_at: optional_timestamp(row.try_get("expires_at").map_err(internal)?)?,
                revoked_at: optional_timestamp(row.try_get("revoked_at").map_err(internal)?)?,
                revision: row_u64(&row, "revision")?,
                created_at: timestamp(&row.try_get::<String, _>("created_at").map_err(internal)?)?,
                extensions: json_field(
                    &row.try_get::<String, _>("extensions_json").map_err(internal)?,
                    "grant extensions",
                )?,
            })
        })
        .collect()
}

/// Every grant of the space as protocol objects, for the published evaluators.
///
/// Crate-visible on purpose: the candidate/record/recall slices authorize
/// through this same loader, so the shared seam is the reviewed one.
pub(crate) async fn load_grants(
    conn: &mut SqliteConnection,
    space_id: &str,
) -> Result<Vec<MemoryGrant>, SpokeReject> {
    load_grant_rows(conn, space_id)
        .await?
        .iter()
        .map(grant_to_wire)
        .collect()
}

/// The provider-private snapshot marker this space's cursors are bound to
/// (§7.0.7/MEM-R026). It is advanced by every committed observable change, so
/// it invalidates cursors even when the paged rows themselves did not move.
pub(crate) async fn snapshot_marker(
    conn: &mut SqliteConnection,
    space_id: &str,
) -> Result<u64, SpokeReject> {
    let row = sqlx::query("SELECT marker FROM provider_snapshot WHERE space_id = ?")
        .bind(space_id)
        .fetch_optional(conn)
        .await
        .map_err(internal)?
        .ok_or_else(|| internal(format!("space {space_id} has no snapshot marker")))?;
    row_u64(&row, "marker")
}

/// Advance the snapshot marker inside the caller's transaction. Every mutator
/// that changes observable state calls this in the same transaction as its own
/// write, so a rollback discards the marker bump with the effect.
pub(crate) async fn bump_snapshot(
    conn: &mut SqliteConnection,
    space_id: &str,
) -> Result<(), SpokeReject> {
    let updated = sqlx::query("UPDATE provider_snapshot SET marker = marker + 1 WHERE space_id = ?")
        .bind(space_id)
        .execute(conn)
        .await
        .map_err(internal)?;
    if updated.rows_affected() != 1 {
        return Err(internal(format!(
            "space {space_id} has no snapshot marker to advance"
        )));
    }
    Ok(())
}

async fn pending_receipts(
    conn: &mut SqliteConnection,
    space_id: &str,
) -> Result<u64, SpokeReject> {
    let row = sqlx::query(
        "SELECT COUNT(*) AS pending FROM receipts WHERE space_id = ? AND status = 'pending'",
    )
    .bind(space_id)
    .fetch_one(conn)
    .await
    .map_err(internal)?;
    let pending: i64 = row.try_get("pending").map_err(internal)?;
    u64::try_from(pending).map_err(internal)
}

// ---------------------------------------------------------------------------
// Stored -> protocol mapping
// ---------------------------------------------------------------------------

fn space_to_wire(space: &StoredSpace, pending: u64) -> Result<MemorySpace, SpokeReject> {
    serde_json::from_value(json!({
        "schema_version": SCHEMA_VERSION,
        "space_id": space.space_id,
        "display_name": space.display_name,
        "authority_host_id": space.authority_host_id,
        "authority_generation": space.authority_generation,
        "status": space.status,
        "pending_receipts": pending,
        "created_at": timestamp_text(space.created_at),
        "revision": space.revision,
        "extensions": space.extensions,
    }))
    .map_err(|error| internal(format!("stored space row is not protocol-shaped: {error}")))
}

fn subject_to_wire(subject: &StoredSubject) -> Result<Subject, SpokeReject> {
    let kind = match (&subject.kind_ns, &subject.kind_id) {
        (Some(ns), Some(id)) => json!({ "ns": ns, "id": id }),
        _ => Value::Null,
    };
    serde_json::from_value(json!({
        "schema_version": SCHEMA_VERSION,
        "space_id": subject.space_id,
        "subject_id": subject.subject_id,
        "kind": kind,
        "status": subject.status,
        "fenced_contexts": subject.fenced_contexts,
        "created_at": timestamp_text(subject.created_at),
        "revision": subject.revision,
        "extensions": subject.extensions,
    }))
    .map_err(|error| internal(format!("stored subject row is not protocol-shaped: {error}")))
}

fn grant_to_wire(grant: &StoredGrant) -> Result<MemoryGrant, SpokeReject> {
    let mut object = json!({
        "schema_version": SCHEMA_VERSION,
        "grant_id": grant.grant_id,
        "space_id": grant.space_id,
        "subject_id": grant.subject_id,
        "principal_id": grant.principal_id,
        "capabilities": grant.capabilities,
        "created_at": timestamp_text(grant.created_at),
        "revision": grant.revision,
        "extensions": grant.extensions,
    });
    {
        let members = object
            .as_object_mut()
            .ok_or_else(|| internal("grant object construction failed"))?;
        if let Some(expires_at) = grant.expires_at {
            members.insert("expires_at".to_owned(), json!(timestamp_text(expires_at)));
        }
        if let Some(revoked_at) = grant.revoked_at {
            members.insert("revoked_at".to_owned(), json!(timestamp_text(revoked_at)));
        }
        if !grant.context_ids.is_empty() {
            members.insert("context_ids".to_owned(), json!(grant.context_ids));
            if let Some(shared_access) = grant.shared_access {
                members.insert("shared_access".to_owned(), json!(shared_access));
            }
        }
    }
    serde_json::from_value(object)
        .map_err(|error| internal(format!("stored grant row is not protocol-shaped: {error}")))
}

// ---------------------------------------------------------------------------
// Admission helpers (§8.2, §8.3, MEM-R007)
// ---------------------------------------------------------------------------

/// §8.2: a frozen space admits exactly the maintenance allowlist; the decision
/// uses the published policy predicate so the fixture cannot drift from it.
fn require_admitted(op: &str, space: &StoredSpace) -> Result<(), SpokeReject> {
    if space.status != "frozen" || is_freeze_allowlisted(op) {
        return Ok(());
    }
    Err(SpokeReject {
        code: SpokeRejectCode::SpaceFrozen,
        message: format!("space {} is frozen; {op} is outside the §8.2 allowlist", space.space_id),
        details: None,
    })
}

fn require_revision(expected: NonZeroU64, actual: u64) -> Result<(), SpokeReject> {
    if expected.get() == actual {
        return Ok(());
    }
    Err(revision_conflict(expected, actual))
}

impl FixtureMemoryProvider {
    pub(crate) fn require_served_space(&self, requested: &str) -> Result<(), SpokeReject> {
        if requested == self.identity.space_id {
            return Ok(());
        }
        Err(not_found(format!(
            "space {requested} is not served by this fixture"
        )))
    }

    // -----------------------------------------------------------------------
    // memory.spaces.list (§7.2)
    // -----------------------------------------------------------------------

    /// List the spaces visible to `principal_id`, with the computed drain
    /// barrier. Visibility is "any valid grant in the space" (§7.2); the
    /// operator's provisioned admin grant makes the space visible to operators.
    ///
    /// `now` is the single admission instant every authorization decision in
    /// this slice uses (MEM-R006), so the caller — later the fault slice's
    /// controllable clock — decides when the request is admitted.
    pub(crate) async fn spaces_list(
        &self,
        principal_id: &str,
        request: &MemorySpacesListRequest,
        now: DateTime<Utc>,
    ) -> Result<SpacePage, SpokeReject> {
        checked(validate_memory_spaces_list(request, MAX_PAGE_SIZE))?;
        if request.cursor.is_some() {
            // One space never needs a second page, so no cursor is ever
            // issued for this op: a presented one cannot be valid (§7.0.7).
            return Err(cursor_invalid(
                "memory.spaces.list serves one space and issues no cursor",
            ));
        }
        // One read transaction: the space row, the caller's grants, the
        // visibility decision and the pending count all come from the same
        // snapshot, so a concurrent revoke or status change cannot be combined
        // with state it was never authorized against (MEM-R026).
        let mut transaction = self.store().pool().begin().await.map_err(internal)?;
        let space = fetch_space(&mut transaction, &self.identity.space_id).await?;
        let visible = load_grant_rows(&mut transaction, &space.space_id)
            .await?
            .iter()
            .any(|grant| grant_is_valid(grant, principal_id, &space.space_id, now));
        let spaces = if visible {
            let pending = pending_receipts(&mut transaction, &space.space_id).await?;
            vec![space_to_wire(&space, pending)?]
        } else {
            Vec::new()
        };
        transaction.commit().await.map_err(internal)?;
        Ok(SpacePage {
            spaces,
            next_cursor: None,
        })
    }

    // -----------------------------------------------------------------------
    // memory.spaces.freeze / memory.spaces.unfreeze (§7.3/§7.4)
    // -----------------------------------------------------------------------

    /// Freeze the space for maintenance. A frozen space with a matching
    /// revision is a no-op success (§7.0.4), which is why this op is not
    /// itself gated by the freeze allowlist.
    pub(crate) async fn spaces_freeze(
        &self,
        principal_id: &str,
        request: &MemorySpacesFreezeRequest,
        now: DateTime<Utc>,
    ) -> Result<SpaceChange, SpokeReject> {
        let mut transaction = self
            .store()
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(internal)?;
        let change = self
            .spaces_freeze_in_tx(&mut transaction, principal_id, request, now)
            .await?;
        transaction.commit().await.map_err(internal)?;
        Ok(change)
    }

    /// [`Self::spaces_freeze`] on a caller-owned connection: the transition is
    /// staged on `conn` and never committed, so a receipt/dedup writer can
    /// commit this effect together with its own receipt state (MEM-R022).
    pub(crate) async fn spaces_freeze_in_tx(
        &self,
        conn: &mut SqliteConnection,
        principal_id: &str,
        request: &MemorySpacesFreezeRequest,
        now: DateTime<Utc>,
    ) -> Result<SpaceChange, SpokeReject> {
        self.set_space_status_in_tx(
            conn,
            principal_id,
            &request.space_id,
            request.expected_revision,
            "frozen",
            now,
        )
        .await
    }

    /// Reverse of [`Self::spaces_freeze`]; always admissible on a frozen space
    /// (maintenance allowlist) and a no-op success on an active one.
    pub(crate) async fn spaces_unfreeze(
        &self,
        principal_id: &str,
        request: &MemorySpacesUnfreezeRequest,
        now: DateTime<Utc>,
    ) -> Result<SpaceChange, SpokeReject> {
        let mut transaction = self
            .store()
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(internal)?;
        let change = self
            .spaces_unfreeze_in_tx(&mut transaction, principal_id, request, now)
            .await?;
        transaction.commit().await.map_err(internal)?;
        Ok(change)
    }

    /// [`Self::spaces_unfreeze`] on a caller-owned connection; never commits.
    pub(crate) async fn spaces_unfreeze_in_tx(
        &self,
        conn: &mut SqliteConnection,
        principal_id: &str,
        request: &MemorySpacesUnfreezeRequest,
        now: DateTime<Utc>,
    ) -> Result<SpaceChange, SpokeReject> {
        self.set_space_status_in_tx(
            conn,
            principal_id,
            &request.space_id,
            request.expected_revision,
            "active",
            now,
        )
        .await
    }

    async fn set_space_status_in_tx(
        &self,
        conn: &mut SqliteConnection,
        principal_id: &str,
        space_id: &str,
        expected_revision: NonZeroU64,
        target_status: &str,
        now: DateTime<Utc>,
    ) -> Result<SpaceChange, SpokeReject> {
        self.require_served_space(space_id)?;
        let space = fetch_space(conn, space_id).await?;
        let grants = load_grants(conn, space_id).await?;
        require_authorized(
            &grants,
            principal_id,
            space_id,
            now,
            // A freeze/unfreeze gate covers every placement context of the
            // space, so a context-limited admin never controls it (§7.3/§7.4).
            &whole_scope_requirement("admin", None),
        )?;
        require_revision(expected_revision, space.revision)?;
        let revision = if space.status == target_status {
            space.revision
        } else {
            sqlx::query(
                "UPDATE spaces SET status = ?, revision = revision + 1 \
                 WHERE space_id = ? AND revision = ?",
            )
            .bind(target_status)
            .bind(space_id)
            .bind(row_i64(space.revision)?)
            .execute(&mut *conn)
            .await
            .map_err(internal)?;
            bump_snapshot(conn, space_id).await?;
            space.revision + 1
        };
        let pending = pending_receipts(conn, space_id).await?;
        Ok(SpaceChange {
            status: target_status.to_owned(),
            pending_receipts: pending,
            revision,
        })
    }

    // -----------------------------------------------------------------------
    // memory.subjects.ensure / get / fence (§7.5–§7.7)
    // -----------------------------------------------------------------------

    /// Get-or-create the subject. `kind` is metadata and is written only when
    /// the row is created; an existing row is returned unchanged.
    pub(crate) async fn subjects_ensure(
        &self,
        principal_id: &str,
        request: &MemorySubjectsEnsureRequest,
        now: DateTime<Utc>,
    ) -> Result<SubjectChange, SpokeReject> {
        let mut transaction = self
            .store()
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(internal)?;
        let change = self
            .subjects_ensure_in_tx(&mut transaction, principal_id, request, now)
            .await?;
        transaction.commit().await.map_err(internal)?;
        Ok(change)
    }

    /// [`Self::subjects_ensure`] on a caller-owned connection; never commits.
    pub(crate) async fn subjects_ensure_in_tx(
        &self,
        conn: &mut SqliteConnection,
        principal_id: &str,
        request: &MemorySubjectsEnsureRequest,
        now: DateTime<Utc>,
    ) -> Result<SubjectChange, SpokeReject> {
        self.require_served_space(&request.space_id)?;
        let space = fetch_space(conn, &request.space_id).await?;
        require_admitted("memory.subjects.ensure", &space)?;
        let grants = load_grants(conn, &request.space_id).await?;
        require_authorized(
            &grants,
            principal_id,
            &request.space_id,
            now,
            &requirement("capture", Some(request.subject_id.as_str())),
        )?;
        let (kind_ns, kind_id) = request.kind.as_ref().map_or((None, None), |kind| {
            (Some(kind.ns.as_str()), Some(kind.id.as_str()))
        });
        let inserted = sqlx::query(
            "INSERT INTO subjects (space_id, subject_id, kind_ns, kind_id, status, \
             fenced_contexts_json, revision, created_at) \
             VALUES (?, ?, ?, ?, 'active', '[]', 1, ?) \
             ON CONFLICT (space_id, subject_id) DO NOTHING",
        )
        .bind(request.space_id.as_str())
        .bind(request.subject_id.as_str())
        .bind(kind_ns)
        .bind(kind_id)
        .bind(timestamp_text(now))
        .execute(&mut *conn)
        .await
        .map_err(|error| match error.as_database_error() {
            Some(database) if database.is_unique_violation() => invalid_input(
                "kind",
                "a subject with this kind already exists in the space",
            ),
            _ => internal(error),
        })?;
        let created = inserted.rows_affected() == 1;
        let subject = fetch_subject(conn, &request.space_id, &request.subject_id).await?;
        // §7.5 lists SUBJECT_FENCED for ensure: a fenced or archived subject
        // refuses the write path that ensure fronts.
        if subject.status != "active" {
            return Err(subject_fenced(&subject.subject_id, "subject"));
        }
        if created {
            bump_snapshot(conn, &request.space_id).await?;
        }
        Ok(SubjectChange {
            subject_id: subject.subject_id,
            created,
            status: subject.status,
            revision: subject.revision,
        })
    }

    /// Read one subject under current authorization (MEM-R026).
    pub(crate) async fn subjects_get(
        &self,
        principal_id: &str,
        request: &MemorySubjectsGetRequest,
        now: DateTime<Utc>,
    ) -> Result<Subject, SpokeReject> {
        self.require_served_space(&request.space_id)?;
        // One read transaction: the space row, the caller's grants, the
        // authorization decision and the returned subject are one snapshot, so
        // a concurrent revoke cannot pair an old grant with changed state
        // (MEM-R026).
        let mut transaction = self.store().pool().begin().await.map_err(internal)?;
        let space = fetch_space(&mut transaction, &request.space_id).await?;
        require_admitted("memory.subjects.get", &space)?;
        let grants = load_grant_rows(&mut transaction, &request.space_id).await?;
        if !any_grant_covers_subject(
            &grants,
            principal_id,
            &request.space_id,
            request.subject_id.as_str(),
            now,
        ) {
            return Err(not_authorized(
                "the authenticated principal holds no grant on this subject",
            ));
        }
        let subject = fetch_subject(&mut transaction, &request.space_id, &request.subject_id).await?;
        let subject = subject_to_wire(&subject)?;
        transaction.commit().await.map_err(internal)?;
        Ok(subject)
    }

    /// Provider-enforced subject or placement-context fencing (§7.7).
    pub(crate) async fn subjects_fence(
        &self,
        principal_id: &str,
        request: &MemorySubjectsFenceRequest,
        now: DateTime<Utc>,
    ) -> Result<SubjectFenceChange, SpokeReject> {
        let mut transaction = self
            .store()
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(internal)?;
        let change = self
            .subjects_fence_in_tx(&mut transaction, principal_id, request, now)
            .await?;
        transaction.commit().await.map_err(internal)?;
        Ok(change)
    }

    /// [`Self::subjects_fence`] on a caller-owned connection; never commits.
    pub(crate) async fn subjects_fence_in_tx(
        &self,
        conn: &mut SqliteConnection,
        principal_id: &str,
        request: &MemorySubjectsFenceRequest,
        now: DateTime<Utc>,
    ) -> Result<SubjectFenceChange, SpokeReject> {
        use spoke_operations::spoke_schemas::memory_subjects_fence_request::{Action, Target};

        checked(validate_memory_subjects_fence(request))?;
        self.require_served_space(&request.space_id)?;
        let space = fetch_space(conn, &request.space_id).await?;
        require_admitted("memory.subjects.fence", &space)?;
        let grants = load_grants(conn, &request.space_id).await?;
        // Fence authority covers the real target: a subject target rewrites
        // every placement context of the subject, so it needs unrestricted
        // coverage of that subject; a context target needs that one placement
        // context (§6.5/§7.7).
        let required = match &request.target {
            Target::Subject => {
                whole_scope_requirement("admin", Some(request.subject_id.as_str()))
            }
            Target::Context(context_id) => GrantRequirement {
                capability: "admin",
                subject_id: Some(request.subject_id.as_str()),
                context_ids: vec![context_id.as_str().to_owned()],
                reads_shared: false,
                subject_all: false,
            },
        };
        require_authorized(&grants, principal_id, &request.space_id, now, &required)?;
        let subject = fetch_subject(conn, &request.space_id, &request.subject_id).await?;
        require_revision(request.expected_revision, subject.revision)?;

        let (status, mut fenced_contexts) = (subject.status.clone(), subject.fenced_contexts.clone());
        let next_status = match (&request.target, &request.action) {
            (Target::Subject, Action::Fence) => match status.as_str() {
                "active" => Some("fenced"),
                _ => None,
            },
            (Target::Subject, Action::Unfence) => match status.as_str() {
                "fenced" => Some("active"),
                _ => None,
            },
            (Target::Subject, Action::Archive) => match status.as_str() {
                "active" | "fenced" => Some("archived"),
                _ => None,
            },
            (Target::Context(context_id), Action::Fence) => {
                if status == "archived" || fenced_contexts.iter().any(|id| id == context_id.as_str())
                {
                    None
                } else {
                    fenced_contexts.push(context_id.as_str().to_owned());
                    // A context transition never rewrites the subject status:
                    // a subject-level fence would otherwise be cleared.
                    Some(status.as_str())
                }
            }
            (Target::Context(context_id), Action::Unfence) => {
                match fenced_contexts
                    .iter()
                    .position(|id| id == context_id.as_str())
                {
                    Some(position) if status != "archived" => {
                        fenced_contexts.remove(position);
                        Some(status.as_str())
                    }
                    _ => None,
                }
            }
            // The published validator rejects archive-on-context as
            // INVALID_INPUT before this table runs; the arm only keeps the
            // transition table exhaustive.
            (Target::Context(_), Action::Archive) => None,
        };
        let Some(next_status) = next_status else {
            return Err(invalid_transition(format!(
                "subject {} cannot apply {} to {}",
                subject.subject_id,
                action_name(&request.action),
                target_name(&request.target)
            )));
        };

        sqlx::query(
            "UPDATE subjects SET status = ?, fenced_contexts_json = ?, revision = revision + 1 \
             WHERE space_id = ? AND subject_id = ? AND revision = ?",
        )
        .bind(next_status)
        .bind(serde_json::to_string(&fenced_contexts).map_err(internal)?)
        .bind(request.space_id.as_str())
        .bind(request.subject_id.as_str())
        .bind(row_i64(subject.revision)?)
        .execute(&mut *conn)
        .await
        .map_err(internal)?;
        bump_snapshot(conn, &request.space_id).await?;
        Ok(SubjectFenceChange {
            subject_id: subject.subject_id,
            status: next_status.to_owned(),
            fenced_contexts,
            revision: subject.revision + 1,
        })
    }

    // -----------------------------------------------------------------------
    // memory.grants.list / grant / revoke (§7.8–§7.10)
    // -----------------------------------------------------------------------

    /// Admin sees every grant in the space (optionally for one principal); any
    /// other principal sees exactly its own grants through the
    /// selector-omitted path (§6.4/§7.8).
    ///
    /// The whole read — space row, grants, the authorization decision, the page
    /// rows and the cursor's snapshot marker — runs inside one read
    /// transaction, so a change committed by another connection between them
    /// cannot pair old rows with a new marker (§7.0.7/MEM-R026).
    pub(crate) async fn grants_list(
        &self,
        principal_id: &str,
        request: &MemoryGrantsListRequest,
        now: DateTime<Utc>,
    ) -> Result<GrantPage, SpokeReject> {
        let mut transaction = self.store().pool().begin().await.map_err(internal)?;
        let page = self
            .grants_list_in_tx(&mut transaction, principal_id, request, now)
            .await?;
        transaction.commit().await.map_err(internal)?;
        Ok(page)
    }

    /// [`Self::grants_list`] on a caller-owned connection: the whole read uses
    /// the caller's snapshot and never begins or commits a transaction. Later
    /// read slices that must pair authorization with data use this same seam.
    pub(crate) async fn grants_list_in_tx(
        &self,
        conn: &mut SqliteConnection,
        principal_id: &str,
        request: &MemoryGrantsListRequest,
        now: DateTime<Utc>,
    ) -> Result<GrantPage, SpokeReject> {
        checked(validate_memory_grants_list(request, MAX_PAGE_SIZE))?;
        self.require_served_space(&request.space_id)?;
        let space = fetch_space(conn, &request.space_id).await?;
        require_admitted("memory.grants.list", &space)?;
        let rows = load_grant_rows(conn, &request.space_id).await?;
        let grants = rows
            .iter()
            .map(grant_to_wire)
            .collect::<Result<Vec<_>, _>>()?;
        // A full view covers the requested subject (or the whole space) across
        // every placement context, so a context-limited admin never gets one.
        let admin = is_authorized(
            &grants,
            principal_id,
            &request.space_id,
            now,
            &whole_scope_requirement(
                "admin",
                request.subject_id.as_deref().map(|id| id.as_str()),
            ),
        );
        // §6.4: the optional principal selector is an admin-only target
        // selector, so a non-admin may not name any principal — not even
        // itself. Self view is the selector-omitted path.
        if !admin && request.principal_id.is_some() {
            return Err(not_authorized(
                "only an admin grant may name a principal explicitly",
            ));
        }
        let target_principal = request.principal_id.as_ref().map(|id| id.as_str());
        let scoped_principal = if admin {
            target_principal
        } else {
            Some(principal_id)
        };
        let subject_filter = request.subject_id.as_deref();

        let filtered: Vec<StoredGrant> = rows
            .into_iter()
            .filter(|grant| {
                scoped_principal.is_none_or(|target| grant.principal_id == target)
                    && subject_filter.is_none_or(|filter| grant.subject_id.as_deref() == Some(filter))
            })
            .collect();

        // §7.0.7/MEM-R026: bind the cursor to the provider snapshot marker and
        // engine policy as well as the selection, so any grant or lifecycle
        // change in the space invalidates it even when the page's rows did not
        // move. The marker is read on the same connection, inside the same
        // snapshot as the authorization and the rows above.
        let snapshot = snapshot_marker(conn, &request.space_id).await?;
        let binding_digest = match jcs_sha256_hex(&json!({
            "principal": principal_id,
            "space": request.space_id.as_str(),
            "target": scoped_principal,
            "subject": subject_filter,
            "snapshot": snapshot,
            "policy": ENGINE_POLICY_VERSION,
        })) {
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
                let position = filtered
                    .iter()
                    .position(|grant| grant.grant_id == last_id)
                    .ok_or_else(|| cursor_invalid("cursor no longer matches the grant set"))?;
                position + 1
            }
        };

        let limit = usize::try_from(
            request
                .limit
                .map_or(MAX_PAGE_SIZE.get(), NonZeroU64::get),
        )
        .map_err(internal)?;
        let remaining = &filtered[start..];
        let page = &remaining[..remaining.len().min(limit)];
        let next_cursor = if remaining.len() > limit {
            page.last()
                .map(|grant| format!("{binding_digest}:{}", grant.grant_id))
        } else {
            None
        };
        let grants = page
            .iter()
            .map(grant_to_wire)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(GrantPage {
            grants,
            next_cursor,
        })
    }

    /// Issue a grant as an admin whose own coverage subsumes it (§7.9).
    pub(crate) async fn grants_grant(
        &self,
        principal_id: &str,
        request: &MemoryGrantsGrantRequest,
        now: DateTime<Utc>,
    ) -> Result<GrantIssued, SpokeReject> {
        let mut transaction = self
            .store()
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(internal)?;
        let issued = self
            .grants_grant_in_tx(&mut transaction, principal_id, request, now)
            .await?;
        transaction.commit().await.map_err(internal)?;
        Ok(issued)
    }

    /// [`Self::grants_grant`] on a caller-owned connection; never commits.
    pub(crate) async fn grants_grant_in_tx(
        &self,
        conn: &mut SqliteConnection,
        principal_id: &str,
        request: &MemoryGrantsGrantRequest,
        now: DateTime<Utc>,
    ) -> Result<GrantIssued, SpokeReject> {
        use spoke_operations::spoke_schemas::MemoryGrantsGrantRequest::{
            Variant0 as Restricted, Variant1 as Unrestricted,
        };

        checked(validate_memory_grants_grant(request))?;
        let (space_id, grantee, capabilities, context_ids, shared_access, expires_at, subject_id) =
            match request {
                Restricted {
                    capabilities,
                    context_ids,
                    expires_at,
                    shared_access,
                    principal_id,
                    space_id,
                    subject_id,
                    ..
                } => (
                    space_id.as_str().to_owned(),
                    principal_id.as_str().to_owned(),
                    capabilities.clone(),
                    context_ids.clone(),
                    shared_access.unwrap_or(false),
                    *expires_at,
                    subject_id.as_ref().map(|id| id.as_str().to_owned()),
                ),
                Unrestricted {
                    capabilities,
                    expires_at,
                    principal_id,
                    space_id,
                    subject_id,
                    ..
                } => (
                    space_id.as_str().to_owned(),
                    principal_id.as_str().to_owned(),
                    capabilities.clone(),
                    Vec::new(),
                    false,
                    expires_at.as_deref().copied(),
                    subject_id.as_ref().map(|id| id.as_str().to_owned()),
                ),
            };
        self.require_served_space(&space_id)?;
        let space = fetch_space(conn, &space_id).await?;
        require_admitted("memory.grants.grant", &space)?;
        let granter_grants = load_grants(conn, &space_id).await?;
        let capability_refs = capabilities
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        checked(validate_grant_issuance(
            &granter_grants,
            principal_id,
            &space_id,
            now,
            &IssuedGrant {
                capabilities: &capability_refs,
                subject_id: subject_id.as_deref(),
                context_ids: &context_ids,
                shared_access,
                expires_at,
            },
        ))?;
        if let Some(subject_id) = subject_id.as_deref() {
            let subject = fetch_subject(conn, &space_id, subject_id).await?;
            if subject.status != "active" {
                return Err(subject_fenced(subject_id, "subject"));
            }
            if context_ids
                .iter()
                .any(|context| subject.fenced_contexts.contains(context))
            {
                return Err(subject_fenced(subject_id, "context"));
            }
        }
        let grant_id = format!("grt_{}", Uuid::new_v4().simple());
        let capabilities_json = serde_json::to_string(&capabilities).map_err(internal)?;
        let context_ids_json = serde_json::to_string(&context_ids).map_err(internal)?;
        sqlx::query(
            "INSERT INTO grants (grant_id, space_id, subject_id, principal_id, \
             capabilities_json, context_ids_json, shared_access, expires_at, revoked_at, \
             revision, created_at, extensions_json) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, NULL, 1, ?, '{}')",
        )
        .bind(&grant_id)
        .bind(&space_id)
        .bind(subject_id.as_deref())
        .bind(&grantee)
        .bind(capabilities_json)
        .bind(context_ids_json)
        .bind(shared_access)
        .bind(expires_at.map(timestamp_text))
        .bind(timestamp_text(now))
        .execute(&mut *conn)
        .await
        .map_err(internal)?;
        bump_snapshot(conn, &space_id).await?;
        Ok(GrantIssued {
            grant_id,
            revision: 1,
        })
    }

    /// Terminal revoke; applies to every subsequent admission (§7.10/MEM-R018).
    pub(crate) async fn grants_revoke(
        &self,
        principal_id: &str,
        request: &MemoryGrantsRevokeRequest,
        now: DateTime<Utc>,
    ) -> Result<GrantRevoked, SpokeReject> {
        let mut transaction = self
            .store()
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(internal)?;
        let revoked = self
            .grants_revoke_in_tx(&mut transaction, principal_id, request, now)
            .await?;
        transaction.commit().await.map_err(internal)?;
        Ok(revoked)
    }

    /// [`Self::grants_revoke`] on a caller-owned connection; never commits.
    pub(crate) async fn grants_revoke_in_tx(
        &self,
        conn: &mut SqliteConnection,
        principal_id: &str,
        request: &MemoryGrantsRevokeRequest,
        now: DateTime<Utc>,
    ) -> Result<GrantRevoked, SpokeReject> {
        self.require_served_space(&request.space_id)?;
        let space = fetch_space(conn, &request.space_id).await?;
        require_admitted("memory.grants.revoke", &space)?;
        let grants = load_grant_rows(conn, &request.space_id).await?;
        // First stage: any admin authority at all, so a non-admin cannot probe
        // grant ids for existence.
        if !grants.iter().any(|grant| {
            grant_is_valid(grant, principal_id, &request.space_id, now)
                && grant
                    .capabilities
                    .iter()
                    .any(|capability| capability == "admin")
        }) {
            return Err(not_authorized(
                "revocation requires an admin grant in this space",
            ));
        }
        let grants_wire = grants
            .iter()
            .map(grant_to_wire)
            .collect::<Result<Vec<_>, _>>()?;
        // A caller holding full space-wide unrestricted admin already sees every
        // grant through the full `grants.list` view, so an absent id tells it
        // nothing it could not list. Every narrower admin must not be able to
        // tell an absent id from a grant outside its coverage, so both answer
        // the one refusal below.
        let sees_every_grant = is_authorized(
            &grants_wire,
            principal_id,
            &request.space_id,
            now,
            &whole_scope_requirement("admin", None),
        );
        let Some(target) = grants
            .iter()
            .find(|grant| grant.grant_id == request.grant_id.as_str())
        else {
            return Err(if sees_every_grant {
                not_found(format!(
                    "grant {} does not exist",
                    request.grant_id.as_str()
                ))
            } else {
                grant_not_revocable()
            });
        };
        // Second stage: revocation is grant management inside the admin's own
        // coverage, so the admin grant must cover the target's subject, its
        // placement contexts, its shared-visibility facet and — when the target
        // is unrestricted — be unrestricted itself. `subject_all` is the
        // published evaluator's only unrestricted-only facet.
        let required = GrantRequirement {
            capability: "admin",
            subject_id: target.subject_id.as_deref(),
            context_ids: target.context_ids.clone(),
            reads_shared: target.shared_access.unwrap_or(false),
            subject_all: target.context_ids.is_empty(),
        };
        if !is_authorized(&grants_wire, principal_id, &request.space_id, now, &required) {
            return Err(grant_not_revocable());
        }
        require_revision(request.expected_revision, target.revision)?;
        if target.revoked_at.is_some() {
            // §7.0.4: revoking an already revoked grant is a no-op success.
            return Ok(GrantRevoked {
                grant_id: target.grant_id.clone(),
                revoked: true,
                revision: target.revision,
            });
        }
        sqlx::query(
            "UPDATE grants SET revoked_at = ?, revision = revision + 1 \
             WHERE grant_id = ? AND revision = ?",
        )
        .bind(timestamp_text(now))
        .bind(request.grant_id.as_str())
        .bind(row_i64(target.revision)?)
        .execute(&mut *conn)
        .await
        .map_err(internal)?;
        bump_snapshot(conn, &request.space_id).await?;
        Ok(GrantRevoked {
            grant_id: target.grant_id.clone(),
            revoked: true,
            revision: target.revision + 1,
        })
    }

    // -----------------------------------------------------------------------
    // Operator provisioning (§7.0.6)
    // -----------------------------------------------------------------------

    /// Provision the space's initial administrator grant directly in provider
    /// storage. This is the operator bootstrap path, not a protocol op: it is
    /// callable before any admin exists and stays outside `grants.grant`'s
    /// non-amplification rule (§7.0.6). Repeating it returns the same grant.
    ///
    /// # Errors
    ///
    /// Returns [`FixtureError`] when the bootstrap transaction fails or the
    /// stored grant rows cannot be read.
    pub(crate) async fn bootstrap_operator_admin(
        &self,
        principal_id: &str,
    ) -> Result<String, FixtureError> {
        let mut transaction = self
            .store()
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await?;
        let rows = sqlx::query(
            "SELECT grant_id, capabilities_json FROM grants \
             WHERE space_id = ? AND principal_id = ? AND subject_id IS NULL \
             ORDER BY grant_id",
        )
        .bind(&self.identity.space_id)
        .bind(principal_id)
        .fetch_all(&mut *transaction)
        .await?;
        for row in &rows {
            let capabilities: Vec<String> =
                serde_json::from_str(&row.try_get::<String, _>("capabilities_json")?)
                    .map_err(|error| FixtureError::Bootstrap(format!("grant capabilities: {error}")))?;
            if capabilities.iter().any(|capability| capability == "admin") {
                let grant_id: String = row.try_get("grant_id")?;
                transaction.commit().await?;
                return Ok(grant_id);
            }
        }
        let grant_id = format!("grt_{}", Uuid::new_v4().simple());
        sqlx::query(
            "INSERT INTO grants (grant_id, space_id, subject_id, principal_id, \
             capabilities_json, context_ids_json, shared_access, expires_at, revoked_at, \
             revision, created_at, extensions_json) \
             VALUES (?, ?, NULL, ?, '[\"admin\"]', '[]', NULL, NULL, NULL, 1, ?, '{}')",
        )
        .bind(&grant_id)
        .bind(&self.identity.space_id)
        .bind(principal_id)
        .bind(timestamp_text(Utc::now()))
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(grant_id)
    }
}

/// Any unrevoked, unexpired grant of the principal in the space covers the
/// subject (or is space-wide) — the §7.6 "any capability on the subject" test.
fn any_grant_covers_subject(
    grants: &[StoredGrant],
    principal_id: &str,
    space_id: &str,
    subject_id: &str,
    now: DateTime<Utc>,
) -> bool {
    grants.iter().any(|grant| {
        grant_is_valid(grant, principal_id, space_id, now)
            && grant
                .subject_id
                .as_deref()
                .is_none_or(|scope| scope == subject_id)
    })
}

fn grant_is_valid(
    grant: &StoredGrant,
    principal_id: &str,
    space_id: &str,
    now: DateTime<Utc>,
) -> bool {
    grant.principal_id == principal_id
        && grant.space_id == space_id
        && grant.revoked_at.is_none()
        && grant.expires_at.is_none_or(|expires_at| expires_at > now)
}

fn action_name(action: &spoke_operations::spoke_schemas::memory_subjects_fence_request::Action) -> &'static str {
    use spoke_operations::spoke_schemas::memory_subjects_fence_request::Action;
    match action {
        Action::Fence => "fence",
        Action::Unfence => "unfence",
        Action::Archive => "archive",
    }
}

fn target_name(target: &spoke_operations::spoke_schemas::memory_subjects_fence_request::Target) -> &'static str {
    use spoke_operations::spoke_schemas::memory_subjects_fence_request::Target;
    match target {
        Target::Subject => "the subject target",
        Target::Context(_) => "a context target",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::candidates::{
        admit_capture, stage_receipt_in_tx, Mutation, ReceiptState, ReceiptTarget,
    };
    use std::path::PathBuf;
    use std::sync::atomic::AtomicBool;
    use std::sync::{mpsc, Arc, Mutex};
    use std::time::Duration;
    use tempfile::TempDir;

    const OPERATOR: &str = "principal-operator";
    const CLIENT: &str = "principal-client-b";
    const OTHER: &str = "principal-other";

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
    }

    fn spaces_list_request(limit: Option<u64>, cursor: Option<&str>) -> MemorySpacesListRequest {
        let mut value = json!({});
        if let Some(limit) = limit {
            value["limit"] = json!(limit);
        }
        if let Some(cursor) = cursor {
            value["cursor"] = json!(cursor);
        }
        serde_json::from_value(value).expect("valid spaces.list request")
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

    fn ensure_request(space: &str, subject: &str) -> MemorySubjectsEnsureRequest {
        serde_json::from_value(json!({
            "space_id": space,
            "subject_id": subject,
        }))
        .expect("valid subjects.ensure request")
    }

    fn get_request(space: &str, subject: &str) -> MemorySubjectsGetRequest {
        serde_json::from_value(json!({
            "space_id": space,
            "subject_id": subject,
        }))
        .expect("valid subjects.get request")
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
        }
        serde_json::from_value(value).expect("valid grants.grant request")
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

    fn grant_list_request(
        space: &str,
        principal: Option<&str>,
        limit: Option<u64>,
        cursor: Option<&str>,
    ) -> MemoryGrantsListRequest {
        let mut value = json!({ "space_id": space });
        if let Some(principal) = principal {
            value["principal_id"] = json!(principal);
        }
        if let Some(limit) = limit {
            value["limit"] = json!(limit);
        }
        if let Some(cursor) = cursor {
            value["cursor"] = json!(cursor);
        }
        serde_json::from_value(value).expect("valid grants.list request")
    }

    fn without_subject() -> Value {
        json!({ "type": "subject" })
    }

    fn context_target(context_id: &str) -> Value {
        json!({ "type": "context", "context_id": context_id })
    }

    fn reject_code<T: std::fmt::Debug>(result: Result<T, SpokeReject>) -> SpokeRejectCode {
        match result {
            Ok(value) => panic!("expected a protocol reject, observed {value:?}"),
            Err(reject) => reject.code,
        }
    }

    /// The whole public refusal, for comparing two refusals field by field
    /// without pinning literal wording.
    fn refusal<T: std::fmt::Debug>(result: Result<T, SpokeReject>) -> SpokeReject {
        match result {
            Ok(value) => panic!("expected a protocol reject, observed {value:?}"),
            Err(reject) => reject,
        }
    }

    fn visible_space(page: &SpacePage) -> &MemorySpace {
        assert_eq!(page.spaces.len(), 1, "the fixture serves exactly one space");
        &page.spaces[0]
    }

    fn instant(text: &str) -> DateTime<Utc> {
        timestamp(text).expect("a valid scenario instant")
    }

    /// The one deterministic admission instant the scenario tests supply: every
    /// authorization decision and stored timestamp must come from it, never
    /// from the wall clock.
    fn now() -> DateTime<Utc> {
        instant("2020-01-01T00:00:00Z")
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

    fn expiring_grant_request(
        space: &str,
        grantee: &str,
        capabilities: &[&str],
        expires_at: DateTime<Utc>,
    ) -> MemoryGrantsGrantRequest {
        serde_json::from_value(json!({
            "space_id": space,
            "principal_id": grantee,
            "capabilities": capabilities,
            "expires_at": timestamp_text(expires_at),
            "idempotency_key": format!("grant-expiring-{grantee}"),
        }))
        .expect("valid expiring grants.grant request")
    }

    #[tokio::test]
    async fn space_identity_and_generation_are_stable_across_reopen() {
        let scenario = Scenario::open().await;
        let space_id = scenario.space_id.clone();
        let generation = scenario.provider.identity().authority_generation;

        let page = scenario
            .provider
            .spaces_list(OPERATOR, &spaces_list_request(None, None), now())
            .await
            .expect("operator lists spaces");
        let space = visible_space(&page);
        assert_eq!(space.space_id.as_str(), space_id);
        assert_eq!(space.authority_host_id.as_str(), scenario.provider.identity().authority_host_id);
        assert_eq!(space.authority_generation.get(), generation);
        assert_eq!(space.status.to_string(), "active");
        assert_eq!(space.revision.get(), 1);
        assert_eq!(space.pending_receipts, 0);
        assert!(page.next_cursor.is_none());

        // A principal with no grant sees no space; an unrelated principal gets
        // an empty page rather than an existence probe.
        let hidden = scenario
            .provider
            .spaces_list(OTHER, &spaces_list_request(None, None), now())
            .await
            .expect("unrelated principal lists spaces");
        assert!(hidden.spaces.is_empty());

        // Any valid grant makes the space visible (§7.2).
        scenario
            .provider
            .grants_grant(
                OPERATOR,
                &grant_request(&space_id, CLIENT, &["capture"], None, None),
                now(),
            )
            .await
            .expect("operator issues capture grant");
        let visible = scenario
            .provider
            .spaces_list(CLIENT, &spaces_list_request(Some(10), None), now())
            .await
            .expect("grantee lists spaces");
        assert_eq!(visible_space(&visible).space_id.as_str(), space_id);

        let reopened = scenario.reopen().await;
        let page = reopened
            .provider
            .spaces_list(OPERATOR, &spaces_list_request(None, None), now())
            .await
            .expect("operator lists spaces after reopen");
        let space = visible_space(&page);
        assert_eq!(space.space_id.as_str(), space_id);
        assert_eq!(space.authority_generation.get(), generation);
        assert_eq!(space.revision.get(), 1);
    }

    #[tokio::test]
    async fn freeze_allowlist_admits_reads_unfreeze_and_revoke_but_rejects_writes() {
        let scenario = Scenario::open().await;
        let space_id = scenario.space_id.clone();
        let issued = scenario
            .provider
            .grants_grant(
                OPERATOR,
                &grant_request(&space_id, CLIENT, &["capture"], None, None),
                now(),
            )
            .await
            .expect("operator issues capture grant");
        scenario
            .provider
            .subjects_ensure(CLIENT, &ensure_request(&space_id, "subject-one"), now())
            .await
            .expect("client ensures a subject before the freeze");

        let frozen = scenario
            .provider
            .spaces_freeze(OPERATOR, &freeze_request(&space_id, 1), now())
            .await
            .expect("operator freezes the space");
        assert_eq!(frozen.status, "frozen");
        assert_eq!(frozen.revision, 2);
        assert_eq!(frozen.pending_receipts, 0);

        // Mutations outside the §8.2 allowlist fail closed.
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .subjects_ensure(CLIENT, &ensure_request(&space_id, "subject-two"), now())
                    .await,
            ),
            SpokeRejectCode::SpaceFrozen
        );
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .grants_grant(
                        OPERATOR,
                        &grant_request(&space_id, OTHER, &["capture"], None, None),
                        now(),
                    )
                    .await,
            ),
            SpokeRejectCode::SpaceFrozen
        );

        // Reads stay admitted and report the frozen status.
        let page = scenario
            .provider
            .spaces_list(OPERATOR, &spaces_list_request(None, None), now())
            .await
            .expect("spaces.list is allowlisted while frozen");
        assert_eq!(visible_space(&page).status.to_string(), "frozen");
        scenario
            .provider
            .subjects_get(OPERATOR, &get_request(&space_id, "subject-one"), now())
            .await
            .expect("subjects.get is allowlisted while frozen");
        scenario
            .provider
            .grants_list(OPERATOR, &grant_list_request(&space_id, None, None, None), now())
            .await
            .expect("grants.list is allowlisted while frozen");

        // Revoke is the one mutation on the allowlist (§8.2).
        let revoked = scenario
            .provider
            .grants_revoke(OPERATOR, &revoke_request(&space_id, &issued.grant_id, 1), now())
            .await
            .expect("revoke is allowlisted while frozen");
        assert!(revoked.revoked);
        assert_eq!(revoked.revision, 2);

        // Freezing an already frozen space with the matching revision is a
        // no-op success and does not bump the revision (§7.0.4).
        let no_op = scenario
            .provider
            .spaces_freeze(OPERATOR, &freeze_request(&space_id, 2), now())
            .await
            .expect("repeat freeze is a no-op success");
        assert_eq!(no_op.revision, 2);

        let thawed = scenario
            .provider
            .spaces_unfreeze(OPERATOR, &unfreeze_request(&space_id, 2), now())
            .await
            .expect("unfreeze is allowlisted");
        assert_eq!(thawed.status, "active");
        assert_eq!(thawed.revision, 3);

        scenario
            .provider
            .subjects_ensure(OPERATOR, &ensure_request(&space_id, "subject-two"), now())
            .await
            .expect("writes are admitted again after unfreeze");
    }

    #[tokio::test]
    async fn stale_revision_cas_rejects_fence_and_freeze_lost_updates() {
        let scenario = Scenario::open().await;
        let space_id = scenario.space_id.clone();
        scenario
            .provider
            .spaces_freeze(OPERATOR, &freeze_request(&space_id, 1), now())
            .await
            .expect("freeze at revision 1");
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .spaces_freeze(OPERATOR, &freeze_request(&space_id, 1), now())
                    .await,
            ),
            SpokeRejectCode::RevisionConflict
        );
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .spaces_unfreeze(OPERATOR, &unfreeze_request(&space_id, 1), now())
                    .await,
            ),
            SpokeRejectCode::RevisionConflict
        );
        scenario
            .provider
            .spaces_unfreeze(OPERATOR, &unfreeze_request(&space_id, 2), now())
            .await
            .expect("unfreeze at the current revision");

        scenario
            .provider
            .subjects_ensure(OPERATOR, &ensure_request(&space_id, "subject-one"), now())
            .await
            .expect("ensure subject");
        scenario
            .provider
            .subjects_fence(
                OPERATOR,
                &fence_request(&space_id, "subject-one", "fence", without_subject(), 1),
                now(),
            )
            .await
            .expect("fence subject at revision 1");
        // A stale writer cannot force a second transition over the fence.
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .subjects_fence(
                        OPERATOR,
                        &fence_request(&space_id, "subject-one", "unfence", without_subject(), 1),
                        now(),
                    )
                    .await,
            ),
            SpokeRejectCode::RevisionConflict
        );
        let unfenced = scenario
            .provider
            .subjects_fence(
                OPERATOR,
                &fence_request(&space_id, "subject-one", "unfence", without_subject(), 2),
                now(),
            )
            .await
            .expect("unfence at the current revision");
        assert_eq!(unfenced.status, "active");
        assert_eq!(unfenced.revision, 3);
    }

    #[tokio::test]
    async fn subject_fence_applies_across_clients_and_context_fence_stays_scoped() {
        let scenario = Scenario::open().await;
        let space_id = scenario.space_id.clone();
        scenario
            .provider
            .grants_grant(
                OPERATOR,
                &grant_request(&space_id, CLIENT, &["capture"], None, None),
                now(),
            )
            .await
            .expect("operator issues capture grant");

        scenario
            .provider
            .subjects_ensure(CLIENT, &ensure_request(&space_id, "subject-one"), now())
            .await
            .expect("client ensures subject-one");
        scenario
            .provider
            .subjects_fence(
                OPERATOR,
                &fence_request(&space_id, "subject-one", "fence", without_subject(), 1),
                now(),
            )
            .await
            .expect("operator fences subject-one");

        // The other client is fenced by provider state, not by process state.
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .subjects_ensure(CLIENT, &ensure_request(&space_id, "subject-one"), now())
                    .await,
            ),
            SpokeRejectCode::SubjectFenced
        );
        // Reads remain admitted on a fenced subject.
        let fenced = scenario
            .provider
            .subjects_get(CLIENT, &get_request(&space_id, "subject-one"), now())
            .await
            .expect("reads stay admitted on a fenced subject");
        assert_eq!(fenced.status.to_string(), "fenced");

        // A context transition on a subject-fenced subject must not clear the
        // subject fence.
        let context_fenced = scenario
            .provider
            .subjects_fence(
                OPERATOR,
                &fence_request(&space_id, "subject-one", "fence", context_target("ctx-x"), 2),
                now(),
            )
            .await
            .expect("context fence on a subject-fenced subject");
        assert_eq!(context_fenced.status, "fenced");
        assert_eq!(context_fenced.fenced_contexts, vec!["ctx-x".to_owned()]);
        assert_eq!(context_fenced.revision, 3);

        // A sibling subject is untouched (MEM-R024 has no collateral effect).
        scenario
            .provider
            .subjects_ensure(CLIENT, &ensure_request(&space_id, "subject-two"), now())
            .await
            .expect("sibling subject stays writable");

        scenario
            .provider
            .subjects_fence(
                OPERATOR,
                &fence_request(
                    &space_id,
                    "subject-two",
                    "fence",
                    context_target("ctx-a"),
                    1,
                ),
                now(),
            )
            .await
            .expect("fence one placement context");
        // A write naming the fenced context fails with the context scope.
        let rejected = scenario
            .provider
            .grants_grant(
                OPERATOR,
                &grant_request(
                    &space_id,
                    CLIENT,
                    &["capture"],
                    Some("subject-two"),
                    Some(&["ctx-a"]),
                ),
                now(),
            )
            .await;
        assert_eq!(reject_code(rejected), SpokeRejectCode::SubjectFenced);
        // The sibling context stays fully usable.
        scenario
            .provider
            .grants_grant(
                OPERATOR,
                &grant_request(
                    &space_id,
                    CLIENT,
                    &["capture"],
                    Some("subject-two"),
                    Some(&["ctx-b"]),
                ),
                now(),
            )
            .await
            .expect("sibling context stays usable");
        // Unfencing the context restores it.
        scenario
            .provider
            .subjects_fence(
                OPERATOR,
                &fence_request(
                    &space_id,
                    "subject-two",
                    "unfence",
                    context_target("ctx-a"),
                    2,
                ),
                now(),
            )
            .await
            .expect("unfence the context");
        scenario
            .provider
            .grants_grant(
                OPERATOR,
                &grant_request(
                    &space_id,
                    CLIENT,
                    &["capture"],
                    Some("subject-two"),
                    Some(&["ctx-a"]),
                ),
                now(),
            )
            .await
            .expect("unfenced context accepts writes again");
    }

    #[tokio::test]
    async fn revoke_takes_effect_at_the_next_admission_boundary() {
        let scenario = Scenario::open().await;
        let space_id = scenario.space_id.clone();
        let issued = scenario
            .provider
            .grants_grant(
                OPERATOR,
                &grant_request(&space_id, CLIENT, &["capture"], None, None),
                now(),
            )
            .await
            .expect("operator issues capture grant");
        scenario
            .provider
            .subjects_ensure(CLIENT, &ensure_request(&space_id, "subject-one"), now())
            .await
            .expect("grant works before the revoke");

        let revoked = scenario
            .provider
            .grants_revoke(OPERATOR, &revoke_request(&space_id, &issued.grant_id, 1), now())
            .await
            .expect("operator revokes the grant");
        assert!(revoked.revoked);
        assert_eq!(revoked.revision, 2);

        // The next admission is refused; the previous call is not re-evaluated.
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .subjects_ensure(CLIENT, &ensure_request(&space_id, "subject-two"), now())
                    .await,
            ),
            SpokeRejectCode::NotAuthorized
        );
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .subjects_get(CLIENT, &get_request(&space_id, "subject-one"), now())
                    .await,
            ),
            SpokeRejectCode::NotAuthorized
        );

        // The revoked grant is still visible to its holder, marked revoked.
        let page = scenario
            .provider
            .grants_list(CLIENT, &grant_list_request(&space_id, None, None, None), now())
            .await
            .expect("holders keep their own grant view");
        assert_eq!(page.grants.len(), 1);
        let held = &page.grants[0];
        match held {
            MemoryGrant::UnrestrictedGrant(grant) => {
                assert_eq!(grant.revoked_at.as_deref().copied(), Some(now()));
                assert_eq!(grant.revision.get(), 2);
            }
            other => panic!("expected an unrestricted grant, got {other:?}"),
        }

        // Stale and repeated revokes follow the CAS and no-op rules.
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .grants_revoke(OPERATOR, &revoke_request(&space_id, &issued.grant_id, 1), now())
                    .await,
            ),
            SpokeRejectCode::RevisionConflict
        );
        let repeated = scenario
            .provider
            .grants_revoke(OPERATOR, &revoke_request(&space_id, &issued.grant_id, 2), now())
            .await
            .expect("re-revoking at the current revision is a no-op success");
        assert_eq!(repeated.revision, 2);

        // A non-admin cannot probe grant ids for existence.
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .grants_revoke(CLIENT, &revoke_request(&space_id, "grt_missing", 1), now())
                    .await,
            ),
            SpokeRejectCode::NotAuthorized
        );
    }

    #[tokio::test]
    async fn grant_issuance_cannot_amplify_granter_coverage() {
        let scenario = Scenario::open().await;
        let space_id = scenario.space_id.clone();
        scenario
            .provider
            .grants_grant(
                OPERATOR,
                &grant_request(
                    &space_id,
                    CLIENT,
                    &["admin"],
                    None,
                    Some(&["ctx-a"]),
                ),
                now(),
            )
            .await
            .expect("operator issues a context-restricted admin grant");

        // Unrestricted issuance would escalate to every context.
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .grants_grant(
                        CLIENT,
                        &grant_request(&space_id, OTHER, &["capture"], None, None),
                        now(),
                    )
                    .await,
            ),
            SpokeRejectCode::NotAuthorized
        );
        // A context outside the granter's coverage is amplification too.
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .grants_grant(
                        CLIENT,
                        &grant_request(
                            &space_id,
                            OTHER,
                            &["capture"],
                            None,
                            Some(&["ctx-b"]),
                        ),
                        now(),
                    )
                    .await,
            ),
            SpokeRejectCode::NotAuthorized
        );
        // Within coverage the issued grant is accepted.
        let issued = scenario
            .provider
            .grants_grant(
                CLIENT,
                &grant_request(
                    &space_id,
                    OTHER,
                    &["capture"],
                    None,
                    Some(&["ctx-a"]),
                ),
                now(),
            )
            .await
            .expect("issuance inside the granter coverage");
        assert_eq!(issued.revision, 1);
        // The provider-assigned id is a real handle: the admin lists the grant
        // it just issued by that id.
        let listed = scenario
            .provider
            .grants_list(
                OPERATOR,
                &grant_list_request(&space_id, Some(OTHER), None, None),
                now(),
            )
            .await
            .expect("operator lists the issued grant");
        assert!(listed
            .grants
            .iter()
            .any(|grant| grant_id(grant) == issued.grant_id));

        // A capture-only principal can never issue.
        let capture_only = Scenario::open().await;
        capture_only
            .provider
            .grants_grant(
                OPERATOR,
                &grant_request(&capture_only.space_id, CLIENT, &["capture"], None, None),
                now(),
            )
            .await
            .expect("capture grant");
        assert_eq!(
            reject_code(
                capture_only
                    .provider
                    .grants_grant(
                        CLIENT,
                        &grant_request(&capture_only.space_id, OTHER, &["capture"], None, None),
                        now(),
                    )
                    .await,
            ),
            SpokeRejectCode::NotAuthorized
        );

        // Subject-scope amplification: a subject admin cannot issue space-wide.
        scenario
            .provider
            .subjects_ensure(OPERATOR, &ensure_request(&space_id, "subject-one"), now())
            .await
            .expect("ensure subject-one");
        scenario
            .provider
            .grants_grant(
                OPERATOR,
                &grant_request(&space_id, CLIENT, &["admin"], Some("subject-one"), None),
                now(),
            )
            .await
            .expect("operator issues a subject-scoped admin grant");
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .grants_grant(
                        CLIENT,
                        &grant_request(&space_id, OTHER, &["capture"], None, None),
                        now(),
                    )
                    .await,
            ),
            SpokeRejectCode::NotAuthorized
        );
        scenario
            .provider
            .grants_grant(
                CLIENT,
                &grant_request(&space_id, OTHER, &["capture"], Some("subject-one"), None),
                now(),
            )
            .await
            .expect("same-subject issuance stays within coverage");
    }

    #[tokio::test]
    async fn freeze_fence_and_grants_survive_reopen() {
        let scenario = Scenario::open().await;
        let space_id = scenario.space_id.clone();
        scenario
            .provider
            .subjects_ensure(OPERATOR, &ensure_request(&space_id, "subject-one"), now())
            .await
            .expect("operator ensures subject-one");
        scenario
            .provider
            .grants_grant(
                OPERATOR,
                &grant_request(&space_id, CLIENT, &["capture"], Some("subject-one"), None),
                now(),
            )
            .await
            .expect("operator issues a subject-scoped capture grant");
        scenario
            .provider
            .subjects_ensure(CLIENT, &ensure_request(&space_id, "subject-one"), now())
            .await
            .expect("client ensures subject-one");
        scenario
            .provider
            .subjects_fence(
                OPERATOR,
                &fence_request(
                    &space_id,
                    "subject-one",
                    "fence",
                    context_target("ctx-a"),
                    1,
                ),
                now(),
            )
            .await
            .expect("fence a context");
        scenario
            .provider
            .spaces_freeze(OPERATOR, &freeze_request(&space_id, 1), now())
            .await
            .expect("freeze the space");

        let reopened = scenario.reopen().await;
        let page = reopened
            .provider
            .spaces_list(OPERATOR, &spaces_list_request(None, None), now())
            .await
            .expect("spaces.list after reopen");
        let space = visible_space(&page);
        assert_eq!(space.status.to_string(), "frozen");
        assert_eq!(space.revision.get(), 2);
        assert_eq!(
            space.authority_generation.get(),
            reopened.provider.identity().authority_generation
        );

        let subject = reopened
            .provider
            .subjects_get(OPERATOR, &get_request(&space_id, "subject-one"), now())
            .await
            .expect("subject survives the reopen");
        assert_eq!(subject.status.to_string(), "active");
        assert_eq!(subject.fenced_contexts, vec!["ctx-a".to_owned()]);
        assert_eq!(subject.revision.get(), 2);

        // The persisted grant still admits the client, and the persisted freeze
        // still rejects writes.
        let grants = reopened
            .provider
            .grants_list(OPERATOR, &grant_list_request(&space_id, Some(CLIENT), None, None), now())
            .await
            .expect("grants survive the reopen");
        assert_eq!(grants.grants.len(), 1);
        assert_eq!(
            reject_code(
                reopened
                    .provider
                    .subjects_ensure(CLIENT, &ensure_request(&space_id, "subject-two"), now())
                    .await,
            ),
            SpokeRejectCode::SpaceFrozen
        );
    }

    #[tokio::test]
    async fn subject_archive_is_terminal_and_context_archive_is_invalid_input() {
        let scenario = Scenario::open().await;
        let space_id = scenario.space_id.clone();
        scenario
            .provider
            .subjects_ensure(OPERATOR, &ensure_request(&space_id, "subject-one"), now())
            .await
            .expect("ensure subject-one");

        let archived = scenario
            .provider
            .subjects_fence(
                OPERATOR,
                &fence_request(&space_id, "subject-one", "archive", without_subject(), 1),
                now(),
            )
            .await
            .expect("archive is reachable from active");
        assert_eq!(archived.status, "archived");
        assert_eq!(archived.revision, 2);

        for (action, target) in [
            ("archive", without_subject()),
            ("fence", without_subject()),
            ("unfence", without_subject()),
            ("fence", context_target("ctx-a")),
        ] {
            assert_eq!(
                reject_code(
                    scenario
                        .provider
                        .subjects_fence(
                            OPERATOR,
                            &fence_request(&space_id, "subject-one", action, target, 2),
                            now(),
                        )
                        .await,
                ),
                SpokeRejectCode::InvalidStatusTransition
            );
        }
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .subjects_fence(
                        OPERATOR,
                        &fence_request(
                            &space_id,
                            "subject-one",
                            "archive",
                            context_target("ctx-a"),
                            2,
                        ),
                        now(),
                    )
                    .await,
            ),
            SpokeRejectCode::InvalidInput
        );
        // An archived subject rejects writes.
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .subjects_ensure(OPERATOR, &ensure_request(&space_id, "subject-one"), now())
                    .await,
            ),
            SpokeRejectCode::SubjectFenced
        );
    }

    #[tokio::test]
    async fn grants_list_pages_bound_and_a_grant_change_invalidates_the_cursor() {
        let scenario = Scenario::open().await;
        let space_id = scenario.space_id.clone();
        let mut issued = Vec::new();
        for grantee in ["principal-g1", "principal-g2", "principal-g3"] {
            issued.push(
                scenario
                    .provider
                    .grants_grant(
                        OPERATOR,
                        &grant_request(&space_id, grantee, &["capture"], None, None),
                        now(),
                    )
                    .await
                    .expect("issue a grant per grantee"),
            );
        }

        let first = scenario
            .provider
            .grants_list(OPERATOR, &grant_list_request(&space_id, None, Some(2), None), now())
            .await
            .expect("first page");
        assert_eq!(first.grants.len(), 2);
        let cursor = first.next_cursor.expect("a bounded page issues a cursor");

        let second = scenario
            .provider
            .grants_list(OPERATOR, &grant_list_request(&space_id, None, Some(2), Some(&cursor)), now())
            .await
            .expect("second page");
        assert_eq!(second.grants.len(), 2);
        assert!(second.next_cursor.is_none());

        // The target selector is admin-only: a non-admin may not name another
        // principal, nor even itself (§6.4/§7.8).
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .grants_list(CLIENT, &grant_list_request(&space_id, Some(OTHER), None, None), now())
                    .await,
            ),
            SpokeRejectCode::NotAuthorized
        );
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .grants_list(CLIENT, &grant_list_request(&space_id, Some(CLIENT), None, None), now())
                    .await,
            ),
            SpokeRejectCode::NotAuthorized
        );

        // The selector-omitted self view lists the principal's real grant.
        scenario
            .provider
            .grants_grant(
                OPERATOR,
                &grant_request(&space_id, CLIENT, &["capture"], None, None),
                now(),
            )
            .await
            .expect("the client holds one grant");
        let self_view = scenario
            .provider
            .grants_list(CLIENT, &grant_list_request(&space_id, None, None, None), now())
            .await
            .expect("self view");
        assert_eq!(self_view.grants.len(), 1);
        assert_eq!(grant_principal(&self_view.grants[0]), CLIENT);

        // Any grant change invalidates an outstanding cursor (MEM-R026).
        let outstanding = scenario
            .provider
            .grants_list(OPERATOR, &grant_list_request(&space_id, None, Some(2), None), now())
            .await
            .expect("first page over the current state");
        let cursor = outstanding
            .next_cursor
            .expect("a bounded page issues a cursor");
        scenario
            .provider
            .grants_revoke(OPERATOR, &revoke_request(&space_id, &issued[0].grant_id, 1), now())
            .await
            .expect("revoke one grant");
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .grants_list(
                        OPERATOR,
                        &grant_list_request(&space_id, None, Some(2), Some(&cursor)),
                        now(),
                    )
                    .await,
            ),
            SpokeRejectCode::CursorInvalid
        );
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .grants_list(
                        OPERATOR,
                        &grant_list_request(&space_id, None, Some(2), Some("not-a-cursor")),
                        now(),
                    )
                    .await,
            ),
            SpokeRejectCode::CursorInvalid
        );
    }

    fn grant_principal(grant: &MemoryGrant) -> &str {
        match grant {
            MemoryGrant::RestrictedGrant(grant) => grant.principal_id.as_str(),
            MemoryGrant::UnrestrictedGrant(grant) => grant.principal_id.as_str(),
        }
    }

    fn grant_id(grant: &MemoryGrant) -> &str {
        match grant {
            MemoryGrant::RestrictedGrant(grant) => grant.grant_id.as_str(),
            MemoryGrant::UnrestrictedGrant(grant) => grant.grant_id.as_str(),
        }
    }

    fn grant_state(grant: &MemoryGrant) -> (u64, bool) {
        match grant {
            MemoryGrant::RestrictedGrant(grant) => {
                (grant.revision.get(), grant.revoked_at.is_some())
            }
            MemoryGrant::UnrestrictedGrant(grant) => {
                (grant.revision.get(), grant.revoked_at.is_some())
            }
        }
    }

    fn grant_created_at(grant: &MemoryGrant) -> DateTime<Utc> {
        match grant {
            MemoryGrant::RestrictedGrant(grant) => grant.created_at,
            MemoryGrant::UnrestrictedGrant(grant) => grant.created_at.0,
        }
    }

    fn grant_expires_at(grant: &MemoryGrant) -> Option<DateTime<Utc>> {
        match grant {
            MemoryGrant::RestrictedGrant(grant) => grant.expires_at.as_deref().copied(),
            MemoryGrant::UnrestrictedGrant(grant) => grant.expires_at.as_deref().copied(),
        }
    }

    #[tokio::test]
    async fn context_scoped_admin_cannot_act_outside_its_coverage() {
        let scenario = Scenario::open().await;
        let space_id = scenario.space_id.clone();
        // A space-wide admin grant restricted to the ctx-a placement.
        scenario
            .provider
            .grants_grant(
                OPERATOR,
                &grant_request(&space_id, CLIENT, &["admin"], None, Some(&["ctx-a"])),
                now(),
            )
            .await
            .expect("operator issues a ctx-a-restricted admin grant");
        scenario
            .provider
            .subjects_ensure(OPERATOR, &ensure_request(&space_id, "subject-one"), now())
            .await
            .expect("operator ensures subject-one");

        // A context outside the admin's coverage cannot be fenced.
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .subjects_fence(
                        CLIENT,
                        &fence_request(
                            &space_id,
                            "subject-one",
                            "fence",
                            context_target("ctx-b"),
                            1,
                        ),
                        now(),
                    )
                    .await,
            ),
            SpokeRejectCode::NotAuthorized
        );
        let untouched = scenario
            .provider
            .subjects_get(OPERATOR, &get_request(&space_id, "subject-one"), now())
            .await
            .expect("subject after the refused fence");
        assert!(untouched.fenced_contexts.is_empty());
        assert_eq!(untouched.revision.get(), 1);

        // Nor can it unfence a context fenced by a broader admin.
        scenario
            .provider
            .subjects_fence(
                OPERATOR,
                &fence_request(&space_id, "subject-one", "fence", context_target("ctx-b"), 1),
                now(),
            )
            .await
            .expect("operator fences ctx-b");
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .subjects_fence(
                        CLIENT,
                        &fence_request(
                            &space_id,
                            "subject-one",
                            "unfence",
                            context_target("ctx-b"),
                            2,
                        ),
                        now(),
                    )
                    .await,
            ),
            SpokeRejectCode::NotAuthorized
        );
        let still_fenced = scenario
            .provider
            .subjects_get(OPERATOR, &get_request(&space_id, "subject-one"), now())
            .await
            .expect("subject after the refused unfence");
        assert_eq!(still_fenced.fenced_contexts, vec!["ctx-b".to_owned()]);
        assert_eq!(still_fenced.revision.get(), 2);

        // Inside its coverage the same admin can fence.
        let in_scope = scenario
            .provider
            .subjects_fence(
                CLIENT,
                &fence_request(&space_id, "subject-one", "fence", context_target("ctx-a"), 2),
                now(),
            )
            .await
            .expect("in-scope context fence");
        assert_eq!(
            in_scope.fenced_contexts,
            vec!["ctx-b".to_owned(), "ctx-a".to_owned()]
        );
        assert_eq!(in_scope.revision, 3);

        // Grants wider than the admin's own coverage cannot be revoked.
        let broader = scenario
            .provider
            .grants_grant(
                OPERATOR,
                &grant_request(&space_id, OTHER, &["capture"], None, None),
                now(),
            )
            .await
            .expect("operator issues an unrestricted grant");
        let foreign = scenario
            .provider
            .grants_grant(
                OPERATOR,
                &grant_request(&space_id, OTHER, &["capture"], None, Some(&["ctx-b"])),
                now(),
            )
            .await
            .expect("operator issues a ctx-b grant");
        let shared = scenario
            .provider
            .grants_grant(
                OPERATOR,
                &shared_grant_request(&space_id, OTHER, &["capture"], &["ctx-a"]),
                now(),
            )
            .await
            .expect("operator issues a shared-visibility grant");
        for grant in [&broader, &foreign, &shared] {
            assert_eq!(
                reject_code(
                    scenario
                        .provider
                        .grants_revoke(
                            CLIENT,
                            &revoke_request(&space_id, &grant.grant_id, 1),
                            now(),
                        )
                        .await,
                ),
                SpokeRejectCode::NotAuthorized
            );
        }
        let untouched_grants = scenario
            .provider
            .grants_list(OPERATOR, &grant_list_request(&space_id, Some(OTHER), None, None), now())
            .await
            .expect("operator lists the untouched grants");
        assert_eq!(untouched_grants.grants.len(), 3);
        for grant in &untouched_grants.grants {
            assert_eq!(
                grant_state(grant),
                (1, false),
                "a refused revoke must not change the target grant"
            );
        }
        scenario
            .provider
            .subjects_ensure(OTHER, &ensure_request(&space_id, "subject-two"), now())
            .await
            .expect("the refused revokes leave the grants valid");

        // A grant inside the admin's coverage is revocable, and the revocation
        // takes effect at the holder's next admission.
        let in_scope_grant = scenario
            .provider
            .grants_grant(
                OPERATOR,
                &grant_request(
                    &space_id,
                    "principal-other-2",
                    &["capture"],
                    None,
                    Some(&["ctx-a"]),
                ),
                now(),
            )
            .await
            .expect("operator issues a ctx-a grant");
        scenario
            .provider
            .subjects_ensure(
                "principal-other-2",
                &ensure_request(&space_id, "subject-two"),
                now(),
            )
            .await
            .expect("the ctx-a grant admits its holder");
        let revoked = scenario
            .provider
            .grants_revoke(
                CLIENT,
                &revoke_request(&space_id, &in_scope_grant.grant_id, 1),
                now(),
            )
            .await
            .expect("in-scope revoke by the ctx-a admin");
        assert_eq!(revoked.revision, 2);
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .subjects_ensure(
                        "principal-other-2",
                        &ensure_request(&space_id, "subject-three"),
                        now(),
                    )
                    .await,
            ),
            SpokeRejectCode::NotAuthorized
        );
    }

    #[tokio::test]
    async fn caller_transaction_controls_the_grant_and_freeze_mutations() {
        let scenario = Scenario::open().await;
        let space_id = scenario.space_id.clone();
        for grantee in ["principal-t1", "principal-t2"] {
            scenario
                .provider
                .grants_grant(
                    OPERATOR,
                    &grant_request(&space_id, grantee, &["capture"], None, None),
                    now(),
                )
                .await
                .expect("seed a grant");
        }
        let first = scenario
            .provider
            .grants_list(OPERATOR, &grant_list_request(&space_id, None, Some(1), None), now())
            .await
            .expect("first page");
        let cursor = first.next_cursor.expect("a bounded page issues a cursor");

        // A caller-owned transaction that rolls back discards the staged grant
        // together with its snapshot bump.
        let mut transaction = scenario
            .provider
            .store()
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .expect("caller transaction");
        let staged = scenario
            .provider
            .grants_grant_in_tx(
                &mut transaction,
                OPERATOR,
                &grant_request(&space_id, CLIENT, &["capture"], None, None),
                now(),
            )
            .await
            .expect("stage the grant");
        transaction.rollback().await.expect("caller rollback");

        let resumed = scenario
            .provider
            .grants_list(
                OPERATOR,
                &grant_list_request(&space_id, None, Some(1), Some(&cursor)),
                now(),
            )
            .await
            .expect("the rolled-back marker bump leaves the cursor valid");
        assert_eq!(resumed.grants.len(), 1);
        let client_grants = scenario
            .provider
            .grants_list(OPERATOR, &grant_list_request(&space_id, Some(CLIENT), None, None), now())
            .await
            .expect("no client grant survived the rollback");
        assert!(client_grants.grants.is_empty());
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .subjects_ensure(CLIENT, &ensure_request(&space_id, "subject-one"), now())
                    .await,
            ),
            SpokeRejectCode::NotAuthorized
        );
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .grants_revoke(OPERATOR, &revoke_request(&space_id, &staged.grant_id, 1), now())
                    .await,
            ),
            SpokeRejectCode::NotFound
        );

        // A staged freeze is discarded the same way.
        let mut transaction = scenario
            .provider
            .store()
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .expect("caller transaction");
        scenario
            .provider
            .spaces_freeze_in_tx(&mut transaction, OPERATOR, &freeze_request(&space_id, 1), now())
            .await
            .expect("stage the freeze");
        transaction.rollback().await.expect("caller rollback");
        let page = scenario
            .provider
            .spaces_list(OPERATOR, &spaces_list_request(None, None), now())
            .await
            .expect("space after the rolled-back freeze");
        assert_eq!(visible_space(&page).status.to_string(), "active");
        assert_eq!(visible_space(&page).revision.get(), 1);

        // A caller commit applies the staged grant.
        let mut transaction = scenario
            .provider
            .store()
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .expect("caller transaction");
        scenario
            .provider
            .grants_grant_in_tx(
                &mut transaction,
                OPERATOR,
                &grant_request(&space_id, CLIENT, &["capture"], None, None),
                now(),
            )
            .await
            .expect("stage the grant");
        transaction.commit().await.expect("caller commit");
        scenario
            .provider
            .subjects_ensure(CLIENT, &ensure_request(&space_id, "subject-one"), now())
            .await
            .expect("the committed grant admits the client");

        // A caller commit applies the staged freeze.
        let mut transaction = scenario
            .provider
            .store()
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .expect("caller transaction");
        scenario
            .provider
            .spaces_freeze_in_tx(&mut transaction, OPERATOR, &freeze_request(&space_id, 1), now())
            .await
            .expect("stage the freeze");
        transaction.commit().await.expect("caller commit");
        let page = scenario
            .provider
            .spaces_list(OPERATOR, &spaces_list_request(None, None), now())
            .await
            .expect("space after the committed freeze");
        assert_eq!(visible_space(&page).status.to_string(), "frozen");
        assert_eq!(visible_space(&page).revision.get(), 2);
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .subjects_ensure(OPERATOR, &ensure_request(&space_id, "subject-two"), now())
                    .await,
            ),
            SpokeRejectCode::SpaceFrozen
        );
    }

    #[tokio::test]
    async fn grant_cursor_is_bound_to_the_provider_snapshot_not_only_the_page_rows() {
        let scenario = Scenario::open().await;
        let space_id = scenario.space_id.clone();
        scenario
            .provider
            .grants_grant(
                OPERATOR,
                &grant_request(&space_id, CLIENT, &["capture"], None, None),
                now(),
            )
            .await
            .expect("first client grant");
        scenario
            .provider
            .grants_grant(
                OPERATOR,
                &grant_request(&space_id, CLIENT, &["records_read"], None, None),
                now(),
            )
            .await
            .expect("second client grant");

        let first = scenario
            .provider
            .grants_list(CLIENT, &grant_list_request(&space_id, None, Some(1), None), now())
            .await
            .expect("self-view first page");
        assert_eq!(first.grants.len(), 1);
        let cursor = first.next_cursor.expect("a bounded self view issues a cursor");

        // A grant to another principal never appears in this view, yet the
        // outstanding cursor is invalidated.
        scenario
            .provider
            .grants_grant(
                OPERATOR,
                &grant_request(&space_id, OTHER, &["capture"], None, None),
                now(),
            )
            .await
            .expect("foreign grant");
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .grants_list(
                        CLIENT,
                        &grant_list_request(&space_id, None, Some(1), Some(&cursor)),
                        now(),
                    )
                    .await,
            ),
            SpokeRejectCode::CursorInvalid
        );

        // A lifecycle change with identical grant rows invalidates it too.
        let fresh = scenario
            .provider
            .grants_list(CLIENT, &grant_list_request(&space_id, None, Some(1), None), now())
            .await
            .expect("fresh self-view first page");
        let cursor = fresh.next_cursor.expect("a bounded self view issues a cursor");
        scenario
            .provider
            .spaces_freeze(OPERATOR, &freeze_request(&space_id, 1), now())
            .await
            .expect("freeze the space");
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .grants_list(
                        CLIENT,
                        &grant_list_request(&space_id, None, Some(1), Some(&cursor)),
                        now(),
                    )
                    .await,
            ),
            SpokeRejectCode::CursorInvalid
        );
    }

    #[tokio::test]
    async fn whole_target_admin_requires_full_subject_or_space_coverage() {
        let scenario = Scenario::open().await;
        let space_id = scenario.space_id.clone();
        // A space-wide admin grant that covers only the ctx-a placement.
        scenario
            .provider
            .grants_grant(
                OPERATOR,
                &grant_request(&space_id, CLIENT, &["admin"], None, Some(&["ctx-a"])),
                now(),
            )
            .await
            .expect("operator issues a ctx-a-restricted admin grant");
        // A grant in another placement: it only reaches a full grant view.
        scenario
            .provider
            .grants_grant(
                OPERATOR,
                &grant_request(&space_id, OTHER, &["capture"], None, Some(&["ctx-b"])),
                now(),
            )
            .await
            .expect("operator issues a ctx-b grant");
        scenario
            .provider
            .subjects_ensure(OPERATOR, &ensure_request(&space_id, "subject-one"), now())
            .await
            .expect("operator ensures subject-one");

        // Whole-subject fence and archive span every placement context of the
        // subject, so the ctx-a-only admin is refused both.
        for action in ["fence", "archive"] {
            assert_eq!(
                reject_code(
                    scenario
                        .provider
                        .subjects_fence(
                            CLIENT,
                            &fence_request(&space_id, "subject-one", action, without_subject(), 1),
                            now(),
                        )
                        .await,
                ),
                SpokeRejectCode::NotAuthorized,
                "a ctx-a-only admin must not {action} the whole subject"
            );
        }
        let untouched = scenario
            .provider
            .subjects_get(OPERATOR, &get_request(&space_id, "subject-one"), now())
            .await
            .expect("subject after the refused whole-subject actions");
        assert_eq!(untouched.status.to_string(), "active");
        assert!(untouched.fenced_contexts.is_empty());
        assert_eq!(untouched.revision.get(), 1);

        // The whole-space maintenance gate is closed to the same admin, and
        // the refusals leave the space untouched.
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .spaces_freeze(CLIENT, &freeze_request(&space_id, 1), now())
                    .await,
            ),
            SpokeRejectCode::NotAuthorized
        );
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .spaces_unfreeze(CLIENT, &unfreeze_request(&space_id, 1), now())
                    .await,
            ),
            SpokeRejectCode::NotAuthorized
        );
        let page = scenario
            .provider
            .spaces_list(OPERATOR, &spaces_list_request(None, None), now())
            .await
            .expect("space after the refused maintenance gates");
        assert_eq!(visible_space(&page).status.to_string(), "active");
        assert_eq!(visible_space(&page).revision.get(), 1);

        // The same admin never obtains a full grant view: the selector-omitted
        // list is its own self view, and naming the ctx-b grantee is refused.
        let self_view = scenario
            .provider
            .grants_list(CLIENT, &grant_list_request(&space_id, None, None, None), now())
            .await
            .expect("the ctx-a admin self view");
        assert_eq!(self_view.grants.len(), 1);
        assert_eq!(grant_principal(&self_view.grants[0]), CLIENT);
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .grants_list(
                        CLIENT,
                        &grant_list_request(&space_id, Some(OTHER), None, None),
                        now(),
                    )
                    .await,
            ),
            SpokeRejectCode::NotAuthorized
        );

        // Full coverage admits the same operations. An unrestricted but
        // subject-scoped admin controls its whole subject...
        scenario
            .provider
            .grants_grant(
                OPERATOR,
                &grant_request(&space_id, CLIENT, &["admin"], Some("subject-one"), None),
                now(),
            )
            .await
            .expect("operator issues an unrestricted subject admin grant");
        let fenced = scenario
            .provider
            .subjects_fence(
                CLIENT,
                &fence_request(&space_id, "subject-one", "fence", without_subject(), 1),
                now(),
            )
            .await
            .expect("the whole-subject admin fences its subject");
        assert_eq!(fenced.status, "fenced");
        assert_eq!(fenced.revision, 2);
        let unfenced = scenario
            .provider
            .subjects_fence(
                CLIENT,
                &fence_request(&space_id, "subject-one", "unfence", without_subject(), 2),
                now(),
            )
            .await
            .expect("the whole-subject admin unfences its subject");
        assert_eq!(unfenced.status, "active");
        assert_eq!(unfenced.revision, 3);

        // ...and the space-wide unrestricted operator admin controls the whole
        // space and sees every grant.
        let frozen = scenario
            .provider
            .spaces_freeze(OPERATOR, &freeze_request(&space_id, 1), now())
            .await
            .expect("the space-wide admin freezes the space");
        assert_eq!(frozen.status, "frozen");
        assert_eq!(frozen.revision, 2);
        let thawed = scenario
            .provider
            .spaces_unfreeze(OPERATOR, &unfreeze_request(&space_id, 2), now())
            .await
            .expect("the space-wide admin unfreezes the space");
        assert_eq!(thawed.status, "active");
        assert_eq!(thawed.revision, 3);
        let full = scenario
            .provider
            .grants_list(OPERATOR, &grant_list_request(&space_id, None, None, None), now())
            .await
            .expect("the space-wide admin full view");
        assert_eq!(full.grants.len(), 4);
        assert!(full
            .grants
            .iter()
            .any(|grant| grant_principal(grant) == OTHER));
    }

    #[tokio::test]
    async fn grant_page_rows_and_marker_share_one_read_snapshot() {
        let scenario = Scenario::open().await;
        let space_id = scenario.space_id.clone();
        let own = scenario
            .provider
            .grants_list(
                OPERATOR,
                &grant_list_request(&space_id, Some(OPERATOR), None, None),
                now(),
            )
            .await
            .expect("operator bootstrap grant");
        assert_eq!(own.grants.len(), 1);
        let own_admin_id = grant_id(&own.grants[0]).to_owned();
        let client_grant = scenario
            .provider
            .grants_grant(
                OPERATOR,
                &grant_request(&space_id, CLIENT, &["capture"], None, None),
                now(),
            )
            .await
            .expect("a client grant");
        let other_grant = scenario
            .provider
            .grants_grant(
                OPERATOR,
                &grant_request(&space_id, OTHER, &["capture"], None, None),
                now(),
            )
            .await
            .expect("another principal's grant");
        let other_two_grant = scenario
            .provider
            .grants_grant(
                OPERATOR,
                &grant_request(&space_id, "principal-other-2", &["capture"], None, None),
                now(),
            )
            .await
            .expect("a third principal's grant");

        // A caller-owned read transaction pins one snapshot; both pages are
        // read inside it.
        let mut snapshot = scenario
            .provider
            .store()
            .pool()
            .begin()
            .await
            .expect("read snapshot");
        let first = scenario
            .provider
            .grants_list_in_tx(
                &mut snapshot,
                OPERATOR,
                &grant_list_request(&space_id, None, Some(2), None),
                now(),
            )
            .await
            .expect("first page inside the snapshot");
        assert_eq!(first.grants.len(), 2);
        let cursor = first
            .next_cursor
            .clone()
            .expect("a bounded full view issues a cursor");

        // Real committed changes land on other connections while the snapshot
        // stays open: a row change, an insert, and a revoke of the lister's own
        // admin authority. Each one bumps the provider marker.
        let revoked = scenario
            .provider
            .grants_revoke(OPERATOR, &revoke_request(&space_id, &other_grant.grant_id, 1), now())
            .await
            .expect("revoke another principal's grant");
        assert_eq!(revoked.revision, 2);
        scenario
            .provider
            .grants_grant(
                OPERATOR,
                &grant_request(&space_id, "principal-other-3", &["capture"], None, None),
                now(),
            )
            .await
            .expect("insert a grant after the snapshot");
        let own_revoked = scenario
            .provider
            .grants_revoke(OPERATOR, &revoke_request(&space_id, &own_admin_id, 1), now())
            .await
            .expect("revoke the lister's own admin grant");
        assert_eq!(own_revoked.revision, 2);

        // The second page is authorized, filtered and marked from the snapshot
        // that produced the cursor, so the committed changes cannot be paired
        // with it.
        let second = scenario
            .provider
            .grants_list_in_tx(
                &mut snapshot,
                OPERATOR,
                &grant_list_request(&space_id, None, Some(2), Some(&cursor)),
                now(),
            )
            .await
            .expect("the second page reads the page's own snapshot");
        assert_eq!(second.grants.len(), 2);
        assert!(second.next_cursor.is_none());
        let mut observed = first
            .grants
            .iter()
            .chain(second.grants.iter())
            .map(|grant| (grant_id(grant).to_owned(), grant_principal(grant).to_owned()))
            .collect::<Vec<_>>();
        observed.sort();
        let mut expected = vec![
            (own_admin_id.clone(), OPERATOR.to_owned()),
            (client_grant.grant_id.clone(), CLIENT.to_owned()),
            (other_grant.grant_id.clone(), OTHER.to_owned()),
            (other_two_grant.grant_id.clone(), "principal-other-2".to_owned()),
        ];
        expected.sort();
        assert_eq!(
            observed, expected,
            "the two pages must cover exactly the snapshot's grants: no skipped row and no post-snapshot insert"
        );
        assert!(
            second
                .grants
                .iter()
                .all(|grant| grant_state(grant) == (1, false)),
            "a row changed after the snapshot must still read at its snapshot revision"
        );
        assert!(
            second
                .grants
                .iter()
                .any(|grant| grant_principal(grant) != OPERATOR),
            "the full view authorization must come from the snapshot too"
        );
        snapshot.commit().await.expect("close the read snapshot");

        // Outside the snapshot the same cursor is rejected, and the committed
        // revoke is visible: the lister has lost admin, so the unselected list
        // degrades to its own self view.
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .grants_list(
                        OPERATOR,
                        &grant_list_request(&space_id, None, Some(2), Some(&cursor)),
                        now(),
                    )
                    .await,
            ),
            SpokeRejectCode::CursorInvalid
        );
        let after = scenario
            .provider
            .grants_list(OPERATOR, &grant_list_request(&space_id, None, None, None), now())
            .await
            .expect("post-change self view");
        assert_eq!(after.grants.len(), 1);
        assert_eq!(grant_id(&after.grants[0]), own_admin_id);
        assert_eq!(grant_state(&after.grants[0]), (2, true));
    }

    #[tokio::test]
    async fn scoped_admin_cannot_probe_foreign_grant_existence() {
        let scenario = Scenario::open().await;
        let space_id = scenario.space_id.clone();
        scenario
            .provider
            .grants_grant(
                OPERATOR,
                &grant_request(&space_id, CLIENT, &["admin"], None, Some(&["ctx-a"])),
                now(),
            )
            .await
            .expect("operator issues a ctx-a-restricted admin grant");
        let in_scope = scenario
            .provider
            .grants_grant(
                OPERATOR,
                &grant_request(&space_id, CLIENT, &["capture"], None, Some(&["ctx-a"])),
                now(),
            )
            .await
            .expect("operator issues a second in-coverage grant to the admin");
        let foreign = scenario
            .provider
            .grants_grant(
                OPERATOR,
                &grant_request(&space_id, OTHER, &["capture"], None, Some(&["ctx-b"])),
                now(),
            )
            .await
            .expect("operator issues a grant outside the admin's coverage");

        // A cursor issued before the probes proves that refused revokes leave
        // the provider state, including the snapshot marker, untouched.
        let first = scenario
            .provider
            .grants_list(
                CLIENT,
                &grant_list_request(&space_id, None, Some(1), None),
                now(),
            )
            .await
            .expect("self-view first page");
        let cursor = first
            .next_cursor
            .clone()
            .expect("a bounded self view issues a cursor");

        // An existing grant outside the caller's coverage and a missing grant
        // must be indistinguishable through code, details and message.
        let hidden = refusal(
            scenario
                .provider
                .grants_revoke(CLIENT, &revoke_request(&space_id, &foreign.grant_id, 1), now())
                .await,
        );
        let missing = refusal(
            scenario
                .provider
                .grants_revoke(
                    CLIENT,
                    &revoke_request(&space_id, "grt_00000000000000000000000000000000", 1),
                    now(),
                )
                .await,
        );
        assert_eq!(hidden.code, SpokeRejectCode::NotAuthorized);
        assert_eq!(
            (&hidden.code, &hidden.message, &hidden.details),
            (&missing.code, &missing.message, &missing.details),
            "a scoped admin must not be able to tell a foreign grant from a missing one"
        );

        // Neither probe changed the foreign grant or the snapshot marker.
        let untouched = scenario
            .provider
            .grants_list(
                OPERATOR,
                &grant_list_request(&space_id, Some(OTHER), None, None),
                now(),
            )
            .await
            .expect("operator lists the untouched foreign grant");
        assert_eq!(untouched.grants.len(), 1);
        assert_eq!(grant_state(&untouched.grants[0]), (1, false));
        let resumed = scenario
            .provider
            .grants_list(
                CLIENT,
                &grant_list_request(&space_id, None, Some(1), Some(&cursor)),
                now(),
            )
            .await
            .expect("the refused probes leave the cursor valid");
        assert_eq!(resumed.grants.len(), 1);

        // A non-admin sees the same refusal for both ids too.
        let non_admin_hidden = refusal(
            scenario
                .provider
                .grants_revoke(
                    OTHER,
                    &revoke_request(&space_id, &in_scope.grant_id, 1),
                    now(),
                )
                .await,
        );
        let non_admin_missing = refusal(
            scenario
                .provider
                .grants_revoke(
                    OTHER,
                    &revoke_request(&space_id, "grt_00000000000000000000000000000000", 1),
                    now(),
                )
                .await,
        );
        assert_eq!(
            (
                &non_admin_hidden.code,
                &non_admin_hidden.message,
                &non_admin_hidden.details
            ),
            (
                &non_admin_missing.code,
                &non_admin_missing.message,
                &non_admin_missing.details
            ),
            "a non-admin must not be able to tell a real grant id from a missing one"
        );

        // Legitimate scoped administration still works, including the
        // already-revoked no-op.
        let revoked = scenario
            .provider
            .grants_revoke(CLIENT, &revoke_request(&space_id, &in_scope.grant_id, 1), now())
            .await
            .expect("the ctx-a admin revokes its in-coverage grant");
        assert_eq!(revoked.revision, 2);
        assert!(revoked.revoked);
        let repeated = scenario
            .provider
            .grants_revoke(CLIENT, &revoke_request(&space_id, &in_scope.grant_id, 2), now())
            .await
            .expect("revoking an already revoked grant is a no-op success");
        assert_eq!(repeated.revision, 2);

        // A caller that already sees every grant keeps NOT_FOUND for a truly
        // missing id: it can list the whole set anyway, so nothing new leaks.
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .grants_revoke(
                        OPERATOR,
                        &revoke_request(
                            &space_id,
                            "grt_00000000000000000000000000000000",
                            1
                        ),
                        now(),
                    )
                    .await,
            ),
            SpokeRejectCode::NotFound
        );
    }

    #[tokio::test]
    async fn admission_instant_gates_grant_expiry_without_a_wall_clock() {
        let scenario = Scenario::open().await;
        let space_id = scenario.space_id.clone();
        let issued_at = instant("2020-01-01T00:00:00Z");
        let expires_at = instant("2020-01-01T01:00:00Z");
        scenario
            .provider
            .grants_grant(
                OPERATOR,
                &expiring_grant_request(&space_id, CLIENT, &["capture"], expires_at),
                issued_at,
            )
            .await
            .expect("operator issues an expiring grant at the supplied instant");

        let grants = scenario
            .provider
            .grants_list(OPERATOR, &grant_list_request(&space_id, Some(CLIENT), None, None), now())
            .await
            .expect("the issued grant is listed");
        assert_eq!(grants.grants.len(), 1);
        // The stored timestamps come from the supplied instant, not the clock.
        assert_eq!(grant_created_at(&grants.grants[0]), issued_at);
        assert_eq!(grant_expires_at(&grants.grants[0]), Some(expires_at));

        // The same grant admits just before the boundary...
        scenario
            .provider
            .subjects_ensure(
                CLIENT,
                &ensure_request(&space_id, "subject-one"),
                instant("2020-01-01T00:59:59Z"),
            )
            .await
            .expect("the grant admits before its expiry");
        // ...and is refused at the boundary, without any elapsed wall time.
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .subjects_ensure(CLIENT, &ensure_request(&space_id, "subject-two"), expires_at)
                    .await,
            ),
            SpokeRejectCode::NotAuthorized
        );
    }

    // -----------------------------------------------------------------------
    // Deterministic two-connection interleaving (MEM-R026)
    // -----------------------------------------------------------------------
    //
    // SQLite offers no production pause point between the statements of these
    // two reads, so the fixture supplies its own. It is test-only and lives on
    // the reader's own pooled connections: a TEMP view named `grants` shadows
    // the real table there, and its `grant_id` column carries a custom
    // collation. Registering that collation is the safe
    // `LockedSqliteHandle::create_collation` seam, and it returns exactly the
    // byte ordering the read would have used anyway, so the paused comparison
    // is the actual native read and not a copy, a fake row or a scheduler
    // assumption. The writer is a separate real WAL provider over the same
    // file: while the reader is held inside the grants statement, that second
    // connection commits the authorization/state transition.

    const READ_MONITOR_COLLATION: &str = "fixture_read_monitor";
    const READER_CONNECTIONS: usize = 5;
    const PENDING_CAPTURE_CAPABILITIES: &[&str] = &["capture"];
    /// Every `load_grant_rows` column, in the table's own order. Only
    /// `grant_id` is collated, so the outer `ORDER BY grant_id` resolves to
    /// the monitor while every other comparison stays BINARY.
    const READER_GRANTS_VIEW: &str = "CREATE TEMP VIEW grants AS \
        SELECT grant_id COLLATE fixture_read_monitor AS grant_id, space_id, subject_id, \
        principal_id, capabilities_json, context_ids_json, shared_access, expires_at, \
        revoked_at, revision, created_at, extensions_json FROM main.grants";

    /// A one-shot native comparison gate. The first `grant_id` comparison in
    /// the grants statement signals the test and then holds that SQLite worker
    /// thread until the writer's commit is durable; every later comparison is
    /// passed through untouched.
    struct ReadMonitor {
        fired: AtomicBool,
        reached: Mutex<Option<mpsc::Sender<()>>>,
        release: Mutex<mpsc::Receiver<()>>,
    }

    impl ReadMonitor {
        fn compare(&self, left: &str, right: &str) -> std::cmp::Ordering {
            if !self.fired.swap(true, std::sync::atomic::Ordering::SeqCst) {
                if let Some(sender) = self.reached.lock().expect("monitor signal").take() {
                    let _ = sender.send(());
                }
                let _ = self.release.lock().expect("monitor release").recv();
            }
            left.cmp(right)
        }
    }

    fn read_monitor() -> (Arc<ReadMonitor>, mpsc::Receiver<()>, mpsc::Sender<()>) {
        let (reached_tx, reached_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        (
            Arc::new(ReadMonitor {
                fired: AtomicBool::new(false),
                reached: Mutex::new(Some(reached_tx)),
                release: Mutex::new(release_rx),
            }),
            reached_rx,
            release_tx,
        )
    }

    /// Install the monitor on every physical connection the reader's pool can
    /// hand the read, so whichever connection `begin()` consumes is the
    /// instrumented one.
    async fn instrument_reader(provider: &FixtureMemoryProvider, monitor: &Arc<ReadMonitor>) {
        let mut connections = Vec::with_capacity(READER_CONNECTIONS);
        for _ in 0..READER_CONNECTIONS {
            connections.push(
                provider
                    .store()
                    .pool()
                    .acquire()
                    .await
                    .expect("reader connection"),
            );
        }
        for connection in &mut connections {
            let comparison = Arc::clone(monitor);
            let mut handle = (&mut **connection)
                .lock_handle()
                .await
                .expect("lock the reader's actual connection");
            handle
                .create_collation(READ_MONITOR_COLLATION, move |left, right| {
                    comparison.compare(left, right)
                })
                .expect("install the read monitor collation");
            drop(handle);
            sqlx::query(READER_GRANTS_VIEW)
                .execute(&mut **connection)
                .await
                .expect("install the reader grants view");
        }
    }

    async fn await_reader_reached(reached: mpsc::Receiver<()>) -> bool {
        tokio::task::spawn_blocking(move || reached.recv_timeout(Duration::from_secs(10)).is_ok())
            .await
            .expect("reader monitor wait")
    }

    /// Stage a genuine async `memory.capture` receipt in the pending state on
    /// the caller-owned writer connection. The producer itself commits this
    /// state when an admitted capture is executed asynchronously, so the page's
    /// drain barrier counts a real receipt, not a fabricated row.
    async fn stage_pending_capture(
        conn: &mut SqliteConnection,
        space_id: &str,
        subject_id: &str,
        receipt_id: &str,
        idempotency_key: &str,
    ) -> Result<(), String> {
        let admitted = admit_capture(json!({
            "space_id": space_id,
            "subject_id": subject_id,
            "content": { "text": "pending receipt probe" },
            "capture_scope": { "visibility": "shared" },
            "kind_hint": "fact",
            "idempotency_key": idempotency_key,
        }))
        .map_err(|error| format!("pending capture admission: {error:?}"))?;
        let mutation = Mutation::keyed("memory.capture", idempotency_key, &admitted);
        let target = ReceiptTarget::new(
            PENDING_CAPTURE_CAPABILITIES,
            Some(subject_id),
            &[],
            true,
            false,
        )
        .map_err(|error| format!("pending receipt target: {error:?}"))?;
        stage_receipt_in_tx(
            conn,
            space_id,
            CLIENT,
            &mutation,
            receipt_id,
            &target,
            ReceiptState::Pending,
            now(),
        )
        .await
        .map_err(|error| format!("pending receipt staging: {error:?}"))?;
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn spaces_list_authorization_and_drain_barrier_share_one_snapshot_under_interleave() {
        let scenario = Scenario::open().await;
        let space_id = scenario.space_id.clone();
        let subject_id = "subject-interleaved";
        scenario
            .provider
            .subjects_ensure(OPERATOR, &ensure_request(&space_id, subject_id), now())
            .await
            .expect("subject");
        let client_grant = scenario
            .provider
            .grants_grant(
                OPERATOR,
                &grant_request(&space_id, CLIENT, &["capture"], None, None),
                now(),
            )
            .await
            .expect("client grant");
        // The second provider is a real WAL writer on the same file with its
        // own pool, so it never sees the reader's TEMP view or collation.
        let writer = FixtureMemoryProvider::open(&scenario.path)
            .await
            .expect("second real WAL provider");
        let (monitor, reached, release) = read_monitor();
        instrument_reader(&scenario.provider, &monitor).await;

        let request = spaces_list_request(None, None);
        let read = scenario.provider.spaces_list(CLIENT, &request, now());
        tokio::pin!(read);
        let early = tokio::select! {
            reached = await_reader_reached(reached) => {
                assert!(reached, "the grants read never reached its native comparison");
                None
            }
            value = &mut read => Some(value),
        };
        if let Some(value) = early {
            panic!("spaces.list finished before the interleave point: {value:?}");
        }

        // One atomic producer transition: the pending receipt, the caller's
        // revoke and the freeze commit together. No committed snapshot ever
        // combines the old active status with the new pending count.
        let transition: Result<(), String> = async {
            let mut transaction = writer
                .store()
                .pool()
                .begin_with("BEGIN IMMEDIATE")
                .await
                .map_err(|error| error.to_string())?;
            stage_pending_capture(
                &mut transaction,
                &space_id,
                subject_id,
                "rcpt_interleave_probe",
                "pending-interleave",
            )
            .await?;
            writer
                .grants_revoke_in_tx(
                    &mut transaction,
                    OPERATOR,
                    &revoke_request(&space_id, &client_grant.grant_id, 1),
                    now(),
                )
                .await
                .map_err(|error| format!("{error:?}"))?;
            writer
                .spaces_freeze_in_tx(&mut transaction, OPERATOR, &freeze_request(&space_id, 1), now())
                .await
                .map_err(|error| format!("{error:?}"))?;
            transaction.commit().await.map_err(|error| error.to_string())
        }
        .await;
        release.send(()).expect("release the paused read");
        let page = read.await.expect("spaces.list completes");
        transition.expect("the interleaved commit must succeed");

        // The returned page is wholly the pre-transition snapshot: the caller
        // was still authorized, the space was still active and the drain
        // barrier had not yet counted the pending receipt.
        let space = visible_space(&page);
        let wire = serde_json::to_value(space).expect("space wire value");
        assert_eq!(wire["status"], json!("active"), "the page must carry its snapshot's status");
        assert_eq!(space.revision.get(), 1, "the page must carry its snapshot's revision");
        assert_eq!(
            space.pending_receipts, 0,
            "a pending receipt committed after the snapshot must not be paired with the snapshot's authorization"
        );

        // The same atomic transition is visible afterwards, so the assertion
        // above distinguishes the mixed combination from a valid old version.
        let after = scenario
            .provider
            .spaces_list(OPERATOR, &spaces_list_request(None, None), now())
            .await
            .expect("post-transition list");
        let after = visible_space(&after);
        assert_eq!(
            serde_json::to_value(after).expect("space wire value")["status"],
            json!("frozen")
        );
        assert_eq!(after.revision.get(), 2);
        assert_eq!(after.pending_receipts, 1);
        assert!(
            scenario
                .provider
                .spaces_list(CLIENT, &spaces_list_request(None, None), now())
                .await
                .expect("revoked caller list")
                .spaces
                .is_empty(),
            "the revoked caller has no post-transition authority"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn subjects_get_authorization_and_subject_state_share_one_snapshot_under_interleave() {
        let scenario = Scenario::open().await;
        let space_id = scenario.space_id.clone();
        let subject_id = "subject-interleaved";
        scenario
            .provider
            .subjects_ensure(OPERATOR, &ensure_request(&space_id, subject_id), now())
            .await
            .expect("subject");
        let client_grant = scenario
            .provider
            .grants_grant(
                OPERATOR,
                &grant_request(&space_id, CLIENT, &["capture"], None, None),
                now(),
            )
            .await
            .expect("client grant");
        let writer = FixtureMemoryProvider::open(&scenario.path)
            .await
            .expect("second real WAL provider");
        let (monitor, reached, release) = read_monitor();
        instrument_reader(&scenario.provider, &monitor).await;

        let request = get_request(&space_id, subject_id);
        let read = scenario.provider.subjects_get(CLIENT, &request, now());
        tokio::pin!(read);
        let early = tokio::select! {
            reached = await_reader_reached(reached) => {
                assert!(reached, "the grants read never reached its native comparison");
                None
            }
            value = &mut read => Some(value),
        };
        if let Some(value) = early {
            panic!("subjects.get finished before the interleave point: {value:?}");
        }

        // Revoke followed by a subject state change, committed atomically on
        // the second connection: no snapshot ever pairs the old grant with the
        // fenced subject.
        let transition: Result<(), String> = async {
            let mut transaction = writer
                .store()
                .pool()
                .begin_with("BEGIN IMMEDIATE")
                .await
                .map_err(|error| error.to_string())?;
            writer
                .grants_revoke_in_tx(
                    &mut transaction,
                    OPERATOR,
                    &revoke_request(&space_id, &client_grant.grant_id, 1),
                    now(),
                )
                .await
                .map_err(|error| format!("{error:?}"))?;
            writer
                .subjects_fence_in_tx(
                    &mut transaction,
                    OPERATOR,
                    &fence_request(&space_id, subject_id, "fence", without_subject(), 1),
                    now(),
                )
                .await
                .map_err(|error| format!("{error:?}"))?;
            transaction.commit().await.map_err(|error| error.to_string())
        }
        .await;
        release.send(()).expect("release the paused read");
        let subject = read.await.expect("subjects.get completes");
        transition.expect("the interleaved commit must succeed");

        assert_eq!(
            serde_json::to_value(&subject).expect("subject wire value")["status"],
            json!("active"),
            "the subject body and the authorization must come from one snapshot"
        );
        assert_eq!(subject.revision.get(), 1);

        // Afterwards the same atomic transition is visible: the subject is
        // fenced and the revoked caller is no longer authorized.
        let after = scenario
            .provider
            .subjects_get(OPERATOR, &get_request(&space_id, subject_id), now())
            .await
            .expect("operator reads the fenced subject");
        assert_eq!(
            serde_json::to_value(&after).expect("subject wire value")["status"],
            json!("fenced")
        );
        assert_eq!(after.revision.get(), 2);
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .subjects_get(CLIENT, &get_request(&space_id, subject_id), now())
                    .await,
            ),
            SpokeRejectCode::NotAuthorized
        );
    }
}
