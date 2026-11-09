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
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqliteConnection};
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

use crate::faults::FaultPoint;
use crate::provider::FixtureMemoryProvider;
use crate::spaces::{
    bump_snapshot, internal, internal_failure, is_authorized, load_grants, requirement,
    snapshot_marker, whole_scope_requirement, ENGINE_POLICY_VERSION, MAX_PAGE_SIZE,
};
use crate::FixtureError;

/// Wire schema version carried by every fixture-served object (§6.1).
const SCHEMA_VERSION: u64 = 1;

/// `memory.describe.limits.max_content_bytes` for the fixture (§7.1); the
/// assembly advertises this same value.
pub const MAX_CONTENT_BYTES: usize = 64 * 1024;

/// Provider candidate retention: a pending candidate stops being listed after
/// this window without being consumed (§6.7 `expires_at`, §8.1).
const CANDIDATE_TTL_DAYS: i64 = 30;

/// §7.1 `receipt_retention_seconds`: how long a receipt *body* is served.
///
/// The window starts at `completed_at`, never at admission, so work that stayed
/// pending keeps its full body window after it settles. Past the window only the
/// content-free dedup tombstone answers (§6.10/MEM-R021).
pub const RECEIPT_RETENTION_SECONDS: u64 = 2_592_000;

/// §7.1 `idempotency_retention_seconds`: the stored receipt-body window.
///
/// The fixture keeps it identical to `receipt_retention_seconds`, as the local
/// provider does, so there is one body lifetime for every terminal receipt.
pub const IDEMPOTENCY_RETENTION_SECONDS: u64 = RECEIPT_RETENTION_SECONDS;

/// §7.1 `dedup_retention_seconds`: how long the dedup tombstone is kept.
///
/// The tombstone (namespace key, payload digest, receipt id, last known status)
/// is kept **after the body retires**, as §6.10 states. It is not counted from
/// admission, so a long pending period never shortens it.
pub const DEDUP_RETENTION_SECONDS: u64 = 5_184_000;

const _: () = assert!(DEDUP_RETENTION_SECONDS >= IDEMPOTENCY_RETENTION_SECONDS);

/// One retention window as a signed span, for the deadline arithmetic below.
fn retention_span(window_seconds: u64) -> Duration {
    Duration::seconds(i64::try_from(window_seconds).unwrap_or(i64::MAX))
}

/// The instant a receipt body retires: the terminal completion plus
/// `receipt_retention_seconds` (§6.10).
fn body_expiry(completed: DateTime<Utc>) -> DateTime<Utc> {
    completed + retention_span(RECEIPT_RETENTION_SECONDS)
}

/// The instant the dedup tombstone retires: the body retirement plus the full
/// `dedup_retention_seconds` window (§6.10/MEM-R021).
fn tombstone_expiry(completed: DateTime<Utc>) -> DateTime<Utc> {
    body_expiry(completed) + retention_span(DEDUP_RETENTION_SECONDS)
}

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
pub struct CandidatePage {
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
pub struct Admitted<Request> {
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
    pub(crate) const fn from_scope(payload_digest: String, request: Request) -> Self {
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
    pub(crate) const fn request(&self) -> &Request {
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
pub fn admit_capture(payload: Value) -> Result<Admitted<MemoryCaptureRequest>, SpokeReject> {
    admit(payload, serde_json::from_value)
}

/// §7.14 `memory.candidates.dismiss` at the boundary.
pub fn admit_candidates_dismiss(
    payload: Value,
) -> Result<Admitted<MemoryCandidatesDismissRequest>, SpokeReject> {
    admit(payload, serde_json::from_value)
}

/// §7.15 `memory.candidates.review` at the boundary.
pub fn admit_candidates_review(
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

/// §14.3: the caller cannot tell whether the mutation committed. Resolution is
/// always a same-key replay (which returns the original receipt) or
/// `memory.receipts.get`.
fn outcome_uncertain() -> SpokeReject {
    reject(
        SpokeRejectCode::OutcomeUncertain,
        "the mutation's commit state is unknown; resolve with the same idempotency key \
         or memory.receipts.get",
    )
}

/// A commit that failed may still have taken effect (§14.3): the caller cannot
/// be told whether anything committed, so it is `OUTCOME_UNCERTAIN` — never a
/// clean retryable internal error.
fn commit_unknown() -> SpokeReject {
    SpokeReject {
        code: SpokeRejectCode::OutcomeUncertain,
        message: "the transaction's commit state is unknown; resolve with the same idempotency \
                  key or memory.receipts.get"
            .to_owned(),
        details: None,
    }
}

/// A fixture fault that ended before this operation committed. §14.2 allows the
/// retryable internal class only when no effect committed, which holds here.
fn fault_before_effect(point: FaultPoint) -> SpokeReject {
    reject(
        SpokeRejectCode::InternalError,
        format!("fixture fault hold at {point:?} ended before any effect was committed"),
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
        .map_err(internal_failure)
}

fn stored_json(raw: &str, field: &str) -> Result<Value, SpokeReject> {
    serde_json::from_str(raw).map_err(|_| internal(format!("stored {field} is not valid JSON")))
}

fn encoded(value: &Value) -> Result<String, SpokeReject> {
    serde_json::to_string(value).map_err(internal_failure)
}

fn row_u64(row: &SqliteRow, column: &str) -> Result<u64, SpokeReject> {
    let raw: i64 = row.try_get(column).map_err(internal_failure)?;
    u64::try_from(raw).map_err(internal_failure)
}

fn row_i64(value: u64) -> Result<i64, SpokeReject> {
    i64::try_from(value).map_err(internal_failure)
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
        .map_err(internal_failure)?
        .ok_or_else(|| internal(format!("space {space_id} is not stored")))?;
    let status: String = row.try_get("status").map_err(internal_failure)?;
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
    .map_err(internal_failure)?
    .ok_or_else(|| not_found(format!("subject {subject_id} does not exist in this space")))?;
    let fenced: String = row
        .try_get("fenced_contexts_json")
        .map_err(internal_failure)?;
    Ok(CaptureTarget {
        status: row.try_get("status").map_err(internal_failure)?,
        fenced_contexts: serde_json::from_str(&fenced).map_err(internal_failure)?,
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
        if target
            .fenced_contexts
            .iter()
            .any(|fenced| fenced == context_id)
        {
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
pub struct ReceiptTarget {
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
    /// - `subject_id: None` names no subject, and the published §6.5 evaluator
    ///   refuses a subject-scoped grant for such a requirement, so only a
    ///   space-wide grant covers that axis; `Some(id)` requires coverage of
    ///   exactly that subject.
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
        let unique = capabilities
            .iter()
            .collect::<std::collections::HashSet<_>>();
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
            capabilities: capabilities
                .iter()
                .map(|value| (*value).to_owned())
                .collect(),
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
    pub(crate) fn whole_space(capabilities: &'static [&'static str]) -> Result<Self, SpokeReject> {
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
    .map_err(internal_failure)?;
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
    .map_err(internal_failure)?;
    row.map(|row| {
        let capabilities: String = row.try_get("capabilities_json").map_err(internal_failure)?;
        let context_ids: String = row.try_get("context_ids_json").map_err(internal_failure)?;
        let reads_shared: i64 = row.try_get("reads_shared").map_err(internal_failure)?;
        let subject_all: i64 = row.try_get("subject_all").map_err(internal_failure)?;
        Ok(ReceiptTarget {
            capabilities: serde_json::from_str(&capabilities).map_err(internal_failure)?,
            subject_id: row.try_get("subject_id").map_err(internal_failure)?,
            context_ids: serde_json::from_str(&context_ids).map_err(internal_failure)?,
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
pub fn required_capabilities(op: &str) -> Option<&'static [&'static str]> {
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
        let (
            grant_id,
            grant_principal,
            grant_space,
            subject_id,
            capabilities,
            context_ids,
            shared_access,
            expires_at,
            revoked_at,
        ) = match grant {
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
pub struct Mutation {
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
pub enum ReceiptState {
    Committed(Value),
    Pending,
    Failed(SpokeReject),
}

/// §14.2 retry classes for the memory codes this slice can store.
const fn retry_class(code: SpokeRejectCode) -> &'static str {
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

pub fn receipt_id() -> String {
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
    /// §6.10 body deadline; `None` only while the receipt is pending.
    body_expires_at: Option<String>,
}

impl StoredReceipt {
    fn is_pending(&self) -> bool {
        self.status == "pending"
    }

    /// Whether the body is still inside its terminal window at `now`.
    fn inside_body_window(&self, now: DateTime<Utc>) -> Result<bool, SpokeReject> {
        let raw = self
            .body_expires_at
            .as_deref()
            .ok_or_else(|| internal("a terminal receipt carries no body deadline"))?;
        Ok(now <= instant(raw)?)
    }
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
                 payload_digest, status, result_json, error_json, created_at, completed_at, \
                 body_expires_at \
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
                 payload_digest, status, result_json, error_json, created_at, completed_at, \
                 body_expires_at \
                 FROM receipts WHERE space_id = ? AND receipt_id = ?",
            )
            .bind(space_id)
            .bind(receipt_id)
            .fetch_optional(&mut *conn)
            .await
        }
    }
    .map_err(internal_failure)?;
    row.map(|row| {
        Ok(StoredReceipt {
            space_id: row.try_get("space_id").map_err(internal_failure)?,
            receipt_id: row.try_get("receipt_id").map_err(internal_failure)?,
            requester_principal_id: row
                .try_get("requester_principal_id")
                .map_err(internal_failure)?,
            op: row.try_get("op").map_err(internal_failure)?,
            idempotency_key: row.try_get("idempotency_key").map_err(internal_failure)?,
            payload_digest: row.try_get("payload_digest").map_err(internal_failure)?,
            status: row.try_get("status").map_err(internal_failure)?,
            result: row.try_get("result_json").map_err(internal_failure)?,
            error: row.try_get("error_json").map_err(internal_failure)?,
            created_at: row.try_get("created_at").map_err(internal_failure)?,
            completed_at: row.try_get("completed_at").map_err(internal_failure)?,
            body_expires_at: row.try_get("body_expires_at").map_err(internal_failure)?,
        })
    })
    .transpose()
}

/// The content-free dedup tombstone metadata that outlives the receipt body
/// (§6.10/MEM-R021), kept independently of the body row.
struct StoredTombstone {
    last_known_status: String,
    /// §6.10 tombstone deadline; `None` only while the receipt is pending.
    tombstone_expires_at: Option<String>,
}

impl StoredTombstone {
    fn tombstone_end(&self) -> Result<DateTime<Utc>, SpokeReject> {
        let raw = self
            .tombstone_expires_at
            .as_deref()
            .ok_or_else(|| internal("a terminal dedup tombstone carries no deadline"))?;
        instant(raw)
    }
}

async fn fetch_tombstone(
    conn: &mut SqliteConnection,
    space_id: &str,
    receipt_id: &str,
    requester: Option<&str>,
) -> Result<Option<StoredTombstone>, SpokeReject> {
    let row = match requester {
        Some(requester) => {
            sqlx::query(
                "SELECT last_known_status, tombstone_expires_at FROM idempotency_namespace \
             WHERE space_id = ? AND requester_principal_id = ? AND receipt_id = ?",
            )
            .bind(space_id)
            .bind(requester)
            .bind(receipt_id)
            .fetch_optional(&mut *conn)
            .await
        }
        None => {
            sqlx::query(
                "SELECT last_known_status, tombstone_expires_at FROM idempotency_namespace \
             WHERE space_id = ? AND receipt_id = ?",
            )
            .bind(space_id)
            .bind(receipt_id)
            .fetch_optional(&mut *conn)
            .await
        }
    }
    .map_err(internal_failure)?;
    row.map(|row| {
        Ok(StoredTombstone {
            last_known_status: row.try_get("last_known_status").map_err(internal_failure)?,
            tombstone_expires_at: row
                .try_get("tombstone_expires_at")
                .map_err(internal_failure)?,
        })
    })
    .transpose()
}

/// §6.10/MEM-R021 logical retention of one receipt at one supplied instant.
///
/// It is evaluated from the persisted lifecycle deadlines, not from physical
/// presence, so an old row that no explicit maintenance pass has reclaimed is
/// still judged by the contract. Nothing here writes.
enum ReceiptRetention {
    /// The body is inside its terminal window and is served as stored.
    Retained,
    /// The body's window ended; the dedup tombstone still protects the key.
    Evicted { last_known_status: String },
    /// The whole lifetime ended (or nothing exists): as good as absent.
    Expired,
}

impl ReceiptRetention {
    fn of(
        tombstone: Option<&StoredTombstone>,
        body: Option<&StoredReceipt>,
        now: DateTime<Utc>,
    ) -> Result<Self, SpokeReject> {
        if let Some(tombstone) = tombstone {
            if tombstone.last_known_status == "pending" {
                // Live work: never expire it into re-execution, and the body it
                // belongs to must still exist.
                return body
                    .map(|_| Self::Retained)
                    .ok_or_else(|| internal("a pending dedup tombstone has no receipt body"));
            }
            if now > tombstone.tombstone_end()? {
                return Ok(Self::Expired);
            }
            return match body {
                Some(body) if !body.is_pending() && body.inside_body_window(now)? => {
                    Ok(Self::Retained)
                }
                _ => Ok(Self::Evicted {
                    last_known_status: tombstone.last_known_status.clone(),
                }),
            };
        }
        // No dedup namespace: a keyless receipt has no tombstone by design, and
        // an orphan keyed body has already outlived its namespace.
        match body {
            None => Ok(Self::Expired),
            Some(body) if body.idempotency_key.is_some() => Ok(Self::Expired),
            Some(body) if body.is_pending() => Ok(Self::Retained),
            Some(body) => {
                if body.inside_body_window(now)? {
                    Ok(Self::Retained)
                } else {
                    Ok(Self::Expired)
                }
            }
        }
    }
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
            value["result"] = stored_json(
                stored.result.as_deref().unwrap_or_default(),
                "receipt result",
            )?;
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
    serde_json::from_value(value).map_err(internal_failure)
}

/// Stage one durable receipt plus its dedup row on the caller's connection and
/// advance the private snapshot marker in the same transaction (§6.10, §8.5).
/// Never commits, so an effect and its receipt commit together (MEM-R009).
#[allow(clippy::too_many_arguments)]
pub async fn stage_receipt_in_tx(
    conn: &mut SqliteConnection,
    space_id: &str,
    principal_id: &str,
    admission: &Mutation,
    receipt_id: &str,
    target: &ReceiptTarget,
    state: ReceiptState,
    now: DateTime<Utc>,
) -> Result<MemoryReceipt, SpokeReject> {
    let (status, result, error, completed_at, body_expires_at) = match state {
        ReceiptState::Committed(result) => (
            "committed",
            Some(encoded(&result)?),
            None,
            Some(timestamp_text(now)),
            Some(timestamp_text(body_expiry(now))),
        ),
        ReceiptState::Pending => ("pending", None, None, None, None),
        ReceiptState::Failed(reject) => (
            "failed",
            None,
            Some(encoded(&error_envelope(&reject))?),
            Some(timestamp_text(now)),
            Some(timestamp_text(body_expiry(now))),
        ),
    };
    // §6.10: the body window starts at terminal completion, so a receipt that
    // stayed pending is not born already expired.
    sqlx::query(
        "INSERT INTO receipts (receipt_id, space_id, requester_principal_id, op, idempotency_key, \
         payload_digest, status, result_json, error_json, created_at, completed_at, \
         body_expires_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
    .bind(&body_expires_at)
    .execute(&mut *conn)
    .await
    .map_err(internal_failure)?;
    // §7.0.5: the keyless mutation is the sole exemption and binds no dedup
    // namespace row, so nothing can ever replay it.
    if let Some(idempotency_key) = admission.idempotency_key.as_deref() {
        // §6.10/MEM-R021: the tombstone deadline starts when the body retires,
        // so it is the terminal completion plus both declared windows. A
        // pending receipt carries no deadline until it settles.
        let tombstone_expires_at = body_expires_at
            .as_ref()
            .map(|_| timestamp_text(tombstone_expiry(now)));
        sqlx::query(
            "INSERT INTO idempotency_namespace (space_id, requester_principal_id, op, \
             idempotency_key, payload_digest, receipt_id, last_known_status, created_at, \
             tombstone_expires_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(space_id)
        .bind(principal_id)
        .bind(admission.op)
        .bind(idempotency_key)
        .bind(&admission.payload_digest)
        .bind(receipt_id)
        .bind(status)
        .bind(timestamp_text(now))
        .bind(tombstone_expires_at)
        .execute(&mut *conn)
        .await
        .map_err(|error| match error.as_database_error() {
            Some(database) if database.is_unique_violation() => idempotency_conflict(),
            _ => internal_failure(error),
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
pub async fn settle_receipt_in_tx(
    conn: &mut SqliteConnection,
    space_id: &str,
    receipt_id: &str,
    state: ReceiptState,
    now: DateTime<Utc>,
) -> Result<MemoryReceipt, SpokeReject> {
    let (status, result, error) = match state {
        ReceiptState::Committed(result) => ("committed", Some(encoded(&result)?), None),
        ReceiptState::Failed(reject) => ("failed", None, Some(encoded(&error_envelope(&reject))?)),
        ReceiptState::Pending => {
            return Err(internal(
                "a settled receipt cannot return to the pending state",
            ))
        }
    };
    // §6.10/MEM-R021: the body and tombstone windows start at this terminal
    // completion, so pending work that settles late still gets both full
    // declared recoverability windows.
    let body_expires_at = timestamp_text(body_expiry(now));
    let tombstone_expires_at = timestamp_text(tombstone_expiry(now));
    let settled = sqlx::query(
        "UPDATE receipts SET status = ?, result_json = ?, error_json = ?, completed_at = ?, \
         body_expires_at = ? \
         WHERE space_id = ? AND receipt_id = ? AND status = 'pending'",
    )
    .bind(status)
    .bind(result)
    .bind(error)
    .bind(timestamp_text(now))
    .bind(&body_expires_at)
    .bind(space_id)
    .bind(receipt_id)
    .execute(&mut *conn)
    .await
    .map_err(internal_failure)?;
    if settled.rows_affected() != 1 {
        return Err(internal(format!(
            "receipt {receipt_id} was not pending and cannot be settled"
        )));
    }
    sqlx::query(
        "UPDATE idempotency_namespace SET last_known_status = ?, tombstone_expires_at = ? \
         WHERE space_id = ? AND receipt_id = ?",
    )
    .bind(status)
    .bind(&tombstone_expires_at)
    .bind(space_id)
    .bind(receipt_id)
    .execute(&mut *conn)
    .await
    .map_err(internal_failure)?;
    bump_snapshot(conn, space_id).await?;
    let stored = fetch_receipt(conn, space_id, receipt_id, None)
        .await?
        .ok_or_else(|| internal(format!("settled receipt {receipt_id} is not readable")))?;
    receipt_to_wire(&stored)
}

/// §7.0.5/MEM-R021 replay resolution at the supplied instant.
#[derive(Debug)]
pub enum ReplayResolution {
    /// No binding: the operation executes like a first attempt.
    Fresh,
    /// The identical admitted replay returns the stored receipt.
    Replayed(Box<MemoryReceipt>),
    /// The whole namespace lifetime ended. The caller releases this stale
    /// binding inside its own transaction before staging the new effect, so a
    /// refusal after this point rolls the release back with the refused effect.
    KeyReusable { receipt_id: String },
}

/// An identical admitted replay returns the stored receipt without
/// re-executing; a changed payload under the same key is a conflict; a retired
/// body answers `RECEIPT_EXPIRED` while its tombstone lives, and only a
/// finished lifetime frees the key again.
///
/// The decision uses the supplied instant and the persisted lifecycle
/// deadlines, so the caller-owned seam needs no prior maintenance pass and
/// writes nothing.
pub async fn resolve_replay(
    conn: &mut SqliteConnection,
    space_id: &str,
    principal_id: &str,
    admission: &Mutation,
    now: DateTime<Utc>,
) -> Result<ReplayResolution, SpokeReject> {
    let Some(idempotency_key) = admission.idempotency_key.as_deref() else {
        // §7.0.5 sole keyless exemption: no namespace row exists, so there is
        // nothing to replay and the op is admitted like a first attempt.
        return Ok(ReplayResolution::Fresh);
    };
    let row = sqlx::query(
        "SELECT payload_digest, receipt_id, last_known_status, tombstone_expires_at \
         FROM idempotency_namespace \
         WHERE space_id = ? AND requester_principal_id = ? AND op = ? AND idempotency_key = ?",
    )
    .bind(space_id)
    .bind(principal_id)
    .bind(admission.op)
    .bind(idempotency_key)
    .fetch_optional(&mut *conn)
    .await
    .map_err(internal_failure)?;
    let Some(row) = row else {
        return Ok(ReplayResolution::Fresh);
    };
    let receipt_id: String = row.try_get("receipt_id").map_err(internal_failure)?;
    let tombstone = StoredTombstone {
        last_known_status: row.try_get("last_known_status").map_err(internal_failure)?,
        tombstone_expires_at: row
            .try_get("tombstone_expires_at")
            .map_err(internal_failure)?,
    };
    // §7.0.5/MEM-R021: the whole lifetime is classified FIRST, independently of
    // any authority. A binding that has fully expired is no longer a receipt to
    // disclose — it is a free key, exactly as after an explicit operator sweep,
    // so the caller's current authority for the *new* execution decides (the
    // caller-owned seam authorizes that as a fresh attempt). Requiring the old
    // target's authority here would make an unswept expired key refuse while the
    // swept state succeeds.
    if tombstone.last_known_status != "pending" && now > tombstone.tombstone_end()? {
        return Ok(ReplayResolution::KeyReusable { receipt_id });
    }
    // A live body or a live tombstone IS a receipt to disclose, so it is only
    // revealed to a principal whose *current* grants still cover the target the
    // receipt recorded — the same content-free target `memory.receipts.get`
    // rechecks. This runs in the caller's transaction, before the outcome, the
    // conflict or the retention state, and never looks at the operation's
    // current entity.
    let grants = load_grants(conn, space_id).await?;
    let retained = fetch_receipt_target(conn, &receipt_id).await?;
    let authorized = retained
        .as_ref()
        .is_some_and(|target| target.admits(&grants, principal_id, space_id, now));
    if !authorized {
        return Err(not_authorized(RECEIPT_UNAUTHORIZED));
    }
    let digest: String = row.try_get("payload_digest").map_err(internal_failure)?;
    if digest != admission.payload_digest {
        return Err(idempotency_conflict());
    }
    let body = fetch_receipt(conn, space_id, &receipt_id, None).await?;
    match ReceiptRetention::of(Some(&tombstone), body.as_ref(), now)? {
        ReceiptRetention::Expired => Ok(ReplayResolution::KeyReusable { receipt_id }),
        ReceiptRetention::Evicted { last_known_status } => Err(receipt_expired(&last_known_status)),
        ReceiptRetention::Retained => {
            let body = body.ok_or_else(|| {
                internal(format!("retained receipt {receipt_id} is not readable"))
            })?;
            Ok(ReplayResolution::Replayed(Box::new(receipt_to_wire(
                &body,
            )?)))
        }
    }
}

/// Resolve replay for a mutation about to execute on the caller's transaction.
///
/// A finished key is released here, inside the same transaction, so a refusal
/// after this point rolls the release back with the refused effect.
async fn resolve_or_release(
    conn: &mut SqliteConnection,
    space_id: &str,
    principal_id: &str,
    admission: &Mutation,
    now: DateTime<Utc>,
) -> Result<Option<MemoryReceipt>, SpokeReject> {
    match resolve_replay(conn, space_id, principal_id, admission, now).await? {
        ReplayResolution::Fresh => Ok(None),
        ReplayResolution::Replayed(receipt) => Ok(Some(*receipt)),
        ReplayResolution::KeyReusable { receipt_id } => {
            release_expired_binding_in_tx(conn, space_id, principal_id, admission, &receipt_id)
                .await?;
            Ok(None)
        }
    }
}

/// Drop the stale binding of a key whose whole lifetime ended.
///
/// This is not maintenance ahead of an operation: it is the atomic prerequisite
/// for reusing the key, executed inside the caller's transaction, so a refusal
/// after this point rolls it back together with the refused effect. The body is
/// already past its window and the target past its tombstone.
pub async fn release_expired_binding_in_tx(
    conn: &mut SqliteConnection,
    space_id: &str,
    principal_id: &str,
    admission: &Mutation,
    receipt_id: &str,
) -> Result<(), SpokeReject> {
    let Some(idempotency_key) = admission.idempotency_key.as_deref() else {
        return Ok(());
    };
    sqlx::query("DELETE FROM receipt_targets WHERE receipt_id = ?")
        .bind(receipt_id)
        .execute(&mut *conn)
        .await
        .map_err(internal_failure)?;
    sqlx::query("DELETE FROM receipts WHERE space_id = ? AND receipt_id = ?")
        .bind(space_id)
        .bind(receipt_id)
        .execute(&mut *conn)
        .await
        .map_err(internal_failure)?;
    sqlx::query(
        "DELETE FROM idempotency_namespace \
         WHERE space_id = ? AND requester_principal_id = ? AND op = ? AND idempotency_key = ?",
    )
    .bind(space_id)
    .bind(principal_id)
    .bind(admission.op)
    .bind(idempotency_key)
    .execute(&mut *conn)
    .await
    .map_err(internal_failure)?;
    Ok(())
}

/// What one explicit §6.10 retention pass reclaimed from the fixture's rows.
///
/// Physical reclamation is operator maintenance only: no read, refusal or
/// mutation wrapper runs it implicitly, so a refused operation can never commit
/// an unrelated cleanup. The counts are raised by the statements themselves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RetentionSweep {
    /// Keyed receipt bodies reclaimed after their terminal body window.
    pub receipt_bodies_evicted: u64,
    /// Keyless receipt bodies reclaimed at their body end, together with their
    /// targets (a keyless receipt binds no dedup namespace).
    pub keyless_bodies_evicted: u64,
    /// Keyed dedup tombstones reclaimed after their full post-body window.
    pub dedup_tombstones_evicted: u64,
    /// Receipt targets reclaimed with a keyed tombstone or a keyless body.
    pub targets_removed: u64,
}

/// §6.10/MEM-R021 physical reclamation on the caller's connection, at one
/// supplied instant (the explicit operator maintenance pass).
///
/// Origins are the persisted lifecycle deadlines, so this pass can never shorten
/// an advertised horizon: a keyed body retires at `body_expires_at`, and its
/// tombstone (with the original content-free target) retires at
/// `tombstone_expires_at`, which is the full `dedup_retention_seconds` after the
/// body. A keyless body and its target retire together at `body_expires_at`
/// because no dedup row exists. A pending receipt has no deadline and is never
/// reclaimed: it is live work.
///
/// This only removes rows; it never writes receipt state or the snapshot marker.
pub async fn retention_sweep_in_tx(
    conn: &mut SqliteConnection,
    space_id: &str,
    now: DateTime<Utc>,
) -> Result<RetentionSweep, SpokeReject> {
    let now_text = timestamp_text(now);
    let mut sweep = RetentionSweep::default();

    // Keyed tombstone end: the target and namespace row end together, so the
    // key is free again and nothing can reauthorize a tombstone that no longer
    // exists.
    let dedup_targets = sqlx::query(
        "DELETE FROM receipt_targets WHERE receipt_id IN ( \
             SELECT receipt_id FROM idempotency_namespace \
             WHERE space_id = ? AND tombstone_expires_at IS NOT NULL \
               AND tombstone_expires_at < ?)",
    )
    .bind(space_id)
    .bind(&now_text)
    .execute(&mut *conn)
    .await
    .map_err(internal_failure)?;
    let dedup = sqlx::query(
        "DELETE FROM idempotency_namespace \
         WHERE space_id = ? AND tombstone_expires_at IS NOT NULL \
           AND tombstone_expires_at < ?",
    )
    .bind(space_id)
    .bind(&now_text)
    .execute(&mut *conn)
    .await
    .map_err(internal_failure)?;
    sweep.targets_removed += dedup_targets.rows_affected();
    sweep.dedup_tombstones_evicted = dedup.rows_affected();

    // Keyless body end: no dedup row exists, so the body and its target end
    // together.
    let keyless_targets = sqlx::query(
        "DELETE FROM receipt_targets WHERE receipt_id IN ( \
             SELECT receipt_id FROM receipts \
             WHERE space_id = ? AND idempotency_key IS NULL AND status != 'pending' \
               AND body_expires_at IS NOT NULL AND body_expires_at < ?)",
    )
    .bind(space_id)
    .bind(&now_text)
    .execute(&mut *conn)
    .await
    .map_err(internal_failure)?;
    let keyless_bodies = sqlx::query(
        "DELETE FROM receipts \
         WHERE space_id = ? AND idempotency_key IS NULL AND status != 'pending' \
           AND body_expires_at IS NOT NULL AND body_expires_at < ?",
    )
    .bind(space_id)
    .bind(&now_text)
    .execute(&mut *conn)
    .await
    .map_err(internal_failure)?;
    sweep.targets_removed += keyless_targets.rows_affected();
    sweep.keyless_bodies_evicted = keyless_bodies.rows_affected();

    // Keyed body end: the body leaves alone; its tombstone and recorded target
    // stay until the tombstone window ends above.
    let keyed_bodies = sqlx::query(
        "DELETE FROM receipts \
         WHERE space_id = ? AND idempotency_key IS NOT NULL AND status != 'pending' \
           AND body_expires_at IS NOT NULL AND body_expires_at < ?",
    )
    .bind(space_id)
    .bind(&now_text)
    .execute(&mut *conn)
    .await
    .map_err(internal_failure)?;
    sweep.receipt_bodies_evicted = keyed_bodies.rows_affected();
    Ok(sweep)
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
        candidate_id: row.try_get("candidate_id").map_err(internal_failure)?,
        subject_id: row.try_get("subject_id").map_err(internal_failure)?,
        capture_visibility: row
            .try_get("capture_visibility")
            .map_err(internal_failure)?,
        capture_context_id: row
            .try_get("capture_context_id")
            .map_err(internal_failure)?,
        kind: row.try_get("kind").map_err(internal_failure)?,
        content: row.try_get("content_json").map_err(internal_failure)?,
        provenance: row.try_get("provenance_json").map_err(internal_failure)?,
        classification: row
            .try_get("classification_json")
            .map_err(internal_failure)?,
        status: row.try_get("status").map_err(internal_failure)?,
        consumed_by: row.try_get("consumed_by_json").map_err(internal_failure)?,
        content_digest: row.try_get("content_digest").map_err(internal_failure)?,
        occurred_at: row.try_get("occurred_at").map_err(internal_failure)?,
        expires_at: row.try_get("expires_at").map_err(internal_failure)?,
        revision: row_u64(row, "revision")?,
        created_at: row.try_get("created_at").map_err(internal_failure)?,
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
    .map_err(internal_failure)?;
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
    .map_err(internal_failure)?;
    rows.iter().map(stored_candidate).collect()
}

impl StoredCandidate {
    fn scope_context(&self) -> Option<&str> {
        self.capture_context_id.as_deref()
    }

    /// §6.7/§7.12: only a pending candidate expires, and an expired pending
    /// candidate is excluded from lists rather than consumed.
    fn expires_at(&self) -> Result<Option<DateTime<Utc>>, SpokeReject> {
        self.expires_at
            .as_deref()
            .map_or(Ok(None), |raw| instant(raw).map(Some))
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
        value["proposed"] = json!({ "kind": candidate.kind, "content": stored_json(content, "candidate content")? });
        if let Some(expires_at) = &candidate.expires_at {
            value["expires_at"] = json!(expires_at);
        }
    }
    serde_json::from_value(value).map_err(internal_failure)
}

const fn status_name(status: CandidateStatus) -> &'static str {
    match status {
        CandidateStatus::Pending => "pending",
        CandidateStatus::Consumed => "consumed",
    }
}

/// The resume index a continuation cursor names on exactly this visible set.
///
/// A cursor is only meaningful against the query and provider state that issued
/// it: a different digest, a missing entry or a malformed cursor is refused
/// instead of silently re-filtering the page (§6.7/§7.0.7).
fn cursor_start(
    binding_digest: &str,
    visible: &[StoredCandidate],
    cursor: Option<&str>,
) -> Result<usize, SpokeReject> {
    let Some(cursor) = cursor else {
        return Ok(0);
    };
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
        .ok_or_else(|| cursor_invalid("cursor no longer matches the candidate set"))
}

/// §7.15: one grant must cover both `review` and `records_write` over the
/// candidate's capture scope, so no union of grants amplifies coverage.
fn require_review_authority(
    grants: &[MemoryGrant],
    principal_id: &str,
    space_id: &str,
    subject_id: &str,
    visibility: &str,
    context_id: Option<&str>,
    now: DateTime<Utc>,
) -> Result<(), SpokeReject> {
    let covered = one_grant_covers(
        grants,
        principal_id,
        space_id,
        now,
        &[
            scope_requirement("review", subject_id, visibility, context_id),
            scope_requirement("records_write", subject_id, visibility, context_id),
        ],
    );
    if covered {
        return Ok(());
    }
    // Indistinguishable from a missing id (C-102), including the partial-coverage
    // case where only one of the two capabilities is held.
    Err(candidate_unavailable())
}

/// One planned content object: the field it was authored under, its kind and its
/// encoded value.
type PlannedContent = Vec<(String, String, Value)>;

/// Plan every content object one review decision retains, bounding each one
/// before ANY effect: an oversized override — or a later oversized fragment
/// after valid earlier ones — can neither consume the candidate nor leave
/// partial records.
///
/// The decision carries no placement: created records inherit the candidate's
/// admitted capture scope (§7.15).
fn plan_review_content(
    candidate: &StoredCandidate,
    decision: &Decision,
) -> Result<(String, PlannedContent), SpokeReject> {
    // §7.1/§7.15: plan every content object this decision will retain and
    // bound each one before ANY effect, so an oversized override — or a later
    // oversized fragment after valid earlier ones — cannot consume the
    // candidate or leave partial records. The decision carries no placement:
    // created records inherit the candidate's admitted capture scope (§7.15).
    let (outcome, planned) = match decision {
        Decision::Promote {
            content_override,
            kind,
        } => {
            let kind = kind
                .as_ref()
                .map_or(candidate.kind.as_str(), |kind| kind.as_str());
            let (field, content) = match content_override {
                Some(overridden) => (
                    "decision.content_override",
                    serde_json::to_value(overridden).map_err(internal_failure)?,
                ),
                None => (
                    "content",
                    stored_json(
                        candidate
                            .content
                            .as_deref()
                            .ok_or_else(|| internal("a pending candidate carries no content"))?,
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
                let content = serde_json::to_value(&fragment.content).map_err(internal_failure)?;
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
    Ok((outcome, planned))
}

/// The candidates this caller may see in the current snapshot, already filtered
/// by status, expiry and each candidate's immutable capture scope.
async fn visible_candidates(
    conn: &mut SqliteConnection,
    grants: &[MemoryGrant],
    principal_id: &str,
    request: &MemoryCandidatesListRequest,
    target: &CaptureTarget,
    now: DateTime<Utc>,
) -> Result<Vec<StoredCandidate>, SpokeReject> {
    let space_id = request.space_id.as_str();
    let subject_id = request.subject_id.as_str();
    let status_filter = request.status.map(status_name);
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
            // Each candidate is filtered by its immutable capture scope, so a
            // context-limited review grant never sees another placement.
            if !is_authorized(
                grants,
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
    Ok(visible)
}

impl FixtureMemoryProvider {
    // -----------------------------------------------------------------------
    // memory.capture (§7.11)
    // -----------------------------------------------------------------------

    /// Capture raw material for one subject; commit the candidates, the receipt
    /// and the dedup binding in one transaction.
    ///
    /// The fault points bracket the real commit: an admission pause runs before
    /// the writer transaction, a pre-commit loss rolls the staged transaction
    /// back, and a post-commit loss withholds the response of a durable effect.
    pub(crate) async fn capture(
        &self,
        principal_id: &str,
        admitted: &Admitted<MemoryCaptureRequest>,
        now: DateTime<Utc>,
    ) -> Result<MemoryReceipt, SpokeReject> {
        let space_id = admitted.request().space_id.as_str();
        require_served_space(self, space_id)?;
        // Bounded test-only admission pause: it runs before the writer
        // transaction, so a competing identical retry can commit while this
        // admission waits and this one then replays that receipt. A hold that
        // ends early is a clean no-effect failure: nothing was staged yet.
        self.faults()
            .hold(FaultPoint::AdmissionPause)
            .await
            .map_err(|_| fault_before_effect(FaultPoint::AdmissionPause))?;
        let mut transaction = self
            .store()
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(internal_failure)?;
        let receipt = self
            .capture_in_tx(&mut transaction, principal_id, admitted, now)
            .await?;
        // A cancellation inside the staged-but-uncommitted window: the caller
        // drops this future, so the transaction is never committed and nothing
        // it staged can become durable. An expired control ends the wait with
        // the same no-effect outcome.
        self.faults()
            .hold(FaultPoint::BeforeCommit)
            .await
            .map_err(|_| fault_before_effect(FaultPoint::BeforeCommit))?;
        // Loss before commit (§14.3): dropping the staged transaction leaves no
        // candidate, record, receipt, dedup row, target or marker behind.
        if self.faults().trip(FaultPoint::BeforeCommit) {
            transaction.rollback().await.map_err(internal_failure)?;
            return Err(outcome_uncertain());
        }
        transaction.commit().await.map_err(|_| commit_unknown())?;
        // Loss after commit, before the response: the effect is durable and the
        // caller must resolve the uncertainty through replay or receipts.get.
        if self.faults().trip(FaultPoint::AfterCommitBeforeResponse) {
            return Err(outcome_uncertain());
        }
        // Response drop: the committed answer is held until the caller tears the
        // connection down. The effect is already durable, so a hold that ends
        // without a release is an uncertain outcome, never a clean failure.
        self.faults()
            .hold(FaultPoint::ResponseDrop)
            .await
            .map_err(|_| outcome_uncertain())?;
        Ok(receipt)
    }

    /// §6.10/MEM-R021 explicit operator maintenance at one caller-supplied
    /// instant (the C7 retention seam).
    ///
    /// This is the only physical reclamation path: reads, refusals and mutation
    /// wrappers never call it, because a refused operation must not commit
    /// unrelated cleanup. It deletes only rows whose persisted lifecycle
    /// deadline has passed, never a pending receipt, and never advances the
    /// snapshot marker: retention is provider maintenance, not an observable
    /// space change, so a read that samples the clock must not invalidate
    /// outstanding cursors.
    ///
    /// # Errors
    ///
    /// Returns the protocol refusal when the space is not served by this
    /// fixture or the statements fail; the pass is one transaction, so a failure
    /// leaves every row in place.
    pub async fn sweep_retained(
        &self,
        space_id: &str,
        now: DateTime<Utc>,
    ) -> Result<RetentionSweep, SpokeReject> {
        require_served_space(self, space_id)?;
        let mut transaction = self
            .store()
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(internal_failure)?;
        let sweep = retention_sweep_in_tx(&mut transaction, space_id, now).await?;
        transaction.commit().await.map_err(internal_failure)?;
        Ok(sweep)
    }

    /// Stage one durable pending candidate under a caller-named identity.
    ///
    /// **Privileged fixture operator construction, not a client or wire
    /// operation.** The protocol's capture op always mints its own candidate
    /// id, while Nexus addresses a legacy-bridge candidate by the deterministic
    /// id it derives from a retained pending id. The conformance suite therefore
    /// cannot create that shape through the wire, and this operator provides the
    /// durable row the real dismissal then fetches: it reuses this slice's own
    /// classification, encoding, lifecycle and TTL, writes exactly one row of
    /// the normal schema at `revision = 1`, and refuses a duplicate id. It is
    /// never remotely invokable and no other operation changes.
    ///
    /// `placement` is `None` for the shared scope or the placement context id
    /// for a binding-local one; `pending_id` is the retained Nexus identity the
    /// row's `content.structured.nexus_legacy` block carries.
    ///
    /// # Errors
    ///
    /// [`FixtureError::Bootstrap`] when the space is not this provider's, the
    /// row cannot be encoded, or the id already exists;
    /// [`FixtureError::Storage`] when the insert itself fails.
    pub async fn stage_pending_candidate(
        &self,
        candidate_id: &str,
        subject_id: &str,
        placement: Option<&str>,
        pending_id: &str,
        text: &str,
    ) -> Result<(), FixtureError> {
        let space_id = self.identity.space_id.as_str();
        require_served_space(self, space_id)
            .map_err(|reject| stage_error(reject.code.as_str(), candidate_id))?;
        let content = json!({
            "text": text,
            "structured": {
                "nexus_legacy": {
                    "pending_id": pending_id,
                    "session_id": format!("sess_{pending_id}"),
                    "task_kind": "brainstorm",
                    "raw_digest": text,
                },
            },
        });
        let capture_content = CaptureContent {
            structured: content.get("structured").cloned(),
            text: text.to_owned(),
        };
        let classification = classify(&capture_content);
        let now = Utc::now();
        let provenance = json!({
            "source_context_ids": placement.map(|context| vec![context]).unwrap_or_default(),
            "anchors": [],
            "captured_by": "nexus:fixture-operator",
            "captured_at": timestamp_text(now),
        });
        let visibility = if placement.is_some() {
            "context"
        } else {
            "shared"
        };
        let encoded_content =
            encoded(&content).map_err(|reject| stage_error(reject.code.as_str(), candidate_id))?;
        let encoded_provenance = encoded(&provenance)
            .map_err(|reject| stage_error(reject.code.as_str(), candidate_id))?;
        let encoded_classification = encoded(&classification)
            .map_err(|reject| stage_error(reject.code.as_str(), candidate_id))?;
        let insert = sqlx::query(
            "INSERT INTO candidates (candidate_id, space_id, subject_id, capture_visibility, \
             capture_context_id, kind, content_json, provenance_json, classification_json, \
             status, occurred_at, expires_at, revision, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 'pending', NULL, ?, 1, ?)",
        )
        .bind(candidate_id)
        .bind(space_id)
        .bind(subject_id)
        .bind(visibility)
        .bind(placement)
        .bind(DEFAULT_CANDIDATE_KIND)
        .bind(encoded_content)
        .bind(encoded_provenance)
        .bind(encoded_classification)
        .bind(timestamp_text(now + Duration::days(CANDIDATE_TTL_DAYS)))
        .bind(timestamp_text(now))
        .execute(self.store().pool())
        .await;
        match insert {
            Ok(result) if result.rows_affected() == 1 => Ok(()),
            Ok(_) => Err(stage_error("staged_candidate_not_inserted", candidate_id)),
            Err(error)
                if error
                    .as_database_error()
                    .is_some_and(sqlx::error::DatabaseError::is_unique_violation) =>
            {
                Err(stage_error("duplicate_candidate_id", candidate_id))
            }
            Err(error) => Err(FixtureError::Storage(error)),
        }
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
        let admission = Mutation::keyed(OP_CAPTURE, request.idempotency_key.as_str(), admitted);
        let target = ReceiptTarget::for_scope(
            required_capabilities(OP_CAPTURE)
                .expect("memory.capture has a published capability list"),
            subject_id,
            visibility,
            context_id,
        )?;
        // Resolve and authorize an EXISTING binding first: its retained target
        // (and the payload digest) decide the replay, conflict or retention
        // outcome, so a changed request naming a target the caller cannot reach
        // must still answer `IDEMPOTENCY_CONFLICT` rather than an authorization
        // refusal. Only a fresh (or fully reusable) key authorizes the request's
        // current target.
        if let Some(replay) =
            resolve_or_release(conn, space_id, principal_id, &admission, now).await?
        {
            return Ok(replay);
        }
        let grants = load_grants(conn, space_id).await?;
        require_one_grant(
            &grants,
            principal_id,
            space_id,
            now,
            &[scope_requirement(
                "capture", subject_id, visibility, context_id,
            )],
        )?;
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
            let content = request.content.structured.as_ref().map_or_else(
                || json!({ "text": request.content.text }),
                |structured| {
                    json!({
                        "text": request.content.text,
                        "structured": structured,
                    })
                },
            );
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
            .map_err(internal_failure)?;
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
        let mut transaction = self
            .store()
            .pool()
            .begin()
            .await
            .map_err(internal_failure)?;
        let page = self
            .candidates_list_in_tx(&mut transaction, principal_id, request, now)
            .await?;
        transaction.commit().await.map_err(internal_failure)?;
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

        let status_filter = request.status.map(status_name);
        let visible =
            visible_candidates(conn, &grants, principal_id, request, &target, now).await?;

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
        let start = cursor_start(
            &binding_digest,
            &visible,
            request.cursor.as_ref().map(|cursor| cursor.as_str()),
        )?;
        let limit = usize::try_from(
            request
                .limit
                .map_or(MAX_PAGE_SIZE.get(), std::num::NonZeroU64::get),
        )
        .map_err(internal_failure)?;
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
        let mut transaction = self
            .store()
            .pool()
            .begin()
            .await
            .map_err(internal_failure)?;
        let candidate = self
            .candidates_get_in_tx(&mut transaction, principal_id, request, now)
            .await?;
        transaction.commit().await.map_err(internal_failure)?;
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
            .ok_or_else(candidate_unavailable)?;
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
            .map_err(internal_failure)?;
        let receipt = self
            .candidates_dismiss_in_tx(&mut transaction, principal_id, admitted, now)
            .await?;
        transaction.commit().await.map_err(|_| commit_unknown())?;
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
        // Replay resolution runs before the target lookup (I-000305): a
        // retained key must resolve its receipt even when the target row is
        // gone, and a changed target under the same key fails with
        // IDEMPOTENCY_CONFLICT instead of replaying. `Fresh` and the
        // expired-binding release fall through to the untouched grant/target
        // path below.
        let admission = Mutation::keyed(
            OP_CANDIDATES_DISMISS,
            request.idempotency_key.as_str(),
            admitted,
        );
        if let Some(replay) =
            resolve_or_release(conn, space_id, principal_id, &admission, now).await?
        {
            return Ok(replay);
        }
        let grants = load_grants(conn, space_id).await?;
        let candidate = fetch_candidate(conn, space_id, candidate_id)
            .await?
            .ok_or_else(candidate_unavailable)?;
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
        let target = ReceiptTarget::for_scope(
            required_capabilities(OP_CANDIDATES_DISMISS)
                .expect("memory.candidates.dismiss has a published capability list"),
            &candidate.subject_id,
            &candidate.capture_visibility,
            candidate.scope_context(),
        )?;
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
            .map_err(internal_failure)?;
        let receipt = self
            .candidates_review_in_tx(&mut transaction, principal_id, admitted, now)
            .await?;
        transaction.commit().await.map_err(|_| commit_unknown())?;
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
        // Replay resolution runs before the target lookup (I-000305): a
        // retained key must resolve its receipt even when the target row is
        // gone, and a changed target under the same key fails with
        // IDEMPOTENCY_CONFLICT instead of replaying. `Fresh` and the
        // expired-binding release fall through to the untouched grant/target
        // path below.
        let admission = Mutation::keyed(
            OP_CANDIDATES_REVIEW,
            request.idempotency_key.as_str(),
            admitted,
        );
        if let Some(replay) =
            resolve_or_release(conn, space_id, principal_id, &admission, now).await?
        {
            return Ok(replay);
        }
        let grants = load_grants(conn, space_id).await?;
        let candidate = fetch_candidate(conn, space_id, candidate_id)
            .await?
            .ok_or_else(candidate_unavailable)?;
        let subject_id = candidate.subject_id.clone();
        let visibility = candidate.capture_visibility.clone();
        let context_id = candidate.scope_context().map(str::to_owned);
        require_review_authority(
            &grants,
            principal_id,
            space_id,
            &subject_id,
            &visibility,
            context_id.as_deref(),
            now,
        )?;
        let target = ReceiptTarget::for_scope(
            required_capabilities(OP_CANDIDATES_REVIEW)
                .expect("memory.candidates.review has a published capability list"),
            &subject_id,
            &visibility,
            context_id.as_deref(),
        )?;
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
        let (outcome, planned) = plan_review_content(&candidate, &request.decision)?;
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

    /// A receipt read is persistently read-only (MEM-R011/§7.28): it opens one
    /// snapshot and never commits a cleanup, whatever the requested receipt or
    /// the caller's authorization is. Physical reclamation belongs to the
    /// explicit operator [`Self::sweep_retained`] pass.
    pub(crate) async fn receipts_get(
        &self,
        principal_id: &str,
        request: &MemoryReceiptsGetRequest,
        now: DateTime<Utc>,
    ) -> Result<MemoryReceipt, SpokeReject> {
        let mut transaction = self
            .store()
            .pool()
            .begin()
            .await
            .map_err(internal_failure)?;
        let receipt = self
            .receipts_get_in_tx(&mut transaction, principal_id, request, now)
            .await?;
        transaction.commit().await.map_err(internal_failure)?;
        Ok(receipt)
    }

    /// Receipt read on the caller's snapshot: current authorization, the
    /// receipt row and its dedup tombstone are read together (MEM-R018/R026),
    /// and the logical retention state is evaluated at the supplied instant, so
    /// an unswept old row answers exactly like a reclaimed one. Nothing here
    /// writes.
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
        let requester = if admin { None } else { Some(principal_id) };
        // §6.5/§7.28 + §7.0.5/MEM-R021: the recorded target is rechecked against
        // current authorization before any body or tombstone status is
        // disclosed, and it is recorded independently of the body so a retired
        // body rechecks the same subject and placement. A non-admin can only
        // ever see its own receipts, so another principal's receipt id stays
        // indistinguishable from a missing one (no existence oracle).
        let target = fetch_receipt_target(conn, receipt_id).await?;
        let caller_covers_target = target
            .as_ref()
            .is_some_and(|target| target.admits(&grants, principal_id, space_id, now));
        let tombstone = fetch_tombstone(conn, space_id, receipt_id, requester).await?;
        let stored = fetch_receipt(conn, space_id, receipt_id, requester).await?;
        match ReceiptRetention::of(tombstone.as_ref(), stored.as_ref(), now)? {
            // The whole lifetime ended (or there was never a visible row): a
            // foreign receipt and a missing one stay indistinguishable.
            ReceiptRetention::Expired => Err(not_found(
                "no receipt is visible to the authenticated principal",
            )),
            // A retired body's terminal status is disclosed only after the
            // recorded target's recheck passes.
            ReceiptRetention::Evicted { last_known_status } => {
                if !admin && !caller_covers_target {
                    return Err(not_authorized(RECEIPT_UNAUTHORIZED));
                }
                Err(receipt_expired(&last_known_status))
            }
            ReceiptRetention::Retained => {
                if !admin && !caller_covers_target {
                    return Err(not_authorized(RECEIPT_UNAUTHORIZED));
                }
                let stored = stored.ok_or_else(|| {
                    internal(format!("retained receipt {receipt_id} is not readable"))
                })?;
                receipt_to_wire(&stored)
            }
        }
    }
}

fn placement(scope: &CapturePlacement) -> (&'static str, Option<&str>) {
    match scope {
        CapturePlacement::Shared => ("shared", None),
        CapturePlacement::Context(context_id) => ("context", Some(context_id.as_str())),
    }
}

/// One operator-staging failure, reported as fixture construction state.
fn stage_error(code: &str, candidate_id: &str) -> FixtureError {
    FixtureError::Bootstrap(format!("candidate {candidate_id} cannot be staged: {code}"))
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
    .map_err(internal_failure)?;
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
    .map_err(internal_failure)?;
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
        MemorySubjectsEnsureRequest, MemorySubjectsFenceRequest,
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
            Self { provider, ..self }
        }

        /// Capture one candidate and return its id.
        async fn captured_candidate(
            &self,
            key: &str,
            text: &str,
            captured_at: DateTime<Utc>,
        ) -> String {
            let captured = self
                .provider
                .capture(
                    CLIENT,
                    &capture_request(&self.space_id, text, context_scope(CONTEXT_A), key),
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
            let admitted = Admitted::from_scope(digest(&payload).expect("payload digest"), ());
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
        let mut payload = json!({
            "space_id": space,
            "subject_id": SUBJECT,
            "content": { "text": text },
            "kind_hint": "fact",
            "idempotency_key": key,
        });
        payload["capture_scope"] = scope;
        payload
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
        let mut value = json!({
            "space_id": space,
            "candidate_id": candidate_id,
            "expected_revision": revision,
            "idempotency_key": key,
        });
        value["decision"] = decision;
        admit_candidates_review(value).expect("valid candidates.review request")
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
            other @ MemoryCandidate::ConsumedCandidate(_) => {
                panic!("expected a pending candidate, observed {other:?}")
            }
        }
    }

    fn consumed_candidate(candidate: &MemoryCandidate) -> &ConsumedCandidate {
        match candidate {
            MemoryCandidate::ConsumedCandidate(consumed) => consumed,
            other @ MemoryCandidate::PendingCandidate(_) => {
                panic!("expected a consumed candidate, observed {other:?}")
            }
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
        let row = sqlx::query("SELECT COUNT(*) AS total FROM idempotency_namespace WHERE op = ?")
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
        let request = capture_request(
            &scenario.space_id,
            secret,
            context_scope(CONTEXT_A),
            "cap-1",
        );

        let first = scenario
            .provider
            .capture(CLIENT, &request, now)
            .await
            .expect("capture commits");
        assert_eq!(receipt_status(&first), "committed");
        let result = committed_result(&first);
        assert_eq!(result["classified"], 1);
        let ids = result["candidate_ids"]
            .as_array()
            .expect("candidate ids")
            .iter()
            .map(|id| id.as_str().expect("string id"));
        assert_eq!(ids.count(), 1, "one pending candidate is produced");
        assert_no_secret(
            &serde_json::to_string(&first).expect("receipt encodes"),
            secret,
        );
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
        let request = capture_request(
            &scenario.space_id,
            "Kept draft.",
            context_scope(CONTEXT_A),
            "cap-1",
        );
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
        let fresh = capture_request(
            &scenario.space_id,
            "After freeze.",
            context_scope(CONTEXT_A),
            "cap-2",
        );
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
                .receipts_get(
                    CLIENT,
                    &receipts_request(&scenario.space_id, receipt_id_of(&committed)),
                    now,
                )
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
        let (candidate_id, dismissal) =
            dismiss_the_captured_candidate(&scenario, now, secret).await;
        the_dismissal_is_terminal_and_replayable(&scenario, now, &candidate_id, &dismissal).await;
    }

    async fn dismiss_the_captured_candidate(
        scenario: &Scenario,
        now: DateTime<Utc>,
        secret: &str,
    ) -> (String, MemoryReceipt) {
        let candidate_id = scenario.captured_candidate("cap-1", secret, now).await;

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
            serde_json::to_value(consumed.consumed_by.outcome).expect("outcome encodes"),
            json!("dismissed")
        );
        assert_eq!(consumed.content_digest.len(), 64);
        assert!(consumed.proposed.is_some(), "the shell keeps its kind");
        assert_no_secret(
            &serde_json::to_string(&shell).expect("shell encodes"),
            secret,
        );

        let stored: Option<String> =
            sqlx::query("SELECT content_json FROM candidates WHERE candidate_id = ?")
                .bind(&candidate_id)
                .fetch_one(scenario.provider.store().pool())
                .await
                .expect("candidate row")
                .get("content_json");
        assert!(stored.is_none(), "consumption removes the stored body");
        (candidate_id, dismissal)
    }

    async fn the_dismissal_is_terminal_and_replayable(
        scenario: &Scenario,
        now: DateTime<Utc>,
        candidate_id: &str,
        dismissal: &MemoryReceipt,
    ) {
        // Consumption is terminal and outranks the CAS precondition.
        assert_eq!(
            refusal(
                scenario
                    .provider
                    .candidates_dismiss(
                        CLIENT,
                        &dismiss_request(&scenario.space_id, candidate_id, 2, "dis-2"),
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
                            candidate_id,
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
                &dismiss_request(&scenario.space_id, candidate_id, 1, "dis-1"),
                now,
            )
            .await
            .expect("dismissal replay answers");
        assert_eq!(receipt_id_of(&replay), receipt_id_of(dismissal));

        let consumed_page = scenario
            .provider
            .candidates_list(
                CLIENT,
                &list_request(&scenario.space_id, Some("consumed"), None, None),
                now,
            )
            .await
            .expect("consumed page");
        assert_eq!(candidate_ids(&consumed_page), vec![candidate_id.to_owned()]);
        let pending_page = scenario
            .provider
            .candidates_list(
                CLIENT,
                &list_request(&scenario.space_id, Some("pending"), None, None),
                now,
            )
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
            .grant(CLIENT, &["capture", "review"], Some(SUBJECT), None, now)
            .await;
        scenario
            .grant(OTHER, &["records_write"], Some(SUBJECT), None, now)
            .await;
        let captured_text = "The gate key is under the third barrel.";
        let candidate_id = review_alone_cannot_promote(&scenario, now, captured_text).await;
        a_dual_grant_promotes_into_the_captured_scope(&scenario, now, captured_text, &candidate_id)
            .await;
    }

    async fn review_alone_cannot_promote(
        scenario: &Scenario,
        now: DateTime<Utc>,
        captured_text: &str,
    ) -> String {
        let candidate_id = scenario
            .captured_candidate("cap-1", captured_text, now)
            .await;
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
        candidate_id
    }

    async fn a_dual_grant_promotes_into_the_captured_scope(
        scenario: &Scenario,
        now: DateTime<Utc>,
        captured_text: &str,
        candidate_id: &str,
    ) {
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
                    candidate_id,
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
            .candidates_get(CLIENT, &get_request(&scenario.space_id, candidate_id), now)
            .await
            .expect("consumed shell");
        let consumed = consumed_candidate(&shell);
        assert_eq!(
            serde_json::to_value(consumed.consumed_by.outcome).expect("outcome encodes"),
            json!("promoted")
        );
        assert_eq!(consumed.consumed_by.record_ids, vec![record_ids[0].clone()]);
        assert_eq!(
            consumed.content_digest.as_str(),
            digest(&json!({ "text": captured_text })).expect("digest"),
            "the shell keeps the digest of the scrubbed body"
        );
        assert_no_secret(
            &serde_json::to_string(&shell).expect("shell encodes"),
            captured_text,
        );
    }

    #[tokio::test]
    async fn review_cannot_escape_the_admitted_capture_scope() {
        let scenario = Scenario::open().await;
        let now = now();
        scenario.subject(now).await;
        scenario
            .grant(CLIENT, &["capture"], Some(SUBJECT), None, now)
            .await;
        let candidate_id = restricted_review_never_sees_a_shared_placement(&scenario, now).await;
        shared_review_grants_promote_the_shared_scope(&scenario, now, &candidate_id).await;
    }

    async fn restricted_review_never_sees_a_shared_placement(
        scenario: &Scenario,
        now: DateTime<Utc>,
    ) -> String {
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
                &capture_request(
                    &scenario.space_id,
                    "Shared world fact.",
                    shared_scope(),
                    "cap-1",
                ),
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
            .candidates_list(
                CLIENT,
                &list_request(&scenario.space_id, None, None, None),
                now,
            )
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
        candidate_id
    }

    async fn shared_review_grants_promote_the_shared_scope(
        scenario: &Scenario,
        now: DateTime<Utc>,
        candidate_id: &str,
    ) {
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
                    candidate_id,
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
        let record = sqlx::query(
            "SELECT scope_visibility, scope_context_id FROM records WHERE record_id = ?",
        )
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
                &capture_request(
                    &scenario.space_id,
                    "Contended capture.",
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
        for outcome in std::iter::once(left).chain(std::iter::once(right)) {
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
                &capture_request(
                    &scenario.space_id,
                    "Receipt subject text.",
                    shared_scope(),
                    "cap-1",
                ),
                now,
            )
            .await
            .expect("capture commits");
        let receipt_id = receipt_id_of(&captured).to_owned();

        let read = scenario
            .provider
            .receipts_get(
                CLIENT,
                &receipts_request(&scenario.space_id, &receipt_id),
                now,
            )
            .await
            .expect("the requester reads its own receipt");
        assert_eq!(receipt_status(&read), "committed");

        // A foreign principal without admin coverage cannot tell this receipt
        // from a missing one.
        let foreign = refusal(
            scenario
                .provider
                .receipts_get(
                    OTHER,
                    &receipts_request(&scenario.space_id, &receipt_id),
                    now,
                )
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
            .receipts_get(
                OPERATOR,
                &receipts_request(&scenario.space_id, &receipt_id),
                now,
            )
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
                .receipts_get(
                    CLIENT,
                    &receipts_request(&scenario.space_id, &receipt_id),
                    now,
                )
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
        a_pending_receipt_is_durable_and_counted(&scenario, now).await;
        a_failed_receipt_keeps_its_coded_envelope(&scenario, now).await;
        receipts_and_replay_survive_reopen(scenario, now).await;
    }

    async fn a_pending_receipt_is_durable_and_counted(scenario: &Scenario, now: DateTime<Utc>) {
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
            .receipts_get(CLIENT, &receipts_request(&space, "rcpt_pending_probe"), now)
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
        assert_eq!(
            tombstone(scenario, "async-1").await.as_deref(),
            Some("committed")
        );
    }

    async fn a_failed_receipt_keeps_its_coded_envelope(scenario: &Scenario, now: DateTime<Utc>) {
        let space = scenario.space_id.clone();
        let probe_target = capture_receipt_target(CONTEXT_A);
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
    }

    async fn receipts_and_replay_survive_reopen(scenario: Scenario, now: DateTime<Utc>) {
        let space = scenario.space_id.clone();
        let failed_admitted = admit_capture(capture_payload(
            &space,
            "Failed probe text.",
            context_scope(CONTEXT_A),
            "async-2",
        ))
        .expect("valid capture payload");
        let failed_admission = Mutation::keyed(OP_CAPTURE, "async-2", &failed_admitted);
        // Replay answers the stored receipt whatever its state (§14.3).
        let mut transaction = scenario
            .provider
            .store()
            .pool()
            .begin()
            .await
            .expect("reader transaction");
        let replayed =
            match resolve_replay(&mut transaction, &space, CLIENT, &failed_admission, now)
                .await
                .expect("replay resolves")
            {
                ReplayResolution::Replayed(receipt) => *receipt,
                other => panic!("expected the bound receipt, observed {other:?}"),
            };
        transaction.commit().await.expect("read commits");
        assert_eq!(receipt_id_of(&replayed), "rcpt_failed_probe");
        assert_eq!(receipt_status(&replayed), "failed");
        assert_eq!(
            tombstone(&scenario, "async-2").await.as_deref(),
            Some("failed")
        );

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
        let (cursor, first) = ctx_a_candidates_first_page_and_scope(&scenario, now).await;
        let union = second_page_covers_the_same_scope(&scenario, now, &cursor, &first).await;
        a_state_change_and_expiry_apply_to_the_cursor(&scenario, now, &cursor, &union).await;
    }

    async fn ctx_a_candidates_first_page_and_scope(
        scenario: &Scenario,
        now: DateTime<Utc>,
    ) -> (String, Vec<String>) {
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
            .candidates_list(
                CLIENT,
                &list_request(&scenario.space_id, None, Some(2), None),
                now,
            )
            .await
            .expect("first page");
        assert_eq!(first.candidates.len(), 2);
        let cursor = first
            .next_cursor
            .clone()
            .expect("a bounded page issues a cursor");
        for candidate in &first.candidates {
            assert_eq!(
                serde_json::to_value(&pending_candidate(candidate).capture_scope)
                    .expect("scope encodes"),
                context_scope(CONTEXT_A)
            );
        }
        (cursor, candidate_ids(&first))
    }

    async fn second_page_covers_the_same_scope(
        scenario: &Scenario,
        now: DateTime<Utc>,
        cursor: &str,
        first: &[String],
    ) -> Vec<String> {
        let second = scenario
            .provider
            .candidates_list(
                CLIENT,
                &list_request(&scenario.space_id, None, Some(2), Some(cursor)),
                now,
            )
            .await
            .expect("second page");
        assert_eq!(second.candidates.len(), 1);
        assert!(second.next_cursor.is_none());
        let mut union = first.to_vec();
        union.extend(candidate_ids(&second));
        union.sort();
        union.dedup();
        assert_eq!(
            union.len(),
            3,
            "the pages cover exactly the ctx-a candidates, never the ctx-b row"
        );
        union
    }

    async fn a_state_change_and_expiry_apply_to_the_cursor(
        scenario: &Scenario,
        now: DateTime<Utc>,
        cursor: &str,
        union: &[String],
    ) {
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
                        &list_request(&scenario.space_id, None, Some(2), Some(cursor)),
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
            .candidates_list(
                CLIENT,
                &list_request(&scenario.space_id, None, None, None),
                expired_at,
            )
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

        let ungranted = capture_request(
            &scenario.space_id,
            "Ungranted text.",
            context_scope(CONTEXT_A),
            "cap-1",
        );
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
        scenario.grant(CLIENT, &["capture"], None, None, now).await;
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

        let oversize = capture_request(
            &scenario.space_id,
            &"x".repeat(MAX_CONTENT_BYTES + 1),
            context_scope(CONTEXT_A),
            "cap-big",
        );
        assert_eq!(
            refusal(scenario.provider.capture(CLIENT, &oversize, now).await).code,
            SpokeRejectCode::InvalidInput
        );

        scenario.fence_context(CONTEXT_A, now).await;
        let fenced = capture_request(
            &scenario.space_id,
            "Fenced text.",
            context_scope(CONTEXT_A),
            "cap-fenced",
        );
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
                &capture_request(
                    &scenario.space_id,
                    "Sibling text.",
                    context_scope(CONTEXT_B),
                    "cap-sibling",
                ),
                now,
            )
            .await
            .expect("the sibling context is admitted");
        assert_eq!(count_rows(&scenario.provider, "candidates").await, 1);
        assert_eq!(count_rows(&scenario.provider, "receipts").await, 1);

        scenario.freeze(now).await;
        let frozen = capture_request(
            &scenario.space_id,
            "Frozen text.",
            context_scope(CONTEXT_B),
            "cap-frozen",
        );
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
        assert_no_secret(
            &serde_json::to_string(&receipt).expect("receipt encodes"),
            secret,
        );
        let shell = scenario
            .provider
            .candidates_get(CLIENT, &get_request(&scenario.space_id, &candidate_id), now)
            .await
            .expect("shell still resolves");
        assert_no_secret(
            &serde_json::to_string(&shell).expect("shell encodes"),
            secret,
        );

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
                .candidates_get(CLIENT, &get_request(&scenario.space_id, "cand_absent"), now)
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

        the_refused_probes_left_no_trace(&scenario, now, &candidate_id, receipts_after_capture)
            .await;

        an_out_of_scope_review_grant_answers_like_a_missing_id(
            &scenario,
            now,
            &candidate_id,
            &get_missing,
        )
        .await;
    }

    async fn the_refused_probes_left_no_trace(
        scenario: &Scenario,
        now: DateTime<Utc>,
        candidate_id: &str,
        receipts_after_capture: i64,
    ) {
        // The candidate is untouched and nothing was recorded.
        let untouched = scenario
            .provider
            .candidates_get(
                OPERATOR,
                &get_request(&scenario.space_id, candidate_id),
                now,
            )
            .await
            .expect("the admin can still read the candidate");
        assert_eq!(pending_candidate(&untouched).revision.get(), 1);
        assert_eq!(
            count_rows(&scenario.provider, "receipts").await,
            receipts_after_capture
        );
    }

    async fn an_out_of_scope_review_grant_answers_like_a_missing_id(
        scenario: &Scenario,
        now: DateTime<Utc>,
        candidate_id: &str,
        missing: &SpokeReject,
    ) {
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
                .candidates_get(CLIENT, &get_request(&scenario.space_id, candidate_id), now)
                .await,
        );
        assert_eq!(&out_of_scope, missing);
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
                &capture_request(
                    &scenario.space_id,
                    "Receipt body.",
                    context_scope(CONTEXT_A),
                    "cap-1",
                ),
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
                .receipts_get(
                    CLIENT,
                    &receipts_request(&scenario.space_id, &receipt_id),
                    now,
                )
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
                .receipts_get(
                    CLIENT,
                    &receipts_request(&scenario.space_id, &receipt_id),
                    now,
                )
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
                .receipts_get(
                    CLIENT,
                    &receipts_request(&scenario.space_id, &receipt_id),
                    now,
                )
                .await,
        );
        assert_eq!(denied_without_grants, denied_body);

        // A space admin still resolves the evicted body from the tombstone.
        let admin = refusal(
            scenario
                .provider
                .receipts_get(
                    OPERATOR,
                    &receipts_request(&scenario.space_id, &receipt_id),
                    now,
                )
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
        let (candidate_ids, dismissal) =
            seed_then_dismiss_the_first_candidate(&scenario, now).await;
        the_fence_replays_the_admitted_dismissal(&scenario, now, &candidate_ids, &dismissal).await;
        an_archived_subject_leaves_the_listing_and_keeps_reads(&scenario, now, &candidate_ids[1])
            .await;
    }

    /// Two queued captures plus the dismissal of the first one.
    async fn seed_then_dismiss_the_first_candidate(
        scenario: &Scenario,
        now: DateTime<Utc>,
    ) -> (Vec<String>, MemoryReceipt) {
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
        for (index, text) in ["First queue item.", "Second queue item."]
            .into_iter()
            .enumerate()
        {
            candidate_ids.push(
                scenario
                    .captured_candidate(&format!("cap-{index}"), text, now)
                    .await,
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
        (candidate_ids, dismissal)
    }

    async fn the_fence_replays_the_admitted_dismissal(
        scenario: &Scenario,
        now: DateTime<Utc>,
        candidate_ids: &[String],
        dismissal: &MemoryReceipt,
    ) {
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
        assert_eq!(receipt_id_of(&replay), receipt_id_of(dismissal));

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
            .candidates_list(
                CLIENT,
                &list_request(&scenario.space_id, None, None, None),
                now,
            )
            .await
            .expect("fenced subject page");
        assert_eq!(fenced_page.candidates.len(), 2);
    }

    async fn an_archived_subject_leaves_the_listing_and_keeps_reads(
        scenario: &Scenario,
        now: DateTime<Utc>,
        candidate_id: &str,
    ) {
        // `archived` is terminal decommission and leaves the listing.
        scenario.fence_subject("archive", 2, now).await;
        let archived_page = scenario
            .provider
            .candidates_list(
                CLIENT,
                &list_request(&scenario.space_id, None, None, None),
                now,
            )
            .await
            .expect("archived subject page");
        assert!(
            archived_page.candidates.is_empty(),
            "an archived subject is excluded from the candidate listing"
        );
        // Reads stay allowed, and so does the refusal of further writes.
        let readable = scenario
            .provider
            .candidates_get(CLIENT, &get_request(&scenario.space_id, candidate_id), now)
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
                            candidate_id,
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
        let mut reader = scenario
            .provider
            .store()
            .pool()
            .begin()
            .await
            .expect("reader");
        assert!(
            matches!(
                resolve_replay(&mut reader, &space, OPERATOR, &admission, now)
                    .await
                    .expect("replay resolves"),
                ReplayResolution::Fresh
            ),
            "a keyless mutation is never a replay"
        );
        reader.rollback().await.expect("read transaction closes");

        a_rolled_back_keyless_stage_leaves_neither_row(&scenario, &space, now).await;
    }

    async fn a_rolled_back_keyless_stage_leaves_neither_row(
        scenario: &Scenario,
        space: &str,
        now: DateTime<Utc>,
    ) {
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
            space,
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
                .receipts_get(
                    CLIENT,
                    &receipts_request(&scenario.space_id, &receipt_id),
                    now,
                )
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
            .receipts_get(
                CLIENT,
                &receipts_request(&scenario.space_id, &receipt_id),
                now,
            )
            .await
            .expect("a covering grant reopens the receipt");
        assert_eq!(receipt_id_of(&allowed), receipt_id);

        the_evicted_body_answers_exactly_like_the_wrong_subject(
            &scenario,
            now,
            &receipt_id,
            &wrong_subject,
            &covering,
        )
        .await;
    }

    async fn the_evicted_body_answers_exactly_like_the_wrong_subject(
        scenario: &Scenario,
        now: DateTime<Utc>,
        receipt_id: &str,
        wrong_subject: &SpokeReject,
        covering: &str,
    ) {
        // Revoke again and evict the body: the tombstone must not reveal the
        // terminal status to the same wrong-target principal.
        scenario.revoke(covering, now).await;
        sqlx::query("DELETE FROM receipts WHERE receipt_id = ?")
            .bind(receipt_id)
            .execute(scenario.provider.store().pool())
            .await
            .expect("receipt body evicted");
        let wrong_subject_tombstone = refusal(
            scenario
                .provider
                .receipts_get(
                    CLIENT,
                    &receipts_request(&scenario.space_id, receipt_id),
                    now,
                )
                .await,
        );
        assert_eq!(
            &wrong_subject_tombstone, wrong_subject,
            "the evicted path answers exactly like the body path"
        );

        // The space admin still resolves the evicted body.
        let admin = refusal(
            scenario
                .provider
                .receipts_get(
                    OPERATOR,
                    &receipts_request(&scenario.space_id, receipt_id),
                    now,
                )
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
                        .receipts_get(CLIENT, &receipts_request(&scenario.space_id, receipt), now,)
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
            .receipts_get(
                CLIENT,
                &receipts_request(&scenario.space_id, &context_id),
                now,
            )
            .await
            .expect("the recorded context is covered");
        assert_eq!(
            refusal(
                scenario
                    .provider
                    .receipts_get(
                        CLIENT,
                        &receipts_request(&scenario.space_id, &shared_id),
                        now
                    )
                    .await
            )
            .code,
            SpokeRejectCode::NotAuthorized
        );

        // Shared access over the recorded context covers it.
        shared_access_covers_the_shared_placement(&scenario, now, &shared_id).await;
    }

    async fn shared_access_covers_the_shared_placement(
        scenario: &Scenario,
        now: DateTime<Utc>,
        shared_id: &str,
    ) {
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
            .receipts_get(
                CLIENT,
                &receipts_request(&scenario.space_id, shared_id),
                now,
            )
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
                .receipts_get(
                    CLIENT,
                    &receipts_request(&scenario.space_id, &receipt_id),
                    now,
                )
                .await,
        );
        assert_eq!(scoped_admin.code, SpokeRejectCode::NotAuthorized);

        // A genuine space-wide admin grant still reads it.
        scenario.grant(CLIENT, &["admin"], None, None, now).await;
        let read = scenario
            .provider
            .receipts_get(
                CLIENT,
                &receipts_request(&scenario.space_id, &receipt_id),
                now,
            )
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
                .receipts_get(
                    OPERATOR,
                    &receipts_request(&scenario.space_id, &receipt_id),
                    now,
                )
                .await,
        );
        assert_eq!(admin.code, SpokeRejectCode::ReceiptExpired);
        scenario.revoke(&capture, now).await;
        assert_eq!(
            refusal(
                scenario
                    .provider
                    .receipts_get(
                        CLIENT,
                        &receipts_request(&scenario.space_id, &receipt_id),
                        now
                    )
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

        the_expired_grant_invalidates_the_cursor(&scenario, now, &cursor).await;
    }

    async fn the_expired_grant_invalidates_the_cursor(
        scenario: &Scenario,
        now: DateTime<Utc>,
        cursor: &str,
    ) {
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
                    &list_request(&scenario.space_id, None, Some(2), Some(cursor)),
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
        let (space_wide_receipt, whole_subject_receipt, multi_context_receipt) =
            stage_the_three_target_families(&scenario, now).await;
        a_space_wide_target_needs_a_space_wide_grant(&scenario, now, &space_wide_receipt).await;
        a_whole_subject_target_needs_an_unrestricted_grant(
            &scenario,
            now,
            &whole_subject_receipt,
            &multi_context_receipt,
        )
        .await;
        a_multi_context_target_needs_every_recorded_context(&scenario, now, &multi_context_receipt)
            .await;
        impossible_targets_are_refused_at_construction();
    }

    /// §7.29: one op, three genuinely different requirements — a whole-space
    /// export (space-wide), a whole-subject export (`subject_all`) and a
    /// selected-contexts export (multi-context).
    async fn stage_the_three_target_families(
        scenario: &Scenario,
        now: DateTime<Utc>,
    ) -> (String, String, String) {
        scenario.subject(now).await;
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
        (
            space_wide_receipt,
            whole_subject_receipt,
            multi_context_receipt,
        )
    }

    async fn a_space_wide_target_needs_a_space_wide_grant(
        scenario: &Scenario,
        now: DateTime<Utc>,
        receipt: &str,
    ) {
        // Space-wide: a subject-scoped grant never covers a whole-space target,
        // and a space-wide grant does.
        let subject_scoped = scenario
            .grant(CLIENT, &["transfer"], Some(SUBJECT), None, now)
            .await;
        assert_eq!(
            refusal(
                scenario
                    .provider
                    .receipts_get(CLIENT, &receipts_request(&scenario.space_id, receipt), now,)
                    .await
            )
            .code,
            SpokeRejectCode::NotAuthorized
        );
        scenario.revoke(&subject_scoped, now).await;
        let space_grant = scenario.grant(CLIENT, &["transfer"], None, None, now).await;
        scenario
            .provider
            .receipts_get(CLIENT, &receipts_request(&scenario.space_id, receipt), now)
            .await
            .expect("a space-wide grant covers the whole-space target");
        scenario.revoke(&space_grant, now).await;
    }

    async fn a_whole_subject_target_needs_an_unrestricted_grant(
        scenario: &Scenario,
        now: DateTime<Utc>,
        whole_subject: &str,
        multi_context: &str,
    ) {
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
                        &receipts_request(&scenario.space_id, whole_subject),
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
                &receipts_request(&scenario.space_id, whole_subject),
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
                &receipts_request(&scenario.space_id, multi_context),
                now,
            )
            .await
            .expect("an unrestricted grant covers every recorded context");
        scenario.revoke(&unrestricted, now).await;
    }

    async fn a_multi_context_target_needs_every_recorded_context(
        scenario: &Scenario,
        now: DateTime<Utc>,
        receipt: &str,
    ) {
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
                    .receipts_get(CLIENT, &receipts_request(&scenario.space_id, receipt), now,)
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
            .receipts_get(CLIENT, &receipts_request(&scenario.space_id, receipt), now)
            .await
            .expect("a grant covering every recorded context covers the target");
    }

    fn impossible_targets_are_refused_at_construction() {
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

        let override_candidate = scenario
            .captured_candidate("cap-override", "Small capture.", now)
            .await;
        an_override_over_the_limit_has_no_effect(
            &scenario,
            now,
            &override_candidate,
            &over_limit_text,
        )
        .await;
        an_override_exactly_at_the_limit_commits(
            &scenario,
            now,
            &override_candidate,
            &at_limit_text,
        )
        .await;

        let fragment_candidate = scenario
            .captured_candidate("cap-fragment", "Fragmentable capture.", now)
            .await;
        a_later_fragment_over_the_limit_leaves_no_earlier_record(
            &scenario,
            now,
            &fragment_candidate,
            &over_limit_text,
        )
        .await;
        bounded_fragments_commit_every_record(&scenario, now, &fragment_candidate).await;
        the_limit_counts_multibyte_utf8_bytes(&scenario, now).await;
    }

    async fn an_override_over_the_limit_has_no_effect(
        scenario: &Scenario,
        now: DateTime<Utc>,
        candidate: &str,
        over_limit_text: &str,
    ) {
        // Promote override over the limit: refused with no effect at all.
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
                        candidate,
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
            .candidates_get(CLIENT, &get_request(&scenario.space_id, candidate), now)
            .await
            .expect("candidate readable");
        assert_eq!(pending_candidate(&untouched).status, "pending");
        assert_eq!(pending_candidate(&untouched).revision.get(), 1);
    }

    async fn an_override_exactly_at_the_limit_commits(
        scenario: &Scenario,
        now: DateTime<Utc>,
        candidate: &str,
        at_limit_text: &str,
    ) {
        // Exactly at the limit is admitted.
        let at_limit = scenario
            .provider
            .candidates_review(
                CLIENT,
                &review_request(
                    &scenario.space_id,
                    candidate,
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
    }

    async fn a_later_fragment_over_the_limit_leaves_no_earlier_record(
        scenario: &Scenario,
        now: DateTime<Utc>,
        candidate: &str,
        over_limit_text: &str,
    ) {
        // A later fragment over the limit must leave no earlier fragment record.
        let records_before_fragments = count_rows(&scenario.provider, "records").await;
        let fragment_rejection = refusal(
            scenario
                .provider
                .candidates_review(
                    CLIENT,
                    &review_request(
                        &scenario.space_id,
                        candidate,
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
            .candidates_get(CLIENT, &get_request(&scenario.space_id, candidate), now)
            .await
            .expect("candidate readable");
        assert_eq!(pending_candidate(&still_pending).status, "pending");
    }

    async fn bounded_fragments_commit_every_record(
        scenario: &Scenario,
        now: DateTime<Utc>,
        candidate: &str,
    ) {
        // The same candidate is still reviewable with bounded fragments.
        let fragmented = scenario
            .provider
            .candidates_review(
                CLIENT,
                &review_request(
                    &scenario.space_id,
                    candidate,
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
    }

    async fn the_limit_counts_multibyte_utf8_bytes(scenario: &Scenario, now: DateTime<Utc>) {
        // Multibyte content: the measure counts UTF-8 bytes, so the text is filled
        // with two-byte characters and one ASCII byte to land exactly on the limit
        // (the `{"text":"…"}` framing is eleven bytes) while the character count
        // stays far below it.
        let multibyte_at_limit = format!("{}x", "é".repeat((MAX_CONTENT_BYTES - 12) / 2));
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
            .grant(
                CLIENT,
                &["transfer"],
                None,
                Some((&[CONTEXT_A], false)),
                now,
            )
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
            .grant(
                CLIENT,
                &["transfer"],
                None,
                Some((&[CONTEXT_A], false)),
                now,
            )
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
                .receipts_get(
                    OPERATOR,
                    &receipts_request(&scenario.space_id, &receipt),
                    now,
                )
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

    /// A deterministic dump of every affected row — all columns — so a test can
    /// prove a read, refusal, cancellation or rollback changed no persistent
    /// value, including candidate and record effects.
    async fn retention_rows(provider: &FixtureMemoryProvider) -> Vec<String> {
        let mut rows = Vec::new();
        for sql in [
            "SELECT receipt_id || '|' || space_id || '|' || requester_principal_id || '|' \
             || op || '|' || COALESCE(idempotency_key, '<null>') || '|' || payload_digest \
             || '|' || status || '|' || COALESCE(result_json, '<null>') || '|' \
             || COALESCE(error_json, '<null>') || '|' || created_at || '|' \
             || COALESCE(completed_at, '<null>') || '|' \
             || COALESCE(body_expires_at, '<null>') || '|' || extensions_json AS row \
             FROM receipts ORDER BY receipt_id",
            "SELECT receipt_id || '|' || capabilities_json || '|' \
             || COALESCE(subject_id, '<null>') || '|' || context_ids_json || '|' \
             || reads_shared || '|' || subject_all AS row \
             FROM receipt_targets ORDER BY receipt_id",
            "SELECT space_id || '|' || requester_principal_id || '|' || op || '|' \
             || idempotency_key || '|' || payload_digest || '|' || receipt_id || '|' \
             || last_known_status || '|' || created_at || '|' \
             || COALESCE(tombstone_expires_at, '<null>') AS row \
             FROM idempotency_namespace \
             ORDER BY space_id, requester_principal_id, op, idempotency_key",
            "SELECT candidate_id || '|' || space_id || '|' || subject_id || '|' \
             || capture_visibility || '|' || COALESCE(capture_context_id, '<null>') || '|' \
             || kind || '|' || COALESCE(content_json, '<null>') || '|' || provenance_json \
             || '|' || classification_json || '|' || status || '|' \
             || COALESCE(consumed_by_json, '<null>') || '|' \
             || COALESCE(content_digest, '<null>') || '|' \
             || COALESCE(occurred_at, '<null>') || '|' || COALESCE(expires_at, '<null>') \
             || '|' || revision || '|' || created_at || '|' || extensions_json AS row \
             FROM candidates ORDER BY candidate_id",
            "SELECT record_id || '|' || space_id || '|' || subject_id || '|' || kind || '|' \
             || content_json || '|' || scope_visibility || '|' \
             || COALESCE(scope_context_id, '<null>') || '|' || provenance_json || '|' \
             || COALESCE(import_ref_json, '<null>') || '|' \
             || COALESCE(occurred_at, '<null>') || '|' || COALESCE(tombstone_json, '<null>') \
             || '|' || revision || '|' || created_at || '|' || updated_at || '|' \
             || extensions_json AS row FROM records ORDER BY record_id",
        ] {
            let found: Vec<String> = sqlx::query_scalar(sql)
                .fetch_all(provider.store().pool())
                .await
                .expect("state rows");
            rows.extend(found);
        }
        rows
    }

    async fn read_snapshot_marker(provider: &FixtureMemoryProvider, space_id: &str) -> i64 {
        sqlx::query("SELECT marker FROM provider_snapshot WHERE space_id = ?")
            .bind(space_id)
            .fetch_one(provider.store().pool())
            .await
            .expect("snapshot marker")
            .get("marker")
    }

    /// The stored-timestamp snapshot of one receipt-scoped count.
    async fn count_receipt_rows(
        provider: &FixtureMemoryProvider,
        table: &str,
        receipt_id: &str,
    ) -> i64 {
        let sql = match table {
            "receipts" => "SELECT COUNT(*) AS total FROM receipts WHERE receipt_id = ?",
            "receipt_targets" => {
                "SELECT COUNT(*) AS total FROM receipt_targets WHERE receipt_id = ?"
            }
            "idempotency_namespace" => {
                "SELECT COUNT(*) AS total FROM idempotency_namespace WHERE receipt_id = ?"
            }
            other => panic!("unknown table {other}"),
        };
        sqlx::query(sql)
            .bind(receipt_id)
            .fetch_one(provider.store().pool())
            .await
            .expect("receipt-scoped count")
            .get("total")
    }

    /// Stage one keyless `subjects.ensure` receipt through the shared producer
    /// on a caller-owned transaction; the keyless exemption binds no dedup row.
    async fn stage_keyless_probe(scenario: &Scenario, now: DateTime<Utc>) -> String {
        let subject_id = "subject-keyless-retention";
        let payload = json!({
            "space_id": scenario.space_id,
            "subject_id": subject_id,
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
            .expect("ensure stages its row");
        let admitted = Admitted::from_scope(digest(&payload).expect("payload digest"), ensure);
        let admission = Mutation::keyless("memory.subjects.ensure", &admitted);
        let target = ReceiptTarget::for_subject(
            required_capabilities("memory.subjects.ensure")
                .expect("memory.subjects.ensure has a capability list"),
            subject_id,
        )
        .expect("valid keyless receipt target");
        let receipt = stage_receipt_in_tx(
            &mut transaction,
            &scenario.space_id,
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
        receipt_id_of(&receipt).to_owned()
    }

    /// Stage one pending receipt for the same admitted capture request.
    async fn stage_pending_receipt(
        scenario: &Scenario,
        admitted: &Admitted<MemoryCaptureRequest>,
        receipt_id: &str,
        now: DateTime<Utc>,
    ) {
        let admission = Mutation::keyed(
            OP_CAPTURE,
            admitted.request().idempotency_key.as_str(),
            admitted,
        );
        let target = capture_receipt_target(CONTEXT_A);
        let mut transaction = scenario
            .provider
            .store()
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .expect("writer transaction");
        stage_receipt_in_tx(
            &mut transaction,
            &scenario.space_id,
            CLIENT,
            &admission,
            receipt_id,
            &target,
            ReceiptState::Pending,
            now,
        )
        .await
        .expect("pending receipt stages");
        transaction.commit().await.expect("pending receipt commits");
    }

    /// F1: every receipt-read outcome — authorized/expired, unauthorized,
    /// not-found and keyless-expired — leaves every persistent row untouched at
    /// the retention boundary while returning the correct logical result.
    #[tokio::test]
    async fn reads_are_persistently_read_only_at_every_outcome() {
        let scenario = Scenario::open().await;
        let base = now();
        scenario.subject(base).await;
        scenario
            .grant(CLIENT, &["capture"], Some(SUBJECT), None, base)
            .await;
        let keyed = capture_request(
            &scenario.space_id,
            "Read-only at its retention boundary.",
            context_scope(CONTEXT_A),
            "read-1",
        );
        let committed = scenario
            .provider
            .capture(CLIENT, &keyed, base)
            .await
            .expect("capture commits");
        let receipt_id = receipt_id_of(&committed).to_owned();
        let keyless_id = stage_keyless_probe(&scenario, base).await;

        let after_body = base + Duration::seconds(RECEIPT_RETENTION_SECONDS.cast_signed() + 1);
        let before = retention_rows(&scenario.provider).await;
        let marker_before = read_snapshot_marker(&scenario.provider, &scenario.space_id).await;

        // Authorized read of a logically retired body.
        assert_eq!(
            refusal(
                scenario
                    .provider
                    .receipts_get(
                        CLIENT,
                        &receipts_request(&scenario.space_id, &receipt_id),
                        after_body,
                    )
                    .await
            )
            .code,
            SpokeRejectCode::ReceiptExpired
        );
        assert_eq!(
            retention_rows(&scenario.provider).await,
            before,
            "the authorized expired read wrote nothing"
        );
        // Unauthorized principal: no existence oracle, no write.
        assert_eq!(
            refusal(
                scenario
                    .provider
                    .receipts_get(
                        OTHER,
                        &receipts_request(&scenario.space_id, &receipt_id),
                        after_body,
                    )
                    .await
            )
            .code,
            SpokeRejectCode::NotFound
        );
        assert_eq!(
            retention_rows(&scenario.provider).await,
            before,
            "the unauthorized read wrote nothing"
        );
        // Nonexistent receipt.
        assert_eq!(
            refusal(
                scenario
                    .provider
                    .receipts_get(
                        OPERATOR,
                        &receipts_request(&scenario.space_id, "rcpt_missing"),
                        after_body,
                    )
                    .await
            )
            .code,
            SpokeRejectCode::NotFound
        );
        assert_eq!(
            retention_rows(&scenario.provider).await,
            before,
            "the not-found read wrote nothing"
        );
        the_keyless_body_end_read_writes_nothing(
            &scenario,
            after_body,
            &keyless_id,
            &before,
            marker_before,
        )
        .await;
    }

    async fn the_keyless_body_end_read_writes_nothing(
        scenario: &Scenario,
        after_body: DateTime<Utc>,
        keyless_id: &str,
        before: &[String],
        marker_before: i64,
    ) {
        // A keyless receipt has no tombstone: its body end is its whole life.
        assert_eq!(
            refusal(
                scenario
                    .provider
                    .receipts_get(
                        OPERATOR,
                        &receipts_request(&scenario.space_id, keyless_id),
                        after_body,
                    )
                    .await
            )
            .code,
            SpokeRejectCode::NotFound
        );

        assert_eq!(
            retention_rows(&scenario.provider).await,
            before,
            "the keyless body-end read wrote nothing"
        );
        assert_eq!(
            read_snapshot_marker(&scenario.provider, &scenario.space_id).await,
            marker_before,
            "a read never bumps the snapshot marker"
        );
    }

    /// F2: refusals and a pre-commit abort never commit maintenance for
    /// unrelated expired rows; only the explicit operator pass reclaims them.
    #[tokio::test]
    async fn refusals_and_precommit_abort_commit_no_maintenance() {
        let scenario = Scenario::open().await;
        let base = now();
        scenario.subject(base).await;
        scenario
            .grant(
                CLIENT,
                &["capture", "review", "records_write"],
                Some(SUBJECT),
                None,
                base,
            )
            .await;
        let keyed = capture_request(
            &scenario.space_id,
            "Expired rows must survive every refusal.",
            context_scope(CONTEXT_A),
            "refuse-1",
        );
        let committed = scenario
            .provider
            .capture(CLIENT, &keyed, base)
            .await
            .expect("capture commits");
        let candidate_id = committed_result(&committed)["candidate_ids"][0]
            .as_str()
            .expect("candidate id")
            .to_owned();
        let receipt_id = receipt_id_of(&committed).to_owned();
        stage_keyless_probe(&scenario, base).await;

        let after_body = base + Duration::seconds(RECEIPT_RETENTION_SECONDS.cast_signed() + 1);
        let expired = retention_rows(&scenario.provider).await;
        assert!(
            expired.iter().any(|row| row.contains(&receipt_id)),
            "the expired rows are present before the refusals"
        );
        let marker_before = read_snapshot_marker(&scenario.provider, &scenario.space_id).await;

        every_refusal_path_leaves_expired_rows_untouched(
            &scenario,
            after_body,
            &candidate_id,
            &expired,
            marker_before,
        )
        .await;
        the_operator_sweep_is_the_only_reclamation(&scenario, after_body).await;
    }

    async fn every_refusal_path_leaves_expired_rows_untouched(
        scenario: &Scenario,
        after_body: DateTime<Utc>,
        candidate_id: &str,
        expired: &[String],
        marker_before: i64,
    ) {
        // Unauthorized capture.
        let unauthorized = capture_request(
            &scenario.space_id,
            "Someone else's attempt.",
            context_scope(CONTEXT_A),
            "refuse-2",
        );
        assert_eq!(
            refusal(
                scenario
                    .provider
                    .capture(OTHER, &unauthorized, after_body)
                    .await
            )
            .code,
            SpokeRejectCode::NotAuthorized
        );
        assert_eq!(
            retention_rows(&scenario.provider).await,
            expired,
            "an unauthorized capture commits no maintenance"
        );

        // Idempotency conflict.
        let conflicting = capture_request(
            &scenario.space_id,
            "A different payload under the same key.",
            context_scope(CONTEXT_A),
            "refuse-1",
        );
        assert_eq!(
            refusal(
                scenario
                    .provider
                    .capture(CLIENT, &conflicting, after_body)
                    .await
            )
            .code,
            SpokeRejectCode::IdempotencyConflict
        );
        assert_eq!(retention_rows(&scenario.provider).await, expired);

        // Stale-revision dismissal.
        let dismiss = admit_candidates_dismiss(json!({
            "space_id": scenario.space_id,
            "candidate_id": candidate_id,
            "expected_revision": 9,
            "idempotency_key": "refuse-dismiss",
        }))
        .expect("valid dismissal request");
        assert_eq!(
            refusal(
                scenario
                    .provider
                    .candidates_dismiss(CLIENT, &dismiss, after_body)
                    .await
            )
            .code,
            SpokeRejectCode::RevisionConflict
        );
        assert_eq!(retention_rows(&scenario.provider).await, expired);

        // Invalid review content: refused before any effect, after replay
        // resolution and authorization.
        let review = review_request(
            &scenario.space_id,
            candidate_id,
            1,
            json!({
                "action": "promote",
                "content_override": { "text": "x".repeat(MAX_CONTENT_BYTES) },
            }),
            "refuse-review",
        );
        assert_eq!(
            refusal(
                scenario
                    .provider
                    .candidates_review(CLIENT, &review, after_body)
                    .await
            )
            .code,
            SpokeRejectCode::InvalidInput
        );
        assert_eq!(retention_rows(&scenario.provider).await, expired);

        a_precommit_abort_commits_no_maintenance(scenario, after_body, expired, marker_before)
            .await;
    }

    async fn a_precommit_abort_commits_no_maintenance(
        scenario: &Scenario,
        after_body: DateTime<Utc>,
        expired: &[String],
        marker_before: i64,
    ) {
        // Pre-commit abort: the staged transaction rolls back.
        let aborted = capture_request(
            &scenario.space_id,
            "Aborted before the commit.",
            context_scope(CONTEXT_A),
            "refuse-abort",
        );
        scenario
            .provider
            .faults()
            .fail_next(FaultPoint::BeforeCommit);
        assert_eq!(
            refusal(
                scenario
                    .provider
                    .capture(CLIENT, &aborted, after_body)
                    .await
            )
            .code,
            SpokeRejectCode::OutcomeUncertain
        );
        assert_eq!(retention_rows(&scenario.provider).await, expired);
        assert_eq!(
            read_snapshot_marker(&scenario.provider, &scenario.space_id).await,
            marker_before
        );
    }

    async fn the_operator_sweep_is_the_only_reclamation(
        scenario: &Scenario,
        after_body: DateTime<Utc>,
    ) {
        // The explicit operator pass is the only reclamation path.
        let sweep = scenario
            .provider
            .sweep_retained(&scenario.space_id, after_body)
            .await
            .expect("maintenance pass");
        assert_eq!(sweep.receipt_bodies_evicted, 1);
        assert_eq!(sweep.keyless_bodies_evicted, 1);
        assert_eq!(sweep.targets_removed, 1);
        assert_eq!(sweep.dedup_tombstones_evicted, 0);
        assert_eq!(count_rows(&scenario.provider, "receipts").await, 0);
        assert_eq!(
            count_rows(&scenario.provider, "receipt_targets").await,
            1,
            "the keyed target stays with its tombstone"
        );
        assert_eq!(
            count_rows(&scenario.provider, "idempotency_namespace").await,
            1
        );
    }

    /// §6.10/MEM-R021 + C-028: the body window starts at terminal completion and
    /// the tombstone keeps its full declared window after the body retires. The
    /// logical state is correct on unswept rows; only the explicit operator pass
    /// removes them.
    #[tokio::test]
    async fn c_028_receipt_and_dedup_retention_windows() {
        let scenario = Scenario::open().await;
        let base = now();
        scenario.subject(base).await;
        scenario
            .grant(CLIENT, &["capture"], Some(SUBJECT), None, base)
            .await;
        let secret = "The retained body names the vault code 4417.";
        let keyed = capture_request(
            &scenario.space_id,
            secret,
            context_scope(CONTEXT_A),
            "retention-1",
        );
        let committed = scenario
            .provider
            .capture(CLIENT, &keyed, base)
            .await
            .expect("capture commits");
        let receipt_id = receipt_id_of(&committed).to_owned();
        let keyless_id = stage_keyless_probe(&scenario, base).await;
        let marker_before = read_snapshot_marker(&scenario.provider, &scenario.space_id).await;

        let body_end = base + Duration::seconds(RECEIPT_RETENTION_SECONDS.cast_signed());

        // Before the body end, and at the inclusive boundary, the body is served
        // and nothing is reclaimed.
        for instant in [body_end - Duration::seconds(1), body_end] {
            assert_eq!(
                receipt_status(
                    &scenario
                        .provider
                        .receipts_get(
                            CLIENT,
                            &receipts_request(&scenario.space_id, &receipt_id),
                            instant,
                        )
                        .await
                        .expect("the body is served inside its window")
                ),
                "committed"
            );
            assert_eq!(
                scenario
                    .provider
                    .sweep_retained(&scenario.space_id, instant)
                    .await
                    .expect("sweep"),
                RetentionSweep::default()
            );
        }

        // One second later the unswept rows already answer logically: the key is
        // protected, the effect never re-runs, the keyless receipt is absent.
        let after_body = body_end + Duration::seconds(1);
        after_the_body_end_the_rows_still_answer_logically(
            &scenario,
            after_body,
            &receipt_id,
            &keyless_id,
            &keyed,
            marker_before,
        )
        .await;
        the_explicit_sweep_reclaims_the_keyless_body_only(
            &scenario,
            after_body,
            &receipt_id,
            secret,
            marker_before,
        )
        .await;
        the_tombstone_boundary_releases_the_key(scenario, keyed, &receipt_id, body_end, after_body)
            .await;
    }

    async fn after_the_body_end_the_rows_still_answer_logically(
        scenario: &Scenario,
        after_body: DateTime<Utc>,
        receipt_id: &str,
        keyless_id: &str,
        keyed: &Admitted<MemoryCaptureRequest>,
        marker_before: i64,
    ) {
        let replay = refusal(scenario.provider.capture(CLIENT, keyed, after_body).await);
        assert_eq!(replay.code, SpokeRejectCode::ReceiptExpired);
        assert_eq!(
            replay
                .details
                .as_ref()
                .and_then(|details| details.get("last_known_status")),
            Some(&json!("committed"))
        );
        assert_eq!(
            count_rows(&scenario.provider, "candidates").await,
            1,
            "an evicted receipt is never re-executed"
        );
        let conflicting = capture_request(
            &scenario.space_id,
            "A different body under the same key.",
            context_scope(CONTEXT_A),
            "retention-1",
        );
        assert_eq!(
            refusal(
                scenario
                    .provider
                    .capture(CLIENT, &conflicting, after_body)
                    .await
            )
            .code,
            SpokeRejectCode::IdempotencyConflict
        );
        assert_eq!(
            refusal(
                scenario
                    .provider
                    .receipts_get(
                        CLIENT,
                        &receipts_request(&scenario.space_id, receipt_id),
                        after_body,
                    )
                    .await
            )
            .code,
            SpokeRejectCode::ReceiptExpired
        );
        assert_eq!(
            refusal(
                scenario
                    .provider
                    .receipts_get(
                        OPERATOR,
                        &receipts_request(&scenario.space_id, keyless_id),
                        after_body,
                    )
                    .await
            )
            .code,
            SpokeRejectCode::NotFound
        );
        assert_eq!(
            count_rows(&scenario.provider, "receipts").await,
            2,
            "logical reads reclaim nothing"
        );
        assert_eq!(
            read_snapshot_marker(&scenario.provider, &scenario.space_id).await,
            marker_before
        );
    }

    async fn the_explicit_sweep_reclaims_the_keyless_body_only(
        scenario: &Scenario,
        after_body: DateTime<Utc>,
        receipt_id: &str,
        secret: &str,
        marker_before: i64,
    ) {
        // Explicit maintenance: the keyed body leaves alone, the keyless body
        // and target leave together.
        let sweep = scenario
            .provider
            .sweep_retained(&scenario.space_id, after_body)
            .await
            .expect("sweep");
        assert_eq!(sweep.receipt_bodies_evicted, 1);
        assert_eq!(sweep.keyless_bodies_evicted, 1);
        assert_eq!(sweep.targets_removed, 1);
        assert_eq!(sweep.dedup_tombstones_evicted, 0);
        assert_eq!(count_rows(&scenario.provider, "receipts").await, 0);
        assert_eq!(
            count_rows(&scenario.provider, "receipt_targets").await,
            1,
            "the keyed target survives body eviction"
        );
        assert_eq!(
            count_rows(&scenario.provider, "idempotency_namespace").await,
            1
        );
        assert_eq!(
            read_snapshot_marker(&scenario.provider, &scenario.space_id).await,
            marker_before,
            "maintenance never bumps the snapshot marker"
        );

        // The surviving tombstone and target stay content-free.
        let tombstone_text: String = sqlx::query_scalar(
            "SELECT idempotency_key || payload_digest || last_known_status \
             FROM idempotency_namespace WHERE receipt_id = ?",
        )
        .bind(receipt_id)
        .fetch_one(scenario.provider.store().pool())
        .await
        .expect("surviving tombstone");
        assert_no_secret(&tombstone_text, secret);
        let target_text: String = sqlx::query_scalar(
            "SELECT capabilities_json || COALESCE(subject_id, '') || context_ids_json \
             FROM receipt_targets WHERE receipt_id = ?",
        )
        .bind(receipt_id)
        .fetch_one(scenario.provider.store().pool())
        .await
        .expect("surviving target");
        assert_no_secret(&target_text, secret);
    }

    async fn the_tombstone_boundary_releases_the_key(
        scenario: Scenario,
        keyed: Admitted<MemoryCaptureRequest>,
        receipt_id: &str,
        body_end: DateTime<Utc>,
        after_body: DateTime<Utc>,
    ) {
        // A reopen keeps the tombstone answering.
        let scenario = scenario.reopen().await;
        assert_eq!(
            refusal(
                scenario
                    .provider
                    .receipts_get(
                        CLIENT,
                        &receipts_request(&scenario.space_id, receipt_id),
                        after_body,
                    )
                    .await
            )
            .code,
            SpokeRejectCode::ReceiptExpired
        );

        // Tombstone boundary: protected at the exact end, free one second later.
        let tombstone_end = body_end + Duration::seconds(DEDUP_RETENTION_SECONDS.cast_signed());
        assert_eq!(
            refusal(
                scenario
                    .provider
                    .receipts_get(
                        CLIENT,
                        &receipts_request(&scenario.space_id, receipt_id),
                        tombstone_end,
                    )
                    .await
            )
            .code,
            SpokeRejectCode::ReceiptExpired
        );
        assert_eq!(
            scenario
                .provider
                .sweep_retained(&scenario.space_id, tombstone_end)
                .await
                .expect("sweep"),
            RetentionSweep::default()
        );

        let after_all = tombstone_end + Duration::seconds(1);
        let fresh = scenario
            .provider
            .capture(CLIENT, &keyed, after_all)
            .await
            .expect("the finished key runs one fresh effect");
        assert_ne!(receipt_id_of(&fresh), receipt_id);
        assert_eq!(count_rows(&scenario.provider, "candidates").await, 2);
        assert_eq!(
            count_rows(&scenario.provider, "idempotency_namespace").await,
            1,
            "the stale binding was released with the fresh effect"
        );
        assert_eq!(count_rows(&scenario.provider, "receipt_targets").await, 1);
        assert_eq!(
            refusal(
                scenario
                    .provider
                    .receipts_get(
                        OPERATOR,
                        &receipts_request(&scenario.space_id, receipt_id),
                        after_all,
                    )
                    .await
            )
            .code,
            SpokeRejectCode::NotFound
        );
        assert_eq!(
            scenario
                .provider
                .sweep_retained(&scenario.space_id, after_all)
                .await
                .expect("sweep"),
            RetentionSweep::default(),
            "nothing of the retired lifetime is left to reclaim"
        );
    }

    /// §7.28/C-104 + §6.10: current authorization gates the retired body, and
    /// the refusal survives a reopen because nothing was physically reclaimed.
    #[tokio::test]
    async fn c_028_evicted_receipt_refuses_a_revoked_requester() {
        let scenario = Scenario::open().await;
        let base = now();
        scenario.subject(base).await;
        let grant = scenario
            .grant(CLIENT, &["capture"], Some(SUBJECT), None, base)
            .await;
        let request = capture_request(
            &scenario.space_id,
            "Evicted after the grant ends.",
            context_scope(CONTEXT_A),
            "revoked-1",
        );
        let committed = scenario
            .provider
            .capture(CLIENT, &request, base)
            .await
            .expect("capture commits");
        let receipt_id = receipt_id_of(&committed).to_owned();

        let after_body = base + Duration::seconds(RECEIPT_RETENTION_SECONDS.cast_signed() + 1);
        let rows = retention_rows(&scenario.provider).await;

        // The admin and the requester both see the logical tombstone.
        for principal in [OPERATOR, CLIENT] {
            assert_eq!(
                refusal(
                    scenario
                        .provider
                        .receipts_get(
                            principal,
                            &receipts_request(&scenario.space_id, &receipt_id),
                            after_body,
                        )
                        .await
                )
                .code,
                SpokeRejectCode::ReceiptExpired
            );
        }

        // Current authorization is evaluated before disclosure.
        scenario.revoke(&grant, after_body).await;
        assert_eq!(
            refusal(
                scenario
                    .provider
                    .receipts_get(
                        CLIENT,
                        &receipts_request(&scenario.space_id, &receipt_id),
                        after_body,
                    )
                    .await
            )
            .code,
            SpokeRejectCode::NotAuthorized
        );
        assert_eq!(
            refusal(
                scenario
                    .provider
                    .capture(CLIENT, &request, after_body)
                    .await
            )
            .code,
            SpokeRejectCode::NotAuthorized,
            "a revoked requester cannot replay either"
        );
        assert_eq!(
            retention_rows(&scenario.provider).await,
            rows,
            "the reads and the revoked replay wrote nothing"
        );

        // The refusal is durable because the tombstone and target were never
        // physically reclaimed.
        let scenario = scenario.reopen().await;
        assert_eq!(
            refusal(
                scenario
                    .provider
                    .receipts_get(
                        CLIENT,
                        &receipts_request(&scenario.space_id, &receipt_id),
                        after_body,
                    )
                    .await
            )
            .code,
            SpokeRejectCode::NotAuthorized
        );
        assert_eq!(
            count_receipt_rows(&scenario.provider, "receipt_targets", &receipt_id).await,
            1
        );
        assert_eq!(
            count_receipt_rows(&scenario.provider, "idempotency_namespace", &receipt_id).await,
            1
        );
    }

    /// §6.10/MEM-R021: late settlement gets a full terminal body window and then
    /// the full post-body tombstone window; the old admission-instant cut is
    /// never used, and the key frees only after both.
    #[tokio::test]
    async fn c_028_retention_keeps_live_pending_work_and_expires_it_after_settlement() {
        let scenario = Scenario::open().await;
        let base = now();
        scenario.subject(base).await;
        scenario
            .grant(CLIENT, &["capture"], Some(SUBJECT), None, base)
            .await;
        let request = capture_request(
            &scenario.space_id,
            "Pending work that outlives both old cutoffs.",
            context_scope(CONTEXT_A),
            "pending-retention-1",
        );
        stage_pending_receipt(&scenario, &request, "rcpt_pending_retention", base).await;

        // Past both old cutoffs the pending receipt is still live, readable and
        // the replay answer; maintenance leaves it alone.
        let stale = base
            + Duration::seconds(
                (RECEIPT_RETENTION_SECONDS + DEDUP_RETENTION_SECONDS).cast_signed() + 1,
            );
        assert_eq!(
            scenario
                .provider
                .sweep_retained(&scenario.space_id, stale)
                .await
                .expect("sweep"),
            RetentionSweep::default()
        );
        assert_eq!(
            receipt_status(
                &scenario
                    .provider
                    .receipts_get(
                        CLIENT,
                        &receipts_request(&scenario.space_id, "rcpt_pending_retention"),
                        stale,
                    )
                    .await
                    .expect("the pending receipt is still served")
            ),
            "pending"
        );
        let replayed = scenario
            .provider
            .capture(CLIENT, &request, stale)
            .await
            .expect("live pending work replays");
        assert_eq!(receipt_id_of(&replayed), "rcpt_pending_retention");
        assert_eq!(count_rows(&scenario.provider, "candidates").await, 0);

        let settle_body_end = the_settled_receipt_keeps_its_terminal_window(&scenario, stale).await;
        the_tombstone_window_then_releases_the_key(&scenario, &request, settle_body_end).await;
    }

    async fn the_settled_receipt_keeps_its_terminal_window(
        scenario: &Scenario,
        stale: DateTime<Utc>,
    ) -> DateTime<Utc> {
        // Settlement starts both terminal windows at that instant.
        let settle_at = stale + Duration::seconds(1);
        let mut transaction = scenario
            .provider
            .store()
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .expect("writer transaction");
        settle_receipt_in_tx(
            &mut transaction,
            &scenario.space_id,
            "rcpt_pending_retention",
            ReceiptState::Committed(json!({ "candidate_ids": [], "classified": 0 })),
            settle_at,
        )
        .await
        .expect("pending receipt settles");
        transaction.commit().await.expect("settlement commits");

        let settle_body_end =
            settle_at + Duration::seconds(RECEIPT_RETENTION_SECONDS.cast_signed());
        assert_eq!(
            receipt_status(
                &scenario
                    .provider
                    .receipts_get(
                        CLIENT,
                        &receipts_request(&scenario.space_id, "rcpt_pending_retention"),
                        settle_body_end,
                    )
                    .await
                    .expect("the full terminal body window is served")
            ),
            "committed"
        );
        assert_eq!(
            scenario
                .provider
                .sweep_retained(&scenario.space_id, settle_body_end)
                .await
                .expect("sweep"),
            RetentionSweep::default()
        );

        let body_gone = settle_body_end + Duration::seconds(1);
        assert_eq!(
            refusal(
                scenario
                    .provider
                    .receipts_get(
                        CLIENT,
                        &receipts_request(&scenario.space_id, "rcpt_pending_retention"),
                        body_gone,
                    )
                    .await
            )
            .code,
            SpokeRejectCode::ReceiptExpired
        );
        let sweep = scenario
            .provider
            .sweep_retained(&scenario.space_id, body_gone)
            .await
            .expect("sweep");
        assert_eq!(sweep.receipt_bodies_evicted, 1);
        assert_eq!(sweep.dedup_tombstones_evicted, 0);
        assert_eq!(count_rows(&scenario.provider, "receipt_targets").await, 1);
        settle_body_end
    }

    async fn the_tombstone_window_then_releases_the_key(
        scenario: &Scenario,
        request: &Admitted<MemoryCaptureRequest>,
        settle_body_end: DateTime<Utc>,
    ) {
        // The full post-body tombstone window still protects the key.
        let tombstone_end =
            settle_body_end + Duration::seconds(DEDUP_RETENTION_SECONDS.cast_signed());
        assert_eq!(
            refusal(
                scenario
                    .provider
                    .receipts_get(
                        CLIENT,
                        &receipts_request(&scenario.space_id, "rcpt_pending_retention"),
                        tombstone_end,
                    )
                    .await
            )
            .code,
            SpokeRejectCode::ReceiptExpired
        );
        assert_eq!(
            scenario
                .provider
                .sweep_retained(&scenario.space_id, tombstone_end)
                .await
                .expect("sweep"),
            RetentionSweep::default()
        );

        let after_all = tombstone_end + Duration::seconds(1);
        let sweep = scenario
            .provider
            .sweep_retained(&scenario.space_id, after_all)
            .await
            .expect("sweep");
        assert_eq!(sweep.dedup_tombstones_evicted, 1);
        assert_eq!(sweep.targets_removed, 1);
        assert_eq!(
            count_rows(&scenario.provider, "idempotency_namespace").await,
            0
        );
        let fresh = scenario
            .provider
            .capture(CLIENT, request, after_all)
            .await
            .expect("the finished key is new again");
        assert_ne!(receipt_id_of(&fresh), "rcpt_pending_retention");
        assert_eq!(count_rows(&scenario.provider, "candidates").await, 1);
    }

    /// F7: the caller-owned seams enforce the same logical boundaries at the
    /// supplied instant with no hidden maintenance commit, and a finished key is
    /// released only inside the transaction that stages the new effect.
    #[tokio::test]
    async fn caller_owned_seams_apply_logical_expiration_without_hidden_writes() {
        let scenario = Scenario::open().await;
        let base = now();
        scenario.subject(base).await;
        scenario
            .grant(CLIENT, &["capture"], Some(SUBJECT), None, base)
            .await;
        let request = capture_request(
            &scenario.space_id,
            "Caller-owned seam retention.",
            context_scope(CONTEXT_A),
            "caller-1",
        );
        let committed = scenario
            .provider
            .capture(CLIENT, &request, base)
            .await
            .expect("capture commits");
        let receipt_id = receipt_id_of(&committed).to_owned();
        let rows = retention_rows(&scenario.provider).await;

        // A caller-owned read transaction answers the logical boundary and
        // writes nothing.
        let after_body = base + Duration::seconds(RECEIPT_RETENTION_SECONDS.cast_signed() + 1);
        let mut transaction = scenario
            .provider
            .store()
            .pool()
            .begin()
            .await
            .expect("read transaction");
        assert_eq!(
            refusal(
                scenario
                    .provider
                    .receipts_get_in_tx(
                        &mut transaction,
                        CLIENT,
                        &receipts_request(&scenario.space_id, &receipt_id),
                        after_body,
                    )
                    .await
            )
            .code,
            SpokeRejectCode::ReceiptExpired
        );
        transaction.rollback().await.expect("read rollback");
        assert_eq!(retention_rows(&scenario.provider).await, rows);

        // A caller-owned mutation transaction answers the retired body without
        // writing it, then rolls back cleanly.
        let mut transaction = scenario
            .provider
            .store()
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .expect("writer transaction");
        assert_eq!(
            refusal(
                scenario
                    .provider
                    .capture_in_tx(&mut transaction, CLIENT, &request, after_body)
                    .await
            )
            .code,
            SpokeRejectCode::ReceiptExpired
        );
        transaction.rollback().await.expect("rollback");
        assert_eq!(retention_rows(&scenario.provider).await, rows);

        the_released_binding_is_transactional(&scenario, &request, &receipt_id, &rows, base).await;
    }

    async fn the_released_binding_is_transactional(
        scenario: &Scenario,
        request: &Admitted<MemoryCaptureRequest>,
        receipt_id: &str,
        rows: &[String],
        base: DateTime<Utc>,
    ) {
        // Past the tombstone end the same seam releases the finished binding and
        // stages one fresh effect; a rollback restores the old binding, a commit
        // keeps exactly the new effect.
        let after_all = base
            + Duration::seconds(
                (RECEIPT_RETENTION_SECONDS + DEDUP_RETENTION_SECONDS).cast_signed() + 1,
            );
        let mut transaction = scenario
            .provider
            .store()
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .expect("writer transaction");
        let fresh = scenario
            .provider
            .capture_in_tx(&mut transaction, CLIENT, request, after_all)
            .await
            .expect("the finished key runs a fresh effect");
        assert_ne!(receipt_id_of(&fresh), receipt_id);
        transaction.rollback().await.expect("rollback");
        assert_eq!(
            retention_rows(&scenario.provider).await,
            rows,
            "the released stale binding is transactional"
        );
        assert_eq!(count_rows(&scenario.provider, "candidates").await, 1);

        let mut transaction = scenario
            .provider
            .store()
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .expect("writer transaction");
        let fresh = scenario
            .provider
            .capture_in_tx(&mut transaction, CLIENT, request, after_all)
            .await
            .expect("the finished key runs one committed effect");
        transaction.commit().await.expect("commit");
        assert_ne!(receipt_id_of(&fresh), receipt_id);
        assert_eq!(count_rows(&scenario.provider, "candidates").await, 2);
        assert_eq!(
            count_rows(&scenario.provider, "idempotency_namespace").await,
            1
        );
        assert_eq!(count_rows(&scenario.provider, "receipt_targets").await, 1);
    }

    /// Seed one scenario with two keyed captures at the fixed scenario instant:
    /// the returned requests are the same payload for the first key and a
    /// changed payload for the second.
    async fn seed_two_expiring_captures() -> (
        Scenario,
        Admitted<MemoryCaptureRequest>,
        Admitted<MemoryCaptureRequest>,
    ) {
        let scenario = Scenario::open().await;
        let base = now();
        scenario.subject(base).await;
        scenario
            .grant(CLIENT, &["capture"], Some(SUBJECT), None, base)
            .await;
        let same = capture_request(
            &scenario.space_id,
            "Retained payload.",
            context_scope(CONTEXT_A),
            "expired-same",
        );
        scenario
            .provider
            .capture(CLIENT, &same, base)
            .await
            .expect("first capture commits");
        let original = capture_request(
            &scenario.space_id,
            "Original changed-key payload.",
            context_scope(CONTEXT_A),
            "expired-changed",
        );
        scenario
            .provider
            .capture(CLIENT, &original, base)
            .await
            .expect("second capture commits");
        let changed = capture_request(
            &scenario.space_id,
            "Different payload after the final expiry.",
            context_scope(CONTEXT_A),
            "expired-changed",
        );
        (scenario, same, changed)
    }

    /// F4/I187: after the full tombstone lifetime an UNSWEPT namespace treats
    /// both the same and a changed payload as a new operation, exactly like an
    /// already-swept one. While the tombstone lives, a changed payload is still
    /// a conflict. A caller rollback restores the released stale binding.
    /// Regression for the expired-binding precedence: the whole lifetime is
    /// classified before any retained-target authority, so a fully expired key
    /// is reusable (the caller's authority for the new execution decides)
    /// exactly as after an operator sweep — while a live binding still requires
    /// the retained target.
    #[tokio::test]
    async fn expired_binding_is_reusable_before_retained_target_authority() {
        let base = now();
        let after =
            base + Duration::seconds(
                (RECEIPT_RETENTION_SECONDS + DEDUP_RETENTION_SECONDS).cast_signed(),
            ) + Duration::seconds(1);

        let (scenario, same, _changed) = seed_two_expiring_captures().await;
        // Simulate a retained target that no longer admits the caller: the
        // content-free metadata is gone, the binding is not.
        let mut conn = scenario
            .provider
            .store()
            .pool()
            .acquire()
            .await
            .expect("fixture connection");
        sqlx::query("DELETE FROM receipt_targets")
            .execute(&mut *conn)
            .await
            .expect("drop retained targets");
        drop(conn);

        // Live binding: still a receipt to disclose, so the missing retained
        // target is a refusal, never a silent replay.
        assert_eq!(
            refusal(scenario.provider.capture(CLIENT, &same, base).await).code,
            SpokeRejectCode::NotAuthorized,
            "a live binding still requires its retained target"
        );

        // Fully expired binding: the key is free, so the same authorized caller
        // executes it as a new operation instead of being refused for a target
        // that no longer exists.
        let fresh = scenario
            .provider
            .capture(CLIENT, &same, after)
            .await
            .expect("an expired binding is reusable regardless of the old target");
        assert_ne!(receipt_id_of(&fresh), "");
        assert_eq!(count_rows(&scenario.provider, "receipt_targets").await, 1);
    }

    #[tokio::test]
    async fn expired_tombstone_accepts_both_payloads_and_matches_the_swept_state() {
        let base = now();
        let boundary = base
            + Duration::seconds(
                (RECEIPT_RETENTION_SECONDS + DEDUP_RETENTION_SECONDS).cast_signed(),
            );
        let after = boundary + Duration::seconds(1);

        // Unswept namespace.
        let (scenario, same, changed) = seed_two_expiring_captures().await;
        assert_eq!(
            refusal(scenario.provider.capture(CLIENT, &same, boundary).await).code,
            SpokeRejectCode::ReceiptExpired,
            "the same payload is a terminal replay at the live boundary"
        );
        assert_eq!(
            refusal(scenario.provider.capture(CLIENT, &changed, boundary).await).code,
            SpokeRejectCode::IdempotencyConflict,
            "a changed payload is still a conflict while the tombstone lives"
        );
        let same_fresh = scenario
            .provider
            .capture(CLIENT, &same, after)
            .await
            .expect("the same payload runs as a new operation after the final expiry");
        let changed_fresh = scenario
            .provider
            .capture(CLIENT, &changed, after)
            .await
            .expect("a changed payload runs as a new operation after the final expiry");
        assert_ne!(receipt_id_of(&same_fresh), receipt_id_of(&changed_fresh));
        assert_eq!(count_rows(&scenario.provider, "candidates").await, 4);
        assert_eq!(
            count_rows(&scenario.provider, "idempotency_namespace").await,
            2,
            "the retired bindings were released with the fresh effects"
        );
        assert_eq!(count_rows(&scenario.provider, "receipt_targets").await, 2);
        let unswept_counts = (
            count_rows(&scenario.provider, "candidates").await,
            count_rows(&scenario.provider, "receipts").await,
            count_rows(&scenario.provider, "receipt_targets").await,
            count_rows(&scenario.provider, "idempotency_namespace").await,
        );

        // The same requests after an explicit maintenance pass must behave the
        // same way: expiry never depends on the physical sweep.
        let (swept, swept_same, swept_changed) = seed_two_expiring_captures().await;
        let sweep = swept
            .provider
            .sweep_retained(&swept.space_id, after)
            .await
            .expect("maintenance pass");
        assert_eq!(sweep.dedup_tombstones_evicted, 2);
        assert_eq!(sweep.targets_removed, 2);
        assert_eq!(sweep.receipt_bodies_evicted, 2);
        let swept_same_fresh = swept
            .provider
            .capture(CLIENT, &swept_same, after)
            .await
            .expect("the swept state accepts the same payload");
        let swept_changed_fresh = swept
            .provider
            .capture(CLIENT, &swept_changed, after)
            .await
            .expect("the swept state accepts a changed payload");
        assert_ne!(
            receipt_id_of(&swept_same_fresh),
            receipt_id_of(&swept_changed_fresh)
        );
        assert_eq!(
            (
                count_rows(&swept.provider, "candidates").await,
                count_rows(&swept.provider, "receipts").await,
                count_rows(&swept.provider, "receipt_targets").await,
                count_rows(&swept.provider, "idempotency_namespace").await,
            ),
            unswept_counts,
            "the unswept and swept states produce the same observable outcome"
        );

        // A caller-owned transaction that releases the stale binding and then
        // rolls back restores every affected row.
        let (rollback, _rollback_same, rollback_changed) = seed_two_expiring_captures().await;
        let rows = retention_rows(&rollback.provider).await;
        let mut transaction = rollback
            .provider
            .store()
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .expect("writer transaction");
        let fresh = rollback
            .provider
            .capture_in_tx(&mut transaction, CLIENT, &rollback_changed, after)
            .await
            .expect("the finished key runs fresh inside the caller transaction");
        assert_ne!(receipt_id_of(&fresh), "rcpt_unbound_probe");
        transaction.rollback().await.expect("rollback");
        assert_eq!(
            retention_rows(&rollback.provider).await,
            rows,
            "rollback restores the stale binding and body"
        );
        assert_eq!(count_rows(&rollback.provider, "candidates").await, 2);
    }

    /// F2/I182 + F4/I184: after the full tombstone expiry, a real operation
    /// first releases the stale binding and then hits a later fence refusal; the
    /// release must roll back automatically, preserving the complete old rows
    /// and the candidate effect.
    #[tokio::test]
    async fn released_binding_rolls_back_when_a_later_admission_refuses() {
        let (scenario, same, _changed) = seed_two_expiring_captures().await;
        let base = now();
        let after = base
            + Duration::seconds(
                (RECEIPT_RETENTION_SECONDS + DEDUP_RETENTION_SECONDS).cast_signed() + 1,
            );
        // A genuine later refusal: the subject is fenced before the write target
        // is checked, after replay resolution already released the stale binding.
        scenario.fence_subject("fence", 1, after).await;
        let rows = retention_rows(&scenario.provider).await;
        let refused = refusal(scenario.provider.capture(CLIENT, &same, after).await);
        assert_eq!(refused.code, SpokeRejectCode::SubjectFenced);
        assert_eq!(
            retention_rows(&scenario.provider).await,
            rows,
            "the release rolled back with the refusal"
        );
        assert_eq!(count_rows(&scenario.provider, "candidates").await, 2);
        assert_eq!(count_rows(&scenario.provider, "receipts").await, 2);
        assert_eq!(count_rows(&scenario.provider, "receipt_targets").await, 2);
        assert_eq!(
            count_rows(&scenario.provider, "idempotency_namespace").await,
            2
        );
    }
}
