//! Capture, candidates, receipts and idempotency (spoke-memory §§6.7, 6.10,
//! 7.0.5, 7.7, 7.11–7.15, 7.28, 8.1, 8.5).
//!
//! Every mutating op takes an [`Admitted`] request — the exact payload object
//! received at the boundary plus the typed view parsed from it — so the dedup
//! digest is taken over the bytes the caller actually sent (§6.1/§7.0.5) and not
//! over a reserialization that would erase distinctions such as an explicit
//! empty array versus an omitted member.
//!
//! Every mutation is split into an outer convenience wrapper that owns one
//! `BEGIN IMMEDIATE` transaction and an `*_in_tx` sibling that stages the
//! authorized effect, the content-free receipt body and its dedup row on the
//! caller's connection without committing, so the assembly task can commit an
//! effect and its receipt exactly once (MEM-R009/R022). The shared receipt
//! writer also supports the §7.0.5 keyless mutation (`memory.subjects.ensure`):
//! no synthetic key, no dedup namespace row, never a replay.
//!
//! Replay is resolved inside that same transaction *after* current
//! authorization but *before* the freeze/subject-lifecycle/CAS gates: an
//! identical admitted replay returns the stored receipt without re-executing and
//! without re-evaluating superseded preconditions (MEM-R021), while a revoked
//! principal can neither replay nor read it (§7.28/C-104). Receipt disclosure
//! rechecks the authority the recorded operation required *before* it reveals a
//! body or an evicted receipt's tombstone status.
//!
//! By-id candidate refusals carry one indistinguishable `NOT_FOUND`, so candidate
//! ids are not an existence oracle (C-102 blind spot closed), and the subject
//! lifecycle is enforced where the protocol states it: a fenced or archived
//! subject rejects fresh writes, and an archived subject leaves the listing.
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
    content_serialized_bytes, grant_covers, is_freeze_allowlisted, jcs_sha256_hex,
    validate_memory_candidates_list, validate_memory_candidates_review, validate_memory_capture,
    GrantRequirement, SpokeReject, SpokeRejectCode, SpokeResult,
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

/// One refusal wording for both receipt-disclosure branches, so a caller can
/// never tell from the response whether a body or only its tombstone exists.
const RECEIPT_UNAUTHORIZED: &str =
    "the receipt requester holds no current authorization for this operation";

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
// Admitted request boundary (§6.1, §7.0.1, §7.0.5)
// ---------------------------------------------------------------------------

/// One request exactly as it was admitted at the transport boundary: the raw
/// payload object the caller sent plus the typed view parsed from *that same*
/// object.
///
/// `payload_digest` is defined as the JCS digest of the request payload object
/// (§6.1/§7.0.5), so the digest is taken over [`Self::payload`] and never over a
/// reserialization: members the typed model normalizes away — an explicit
/// `"source_context_ids": []` versus an omitted member, an explicit `null`
/// versus an absent optional — stay distinguishable exactly as the protocol
/// requires. Construction parses the payload, so the typed view can never
/// describe a different request than the one being digested.
pub(crate) struct Admitted<Request> {
    payload_digest: String,
    request: Request,
}

impl<Request> Admitted<Request> {
    /// The C7 request-scope bridge (D15): `payload_digest` was computed over the
    /// `Value` the transport actually received, *before* typed decode, under the
    /// per-request scope's matching op and provider identity. It is never a
    /// reserialization, a normalized fallback or a client-supplied value, and it
    /// is crate-private so no untrusted caller can pair a digest with a request
    /// it did not produce.
    pub(crate) fn from_scope(payload_digest: String, request: Request) -> Self {
        Self {
            payload_digest,
            request,
        }
    }

    /// The canonical `payload_digest` of the admitted object (§6.1/§7.0.5).
    pub(crate) fn payload_digest(&self) -> &str {
        &self.payload_digest
    }

    /// The typed view parsed from that same object at admission.
    pub(crate) fn request(&self) -> &Request {
        &self.request
    }
}

/// Admission of one received payload: hash the object once and move it into the
/// decoder, so the digest input and the executed request are the same bytes and
/// nothing is cloned or reserialized.
fn admit<Request>(
    payload: Value,
    parse: impl FnOnce(Value) -> Result<Request, serde_json::Error>,
) -> Result<Admitted<Request>, SpokeReject> {
    let payload_digest = digest(&payload)?;
    let request = parse(payload).map_err(invalid_payload)?;
    Ok(Admitted::from_scope(payload_digest, request))
}

/// §7.11 `memory.capture` at the boundary.
pub(crate) fn admit_capture(
    payload: Value,
) -> Result<Admitted<MemoryCaptureRequest>, SpokeReject> {
    admit(payload, serde_json::from_value)
}

/// §7.14 `memory.candidates.dismiss` at the boundary.
pub(crate) fn admit_candidates_dismiss(
    payload: Value,
) -> Result<Admitted<MemoryCandidatesDismissRequest>, SpokeReject> {
    admit(payload, serde_json::from_value)
}

/// §7.15 `memory.candidates.review` at the boundary.
pub(crate) fn admit_candidates_review(
    payload: Value,
) -> Result<Admitted<MemoryCandidatesReviewRequest>, SpokeReject> {
    admit(payload, serde_json::from_value)
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

/// §7.13–§7.15 by-id refusal (C-102): a candidate the caller may not act on and
/// a candidate id that does not exist must be indistinguishable, so the refusal
/// is the same `NOT_FOUND` and names only the requested id.
fn candidate_unavailable() -> SpokeReject {
    not_found("no candidate is available to the authenticated principal")
}

fn not_authorized(message: impl Into<String>) -> SpokeReject {
    reject(SpokeRejectCode::NotAuthorized, message)
}

fn not_found(message: impl Into<String>) -> SpokeReject {
    reject(SpokeRejectCode::NotFound, message)
}

/// A payload that is not the op's closed request object (§7.0.1): the caller's
/// fault, so `INVALID_INPUT` rather than an internal failure.
fn invalid_payload(error: impl std::fmt::Display) -> SpokeReject {
    reject_with(
        SpokeRejectCode::InvalidInput,
        format!("request payload is not a valid request object: {error}"),
        Map::from_iter([("field".to_owned(), Value::String("payload".to_owned()))]),
    )
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

/// A receipt target that cannot describe a real requirement (§6.5/§6.12).
fn invalid_target(message: impl Into<String>) -> SpokeReject {
    reject_with(
        SpokeRejectCode::InvalidInput,
        format!("invalid receipt target: {}", message.into()),
        Map::from_iter([(
            "field".to_owned(),
            Value::String("receipt_target".to_owned()),
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

/// §7.1/§7.15: every content object a review writes to a retained record is
/// measured with the published `content_serialized_bytes` — the same advertised
/// `max_content_bytes` measure the capture and record validators use — before
/// any effect. An oversized object fails `INVALID_INPUT` naming the limit.
fn require_review_content(field: &str, content: &Value) -> Result<(), SpokeReject> {
    if content_serialized_bytes(content) <= MAX_CONTENT_BYTES {
        return Ok(());
    }
    Err(reject_with(
        SpokeRejectCode::InvalidInput,
        format!("{field} exceeds the provider max_content_bytes ({MAX_CONTENT_BYTES})"),
        Map::from_iter([
            ("field".to_owned(), Value::String(field.to_owned())),
            (
                "limit".to_owned(),
                Value::String("max_content_bytes".to_owned()),
            ),
        ]),
    ))
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

/// §7.7: a subject-level fence or archive rejects every write for that subject.
async fn require_active_subject(
    conn: &mut SqliteConnection,
    space_id: &str,
    subject_id: &str,
) -> Result<(), SpokeReject> {
    let target = capture_target(conn, space_id, subject_id).await?;
    if target.status != "active" {
        return Err(subject_fenced(subject_id, "subject"));
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

/// §6.5/MEM-R006: one grant must cover every facet of every requirement. Two
/// grants never union, so an op that needs two capabilities needs one grant
/// carrying both.
fn one_grant_covers(
    grants: &[MemoryGrant],
    principal_id: &str,
    space_id: &str,
    now: DateTime<Utc>,
    requirements: &[GrantRequirement<'_>],
) -> bool {
    grants.iter().any(|grant| {
        requirements
            .iter()
            .all(|requirement| grant_covers(grant, principal_id, space_id, now, requirement))
    })
}

/// [`one_grant_covers`] as an explicit refusal, for ops whose target the caller
/// names itself (the capture placement, the list subject).
fn require_one_grant(
    grants: &[MemoryGrant],
    principal_id: &str,
    space_id: &str,
    now: DateTime<Utc>,
    requirements: &[GrantRequirement<'_>],
) -> Result<(), SpokeReject> {
    if one_grant_covers(grants, principal_id, space_id, now, requirements) {
        return Ok(());
    }
    Err(not_authorized(
        "the authenticated principal holds no single grant covering this operation's target",
    ))
}

/// The content-free authorization target of the operation that produced a
/// receipt (§6.5/§7.28): the capabilities it required and the subject and
/// placement/selection facets a current grant must actually cover.
///
/// It is durable beside the receipt and independent of the receipt body, so a
/// tombstone read rechecks the *original* target instead of inferring one from
/// whichever grant happens to survive. It never carries payload, content,
/// digest, seed or proof material.
#[derive(Debug)]
pub(crate) struct ReceiptTarget {
    capabilities: Vec<String>,
    subject_id: Option<String>,
    context_ids: Vec<String>,
    reads_shared: bool,
    subject_all: bool,
}

impl ReceiptTarget {
    /// Crate-visible construction for any §7 operation requirement: the
    /// capabilities that operation needed plus its complete subject and
    /// placement/selection facets.
    ///
    /// The facets are independent axes, and an **omitted** facet only means
    /// "this requirement names it" — never "the target is unrestricted":
    /// - `subject_id: None` names no subject, so any subject scope qualifies;
    ///   `Some(id)` requires coverage of exactly that subject.
    /// - `context_ids` names the placement set; an empty set names no context, so
    ///   a context-restricted grant still covers it (that is legitimate for ops
    ///   such as `memory.subjects.ensure`, which are not placement-scoped).
    /// - `reads_shared` records the shared-visibility facet.
    /// - `subject_all` is the published evaluator's *unrestricted-only* facet
    ///   (§6.12): it requires a grant with no context restriction. A whole-subject
    ///   selection needs it, and it is also the only way to require genuinely
    ///   unrestricted coverage — see [`Self::whole_space`] for the whole-space
    ///   requirement, which is **not** the same as an empty context list.
    ///
    /// A structurally impossible target is refused instead of stored, so no
    /// caller can stage a receipt with a permissive default: an empty or
    /// duplicated capability list, duplicated contexts, or a whole-subject
    /// selection that also names contexts.
    pub(crate) fn new(
        capabilities: &'static [&'static str],
        subject_id: Option<&str>,
        context_ids: &[&str],
        reads_shared: bool,
        subject_all: bool,
    ) -> Result<Self, SpokeReject> {
        if capabilities.is_empty() {
            return Err(invalid_target("at least one capability is required"));
        }
        let unique = capabilities.iter().collect::<std::collections::HashSet<_>>();
        if unique.len() != capabilities.len() {
            return Err(invalid_target("the capability list must be unique"));
        }
        let mut contexts: Vec<String> = Vec::with_capacity(context_ids.len());
        for context_id in context_ids {
            if contexts.iter().any(|existing| existing == context_id) {
                return Err(invalid_target("the context ids must be unique"));
            }
            contexts.push((*context_id).to_owned());
        }
        if subject_all && !contexts.is_empty() {
            return Err(invalid_target(
                "a whole-subject selection cannot name placement contexts",
            ));
        }
        Ok(Self {
            capabilities: capabilities.iter().map(|value| (*value).to_owned()).collect(),
            subject_id: subject_id.map(str::to_owned),
            context_ids: contexts,
            reads_shared,
            subject_all,
        })
    }

    /// §6.5 **whole-space** requirement: the complete published
    /// `whole_scope_requirement` shape — no subject, no named context, and the
    /// unrestricted-coverage facet — so a context- or shared-restricted grant
    /// never covers it.
    ///
    /// An empty context list on [`Self::new`] is deliberately *not* this: it only
    /// means the requirement names no context, which a context-restricted grant
    /// still satisfies. Use this constructor whenever the operation's real target
    /// is the whole space (whole-space export, space maintenance/admin ops).
    pub(crate) fn whole_space(
        capabilities: &'static [&'static str],
    ) -> Result<Self, SpokeReject> {
        Self::new(capabilities, None, &[], false, true)
    }

    /// Target of an op whose requirement is a placement scope: a `context`
    /// placement needs that context, a `shared` placement needs shared access.
    fn for_scope(
        capabilities: &'static [&'static str],
        subject_id: &str,
        visibility: &str,
        context_id: Option<&str>,
    ) -> Result<Self, SpokeReject> {
        match visibility {
            "context" => {
                let contexts: Vec<&str> = context_id.into_iter().collect();
                Self::new(capabilities, Some(subject_id), &contexts, false, false)
            }
            _ => Self::new(capabilities, Some(subject_id), &[], true, false),
        }
    }

    /// Target of an op that names only a subject and no placement facet, such
    /// as §7.5 `memory.subjects.ensure`.
    fn for_subject(
        capabilities: &'static [&'static str],
        subject_id: &str,
    ) -> Result<Self, SpokeReject> {
        Self::new(capabilities, Some(subject_id), &[], false, false)
    }

    /// §6.5/MEM-R006 single-grant coverage of the recorded target: one live
    /// grant must carry every required capability *and* cover the recorded
    /// subject and placement facets. Grants are never unioned, so an unrelated
    /// same-capability grant never reopens this receipt.
    fn admits(
        &self,
        grants: &[MemoryGrant],
        principal_id: &str,
        space_id: &str,
        now: DateTime<Utc>,
    ) -> bool {
        grants.iter().any(|grant| {
            self.capabilities.iter().all(|capability| {
                grant_covers(
                    grant,
                    principal_id,
                    space_id,
                    now,
                    &GrantRequirement {
                        capability,
                        subject_id: self.subject_id.as_deref(),
                        context_ids: self.context_ids.clone(),
                        reads_shared: self.reads_shared,
                        subject_all: self.subject_all,
                    },
                )
            })
        })
    }
}

async fn store_receipt_target_in_tx(
    conn: &mut SqliteConnection,
    receipt_id: &str,
    target: &ReceiptTarget,
) -> Result<(), SpokeReject> {
    sqlx::query(
        "INSERT INTO receipt_targets (receipt_id, capabilities_json, subject_id, \
         context_ids_json, reads_shared, subject_all) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(receipt_id)
    .bind(encoded(&json!(target.capabilities))?)
    .bind(target.subject_id.as_deref())
    .bind(encoded(&json!(target.context_ids))?)
    .bind(i64::from(target.reads_shared))
    .bind(i64::from(target.subject_all))
    .execute(&mut *conn)
    .await
    .map_err(internal)?;
    Ok(())
}

async fn fetch_receipt_target(
    conn: &mut SqliteConnection,
    receipt_id: &str,
) -> Result<Option<ReceiptTarget>, SpokeReject> {
    let row = sqlx::query(
        "SELECT capabilities_json, subject_id, context_ids_json, reads_shared, subject_all \
         FROM receipt_targets WHERE receipt_id = ?",
    )
    .bind(receipt_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(internal)?;
    row.map(|row| {
        let capabilities: String = row.try_get("capabilities_json").map_err(internal)?;
        let context_ids: String = row.try_get("context_ids_json").map_err(internal)?;
        let reads_shared: i64 = row.try_get("reads_shared").map_err(internal)?;
        let subject_all: i64 = row.try_get("subject_all").map_err(internal)?;
        Ok(ReceiptTarget {
            capabilities: serde_json::from_str(&capabilities).map_err(|error| {
                internal(format!("stored receipt capabilities are not JSON: {error}"))
            })?,
            subject_id: row.try_get("subject_id").map_err(internal)?,
            context_ids: serde_json::from_str(&context_ids).map_err(|error| {
                internal(format!("stored receipt contexts are not JSON: {error}"))
            })?,
            reads_shared: reads_shared != 0,
            subject_all: subject_all != 0,
        })
    })
    .transpose()
}

/// The capabilities each mutating op required, from the §7 grant tables; `None`
/// for an op string the published base table does not define. `admin` in a
/// grant implies every capability within that grant's coverage (§6.5), so a
/// grant that literally carries `admin` satisfies any requirement here.
pub(crate) fn required_capabilities(op: &str) -> Option<&'static [&'static str]> {
    const ADMIN: &[&str] = &["admin"];
    const CAPTURE: &[&str] = &["capture"];
    const REVIEW: &[&str] = &["review"];
    const REVIEW_WRITE: &[&str] = &["review", "records_write"];
    const RECORDS_WRITE: &[&str] = &["records_write"];
    const PROMOTE: &[&str] = &["promote", "records_write"];
    const PROFILE_WRITE: &[&str] = &["profile_write"];
    const REFLECT: &[&str] = &["reflect"];
    const TRANSFER: &[&str] = &["transfer"];
    const TRANSFER_ADMIN: &[&str] = &["transfer", "admin"];
    Some(match op {
        "memory.spaces.freeze"
        | "memory.spaces.unfreeze"
        | "memory.subjects.fence"
        | "memory.grants.grant"
        | "memory.grants.revoke" => ADMIN,
        "memory.subjects.ensure" | "memory.capture" => CAPTURE,
        "memory.candidates.dismiss" => REVIEW,
        "memory.candidates.review" => REVIEW_WRITE,
        "memory.records.create" | "memory.records.update" | "memory.records.delete" => {
            RECORDS_WRITE
        }
        "memory.records.promote" => PROMOTE,
        "memory.profile.update" => PROFILE_WRITE,
        "memory.reflections.generate" => REFLECT,
        "memory.exports.create" | "memory.exports.get" => TRANSFER,
        "memory.imports.stage" | "memory.imports.activate" | "memory.imports.discard" => {
            TRANSFER_ADMIN
        }
        _ => return None,
    })
}

/// §6.5 treats an expired grant exactly as a revoked one, so the cursor must
/// bind the caller's **effective** authorization at the supplied instant, not
/// only the persistent marker: a grant that expires between two pages changes
/// this fingerprint without any row changing, and the next page is refused
/// instead of being silently re-filtered (MEM-R026/§7.0.7).
fn effective_authorization_digest(
    grants: &[MemoryGrant],
    principal_id: &str,
    space_id: &str,
    now: DateTime<Utc>,
) -> Result<String, SpokeReject> {
    let mut live = Vec::new();
    for grant in grants {
        let (grant_id, grant_principal, grant_space, subject_id, capabilities, context_ids, shared_access, expires_at, revoked_at) =
            match grant {
                MemoryGrant::RestrictedGrant(grant) => (
                    grant.grant_id.as_str(),
                    grant.principal_id.as_str(),
                    grant.space_id.as_str(),
                    grant.subject_id.as_deref(),
                    &grant.capabilities,
                    Some(&grant.context_ids),
                    grant.shared_access,
                    grant.expires_at.as_deref(),
                    grant.revoked_at.is_some(),
                ),
                MemoryGrant::UnrestrictedGrant(grant) => (
                    grant.grant_id.as_str(),
                    grant.principal_id.as_str(),
                    grant.space_id.as_str(),
                    grant.subject_id.as_deref(),
                    &grant.capabilities,
                    None,
                    None,
                    grant.expires_at.as_deref(),
                    grant.revoked_at.is_some(),
                ),
            };
        if grant_principal != principal_id || grant_space != space_id || revoked_at {
            continue;
        }
        if expires_at.is_some_and(|expires| *expires <= now) {
            continue;
        }
        live.push(json!({
            "grant_id": grant_id,
            "subject_id": subject_id,
            "capabilities": capabilities,
            "context_ids": context_ids,
            "shared_access": shared_access,
        }));
    }
    // One canonical order: the row order of `load_grants` already sorts by
    // grant id, but the digest must not depend on it.
    live.sort_by(|left, right| {
        left["grant_id"]
            .as_str()
            .unwrap_or_default()
            .cmp(right["grant_id"].as_str().unwrap_or_default())
    });
    digest(&Value::Array(live))
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
    idempotency_key: Option<String>,
    payload_digest: String,
}

impl Mutation {
    /// Keyed dedup identity (§7.0.5). The digest is read from the admitted
    /// request, so it is always the digest of the object the caller actually
    /// sent — never a typed reserialization.
    pub(crate) fn keyed<Request>(
        op: &'static str,
        idempotency_key: &str,
        admitted: &Admitted<Request>,
    ) -> Self {
        Self {
            op,
            idempotency_key: Some(idempotency_key.to_owned()),
            payload_digest: admitted.payload_digest().to_owned(),
        }
    }

    /// The §7.0.5 sole keyless mutation (`memory.subjects.ensure`): it binds no
    /// dedup namespace row, is never a replay, and still carries the admitted
    /// `payload_digest` on its receipt.
    pub(crate) fn keyless<Request>(op: &'static str, admitted: &Admitted<Request>) -> Self {
        Self {
            op,
            idempotency_key: None,
            payload_digest: admitted.payload_digest().to_owned(),
        }
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
#[allow(clippy::too_many_arguments)]
pub(crate) async fn stage_receipt_in_tx(
    conn: &mut SqliteConnection,
    space_id: &str,
    principal_id: &str,
    admission: &Mutation,
    receipt_id: &str,
    target: &ReceiptTarget,
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
    .bind(admission.idempotency_key.as_deref())
    .bind(&admission.payload_digest)
    .bind(status)
    .bind(result)
    .bind(error)
    .bind(timestamp_text(now))
    .bind(completed_at)
    .execute(&mut *conn)
    .await
    .map_err(internal)?;
    // §7.0.5: the keyless mutation is the sole exemption and binds no dedup
    // namespace row, so nothing can ever replay it.
    if let Some(idempotency_key) = admission.idempotency_key.as_deref() {
        sqlx::query(
            "INSERT INTO idempotency_namespace (space_id, requester_principal_id, op, \
             idempotency_key, payload_digest, receipt_id, last_known_status, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(space_id)
        .bind(principal_id)
        .bind(admission.op)
        .bind(idempotency_key)
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
    }
    // The target is written with the receipt and independent of its body, so an
    // evicted body's tombstone read still rechecks the original scope.
    store_receipt_target_in_tx(conn, receipt_id, target).await?;
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
    let Some(idempotency_key) = admission.idempotency_key.as_deref() else {
        // §7.0.5 sole keyless exemption: no namespace row exists, so there is
        // nothing to replay and the op is admitted like a first attempt.
        return Ok(None);
    };
    let row = sqlx::query(
        "SELECT payload_digest, receipt_id, last_known_status FROM idempotency_namespace \
         WHERE space_id = ? AND requester_principal_id = ? AND op = ? AND idempotency_key = ?",
    )
    .bind(space_id)
    .bind(principal_id)
    .bind(admission.op)
    .bind(idempotency_key)
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

/// The dedup tombstone that outlives an evicted receipt body (§6.10/MEM-R021):
/// the terminal status a `RECEIPT_EXPIRED` may report, disclosed only after the
/// recorded target's authorization recheck has passed.
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
        admitted: &Admitted<MemoryCaptureRequest>,
        now: DateTime<Utc>,
    ) -> Result<MemoryReceipt, SpokeReject> {
        let mut transaction = self
            .store()
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(internal)?;
        let receipt = self
            .capture_in_tx(&mut transaction, principal_id, admitted, now)
            .await?;
        transaction.commit().await.map_err(internal)?;
        Ok(receipt)
    }

    /// [`Self::capture`] on a caller-owned connection; never commits.
    pub(crate) async fn capture_in_tx(
        &self,
        conn: &mut SqliteConnection,
        principal_id: &str,
        admitted: &Admitted<MemoryCaptureRequest>,
        now: DateTime<Utc>,
    ) -> Result<MemoryReceipt, SpokeReject> {
        let request = admitted.request();
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
        let admission =
            Mutation::keyed(OP_CAPTURE, request.idempotency_key.as_str(), admitted);
        let target = ReceiptTarget::for_scope(
            required_capabilities(OP_CAPTURE)
                .expect("memory.capture has a published capability list"),
            subject_id,
            visibility,
            context_id,
        )?;
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
            &target,
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
        let target = capture_target(conn, space_id, subject_id).await?;

        let status_filter = request.status.as_ref().map(status_name);
        let mut visible = Vec::new();
        // §7.7: `archived` is terminal decommission and excludes the subject's
        // candidates from the listing; fence (`fenced`) still lists so the queue
        // stays inspectable, and reads remain allowed for archived subjects.
        if target.status != "archived" {
            for candidate in candidate_rows(conn, space_id, subject_id).await? {
                if status_filter.is_some_and(|status| candidate.status != status) {
                    continue;
                }
                if candidate.is_expired_at(now)? {
                    continue;
                }
                // Each candidate is filtered by its immutable capture scope, so
                // a context-limited review grant never sees another placement.
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
        }

        // §6.7/§7.0.7: a pending candidate's expiry is time-dependent, so the
        // cursor also binds the visible set and every entry's expiry boundary at
        // the supplied instant. An entry that expires after page one changes this
        // fingerprint with no row or marker changing, so the continuation is
        // refused instead of silently dropping it.
        let visible_signature: Vec<Value> = visible
            .iter()
            .map(|candidate| {
                json!({
                    "candidate_id": candidate.candidate_id,
                    "expires_at": candidate.expires_at,
                })
            })
            .collect();
        let snapshot = snapshot_marker(conn, space_id).await?;
        // The cursor binds the caller's *effective* authorization at the supplied
        // instant as well as the persistent marker: a grant that expires between
        // two pages changes this fingerprint even though no row changed, so the
        // next page is refused instead of silently re-filtered (MEM-R026/§6.5).
        let authorization = effective_authorization_digest(&grants, principal_id, space_id, now)?;
        let binding_digest = digest(&json!({
            "principal": principal_id,
            "space": space_id,
            "subject": subject_id,
            "status": status_filter,
            "snapshot": snapshot,
            "authorization": authorization,
            "visible": visible_signature,
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
            .ok_or_else(|| candidate_unavailable())?;
        // Same refusal as a missing id: holding no coverage for an existing
        // candidate must not be observable (C-102).
        if !one_grant_covers(
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
        ) {
            return Err(candidate_unavailable());
        }
        candidate_to_wire(&candidate, space_id)
    }

    // -----------------------------------------------------------------------
    // memory.candidates.dismiss (§7.14)
    // -----------------------------------------------------------------------

    pub(crate) async fn candidates_dismiss(
        &self,
        principal_id: &str,
        admitted: &Admitted<MemoryCandidatesDismissRequest>,
        now: DateTime<Utc>,
    ) -> Result<MemoryReceipt, SpokeReject> {
        let mut transaction = self
            .store()
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(internal)?;
        let receipt = self
            .candidates_dismiss_in_tx(&mut transaction, principal_id, admitted, now)
            .await?;
        transaction.commit().await.map_err(internal)?;
        Ok(receipt)
    }

    /// [`Self::candidates_dismiss`] on a caller-owned connection; never commits.
    pub(crate) async fn candidates_dismiss_in_tx(
        &self,
        conn: &mut SqliteConnection,
        principal_id: &str,
        admitted: &Admitted<MemoryCandidatesDismissRequest>,
        now: DateTime<Utc>,
    ) -> Result<MemoryReceipt, SpokeReject> {
        let request = admitted.request();
        let space_id = request.space_id.as_str();
        require_served_space(self, space_id)?;
        let candidate_id = request.candidate_id.as_str();
        let grants = load_grants(conn, space_id).await?;
        let candidate = fetch_candidate(conn, space_id, candidate_id)
            .await?
            .ok_or_else(|| candidate_unavailable())?;
        if !one_grant_covers(
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
        ) {
            // Indistinguishable from a missing id (C-102).
            return Err(candidate_unavailable());
        }
        let admission = Mutation::keyed(
            OP_CANDIDATES_DISMISS,
            request.idempotency_key.as_str(),
            admitted,
        );
        let target = ReceiptTarget::for_scope(
            required_capabilities(OP_CANDIDATES_DISMISS)
                .expect("memory.candidates.dismiss has a published capability list"),
            &candidate.subject_id,
            &candidate.capture_visibility,
            candidate.scope_context(),
        )?;
        if let Some(replay) = resolve_replay(conn, space_id, principal_id, &admission).await? {
            return Ok(replay);
        }
        require_space_admits(conn, OP_CANDIDATES_DISMISS, space_id).await?;
        // §7.7: a fenced or archived subject rejects every write for it. A
        // dismissal admitted before the fence still replays above; a fresh one
        // fails here. The protocol makes context fences apply to writes that
        // place into that context, so only the subject lifecycle applies here.
        require_active_subject(conn, space_id, &candidate.subject_id).await?;
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
            &target,
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
        admitted: &Admitted<MemoryCandidatesReviewRequest>,
        now: DateTime<Utc>,
    ) -> Result<MemoryReceipt, SpokeReject> {
        let mut transaction = self
            .store()
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(internal)?;
        let receipt = self
            .candidates_review_in_tx(&mut transaction, principal_id, admitted, now)
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
        admitted: &Admitted<MemoryCandidatesReviewRequest>,
        now: DateTime<Utc>,
    ) -> Result<MemoryReceipt, SpokeReject> {
        let request = admitted.request();
        checked(validate_memory_candidates_review(request))?;
        let space_id = request.space_id.as_str();
        require_served_space(self, space_id)?;
        let candidate_id = request.candidate_id.as_str();
        let grants = load_grants(conn, space_id).await?;
        let candidate = fetch_candidate(conn, space_id, candidate_id)
            .await?
            .ok_or_else(|| candidate_unavailable())?;
        let subject_id = candidate.subject_id.clone();
        let visibility = candidate.capture_visibility.clone();
        let context_id = candidate.scope_context().map(str::to_owned);
        // §7.15: `review` *and* `records_write`, both covering the candidate's
        // capture scope — in one grant, so no union amplifies coverage.
        if !one_grant_covers(
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
        ) {
            // Indistinguishable from a missing id (C-102), including the
            // partial-coverage case where only one of the two capabilities is held.
            return Err(candidate_unavailable());
        }
        let admission = Mutation::keyed(
            OP_CANDIDATES_REVIEW,
            request.idempotency_key.as_str(),
            admitted,
        );
        let target = ReceiptTarget::for_scope(
            required_capabilities(OP_CANDIDATES_REVIEW)
                .expect("memory.candidates.review has a published capability list"),
            &subject_id,
            &visibility,
            context_id.as_deref(),
        )?;
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
        // §7.1/§7.15: plan every content object this decision will retain and
        // bound each one before ANY effect, so an oversized override — or a later
        // oversized fragment after valid earlier ones — cannot consume the
        // candidate or leave partial records. The decision carries no placement:
        // created records inherit the candidate's admitted capture scope (§7.15).
        let (outcome, planned) = match &request.decision {
            Decision::Promote {
                content_override,
                kind,
            } => {
                let kind = match kind {
                    Some(kind) => kind.as_str(),
                    None => candidate.kind.as_str(),
                };
                let (field, content) = match content_override {
                    Some(overridden) => (
                        "decision.content_override",
                        serde_json::to_value(overridden).map_err(|error| {
                            internal(format!("decision content is not encodable: {error}"))
                        })?,
                    ),
                    None => (
                        "content",
                        stored_json(
                            candidate.content.as_deref().ok_or_else(|| {
                                internal("a pending candidate carries no content")
                            })?,
                            "candidate content",
                        )?,
                    ),
                };
                (
                    "promoted".to_owned(),
                    vec![(field.to_owned(), kind.to_owned(), content)],
                )
            }
            Decision::Fragment { fragments } => {
                let mut planned = Vec::with_capacity(fragments.len());
                for (index, fragment) in fragments.iter().enumerate() {
                    let content = serde_json::to_value(&fragment.content).map_err(|error| {
                        internal(format!("fragment content is not encodable: {error}"))
                    })?;
                    planned.push((
                        format!("decision.fragments[{index}].content"),
                        fragment.kind.as_str().to_owned(),
                        content,
                    ));
                }
                ("fragmented".to_owned(), planned)
            }
            Decision::Drop { .. } => ("dropped".to_owned(), Vec::new()),
        };
        for (field, _, content) in &planned {
            require_review_content(field, content)?;
        }
        let mut record_ids = Vec::with_capacity(planned.len());
        for (_, kind, content) in &planned {
            record_ids.push(
                insert_record(conn, space_id, &candidate, kind, content, principal_id, now).await?,
            );
        }

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
            &target,
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
        // §6.5/§7.28 + §7.0.5/MEM-R021: the recorded target is rechecked against
        // current authorization before any body or tombstone status is
        // disclosed, and it is recorded independently of the body so an evicted
        // receipt rechecks the same subject and placement. A non-admin can only
        // ever see its own receipts, so another principal's receipt id stays
        // indistinguishable from a missing one (no existence oracle). A receipt
        // with no recorded target is refused rather than treated as space-wide.
        let target = fetch_receipt_target(conn, receipt_id).await?;
        let caller_covers_target = target
            .as_ref()
            .is_some_and(|target| target.admits(&grants, principal_id, space_id, now));
        let stored = fetch_receipt(
            conn,
            space_id,
            receipt_id,
            if admin { None } else { Some(principal_id) },
        )
        .await?;
        if let Some(stored) = stored {
            if !admin && !caller_covers_target {
                return Err(not_authorized(RECEIPT_UNAUTHORIZED));
            }
            return receipt_to_wire(&stored);
        }
        // The body is gone while its dedup tombstone survives: the status is
        // disclosed only after the recorded target's recheck passes.
        match tombstone_status(
            conn,
            space_id,
            receipt_id,
            if admin { None } else { Some(principal_id) },
        )
        .await?
        {
            Some(last_known_status) => {
                if !admin && !caller_covers_target {
                    return Err(not_authorized(RECEIPT_UNAUTHORIZED));
                }
                Err(receipt_expired(&last_known_status))
            }
            // Deliberately carries no receipt id: a foreign receipt and a
            // missing one must be indistinguishable (§7.28, no existence oracle).
            None => Err(not_found(
                "no receipt is visible to the authenticated principal",
            )),
        }
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

        /// Capture one candidate and return its id.
        async fn captured_candidate(&self, key: &str, text: &str, captured_at: DateTime<Utc>) -> String {
            let captured = self
                .provider
                .capture(
                    CLIENT,
                    &capture_request(
                        &self.space_id,
                        text,
                        context_scope(CONTEXT_A),
                        key,
                    ),
                    captured_at,
                )
                .await
                .expect("capture commits");
            committed_result(&captured)["candidate_ids"][0]
                .as_str()
                .expect("candidate id")
                .to_owned()
        }

        /// Stage one receipt through the real producer with an arbitrary target.
        async fn stage_targeted_receipt(
            &self,
            key: &str,
            op: &'static str,
            target: &ReceiptTarget,
            now: DateTime<Utc>,
        ) -> String {
            let payload = json!({ "space_id": self.space_id, "probe": key });
            let admitted = Admitted::from_scope(
                digest(&payload).expect("payload digest"),
                (),
            );
            let admission = Mutation::keyed(op, key, &admitted);
            let mut transaction = self
                .provider
                .store()
                .pool()
                .begin_with("BEGIN IMMEDIATE")
                .await
                .expect("writer transaction");
            let receipt = stage_receipt_in_tx(
                &mut transaction,
                &self.space_id,
                CLIENT,
                &admission,
                &receipt_id(),
                target,
                ReceiptState::Committed(json!({ "probe": key })),
                now,
            )
            .await
            .expect("targeted receipt stages");
            transaction.commit().await.expect("commit");
            receipt_id_of(&receipt).to_owned()
        }

        async fn ensure_named(&self, subject_id: &str, now: DateTime<Utc>) {
            let request: MemorySubjectsEnsureRequest = serde_json::from_value(json!({
                "space_id": self.space_id,
                "subject_id": subject_id,
            }))
            .expect("valid subjects.ensure request");
            self.provider
                .subjects_ensure(OPERATOR, &request, now)
                .await
                .expect("subject ensured");
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

        /// A grant with a natural expiry, for the effective-authorization bound.
        async fn grant_until(
            &self,
            grantee: &str,
            capabilities: &[&str],
            subject: Option<&str>,
            contexts: Option<(&[&str], bool)>,
            expires_at: DateTime<Utc>,
            now: DateTime<Utc>,
        ) -> String {
            let mut value = json!({
                "space_id": self.space_id,
                "principal_id": grantee,
                "capabilities": capabilities,
                "expires_at": timestamp_text(expires_at),
                "idempotency_key": format!(
                    "grant-until-{grantee}-{}-{}",
                    capabilities.join("-"),
                    timestamp_text(expires_at)
                ),
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
                .expect("expiring grant issued")
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

        async fn fence_subject(&self, action: &str, expected_revision: u64, now: DateTime<Utc>) {
            let request: MemorySubjectsFenceRequest = serde_json::from_value(json!({
                "space_id": self.space_id,
                "subject_id": SUBJECT,
                "target": { "type": "subject" },
                "action": action,
                "expected_revision": expected_revision,
                "idempotency_key": format!("fence-subject-{action}-{expected_revision}"),
            }))
            .expect("valid subjects.fence request");
            self.provider
                .subjects_fence(OPERATOR, &request, now)
                .await
                .expect("subject transition commits");
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

    /// The raw capture payload a client sends.
    fn capture_payload(space: &str, text: &str, scope: Value, key: &str) -> Value {
        json!({
            "space_id": space,
            "subject_id": SUBJECT,
            "content": { "text": text },
            "capture_scope": scope,
            "kind_hint": "fact",
            "idempotency_key": key,
        })
    }

    /// The admitted view of [`capture_payload`], keeping the raw object that the
    /// dedup digest is taken over.
    fn capture_request(
        space: &str,
        text: &str,
        scope: Value,
        key: &str,
    ) -> Admitted<MemoryCaptureRequest> {
        admit_capture(capture_payload(space, text, scope, key)).expect("valid capture request")
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
    ) -> Admitted<MemoryCandidatesDismissRequest> {
        admit_candidates_dismiss(json!({
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
    ) -> Admitted<MemoryCandidatesReviewRequest> {
        admit_candidates_review(json!({
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

    fn capture_receipt_target(context_id: &str) -> ReceiptTarget {
        ReceiptTarget::for_scope(
            required_capabilities(OP_CAPTURE).expect("memory.capture has a capability list"),
            SUBJECT,
            "context",
            Some(context_id),
        )
        .expect("valid capture receipt target")
    }

    fn receipt_payload_digest(receipt: &MemoryReceipt) -> &str {
        match receipt {
            MemoryReceipt::CommittedReceipt(receipt) => receipt.payload_digest.as_str(),
            MemoryReceipt::PendingReceipt(receipt) => receipt.payload_digest.as_str(),
            MemoryReceipt::FailedReceipt(receipt) => receipt.payload_digest.as_str(),
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
            "subjects" => "SELECT COUNT(*) AS total FROM subjects",
            "receipt_targets" => "SELECT COUNT(*) AS total FROM receipt_targets",
            "idempotency_namespace" => "SELECT COUNT(*) AS total FROM idempotency_namespace",
            other => panic!("unknown table {other}"),
        };
        let row = sqlx::query(sql)
            .fetch_one(provider.store().pool())
            .await
            .expect("count query");
        row.get("total")
    }

    async fn count_namespace_rows(provider: &FixtureMemoryProvider, op: &str) -> i64 {
        let row = sqlx::query(
            "SELECT COUNT(*) AS total FROM idempotency_namespace WHERE op = ?",
        )
        .bind(op)
        .fetch_one(provider.store().pool())
        .await
        .expect("namespace count");
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
        assert_eq!(split.code, SpokeRejectCode::NotFound);
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
            SpokeRejectCode::NotFound
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
        // The probe carries a real admitted capture payload, so its receipt
        // binds a genuine digest and a real recorded target.
        let pending_admitted = admit_capture(capture_payload(
            &space,
            "Pending probe text.",
            context_scope(CONTEXT_A),
            "async-1",
        ))
        .expect("valid capture payload");
        let pending_admission = Mutation::keyed(OP_CAPTURE, "async-1", &pending_admitted);
        let probe_target = capture_receipt_target(CONTEXT_A);

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
            &probe_target,
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
        let failed_admitted = admit_capture(capture_payload(
            &space,
            "Failed probe text.",
            context_scope(CONTEXT_A),
            "async-2",
        ))
        .expect("valid capture payload");
        let failed_admission = Mutation::keyed(OP_CAPTURE, "async-2", &failed_admitted);
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
            &probe_target,
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
        let foreign = admit_capture(json!({
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

    #[tokio::test]
    async fn candidate_by_id_refusals_are_indistinguishable_for_every_coverage_gap() {
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
                &capture_request(
                    &scenario.space_id,
                    "Queue item outside the caller's coverage.",
                    context_scope(CONTEXT_A),
                    "cap-1",
                ),
                now,
            )
            .await
            .expect("capture commits");
        let candidate_id = committed_result(&captured)["candidate_ids"][0]
            .as_str()
            .expect("candidate id")
            .to_owned();
        let receipts_after_capture = count_rows(&scenario.provider, "receipts").await;

        // No `review` capability at all: the existing candidate and a missing id
        // must produce the same refusal.
        let get_foreign = refusal(
            scenario
                .provider
                .candidates_get(CLIENT, &get_request(&scenario.space_id, &candidate_id), now)
                .await,
        );
        let get_missing = refusal(
            scenario
                .provider
                .candidates_get(
                    CLIENT,
                    &get_request(&scenario.space_id, "cand_absent"),
                    now,
                )
                .await,
        );
        assert_eq!(get_foreign.code, SpokeRejectCode::NotFound);
        assert_eq!(get_foreign, get_missing, "no candidate existence oracle");

        let dismiss_foreign = refusal(
            scenario
                .provider
                .candidates_dismiss(
                    CLIENT,
                    &dismiss_request(&scenario.space_id, &candidate_id, 1, "dis-1"),
                    now,
                )
                .await,
        );
        let dismiss_missing = refusal(
            scenario
                .provider
                .candidates_dismiss(
                    CLIENT,
                    &dismiss_request(&scenario.space_id, "cand_absent", 1, "dis-1"),
                    now,
                )
                .await,
        );
        assert_eq!(dismiss_foreign, get_missing);

        let review_foreign = refusal(
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
        assert_eq!(review_foreign, dismiss_missing);

        // The candidate is untouched and nothing was recorded.
        let untouched = scenario
            .provider
            .candidates_get(
                OPERATOR,
                &get_request(&scenario.space_id, &candidate_id),
                now,
            )
            .await
            .expect("the admin can still read the candidate");
        assert_eq!(pending_candidate(&untouched).revision.get(), 1);
        assert_eq!(
            count_rows(&scenario.provider, "receipts").await,
            receipts_after_capture
        );

        // A review grant that does not cover this capture scope is the same
        // answer as a missing id, not a distinguishable denial.
        scenario
            .grant(
                CLIENT,
                &["review"],
                Some(SUBJECT),
                Some((&[CONTEXT_B], false)),
                now,
            )
            .await;
        let out_of_scope = refusal(
            scenario
                .provider
                .candidates_get(CLIENT, &get_request(&scenario.space_id, &candidate_id), now)
                .await,
        );
        assert_eq!(out_of_scope, get_missing);
    }

    #[tokio::test]
    async fn receipt_disclosure_requires_the_operations_capability_before_any_status() {
        let scenario = Scenario::open().await;
        let now = now();
        scenario.subject(now).await;
        let capture_grant = scenario
            .grant(CLIENT, &["capture"], Some(SUBJECT), None, now)
            .await;
        let captured = scenario
            .provider
            .capture(
                CLIENT,
                &capture_request(&scenario.space_id, "Receipt body.", context_scope(CONTEXT_A), "cap-1"),
                now,
            )
            .await
            .expect("capture commits");
        let receipt_id = receipt_id_of(&captured).to_owned();

        // The capture authority is revoked and an unrelated capability
        // survives: the receipt must stay closed with the body present…
        scenario.revoke(&capture_grant, now).await;
        let surviving = scenario
            .grant(CLIENT, &["recall"], Some(SUBJECT), None, now)
            .await;
        let denied_body = refusal(
            scenario
                .provider
                .receipts_get(CLIENT, &receipts_request(&scenario.space_id, &receipt_id), now)
                .await,
        );
        assert_eq!(denied_body.code, SpokeRejectCode::NotAuthorized);
        assert!(denied_body.details.is_none(), "no status in the refusal");

        // …and it must stay exactly as closed once the body is evicted: an
        // eviction cannot upgrade a denied principal into a tombstone query.
        sqlx::query("DELETE FROM receipts WHERE receipt_id = ?")
            .bind(&receipt_id)
            .execute(scenario.provider.store().pool())
            .await
            .expect("receipt body evicted");
        let denied_tombstone = refusal(
            scenario
                .provider
                .receipts_get(CLIENT, &receipts_request(&scenario.space_id, &receipt_id), now)
                .await,
        );
        assert_eq!(
            denied_tombstone, denied_body,
            "an evicted body reveals no existence or terminal status"
        );

        // Revoking the last grant changes nothing about that refusal.
        scenario.revoke(&surviving, now).await;
        let denied_without_grants = refusal(
            scenario
                .provider
                .receipts_get(CLIENT, &receipts_request(&scenario.space_id, &receipt_id), now)
                .await,
        );
        assert_eq!(denied_without_grants, denied_body);

        // A space admin still resolves the evicted body from the tombstone.
        let admin = refusal(
            scenario
                .provider
                .receipts_get(OPERATOR, &receipts_request(&scenario.space_id, &receipt_id), now)
                .await,
        );
        assert_eq!(admin.code, SpokeRejectCode::ReceiptExpired);
        assert_eq!(
            admin
                .details
                .as_ref()
                .and_then(|details| details.get("last_known_status")),
            Some(&json!("committed"))
        );
    }

    #[tokio::test]
    async fn subject_fence_and_archive_gate_dismissal_and_listing() {
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
        let mut candidate_ids = Vec::new();
        for (index, text) in ["First queue item.", "Second queue item."].into_iter().enumerate() {
            let captured = scenario
                .provider
                .capture(
                    CLIENT,
                    &capture_request(
                        &scenario.space_id,
                        text,
                        context_scope(CONTEXT_A),
                        &format!("cap-{index}"),
                    ),
                    now,
                )
                .await
                .expect("capture commits");
            candidate_ids.push(
                committed_result(&captured)["candidate_ids"][0]
                    .as_str()
                    .expect("candidate id")
                    .to_owned(),
            );
        }
        let dismissal = scenario
            .provider
            .candidates_dismiss(
                CLIENT,
                &dismiss_request(&scenario.space_id, &candidate_ids[0], 1, "dis-1"),
                now,
            )
            .await
            .expect("dismiss commits");

        scenario.fence_subject("fence", 1, now).await;

        // The dismissal admitted before the fence still replays.
        let replay = scenario
            .provider
            .candidates_dismiss(
                CLIENT,
                &dismiss_request(&scenario.space_id, &candidate_ids[0], 1, "dis-1"),
                now,
            )
            .await
            .expect("an admitted dismissal replays across the fence");
        assert_eq!(receipt_id_of(&replay), receipt_id_of(&dismissal));

        // A fresh dismissal on the fenced subject is a write and is refused.
        let receipts_before = count_rows(&scenario.provider, "receipts").await;
        let fenced = refusal(
            scenario
                .provider
                .candidates_dismiss(
                    CLIENT,
                    &dismiss_request(&scenario.space_id, &candidate_ids[1], 1, "dis-2"),
                    now,
                )
                .await,
        );
        assert_eq!(fenced.code, SpokeRejectCode::SubjectFenced);
        assert_eq!(
            fenced
                .details
                .as_ref()
                .and_then(|details| details.get("scope")),
            Some(&json!("subject"))
        );
        let fenced_shell = scenario
            .provider
            .candidates_get(
                CLIENT,
                &get_request(&scenario.space_id, &candidate_ids[1]),
                now,
            )
            .await
            .expect("candidate still readable");
        let untouched = pending_candidate(&fenced_shell);
        assert_eq!(untouched.status, "pending");
        assert_eq!(untouched.revision.get(), 1);
        assert_eq!(
            count_rows(&scenario.provider, "receipts").await,
            receipts_before,
            "a fenced dismissal records nothing"
        );

        // `fenced` still lists so the queue stays inspectable.
        let fenced_page = scenario
            .provider
            .candidates_list(CLIENT, &list_request(&scenario.space_id, None, None, None), now)
            .await
            .expect("fenced subject page");
        assert_eq!(fenced_page.candidates.len(), 2);

        // `archived` is terminal decommission and leaves the listing.
        scenario.fence_subject("archive", 2, now).await;
        let archived_page = scenario
            .provider
            .candidates_list(CLIENT, &list_request(&scenario.space_id, None, None, None), now)
            .await
            .expect("archived subject page");
        assert!(
            archived_page.candidates.is_empty(),
            "an archived subject is excluded from the candidate listing"
        );
        // Reads stay allowed, and so does the refusal of further writes.
        let readable = scenario
            .provider
            .candidates_get(
                CLIENT,
                &get_request(&scenario.space_id, &candidate_ids[1]),
                now,
            )
            .await
            .expect("reads remain allowed on an archived subject");
        assert_eq!(pending_candidate(&readable).status, "pending");
        assert_eq!(
            refusal(
                scenario
                    .provider
                    .candidates_review(
                        CLIENT,
                        &review_request(
                            &scenario.space_id,
                            &candidate_ids[1],
                            1,
                            json!({ "action": "promote" }),
                            "rev-1",
                        ),
                        now,
                    )
                    .await
            )
            .code,
            SpokeRejectCode::SubjectFenced
        );
    }

    #[tokio::test]
    async fn capture_digest_covers_the_received_payload_not_its_typed_form() {
        let scenario = Scenario::open().await;
        let now = now();
        scenario.subject(now).await;
        scenario
            .grant(CLIENT, &["capture"], Some(SUBJECT), None, now)
            .await;

        // A client may send the member explicitly empty; the typed model
        // normalizes it to the same empty vector as an omitted member.
        let mut payload = capture_payload(
            &scenario.space_id,
            "Digest source text.",
            context_scope(CONTEXT_A),
            "cap-raw",
        );
        payload["source_context_ids"] = json!([]);
        let explicit = admit_capture(payload.clone()).expect("valid capture payload");
        let committed = scenario
            .provider
            .capture(CLIENT, &explicit, now)
            .await
            .expect("capture commits");
        assert_eq!(
            receipt_payload_digest(&committed),
            digest(&payload).expect("payload digest"),
            "the receipt binds the JCS digest of the received payload object"
        );

        let mut omitted = payload.clone();
        omitted
            .as_object_mut()
            .expect("payload object")
            .remove("source_context_ids");
        let without_member = admit_capture(omitted.clone()).expect("valid capture payload");
        assert_eq!(
            explicit.request().source_context_ids,
            without_member.request().source_context_ids,
            "the typed views are identical..."
        );
        assert_ne!(
            digest(&payload).expect("payload digest"),
            digest(&omitted).expect("payload digest"),
            "...while the received objects are not"
        );

        let conflict = refusal(
            scenario
                .provider
                .capture(CLIENT, &without_member, now)
                .await,
        );
        assert_eq!(
            conflict.code,
            SpokeRejectCode::IdempotencyConflict,
            "an omitted member is a different payload, never a replay"
        );
        assert_eq!(count_rows(&scenario.provider, "candidates").await, 1);

        // The identical raw object still replays the original receipt.
        let replay = scenario
            .provider
            .capture(CLIENT, &explicit, now)
            .await
            .expect("identical payload replays");
        assert_eq!(receipt_id_of(&replay), receipt_id_of(&committed));
    }

    #[tokio::test]
    async fn keyless_ensure_receipt_stages_atomically_without_dedup() {
        let scenario = Scenario::open().await;
        let now = now();
        let space = scenario.space_id.clone();
        let payload = json!({
            "space_id": space,
            "subject_id": "subject-keyless",
        });
        let ensure: MemorySubjectsEnsureRequest =
            serde_json::from_value(payload.clone()).expect("valid subjects.ensure request");

        let mut transaction = scenario
            .provider
            .store()
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .expect("writer transaction");
        let change = scenario
            .provider
            .subjects_ensure_in_tx(&mut transaction, OPERATOR, &ensure, now)
            .await
            .expect("ensure commits its row");
        // Exactly the C7 request-scope bridge: the digest is taken over the
        // received object before decode, then paired with that typed view.
        let admitted = Admitted::from_scope(digest(&payload).expect("payload digest"), ensure);
        let admission = Mutation::keyless("memory.subjects.ensure", &admitted);
        let target = ReceiptTarget::for_subject(
            required_capabilities("memory.subjects.ensure")
                .expect("memory.subjects.ensure has a capability list"),
            "subject-keyless",
        )
        .expect("valid keyless receipt target");
        let receipt = stage_receipt_in_tx(
            &mut transaction,
            &space,
            OPERATOR,
            &admission,
            &receipt_id(),
            &target,
            ReceiptState::Committed(json!({
                "subject_id": change.subject_id,
                "created": change.created,
                "status": change.status,
                "revision": change.revision,
            })),
            now,
        )
        .await
        .expect("keyless receipt stages");
        transaction.commit().await.expect("one commit");

        assert_eq!(receipt_status(&receipt), "committed");
        assert_eq!(
            committed_result(&receipt),
            &json!({
                "subject_id": "subject-keyless",
                "created": true,
                "status": "active",
                "revision": 1,
            })
        );
        assert_eq!(
            receipt_payload_digest(&receipt),
            digest(&payload).expect("payload digest")
        );
        let wire = serde_json::to_value(&receipt).expect("receipt encodes");
        assert!(
            wire.get("idempotency_key").is_none(),
            "a keyless receipt echoes no key"
        );
        assert_eq!(
            count_namespace_rows(&scenario.provider, "memory.subjects.ensure").await,
            0,
            "the keyless exemption binds no dedup namespace row"
        );
        assert_eq!(count_rows(&scenario.provider, "subjects").await, 1);
        let mut reader = scenario.provider.store().pool().begin().await.expect("reader");
        assert!(
            resolve_replay(&mut reader, &space, OPERATOR, &admission)
                .await
                .expect("replay resolves")
                .is_none(),
            "a keyless mutation is never a replay"
        );
        reader.rollback().await.expect("read transaction closes");

        // Atomicity: a staged ensure plus receipt that the caller rolls back
        // leaves neither behind.
        let second_payload = json!({
            "space_id": space,
            "subject_id": "subject-rolled-back",
        });
        let second: MemorySubjectsEnsureRequest =
            serde_json::from_value(second_payload.clone()).expect("valid subjects.ensure request");

        let mut transaction = scenario
            .provider
            .store()
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .expect("writer transaction");
        scenario
            .provider
            .subjects_ensure_in_tx(&mut transaction, OPERATOR, &second, now)
            .await
            .expect("ensure stages");
        let second_admitted =
            Admitted::from_scope(digest(&second_payload).expect("payload digest"), second);
        let second_admission = Mutation::keyless("memory.subjects.ensure", &second_admitted);
        let second_target = ReceiptTarget::for_subject(
            required_capabilities("memory.subjects.ensure")
                .expect("memory.subjects.ensure has a capability list"),
            "subject-rolled-back",
        )
        .expect("valid keyless receipt target");
        stage_receipt_in_tx(
            &mut transaction,
            &space,
            OPERATOR,
            &second_admission,
            &receipt_id(),
            &second_target,
            ReceiptState::Committed(json!({
                "subject_id": "subject-rolled-back",
                "created": true,
                "status": "active",
                "revision": 1,
            })),
            now,
        )
        .await
        .expect("receipt stages");
        transaction.rollback().await.expect("caller rollback");

        assert_eq!(
            count_rows(&scenario.provider, "subjects").await,
            1,
            "the rolled-back subject left no row"
        );
        assert_eq!(
            count_rows(&scenario.provider, "receipts").await,
            1,
            "the rolled-back receipt left no row"
        );
    }

    #[tokio::test]
    async fn receipt_target_refuses_the_wrong_subject_on_body_and_tombstone() {
        let scenario = Scenario::open().await;
        let now = now();
        scenario.subject(now).await;
        scenario.ensure_named("subject-beta", now).await;
        let alpha_capture = scenario
            .grant(CLIENT, &["capture"], Some(SUBJECT), None, now)
            .await;
        let captured = scenario
            .provider
            .capture(
                CLIENT,
                &capture_request(
                    &scenario.space_id,
                    "Receipt bound to subject-alpha.",
                    context_scope(CONTEXT_A),
                    "cap-1",
                ),
                now,
            )
            .await
            .expect("capture commits");
        let receipt_id = receipt_id_of(&captured).to_owned();

        // The capture authority is revoked and a same-capability grant for
        // another subject survives: the recorded target is not covered.
        scenario.revoke(&alpha_capture, now).await;
        scenario
            .grant(CLIENT, &["capture"], Some("subject-beta"), None, now)
            .await;
        let wrong_subject = refusal(
            scenario
                .provider
                .receipts_get(CLIENT, &receipts_request(&scenario.space_id, &receipt_id), now)
                .await,
        );
        assert_eq!(wrong_subject.code, SpokeRejectCode::NotAuthorized);
        assert!(wrong_subject.details.is_none());

        // A grant that actually covers the recorded subject and placement is
        // still enough: the fix is not a blanket denial.
        let covering = scenario
            .grant(CLIENT, &["capture"], Some(SUBJECT), None, now)
            .await;
        let allowed = scenario
            .provider
            .receipts_get(CLIENT, &receipts_request(&scenario.space_id, &receipt_id), now)
            .await
            .expect("a covering grant reopens the receipt");
        assert_eq!(receipt_id_of(&allowed), receipt_id);

        // Revoke again and evict the body: the tombstone must not reveal the
        // terminal status to the same wrong-target principal.
        scenario.revoke(&covering, now).await;
        sqlx::query("DELETE FROM receipts WHERE receipt_id = ?")
            .bind(&receipt_id)
            .execute(scenario.provider.store().pool())
            .await
            .expect("receipt body evicted");
        let wrong_subject_tombstone = refusal(
            scenario
                .provider
                .receipts_get(CLIENT, &receipts_request(&scenario.space_id, &receipt_id), now)
                .await,
        );
        assert_eq!(
            wrong_subject_tombstone, wrong_subject,
            "the evicted path answers exactly like the body path"
        );

        // The space admin still resolves the evicted body.
        let admin = refusal(
            scenario
                .provider
                .receipts_get(OPERATOR, &receipts_request(&scenario.space_id, &receipt_id), now)
                .await,
        );
        assert_eq!(admin.code, SpokeRejectCode::ReceiptExpired);
        assert_eq!(
            admin
                .details
                .as_ref()
                .and_then(|details| details.get("last_known_status")),
            Some(&json!("committed"))
        );
    }

    #[tokio::test]
    async fn receipt_target_refuses_the_wrong_placement() {
        let scenario = Scenario::open().await;
        let now = now();
        scenario.subject(now).await;
        let unrestricted = scenario
            .grant(CLIENT, &["capture"], Some(SUBJECT), None, now)
            .await;
        let context_receipt = scenario
            .provider
            .capture(
                CLIENT,
                &capture_request(
                    &scenario.space_id,
                    "Context placed receipt.",
                    context_scope(CONTEXT_A),
                    "cap-a",
                ),
                now,
            )
            .await
            .expect("context capture commits");
        let shared_receipt = scenario
            .provider
            .capture(
                CLIENT,
                &capture_request(
                    &scenario.space_id,
                    "Shared placed receipt.",
                    shared_scope(),
                    "cap-b",
                ),
                now,
            )
            .await
            .expect("shared capture commits");
        let context_id = receipt_id_of(&context_receipt).to_owned();
        let shared_id = receipt_id_of(&shared_receipt).to_owned();
        scenario.revoke(&unrestricted, now).await;

        // Same capability, wrong context: neither receipt reopens.
        scenario
            .grant(
                CLIENT,
                &["capture"],
                Some(SUBJECT),
                Some((&[CONTEXT_B], false)),
                now,
            )
            .await;
        for receipt in [&context_id, &shared_id] {
            assert_eq!(
                refusal(
                    scenario
                        .provider
                        .receipts_get(
                            CLIENT,
                            &receipts_request(&scenario.space_id, receipt),
                            now,
                        )
                        .await
                )
                .code,
                SpokeRejectCode::NotAuthorized
            );
        }

        // The right context covers the context-placed receipt but a restricted
        // grant without shared access still cannot read the shared placement.
        scenario
            .grant(
                CLIENT,
                &["capture"],
                Some(SUBJECT),
                Some((&[CONTEXT_A], false)),
                now,
            )
            .await;
        scenario
            .provider
            .receipts_get(CLIENT, &receipts_request(&scenario.space_id, &context_id), now)
            .await
            .expect("the recorded context is covered");
        assert_eq!(
            refusal(
                scenario
                    .provider
                    .receipts_get(CLIENT, &receipts_request(&scenario.space_id, &shared_id), now)
                    .await
            )
            .code,
            SpokeRejectCode::NotAuthorized
        );

        // Shared access over the recorded context covers it.
        scenario
            .grant(
                CLIENT,
                &["capture"],
                Some(SUBJECT),
                Some((&[CONTEXT_A], true)),
                now,
            )
            .await;
        scenario
            .provider
            .receipts_get(CLIENT, &receipts_request(&scenario.space_id, &shared_id), now)
            .await
            .expect("shared access covers the shared placement");
    }

    #[tokio::test]
    async fn scoped_admin_cannot_read_a_foreign_target_receipt() {
        let scenario = Scenario::open().await;
        let now = now();
        scenario.subject(now).await;
        scenario.ensure_named("subject-beta", now).await;
        let capture = scenario
            .grant(CLIENT, &["capture"], Some(SUBJECT), None, now)
            .await;
        let captured = scenario
            .provider
            .capture(
                CLIENT,
                &capture_request(
                    &scenario.space_id,
                    "Admin-scoped receipt.",
                    context_scope(CONTEXT_A),
                    "cap-1",
                ),
                now,
            )
            .await
            .expect("capture commits");
        let receipt_id = receipt_id_of(&captured).to_owned();
        scenario.revoke(&capture, now).await;

        // An admin capped to another subject is not the space admin, and its
        // `admin` capability does not extend to this subject.
        scenario
            .grant(CLIENT, &["admin"], Some("subject-beta"), None, now)
            .await;
        let scoped_admin = refusal(
            scenario
                .provider
                .receipts_get(CLIENT, &receipts_request(&scenario.space_id, &receipt_id), now)
                .await,
        );
        assert_eq!(scoped_admin.code, SpokeRejectCode::NotAuthorized);

        // A genuine space-wide admin grant still reads it.
        scenario.grant(CLIENT, &["admin"], None, None, now).await;
        let read = scenario
            .provider
            .receipts_get(CLIENT, &receipts_request(&scenario.space_id, &receipt_id), now)
            .await
            .expect("space admin reads the receipt");
        assert_eq!(receipt_id_of(&read), receipt_id);
    }

    #[tokio::test]
    async fn receipt_target_metadata_is_content_free_and_survives_reopen() {
        let scenario = Scenario::open().await;
        let now = now();
        scenario.subject(now).await;
        let secret = "Only the archivist knows the vault code.";
        let capture = scenario
            .grant(CLIENT, &["capture"], Some(SUBJECT), None, now)
            .await;
        let captured = scenario
            .provider
            .capture(
                CLIENT,
                &capture_request(
                    &scenario.space_id,
                    secret,
                    context_scope(CONTEXT_A),
                    "cap-1",
                ),
                now,
            )
            .await
            .expect("capture commits");
        let receipt_id = receipt_id_of(&captured).to_owned();

        // The recorded target carries the capability list and the subject and
        // placement facets — never payload, content or a digest of the operation.
        let row = sqlx::query(
            "SELECT capabilities_json, subject_id, context_ids_json, reads_shared, subject_all \
             FROM receipt_targets WHERE receipt_id = ?",
        )
        .bind(&receipt_id)
        .fetch_one(scenario.provider.store().pool())
        .await
        .expect("recorded target");
        assert_eq!(
            serde_json::from_str::<Value>(&row.get::<String, _>("capabilities_json"))
                .expect("capabilities json"),
            json!(["capture"])
        );
        assert_eq!(row.get::<String, _>("subject_id"), SUBJECT);
        assert_eq!(
            serde_json::from_str::<Value>(&row.get::<String, _>("context_ids_json"))
                .expect("contexts json"),
            json!([CONTEXT_A])
        );
        assert_eq!(row.get::<i64, _>("reads_shared"), 0);
        assert_eq!(row.get::<i64, _>("subject_all"), 0);
        assert_no_secret(&row.get::<String, _>("capabilities_json"), secret);
        assert_no_secret(&row.get::<String, _>("subject_id"), secret);
        assert_no_secret(&row.get::<String, _>("context_ids_json"), secret);

        // Durability: the target is read back after a real close/reopen.
        let scenario = scenario.reopen().await;
        let reopened = scenario
            .provider
            .receipts_get(
                CLIENT,
                &receipts_request(&scenario.space_id, &receipt_id),
                now,
            )
            .await
            .expect("the receipt still resolves after reopen");

        // And it outlives the receipt body: after another reopen the admin still
        // resolves the tombstone while the losing principal is refused.
        sqlx::query("DELETE FROM receipts WHERE receipt_id = ?")
            .bind(receipt_id_of(&reopened))
            .execute(scenario.provider.store().pool())
            .await
            .expect("receipt body evicted");
        let scenario = scenario.reopen().await;
        let admin = refusal(
            scenario
                .provider
                .receipts_get(OPERATOR, &receipts_request(&scenario.space_id, &receipt_id), now)
                .await,
        );
        assert_eq!(admin.code, SpokeRejectCode::ReceiptExpired);
        scenario.revoke(&capture, now).await;
        assert_eq!(
            refusal(
                scenario
                    .provider
                    .receipts_get(CLIENT, &receipts_request(&scenario.space_id, &receipt_id), now)
                    .await
            )
            .code,
            SpokeRejectCode::NotAuthorized
        );
    }

    #[tokio::test]
    async fn grant_expiry_invalidates_the_cursor_instead_of_refiltering() {
        let scenario = Scenario::open().await;
        let now = now();
        scenario.subject(now).await;
        scenario
            .grant(CLIENT, &["capture"], Some(SUBJECT), None, now)
            .await;
        for (index, context_id) in [CONTEXT_A, CONTEXT_A, CONTEXT_B, CONTEXT_B]
            .into_iter()
            .enumerate()
        {
            scenario
                .provider
                .capture(
                    CLIENT,
                    &capture_request(
                        &scenario.space_id,
                        &format!("Expiry material {index}"),
                        context_scope(context_id),
                        &format!("cap-{index}"),
                    ),
                    now,
                )
                .await
                .expect("capture commits");
        }
        // A survives every page; B expires an hour after the first page.
        scenario
            .grant(
                CLIENT,
                &["review"],
                Some(SUBJECT),
                Some((&[CONTEXT_A], false)),
                now,
            )
            .await;
        scenario
            .grant_until(
                CLIENT,
                &["review"],
                Some(SUBJECT),
                Some((&[CONTEXT_B], false)),
                now + Duration::hours(1),
                now,
            )
            .await;

        let first = scenario
            .provider
            .candidates_list(
                CLIENT,
                &list_request(&scenario.space_id, None, Some(2), None),
                now,
            )
            .await
            .expect("first page");
        let cursor = first.next_cursor.clone().expect("page one issues a cursor");
        // No-change continuation at the same instant keeps working.
        let second = scenario
            .provider
            .candidates_list(
                CLIENT,
                &list_request(&scenario.space_id, None, Some(2), Some(&cursor)),
                now,
            )
            .await
            .expect("the unchanged continuation is accepted");
        let mut union = candidate_ids(&first);
        union.extend(candidate_ids(&second));
        union.sort();
        union.dedup();
        assert_eq!(union.len(), 4, "both contexts are visible while B is live");

        // Same cursor, later instant: B has expired and A survives, so the
        // subject is still authorized but the effectively authorized set moved.
        let marker_before = sqlx::query("SELECT marker FROM provider_snapshot WHERE space_id = ?")
            .bind(&scenario.space_id)
            .fetch_one(scenario.provider.store().pool())
            .await
            .expect("marker")
            .get::<i64, _>("marker");
        let expired_at = now + Duration::hours(2);
        let refused = refusal(
            scenario
                .provider
                .candidates_list(
                    CLIENT,
                    &list_request(&scenario.space_id, None, Some(2), Some(&cursor)),
                    expired_at,
                )
                .await,
        );
        assert_eq!(
            refused.code,
            SpokeRejectCode::CursorInvalid,
            "an expired grant invalidates the cursor instead of re-filtering it"
        );
        let marker_after = sqlx::query("SELECT marker FROM provider_snapshot WHERE space_id = ?")
            .bind(&scenario.space_id)
            .fetch_one(scenario.provider.store().pool())
            .await
            .expect("marker")
            .get::<i64, _>("marker");
        assert_eq!(
            marker_before, marker_after,
            "the invalidation is not faked by advancing the persistent marker"
        );

        // A fresh query at that instant performs proper current authorization:
        // only the surviving context's candidates remain.
        let fresh = scenario
            .provider
            .candidates_list(
                CLIENT,
                &list_request(&scenario.space_id, None, None, None),
                expired_at,
            )
            .await
            .expect("fresh page");
        assert_eq!(fresh.candidates.len(), 2);
        for candidate in &fresh.candidates {
            assert_eq!(
                serde_json::to_value(&pending_candidate(candidate).capture_scope)
                    .expect("scope encodes"),
                context_scope(CONTEXT_A)
            );
        }
    }

    #[tokio::test]
    async fn target_families_authorize_space_wide_whole_subject_and_multi_context() {
        let scenario = Scenario::open().await;
        let now = now();
        scenario.subject(now).await;

        // §7.29: one op, three genuinely different requirements — a whole-space
        // export (space-wide), a whole-subject export (`subject_all`) and a
        // selected-contexts export (multi-context).
        let export = required_capabilities("memory.exports.create")
            .expect("memory.exports.create has a capability list");
        let space_wide = ReceiptTarget::whole_space(export).expect("whole-space target");
        let whole_subject = ReceiptTarget::new(export, Some(SUBJECT), &[], false, true)
            .expect("whole-subject target");
        let multi_context =
            ReceiptTarget::new(export, Some(SUBJECT), &[CONTEXT_A, CONTEXT_B], false, false)
                .expect("multi-context target");
        let space_wide_receipt = scenario
            .stage_targeted_receipt("export-wide", "memory.exports.create", &space_wide, now)
            .await;
        let whole_subject_receipt = scenario
            .stage_targeted_receipt(
                "export-subject",
                "memory.exports.create",
                &whole_subject,
                now,
            )
            .await;
        let multi_context_receipt = scenario
            .stage_targeted_receipt(
                "export-contexts",
                "memory.exports.create",
                &multi_context,
                now,
            )
            .await;

        // Space-wide: a subject-scoped grant never covers a whole-space target,
        // and a space-wide grant does.
        let subject_scoped = scenario
            .grant(CLIENT, &["transfer"], Some(SUBJECT), None, now)
            .await;
        assert_eq!(
            refusal(
                scenario
                    .provider
                    .receipts_get(
                        CLIENT,
                        &receipts_request(&scenario.space_id, &space_wide_receipt),
                        now,
                    )
                    .await
            )
            .code,
            SpokeRejectCode::NotAuthorized
        );
        scenario.revoke(&subject_scoped, now).await;
        let space_grant = scenario.grant(CLIENT, &["transfer"], None, None, now).await;
        scenario
            .provider
            .receipts_get(
                CLIENT,
                &receipts_request(&scenario.space_id, &space_wide_receipt),
                now,
            )
            .await
            .expect("a space-wide grant covers the whole-space target");
        scenario.revoke(&space_grant, now).await;

        // Whole-subject: a context-restricted grant never obtains `subject_all`,
        // and an unrestricted one does.
        let restricted = scenario
            .grant(
                CLIENT,
                &["transfer"],
                Some(SUBJECT),
                Some((&[CONTEXT_A], true)),
                now,
            )
            .await;
        assert_eq!(
            refusal(
                scenario
                    .provider
                    .receipts_get(
                        CLIENT,
                        &receipts_request(&scenario.space_id, &whole_subject_receipt),
                        now,
                    )
                    .await
            )
            .code,
            SpokeRejectCode::NotAuthorized
        );
        scenario.revoke(&restricted, now).await;
        let unrestricted = scenario
            .grant(CLIENT, &["transfer"], Some(SUBJECT), None, now)
            .await;
        scenario
            .provider
            .receipts_get(
                CLIENT,
                &receipts_request(&scenario.space_id, &whole_subject_receipt),
                now,
            )
            .await
            .expect("an unrestricted grant covers the whole-subject target");
        // The same unrestricted grant also covers the multi-context target: an
        // omitted context restriction is unrestricted, so it names no context.
        scenario
            .provider
            .receipts_get(
                CLIENT,
                &receipts_request(&scenario.space_id, &multi_context_receipt),
                now,
            )
            .await
            .expect("an unrestricted grant covers every recorded context");
        scenario.revoke(&unrestricted, now).await;
        let partial = scenario
            .grant(
                CLIENT,
                &["transfer"],
                Some(SUBJECT),
                Some((&[CONTEXT_A], false)),
                now,
            )
            .await;
        assert_eq!(
            refusal(
                scenario
                    .provider
                    .receipts_get(
                        CLIENT,
                        &receipts_request(&scenario.space_id, &multi_context_receipt),
                        now,
                    )
                    .await
            )
            .code,
            SpokeRejectCode::NotAuthorized
        );
        scenario.revoke(&partial, now).await;
        scenario
            .grant(
                CLIENT,
                &["transfer"],
                Some(SUBJECT),
                Some((&[CONTEXT_A, CONTEXT_B], false)),
                now,
            )
            .await;
        scenario
            .provider
            .receipts_get(
                CLIENT,
                &receipts_request(&scenario.space_id, &multi_context_receipt),
                now,
            )
            .await
            .expect("a grant covering every recorded context covers the target");

        // A structurally impossible target is refused at construction.
        for (capabilities, contexts, subject_all) in [
            (&[][..], &[][..], false),
            (&["review", "review"][..], &[][..], false),
            (&["review"][..], &[CONTEXT_A][..], true),
            (&["review"][..], &[CONTEXT_A, CONTEXT_A][..], false),
        ] {
            assert_eq!(
                refusal(ReceiptTarget::new(
                    capabilities,
                    Some(SUBJECT),
                    contexts,
                    false,
                    subject_all
                ))
                .code,
                SpokeRejectCode::InvalidInput
            );
        }
    }

    #[tokio::test]
    async fn candidate_expiry_invalidates_the_cursor_instead_of_dropping_pages() {
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

        // Two staggered captures: the older one expires two hours before the
        // newer one, and page one is anchored on the newer entry.
        let older = scenario
            .captured_candidate("cap-older", "Older pending item.", now - Duration::hours(2))
            .await;
        let newer = scenario
            .captured_candidate("cap-newer", "Newer pending item.", now)
            .await;

        let first = scenario
            .provider
            .candidates_list(
                CLIENT,
                &list_request(&scenario.space_id, None, Some(1), None),
                now,
            )
            .await
            .expect("first page");
        assert_eq!(candidate_ids(&first), vec![newer.clone()]);
        let cursor = first.next_cursor.clone().expect("page one issues a cursor");

        // An unchanged continuation at the same instant still works.
        let second = scenario
            .provider
            .candidates_list(
                CLIENT,
                &list_request(&scenario.space_id, None, Some(1), Some(&cursor)),
                now,
            )
            .await
            .expect("the unchanged continuation is accepted");
        assert_eq!(candidate_ids(&second), vec![older.clone()]);

        // Much later the older entry has expired while the anchor is still live:
        // the visible set changed with no row, grant or marker change.
        let later = now + Duration::days(29) + Duration::hours(23);
        let marker_before = sqlx::query("SELECT marker FROM provider_snapshot WHERE space_id = ?")
            .bind(&scenario.space_id)
            .fetch_one(scenario.provider.store().pool())
            .await
            .expect("marker")
            .get::<i64, _>("marker");
        let refused = refusal(
            scenario
                .provider
                .candidates_list(
                    CLIENT,
                    &list_request(&scenario.space_id, None, Some(1), Some(&cursor)),
                    later,
                )
                .await,
        );
        assert_eq!(
            refused.code,
            SpokeRejectCode::CursorInvalid,
            "a candidate expiring after page one invalidates the cursor"
        );
        let marker_after = sqlx::query("SELECT marker FROM provider_snapshot WHERE space_id = ?")
            .bind(&scenario.space_id)
            .fetch_one(scenario.provider.store().pool())
            .await
            .expect("marker")
            .get::<i64, _>("marker");
        assert_eq!(
            marker_before, marker_after,
            "the invalidation does not fake a marker advance"
        );

        // A fresh query at that instant applies the expiry filter normally.
        let fresh = scenario
            .provider
            .candidates_list(
                CLIENT,
                &list_request(&scenario.space_id, None, None, None),
                later,
            )
            .await
            .expect("fresh page");
        assert_eq!(candidate_ids(&fresh), vec![newer]);
    }

    #[tokio::test]
    async fn review_content_limit_bounds_override_and_every_fragment_before_any_effect() {
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

        // The advertised measure is the serialized byte size, not the character
        // count: `{"text":"…"}` contributes eleven bytes of framing.
        let at_limit_text = "x".repeat(MAX_CONTENT_BYTES - 11);
        let over_limit_text = "x".repeat(MAX_CONTENT_BYTES - 10);
        assert_eq!(
            content_serialized_bytes(&json!({ "text": at_limit_text })),
            MAX_CONTENT_BYTES
        );
        assert_eq!(
            content_serialized_bytes(&json!({ "text": over_limit_text })),
            MAX_CONTENT_BYTES + 1
        );

        // Promote override over the limit: refused with no effect at all.
        let override_candidate = scenario
            .captured_candidate("cap-override", "Small capture.", now)
            .await;
        let records_before = count_rows(&scenario.provider, "records").await;
        let receipts_before = count_rows(&scenario.provider, "receipts").await;
        let targets_before = count_rows(&scenario.provider, "receipt_targets").await;
        let marker_before = sqlx::query("SELECT marker FROM provider_snapshot WHERE space_id = ?")
            .bind(&scenario.space_id)
            .fetch_one(scenario.provider.store().pool())
            .await
            .expect("marker")
            .get::<i64, _>("marker");
        let rejected = refusal(
            scenario
                .provider
                .candidates_review(
                    CLIENT,
                    &review_request(
                        &scenario.space_id,
                        &override_candidate,
                        1,
                        json!({
                            "action": "promote",
                            "content_override": { "text": over_limit_text },
                        }),
                        "rev-override-over",
                    ),
                    now,
                )
                .await,
        );
        assert_eq!(rejected.code, SpokeRejectCode::InvalidInput);
        assert_eq!(
            rejected
                .details
                .as_ref()
                .and_then(|details| details.get("limit")),
            Some(&json!("max_content_bytes"))
        );
        assert_eq!(
            records_before,
            count_rows(&scenario.provider, "records").await,
            "a rejected review writes no record"
        );
        assert_eq!(
            receipts_before,
            count_rows(&scenario.provider, "receipts").await,
            "a rejected review writes no receipt"
        );
        assert_eq!(
            targets_before,
            count_rows(&scenario.provider, "receipt_targets").await,
            "a rejected review records no target"
        );
        assert_eq!(
            marker_before,
            sqlx::query("SELECT marker FROM provider_snapshot WHERE space_id = ?")
                .bind(&scenario.space_id)
                .fetch_one(scenario.provider.store().pool())
                .await
                .expect("marker")
                .get::<i64, _>("marker"),
            "a rejected review advances nothing"
        );
        let untouched = scenario
            .provider
            .candidates_get(
                CLIENT,
                &get_request(&scenario.space_id, &override_candidate),
                now,
            )
            .await
            .expect("candidate readable");
        assert_eq!(pending_candidate(&untouched).status, "pending");
        assert_eq!(pending_candidate(&untouched).revision.get(), 1);

        // Exactly at the limit is admitted.
        let at_limit = scenario
            .provider
            .candidates_review(
                CLIENT,
                &review_request(
                    &scenario.space_id,
                    &override_candidate,
                    1,
                    json!({
                        "action": "promote",
                        "content_override": { "text": at_limit_text },
                    }),
                    "rev-override-at",
                ),
                now,
            )
            .await
            .expect("an at-limit override commits");
        let record_ids: Vec<String> = committed_result(&at_limit)["record_ids"]
            .as_array()
            .expect("record ids")
            .iter()
            .map(|id| id.as_str().expect("string id").to_owned())
            .collect();
        assert_eq!(record_ids.len(), 1);
        let record_content: String =
            sqlx::query("SELECT content_json FROM records WHERE record_id = ?")
                .bind(&record_ids[0])
                .fetch_one(scenario.provider.store().pool())
                .await
                .expect("created record")
                .get("content_json");
        assert_eq!(
            content_serialized_bytes(
                &serde_json::from_str::<Value>(&record_content).expect("record content")
            ),
            MAX_CONTENT_BYTES
        );

        // A later fragment over the limit must leave no earlier fragment record.
        let fragment_candidate = scenario
            .captured_candidate("cap-fragment", "Fragmentable capture.", now)
            .await;
        let records_before_fragments = count_rows(&scenario.provider, "records").await;
        let fragment_rejection = refusal(
            scenario
                .provider
                .candidates_review(
                    CLIENT,
                    &review_request(
                        &scenario.space_id,
                        &fragment_candidate,
                        1,
                        json!({
                            "action": "fragment",
                            "fragments": [
                                { "kind": "fact", "content": { "text": "A small fragment." } },
                                { "kind": "fact", "content": { "text": over_limit_text } },
                            ],
                        }),
                        "rev-fragment-over",
                    ),
                    now,
                )
                .await,
        );
        assert_eq!(fragment_rejection.code, SpokeRejectCode::InvalidInput);
        assert_eq!(
            fragment_rejection
                .details
                .as_ref()
                .and_then(|details| details.get("field")),
            Some(&json!("decision.fragments[1].content"))
        );
        assert_eq!(
            count_rows(&scenario.provider, "records").await,
            records_before_fragments,
            "the earlier fragment never reached a record"
        );
        let still_pending = scenario
            .provider
            .candidates_get(
                CLIENT,
                &get_request(&scenario.space_id, &fragment_candidate),
                now,
            )
            .await
            .expect("candidate readable");
        assert_eq!(pending_candidate(&still_pending).status, "pending");

        // The same candidate is still reviewable with bounded fragments.
        let fragmented = scenario
            .provider
            .candidates_review(
                CLIENT,
                &review_request(
                    &scenario.space_id,
                    &fragment_candidate,
                    1,
                    json!({
                        "action": "fragment",
                        "fragments": [
                            { "kind": "fact", "content": { "text": "A small fragment." } },
                            { "kind": "fact", "content": { "text": "Another small fragment." } },
                        ],
                    }),
                    "rev-fragment-ok",
                ),
                now,
            )
            .await
            .expect("bounded fragments commit");
        assert_eq!(
            committed_result(&fragmented)["record_ids"]
                .as_array()
                .expect("record ids")
                .len(),
            2
        );

        // Multibyte content: the measure counts UTF-8 bytes, so the text is filled
        // with two-byte characters and one ASCII byte to land exactly on the limit
        // (the `{"text":"…"}` framing is eleven bytes) while the character count
        // stays far below it.
        let multibyte_at_limit =
            format!("{}x", "é".repeat((MAX_CONTENT_BYTES - 12) / 2));
        assert_eq!(
            content_serialized_bytes(&json!({ "text": multibyte_at_limit })),
            MAX_CONTENT_BYTES
        );
        let unicode_candidate = scenario
            .captured_candidate("cap-unicode", "Unicode capture.", now)
            .await;
        scenario
            .provider
            .candidates_review(
                CLIENT,
                &review_request(
                    &scenario.space_id,
                    &unicode_candidate,
                    1,
                    json!({
                        "action": "promote",
                        "content_override": { "text": multibyte_at_limit },
                    }),
                    "rev-unicode-at",
                ),
                now,
            )
            .await
            .expect("a multibyte at-limit override commits");
        let over_unicode_candidate = scenario
            .captured_candidate("cap-unicode-over", "Unicode overflow capture.", now)
            .await;
        let unicode_rejection = refusal(
            scenario
                .provider
                .candidates_review(
                    CLIENT,
                    &review_request(
                        &scenario.space_id,
                        &over_unicode_candidate,
                        1,
                        json!({
                            "action": "promote",
                            "content_override": {
                                "text": "é".repeat((MAX_CONTENT_BYTES - 11) / 2 + 1),
                            },
                        }),
                        "rev-unicode-over",
                    ),
                    now,
                )
                .await,
        );
        assert_eq!(unicode_rejection.code, SpokeRejectCode::InvalidInput);
    }

    #[tokio::test]
    async fn whole_space_target_requires_unrestricted_coverage_on_body_and_tombstone() {
        let scenario = Scenario::open().await;
        let now = now();
        scenario.subject(now).await;
        let export = required_capabilities("memory.exports.create")
            .expect("memory.exports.create has a capability list");
        let whole_space = ReceiptTarget::whole_space(export).expect("whole-space target");
        let receipt = scenario
            .stage_targeted_receipt(
                "export-whole-space",
                "memory.exports.create",
                &whole_space,
                now,
            )
            .await;

        // A space-wide grant that still restricts placement is not whole-space
        // coverage: the recorded target requires the unrestricted facet.
        let context_limited = scenario
            .grant(CLIENT, &["transfer"], None, Some((&[CONTEXT_A], false)), now)
            .await;
        let denied_body = refusal(
            scenario
                .provider
                .receipts_get(CLIENT, &receipts_request(&scenario.space_id, &receipt), now)
                .await,
        );
        assert_eq!(denied_body.code, SpokeRejectCode::NotAuthorized);

        // The same context-limited grant still covers a requirement that merely
        // names no context (§7.5-shaped subject target): nothing normalized the
        // omitted facet into "unrestricted".
        let subject_target = ReceiptTarget::new(export, Some(SUBJECT), &[], false, false)
            .expect("subject-only target");
        let subject_receipt = scenario
            .stage_targeted_receipt(
                "export-subject-only",
                "memory.exports.create",
                &subject_target,
                now,
            )
            .await;
        scenario
            .provider
            .receipts_get(
                CLIENT,
                &receipts_request(&scenario.space_id, &subject_receipt),
                now,
            )
            .await
            .expect("a context-limited space-wide grant covers a subject-only requirement");

        // Genuinely unrestricted coverage reads the whole-space receipt.
        scenario.revoke(&context_limited, now).await;
        let unrestricted = scenario.grant(CLIENT, &["transfer"], None, None, now).await;
        scenario
            .provider
            .receipts_get(CLIENT, &receipts_request(&scenario.space_id, &receipt), now)
            .await
            .expect("unrestricted coverage reads a whole-space receipt");

        // Evicted body, restricted grant again: the tombstone refuses exactly as
        // the body path did and discloses no terminal status.
        scenario.revoke(&unrestricted, now).await;
        let restricted_again = scenario
            .grant(CLIENT, &["transfer"], None, Some((&[CONTEXT_A], false)), now)
            .await;
        sqlx::query("DELETE FROM receipts WHERE receipt_id = ?")
            .bind(&receipt)
            .execute(scenario.provider.store().pool())
            .await
            .expect("receipt body evicted");
        let denied_tombstone = refusal(
            scenario
                .provider
                .receipts_get(CLIENT, &receipts_request(&scenario.space_id, &receipt), now)
                .await,
        );
        assert_eq!(
            denied_tombstone, denied_body,
            "the evicted path answers exactly like the body path"
        );
        scenario.revoke(&restricted_again, now).await;
        let admin = refusal(
            scenario
                .provider
                .receipts_get(OPERATOR, &receipts_request(&scenario.space_id, &receipt), now)
                .await,
        );
        assert_eq!(admin.code, SpokeRejectCode::ReceiptExpired);
        assert_eq!(
            admin
                .details
                .as_ref()
                .and_then(|details| details.get("last_known_status")),
            Some(&json!("committed"))
        );
    }
}
