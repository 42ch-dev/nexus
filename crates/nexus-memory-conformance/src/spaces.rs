//! Spaces, subjects, grants and the shared authorization seam
//! (spoke-memory §§6.2–6.5, 7.2–7.10, 8.2, 8.3).
//!
//! Every operation takes the authenticated principal id from the caller's
//! admission layer; payload principal fields only select grant targets
//! (MEM-R003). Mutations run inside one `BEGIN IMMEDIATE` transaction, so the
//! CAS precondition, the state transition and the freeze/fence admission gate
//! commit together (MEM-R017/R024).
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
async fn load_grants(
    conn: &mut SqliteConnection,
    space_id: &str,
) -> Result<Vec<MemoryGrant>, SpokeReject> {
    load_grant_rows(conn, space_id)
        .await?
        .iter()
        .map(grant_to_wire)
        .collect()
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
    fn require_served_space(&self, requested: &str) -> Result<(), SpokeReject> {
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
    pub(crate) async fn spaces_list(
        &self,
        principal_id: &str,
        request: &MemorySpacesListRequest,
    ) -> Result<SpacePage, SpokeReject> {
        checked(validate_memory_spaces_list(request, MAX_PAGE_SIZE))?;
        if request.cursor.is_some() {
            // One space never needs a second page, so no cursor is ever
            // issued for this op: a presented one cannot be valid (§7.0.7).
            return Err(cursor_invalid(
                "memory.spaces.list serves one space and issues no cursor",
            ));
        }
        let mut conn = self.store().pool().acquire().await.map_err(internal)?;
        let space = fetch_space(&mut conn, &self.identity.space_id).await?;
        let now = Utc::now();
        let visible = load_grant_rows(&mut conn, &space.space_id)
            .await?
            .iter()
            .any(|grant| grant_is_valid(grant, principal_id, &space.space_id, now));
        let spaces = if visible {
            let pending = pending_receipts(&mut conn, &space.space_id).await?;
            vec![space_to_wire(&space, pending)?]
        } else {
            Vec::new()
        };
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
    ) -> Result<SpaceChange, SpokeReject> {
        self.set_space_status(
            principal_id,
            &request.space_id,
            request.expected_revision,
            "frozen",
        )
        .await
    }

    /// Reverse of [`Self::spaces_freeze`]; always admissible on a frozen space
    /// (maintenance allowlist) and a no-op success on an active one.
    pub(crate) async fn spaces_unfreeze(
        &self,
        principal_id: &str,
        request: &MemorySpacesUnfreezeRequest,
    ) -> Result<SpaceChange, SpokeReject> {
        self.set_space_status(
            principal_id,
            &request.space_id,
            request.expected_revision,
            "active",
        )
        .await
    }

    async fn set_space_status(
        &self,
        principal_id: &str,
        space_id: &str,
        expected_revision: NonZeroU64,
        target_status: &str,
    ) -> Result<SpaceChange, SpokeReject> {
        self.require_served_space(space_id)?;
        let mut transaction = self
            .store()
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(internal)?;
        let space = fetch_space(&mut transaction, space_id).await?;
        let now = Utc::now();
        let grants = load_grants(&mut transaction, space_id).await?;
        require_authorized(
            &grants,
            principal_id,
            space_id,
            now,
            &requirement("admin", None),
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
            .execute(&mut *transaction)
            .await
            .map_err(internal)?;
            space.revision + 1
        };
        let pending = pending_receipts(&mut transaction, space_id).await?;
        transaction.commit().await.map_err(internal)?;
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
    ) -> Result<SubjectChange, SpokeReject> {
        self.require_served_space(&request.space_id)?;
        let mut transaction = self
            .store()
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(internal)?;
        let space = fetch_space(&mut transaction, &request.space_id).await?;
        require_admitted("memory.subjects.ensure", &space)?;
        let grants = load_grants(&mut transaction, &request.space_id).await?;
        require_authorized(
            &grants,
            principal_id,
            &request.space_id,
            Utc::now(),
            &requirement("capture", Some(request.subject_id.as_str())),
        )?;
        let (kind_ns, kind_id) = request.kind.as_ref().map_or((None, None), |kind| {
            (Some(kind.ns.as_str()), Some(kind.id.as_str()))
        });
        let created = sqlx::query(
            "INSERT INTO subjects (space_id, subject_id, kind_ns, kind_id, status, \
             fenced_contexts_json, revision, created_at) \
             VALUES (?, ?, ?, ?, 'active', '[]', 1, ?) \
             ON CONFLICT (space_id, subject_id) DO NOTHING",
        )
        .bind(request.space_id.as_str())
        .bind(request.subject_id.as_str())
        .bind(kind_ns)
        .bind(kind_id)
        .bind(timestamp_text(Utc::now()))
        .execute(&mut *transaction)
        .await
        .map_err(|error| match error.as_database_error() {
            Some(database) if database.is_unique_violation() => invalid_input(
                "kind",
                "a subject with this kind already exists in the space",
            ),
            _ => internal(error),
        })?;
        let subject = fetch_subject(&mut transaction, &request.space_id, &request.subject_id).await?;
        // §7.5 lists SUBJECT_FENCED for ensure: a fenced or archived subject
        // refuses the write path that ensure fronts.
        if subject.status != "active" {
            return Err(subject_fenced(&subject.subject_id, "subject"));
        }
        transaction.commit().await.map_err(internal)?;
        Ok(SubjectChange {
            subject_id: subject.subject_id,
            created: created.rows_affected() == 1,
            status: subject.status,
            revision: subject.revision,
        })
    }

    /// Read one subject under current authorization (MEM-R026).
    pub(crate) async fn subjects_get(
        &self,
        principal_id: &str,
        request: &MemorySubjectsGetRequest,
    ) -> Result<Subject, SpokeReject> {
        self.require_served_space(&request.space_id)?;
        let mut conn = self.store().pool().acquire().await.map_err(internal)?;
        let space = fetch_space(&mut conn, &request.space_id).await?;
        require_admitted("memory.subjects.get", &space)?;
        let grants = load_grant_rows(&mut conn, &request.space_id).await?;
        if !any_grant_covers_subject(
            &grants,
            principal_id,
            &request.space_id,
            request.subject_id.as_str(),
            Utc::now(),
        ) {
            return Err(not_authorized(
                "the authenticated principal holds no grant on this subject",
            ));
        }
        let subject = fetch_subject(&mut conn, &request.space_id, &request.subject_id).await?;
        subject_to_wire(&subject)
    }

    /// Provider-enforced subject or placement-context fencing (§7.7).
    pub(crate) async fn subjects_fence(
        &self,
        principal_id: &str,
        request: &MemorySubjectsFenceRequest,
    ) -> Result<SubjectFenceChange, SpokeReject> {
        use spoke_operations::spoke_schemas::memory_subjects_fence_request::{Action, Target};

        checked(validate_memory_subjects_fence(request))?;
        self.require_served_space(&request.space_id)?;
        let mut transaction = self
            .store()
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(internal)?;
        let space = fetch_space(&mut transaction, &request.space_id).await?;
        require_admitted("memory.subjects.fence", &space)?;
        let grants = load_grants(&mut transaction, &request.space_id).await?;
        require_authorized(
            &grants,
            principal_id,
            &request.space_id,
            Utc::now(),
            &requirement("admin", Some(request.subject_id.as_str())),
        )?;
        let subject = fetch_subject(&mut transaction, &request.space_id, &request.subject_id).await?;
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
        .execute(&mut *transaction)
        .await
        .map_err(internal)?;
        transaction.commit().await.map_err(internal)?;
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
    /// other principal sees exactly its own grants.
    pub(crate) async fn grants_list(
        &self,
        principal_id: &str,
        request: &MemoryGrantsListRequest,
    ) -> Result<GrantPage, SpokeReject> {
        checked(validate_memory_grants_list(request, MAX_PAGE_SIZE))?;
        self.require_served_space(&request.space_id)?;
        let mut conn = self.store().pool().acquire().await.map_err(internal)?;
        let space = fetch_space(&mut conn, &request.space_id).await?;
        require_admitted("memory.grants.list", &space)?;
        let now = Utc::now();
        let rows = load_grant_rows(&mut conn, &request.space_id).await?;
        let grants = rows
            .iter()
            .map(grant_to_wire)
            .collect::<Result<Vec<_>, _>>()?;
        let admin = is_authorized(
            &grants,
            principal_id,
            &request.space_id,
            now,
            &requirement("admin", None),
        );
        let target_principal = request.principal_id.as_ref().map(|id| id.as_str());
        if !admin && target_principal.is_some_and(|target| target != principal_id) {
            return Err(not_authorized(
                "only an admin grant may list another principal's grants",
            ));
        }
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

        let state = filtered
            .iter()
            .map(|grant| json!([grant.grant_id, grant.revision]))
            .collect::<Vec<_>>();
        let binding_digest = match jcs_sha256_hex(&json!({
            "principal": principal_id,
            "space": request.space_id.as_str(),
            "target": scoped_principal,
            "subject": subject_filter,
            "state": state,
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
                        "cursor was issued for a different query or the grant set changed",
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
        let mut transaction = self
            .store()
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(internal)?;
        let space = fetch_space(&mut transaction, &space_id).await?;
        require_admitted("memory.grants.grant", &space)?;
        let now = Utc::now();
        let granter_grants = load_grants(&mut transaction, &space_id).await?;
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
            let subject = fetch_subject(&mut transaction, &space_id, subject_id).await?;
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
        .execute(&mut *transaction)
        .await
        .map_err(internal)?;
        transaction.commit().await.map_err(internal)?;
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
    ) -> Result<GrantRevoked, SpokeReject> {
        self.require_served_space(&request.space_id)?;
        let mut transaction = self
            .store()
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(internal)?;
        let space = fetch_space(&mut transaction, &request.space_id).await?;
        require_admitted("memory.grants.revoke", &space)?;
        let now = Utc::now();
        let grants = load_grant_rows(&mut transaction, &request.space_id).await?;
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
        let target = grants
            .iter()
            .find(|grant| grant.grant_id == request.grant_id.as_str())
            .ok_or_else(|| not_found(format!("grant {} does not exist", request.grant_id.as_str())))?;
        // Second stage: the admin coverage must reach the target's scope.
        let grants_wire = grants
            .iter()
            .map(grant_to_wire)
            .collect::<Result<Vec<_>, _>>()?;
        require_authorized(
            &grants_wire,
            principal_id,
            &request.space_id,
            now,
            &requirement("admin", target.subject_id.as_deref()),
        )?;
        require_revision(request.expected_revision, target.revision)?;
        if target.revoked_at.is_some() {
            // §7.0.4: revoking an already revoked grant is a no-op success.
            transaction.commit().await.map_err(internal)?;
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
        .execute(&mut *transaction)
        .await
        .map_err(internal)?;
        transaction.commit().await.map_err(internal)?;
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
    use std::path::PathBuf;
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

    fn visible_space(page: &SpacePage) -> &MemorySpace {
        assert_eq!(page.spaces.len(), 1, "the fixture serves exactly one space");
        &page.spaces[0]
    }

    #[tokio::test]
    async fn space_identity_and_generation_are_stable_across_reopen() {
        let scenario = Scenario::open().await;
        let space_id = scenario.space_id.clone();
        let generation = scenario.provider.identity().authority_generation;

        let page = scenario
            .provider
            .spaces_list(OPERATOR, &spaces_list_request(None, None))
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
            .spaces_list(OTHER, &spaces_list_request(None, None))
            .await
            .expect("unrelated principal lists spaces");
        assert!(hidden.spaces.is_empty());

        // Any valid grant makes the space visible (§7.2).
        scenario
            .provider
            .grants_grant(
                OPERATOR,
                &grant_request(&space_id, CLIENT, &["capture"], None, None),
            )
            .await
            .expect("operator issues capture grant");
        let visible = scenario
            .provider
            .spaces_list(CLIENT, &spaces_list_request(Some(10), None))
            .await
            .expect("grantee lists spaces");
        assert_eq!(visible_space(&visible).space_id.as_str(), space_id);

        let reopened = scenario.reopen().await;
        let page = reopened
            .provider
            .spaces_list(OPERATOR, &spaces_list_request(None, None))
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
            )
            .await
            .expect("operator issues capture grant");
        scenario
            .provider
            .subjects_ensure(CLIENT, &ensure_request(&space_id, "subject-one"))
            .await
            .expect("client ensures a subject before the freeze");

        let frozen = scenario
            .provider
            .spaces_freeze(OPERATOR, &freeze_request(&space_id, 1))
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
                    .subjects_ensure(CLIENT, &ensure_request(&space_id, "subject-two"))
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
                    )
                    .await,
            ),
            SpokeRejectCode::SpaceFrozen
        );

        // Reads stay admitted and report the frozen status.
        let page = scenario
            .provider
            .spaces_list(OPERATOR, &spaces_list_request(None, None))
            .await
            .expect("spaces.list is allowlisted while frozen");
        assert_eq!(visible_space(&page).status.to_string(), "frozen");
        scenario
            .provider
            .subjects_get(OPERATOR, &get_request(&space_id, "subject-one"))
            .await
            .expect("subjects.get is allowlisted while frozen");
        scenario
            .provider
            .grants_list(OPERATOR, &grant_list_request(&space_id, None, None, None))
            .await
            .expect("grants.list is allowlisted while frozen");

        // Revoke is the one mutation on the allowlist (§8.2).
        let revoked = scenario
            .provider
            .grants_revoke(OPERATOR, &revoke_request(&space_id, &issued.grant_id, 1))
            .await
            .expect("revoke is allowlisted while frozen");
        assert!(revoked.revoked);
        assert_eq!(revoked.revision, 2);

        // Freezing an already frozen space with the matching revision is a
        // no-op success and does not bump the revision (§7.0.4).
        let no_op = scenario
            .provider
            .spaces_freeze(OPERATOR, &freeze_request(&space_id, 2))
            .await
            .expect("repeat freeze is a no-op success");
        assert_eq!(no_op.revision, 2);

        let thawed = scenario
            .provider
            .spaces_unfreeze(OPERATOR, &unfreeze_request(&space_id, 2))
            .await
            .expect("unfreeze is allowlisted");
        assert_eq!(thawed.status, "active");
        assert_eq!(thawed.revision, 3);

        scenario
            .provider
            .subjects_ensure(OPERATOR, &ensure_request(&space_id, "subject-two"))
            .await
            .expect("writes are admitted again after unfreeze");
    }

    #[tokio::test]
    async fn stale_revision_cas_rejects_fence_and_freeze_lost_updates() {
        let scenario = Scenario::open().await;
        let space_id = scenario.space_id.clone();
        scenario
            .provider
            .spaces_freeze(OPERATOR, &freeze_request(&space_id, 1))
            .await
            .expect("freeze at revision 1");
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .spaces_freeze(OPERATOR, &freeze_request(&space_id, 1))
                    .await,
            ),
            SpokeRejectCode::RevisionConflict
        );
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .spaces_unfreeze(OPERATOR, &unfreeze_request(&space_id, 1))
                    .await,
            ),
            SpokeRejectCode::RevisionConflict
        );
        scenario
            .provider
            .spaces_unfreeze(OPERATOR, &unfreeze_request(&space_id, 2))
            .await
            .expect("unfreeze at the current revision");

        scenario
            .provider
            .subjects_ensure(OPERATOR, &ensure_request(&space_id, "subject-one"))
            .await
            .expect("ensure subject");
        scenario
            .provider
            .subjects_fence(
                OPERATOR,
                &fence_request(&space_id, "subject-one", "fence", without_subject(), 1),
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
            )
            .await
            .expect("operator issues capture grant");

        scenario
            .provider
            .subjects_ensure(CLIENT, &ensure_request(&space_id, "subject-one"))
            .await
            .expect("client ensures subject-one");
        scenario
            .provider
            .subjects_fence(
                OPERATOR,
                &fence_request(&space_id, "subject-one", "fence", without_subject(), 1),
            )
            .await
            .expect("operator fences subject-one");

        // The other client is fenced by provider state, not by process state.
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .subjects_ensure(CLIENT, &ensure_request(&space_id, "subject-one"))
                    .await,
            ),
            SpokeRejectCode::SubjectFenced
        );
        // Reads remain admitted on a fenced subject.
        let fenced = scenario
            .provider
            .subjects_get(CLIENT, &get_request(&space_id, "subject-one"))
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
            )
            .await
            .expect("context fence on a subject-fenced subject");
        assert_eq!(context_fenced.status, "fenced");
        assert_eq!(context_fenced.fenced_contexts, vec!["ctx-x".to_owned()]);
        assert_eq!(context_fenced.revision, 3);

        // A sibling subject is untouched (MEM-R024 has no collateral effect).
        scenario
            .provider
            .subjects_ensure(CLIENT, &ensure_request(&space_id, "subject-two"))
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
            )
            .await
            .expect("operator issues capture grant");
        scenario
            .provider
            .subjects_ensure(CLIENT, &ensure_request(&space_id, "subject-one"))
            .await
            .expect("grant works before the revoke");

        let revoked = scenario
            .provider
            .grants_revoke(OPERATOR, &revoke_request(&space_id, &issued.grant_id, 1))
            .await
            .expect("operator revokes the grant");
        assert!(revoked.revoked);
        assert_eq!(revoked.revision, 2);

        // The next admission is refused; the previous call is not re-evaluated.
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .subjects_ensure(CLIENT, &ensure_request(&space_id, "subject-two"))
                    .await,
            ),
            SpokeRejectCode::NotAuthorized
        );
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .subjects_get(CLIENT, &get_request(&space_id, "subject-one"))
                    .await,
            ),
            SpokeRejectCode::NotAuthorized
        );

        // The revoked grant is still visible to its holder, marked revoked.
        let page = scenario
            .provider
            .grants_list(CLIENT, &grant_list_request(&space_id, None, None, None))
            .await
            .expect("holders keep their own grant view");
        assert_eq!(page.grants.len(), 1);
        let held = &page.grants[0];
        match held {
            MemoryGrant::UnrestrictedGrant(grant) => {
                assert!(grant.revoked_at.is_some());
                assert_eq!(grant.revision.get(), 2);
            }
            other => panic!("expected an unrestricted grant, got {other:?}"),
        }

        // Stale and repeated revokes follow the CAS and no-op rules.
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .grants_revoke(OPERATOR, &revoke_request(&space_id, &issued.grant_id, 1))
                    .await,
            ),
            SpokeRejectCode::RevisionConflict
        );
        let repeated = scenario
            .provider
            .grants_revoke(OPERATOR, &revoke_request(&space_id, &issued.grant_id, 2))
            .await
            .expect("re-revoking at the current revision is a no-op success");
        assert_eq!(repeated.revision, 2);

        // A non-admin cannot probe grant ids for existence.
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .grants_revoke(CLIENT, &revoke_request(&space_id, "grt_missing", 1))
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
            )
            .await
            .expect("issuance inside the granter coverage");
        assert_eq!(issued.revision, 1);
        assert!(issued.grant_id.starts_with("grt_"));

        // A capture-only principal can never issue.
        let capture_only = Scenario::open().await;
        capture_only
            .provider
            .grants_grant(
                OPERATOR,
                &grant_request(&capture_only.space_id, CLIENT, &["capture"], None, None),
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
                    )
                    .await,
            ),
            SpokeRejectCode::NotAuthorized
        );

        // Subject-scope amplification: a subject admin cannot issue space-wide.
        scenario
            .provider
            .subjects_ensure(OPERATOR, &ensure_request(&space_id, "subject-one"))
            .await
            .expect("ensure subject-one");
        scenario
            .provider
            .grants_grant(
                OPERATOR,
                &grant_request(&space_id, CLIENT, &["admin"], Some("subject-one"), None),
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
            .subjects_ensure(OPERATOR, &ensure_request(&space_id, "subject-one"))
            .await
            .expect("operator ensures subject-one");
        scenario
            .provider
            .grants_grant(
                OPERATOR,
                &grant_request(&space_id, CLIENT, &["capture"], Some("subject-one"), None),
            )
            .await
            .expect("operator issues a subject-scoped capture grant");
        scenario
            .provider
            .subjects_ensure(CLIENT, &ensure_request(&space_id, "subject-one"))
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
            )
            .await
            .expect("fence a context");
        scenario
            .provider
            .spaces_freeze(OPERATOR, &freeze_request(&space_id, 1))
            .await
            .expect("freeze the space");

        let reopened = scenario.reopen().await;
        let page = reopened
            .provider
            .spaces_list(OPERATOR, &spaces_list_request(None, None))
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
            .subjects_get(OPERATOR, &get_request(&space_id, "subject-one"))
            .await
            .expect("subject survives the reopen");
        assert_eq!(subject.status.to_string(), "active");
        assert_eq!(subject.fenced_contexts, vec!["ctx-a".to_owned()]);
        assert_eq!(subject.revision.get(), 2);

        // The persisted grant still admits the client, and the persisted freeze
        // still rejects writes.
        let grants = reopened
            .provider
            .grants_list(OPERATOR, &grant_list_request(&space_id, Some(CLIENT), None, None))
            .await
            .expect("grants survive the reopen");
        assert_eq!(grants.grants.len(), 1);
        assert_eq!(
            reject_code(
                reopened
                    .provider
                    .subjects_ensure(CLIENT, &ensure_request(&space_id, "subject-two"))
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
            .subjects_ensure(OPERATOR, &ensure_request(&space_id, "subject-one"))
            .await
            .expect("ensure subject-one");

        let archived = scenario
            .provider
            .subjects_fence(
                OPERATOR,
                &fence_request(&space_id, "subject-one", "archive", without_subject(), 1),
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
                    .subjects_ensure(OPERATOR, &ensure_request(&space_id, "subject-one"))
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
                    )
                    .await
                    .expect("issue a grant per grantee"),
            );
        }

        let first = scenario
            .provider
            .grants_list(OPERATOR, &grant_list_request(&space_id, None, Some(2), None))
            .await
            .expect("first page");
        assert_eq!(first.grants.len(), 2);
        let cursor = first.next_cursor.expect("a bounded page issues a cursor");

        let second = scenario
            .provider
            .grants_list(OPERATOR, &grant_list_request(&space_id, None, Some(2), Some(&cursor)))
            .await
            .expect("second page");
        assert_eq!(second.grants.len(), 2);
        assert!(second.next_cursor.is_none());

        // The admin target selector is admin-only.
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .grants_list(CLIENT, &grant_list_request(&space_id, Some(OTHER), None, None))
                    .await,
            ),
            SpokeRejectCode::NotAuthorized
        );
        // A non-admin self view never sees other principals' grants.
        let self_view = scenario
            .provider
            .grants_list(CLIENT, &grant_list_request(&space_id, None, None, None))
            .await
            .expect("self view");
        assert!(self_view
            .grants
            .iter()
            .all(|grant| grant_principal(grant) == CLIENT));

        // Any grant change invalidates an outstanding cursor (MEM-R026).
        scenario
            .provider
            .grants_revoke(OPERATOR, &revoke_request(&space_id, &issued[0].grant_id, 1))
            .await
            .expect("revoke one grant");
        assert_eq!(
            reject_code(
                scenario
                    .provider
                    .grants_list(
                        OPERATOR,
                        &grant_list_request(&space_id, None, Some(2), Some(&cursor)),
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
}
