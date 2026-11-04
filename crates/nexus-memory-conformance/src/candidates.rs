//! Capture, candidates, receipts and idempotency (spoke-memory §§6.7, 6.10,
//! 7.0.5, 7.11–7.15, 7.28, 8.1, 8.5).
//!
//! Every mutating op is split into an outer convenience wrapper that owns one
//! `BEGIN IMMEDIATE` transaction and an `*_in_tx` sibling that stages the
//! authorized effect, the content-free receipt body and its dedup row on the
//! caller's connection without committing, so the assembly task can commit an
//! effect and its receipt exactly once (MEM-R009/R022).
//!
//! Replay is resolved inside that same transaction *after* current
//! authorization but *before* the freeze/fence/CAS gates: an identical admitted
//! replay returns the stored receipt without re-executing and without
//! re-evaluating superseded preconditions (MEM-R021), while a revoked principal
//! can neither replay nor read it (§7.28/C-104).
//!
//! Receipt bodies and dedup rows are the only place this slice records op
//! outcomes, and they stay content-free (MEM-R022): results carry ids, counts
//! and outcome enums, the dedup row carries the JCS payload digest, and a
//! consumed candidate keeps only its `content_digest` (§6.7).

use std::num::NonZeroU64;

use chrono::{DateTime, Duration, SecondsFormat, Utc};
use serde_json::{json, Map, Value};
use sqlx::{Row, SqliteConnection};
use sqlx::sqlite::SqliteRow;
use uuid::Uuid;

use spoke_operations::spoke_schemas::memory_candidates_list_request::Status as CandidateStatus;
use spoke_operations::spoke_schemas::memory_candidates_review_request::Decision;
use spoke_operations::spoke_schemas::memory_capture_request::{
    Content as CaptureContent, PlacementScope as CapturePlacement,
};
use spoke_operations::spoke_schemas::{
    MemoryCandidate, MemoryCandidatesDismissRequest, MemoryCandidatesGetRequest,
    MemoryCandidatesListRequest, MemoryCandidatesReviewRequest, MemoryCaptureRequest, MemoryGrant,
    MemoryReceipt, MemoryReceiptsGetRequest,
};
use spoke_operations::{
    grant_covers, is_freeze_allowlisted, jcs_sha256_hex, validate_memory_candidates_list,
    validate_memory_candidates_review, validate_memory_capture, GrantRequirement, SpokeReject,
    SpokeRejectCode, SpokeResult,
};

use crate::provider::FixtureMemoryProvider;
use crate::spaces::{
    bump_snapshot, internal, is_authorized, load_grants, requirement, snapshot_marker,
    whole_scope_requirement, ENGINE_POLICY_VERSION, MAX_PAGE_SIZE,
};

/// Wire schema version carried by every fixture-served object (§6.1).
const SCHEMA_VERSION: u64 = 1;

/// `memory.describe.limits.max_content_bytes` for the fixture (§7.1); the
/// assembly advertises this same value.
pub(crate) const MAX_CONTENT_BYTES: usize = 64 * 1024;

/// Provider candidate retention: a pending candidate stops being listed after
/// this window without being consumed (§6.7 `expires_at`, §8.1).
const CANDIDATE_TTL_DAYS: i64 = 30;

/// Advisory kind recorded when the request carries no `kind_hint` (§6.7).
const DEFAULT_CANDIDATE_KIND: &str = "note";

const OP_CAPTURE: &str = "memory.capture";
const OP_CANDIDATES_DISMISS: &str = "memory.candidates.dismiss";
const OP_CANDIDATES_REVIEW: &str = "memory.candidates.review";

/// `memory.candidates.list` payload (§7.12).
#[derive(Debug)]
pub(crate) struct CandidatePage {
    pub(crate) candidates: Vec<MemoryCandidate>,
    pub(crate) next_cursor: Option<String>,
}

// ---------------------------------------------------------------------------
// Protocol errors
// ---------------------------------------------------------------------------

fn reject(code: SpokeRejectCode, message: impl Into<String>) -> SpokeReject {
    SpokeReject {
        code,
        message: message.into(),
        details: None,
    }
}

fn reject_with(
    code: SpokeRejectCode,
    message: impl Into<String>,
    details: Map<String, Value>,
) -> SpokeReject {
    SpokeReject {
        code,
        message: message.into(),
        details: Some(details),
    }
}

/// One indistinguishable refusal for a candidate the caller cannot act on.
fn not_authorized(message: impl Into<String>) -> SpokeReject {
    reject(SpokeRejectCode::NotAuthorized, message)
}

fn not_found(message: impl Into<String>) -> SpokeReject {
    reject(SpokeRejectCode::NotFound, message)
}

fn subject_fenced(subject_id: &str, scope: &str) -> SpokeReject {
    reject_with(
        SpokeRejectCode::SubjectFenced,
        format!("subject {subject_id} is fenced for writes ({scope} scope)"),
        Map::from_iter([("scope".to_owned(), Value::String(scope.to_owned()))]),
    )
}

fn revision_conflict(expected: NonZeroU64, actual: u64) -> SpokeReject {
    reject_with(
        SpokeRejectCode::RevisionConflict,
        format!(
            "expected revision {} does not match the stored revision {actual}",
            expected.get()
        ),
        Map::from_iter([
            ("expected_revision".to_owned(), json!(expected.get())),
            ("actual_revision".to_owned(), json!(actual)),
        ]),
    )
}

fn candidate_consumed(candidate_id: &str) -> SpokeReject {
    reject(
        SpokeRejectCode::CandidateConsumed,
        format!("candidate {candidate_id} is already consumed"),
    )
}

/// §6.10/MEM-R021: the dedup tombstone outlived the receipt body, so the key is
/// bound but no receipt can be disclosed.
fn receipt_expired(last_known_status: &str) -> SpokeReject {
    reject_with(
        SpokeRejectCode::ReceiptExpired,
        "the receipt body was evicted; only the dedup tombstone remains",
        Map::from_iter([(
            "last_known_status".to_owned(),
            Value::String(last_known_status.to_owned()),
        )]),
    )
}

fn cursor_invalid(message: impl Into<String>) -> SpokeReject {
    reject(SpokeRejectCode::CursorInvalid, message)
}

fn idempotency_conflict() -> SpokeReject {
    reject(
        SpokeRejectCode::IdempotencyConflict,
        "the idempotency key is bound to a different payload digest",
    )
}

/// Flatten the published validator envelope into the crate's result shape.
fn checked(result: SpokeResult<()>) -> Result<(), SpokeReject> {
    match result {
        SpokeResult::Ok(()) => Ok(()),
        SpokeResult::Reject(reject) => Err(reject),
    }
}

fn digest(value: &Value) -> Result<String, SpokeReject> {
    match jcs_sha256_hex(value) {
        SpokeResult::Ok(digest) => Ok(digest),
        SpokeResult::Reject(reject) => Err(reject),
    }
}

fn timestamp_text(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Micros, true)
}

fn instant(raw: &str) -> Result<DateTime<Utc>, SpokeReject> {
    DateTime::parse_from_rfc3339(raw)
        .map(|parsed| parsed.with_timezone(&Utc))
        .map_err(|error| internal(format!("stored timestamp {raw} is not RFC 3339: {error}")))
}

fn stored_json(raw: &str, field: &str) -> Result<Value, SpokeReject> {
    serde_json::from_str(raw)
        .map_err(|error| internal(format!("stored {field} is not valid JSON: {error}")))
}

fn encoded(value: &Value) -> Result<String, SpokeReject> {
    serde_json::to_string(value).map_err(|error| internal(format!("value is not encodable: {error}")))
}

fn row_u64(row: &SqliteRow, column: &str) -> Result<u64, SpokeReject> {
    let raw: i64 = row.try_get(column).map_err(internal)?;
    u64::try_from(raw).map_err(internal)
}

fn row_i64(value: u64) -> Result<i64, SpokeReject> {
    i64::try_from(value).map_err(internal)
}

/// The fixture serves exactly one space; any other id has no existence signal.
fn require_served_space(
    provider: &FixtureMemoryProvider,
    requested: &str,
) -> Result<(), SpokeReject> {
    if requested == provider.identity.space_id {
        return Ok(());
    }
    Err(not_found(format!(
        "space {requested} is not served by this fixture"
    )))
}

/// §8.2 gate for a mutating candidate op. The allowlist decision is the
/// published predicate, so the fixture cannot drift from the protocol.
async fn require_space_admits(
    conn: &mut SqliteConnection,
    op: &str,
    space_id: &str,
) -> Result<(), SpokeReject> {
    let row = sqlx::query("SELECT status FROM spaces WHERE space_id = ?")
        .bind(space_id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(internal)?
        .ok_or_else(|| internal(format!("space {space_id} is not stored")))?;
    let status: String = row.try_get("status").map_err(internal)?;
    if status != "frozen" || is_freeze_allowlisted(op) {
        return Ok(());
    }
    Err(reject(
        SpokeRejectCode::SpaceFrozen,
        format!("space {space_id} is frozen; {op} is outside the §8.2 allowlist"),
    ))
}

/// The subject facts a candidate write needs: the lifecycle status and the
/// context-scoped fence allowlist (§7.7/MEM-R024).
struct CaptureTarget {
    status: String,
    fenced_contexts: Vec<String>,
}

async fn capture_target(
    conn: &mut SqliteConnection,
    space_id: &str,
    subject_id: &str,
) -> Result<CaptureTarget, SpokeReject> {
    let row = sqlx::query(
        "SELECT status, fenced_contexts_json FROM subjects WHERE space_id = ? AND subject_id = ?",
    )
    .bind(space_id)
    .bind(subject_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(internal)?
    .ok_or_else(|| not_found(format!("subject {subject_id} does not exist in this space")))?;
    let fenced: String = row.try_get("fenced_contexts_json").map_err(internal)?;
    Ok(CaptureTarget {
        status: row.try_get("status").map_err(internal)?,
        fenced_contexts: serde_json::from_str(&fenced)
            .map_err(|error| internal(format!("stored subject fences are not JSON: {error}")))?,
    })
}

/// Refuse a write that would place content on a fenced subject or into a
/// specific fenced placement context (§8.3/MEM-R024).
async fn require_write_target(
    conn: &mut SqliteConnection,
    space_id: &str,
    subject_id: &str,
    context_id: Option<&str>,
) -> Result<(), SpokeReject> {
    let target = capture_target(conn, space_id, subject_id).await?;
    if target.status != "active" {
        return Err(subject_fenced(subject_id, "subject"));
    }
    if let Some(context_id) = context_id {
        if target.fenced_contexts.iter().any(|fenced| fenced == context_id) {
            return Err(subject_fenced(subject_id, "context"));
        }
    }
    Ok(())
}

/// §6.5 coverage for one placement scope: a context placement needs that
/// context, a shared placement needs shared access.
fn scope_requirement<'a>(
    capability: &'a str,
    subject_id: &'a str,
    visibility: &str,
    context_id: Option<&'a str>,
) -> GrantRequirement<'a> {
    match visibility {
        "context" => GrantRequirement {
            capability,
            subject_id: Some(subject_id),
            context_ids: context_id.into_iter().map(str::to_owned).collect(),
            reads_shared: false,
            subject_all: false,
        },
        _ => GrantRequirement {
            capability,
            subject_id: Some(subject_id),
            context_ids: Vec::new(),
            reads_shared: true,
            subject_all: false,
        },
    }
}

/// §6.5/MEM-R006: one grant must cover every facet of the operation's actual
/// target. Two grants never union, so an op that needs two capabilities needs
/// one grant carrying both.
fn require_one_grant(
    grants: &[MemoryGrant],
    principal_id: &str,
    space_id: &str,
    now: DateTime<Utc>,
    requirements: &[GrantRequirement<'_>],
) -> Result<(), SpokeReject> {
    let covered = grants.iter().any(|grant| {
        requirements
            .iter()
            .all(|requirement| grant_covers(grant, principal_id, space_id, now, requirement))
    });
    if covered {
        return Ok(());
    }
    Err(not_authorized(
        "the authenticated principal holds no single grant covering this operation's target",
    ))
}

/// §7.28/MEM-R021: a receipt read rechecks current authorization. The receipt
/// body is content-free, so the only facet it can be re-evaluated against is
/// whether the requester still holds a live grant in the space — the same
/// admission fact `spaces.list` visibility uses.
fn holds_current_grant(
    grants: &[MemoryGrant],
    principal_id: &str,
    space_id: &str,
    now: DateTime<Utc>,
) -> bool {
    grants.iter().any(|grant| match grant {
        MemoryGrant::RestrictedGrant(grant) => {
            grant.principal_id.as_str() == principal_id
                && grant.space_id.as_str() == space_id
                && grant.revoked_at.is_none()
                && grant.expires_at.as_deref().is_none_or(|expires| *expires > now)
        }
        MemoryGrant::UnrestrictedGrant(grant) => {
            grant.principal_id.as_str() == principal_id
                && grant.space_id.as_str() == space_id
                && grant.revoked_at.is_none()
                && grant.expires_at.as_deref().is_none_or(|expires| *expires > now)
        }
    })
}

// ---------------------------------------------------------------------------
// Receipts and idempotency (§6.10, §7.0.5, §8.5)
// ---------------------------------------------------------------------------

/// The dedup identity of one admitted mutation: the op, the caller-chosen key
/// and the JCS payload digest that key is bound to (§6.1/§7.0.5).
///
/// Crate-visible with [`resolve_replay`] and [`stage_receipt_in_tx`] so the
/// assembly task can bind every other mutating op to this same namespace and
/// commit its effect, receipt and dedup row together.
pub(crate) struct Mutation {
    op: &'static str,
    idempotency_key: String,
    payload_digest: String,
}

impl Mutation {
    /// Build the dedup identity from the request object the op was called with.
    pub(crate) fn new(
        op: &'static str,
        idempotency_key: &str,
        payload: &Value,
    ) -> Result<Self, SpokeReject> {
        Ok(Self {
            op,
            idempotency_key: idempotency_key.to_owned(),
            payload_digest: digest(payload)?,
        })
    }
}

/// The state one durable receipt row is written with (§6.10). `Committed`
/// carries the content-free result summary; `Failed` carries the reject whose
/// envelope is stored with its §14.2 retry class.
pub(crate) enum ReceiptState {
    Committed(Value),
    Pending,
    Failed(SpokeReject),
}

/// §14.2 retry classes for the memory codes this slice can store.
fn retry_class(code: SpokeRejectCode) -> &'static str {
    match code {
        SpokeRejectCode::OutcomeUncertain => "uncertain",
        SpokeRejectCode::InternalError => "retryable",
        _ => "terminal",
    }
}

fn error_envelope(reject: &SpokeReject) -> Value {
    let mut details = reject.details.clone().unwrap_or_default();
    details.insert(
        "retry_class".to_owned(),
        Value::String(retry_class(reject.code).to_owned()),
    );
    json!({
        "code": reject.code.as_str(),
        "message": reject.message,
        "details": details,
        "extensions": {},
    })
}

pub(crate) fn receipt_id() -> String {
    format!("rcpt_{}", Uuid::new_v4().simple())
}

struct StoredReceipt {
    space_id: String,
    receipt_id: String,
    requester_principal_id: String,
    op: String,
    idempotency_key: Option<String>,
    payload_digest: String,
    status: String,
    result: Option<String>,
    error: Option<String>,
    created_at: String,
    completed_at: Option<String>,
}

async fn fetch_receipt(
    conn: &mut SqliteConnection,
    space_id: &str,
    receipt_id: &str,
    requester: Option<&str>,
) -> Result<Option<StoredReceipt>, SpokeReject> {
    let row = match requester {
        Some(requester) => {
            sqlx::query(
                "SELECT space_id, receipt_id, requester_principal_id, op, idempotency_key, \
                 payload_digest, status, result_json, error_json, created_at, completed_at \
                 FROM receipts \
                 WHERE space_id = ? AND receipt_id = ? AND requester_principal_id = ?",
            )
            .bind(space_id)
            .bind(receipt_id)
            .bind(requester)
            .fetch_optional(&mut *conn)
            .await
        }
        None => {
            sqlx::query(
                "SELECT space_id, receipt_id, requester_principal_id, op, idempotency_key, \
                 payload_digest, status, result_json, error_json, created_at, completed_at \
                 FROM receipts WHERE space_id = ? AND receipt_id = ?",
            )
            .bind(space_id)
            .bind(receipt_id)
            .fetch_optional(&mut *conn)
            .await
        }
    }
    .map_err(internal)?;
    row.map(|row| {
        Ok(StoredReceipt {
            space_id: row.try_get("space_id").map_err(internal)?,
            receipt_id: row.try_get("receipt_id").map_err(internal)?,
            requester_principal_id: row.try_get("requester_principal_id").map_err(internal)?,
            op: row.try_get("op").map_err(internal)?,
            idempotency_key: row.try_get("idempotency_key").map_err(internal)?,
            payload_digest: row.try_get("payload_digest").map_err(internal)?,
            status: row.try_get("status").map_err(internal)?,
            result: row.try_get("result_json").map_err(internal)?,
            error: row.try_get("error_json").map_err(internal)?,
            created_at: row.try_get("created_at").map_err(internal)?,
            completed_at: row.try_get("completed_at").map_err(internal)?,
        })
    })
    .transpose()
}

fn receipt_to_wire(stored: &StoredReceipt) -> Result<MemoryReceipt, SpokeReject> {
    let mut value = json!({
        "schema_version": SCHEMA_VERSION,
        "receipt_id": stored.receipt_id,
        "op": stored.op,
        "space_id": stored.space_id,
        "requester_principal_id": stored.requester_principal_id,
        "payload_digest": stored.payload_digest,
        "status": stored.status,
        "created_at": stored.created_at,
        "extensions": {},
    });
    if let Some(idempotency_key) = &stored.idempotency_key {
        value["idempotency_key"] = json!(idempotency_key);
    }
    match stored.status.as_str() {
        "pending" => {}
        "committed" => {
            value["result"] =
                stored_json(stored.result.as_deref().unwrap_or_default(), "receipt result")?;
            value["completed_at"] = json!(stored.completed_at);
        }
        "failed" => {
            value["error"] =
                stored_json(stored.error.as_deref().unwrap_or_default(), "receipt error")?;
            value["completed_at"] = json!(stored.completed_at);
        }
        other => {
            return Err(internal(format!(
                "stored receipt {} has unknown status {other}",
                stored.receipt_id
            )))
        }
    }
    serde_json::from_value(value)
        .map_err(|error| internal(format!("stored receipt row is not protocol-shaped: {error}")))
}

/// Stage one durable receipt plus its dedup row on the caller's connection and
/// advance the private snapshot marker in the same transaction (§6.10, §8.5).
/// Never commits, so an effect and its receipt commit together (MEM-R009).
pub(crate) async fn stage_receipt_in_tx(
    conn: &mut SqliteConnection,
    space_id: &str,
    principal_id: &str,
    admission: &Mutation,
    receipt_id: &str,
    state: ReceiptState,
    now: DateTime<Utc>,
) -> Result<MemoryReceipt, SpokeReject> {
    let (status, result, error, completed_at) = match state {
        ReceiptState::Committed(result) => (
            "committed",
            Some(encoded(&result)?),
            None,
            Some(timestamp_text(now)),
        ),
        ReceiptState::Pending => ("pending", None, None, None),
        ReceiptState::Failed(reject) => (
            "failed",
            None,
            Some(encoded(&error_envelope(&reject))?),
            Some(timestamp_text(now)),
        ),
    };
    sqlx::query(
        "INSERT INTO receipts (receipt_id, space_id, requester_principal_id, op, idempotency_key, \
         payload_digest, status, result_json, error_json, created_at, completed_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(receipt_id)
    .bind(space_id)
    .bind(principal_id)
    .bind(admission.op)
    .bind(&admission.idempotency_key)
    .bind(&admission.payload_digest)
    .bind(status)
    .bind(result)
    .bind(error)
    .bind(timestamp_text(now))
    .bind(completed_at)
    .execute(&mut *conn)
    .await
    .map_err(internal)?;
    sqlx::query(
        "INSERT INTO idempotency_namespace (space_id, requester_principal_id, op, idempotency_key, \
         payload_digest, receipt_id, last_known_status, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(space_id)
    .bind(principal_id)
    .bind(admission.op)
    .bind(&admission.idempotency_key)
    .bind(&admission.payload_digest)
    .bind(receipt_id)
    .bind(status)
    .bind(timestamp_text(now))
    .execute(&mut *conn)
    .await
    .map_err(|error| match error.as_database_error() {
        Some(database) if database.is_unique_violation() => idempotency_conflict(),
        _ => internal(error),
    })?;
    bump_snapshot(conn, space_id).await?;
    let stored = fetch_receipt(conn, space_id, receipt_id, None)
        .await?
        .ok_or_else(|| internal(format!("staged receipt {receipt_id} is not readable")))?;
    receipt_to_wire(&stored)
}

/// Complete a pending receipt and its dedup tombstone (§8.5): stage a
/// `Pending` receipt, later settle it to `Committed` or `Failed` on the caller's
/// connection and never return it to pending. The fault and retention slices
/// drive this transition; a synchronous op skips it and commits directly.
pub(crate) async fn settle_receipt_in_tx(
    conn: &mut SqliteConnection,
    space_id: &str,
    receipt_id: &str,
    state: ReceiptState,
    now: DateTime<Utc>,
) -> Result<MemoryReceipt, SpokeReject> {
    let (status, result, error) = match state {
        ReceiptState::Committed(result) => ("committed", Some(encoded(&result)?), None),
        ReceiptState::Failed(reject) => {
            ("failed", None, Some(encoded(&error_envelope(&reject))?))
        }
        ReceiptState::Pending => {
            return Err(internal(
                "a settled receipt cannot return to the pending state",
            ))
        }
    };
    let settled = sqlx::query(
        "UPDATE receipts SET status = ?, result_json = ?, error_json = ?, completed_at = ? \
         WHERE space_id = ? AND receipt_id = ? AND status = 'pending'",
    )
    .bind(status)
    .bind(result)
    .bind(error)
    .bind(timestamp_text(now))
    .bind(space_id)
    .bind(receipt_id)
    .execute(&mut *conn)
    .await
    .map_err(internal)?;
    if settled.rows_affected() != 1 {
        return Err(internal(format!(
            "receipt {receipt_id} was not pending and cannot be settled"
        )));
    }
    sqlx::query(
        "UPDATE idempotency_namespace SET last_known_status = ? \
         WHERE space_id = ? AND receipt_id = ?",
    )
    .bind(status)
    .bind(space_id)
    .bind(receipt_id)
    .execute(&mut *conn)
    .await
    .map_err(internal)?;
    bump_snapshot(conn, space_id).await?;
    let stored = fetch_receipt(conn, space_id, receipt_id, None)
        .await?
        .ok_or_else(|| internal(format!("settled receipt {receipt_id} is not readable")))?;
    receipt_to_wire(&stored)
}

/// §7.0.5/MEM-R021: an identical admitted replay returns the stored receipt
/// without re-executing; a changed payload under the same key is a conflict; an
/// evicted body leaves only the dedup tombstone.
pub(crate) async fn resolve_replay(
    conn: &mut SqliteConnection,
    space_id: &str,
    principal_id: &str,
    admission: &Mutation,
) -> Result<Option<MemoryReceipt>, SpokeReject> {
    let row = sqlx::query(
        "SELECT payload_digest, receipt_id, last_known_status FROM idempotency_namespace \
         WHERE space_id = ? AND requester_principal_id = ? AND op = ? AND idempotency_key = ?",
    )
    .bind(space_id)
    .bind(principal_id)
    .bind(admission.op)
    .bind(&admission.idempotency_key)
    .fetch_optional(&mut *conn)
    .await
    .map_err(internal)?;
    let Some(row) = row else {
        return Ok(None);
    };
    let digest: String = row.try_get("payload_digest").map_err(internal)?;
    if digest != admission.payload_digest {
        return Err(idempotency_conflict());
    }
    let receipt_id: String = row.try_get("receipt_id").map_err(internal)?;
    if let Some(stored) = fetch_receipt(conn, space_id, &receipt_id, None).await? {
        return Ok(Some(receipt_to_wire(&stored)?));
    }
    let last_known_status: String = row.try_get("last_known_status").map_err(internal)?;
    Err(receipt_expired(&last_known_status))
}

async fn tombstone_status(
    conn: &mut SqliteConnection,
    space_id: &str,
    receipt_id: &str,
    requester: Option<&str>,
) -> Result<Option<String>, SpokeReject> {
    let row = match requester {
        Some(requester) => sqlx::query(
            "SELECT last_known_status FROM idempotency_namespace \
             WHERE space_id = ? AND requester_principal_id = ? AND receipt_id = ?",
        )
        .bind(space_id)
        .bind(requester)
        .bind(receipt_id)
        .fetch_optional(&mut *conn)
        .await,
        None => sqlx::query(
            "SELECT last_known_status FROM idempotency_namespace \
             WHERE space_id = ? AND receipt_id = ?",
        )
        .bind(space_id)
        .bind(receipt_id)
        .fetch_optional(&mut *conn)
        .await,
    }
    .map_err(internal)?;
    row.map(|row| row.try_get("last_known_status").map_err(internal))
        .transpose()
}

// ---------------------------------------------------------------------------
// Stored candidates (§6.7)
// ---------------------------------------------------------------------------

struct StoredCandidate {
    candidate_id: String,
    subject_id: String,
    capture_visibility: String,
    capture_context_id: Option<String>,
    kind: String,
    content: Option<String>,
    provenance: String,
    classification: String,
    status: String,
    consumed_by: Option<String>,
    content_digest: Option<String>,
    occurred_at: Option<String>,
    expires_at: Option<String>,
    revision: u64,
    created_at: String,
}

fn stored_candidate(row: &SqliteRow) -> Result<StoredCandidate, SpokeReject> {
    Ok(StoredCandidate {
        candidate_id: row.try_get("candidate_id").map_err(internal)?,
        subject_id: row.try_get("subject_id").map_err(internal)?,
        capture_visibility: row.try_get("capture_visibility").map_err(internal)?,
        capture_context_id: row.try_get("capture_context_id").map_err(internal)?,
        kind: row.try_get("kind").map_err(internal)?,
        content: row.try_get("content_json").map_err(internal)?,
        provenance: row.try_get("provenance_json").map_err(internal)?,
        classification: row.try_get("classification_json").map_err(internal)?,
        status: row.try_get("status").map_err(internal)?,
        consumed_by: row.try_get("consumed_by_json").map_err(internal)?,
        content_digest: row.try_get("content_digest").map_err(internal)?,
        occurred_at: row.try_get("occurred_at").map_err(internal)?,
        expires_at: row.try_get("expires_at").map_err(internal)?,
        revision: row_u64(row, "revision")?,
        created_at: row.try_get("created_at").map_err(internal)?,
    })
}

async fn fetch_candidate(
    conn: &mut SqliteConnection,
    space_id: &str,
    candidate_id: &str,
) -> Result<Option<StoredCandidate>, SpokeReject> {
    let row = sqlx::query(
        "SELECT candidate_id, subject_id, capture_visibility, capture_context_id, kind, \
         content_json, provenance_json, classification_json, status, consumed_by_json, \
         content_digest, occurred_at, expires_at, revision, created_at \
         FROM candidates WHERE space_id = ? AND candidate_id = ?",
    )
    .bind(space_id)
    .bind(candidate_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(internal)?;
    row.map(|row| stored_candidate(&row)).transpose()
}

async fn candidate_rows(
    conn: &mut SqliteConnection,
    space_id: &str,
    subject_id: &str,
) -> Result<Vec<StoredCandidate>, SpokeReject> {
    let rows = sqlx::query(
        "SELECT candidate_id, subject_id, capture_visibility, capture_context_id, kind, \
         content_json, provenance_json, classification_json, status, consumed_by_json, \
         content_digest, occurred_at, expires_at, revision, created_at \
         FROM candidates WHERE space_id = ? AND subject_id = ? \
         ORDER BY created_at DESC, candidate_id",
    )
    .bind(space_id)
    .bind(subject_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(internal)?;
    rows.iter().map(stored_candidate).collect()
}

impl StoredCandidate {
    fn scope_context(&self) -> Option<&str> {
        self.capture_context_id.as_deref()
    }

    /// §6.7/§7.12: only a pending candidate expires, and an expired pending
    /// candidate is excluded from lists rather than consumed.
    fn expires_at(&self) -> Result<Option<DateTime<Utc>>, SpokeReject> {
        match self.expires_at.as_deref() {
            Some(raw) => instant(raw).map(Some),
            None => Ok(None),
        }
    }

    fn is_expired_at(&self, now: DateTime<Utc>) -> Result<bool, SpokeReject> {
        Ok(self.status == "pending" && self.expires_at()?.is_some_and(|expires| expires <= now))
    }

    /// §6.7 consumption scrub: the digest of the content that is about to leave
    /// the shell, retained for dedup/audit.
    fn scrubbed_digest(&self) -> Result<String, SpokeReject> {
        let raw = self
            .content
            .as_deref()
            .ok_or_else(|| internal("a pending candidate carries no content to scrub"))?;
        digest(&stored_json(raw, "candidate content")?)
    }
}

fn candidate_to_wire(
    candidate: &StoredCandidate,
    space_id: &str,
) -> Result<MemoryCandidate, SpokeReject> {
    let capture_scope = match (
        candidate.capture_visibility.as_str(),
        candidate.scope_context(),
    ) {
        ("shared", None) => json!({ "visibility": "shared" }),
        ("context", Some(context_id)) => {
            json!({ "visibility": "context", "context_id": context_id })
        }
        _ => {
            return Err(internal(format!(
                "candidate {} has an inconsistent capture scope",
                candidate.candidate_id
            )))
        }
    };
    let mut value = json!({
        "schema_version": SCHEMA_VERSION,
        "candidate_id": candidate.candidate_id,
        "space_id": space_id,
        "subject_id": candidate.subject_id,
        "capture_scope": capture_scope,
        "classification": stored_json(&candidate.classification, "candidate classification")?,
        "provenance": stored_json(&candidate.provenance, "candidate provenance")?,
        "status": candidate.status,
        "created_at": candidate.created_at,
        "revision": candidate.revision,
        "extensions": {},
    });
    if let Some(occurred_at) = &candidate.occurred_at {
        value["occurred_at"] = json!(occurred_at);
    }
    if candidate.status == "consumed" {
        let consumed_by = candidate
            .consumed_by
            .as_deref()
            .ok_or_else(|| internal("a consumed candidate carries no consumption record"))?;
        let content_digest = candidate
            .content_digest
            .as_deref()
            .ok_or_else(|| internal("a consumed candidate carries no content digest"))?;
        // The shell keeps the identity, the kind and the outcome; the body is
        // gone (§6.7 consumption scrub).
        value["proposed"] = json!({ "kind": candidate.kind });
        value["consumed_by"] = stored_json(consumed_by, "candidate consumption")?;
        value["content_digest"] = json!(content_digest);
    } else {
        let content = candidate
            .content
            .as_deref()
            .ok_or_else(|| internal("a pending candidate carries no content"))?;
        value["proposed"] =
            json!({ "kind": candidate.kind, "content": stored_json(content, "candidate content")? });
        if let Some(expires_at) = &candidate.expires_at {
            value["expires_at"] = json!(expires_at);
        }
    }
    serde_json::from_value(value)
        .map_err(|error| internal(format!("candidate row is not protocol-shaped: {error}")))
}

fn status_name(status: &CandidateStatus) -> &'static str {
    match status {
        CandidateStatus::Pending => "pending",
        CandidateStatus::Consumed => "consumed",
    }
}

impl FixtureMemoryProvider {
    // -----------------------------------------------------------------------
    // memory.capture (§7.11)
    // -----------------------------------------------------------------------

    /// Capture raw material for one subject; commit the candidates, the receipt
    /// and the dedup binding in one transaction.
    pub(crate) async fn capture(
        &self,
        principal_id: &str,
        request: &MemoryCaptureRequest,
        now: DateTime<Utc>,
    ) -> Result<MemoryReceipt, SpokeReject> {
        let mut transaction = self
            .store()
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(internal)?;
        let receipt = self
            .capture_in_tx(&mut transaction, principal_id, request, now)
            .await?;
        transaction.commit().await.map_err(internal)?;
        Ok(receipt)
    }

    /// [`Self::capture`] on a caller-owned connection; never commits.
    pub(crate) async fn capture_in_tx(
        &self,
        conn: &mut SqliteConnection,
        principal_id: &str,
        request: &MemoryCaptureRequest,
        now: DateTime<Utc>,
    ) -> Result<MemoryReceipt, SpokeReject> {
        checked(validate_memory_capture(request, MAX_CONTENT_BYTES))?;
        let space_id = request.space_id.as_str();
        require_served_space(self, space_id)?;
        let subject_id = request.subject_id.as_str();
        let (visibility, context_id) = placement(&request.capture_scope);
        let grants = load_grants(conn, space_id).await?;
        require_one_grant(
            &grants,
            principal_id,
            space_id,
            now,
            &[scope_requirement("capture", subject_id, visibility, context_id)],
        )?;
        let payload = serde_json::to_value(request)
            .map_err(|error| internal(format!("capture request is not encodable: {error}")))?;
        let admission = Mutation::new(OP_CAPTURE, request.idempotency_key.as_str(), &payload)?;
        if let Some(replay) = resolve_replay(conn, space_id, principal_id, &admission).await? {
            return Ok(replay);
        }
        require_space_admits(conn, OP_CAPTURE, space_id).await?;
        require_write_target(conn, space_id, subject_id, context_id).await?;

        let classification = classify(&request.content);
        let disposition = classification["disposition"]
            .as_str()
            .unwrap_or("drop")
            .to_owned();
        let mut candidate_ids = Vec::new();
        if disposition != "drop" {
            let candidate_id = format!("cand_{}", Uuid::new_v4().simple());
            let kind = request
                .kind_hint
                .as_deref()
                .map_or(DEFAULT_CANDIDATE_KIND, String::as_str);
            let provenance = json!({
                "source_context_ids": request.source_context_ids,
                "anchors": request.anchors,
                "captured_by": principal_id,
                "captured_at": timestamp_text(now),
            });
            // Wire shape of `proposed.content` (§6.6): `structured` is present
            // only when the request carried it, matching the typed round trip
            // that the consumption digest is computed over.
            let content = match &request.content.structured {
                Some(structured) => json!({
                    "text": request.content.text,
                    "structured": structured,
                }),
                None => json!({ "text": request.content.text }),
            };
            sqlx::query(
                "INSERT INTO candidates (candidate_id, space_id, subject_id, capture_visibility, \
                 capture_context_id, kind, content_json, provenance_json, classification_json, \
                 status, occurred_at, expires_at, revision, created_at) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 'pending', ?, ?, 1, ?)",
            )
            .bind(&candidate_id)
            .bind(space_id)
            .bind(subject_id)
            .bind(visibility)
            .bind(context_id)
            .bind(kind)
            .bind(encoded(&content)?)
            .bind(encoded(&provenance)?)
            .bind(encoded(&classification)?)
            .bind(request.occurred_at.map(timestamp_text))
            .bind(timestamp_text(now + Duration::days(CANDIDATE_TTL_DAYS)))
            .bind(timestamp_text(now))
            .execute(&mut *conn)
            .await
            .map_err(internal)?;
            candidate_ids.push(candidate_id);
        }

        // A zero-candidate capture is a committed outcome whose receipt records
        // the count, never an empty success without a receipt (§7.11).
        let result = json!({
            "candidate_ids": candidate_ids,
            "classified": 1,
        });
        stage_receipt_in_tx(
            conn,
            space_id,
            principal_id,
            &admission,
            &receipt_id(),
            ReceiptState::Committed(result),
            now,
        )
        .await
    }

    // -----------------------------------------------------------------------
    // memory.candidates.list / get (§7.12/§7.13)
    // -----------------------------------------------------------------------

    pub(crate) async fn candidates_list(
        &self,
        principal_id: &str,
        request: &MemoryCandidatesListRequest,
        now: DateTime<Utc>,
    ) -> Result<CandidatePage, SpokeReject> {
        let mut transaction = self.store().pool().begin().await.map_err(internal)?;
        let page = self
            .candidates_list_in_tx(&mut transaction, principal_id, request, now)
            .await?;
        transaction.commit().await.map_err(internal)?;
        Ok(page)
    }

    /// Whole page read on the caller's snapshot: space access, grants,
    /// candidates and the cursor marker all agree in one snapshot (MEM-R026).
    pub(crate) async fn candidates_list_in_tx(
        &self,
        conn: &mut SqliteConnection,
        principal_id: &str,
        request: &MemoryCandidatesListRequest,
        now: DateTime<Utc>,
    ) -> Result<CandidatePage, SpokeReject> {
        checked(validate_memory_candidates_list(request, MAX_PAGE_SIZE))?;
        let space_id = request.space_id.as_str();
        require_served_space(self, space_id)?;
        let subject_id = request.subject_id.as_str();
        let grants = load_grants(conn, space_id).await?;
        // Refuse before revealing whether the subject exists: a principal with
        // no `review` grant on the subject gets the same answer either way.
        if !is_authorized(
            &grants,
            principal_id,
            space_id,
            now,
            &requirement("review", Some(subject_id)),
        ) {
            return Err(not_authorized(
                "the authenticated principal holds no review grant on this subject",
            ));
        }
        capture_target(conn, space_id, subject_id).await?;

        let status_filter = request.status.as_ref().map(status_name);
        let mut visible = Vec::new();
        for candidate in candidate_rows(conn, space_id, subject_id).await? {
            if status_filter.is_some_and(|status| candidate.status != status) {
                continue;
            }
            if candidate.is_expired_at(now)? {
                continue;
            }
            // Each candidate is filtered by its immutable capture scope, so a
            // context-limited review grant never sees another placement.
            if !is_authorized(
                &grants,
                principal_id,
                space_id,
                now,
                &scope_requirement(
                    "review",
                    subject_id,
                    &candidate.capture_visibility,
                    candidate.scope_context(),
                ),
            ) {
                continue;
            }
            visible.push(candidate);
        }

        let snapshot = snapshot_marker(conn, space_id).await?;
        let binding_digest = digest(&json!({
            "principal": principal_id,
            "space": space_id,
            "subject": subject_id,
            "status": status_filter,
            "snapshot": snapshot,
            "policy": ENGINE_POLICY_VERSION,
        }))?;
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
                visible
                    .iter()
                    .position(|candidate| candidate.candidate_id == last_id)
                    .map(|position| position + 1)
                    .ok_or_else(|| cursor_invalid("cursor no longer matches the candidate set"))?
            }
        };
        let limit = usize::try_from(
            request
                .limit
                .map_or(MAX_PAGE_SIZE.get(), std::num::NonZeroU64::get),
        )
        .map_err(internal)?;
        let remaining = &visible[start..];
        let page = &remaining[..remaining.len().min(limit)];
        let next_cursor = if remaining.len() > limit {
            page.last()
                .map(|candidate| format!("{binding_digest}:{}", candidate.candidate_id))
        } else {
            None
        };
        let candidates = page
            .iter()
            .map(|candidate| candidate_to_wire(candidate, space_id))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(CandidatePage {
            candidates,
            next_cursor,
        })
    }

    pub(crate) async fn candidates_get(
        &self,
        principal_id: &str,
        request: &MemoryCandidatesGetRequest,
        now: DateTime<Utc>,
    ) -> Result<MemoryCandidate, SpokeReject> {
        let mut transaction = self.store().pool().begin().await.map_err(internal)?;
        let candidate = self
            .candidates_get_in_tx(&mut transaction, principal_id, request, now)
            .await?;
        transaction.commit().await.map_err(internal)?;
        Ok(candidate)
    }

    /// By-id read on the caller's snapshot: current authorization and the row
    /// are read together, so a revoke cannot slip between them (MEM-R026).
    pub(crate) async fn candidates_get_in_tx(
        &self,
        conn: &mut SqliteConnection,
        principal_id: &str,
        request: &MemoryCandidatesGetRequest,
        now: DateTime<Utc>,
    ) -> Result<MemoryCandidate, SpokeReject> {
        let space_id = request.space_id.as_str();
        require_served_space(self, space_id)?;
        let grants = load_grants(conn, space_id).await?;
        let candidate_id = request.candidate_id.as_str();
        let candidate = fetch_candidate(conn, space_id, candidate_id)
            .await?
            .ok_or_else(|| not_found(format!("candidate {candidate_id} does not exist")))?;
        require_one_grant(
            &grants,
            principal_id,
            space_id,
            now,
            &[scope_requirement(
                "review",
                &candidate.subject_id,
                &candidate.capture_visibility,
                candidate.scope_context(),
            )],
        )?;
        candidate_to_wire(&candidate, space_id)
    }

    // -----------------------------------------------------------------------
    // memory.candidates.dismiss (§7.14)
    // -----------------------------------------------------------------------

    pub(crate) async fn candidates_dismiss(
        &self,
        principal_id: &str,
        request: &MemoryCandidatesDismissRequest,
        now: DateTime<Utc>,
    ) -> Result<MemoryReceipt, SpokeReject> {
        let mut transaction = self
            .store()
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(internal)?;
        let receipt = self
            .candidates_dismiss_in_tx(&mut transaction, principal_id, request, now)
            .await?;
        transaction.commit().await.map_err(internal)?;
        Ok(receipt)
    }

    /// [`Self::candidates_dismiss`] on a caller-owned connection; never commits.
    pub(crate) async fn candidates_dismiss_in_tx(
        &self,
        conn: &mut SqliteConnection,
        principal_id: &str,
        request: &MemoryCandidatesDismissRequest,
        now: DateTime<Utc>,
    ) -> Result<MemoryReceipt, SpokeReject> {
        let space_id = request.space_id.as_str();
        require_served_space(self, space_id)?;
        let candidate_id = request.candidate_id.as_str();
        let grants = load_grants(conn, space_id).await?;
        let candidate = fetch_candidate(conn, space_id, candidate_id)
            .await?
            .ok_or_else(|| not_found(format!("candidate {candidate_id} does not exist")))?;
        require_one_grant(
            &grants,
            principal_id,
            space_id,
            now,
            &[scope_requirement(
                "review",
                &candidate.subject_id,
                &candidate.capture_visibility,
                candidate.scope_context(),
            )],
        )?;
        let payload = serde_json::to_value(request)
            .map_err(|error| internal(format!("dismiss request is not encodable: {error}")))?;
        let admission =
            Mutation::new(OP_CANDIDATES_DISMISS, request.idempotency_key.as_str(), &payload)?;
        if let Some(replay) = resolve_replay(conn, space_id, principal_id, &admission).await? {
            return Ok(replay);
        }
        require_space_admits(conn, OP_CANDIDATES_DISMISS, space_id).await?;
        // `consumed` is terminal, so it outranks the CAS precondition (§8.1).
        if candidate.status == "consumed" {
            return Err(candidate_consumed(candidate_id));
        }
        if request.expected_revision.get() != candidate.revision {
            return Err(revision_conflict(
                request.expected_revision,
                candidate.revision,
            ));
        }

        let receipt_id = receipt_id();
        let content_digest = candidate.scrubbed_digest()?;
        let consumed_by = json!({
            "op": OP_CANDIDATES_DISMISS,
            "receipt_id": receipt_id,
            "outcome": "dismissed",
            "record_ids": [],
        });
        consume_candidate(conn, space_id, &candidate, &content_digest, &consumed_by).await?;
        stage_receipt_in_tx(
            conn,
            space_id,
            principal_id,
            &admission,
            &receipt_id,
            ReceiptState::Committed(json!({
                "candidate_id": candidate_id,
                "outcome": "dismissed",
            })),
            now,
        )
        .await
    }

    // -----------------------------------------------------------------------
    // memory.candidates.review (§7.15)
    // -----------------------------------------------------------------------

    pub(crate) async fn candidates_review(
        &self,
        principal_id: &str,
        request: &MemoryCandidatesReviewRequest,
        now: DateTime<Utc>,
    ) -> Result<MemoryReceipt, SpokeReject> {
        let mut transaction = self
            .store()
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(internal)?;
        let receipt = self
            .candidates_review_in_tx(&mut transaction, principal_id, request, now)
            .await?;
        transaction.commit().await.map_err(internal)?;
        Ok(receipt)
    }

    /// [`Self::candidates_review`] on a caller-owned connection; never commits,
    /// so candidate consumption, every created record and the receipt are one
    /// atomic outcome (MEM-R009).
    pub(crate) async fn candidates_review_in_tx(
        &self,
        conn: &mut SqliteConnection,
        principal_id: &str,
        request: &MemoryCandidatesReviewRequest,
        now: DateTime<Utc>,
    ) -> Result<MemoryReceipt, SpokeReject> {
        checked(validate_memory_candidates_review(request))?;
        let space_id = request.space_id.as_str();
        require_served_space(self, space_id)?;
        let candidate_id = request.candidate_id.as_str();
        let grants = load_grants(conn, space_id).await?;
        let candidate = fetch_candidate(conn, space_id, candidate_id)
            .await?
            .ok_or_else(|| not_found(format!("candidate {candidate_id} does not exist")))?;
        let subject_id = candidate.subject_id.clone();
        let visibility = candidate.capture_visibility.clone();
        let context_id = candidate.scope_context().map(str::to_owned);
        // §7.15: `review` *and* `records_write`, both covering the candidate's
        // capture scope — in one grant, so no union amplifies coverage.
        require_one_grant(
            &grants,
            principal_id,
            space_id,
            now,
            &[
                scope_requirement("review", &subject_id, &visibility, context_id.as_deref()),
                scope_requirement(
                    "records_write",
                    &subject_id,
                    &visibility,
                    context_id.as_deref(),
                ),
            ],
        )?;
        let payload = serde_json::to_value(request)
            .map_err(|error| internal(format!("review request is not encodable: {error}")))?;
        let admission =
            Mutation::new(OP_CANDIDATES_REVIEW, request.idempotency_key.as_str(), &payload)?;
        if let Some(replay) = resolve_replay(conn, space_id, principal_id, &admission).await? {
            return Ok(replay);
        }
        require_space_admits(conn, OP_CANDIDATES_REVIEW, space_id).await?;
        if candidate.status == "consumed" {
            return Err(candidate_consumed(candidate_id));
        }
        if request.expected_revision.get() != candidate.revision {
            return Err(revision_conflict(
                request.expected_revision,
                candidate.revision,
            ));
        }
        require_write_target(conn, space_id, &subject_id, context_id.as_deref()).await?;

        let receipt_id = receipt_id();
        // The decision carries no placement: created records inherit the
        // candidate's admitted capture scope verbatim (§7.15/C-034).
        let (outcome, record_ids) = match &request.decision {
            Decision::Promote {
                content_override,
                kind,
            } => {
                let kind = match kind {
                    Some(kind) => kind.as_str(),
                    None => candidate.kind.as_str(),
                };
                let content = match content_override {
                    Some(overridden) => serde_json::to_value(overridden).map_err(|error| {
                        internal(format!("decision content is not encodable: {error}"))
                    })?,
                    None => stored_json(
                        candidate
                            .content
                            .as_deref()
                            .ok_or_else(|| internal("a pending candidate carries no content"))?,
                        "candidate content",
                    )?,
                };
                let record_id =
                    insert_record(conn, space_id, &candidate, kind, &content, principal_id, now)
                        .await?;
                ("promoted".to_owned(), vec![record_id])
            }
            Decision::Fragment { fragments } => {
                let mut record_ids = Vec::with_capacity(fragments.len());
                for fragment in fragments {
                    let content = serde_json::to_value(&fragment.content).map_err(|error| {
                        internal(format!("fragment content is not encodable: {error}"))
                    })?;
                    record_ids.push(
                        insert_record(
                            conn,
                            space_id,
                            &candidate,
                            fragment.kind.as_str(),
                            &content,
                            principal_id,
                            now,
                        )
                        .await?,
                    );
                }
                ("fragmented".to_owned(), record_ids)
            }
            Decision::Drop { .. } => ("dropped".to_owned(), Vec::new()),
        };

        let content_digest = candidate.scrubbed_digest()?;
        let consumed_by = json!({
            "op": OP_CANDIDATES_REVIEW,
            "receipt_id": receipt_id,
            "outcome": outcome,
            "record_ids": record_ids,
        });
        consume_candidate(conn, space_id, &candidate, &content_digest, &consumed_by).await?;
        stage_receipt_in_tx(
            conn,
            space_id,
            principal_id,
            &admission,
            &receipt_id,
            ReceiptState::Committed(json!({
                "outcome": outcome,
                "record_ids": record_ids,
            })),
            now,
        )
        .await
    }

    // -----------------------------------------------------------------------
    // memory.receipts.get (§7.28)
    // -----------------------------------------------------------------------

    pub(crate) async fn receipts_get(
        &self,
        principal_id: &str,
        request: &MemoryReceiptsGetRequest,
        now: DateTime<Utc>,
    ) -> Result<MemoryReceipt, SpokeReject> {
        let mut transaction = self.store().pool().begin().await.map_err(internal)?;
        let receipt = self
            .receipts_get_in_tx(&mut transaction, principal_id, request, now)
            .await?;
        transaction.commit().await.map_err(internal)?;
        Ok(receipt)
    }

    /// Receipt read on the caller's snapshot: current authorization and the
    /// receipt row are read together (MEM-R018/R026).
    pub(crate) async fn receipts_get_in_tx(
        &self,
        conn: &mut SqliteConnection,
        principal_id: &str,
        request: &MemoryReceiptsGetRequest,
        now: DateTime<Utc>,
    ) -> Result<MemoryReceipt, SpokeReject> {
        let space_id = request.space_id.as_str();
        require_served_space(self, space_id)?;
        let receipt_id = request.receipt_id.as_str();
        let grants = load_grants(conn, space_id).await?;
        let admin = is_authorized(
            &grants,
            principal_id,
            space_id,
            now,
            &whole_scope_requirement("admin", None),
        );
        // A non-admin can only ever see its own receipts, so another principal's
        // receipt id is indistinguishable from a missing one (no oracle).
        let stored = fetch_receipt(
            conn,
            space_id,
            receipt_id,
            if admin { None } else { Some(principal_id) },
        )
        .await?;
        let Some(stored) = stored else {
            if let Some(last_known_status) = tombstone_status(
                conn,
                space_id,
                receipt_id,
                if admin { None } else { Some(principal_id) },
            )
            .await?
            {
                return Err(receipt_expired(&last_known_status));
            }
            // Deliberately carries no receipt id: a foreign receipt and a
            // missing one must be indistinguishable (§7.28, no existence oracle).
            return Err(not_found(
                "no receipt is visible to the authenticated principal",
            ));
        };
        if !admin && !holds_current_grant(&grants, principal_id, space_id, now) {
            return Err(not_authorized(
                "the receipt requester holds no current grant in this space",
            ));
        }
        receipt_to_wire(&stored)
    }
}

fn placement(scope: &CapturePlacement) -> (&'static str, Option<&str>) {
    match scope {
        CapturePlacement::Shared => ("shared", None),
        CapturePlacement::Context(context_id) => ("context", Some(context_id.as_str())),
    }
}

/// Deterministic advisory classification (§6.7): empty material is dropped
/// without becoming a candidate, everything else becomes one pending candidate.
fn classify(content: &CaptureContent) -> Value {
    let empty = content.text.trim().is_empty() && content.structured.is_none();
    if empty {
        json!({
            "disposition": "drop",
            "rationale_codes": ["empty_content"],
        })
    } else {
        json!({
            "disposition": "promote",
            "rationale_codes": ["captured_text"],
        })
    }
}

/// One-way consumption with the §6.7 scrub: the body leaves, the digest and the
/// outcome arrive, the shell keeps its identity. `pending` is the only status
/// that can be consumed, so a second consumer sees zero rows.
async fn consume_candidate(
    conn: &mut SqliteConnection,
    space_id: &str,
    candidate: &StoredCandidate,
    content_digest: &str,
    consumed_by: &Value,
) -> Result<(), SpokeReject> {
    let consumed = sqlx::query(
        "UPDATE candidates SET status = 'consumed', content_json = NULL, content_digest = ?, \
         consumed_by_json = ?, revision = revision + 1 \
         WHERE space_id = ? AND candidate_id = ? AND status = 'pending' AND revision = ?",
    )
    .bind(content_digest)
    .bind(encoded(consumed_by)?)
    .bind(space_id)
    .bind(&candidate.candidate_id)
    .bind(row_i64(candidate.revision)?)
    .execute(&mut *conn)
    .await
    .map_err(internal)?;
    if consumed.rows_affected() != 1 {
        return Err(internal(format!(
            "candidate {} moved during consumption",
            candidate.candidate_id
        )));
    }
    Ok(())
}

/// Insert one retained record that inherits the candidate's capture scope. The
/// provenance keeps the candidate's claims and takes the reviewing principal as
/// the provider-populated `captured_by` (§6.6, §7.15).
async fn insert_record(
    conn: &mut SqliteConnection,
    space_id: &str,
    candidate: &StoredCandidate,
    kind: &str,
    content: &Value,
    principal_id: &str,
    now: DateTime<Utc>,
) -> Result<String, SpokeReject> {
    let mut provenance = stored_json(&candidate.provenance, "candidate provenance")?;
    let members = provenance
        .as_object_mut()
        .ok_or_else(|| internal("stored candidate provenance is not an object"))?;
    members.insert("captured_by".to_owned(), json!(principal_id));
    members.insert("captured_at".to_owned(), json!(timestamp_text(now)));
    let record_id = format!("rec_{}", Uuid::new_v4().simple());
    sqlx::query(
        "INSERT INTO records (record_id, space_id, subject_id, kind, content_json, \
         scope_visibility, scope_context_id, provenance_json, occurred_at, revision, created_at, \
         updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 1, ?, ?)",
    )
    .bind(&record_id)
    .bind(space_id)
    .bind(&candidate.subject_id)
    .bind(kind)
    .bind(encoded(content)?)
    .bind(&candidate.capture_visibility)
    .bind(candidate.scope_context())
    .bind(encoded(&provenance)?)
    .bind(candidate.occurred_at.as_deref())
    .bind(timestamp_text(now))
    .bind(timestamp_text(now))
    .execute(&mut *conn)
    .await
    .map_err(internal)?;
    Ok(record_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::Arc;
    use tempfile::TempDir;

    use spoke_operations::spoke_schemas::memory_candidate::{ConsumedCandidate, PendingCandidate};
    use spoke_operations::spoke_schemas::{
        MemoryGrantsGrantRequest, MemoryGrantsRevokeRequest, MemorySpacesFreezeRequest,
        MemorySubjectsFenceRequest, MemorySubjectsEnsureRequest,
    };

    const OPERATOR: &str = "principal-operator";
    const CLIENT: &str = "principal-client";
    const OTHER: &str = "principal-other";
    const SUBJECT: &str = "subject-alpha";
    const CONTEXT_A: &str = "bnd-alpha";
    const CONTEXT_B: &str = "bnd-beta";
    const NOW_TEXT: &str = "2026-11-04T00:00:00Z";

    struct Scenario {
        _home: TempDir,
        path: PathBuf,
        provider: FixtureMemoryProvider,
        space_id: String,
    }

    impl Scenario {
        async fn open() -> Self {
            let home = tempfile::tempdir().expect("scenario home");
            let path = home.path().join("candidates.sqlite");
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

        async fn subject(&self, now: DateTime<Utc>) {
            let request: MemorySubjectsEnsureRequest = serde_json::from_value(json!({
                "space_id": self.space_id,
                "subject_id": SUBJECT,
            }))
            .expect("valid subjects.ensure request");
            self.provider
                .subjects_ensure(OPERATOR, &request, now)
                .await
                .expect("subject ensured");
        }

        async fn grant(
            &self,
            grantee: &str,
            capabilities: &[&str],
            subject: Option<&str>,
            contexts: Option<(&[&str], bool)>,
            now: DateTime<Utc>,
        ) -> String {
            let mut value = json!({
                "space_id": self.space_id,
                "principal_id": grantee,
                "capabilities": capabilities,
                "idempotency_key": format!("grant-{grantee}-{}", capabilities.join("-")),
            });
            if let Some(subject) = subject {
                value["subject_id"] = json!(subject);
            }
            if let Some((contexts, shared)) = contexts {
                value["context_ids"] = json!(contexts);
                value["shared_access"] = json!(shared);
            }
            let request: MemoryGrantsGrantRequest =
                serde_json::from_value(value).expect("valid grants.grant request");
            self.provider
                .grants_grant(OPERATOR, &request, now)
                .await
                .expect("grant issued")
                .grant_id
        }

        async fn revoke(&self, grant_id: &str, now: DateTime<Utc>) {
            let request: MemoryGrantsRevokeRequest = serde_json::from_value(json!({
                "space_id": self.space_id,
                "grant_id": grant_id,
                "expected_revision": 1,
                "idempotency_key": format!("revoke-{grant_id}"),
            }))
            .expect("valid grants.revoke request");
            self.provider
                .grants_revoke(OPERATOR, &request, now)
                .await
                .expect("grant revoked");
        }

        async fn freeze(&self, now: DateTime<Utc>) {
            let request: MemorySpacesFreezeRequest = serde_json::from_value(json!({
                "space_id": self.space_id,
                "expected_revision": 1,
                "idempotency_key": "freeze-1",
            }))
            .expect("valid spaces.freeze request");
            self.provider
                .spaces_freeze(OPERATOR, &request, now)
                .await
                .expect("space frozen");
        }

        async fn fence_context(&self, context_id: &str, now: DateTime<Utc>) {
            let request: MemorySubjectsFenceRequest = serde_json::from_value(json!({
                "space_id": self.space_id,
                "subject_id": SUBJECT,
                "target": { "type": "context", "context_id": context_id },
                "action": "fence",
                "expected_revision": 1,
                "idempotency_key": format!("fence-{context_id}"),
            }))
            .expect("valid subjects.fence request");
            self.provider
                .subjects_fence(OPERATOR, &request, now)
                .await
                .expect("context fenced");
        }
    }

    /// The one deterministic admission instant the scenario tests supply.
    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(NOW_TEXT)
            .expect("a valid scenario instant")
            .with_timezone(&Utc)
    }

    fn later(days: i64) -> DateTime<Utc> {
        now() + Duration::days(days)
    }

    fn context_scope(context_id: &str) -> Value {
        json!({ "visibility": "context", "context_id": context_id })
    }

    fn shared_scope() -> Value {
        json!({ "visibility": "shared" })
    }

    fn capture_request(
        space: &str,
        text: &str,
        scope: Value,
        key: &str,
    ) -> MemoryCaptureRequest {
        serde_json::from_value(json!({
            "space_id": space,
            "subject_id": SUBJECT,
            "content": { "text": text },
            "capture_scope": scope,
            "source_context_ids": [],
            "kind_hint": "fact",
            "idempotency_key": key,
        }))
        .expect("valid capture request")
    }

    fn list_request(
        space: &str,
        status: Option<&str>,
        limit: Option<u64>,
        cursor: Option<&str>,
    ) -> MemoryCandidatesListRequest {
        let mut value = json!({ "space_id": space, "subject_id": SUBJECT });
        if let Some(status) = status {
            value["status"] = json!(status);
        }
        if let Some(limit) = limit {
            value["limit"] = json!(limit);
        }
        if let Some(cursor) = cursor {
            value["cursor"] = json!(cursor);
        }
        serde_json::from_value(value).expect("valid candidates.list request")
    }

    fn get_request(space: &str, candidate_id: &str) -> MemoryCandidatesGetRequest {
        serde_json::from_value(json!({
            "space_id": space,
            "candidate_id": candidate_id,
        }))
        .expect("valid candidates.get request")
    }

    fn dismiss_request(
        space: &str,
        candidate_id: &str,
        revision: u64,
        key: &str,
    ) -> MemoryCandidatesDismissRequest {
        serde_json::from_value(json!({
            "space_id": space,
            "candidate_id": candidate_id,
            "expected_revision": revision,
            "idempotency_key": key,
        }))
        .expect("valid candidates.dismiss request")
    }

    fn review_request(
        space: &str,
        candidate_id: &str,
        revision: u64,
        decision: Value,
        key: &str,
    ) -> MemoryCandidatesReviewRequest {
        serde_json::from_value(json!({
            "space_id": space,
            "candidate_id": candidate_id,
            "expected_revision": revision,
            "decision": decision,
            "idempotency_key": key,
        }))
        .expect("valid candidates.review request")
    }

    fn receipts_request(space: &str, receipt_id: &str) -> MemoryReceiptsGetRequest {
        serde_json::from_value(json!({
            "space_id": space,
            "receipt_id": receipt_id,
        }))
        .expect("valid receipts.get request")
    }

    fn refusal<T: std::fmt::Debug>(result: Result<T, SpokeReject>) -> SpokeReject {
        match result {
            Ok(value) => panic!("expected a protocol reject, observed {value:?}"),
            Err(reject) => reject,
        }
    }

    fn receipt_status(receipt: &MemoryReceipt) -> &'static str {
        match receipt {
            MemoryReceipt::CommittedReceipt(_) => "committed",
            MemoryReceipt::PendingReceipt(_) => "pending",
            MemoryReceipt::FailedReceipt(_) => "failed",
        }
    }

    fn receipt_id_of(receipt: &MemoryReceipt) -> &str {
        match receipt {
            MemoryReceipt::CommittedReceipt(receipt) => receipt.receipt_id.as_str(),
            MemoryReceipt::PendingReceipt(receipt) => receipt.receipt_id.as_str(),
            MemoryReceipt::FailedReceipt(receipt) => receipt.receipt_id.as_str(),
        }
    }

    fn committed_result(receipt: &MemoryReceipt) -> &Value {
        match receipt {
            MemoryReceipt::CommittedReceipt(receipt) => &receipt.result,
            other => panic!("expected a committed receipt, observed {other:?}"),
        }
    }

    fn candidate_ids(page: &CandidatePage) -> Vec<String> {
        page.candidates
            .iter()
            .map(|candidate| match candidate {
                MemoryCandidate::PendingCandidate(candidate) => {
                    candidate.candidate_id.as_str().to_owned()
                }
                MemoryCandidate::ConsumedCandidate(candidate) => {
                    candidate.candidate_id.as_str().to_owned()
                }
            })
            .collect()
    }

    fn pending_candidate(candidate: &MemoryCandidate) -> &PendingCandidate {
        match candidate {
            MemoryCandidate::PendingCandidate(pending) => pending,
            other => panic!("expected a pending candidate, observed {other:?}"),
        }
    }

    fn consumed_candidate(candidate: &MemoryCandidate) -> &ConsumedCandidate {
        match candidate {
            MemoryCandidate::ConsumedCandidate(consumed) => consumed,
            other => panic!("expected a consumed candidate, observed {other:?}"),
        }
    }

    /// The dedup tombstone's recorded status for one caller-chosen key.
    async fn tombstone(scenario: &Scenario, idempotency_key: &str) -> Option<String> {
        let row = sqlx::query(
            "SELECT last_known_status FROM idempotency_namespace \
             WHERE space_id = ? AND idempotency_key = ?",
        )
        .bind(&scenario.space_id)
        .bind(idempotency_key)
        .fetch_optional(scenario.provider.store().pool())
        .await
        .expect("tombstone query");
        row.map(|row| row.get("last_known_status"))
    }

    async fn count_rows(provider: &FixtureMemoryProvider, table: &str) -> i64 {
        let sql = match table {
            "candidates" => "SELECT COUNT(*) AS total FROM candidates",
            "records" => "SELECT COUNT(*) AS total FROM records",
            "receipts" => "SELECT COUNT(*) AS total FROM receipts",
            "idempotency_namespace" => "SELECT COUNT(*) AS total FROM idempotency_namespace",
            other => panic!("unknown table {other}"),
        };
        let row = sqlx::query(sql)
            .fetch_one(provider.store().pool())
            .await
            .expect("count query");
        row.get("total")
    }

    fn assert_no_secret(text: &str, secret: &str) {
        assert!(
            !text.contains(secret),
            "the disclosed value must not carry captured text"
        );
    }

    #[tokio::test]
    async fn capture_receipt_is_content_free_and_replay_binds_the_payload_digest() {
        let scenario = Scenario::open().await;
        let now = now();
        scenario.subject(now).await;
        scenario
            .grant(CLIENT, &["capture"], Some(SUBJECT), None, now)
            .await;
        let secret = "Mira told Bo the gate key is hidden under the third barrel.";
        let request = capture_request(&scenario.space_id, secret, context_scope(CONTEXT_A), "cap-1");

        let first = scenario
            .provider
            .capture(CLIENT, &request, now)
            .await
            .expect("capture commits");
        assert_eq!(receipt_status(&first), "committed");
        let result = committed_result(&first);
        assert_eq!(result["classified"], 1);
        let ids: Vec<String> = result["candidate_ids"]
            .as_array()
            .expect("candidate ids")
            .iter()
            .map(|id| id.as_str().expect("string id").to_owned())
            .collect();
        assert_eq!(ids.len(), 1, "one pending candidate is produced");
        assert_no_secret(&serde_json::to_string(&first).expect("receipt encodes"), secret);
        assert_eq!(count_rows(&scenario.provider, "candidates").await, 1);

        let row = sqlx::query("SELECT payload_digest, idempotency_key FROM idempotency_namespace")
            .fetch_one(scenario.provider.store().pool())
            .await
            .expect("dedup row");
        let payload_digest: String = row.get("payload_digest");
        assert_eq!(payload_digest.len(), 64, "the dedup row binds a digest");
        assert!(payload_digest.chars().all(|c| c.is_ascii_hexdigit()));
        assert_no_secret(&payload_digest, secret);

        // Identical retry: same key, same canonical payload.
        let replay = scenario
            .provider
            .capture(CLIENT, &request, now)
            .await
            .expect("identical replay answers");
        assert_eq!(receipt_id_of(&replay), receipt_id_of(&first));
        assert_eq!(committed_result(&replay), result);
        assert_eq!(
            count_rows(&scenario.provider, "candidates").await,
            1,
            "a replay never re-executes the effect"
        );

        // Same key, different payload digest.
        let conflicting = capture_request(
            &scenario.space_id,
            "Another sentence.",
            context_scope(CONTEXT_A),
            "cap-1",
        );
        let conflict = refusal(scenario.provider.capture(CLIENT, &conflicting, now).await);
        assert_eq!(conflict.code, SpokeRejectCode::IdempotencyConflict);
        assert_eq!(count_rows(&scenario.provider, "candidates").await, 1);

        // A new key is a new admission.
        let second = capture_request(
            &scenario.space_id,
            "Another sentence.",
            context_scope(CONTEXT_A),
            "cap-2",
        );
        scenario
            .provider
            .capture(CLIENT, &second, now)
            .await
            .expect("second capture commits");
        assert_eq!(count_rows(&scenario.provider, "candidates").await, 2);
    }

    #[tokio::test]
    async fn replay_skips_current_freeze_but_not_revoked_authorization() {
        let scenario = Scenario::open().await;
        let now = now();
        scenario.subject(now).await;
        let grant_id = scenario
            .grant(CLIENT, &["capture"], Some(SUBJECT), None, now)
            .await;
        let request = capture_request(&scenario.space_id, "Kept draft.", context_scope(CONTEXT_A), "cap-1");
        let committed = scenario
            .provider
            .capture(CLIENT, &request, now)
            .await
            .expect("capture commits");

        scenario.freeze(now).await;

        // C-036/MEM-R021: the stored receipt is returned without re-running the
        // freeze gate the op already passed.
        let replay = scenario
            .provider
            .capture(CLIENT, &request, now)
            .await
            .expect("an admitted replay survives a later freeze");
        assert_eq!(receipt_id_of(&replay), receipt_id_of(&committed));

        // A new admission in the frozen space is refused, with no effect.
        let fresh = capture_request(&scenario.space_id, "After freeze.", context_scope(CONTEXT_A), "cap-2");
        assert_eq!(
            refusal(scenario.provider.capture(CLIENT, &fresh, now).await).code,
            SpokeRejectCode::SpaceFrozen
        );
        assert_eq!(count_rows(&scenario.provider, "candidates").await, 1);

        // MEM-R021: receipt disclosure still requires current authorization.
        scenario.revoke(&grant_id, now).await;
        assert_eq!(
            refusal(scenario.provider.capture(CLIENT, &request, now).await).code,
            SpokeRejectCode::NotAuthorized
        );
        let refused_read = refusal(
            scenario
                .provider
                .receipts_get(CLIENT, &receipts_request(&scenario.space_id, receipt_id_of(&committed)), now)
                .await,
        );
        assert_eq!(refused_read.code, SpokeRejectCode::NotAuthorized);
        assert_eq!(count_rows(&scenario.provider, "receipts").await, 1);
    }

    #[tokio::test]
    async fn dismiss_consumes_once_and_replay_never_resurrects() {
        let scenario = Scenario::open().await;
        let now = now();
        scenario.subject(now).await;
        scenario
            .grant(
                CLIENT,
                &["capture", "review", "records_write"],
                Some(SUBJECT),
                None,
                now,
            )
            .await;
        let secret = "Bo keeps the ledger under the loose floorboard.";
        let captured = scenario
            .provider
            .capture(
                CLIENT,
                &capture_request(&scenario.space_id, secret, context_scope(CONTEXT_A), "cap-1"),
                now,
            )
            .await
            .expect("capture commits");
        let candidate_id = committed_result(&captured)["candidate_ids"][0]
            .as_str()
            .expect("candidate id")
            .to_owned();

        let dismissal = scenario
            .provider
            .candidates_dismiss(
                CLIENT,
                &dismiss_request(&scenario.space_id, &candidate_id, 1, "dis-1"),
                now,
            )
            .await
            .expect("dismiss commits");
        assert_eq!(
            committed_result(&dismissal),
            &json!({ "candidate_id": candidate_id, "outcome": "dismissed" })
        );
        assert_eq!(count_rows(&scenario.provider, "records").await, 0);

        let shell = scenario
            .provider
            .candidates_get(CLIENT, &get_request(&scenario.space_id, &candidate_id), now)
            .await
            .expect("consumed shell is readable");
        let consumed = consumed_candidate(&shell);
        assert_eq!(consumed.status, "consumed");
        assert_eq!(consumed.revision.get(), 2);
        assert_eq!(
            serde_json::to_value(&consumed.consumed_by.outcome).expect("outcome encodes"),
            json!("dismissed")
        );
        assert_eq!(consumed.content_digest.len(), 64);
        assert!(consumed.proposed.is_some(), "the shell keeps its kind");
        assert_no_secret(&serde_json::to_string(&shell).expect("shell encodes"), secret);

        let stored: Option<String> =
            sqlx::query("SELECT content_json FROM candidates WHERE candidate_id = ?")
                .bind(&candidate_id)
                .fetch_one(scenario.provider.store().pool())
                .await
                .expect("candidate row")
                .get("content_json");
        assert!(stored.is_none(), "consumption removes the stored body");

        // Consumption is terminal and outranks the CAS precondition.
        assert_eq!(
            refusal(
                scenario
                    .provider
                    .candidates_dismiss(
                        CLIENT,
                        &dismiss_request(&scenario.space_id, &candidate_id, 2, "dis-2"),
                        now,
                    )
                    .await
            )
            .code,
            SpokeRejectCode::CandidateConsumed
        );
        assert_eq!(
            refusal(
                scenario
                    .provider
                    .candidates_review(
                        CLIENT,
                        &review_request(
                            &scenario.space_id,
                            &candidate_id,
                            2,
                            json!({ "action": "promote" }),
                            "rev-1",
                        ),
                        now,
                    )
                    .await
            )
            .code,
            SpokeRejectCode::CandidateConsumed
        );

        // The dismissal itself is replayable and never resurrects the body.
        let replay = scenario
            .provider
            .candidates_dismiss(
                CLIENT,
                &dismiss_request(&scenario.space_id, &candidate_id, 1, "dis-1"),
                now,
            )
            .await
            .expect("dismissal replay answers");
        assert_eq!(receipt_id_of(&replay), receipt_id_of(&dismissal));

        let consumed_page = scenario
            .provider
            .candidates_list(CLIENT, &list_request(&scenario.space_id, Some("consumed"), None, None), now)
            .await
            .expect("consumed page");
        assert_eq!(candidate_ids(&consumed_page), vec![candidate_id.clone()]);
        let pending_page = scenario
            .provider
            .candidates_list(CLIENT, &list_request(&scenario.space_id, Some("pending"), None, None), now)
            .await
            .expect("pending page");
        assert!(
            pending_page.candidates.is_empty(),
            "the consumed shell never returns to the queue"
        );
    }

    #[tokio::test]
    async fn review_promote_creates_the_record_in_the_consumed_scope_atomically() {
        let scenario = Scenario::open().await;
        let now = now();
        scenario.subject(now).await;
        scenario
            .grant(
                CLIENT,
                &["capture", "review"],
                Some(SUBJECT),
                None,
                now,
            )
            .await;
        scenario
            .grant(OTHER, &["records_write"], Some(SUBJECT), None, now)
            .await;
        let captured_text = "The gate key is under the third barrel.";
        let captured = scenario
            .provider
            .capture(
                CLIENT,
                &capture_request(&scenario.space_id, captured_text, context_scope(CONTEXT_A), "cap-1"),
                now,
            )
            .await
            .expect("capture commits");
        let candidate_id = committed_result(&captured)["candidate_ids"][0]
            .as_str()
            .expect("candidate id")
            .to_owned();

        // Two disjoint grants never union: review alone is not enough.
        let split = refusal(
            scenario
                .provider
                .candidates_review(
                    CLIENT,
                    &review_request(
                        &scenario.space_id,
                        &candidate_id,
                        1,
                        json!({ "action": "promote" }),
                        "rev-1",
                    ),
                    now,
                )
                .await,
        );
        assert_eq!(split.code, SpokeRejectCode::NotAuthorized);
        assert_eq!(
            pending_candidate(
                &scenario
                    .provider
                    .candidates_get(CLIENT, &get_request(&scenario.space_id, &candidate_id), now)
                    .await
                    .expect("pending candidate")
            )
            .revision
            .get(),
            1,
            "a refused review leaves the candidate untouched"
        );

        let review_grant = scenario
            .grant(
                CLIENT,
                &["review", "records_write"],
                Some(SUBJECT),
                None,
                now,
            )
            .await;
        let _ = review_grant;
        let reviewed = scenario
            .provider
            .candidates_review(
                CLIENT,
                &review_request(
                    &scenario.space_id,
                    &candidate_id,
                    1,
                    json!({
                        "action": "promote",
                        "kind": "event",
                        "content_override": { "text": "The key was revealed." },
                    }),
                    "rev-2",
                ),
                now,
            )
            .await
            .expect("review commits");
        let result = committed_result(&reviewed);
        assert_eq!(result["outcome"], "promoted");
        let record_ids: Vec<String> = result["record_ids"]
            .as_array()
            .expect("record ids")
            .iter()
            .map(|id| id.as_str().expect("string id").to_owned())
            .collect();
        assert_eq!(record_ids.len(), 1);
        assert_no_secret(
            &serde_json::to_string(&reviewed).expect("receipt encodes"),
            captured_text,
        );

        let record = sqlx::query(
            "SELECT kind, content_json, scope_visibility, scope_context_id, provenance_json, \
             revision FROM records WHERE record_id = ?",
        )
        .bind(&record_ids[0])
        .fetch_one(scenario.provider.store().pool())
        .await
        .expect("created record");
        assert_eq!(record.get::<String, _>("kind"), "event");
        assert_eq!(record.get::<String, _>("scope_visibility"), "context");
        assert_eq!(record.get::<String, _>("scope_context_id"), CONTEXT_A);
        assert_eq!(record.get::<i64, _>("revision"), 1);
        assert_eq!(
            serde_json::from_str::<Value>(&record.get::<String, _>("content_json"))
                .expect("record content"),
            json!({ "text": "The key was revealed." })
        );
        let provenance: Value =
            serde_json::from_str(&record.get::<String, _>("provenance_json")).expect("provenance");
        assert_eq!(provenance["captured_by"], json!(CLIENT));

        let shell = scenario
            .provider
            .candidates_get(CLIENT, &get_request(&scenario.space_id, &candidate_id), now)
            .await
            .expect("consumed shell");
        let consumed = consumed_candidate(&shell);
        assert_eq!(
            serde_json::to_value(&consumed.consumed_by.outcome).expect("outcome encodes"),
            json!("promoted")
        );
        assert_eq!(
            consumed.consumed_by.record_ids,
            vec![record_ids[0].clone()]
        );
        assert_eq!(
            consumed.content_digest.as_str(),
            digest(&json!({ "text": captured_text })).expect("digest"),
            "the shell keeps the digest of the scrubbed body"
        );
        assert_no_secret(&serde_json::to_string(&shell).expect("shell encodes"), captured_text);
    }

    #[tokio::test]
    async fn review_cannot_escape_the_admitted_capture_scope() {
        let scenario = Scenario::open().await;
        let now = now();
        scenario.subject(now).await;
        scenario
            .grant(CLIENT, &["capture"], Some(SUBJECT), None, now)
            .await;
        scenario
            .grant(
                CLIENT,
                &["review", "records_write"],
                Some(SUBJECT),
                Some((&[CONTEXT_A], false)),
                now,
            )
            .await;
        let captured = scenario
            .provider
            .capture(
                CLIENT,
                &capture_request(&scenario.space_id, "Shared world fact.", shared_scope(), "cap-1"),
                now,
            )
            .await
            .expect("shared capture commits");
        let candidate_id = committed_result(&captured)["candidate_ids"][0]
            .as_str()
            .expect("candidate id")
            .to_owned();

        // A context-restricted grant without shared access cannot see or review
        // a shared-placement candidate.
        let page = scenario
            .provider
            .candidates_list(CLIENT, &list_request(&scenario.space_id, None, None, None), now)
            .await
            .expect("scoped page");
        assert!(
            page.candidates.is_empty(),
            "a restricted review grant never sees a shared placement"
        );
        assert_eq!(
            refusal(
                scenario
                    .provider
                    .candidates_review(
                        CLIENT,
                        &review_request(
                            &scenario.space_id,
                            &candidate_id,
                            1,
                            json!({ "action": "promote" }),
                            "rev-1",
                        ),
                        now,
                    )
                    .await
            )
            .code,
            SpokeRejectCode::NotAuthorized
        );

        scenario
            .grant(
                CLIENT,
                &["review", "records_write"],
                Some(SUBJECT),
                Some((&[CONTEXT_A], true)),
                now,
            )
            .await;
        let reviewed = scenario
            .provider
            .candidates_review(
                CLIENT,
                &review_request(
                    &scenario.space_id,
                    &candidate_id,
                    1,
                    json!({ "action": "promote" }),
                    "rev-2",
                ),
                now,
            )
            .await
            .expect("review commits");
        let record_id = committed_result(&reviewed)["record_ids"][0]
            .as_str()
            .expect("record id")
            .to_owned();
        let record = sqlx::query("SELECT scope_visibility, scope_context_id FROM records WHERE record_id = ?")
            .bind(&record_id)
            .fetch_one(scenario.provider.store().pool())
            .await
            .expect("created record");
        assert_eq!(record.get::<String, _>("scope_visibility"), "shared");
        assert!(
            record
                .get::<Option<String>, _>("scope_context_id")
                .is_none(),
            "the record inherits the shared capture scope verbatim"
        );
    }

    #[tokio::test]
    async fn concurrent_review_has_exactly_one_effect() {
        let scenario = Scenario::open().await;
        let now = now();
        scenario.subject(now).await;
        scenario
            .grant(
                CLIENT,
                &["capture", "review", "records_write"],
                Some(SUBJECT),
                None,
                now,
            )
            .await;
        let captured = scenario
            .provider
            .capture(
                CLIENT,
                &capture_request(&scenario.space_id, "Contended capture.", context_scope(CONTEXT_A), "cap-1"),
                now,
            )
            .await
            .expect("capture commits");
        let candidate_id = committed_result(&captured)["candidate_ids"][0]
            .as_str()
            .expect("candidate id")
            .to_owned();

        let first = review_request(
            &scenario.space_id,
            &candidate_id,
            1,
            json!({ "action": "promote", "kind": "note" }),
            "rev-a",
        );
        let second = review_request(
            &scenario.space_id,
            &candidate_id,
            1,
            json!({ "action": "promote", "kind": "note" }),
            "rev-b",
        );
        let provider = Arc::new(scenario.provider);
        let (left, right) = tokio::join!(
            provider.candidates_review(CLIENT, &first, now),
            provider.candidates_review(CLIENT, &second, now)
        );
        let mut committed = 0;
        let mut refused = Vec::new();
        for outcome in [left, right] {
            match outcome {
                Ok(_) => committed += 1,
                Err(reject) => refused.push(reject),
            }
        }
        assert_eq!(committed, 1, "exactly one review commits");
        assert_eq!(refused.len(), 1);
        assert_eq!(refused[0].code, SpokeRejectCode::CandidateConsumed);
        assert_eq!(count_rows(&provider, "records").await, 1);
        // The capture receipt and the one committed review receipt.
        assert_eq!(count_rows(&provider, "receipts").await, 2);
    }

    #[tokio::test]
    async fn receipts_get_rechecks_authorization_and_reports_evicted_bodies() {
        let scenario = Scenario::open().await;
        let now = now();
        scenario.subject(now).await;
        scenario
            .grant(CLIENT, &["capture"], Some(SUBJECT), None, now)
            .await;
        let captured = scenario
            .provider
            .capture(
                CLIENT,
                &capture_request(&scenario.space_id, "Receipt subject text.", shared_scope(), "cap-1"),
                now,
            )
            .await
            .expect("capture commits");
        let receipt_id = receipt_id_of(&captured).to_owned();

        let read = scenario
            .provider
            .receipts_get(CLIENT, &receipts_request(&scenario.space_id, &receipt_id), now)
            .await
            .expect("the requester reads its own receipt");
        assert_eq!(receipt_status(&read), "committed");

        // A foreign principal without admin coverage cannot tell this receipt
        // from a missing one.
        let foreign = refusal(
            scenario
                .provider
                .receipts_get(OTHER, &receipts_request(&scenario.space_id, &receipt_id), now)
                .await,
        );
        let missing = refusal(
            scenario
                .provider
                .receipts_get(
                    OTHER,
                    &receipts_request(&scenario.space_id, "rcpt_absent"),
                    now,
                )
                .await,
        );
        assert_eq!(foreign.code, SpokeRejectCode::NotFound);
        assert_eq!(missing.code, SpokeRejectCode::NotFound);
        assert_eq!(foreign.message, missing.message);

        // The space admin may read another principal's receipt.
        let admin_read = scenario
            .provider
            .receipts_get(OPERATOR, &receipts_request(&scenario.space_id, &receipt_id), now)
            .await
            .expect("admin reads the receipt");
        assert_eq!(receipt_id_of(&admin_read), receipt_id);

        // Retention eviction belongs to the fault slice; the read path must
        // still resolve the dedup tombstone once the body is gone.
        sqlx::query("DELETE FROM receipts WHERE receipt_id = ?")
            .bind(&receipt_id)
            .execute(scenario.provider.store().pool())
            .await
            .expect("receipt body evicted");
        let expired = refusal(
            scenario
                .provider
                .receipts_get(CLIENT, &receipts_request(&scenario.space_id, &receipt_id), now)
                .await,
        );
        assert_eq!(expired.code, SpokeRejectCode::ReceiptExpired);
        assert_eq!(
            expired
                .details
                .as_ref()
                .and_then(|details| details.get("last_known_status")),
            Some(&json!("committed"))
        );
    }

    #[tokio::test]
    async fn pending_and_failed_receipts_settle_durably_and_stay_content_free() {
        let scenario = Scenario::open().await;
        let now = now();
        scenario.subject(now).await;
        scenario
            .grant(CLIENT, &["capture"], Some(SUBJECT), None, now)
            .await;
        let space = scenario.space_id.clone();
        let pending_admission = Mutation::new(
            OP_CAPTURE,
            "async-1",
            &json!({ "space_id": space, "probe": "pending" }),
        )
        .expect("admission");

        let mut transaction = scenario
            .provider
            .store()
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .expect("writer transaction");
        let staged = stage_receipt_in_tx(
            &mut transaction,
            &space,
            CLIENT,
            &pending_admission,
            "rcpt_pending_probe",
            ReceiptState::Pending,
            now,
        )
        .await
        .expect("pending receipt staged");
        transaction.commit().await.expect("pending receipt commits");
        assert_eq!(receipt_status(&staged), "pending");

        let pending_read = scenario
            .provider
            .receipts_get(
                CLIENT,
                &receipts_request(&space, "rcpt_pending_probe"),
                now,
            )
            .await
            .expect("pending receipt is durable");
        assert_eq!(receipt_status(&pending_read), "pending");
        // The drain barrier counts the durable pending receipt (MEM-R023).
        let spaces: spoke_operations::spoke_schemas::MemorySpacesListRequest =
            serde_json::from_value(json!({})).expect("valid spaces.list");
        let page = scenario
            .provider
            .spaces_list(OPERATOR, &spaces, now)
            .await
            .expect("spaces.list");
        assert_eq!(page.spaces[0].pending_receipts, 1);

        let mut transaction = scenario
            .provider
            .store()
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .expect("writer transaction");
        let settled = settle_receipt_in_tx(
            &mut transaction,
            &space,
            "rcpt_pending_probe",
            ReceiptState::Committed(json!({ "candidate_ids": [], "classified": 0 })),
            now,
        )
        .await
        .expect("pending receipt settles");
        transaction.commit().await.expect("settlement commits");
        assert_eq!(receipt_status(&settled), "committed");
        assert_eq!(
            committed_result(&settled),
            &json!({ "candidate_ids": [], "classified": 0 })
        );
        let page = scenario
            .provider
            .spaces_list(OPERATOR, &spaces, now)
            .await
            .expect("spaces.list");
        assert_eq!(
            page.spaces[0].pending_receipts, 0,
            "a settled receipt leaves the drain barrier"
        );
        assert_eq!(tombstone(&scenario, "async-1").await.as_deref(), Some("committed"));

        // A failed asynchronous receipt keeps a coded envelope with its retry
        // class and no result at all.
        let failed_admission = Mutation::new(
            OP_CAPTURE,
            "async-2",
            &json!({ "space_id": space, "probe": "failed" }),
        )
        .expect("admission");
        let mut transaction = scenario
            .provider
            .store()
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .expect("writer transaction");
        let pending = stage_receipt_in_tx(
            &mut transaction,
            &space,
            CLIENT,
            &failed_admission,
            "rcpt_failed_probe",
            ReceiptState::Pending,
            now,
        )
        .await
        .expect("pending receipt staged");
        assert_eq!(receipt_status(&pending), "pending");
        let failed = settle_receipt_in_tx(
            &mut transaction,
            &space,
            "rcpt_failed_probe",
            ReceiptState::Failed(reject(
                SpokeRejectCode::NotFound,
                "the generation input no longer exists",
            )),
            now,
        )
        .await
        .expect("pending receipt fails");
        transaction.commit().await.expect("failure commits");
        match &failed {
            MemoryReceipt::FailedReceipt(receipt) => {
                assert_eq!(receipt.status, "failed");
                let envelope = serde_json::to_value(&receipt.error).expect("envelope encodes");
                assert_eq!(envelope["code"], json!("NOT_FOUND"));
                assert_eq!(envelope["details"]["retry_class"], json!("terminal"));
                assert_eq!(*receipt.completed_at, receipt.created_at);
            }
            other => panic!("expected a failed receipt, observed {other:?}"),
        }

        // Replay answers the stored receipt whatever its state (§14.3).
        let mut transaction = scenario
            .provider
            .store()
            .pool()
            .begin()
            .await
            .expect("reader transaction");
        let replayed = resolve_replay(&mut transaction, &space, CLIENT, &failed_admission)
            .await
            .expect("replay resolves")
            .expect("the failed admission is bound");
        transaction.commit().await.expect("read commits");
        assert_eq!(receipt_id_of(&replayed), "rcpt_failed_probe");
        assert_eq!(receipt_status(&replayed), "failed");
        assert_eq!(tombstone(&scenario, "async-2").await.as_deref(), Some("failed"));

        // Both receipts and the dedup tombstones survive a reopen.
        let scenario = scenario.reopen().await;
        let reopened = scenario
            .provider
            .receipts_get(
                CLIENT,
                &receipts_request(&scenario.space_id, "rcpt_failed_probe"),
                now,
            )
            .await
            .expect("failed receipt survives reopen");
        assert_eq!(receipt_status(&reopened), "failed");
        let reopened_pending = scenario
            .provider
            .receipts_get(
                CLIENT,
                &receipts_request(&scenario.space_id, "rcpt_pending_probe"),
                now,
            )
            .await
            .expect("settled receipt survives reopen");
        assert_eq!(receipt_status(&reopened_pending), "committed");
    }

    #[tokio::test]
    async fn candidates_list_pages_respect_scope_cursor_and_expiry() {
        let scenario = Scenario::open().await;
        let now = now();
        scenario.subject(now).await;
        scenario
            .grant(CLIENT, &["capture"], Some(SUBJECT), None, now)
            .await;
        scenario
            .grant(
                CLIENT,
                &["review"],
                Some(SUBJECT),
                Some((&[CONTEXT_A], false)),
                now,
            )
            .await;
        for (index, context_id) in [CONTEXT_A, CONTEXT_A, CONTEXT_A, CONTEXT_B]
            .into_iter()
            .enumerate()
        {
            scenario
                .provider
                .capture(
                    CLIENT,
                    &capture_request(
                        &scenario.space_id,
                        &format!("Material {index}"),
                        context_scope(context_id),
                        &format!("cap-{index}"),
                    ),
                    now,
                )
                .await
                .expect("capture commits");
        }

        let first = scenario
            .provider
            .candidates_list(CLIENT, &list_request(&scenario.space_id, None, Some(2), None), now)
            .await
            .expect("first page");
        assert_eq!(first.candidates.len(), 2);
        let cursor = first.next_cursor.clone().expect("a bounded page issues a cursor");
        for candidate in &first.candidates {
            assert_eq!(
                serde_json::to_value(&pending_candidate(candidate).capture_scope)
                    .expect("scope encodes"),
                context_scope(CONTEXT_A)
            );
        }
        let second = scenario
            .provider
            .candidates_list(
                CLIENT,
                &list_request(&scenario.space_id, None, Some(2), Some(&cursor)),
                now,
            )
            .await
            .expect("second page");
        assert_eq!(second.candidates.len(), 1);
        assert!(second.next_cursor.is_none());
        let mut union = candidate_ids(&first);
        union.extend(candidate_ids(&second));
        union.sort();
        union.dedup();
        assert_eq!(
            union.len(),
            3,
            "the pages cover exactly the ctx-a candidates, never the ctx-b row"
        );

        // Any observable state change invalidates the outstanding cursor.
        scenario
            .provider
            .candidates_dismiss(
                CLIENT,
                &dismiss_request(&scenario.space_id, &union[0], 1, "dis-1"),
                now,
            )
            .await
            .expect("dismiss commits");
        assert_eq!(
            refusal(
                scenario
                    .provider
                    .candidates_list(
                        CLIENT,
                        &list_request(&scenario.space_id, None, Some(2), Some(&cursor)),
                        now,
                    )
                    .await
            )
            .code,
            SpokeRejectCode::CursorInvalid
        );

        // Pending shells expire out of the list; consumed shells stay for audit.
        let expired_at = later(CANDIDATE_TTL_DAYS + 1);
        let expired = scenario
            .provider
            .candidates_list(CLIENT, &list_request(&scenario.space_id, None, None, None), expired_at)
            .await
            .expect("expired page");
        assert_eq!(candidate_ids(&expired), vec![union[0].clone()]);
        let expired_pending = scenario
            .provider
            .candidates_list(
                CLIENT,
                &list_request(&scenario.space_id, Some("pending"), None, None),
                expired_at,
            )
            .await
            .expect("expired pending page");
        assert!(expired_pending.candidates.is_empty());
    }

    #[tokio::test]
    async fn capture_refusals_persist_nothing() {
        let scenario = Scenario::open().await;
        let now = now();
        scenario.subject(now).await;

        let ungranted = capture_request(&scenario.space_id, "Ungranted text.", context_scope(CONTEXT_A), "cap-1");
        assert_eq!(
            refusal(scenario.provider.capture(CLIENT, &ungranted, now).await).code,
            SpokeRejectCode::NotAuthorized
        );
        assert_eq!(count_rows(&scenario.provider, "candidates").await, 0);
        assert_eq!(count_rows(&scenario.provider, "receipts").await, 0);
        assert_eq!(
            count_rows(&scenario.provider, "idempotency_namespace").await,
            0
        );

        // A space-wide capture grant makes the subject-existence answer real.
        scenario
            .grant(CLIENT, &["capture"], None, None, now)
            .await;
        let foreign: MemoryCaptureRequest = serde_json::from_value(json!({
            "space_id": scenario.space_id,
            "subject_id": "subject-absent",
            "content": { "text": "No subject." },
            "capture_scope": context_scope(CONTEXT_A),
            "idempotency_key": "cap-absent",
        }))
        .expect("valid capture request");
        assert_eq!(
            refusal(scenario.provider.capture(CLIENT, &foreign, now).await).code,
            SpokeRejectCode::NotFound
        );

        let oversize = capture_request(&scenario.space_id, &"x".repeat(MAX_CONTENT_BYTES + 1), context_scope(CONTEXT_A), "cap-big");
        assert_eq!(
            refusal(scenario.provider.capture(CLIENT, &oversize, now).await).code,
            SpokeRejectCode::InvalidInput
        );

        scenario.fence_context(CONTEXT_A, now).await;
        let fenced = capture_request(&scenario.space_id, "Fenced text.", context_scope(CONTEXT_A), "cap-fenced");
        let fenced_refusal = refusal(scenario.provider.capture(CLIENT, &fenced, now).await);
        assert_eq!(fenced_refusal.code, SpokeRejectCode::SubjectFenced);
        assert_eq!(
            fenced_refusal
                .details
                .as_ref()
                .and_then(|details| details.get("scope")),
            Some(&json!("context"))
        );

        // A sibling context is still admitted.
        scenario
            .provider
            .capture(
                CLIENT,
                &capture_request(&scenario.space_id, "Sibling text.", context_scope(CONTEXT_B), "cap-sibling"),
                now,
            )
            .await
            .expect("the sibling context is admitted");
        assert_eq!(count_rows(&scenario.provider, "candidates").await, 1);
        assert_eq!(count_rows(&scenario.provider, "receipts").await, 1);

        scenario.freeze(now).await;
        let frozen = capture_request(&scenario.space_id, "Frozen text.", context_scope(CONTEXT_B), "cap-frozen");
        assert_eq!(
            refusal(scenario.provider.capture(CLIENT, &frozen, now).await).code,
            SpokeRejectCode::SpaceFrozen
        );
        assert_eq!(count_rows(&scenario.provider, "candidates").await, 1);
        assert_eq!(count_rows(&scenario.provider, "receipts").await, 1);
    }

    #[tokio::test]
    async fn receipts_and_shells_never_leak_deleted_content() {
        let scenario = Scenario::open().await;
        let now = now();
        scenario.subject(now).await;
        scenario
            .grant(
                CLIENT,
                &["capture", "review", "records_write"],
                Some(SUBJECT),
                None,
                now,
            )
            .await;
        let secret = "Only the archivist knows the vault code.";
        let captured = scenario
            .provider
            .capture(
                CLIENT,
                &capture_request(&scenario.space_id, secret, context_scope(CONTEXT_A), "cap-1"),
                now,
            )
            .await
            .expect("capture commits");
        let candidate_id = committed_result(&captured)["candidate_ids"][0]
            .as_str()
            .expect("candidate id")
            .to_owned();
        let reviewed = scenario
            .provider
            .candidates_review(
                CLIENT,
                &review_request(
                    &scenario.space_id,
                    &candidate_id,
                    1,
                    json!({ "action": "promote" }),
                    "rev-1",
                ),
                now,
            )
            .await
            .expect("review commits");
        let record_id = committed_result(&reviewed)["record_ids"][0]
            .as_str()
            .expect("record id")
            .to_owned();

        // The record deletion op belongs to the records slice; the receipt and
        // dedup metadata must already be content-free before it happens, and
        // must stay so afterwards.
        sqlx::query("DELETE FROM records WHERE record_id = ?")
            .bind(&record_id)
            .execute(scenario.provider.store().pool())
            .await
            .expect("record deleted");

        let receipt = scenario
            .provider
            .receipts_get(
                CLIENT,
                &receipts_request(&scenario.space_id, receipt_id_of(&reviewed)),
                now,
            )
            .await
            .expect("receipt still resolves");
        assert_no_secret(&serde_json::to_string(&receipt).expect("receipt encodes"), secret);
        let shell = scenario
            .provider
            .candidates_get(CLIENT, &get_request(&scenario.space_id, &candidate_id), now)
            .await
            .expect("shell still resolves");
        assert_no_secret(&serde_json::to_string(&shell).expect("shell encodes"), secret);

        // The dedup namespace as a whole carries no captured text either.
        let rows = sqlx::query("SELECT payload_digest, idempotency_key FROM idempotency_namespace")
            .fetch_all(scenario.provider.store().pool())
            .await
            .expect("dedup rows");
        for row in rows {
            let payload_digest: String = row.get("payload_digest");
            let idempotency_key: String = row.get("idempotency_key");
            assert_no_secret(&payload_digest, secret);
            assert_no_secret(&idempotency_key, secret);
        }
    }
}
