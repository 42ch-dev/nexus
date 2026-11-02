//! `LocalMemoryProvider` — the in-process `spoke_operations::MemoryPort` over
//! the workspace's existing local storage (v1.227 NXM-1).
//!
//! The provider is the single `MemoryPort` boundary inside `nexus-core`; it owns
//! admission (freeze allowlist, grant coverage, revision CAS, idempotent replay)
//! and delegates the protocol-free parts to:
//!
//! * [`storage`] — the provider-private transaction layer inside the core writer
//!   guard (never a second storage protocol),
//! * [`policy`] — the Nexus-owned declared surface (limits, policy version,
//!   required profile kinds),
//! * `spoke_operations` — the protocol's own validators, digest helpers and grant
//!   evaluation, so no protocol rule is re-implemented here.
//!
//! **Error shape.** Refusals travel the port's own `SpokeResult::Reject` channel,
//! never the success channel: the memory error codes are ops-layer reject codes
//! (§14.1, `spoke_operations::SpokeRejectCode`), and an unserved op is
//! `CAPABILITY_PORT_MISSING` — the code upstream's own responder answers an
//! unimplemented port op with, and serializes as-is
//! (`responder.rs:367-371`, `responder.rs:1506-1522`). The lowercase
//! `op_unsupported` string exists only on the *inbound* side, where
//! `map_error_envelope` folds a peer's dispatch-deny into
//! `CAPABILITY_PORT_MISSING` + `details.wire_code` (`remote_adapter.rs:296-299`),
//! so minting it here would name a code no reject can serialize.
//! Every reject carries `details.retry_class` (§14.2);
//! `OUTCOME_UNCERTAIN` is the only `uncertain` class, and `INTERNAL_ERROR` is
//! `retryable` only because it is raised strictly before the effect commits.
//!
//! **Served surface.** `memory.describe`, `memory.spaces.*`,
//! `memory.subjects.*`, `memory.grants.*`, `memory.capture`,
//! `memory.candidates.*`, `memory.records.*`, `memory.recall`,
//! `memory.profile.*` and `memory.receipts.get` (§7.28) are served. The
//! sub-capability gated families (reflection, transfer) are rejected as not
//! served.

mod journal;
/// The single provider-seam legacy projection/read bridge (NXM-5 Task 1).
pub mod legacy;
pub mod policy;
pub mod profile;
mod storage;

use std::num::NonZeroU64;
use std::path::PathBuf;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Map, Value};
use spoke_operations::spoke_schemas as schemas;
use spoke_operations::{
    jcs_sha256_hex, validate_memory_candidates_list, validate_memory_candidates_review,
    validate_memory_capture, validate_memory_grants_grant, validate_memory_grants_list,
    validate_memory_recall, validate_memory_records_create, validate_memory_records_list,
    validate_memory_records_update, validate_memory_spaces_list, validate_memory_subjects_fence,
    GrantRequirement, IssuedGrant, MemoryPort, SpokeRejectCode, SpokeResult,
};

use crate::error::{CoreError, CoreResult};
use storage::{
    Admission, CandidateRow, Coverage, Fence, FenceAction, FenceTarget, GrantRow, MemoryStore,
    NewCandidate, NewGrant, OpContext, Placement, ProfileRow, ReceiptRow, RecordRead, RecordRow,
    ReviewDecision, Selection, SpaceRow, StoreError, SubjectRow, RECEIPT_FAILED, RECEIPT_PENDING,
};

/// The single space this provider provisions (§7.0.6). Clients discover it via
/// `memory.describe`/`memory.spaces.list`; it is never a payload default.
pub const DEFAULT_SPACE_ID: &str = "spc_local_default";

/// `authority_host_id` of the single authoritative provider (MEM-R001). It is an
/// opaque host label, not a transport address.
pub const LOCAL_AUTHORITY_HOST_ID: &str = "nexus-local";

const DEFAULT_SPACE_DISPLAY_NAME: &str = "Local memory";

const OP_FREEZE_SPACE: &str = "memory.spaces.freeze";
const OP_UNFREEZE_SPACE: &str = "memory.spaces.unfreeze";
const OP_ENSURE_SUBJECT: &str = "memory.subjects.ensure";
const OP_FENCE_SUBJECT: &str = "memory.subjects.fence";
const OP_GRANT: &str = "memory.grants.grant";
const OP_REVOKE_GRANT: &str = "memory.grants.revoke";
const OP_CAPTURE: &str = "memory.capture";
const OP_CANDIDATES_LIST: &str = "memory.candidates.list";
const OP_CANDIDATES_DISMISS: &str = "memory.candidates.dismiss";
const OP_CANDIDATES_REVIEW: &str = "memory.candidates.review";
const OP_RECORDS_CREATE: &str = "memory.records.create";
const OP_RECORDS_UPDATE: &str = "memory.records.update";
const OP_RECORDS_DELETE: &str = "memory.records.delete";
const OP_RECORDS_LIST: &str = "memory.records.list";
const OP_RECORDS_PROMOTE: &str = "memory.records.promote";
const OP_RECALL: &str = "memory.recall";
const OP_PROFILE_UPDATE: &str = "memory.profile.update";
/// NXM-5 Task 2's internal op: adopting a legacy-derived object into provider
/// storage. It is never a wire op — it is the receipt identity of the
/// just-in-time adoption a dismissal performs **inside the transaction that
/// consumes the row** (NXM-5 Task 2 fix round 3, finding F1), so a retried
/// dismissal replays instead of duplicating and no adopted row is left pending.
const OP_LEGACY_ADOPT: &str = "memory.legacy.adopt";

/// §7.3/§7.4: the two space lifecycle transitions, each naming its op string and
/// the state it writes.
#[derive(Clone, Copy)]
enum SpaceTransition {
    Freeze,
    Unfreeze,
}

impl SpaceTransition {
    const fn op(self) -> &'static str {
        match self {
            Self::Freeze => OP_FREEZE_SPACE,
            Self::Unfreeze => OP_UNFREEZE_SPACE,
        }
    }

    const fn status(self) -> &'static str {
        match self {
            Self::Freeze => "frozen",
            Self::Unfreeze => "active",
        }
    }
}

/// Why an op was refused, in protocol vocabulary (§14.1).
///
/// Every variant maps onto a wire `code` plus its §14.2 retry class, so the
/// mapping lives in one place and cannot drift between ops.
#[derive(Debug)]
enum ProviderError {
    NotFound(String),
    NotAuthorized(String),
    SpaceFrozen(String),
    SubjectFenced {
        subject_id: String,
        status: String,
        context_id: Option<String>,
    },
    RevisionConflict {
        current: i64,
    },
    IdempotencyConflict,
    /// §6.10/MEM-R021: the receipt body is past `receipt_retention_seconds`; the
    /// op is never re-executed and only the tombstone's last known status is
    /// disclosed (§7.28).
    ReceiptExpired {
        last_known_status: String,
    },
    CandidateConsumed(String),
    /// §4.2/MEM-R005: an admission-relevant consumer is missing a required
    /// authored content kind — fail closed, never an empty success.
    ProfileIncomplete {
        subject_id: String,
        missing: Vec<String>,
    },
    InvalidInput {
        field: String,
        reason: String,
    },
    StatusTransition(String),
    CursorInvalid,
    /// The effect may have committed (§14.2/§14.3): never reported as a clean
    /// no-effect failure.
    OutcomeUncertain(String),
    /// §5.2/D-10: the subject's journaled file intent is quarantined, so its
    /// objects are withheld until an operator resolves it. Terminal: retrying
    /// cannot help.
    Unreconciled {
        subject_id: String,
    },
    /// §5.2 step 3/MEM-R010: this receipt records a quarantine — the intent
    /// committed but its file effect could not be published. Terminal on the
    /// replay channel; the recorded envelope is served by
    /// `memory.receipts.get`, and recovery re-drives the intent once the target
    /// is resolved.
    ReceiptFailed {
        receipt_id: String,
        message: String,
    },
    /// §5.2 step 2: an earlier intent for this operation's file target is still
    /// unsettled. Raised before the operation commits, so retrying after the
    /// journal recovery settles that predecessor is the supported path.
    UnsettledPredecessor {
        receipt_id: String,
        state: String,
    },
    /// The provider was opened read-only: a mutation is refused before
    /// admission, retention, the publication lock or any connection write
    /// (fix round 2, finding F3).
    ReadOnly(String),
    Internal(String),
}

impl ProviderError {
    const fn reject_code(&self) -> SpokeRejectCode {
        match self {
            Self::NotFound(_) => SpokeRejectCode::NotFound,
            Self::NotAuthorized(_) | Self::ReadOnly(_) => SpokeRejectCode::NotAuthorized,
            Self::SpaceFrozen(_) => SpokeRejectCode::SpaceFrozen,
            Self::SubjectFenced { .. } => SpokeRejectCode::SubjectFenced,
            Self::RevisionConflict { .. } => SpokeRejectCode::RevisionConflict,
            Self::IdempotencyConflict => SpokeRejectCode::IdempotencyConflict,
            Self::ReceiptExpired { .. } => SpokeRejectCode::ReceiptExpired,
            Self::CandidateConsumed(_) => SpokeRejectCode::CandidateConsumed,
            Self::ProfileIncomplete { .. } => SpokeRejectCode::ProfileIncomplete,
            Self::InvalidInput { .. } => SpokeRejectCode::InvalidInput,
            Self::StatusTransition(_) => SpokeRejectCode::InvalidStatusTransition,
            Self::CursorInvalid => SpokeRejectCode::CursorInvalid,
            Self::OutcomeUncertain(_) => SpokeRejectCode::OutcomeUncertain,
            Self::Unreconciled { .. }
            | Self::ReceiptFailed { .. }
            | Self::UnsettledPredecessor { .. }
            | Self::Internal(_) => SpokeRejectCode::InternalError,
        }
    }

    /// §14.2 retry class. `INTERNAL_ERROR` is retryable only because this
    /// provider raises it strictly before the effect commits — which is exactly
    /// the shape of an unsettled predecessor; an uncertain outcome is its own
    /// class, and a quarantined intent stops being retryable once its receipt
    /// recorded the failure (the journal re-drives it without the caller).
    const fn retry_class(&self) -> &'static str {
        match self {
            Self::Internal(_) | Self::UnsettledPredecessor { .. } => "retryable",
            Self::OutcomeUncertain(_) => "uncertain",
            _ => "terminal",
        }
    }

    fn message(&self) -> String {
        match self {
            Self::NotFound(target) => format!("{target} not found"),
            Self::ProfileIncomplete {
                subject_id,
                missing,
            } => format!(
                "subject {subject_id} has not authored the required profile kinds ({}); author them with memory.profile.update",
                missing.join(", ")
            ),
            Self::NotAuthorized(reason)
            | Self::ReadOnly(reason)
            | Self::SpaceFrozen(reason)
            | Self::StatusTransition(reason)
            | Self::CandidateConsumed(reason)
            | Self::OutcomeUncertain(reason)
            | Self::ReceiptFailed {
                message: reason, ..
            }
            | Self::Internal(reason) => reason.clone(),
            Self::UnsettledPredecessor { receipt_id, state } => format!(
                "an earlier memory journal intent for this operation's file target is unsettled \
                 (receipt {receipt_id}, state {state}); the journal recovery settles it, so repeat \
                 this operation once that intent is resolved"
            ),
            Self::Unreconciled { subject_id } => format!(
                "subject {subject_id} has an unresolved memory journal intent (quarantined); its objects are withheld from reads until that journal entry is settled"
            ),
            Self::SubjectFenced {
                subject_id,
                status,
                context_id,
            } => context_id.as_deref().map_or_else(
                || format!("subject {subject_id} is {status}; its writes are rejected"),
                |context_id| format!(
                    "subject {subject_id} has context {context_id} fenced; writes placing content there are rejected"
                ),
            ),
            Self::RevisionConflict { current } => format!(
                "expected_revision does not match the stored revision ({current}); re-read and retry"
            ),
            Self::IdempotencyConflict => {
                "this idempotency key was already used with a different payload".to_string()
            }
            Self::ReceiptExpired { last_known_status } => format!(
                "this receipt's body was evicted after its retention window and is no longer served; the recorded outcome was {last_known_status}, and the operation is not re-executed — the idempotency_key stays bound to that outcome until its dedup retention window ends, so repeating that key cannot reach a new effect; use a new idempotency_key to perform a new operation"
            ),
            Self::InvalidInput { field, reason } => format!("{field}: {reason}"),
            Self::CursorInvalid => {
                "the pagination cursor is no longer valid (state or grant change); restart the listing"
                    .to_string()
            }
        }
    }

    fn details(&self) -> Map<String, Value> {
        let mut details = Map::new();
        details.insert(
            "retry_class".to_string(),
            Value::String(self.retry_class().to_string()),
        );
        match self {
            Self::InvalidInput { field, .. } => {
                details.insert("field".to_string(), json!(field));
            }
            Self::RevisionConflict { current } => {
                details.insert("current_revision".to_string(), json!(current));
            }
            Self::ReceiptExpired { last_known_status } => {
                details.insert("last_known_status".to_string(), json!(last_known_status));
            }
            Self::ProfileIncomplete { missing, .. } => {
                details.insert("missing".to_string(), json!(missing));
            }
            Self::SubjectFenced {
                subject_id,
                status,
                context_id,
            } => {
                details.insert("subject_id".to_string(), json!(subject_id));
                details.insert("status".to_string(), json!(status));
                details.insert(
                    "scope".to_string(),
                    json!(if context_id.is_some() {
                        "context"
                    } else {
                        "subject"
                    }),
                );
                if let Some(context_id) = context_id {
                    details.insert("context_id".to_string(), json!(context_id));
                }
            }
            Self::Unreconciled { subject_id } => {
                details.insert("subject_id".to_string(), json!(subject_id));
            }
            Self::ReceiptFailed { receipt_id, .. } => {
                details.insert("receipt_id".to_string(), json!(receipt_id));
            }
            Self::UnsettledPredecessor { receipt_id, state } => {
                details.insert("receipt_id".to_string(), json!(receipt_id));
                details.insert("state".to_string(), json!(state));
            }
            _ => {}
        }
        details
    }
}

fn store_error(error: StoreError) -> ProviderError {
    match error {
        StoreError::NotFound(target) => ProviderError::NotFound(target),
        StoreError::NotAuthorized(reason) => ProviderError::NotAuthorized(reason),
        StoreError::SpaceFrozen(reason) => ProviderError::SpaceFrozen(reason),
        StoreError::SubjectFenced {
            subject_id,
            status,
            context_id,
        } => ProviderError::SubjectFenced {
            subject_id,
            status,
            context_id,
        },
        StoreError::RevisionConflict { current } => ProviderError::RevisionConflict { current },
        StoreError::IdempotencyConflict => ProviderError::IdempotencyConflict,
        StoreError::ReceiptExpired { last_known_status } => {
            ProviderError::ReceiptExpired { last_known_status }
        }
        StoreError::CandidateConsumed(reason) => ProviderError::CandidateConsumed(reason),
        StoreError::StatusTransition(reason) => ProviderError::StatusTransition(reason),
        StoreError::CommitUnknown(reason) => ProviderError::OutcomeUncertain(reason),
        StoreError::Unreconciled { subject_id } => ProviderError::Unreconciled { subject_id },
        StoreError::UnsettledPredecessor { receipt_id, state } => {
            ProviderError::UnsettledPredecessor { receipt_id, state }
        }
        StoreError::Corrupt(reason) => ProviderError::Internal(reason),
        StoreError::ReadOnly(reason) => ProviderError::ReadOnly(reason),
        StoreError::Database(error) => ProviderError::Internal(error.to_string()),
    }
}

/// A refusal is a reject, never a success value (§14.1).
fn deny<T>(error: &ProviderError) -> SpokeResult<T> {
    spoke_operations::spoke_reject(error.reject_code(), error.message(), Some(error.details()))
}

/// The sub-capability gated families (reflection, transfer): the
/// dispatch-level denial shape. A reject can only carry a
/// [`SpokeRejectCode`], and the port
/// vocabulary has no `op_unsupported` variant — upstream's responder answers an
/// unimplemented port op with `CapabilityPortMissing` (`responder.rs:367-371`)
/// and serializes `reject.code.as_str()` verbatim (`responder.rs:1506-1522`), so
/// `CAPABILITY_PORT_MISSING` is the code the wire carries. The lowercase
/// `op_unsupported` marker belongs to the receiving side of a dispatch deny
/// (`remote_adapter.rs:296-299`); a provider that minted it would describe a
/// code no reject can serialize.
fn unsupported<T>(op: &str) -> SpokeResult<T> {
    let mut details = Map::new();
    details.insert(
        "retry_class".to_string(),
        Value::String("terminal".to_string()),
    );
    details.insert("op".to_string(), json!(op));
    spoke_operations::spoke_reject(
        SpokeRejectCode::CapabilityPortMissing,
        format!("{op} is not served by this provider build"),
        Some(details),
    )
}

fn respond<T: DeserializeOwned>(body: Value) -> SpokeResult<T> {
    match serde_json::from_value(body) {
        Ok(response) => spoke_operations::spoke_ok(response),
        Err(error) => deny(&ProviderError::Internal(format!(
            "memory response projection: {error}"
        ))),
    }
}

/// Project a receipt that is already committed. A failure here is *after* the
/// effect, so it is `OUTCOME_UNCERTAIN`, never a retryable `INTERNAL_ERROR`
/// (§14.2/§14.3).
fn settle_receipt<T: DeserializeOwned>(receipt: &ReceiptRow) -> SpokeResult<T> {
    if let Some(error) = pending_receipt(receipt) {
        return deny(&error);
    }
    if let Some(error) = failed_receipt(receipt) {
        return deny(&error);
    }
    match receipt_body(receipt) {
        Ok(body) => match serde_json::from_value(body) {
            Ok(response) => spoke_operations::spoke_ok(response),
            Err(error) => deny(&ProviderError::OutcomeUncertain(format!(
                "memory response projection after commit: {error}"
            ))),
        },
        Err(error) => deny(&ProviderError::OutcomeUncertain(error.message())),
    }
}

/// §5.2 step 3: a receipt the journal settled as `failed` records a committed
/// intent whose file effect could not be published. Repeating the key cannot
/// settle it, so until the intent is re-driven both the replay channel and
/// `memory.receipts.get` answer with the recorded failure. That answer is not
/// permanent: once its target is resolved, the journal re-drives the intent
/// without the caller and a successful re-drive replaces the `failed` receipt
/// with a `committed` one carrying the published outcome, so the same key and
/// the same receipt id then settle normally.
fn failed_receipt(receipt: &ReceiptRow) -> Option<ProviderError> {
    (receipt.status == RECEIPT_FAILED).then(|| {
        let recorded = receipt_failure(receipt).ok().and_then(|envelope| {
            envelope
                .get("message")
                .and_then(Value::as_str)
                .map(str::to_string)
        });
        ProviderError::ReceiptFailed {
            receipt_id: receipt.receipt_id.clone(),
            message: recorded.unwrap_or_else(|| {
                format!(
                    "receipt {} is failed: its committed file effect is quarantined",
                    receipt.receipt_id
                )
            }),
        }
    })
}

/// The failure envelope a `failed` receipt recorded at quarantine time.
fn receipt_failure(receipt: &ReceiptRow) -> Result<Value, ProviderError> {
    let raw = receipt.error_json.as_deref().ok_or_else(|| {
        ProviderError::Internal(format!(
            "failed receipt {} carries no failure envelope",
            receipt.receipt_id
        ))
    })?;
    serde_json::from_str(raw)
        .map_err(|e| ProviderError::Internal(format!("receipt failure envelope: {e}")))
}

/// §5.2 step 2: a journaled op's receipt is durable at the point of no return
/// but terminal only once its file effect is consistent. Until then both the
/// replay and `memory.receipts.get` answer the pending state as `uncertain`, so
/// no caller is told an unsettled outcome is final.
fn pending_receipt(receipt: &ReceiptRow) -> Option<ProviderError> {
    (receipt.status == RECEIPT_PENDING).then(|| {
        ProviderError::OutcomeUncertain(format!(
            "receipt {} is committed but its journaled file effect is not published yet, so the \
             outcome is not settled: repeat the same idempotency_key or read \
             memory.receipts.get once the journal has been recovered",
            receipt.receipt_id
        ))
    })
}

/// §6.1 `payload_digest`, bound to the operation as the plan's memory identity
/// requires: SHA-256 over the JCS canonicalization of `{"op", "payload"}`.
fn payload_digest<T: Serialize>(op: &str, request: &T) -> Result<String, ProviderError> {
    let payload = serde_json::to_value(request)
        .map_err(|e| ProviderError::Internal(format!("payload encode: {e}")))?;
    match jcs_sha256_hex(&json!({ "op": op, "payload": payload })) {
        SpokeResult::Ok(digest) => Ok(digest),
        SpokeResult::Reject(reject) => Err(ProviderError::InvalidInput {
            field: "payload".to_string(),
            reason: reject.message,
        }),
    }
}

/// §7.1 `memory.describe`'s response, shared with nothing: it is the provider's
/// declaration of its own surface.
fn describe_body() -> Value {
    json!({
        "memory_protocol_version": policy::MEMORY_PROTOCOL_VERSION,
        "capabilities": policy::CAPABILITIES,
        "limits": policy::limits(),
        "required_profile_kinds": policy::REQUIRED_PROFILE_KINDS,
        "engine_policy_version": policy::ENGINE_POLICY_VERSION,
    })
}

/// §6.2 space projection.
fn space_body(space: &SpaceRow) -> Value {
    json!({
        "schema_version": 1,
        "space_id": space.space_id,
        "display_name": space.display_name,
        "authority_host_id": space.authority_host_id,
        "authority_generation": space.authority_generation,
        "status": space.status,
        "pending_receipts": space.pending_receipts,
        "created_at": space.created_at,
        "revision": space.revision,
        "extensions": {},
    })
}

/// §6.3 subject projection.
fn subject_body(subject: &SubjectRow) -> Result<Value, ProviderError> {
    let kind = match (&subject.kind_ns, &subject.kind_id) {
        (Some(ns), Some(id)) => json!({ "ns": ns, "id": id }),
        _ => Value::Null,
    };
    Ok(json!({
        "schema_version": 1,
        "space_id": subject.space_id,
        "subject_id": subject.subject_id,
        "kind": kind,
        "status": subject.status,
        "fenced_contexts": subject.fenced_contexts().map_err(store_error)?,
        "created_at": subject.created_at,
        "revision": subject.revision,
        "extensions": {},
    }))
}

/// §6.5 grant projection. `context_ids` is present only on the restricted shape,
/// which is what the generated `MemoryGrant` oneOf distinguishes.
fn grant_body(grant: &GrantRow) -> Result<Value, ProviderError> {
    let capabilities = grant.capabilities().map_err(store_error)?;
    let context_ids = grant.context_ids().map_err(store_error)?;
    let mut body = json!({
        "schema_version": 1,
        "grant_id": grant.grant_id,
        "space_id": grant.space_id,
        "subject_id": grant.subject_id,
        "principal_id": grant.principal_id,
        "capabilities": capabilities,
        "created_at": grant.created_at,
        "expires_at": grant.expires_at,
        "revoked_at": grant.revoked_at,
        "revision": grant.revision,
        "extensions": {},
    });
    if !context_ids.is_empty() {
        let map = body.as_object_mut().expect("grant projection is an object");
        map.insert("context_ids".to_string(), json!(context_ids));
        map.insert("shared_access".to_string(), json!(grant.shared_access != 0));
    }
    Ok(body)
}

fn grant_wire(grant: &GrantRow) -> Result<schemas::MemoryGrant, ProviderError> {
    serde_json::from_value(grant_body(grant)?)
        .map_err(|e| ProviderError::Internal(format!("grant projection: {e}")))
}

/// §6.10 receipt projection, as every mutating op returns it. A `failed` receipt
/// carries the recorded failure envelope and no result — the wire schema's own
/// failed branch (§5.2 step 3), which is the channel that reports a quarantined
/// intent terminally.
fn receipt_body(receipt: &ReceiptRow) -> Result<Value, ProviderError> {
    let mut body = json!({
        "schema_version": 1,
        "receipt_id": receipt.receipt_id,
        "op": receipt.op,
        "space_id": receipt.space_id,
        "requester_principal_id": receipt.requester_principal_id,
        "idempotency_key": receipt.idempotency_key,
        "payload_digest": receipt.payload_digest,
        "status": receipt.status,
        "created_at": receipt.created_at,
        "completed_at": receipt.completed_at,
        "extensions": {},
    });
    let object = body
        .as_object_mut()
        .expect("a receipt projection is always a JSON object");
    if receipt.status == RECEIPT_FAILED {
        object.insert("error".to_string(), receipt_failure(receipt)?);
    } else {
        let result = match receipt.result_json.as_deref() {
            Some(raw) => serde_json::from_str::<Value>(raw)
                .map_err(|e| ProviderError::Internal(format!("receipt result: {e}")))?,
            None => Value::Null,
        };
        object.insert("result".to_string(), result);
    }
    Ok(body)
}

/// Opaque page cursor bound to principal + space + subject/filter + policy
/// version + snapshot marker (§7.0.7, MEM-R026). It is plain JSON: cursors are
/// opaque strings to clients, and JSON keeps the binding inspectable in tests.
#[derive(serde::Serialize, serde::Deserialize)]
struct PageCursor {
    principal: String,
    scope: String,
    offset: u64,
    marker: String,
}

/// The bound query a cursor belongs to: op, space, the subject and principal
/// filters actually applied, and the engine policy version. Any other request
/// under the same cursor is `CURSOR_INVALID` (MEM-R026).
fn cursor_scope(
    op: &str,
    space_id: Option<&str>,
    subject_id: Option<&str>,
    principal_filter: Option<&str>,
) -> String {
    format!(
        "{op}|space={}|subject={}|principal={}|policy={}",
        space_id.unwrap_or("*"),
        subject_id.unwrap_or("*"),
        principal_filter.unwrap_or("*"),
        policy::ENGINE_POLICY_VERSION,
    )
}

fn encode_cursor(principal: &str, scope: &str, offset: u64, marker: &str) -> String {
    let cursor = PageCursor {
        principal: principal.to_string(),
        scope: scope.to_string(),
        offset,
        marker: marker.to_string(),
    };
    serde_json::to_string(&cursor).expect("a page cursor always serializes")
}

/// The wire `cursor` newtypes are transparent over `String`; the decoder binds
/// the text they carry.
fn cursor_text<C: std::ops::Deref<Target = String>>(cursor: Option<&C>) -> Option<&str> {
    cursor.map(|value| value.as_str())
}

fn decode_cursor(
    raw: Option<&str>,
    principal: &str,
    scope: &str,
    marker: &str,
) -> Result<u64, ProviderError> {
    let Some(raw) = raw else {
        return Ok(0);
    };
    let cursor: PageCursor = serde_json::from_str(raw).map_err(|_| ProviderError::CursorInvalid)?;
    if cursor.principal != principal || cursor.scope != scope || cursor.marker != marker {
        return Err(ProviderError::CursorInvalid);
    }
    Ok(cursor.offset)
}

/// The offset a cursor claims, decoded **before** the page is cut, for reads
/// whose scope is only known after the read resolves it (the grants page names
/// the principal filter that read decides). The claim is verified afterwards by
/// [`decode_cursor`] — an offset from a foreign or stale cursor only ever
/// selects rows the caller could already read, and the mismatched response is
/// refused.
fn claimed_offset(raw: Option<&str>) -> Result<u64, ProviderError> {
    let Some(raw) = raw else {
        return Ok(0);
    };
    Ok(serde_json::from_str::<PageCursor>(raw)
        .map_err(|_| ProviderError::CursorInvalid)?
        .offset)
}

/// The cursor's offset and the snapshot marker it was issued against, decoded
/// **before** the store reads the page: the offset is what selects the SQL page,
/// while the marker is verified afterwards against that same snapshot, so a
/// stale cursor still answers `CURSOR_INVALID` (§7.0.7/MEM-R026).
fn cursor_page(
    raw: Option<&str>,
    principal: &str,
    scope: &str,
) -> Result<(u64, Option<String>), ProviderError> {
    let Some(raw) = raw else {
        return Ok((0, None));
    };
    let cursor: PageCursor = serde_json::from_str(raw).map_err(|_| ProviderError::CursorInvalid)?;
    if cursor.principal != principal || cursor.scope != scope {
        return Err(ProviderError::CursorInvalid);
    }
    Ok((cursor.offset, Some(cursor.marker)))
}

/// Refuse a page read against a different snapshot than the cursor was minted
/// on — the visible set or its grants changed between pages.
fn verify_cursor(claimed: Option<&str>, marker: &str) -> Result<(), ProviderError> {
    if claimed.is_some_and(|claimed| claimed != marker) {
        return Err(ProviderError::CursorInvalid);
    }
    Ok(())
}

fn page<T>(items: Vec<T>, limit: u64, offset: u64) -> (Vec<T>, Option<u64>) {
    let total = items.len();
    let start = usize::try_from(offset).unwrap_or(total).min(total);
    let take = usize::try_from(limit).unwrap_or(total);
    let page: Vec<T> = items.into_iter().skip(start).take(take).collect();
    let consumed = start + page.len();
    let next = (consumed < total).then_some(consumed as u64);
    (page, next)
}

fn page_limit(limit: Option<NonZeroU64>) -> u64 {
    limit.unwrap_or(policy::MAX_PAGE_SIZE_BOUND).get()
}

pub struct LocalMemoryProvider {
    store: MemoryStore,
    space_id: String,
}

/// Exclusive per-bearer memory-disposition section
/// ([`LocalMemoryProvider::review_disposition`]).
///
/// Held for the critical section and released on drop. It is not a writer
/// admission and grants nothing on its own.
pub struct MemoryDisposition {
    _file: std::fs::File,
}

/// The lock file name for one bearer: a SHA-256 digest of the exact subject id
/// under `<state db dir>/memory_locks/` (NXM-5 Task 2 fix round 4, finding
/// R4-F2).
///
/// The digest is the identity: it distinguishes ids that differ only by ASCII
/// case (unlike a readable encoding on a case-insensitive volume) and it is
/// fixed-length, so any subject id maps to a name well inside the filesystem
/// component limit. Confinement comes from the fixed prefix plus the digest's
/// hex alphabet, so no id can name a path component, `..`, or another subject's
/// lock.
fn disposition_lock_file(subject_id: &str) -> Result<String, ProviderError> {
    let digest = match jcs_sha256_hex(&Value::String(subject_id.to_owned())) {
        SpokeResult::Ok(digest) => digest,
        SpokeResult::Reject(reject) => return Err(ProviderError::Internal(reject.message)),
    };
    Ok(format!("subject-{digest}.lock"))
}

impl LocalMemoryProvider {
    /// Open (or join) the workspace's admitted writer and provision the local
    /// space plus its first `admin` grant (§7.0.6). Re-opening is a no-op.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError`] when the workspace directory cannot be created, the
    /// writer admission or migration fails, or the provisioning write is refused
    /// by the writer guard.
    pub async fn open(
        user_home: PathBuf,
        creator_id: &str,
        workspace_slug: &str,
    ) -> CoreResult<Self> {
        let db_path =
            nexus_home_layout::workspace_state_db_path(&user_home, creator_id, workspace_slug);
        if let Some(parent) = db_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|error| CoreError::Internal {
                    category: format!("memory provider workspace dir: {error}"),
                })?;
        }
        let store = MemoryStore::open(&db_path, creator_id, &user_home)
            .await
            .map_err(store_error)
            .map_err(to_core_error)?;
        store
            .provision(
                DEFAULT_SPACE_ID,
                LOCAL_AUTHORITY_HOST_ID,
                creator_id,
                &bootstrap_grant_id(creator_id),
                Some(DEFAULT_SPACE_DISPLAY_NAME),
                Utc::now(),
            )
            .await
            .map_err(store_error)
            .map_err(to_core_error)?;
        Ok(Self {
            store,
            space_id: DEFAULT_SPACE_ID.to_string(),
        })
    }

    /// The space this provider serves.
    #[must_use]
    pub fn space_id(&self) -> &str {
        &self.space_id
    }

    /// Open the provider over a **read-only** connection (NXM-5 Task 2 fix round
    /// 1, finding F6).
    ///
    /// A read-only core access must keep serving memory reads, so this open
    /// serves the same `MemoryPort` reads over `mode=ro` storage. It takes no
    /// writer admission and performs no provisioning, journal recovery or
    /// retention sweep: an open-pinned provider on that access mode must not
    /// write. A write op reached through this handle is refused by the writer
    /// protocol (the read-only connection rejects it), and `CoreService` refuses
    /// the E4 write handlers before they get here.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError`] when the workspace database cannot be read.
    pub async fn open_read_only(
        user_home: PathBuf,
        creator_id: &str,
        workspace_slug: &str,
    ) -> CoreResult<Self> {
        let db_path =
            nexus_home_layout::workspace_state_db_path(&user_home, creator_id, workspace_slug);
        let store = MemoryStore::open_read_only(&db_path, creator_id, &user_home)
            .await
            .map_err(store_error)
            .map_err(to_core_error)?;
        Ok(Self {
            store,
            space_id: DEFAULT_SPACE_ID.to_string(),
        })
    }

    /// Whether this provider may write (`false` for a read-only open).
    #[must_use]
    pub const fn is_writable(&self) -> bool {
        self.store.writable()
    }

    /// The bearer's memory-disposition section (NXM-5 Task 2 fix round 2,
    /// finding F1).
    ///
    /// A provider candidate consumption and the legacy review drain's
    /// consumed-check plus classification are one decision in two halves, so
    /// both take this exclusive OS lock for their whole critical section: a
    /// dismissal can never commit between the check and the classify write.
    /// One lock file per bearer under `<state db dir>/memory_locks/`, opened per
    /// acquisition — descriptors in one process contend exactly like two
    /// processes (the same primitive the Character activity fences use), so a
    /// daemon drain and a CLI dismissal over one workspace serialize too.
    ///
    /// # Errors
    /// [`CoreError::Forbidden`] under read-only access (a section that exists to
    /// decide a write is not served) and [`CoreError::Internal`] when the lock
    /// file cannot be opened or locked.
    pub async fn review_disposition(&self, subject_id: &str) -> CoreResult<MemoryDisposition> {
        self.disposition(subject_id)
            .await
            .map_err(|error| match error {
                ProviderError::ReadOnly(reason) => CoreError::Forbidden { resource: reason },
                other => to_core_error(other),
            })
    }

    /// [`Self::review_disposition`] as the provider's own error taxonomy.
    async fn disposition(&self, subject_id: &str) -> Result<MemoryDisposition, ProviderError> {
        if !self.is_writable() {
            return Err(ProviderError::ReadOnly(
                "memory: read-only core access".to_string(),
            ));
        }
        let dir = self.store.disposition_dir();
        let name = disposition_lock_file(subject_id)?;
        let opened = tokio::task::spawn_blocking(move || -> std::io::Result<std::fs::File> {
            std::fs::create_dir_all(&dir)?;
            std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(dir.join(name))
        })
        .await;
        let file = match opened {
            Ok(Ok(file)) => file,
            Ok(Err(error)) => {
                return Err(ProviderError::Internal(format!(
                    "memory disposition lock: {error}"
                )))
            }
            Err(error) => {
                return Err(ProviderError::Internal(format!(
                    "memory disposition lock task: {error}"
                )))
            }
        };
        // The attempt itself (fix round 4, finding R4-F3): the non-blocking
        // probe decides whether this acquisition is contended, so a test can
        // observe a real lock wait instead of a pre-lock rendezvous. The probe
        // also takes the section when free, so the blocking `lock()` below runs
        // only when another descriptor really holds it.
        #[cfg(feature = "test-hooks")]
        let wait_report = self.store.take_disposition_wait_report();
        let locked = tokio::task::spawn_blocking(move || -> std::io::Result<std::fs::File> {
            match file.try_lock() {
                Ok(()) => {}
                Err(std::fs::TryLockError::WouldBlock) => {
                    #[cfg(feature = "test-hooks")]
                    if let Some(report) = wait_report.as_ref() {
                        report.notify_one();
                    }
                    file.lock()?;
                }
                Err(std::fs::TryLockError::Error(error)) => return Err(error),
            }
            Ok(file)
        })
        .await;
        match locked {
            Ok(Ok(file)) => Ok(MemoryDisposition { _file: file }),
            Ok(Err(error)) => Err(ProviderError::Internal(format!(
                "memory disposition lock: {error}"
            ))),
            Err(error) => Err(ProviderError::Internal(format!(
                "memory disposition lock task: {error}"
            ))),
        }
    }

    /// §14.2 proof seam: whether the bearer's disposition section is free right
    /// now. Compiled out of a production build.
    #[cfg(feature = "test-hooks")]
    #[doc(hidden)]
    pub fn disposition_available(&self, subject_id: &str) -> bool {
        let Ok(name) = disposition_lock_file(subject_id) else {
            return false;
        };
        let path = self.store.disposition_dir().join(name);
        let Ok(file) = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
        else {
            return false;
        };
        let free = file.try_lock().is_ok();
        if free {
            let _ = file.unlock();
        }
        free
    }

    /// NXM-5 Task 2 fix round 4 (finding R4-F3) proof seam: notify `waiter`
    /// from inside the lock attempt of the next disposition acquisition whose
    /// non-blocking probe finds the section held, i.e. only after the caller has
    /// really reached the contended lock. Compiled out of a production build.
    #[cfg(feature = "test-hooks")]
    #[doc(hidden)]
    pub fn report_disposition_waits(&self, waiter: std::sync::Arc<tokio::sync::Notify>) {
        self.store.report_disposition_waits(waiter);
    }

    /// NXM-5 Task 2 fix round 3 (finding F5) proof seam: entries that passed the
    /// read-only gate and reached the admission or publication machinery.
    /// Compiled out of a production build.
    #[cfg(feature = "test-hooks")]
    #[doc(hidden)]
    #[must_use]
    pub fn lock_attempts(&self) -> usize {
        self.store.lock_attempts()
    }

    /// MEM-R005 profile admission gate: the authored content kinds a subject
    /// has, or the typed `PROFILE_INCOMPLETE` refusal.
    ///
    /// An absent *optional* kind is honest-empty (§7.23) and simply not listed.
    /// A missing §4.2 required kind is the fail-closed state an
    /// admission-relevant consumer acts on, so it leaves on the reject channel
    /// with `details.missing`, never as a success value that could be read as an
    /// empty success. Capability absence is decided at provider selection
    /// (`CAPABILITY_PORT_MISSING`) and never reaches this gate.
    ///
    /// # Errors
    ///
    /// `PROFILE_INCOMPLETE` when a required authored kind is absent.
    /// `NOT_AUTHORIZED` when the principal holds no profile-visible grant on the
    /// subject (§7.23).
    pub async fn profile_admission(
        &self,
        principal: &schemas::Principal,
        space_id: &str,
        subject_id: &str,
    ) -> SpokeResult<profile::ProfileAdmission> {
        let rows = match self
            .store
            .profiles(space_id, subject_id, principal_id(principal))
            .await
        {
            Ok(rows) => rows,
            Err(error) => return deny(&store_error(error)),
        };
        let Some(rows) = rows else {
            return deny(&ProviderError::NotAuthorized(format!(
                "principal {} holds no profile-visible grant on subject {subject_id}",
                principal_id(principal)
            )));
        };
        match profile::admission(&rows, &policy::REQUIRED_PROFILE_KINDS) {
            Ok(admission) => spoke_operations::spoke_ok(admission),
            Err(missing) => deny(&ProviderError::ProfileIncomplete {
                subject_id: subject_id.to_string(),
                missing,
            }),
        }
    }

    /// Dismiss a legacy-derived candidate: adopt the projection under the
    /// bridge's deterministic id and consume it **in one guarded transaction**
    /// (NXM-5 Task 2 fix round 1, finding F4).
    ///
    /// The adoption is not a separate step a caller can interleave with: a
    /// failure of either half leaves neither effect, so an adopted pending row
    /// can never be left behind by a dismissal that did not commit. There is no
    /// standalone adoption entry point at all (fix round 3, finding F1): every
    /// provider adoption happens inside the mutation that consumes the row, and
    /// every candidate-consuming provider op holds the bearer's disposition
    /// section. The departure from the wire request shape is deliberate — the E4
    /// dismissal has no caller-supplied revision (`expected_revision: None`
    /// consumes the revision the transaction reads) and it names the bearer's
    /// subject, which only this seam knows.
    ///
    /// # Refusals
    /// The same reject codes `memory.candidates.dismiss` answers: `NOT_FOUND`
    /// when nothing resolves to the id, `CANDIDATE_CONSUMED`/
    /// `REVISION_CONFLICT` when the CAS cannot land, and the admission codes.
    /// A stored candidate owned by another subject is `INVALID_REQUEST`: the
    /// dismissal names the bearer whose section it takes, and only the row's
    /// owner is that bearer (fix round 4, finding R4-F1).
    pub async fn dismiss_legacy_candidate(
        &self,
        principal: &schemas::Principal,
        subject_id: &str,
        candidate_id: &str,
        expected_revision: Option<i64>,
        idempotency_key: &str,
    ) -> SpokeResult<schemas::MemoryCandidatesDismissResponse> {
        // The section must belong to the row's owner, not to whatever subject the
        // caller named (fix round 4, finding R4-F1): resolving the stored owner
        // before locking means a caller can never consume A while holding B's
        // section, interleaving A's review drain. A projected-only id has no
        // provider row, so the supplied subject is the only owner it can adopt
        // under, and the store's own adoption refuses a legacy row of another
        // subject.
        let owner = match self.store.candidate_subject(&self.space_id, candidate_id).await {
            Ok(owner) => owner,
            Err(error) => return deny(&store_error(error)),
        };
        let subject_id = match owner.as_deref() {
            Some(owner) if owner != subject_id => {
                return deny(&ProviderError::InvalidInput {
                    field: "subject_id".to_string(),
                    reason: format!(
                        "candidate {candidate_id} belongs to subject {owner}, not {subject_id}"
                    ),
                })
            }
            Some(owner) => owner,
            None => subject_id,
        };
        // The consumption decision holds the bearer's disposition section (fix
        // round 2, finding F1): a legacy review drain holds the same section
        // across its consumed-check, classification and persistence, so a
        // dismissal can never commit in between.
        let _disposition = match self.disposition(subject_id).await {
            Ok(section) => section,
            Err(error) => return deny(&error),
        };
        let principal_id = principal_id(principal);
        let op = OP_CANDIDATES_DISMISS;
        let digest = match jcs_sha256_hex(&json!({
            "candidate_id": candidate_id,
            "expected_revision": expected_revision,
            "idempotency_key": idempotency_key,
        })) {
            SpokeResult::Ok(digest) => digest,
            SpokeResult::Reject(reject) => {
                return deny(&ProviderError::Internal(reject.message));
            }
        };
        let ctx = OpContext {
            space_id: &self.space_id,
            requester_principal_id: principal_id,
            op,
            idempotency_key: Some(idempotency_key),
            payload_digest: &digest,
        };
        let admission = Admission {
            op,
            space_id: &self.space_id,
            principal_id,
            coverage: Coverage::Candidate {
                candidate_id,
                capabilities: &["review"],
            },
            issuance: None,
            subject_gate: None,
            placement_gate: None,
            freeze_gated: true,
        };
        match self
            .store
            .dismiss_legacy_candidate(
                &ctx,
                &admission,
                candidate_id,
                subject_id,
                expected_revision,
            )
            .await
        {
            Ok(receipt) => settle_receipt(&receipt),
            Err(error) => deny(&store_error(error)),
        }
    }

    /// The disposition section of the bearer that owns one provider-native
    /// candidate, or `None` when this store holds no such row (NXM-5 Task 2 fix
    /// round 3, finding F1).
    ///
    /// Every candidate consumption takes the owning bearer's section, so the
    /// ordinary wire dismissal/review cannot consume a legacy-derived row behind
    /// a review drain's back. A projected legacy id has no provider row: there
    /// is nothing for the wire path to consume and its own read answers the
    /// absence, so no section is taken. A failed probe fails the operation
    /// rather than proceeding unsectioned.
    async fn candidate_disposition(
        &self,
        space_id: &str,
        candidate_id: &str,
    ) -> Result<Option<MemoryDisposition>, ProviderError> {
        let Some(subject) = self
            .store
            .candidate_subject(space_id, candidate_id)
            .await
            .map_err(store_error)?
        else {
            return Ok(None);
        };
        self.disposition(&subject).await.map(Some)
    }

    /// §14.2/§14.3 proof seam: force the next committing transaction to fail at
    /// the commit boundary (`after_commit: false` → before `COMMIT`, no effect;
    /// `true` → after `COMMIT`, effect committed). Compiled out of a production
    /// build.
    #[cfg(feature = "test-hooks")]
    #[doc(hidden)]
    pub fn inject_commit_fault(&self, after_commit: bool) {
        self.store.inject_commit_fault(after_commit);
    }

    /// §5.2/C-113 proof seam: force the next journal publish to stop at a crash
    /// window (`after_publish: false` → after the hidden staged write, before
    /// the rename; `true` → after the publish rename, before the applied mark).
    /// Compiled out of a production build.
    #[cfg(feature = "test-hooks")]
    #[doc(hidden)]
    pub fn inject_file_fault(&self, after_publish: bool) {
        self.store.inject_file_fault(after_publish);
    }

    /// §5.2 proof seam: the provider's markdown projection root. Compiled out of
    /// a production build.
    #[cfg(feature = "test-hooks")]
    #[doc(hidden)]
    #[must_use]
    pub fn file_root(&self) -> std::path::PathBuf {
        self.store.file_root().to_path_buf()
    }

    /// §14.3 proof seam: force the next post-commit journal settlement to fail
    /// (`before_write: true` → its transaction never opens; `false` → its write
    /// fails). Both are failures after the journal COMMIT. Compiled out of a
    /// production build.
    #[cfg(feature = "test-hooks")]
    #[doc(hidden)]
    pub fn inject_settle_fault(&self, before_write: bool) {
        self.store.inject_settle_fault(before_write);
    }

    /// §6.5/§6.10 proof seam: move the provider clock forward so grant expiries
    /// and the receipt retention windows elapse. Compiled out of a production
    /// build.
    #[cfg(feature = "test-hooks")]
    #[doc(hidden)]
    pub fn advance_clock(&self, seconds: i64) {
        self.store.advance_clock(seconds);
    }

    /// §6.5/§7.28 proof seam: park the next read once it holds its connection
    /// (`entered` signals it is parked, `release` frees it), so a test can move
    /// the clock while the read is queued and only after it has parked. Compiled
    /// out of a production build.
    #[cfg(feature = "test-hooks")]
    #[doc(hidden)]
    pub fn park_next_read(
        &self,
        entered: std::sync::Arc<tokio::sync::Notify>,
        release: std::sync::Arc<tokio::sync::Barrier>,
    ) {
        self.store.park_next_read(entered, release);
    }

    /// §5.2 step 2 proof seam: park the next publication while it holds the
    /// workspace publication section — cross-process flock and in-process gate —
    /// (`entered` signals it, `release` frees it). Test only.
    #[cfg(feature = "test-hooks")]
    pub fn park_next_publish(
        &self,
        entered: std::sync::Arc<tokio::sync::Notify>,
        release: std::sync::Arc<tokio::sync::Barrier>,
    ) {
        self.store.park_next_publish(entered, release);
    }

    /// §5.2 step 2 proof seam: have every workspace flock **wait** report from
    /// its blocking task just before it blocks, so a test can cancel callers that
    /// have really reached the wait. Compiled out of a production build.
    #[cfg(feature = "test-hooks")]
    #[doc(hidden)]
    pub fn report_flock_waits(&self, report: std::sync::mpsc::Sender<()>) {
        self.store.report_flock_waits(report);
    }

    /// §7.3/§7.4. The space lifecycle ops define their own freeze admission
    /// (§7.3 names re-freezing a frozen space a no-op success, and §8.2 admits
    /// `unfreeze`), so `freeze_gated` is false for them.
    async fn change_space_status<T: DeserializeOwned>(
        &self,
        principal: &schemas::Principal,
        transition: SpaceTransition,
        space_id: &str,
        expected_revision: i64,
        idempotency_key: Option<&str>,
        request: &(impl Serialize + Sync),
    ) -> SpokeResult<T> {
        let principal_id = principal_id(principal);
        let op = transition.op();
        let digest = match payload_digest(op, request) {
            Ok(digest) => digest,
            Err(error) => return deny(&error),
        };
        let ctx = OpContext {
            space_id,
            requester_principal_id: principal_id,
            op,
            idempotency_key,
            payload_digest: &digest,
        };
        let requirements = [admin_requirement(None)];
        let admission = Admission {
            op,
            space_id,
            principal_id,
            coverage: Coverage::Requirements(&requirements),
            issuance: None,
            subject_gate: None,
            placement_gate: None,
            freeze_gated: false,
        };
        match self
            .store
            .set_space_status(&ctx, &admission, expected_revision, transition.status())
            .await
        {
            Ok(receipt) => settle_receipt(&receipt),
            Err(error) => deny(&store_error(error)),
        }
    }
}

/// The facts both `memory.grants.grant` request shapes name (§7.9); the restricted
/// shape adds the placement-context restriction (§6.5).
struct GrantFacts<'a> {
    space_id: &'a str,
    grantee: &'a str,
    capabilities: Vec<String>,
    context_ids: Vec<String>,
    shared_access: bool,
    subject_id: Option<&'a str>,
    expires_at: Option<DateTime<Utc>>,
    idempotency_key: &'a str,
}

fn grant_facts(request: &schemas::MemoryGrantsGrantRequest) -> GrantFacts<'_> {
    match request {
        schemas::MemoryGrantsGrantRequest::Variant0 {
            capabilities,
            context_ids,
            expires_at,
            idempotency_key,
            principal_id: grantee,
            shared_access,
            space_id,
            subject_id,
            ..
        } => GrantFacts {
            space_id: space_id.as_str(),
            grantee: grantee.as_str(),
            capabilities: capabilities.clone(),
            context_ids: context_ids.clone(),
            shared_access: shared_access.unwrap_or(false),
            subject_id: subject_id.as_ref().map(|value| value.as_str()),
            expires_at: *expires_at,
            idempotency_key: idempotency_key.as_str(),
        },
        schemas::MemoryGrantsGrantRequest::Variant1 {
            capabilities,
            expires_at,
            idempotency_key,
            principal_id: grantee,
            space_id,
            subject_id,
            ..
        } => GrantFacts {
            space_id: space_id.as_str(),
            grantee: grantee.as_str(),
            capabilities: capabilities.clone(),
            context_ids: Vec::new(),
            shared_access: false,
            subject_id: subject_id.as_ref().map(|value| value.as_str()),
            expires_at: expires_at.as_ref().map(|value| **value),
            idempotency_key: idempotency_key.as_str(),
        },
    }
}

/// The §7 `admin` requirement for a space- or subject-scoped admin op.
pub(super) const fn admin_requirement(subject_id: Option<&str>) -> GrantRequirement<'_> {
    GrantRequirement {
        capability: "admin",
        subject_id,
        context_ids: Vec::new(),
        reads_shared: false,
        subject_all: false,
    }
}

fn bootstrap_grant_id(creator_id: &str) -> String {
    format!("grt_bootstrap_{creator_id}")
}

/// Round A's error taxonomy is deliberately narrower than `CoreError`: opening
/// maps it onto the core taxonomy, while op refusals travel the `SpokeResult`
/// reject channel, so only the open path needs this bridge.
fn to_core_error(error: ProviderError) -> CoreError {
    match error {
        ProviderError::Internal(reason) => CoreError::Internal { category: reason },
        other => CoreError::Internal {
            category: other.message(),
        },
    }
}

fn principal_id(principal: &schemas::Principal) -> &str {
    principal.principal_id.as_str()
}

/// `expected_revision` as the stored integer. A wire revision above `i64::MAX`
/// cannot be stored, so it saturates and can only ever fail the CAS.
fn expected_revision(value: NonZeroU64) -> i64 {
    i64::try_from(value.get()).unwrap_or(i64::MAX)
}

// ── Content-family projections and requirements (§6.6–§6.8) ────────────

/// §6.6/§6.7 placement projection: `context_id` is present iff the visibility is
/// `context` — the conditional both the table CHECK and the wire shape carry.
fn placement_body(placement: Placement<'_>) -> Value {
    placement.context_id.map_or_else(
        || json!({ "visibility": placement.visibility }),
        |context_id| json!({ "visibility": placement.visibility, "context_id": context_id }),
    )
}

fn json_member(raw: &str, what: &str) -> Result<Value, ProviderError> {
    serde_json::from_str(raw).map_err(|e| ProviderError::Internal(format!("{what}: {e}")))
}

fn to_json<T: Serialize>(value: &T, what: &str) -> Result<Value, ProviderError> {
    serde_json::to_value(value).map_err(|e| ProviderError::Internal(format!("{what}: {e}")))
}

/// §6.5 requirement for one named placement: a `context` placement needs
/// coverage of that context, a `shared` placement needs shared access.
fn placement_requirement<'a>(
    capability: &'a str,
    subject_id: &'a str,
    placement: Placement<'_>,
) -> GrantRequirement<'a> {
    GrantRequirement {
        capability,
        subject_id: Some(subject_id),
        context_ids: placement
            .context_id
            .map(str::to_string)
            .into_iter()
            .collect(),
        reads_shared: placement.visibility == Placement::SHARED,
        subject_all: false,
    }
}

/// D-7: a Nexus authoring kind keeps its own name inside the namespaced record
/// kind; any other hint is an open-vocabulary product kind and is used verbatim.
fn candidate_kind(hint: Option<&str>) -> String {
    match hint {
        None => policy::namespaced_record_kind("custom"),
        Some(hint) if policy::NEXUS_MEMORY_KINDS.contains(&hint) => {
            policy::namespaced_record_kind(hint)
        }
        Some(hint) => hint.to_string(),
    }
}

/// The bound query a content page belongs to (MEM-R026): everything that
/// changes which rows the page was cut from.
fn selection_key(selection: &Selection) -> String {
    match selection {
        Selection::SubjectAll => "subject_all".to_string(),
        Selection::SharedOnly => "shared_only".to_string(),
        Selection::SelectedContexts {
            context_ids,
            include_shared,
        } => format!(
            "selected_contexts({}){}",
            context_ids.join(","),
            if *include_shared { "+shared" } else { "" }
        ),
    }
}

fn filter_key(filter: Option<&[String]>) -> String {
    filter.map_or_else(|| "*".to_string(), |sources| sources.join(","))
}

/// §9.2 ranking input: the record's untrusted text, never re-interpreted.
fn record_text(row: &RecordRow) -> String {
    json_member(&row.content_json, "record content")
        .ok()
        .and_then(|content| {
            content
                .get("text")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_default()
}

/// §6.7 candidate projection. The pending shape carries the proposed body; the
/// consumed shell replaces it with the digest and the outcome (§8.1 scrub), and
/// `expires_at` belongs to the pending shape alone.
fn candidate_body(row: &CandidateRow) -> Result<Value, ProviderError> {
    let mut body = json!({
        "schema_version": 1,
        "candidate_id": row.candidate_id,
        "space_id": row.space_id,
        "subject_id": row.subject_id,
        "capture_scope": placement_body(row.scope()),
        "classification": json_member(&row.classification_json, "candidate classification")?,
        "provenance": json_member(&row.provenance_json, "candidate provenance")?,
        "status": row.status,
        "created_at": row.created_at,
        "revision": row.revision,
        "extensions": {},
    });
    let members = body
        .as_object_mut()
        .expect("candidate projection is an object");
    if let Some(occurred_at) = &row.occurred_at {
        members.insert("occurred_at".to_string(), json!(occurred_at));
    }
    if row.status == "pending" {
        let content = json_member(
            row.content_json.as_deref().unwrap_or("null"),
            "candidate content",
        )?;
        members.insert(
            "proposed".to_string(),
            json!({ "kind": row.kind, "content": content }),
        );
        if let Some(expires_at) = &row.expires_at {
            members.insert("expires_at".to_string(), json!(expires_at));
        }
    } else {
        members.insert("proposed".to_string(), json!({ "kind": row.kind }));
        members.insert(
            "consumed_by".to_string(),
            json_member(
                row.consumed_by_json.as_deref().unwrap_or("null"),
                "candidate outcome",
            )?,
        );
        members.insert("content_digest".to_string(), json!(row.content_digest));
    }
    Ok(body)
}

/// §6.6 record projection. A tombstoned record carries its scrubbed content and
/// the deletion marker instead of `import_ref` (MEM-R014).
fn record_body(row: &RecordRow) -> Result<Value, ProviderError> {
    let mut body = json!({
        "schema_version": 1,
        "record_id": row.record_id,
        "space_id": row.space_id,
        "subject_id": row.subject_id,
        "kind": row.kind,
        "content": json_member(&row.content_json, "record content")?,
        "scope": placement_body(row.scope()),
        "provenance": json_member(&row.provenance_json, "record provenance")?,
        "created_at": row.created_at,
        "updated_at": row.updated_at,
        "revision": row.revision,
        "extensions": {},
    });
    let members = body
        .as_object_mut()
        .expect("record projection is an object");
    if let Some(occurred_at) = &row.occurred_at {
        members.insert("occurred_at".to_string(), json!(occurred_at));
    }
    match &row.tombstone_json {
        Some(raw) => {
            members.insert(
                "tombstone".to_string(),
                json_member(raw, "record tombstone")?,
            );
        }
        None => {
            if let Some(raw) = &row.import_ref_json {
                members.insert(
                    "import_ref".to_string(),
                    json_member(raw, "record import ref")?,
                );
            }
        }
    }
    Ok(body)
}

/// §6.8 profile projection.
fn profile_body(row: &ProfileRow) -> Result<Value, ProviderError> {
    Ok(json!({
        "schema_version": 1,
        "space_id": row.space_id,
        "subject_id": row.subject_id,
        "profile_kind": row.profile_kind,
        "document": json_member(&row.document_json, "profile document")?,
        "updated_by": row.updated_by,
        "created_at": row.created_at,
        "updated_at": row.updated_at,
        "revision": row.revision,
        "extensions": {},
    }))
}

#[async_trait]
impl MemoryPort for LocalMemoryProvider {
    async fn describe(
        &self,
        _principal: &schemas::Principal,
        _request: schemas::MemoryDescribeRequest,
    ) -> SpokeResult<schemas::MemoryDescribeResponse> {
        respond(describe_body())
    }

    async fn list_spaces(
        &self,
        principal: &schemas::Principal,
        request: schemas::MemorySpacesListRequest,
    ) -> SpokeResult<schemas::MemorySpacesListResponse> {
        let principal_id = principal_id(principal);
        if let SpokeResult::Reject(reject) =
            validate_memory_spaces_list(&request, policy::MAX_PAGE_SIZE_BOUND)
        {
            return deny(&ProviderError::InvalidInput {
                field: "limit".to_string(),
                reason: reject.message,
            });
        }
        let (spaces, marker) = match self.store.visible_spaces(principal_id).await {
            Ok(page) => page,
            Err(error) => return deny(&store_error(error)),
        };
        let scope = cursor_scope("memory.spaces.list", None, None, None);
        let offset = match decode_cursor(
            cursor_text(request.cursor.as_ref()),
            principal_id,
            &scope,
            &marker,
        ) {
            Ok(offset) => offset,
            Err(error) => return deny(&error),
        };
        let (page, next) = page(spaces, page_limit(request.limit), offset);
        let bodies: Vec<Value> = page.iter().map(space_body).collect();
        let next_cursor = next.map(|offset| encode_cursor(principal_id, &scope, offset, &marker));
        respond(json!({ "spaces": bodies, "next_cursor": next_cursor }))
    }

    async fn freeze_space(
        &self,
        principal: &schemas::Principal,
        request: schemas::MemorySpacesFreezeRequest,
    ) -> SpokeResult<schemas::MemorySpacesFreezeResponse> {
        self.change_space_status(
            principal,
            SpaceTransition::Freeze,
            request.space_id.as_str(),
            expected_revision(request.expected_revision),
            Some(request.idempotency_key.as_str()),
            &request,
        )
        .await
    }

    async fn unfreeze_space(
        &self,
        principal: &schemas::Principal,
        request: schemas::MemorySpacesUnfreezeRequest,
    ) -> SpokeResult<schemas::MemorySpacesUnfreezeResponse> {
        self.change_space_status(
            principal,
            SpaceTransition::Unfreeze,
            request.space_id.as_str(),
            expected_revision(request.expected_revision),
            Some(request.idempotency_key.as_str()),
            &request,
        )
        .await
    }

    async fn ensure_subject(
        &self,
        principal: &schemas::Principal,
        request: schemas::MemorySubjectsEnsureRequest,
    ) -> SpokeResult<schemas::MemorySubjectsEnsureResponse> {
        let principal_id = principal_id(principal);
        let op = OP_ENSURE_SUBJECT;
        let digest = match payload_digest(op, &request) {
            Ok(digest) => digest,
            Err(error) => return deny(&error),
        };
        let kind = request
            .kind
            .as_ref()
            .map(|kind| (kind.ns.as_str(), kind.id.as_str()));
        // The sole `idempotency_key` exemption (§7.0.5): a keyless receipt sits
        // outside the dedup namespace, so every ensure records its own outcome.
        let ctx = OpContext {
            space_id: request.space_id.as_str(),
            requester_principal_id: principal_id,
            op,
            idempotency_key: None,
            payload_digest: &digest,
        };
        let requirements = [GrantRequirement {
            capability: "capture",
            subject_id: Some(request.subject_id.as_str()),
            context_ids: Vec::new(),
            reads_shared: false,
            subject_all: false,
        }];
        let admission = Admission {
            op,
            space_id: request.space_id.as_str(),
            principal_id,
            coverage: Coverage::Requirements(&requirements),
            issuance: None,
            // §7.5: a fenced or archived subject rejects its writes, checked in
            // the same transaction as the get-or-create.
            subject_gate: Some(request.subject_id.as_str()),
            placement_gate: None,
            freeze_gated: true,
        };
        match self
            .store
            .ensure_subject(&ctx, &admission, request.subject_id.as_str(), kind)
            .await
        {
            Ok(receipt) => settle_receipt(&receipt),
            Err(error) => deny(&store_error(error)),
        }
    }

    async fn get_subject(
        &self,
        principal: &schemas::Principal,
        request: schemas::MemorySubjectsGetRequest,
    ) -> SpokeResult<schemas::MemorySubjectsGetResponse> {
        let principal_id = principal_id(principal);
        // §7.6/S-4: the "any live grant" test and the subject row are one read
        // transaction on the store's clock.
        match self
            .store
            .subject_for_read(
                principal_id,
                request.space_id.as_str(),
                request.subject_id.as_str(),
            )
            .await
        {
            Ok(subject) => match subject_body(&subject) {
                Ok(body) => respond(json!({ "subject": body })),
                Err(error) => deny(&error),
            },
            Err(error) => deny(&store_error(error)),
        }
    }

    async fn fence_subject(
        &self,
        principal: &schemas::Principal,
        request: schemas::MemorySubjectsFenceRequest,
    ) -> SpokeResult<schemas::MemorySubjectsFenceResponse> {
        let principal_id = principal_id(principal);
        if let SpokeResult::Reject(reject) = validate_memory_subjects_fence(&request) {
            return deny(&ProviderError::InvalidInput {
                field: "target".to_string(),
                reason: reject.message,
            });
        }
        let op = OP_FENCE_SUBJECT;
        let target = match &request.target {
            schemas::memory_subjects_fence_request::Target::Subject => FenceTarget::Subject,
            schemas::memory_subjects_fence_request::Target::Context(context_id) => {
                FenceTarget::Context(context_id.as_str())
            }
        };
        let action = match request.action {
            schemas::memory_subjects_fence_request::Action::Fence => FenceAction::Fence,
            schemas::memory_subjects_fence_request::Action::Unfence => FenceAction::Unfence,
            schemas::memory_subjects_fence_request::Action::Archive => FenceAction::Archive,
        };
        // §6.5/§7.7: authorization is bound to what the op actually touches — a
        // context fence needs coverage of THAT context, and a whole-subject
        // fence/archive needs unrestricted coverage of the subject.
        let mut requirement = admin_requirement(Some(request.subject_id.as_str()));
        match &target {
            FenceTarget::Subject => requirement.subject_all = true,
            FenceTarget::Context(context_id) => {
                requirement.context_ids = vec![(*context_id).to_string()];
            }
        }
        let digest = match payload_digest(op, &request) {
            Ok(digest) => digest,
            Err(error) => return deny(&error),
        };
        let ctx = OpContext {
            space_id: request.space_id.as_str(),
            requester_principal_id: principal_id,
            op,
            idempotency_key: Some(request.idempotency_key.as_str()),
            payload_digest: &digest,
        };
        let requirements = [requirement];
        let admission = Admission {
            op,
            space_id: request.space_id.as_str(),
            principal_id,
            coverage: Coverage::Requirements(&requirements),
            issuance: None,
            subject_gate: None,
            placement_gate: None,
            freeze_gated: true,
        };
        let fence = Fence {
            subject_id: request.subject_id.as_str(),
            target,
            action,
            expected_revision: expected_revision(request.expected_revision),
        };
        match self.store.fence_subject(&ctx, &admission, fence).await {
            Ok(receipt) => settle_receipt(&receipt),
            Err(error) => deny(&store_error(error)),
        }
    }

    async fn list_grants(
        &self,
        principal: &schemas::Principal,
        request: schemas::MemoryGrantsListRequest,
    ) -> SpokeResult<schemas::MemoryGrantsListResponse> {
        let principal_id = principal_id(principal);
        if let SpokeResult::Reject(reject) =
            validate_memory_grants_list(&request, policy::MAX_PAGE_SIZE_BOUND)
        {
            return deny(&ProviderError::InvalidInput {
                field: "limit".to_string(),
                reason: reject.message,
            });
        }
        let principal_filter = request.principal_id.as_ref().map(|value| value.as_str());
        let subject_filter = request.subject_id.as_ref().map(|value| value.as_str());
        let limit = page_limit(request.limit);
        // §7.0.7/MEM-R026: the cursor's offset selects the SQL page; its claim is
        // verified after the read, because the scope names the principal filter
        // that read resolves.
        let page_offset = match claimed_offset(cursor_text(request.cursor.as_ref())) {
            Ok(offset) => offset,
            Err(error) => return deny(&error),
        };
        // §7.0.7/§7.8/S-4: the `admin` verdict, the page and its invalidation
        // marker are ONE read transaction on the store's clock, and the returned
        // filter is the one that read actually applied.
        let (rows, marker, filter, more) = match self
            .store
            .grants_page_for(
                principal_id,
                request.space_id.as_str(),
                subject_filter,
                principal_filter,
                Some((limit, page_offset)),
            )
            .await
        {
            Ok(page) => page,
            Err(error) => return deny(&store_error(error)),
        };
        let scope = cursor_scope(
            "memory.grants.list",
            Some(request.space_id.as_str()),
            subject_filter,
            filter.as_deref(),
        );
        let offset = match decode_cursor(
            cursor_text(request.cursor.as_ref()),
            principal_id,
            &scope,
            &marker,
        ) {
            Ok(offset) => offset,
            Err(error) => return deny(&error),
        };
        let next = more.then(|| offset.saturating_add(limit));
        let mut bodies = Vec::with_capacity(rows.len());
        for row in &rows {
            match grant_body(row) {
                Ok(body) => bodies.push(body),
                Err(error) => return deny(&error),
            }
        }
        let next_cursor = next.map(|offset| encode_cursor(principal_id, &scope, offset, &marker));
        respond(json!({ "grants": bodies, "next_cursor": next_cursor }))
    }

    async fn grant(
        &self,
        principal: &schemas::Principal,
        request: schemas::MemoryGrantsGrantRequest,
    ) -> SpokeResult<schemas::MemoryGrantsGrantResponse> {
        let principal_id = principal_id(principal);
        if let SpokeResult::Reject(reject) = validate_memory_grants_grant(&request) {
            return deny(&ProviderError::InvalidInput {
                field: "capabilities".to_string(),
                reason: reject.message,
            });
        }
        let GrantFacts {
            space_id,
            grantee,
            capabilities,
            context_ids,
            shared_access,
            subject_id,
            expires_at,
            idempotency_key,
        } = grant_facts(&request);
        let capability_refs: Vec<&str> = capabilities.iter().map(String::as_str).collect();
        let issued = IssuedGrant {
            capabilities: &capability_refs,
            subject_id,
            context_ids: &context_ids,
            shared_access,
            expires_at,
        };
        let digest = match payload_digest(OP_GRANT, &request) {
            Ok(digest) => digest,
            Err(error) => return deny(&error),
        };
        let grant_id = format!("grt_{}", uuid::Uuid::new_v4().simple());
        let ctx = OpContext {
            space_id,
            requester_principal_id: principal_id,
            op: OP_GRANT,
            idempotency_key: Some(idempotency_key),
            payload_digest: &digest,
        };
        let new_grant = NewGrant {
            grant_id: &grant_id,
            space_id,
            subject_id,
            principal_id: grantee,
            capabilities: &capabilities,
            context_ids: &context_ids,
            shared_access,
            expires_at,
        };
        // §7.9: admission uses the ISSUED grant's scope — a subject-scoped admin
        // delegates within its own subject — and non-amplification is evaluated
        // on the same grant set, inside the transaction that inserts the grant.
        let requirements = [admin_requirement(subject_id)];
        let admission = Admission {
            op: OP_GRANT,
            space_id,
            principal_id,
            coverage: Coverage::Requirements(&requirements),
            issuance: Some(&issued),
            subject_gate: subject_id,
            placement_gate: None,
            freeze_gated: true,
        };
        match self.store.insert_grant(&ctx, &admission, &new_grant).await {
            Ok(receipt) => settle_receipt(&receipt),
            Err(error) => deny(&store_error(error)),
        }
    }

    async fn revoke_grant(
        &self,
        principal: &schemas::Principal,
        request: schemas::MemoryGrantsRevokeRequest,
    ) -> SpokeResult<schemas::MemoryGrantsRevokeResponse> {
        let principal_id = principal_id(principal);
        let op = OP_REVOKE_GRANT;
        let digest = match payload_digest(op, &request) {
            Ok(digest) => digest,
            Err(error) => return deny(&error),
        };
        let ctx = OpContext {
            space_id: request.space_id.as_str(),
            requester_principal_id: principal_id,
            op,
            idempotency_key: Some(request.idempotency_key.as_str()),
            payload_digest: &digest,
        };
        // `memory.grants.revoke` is on the §8.2 allowlist, so an incident can be
        // contained while the space is frozen.
        let requirements = [admin_requirement(None)];
        let admission = Admission {
            op,
            space_id: request.space_id.as_str(),
            principal_id,
            coverage: Coverage::Requirements(&requirements),
            issuance: None,
            subject_gate: None,
            placement_gate: None,
            freeze_gated: true,
        };
        match self
            .store
            .revoke_grant(
                &ctx,
                &admission,
                request.grant_id.as_str(),
                expected_revision(request.expected_revision),
            )
            .await
        {
            Ok(receipt) => settle_receipt(&receipt),
            Err(error) => deny(&store_error(error)),
        }
    }

    // ── Content families ───────────────────────────────────────────────
    //
    // Every mutating op here follows the same §7.0 discipline as the control
    // families:
    // validate the request against the protocol's own rules, bind the payload
    // digest, run admission (coverage → dedup probe → lifecycle gates) inside
    // the effect transaction, and answer with the committed receipt whose
    // `result` is content-free (MEM-R022). Reads authorize first and page what
    // authorization left (MEM-R006).

    async fn capture(
        &self,
        principal: &schemas::Principal,
        request: schemas::MemoryCaptureRequest,
    ) -> SpokeResult<schemas::MemoryCaptureResponse> {
        let principal_id = principal_id(principal);
        if let SpokeResult::Reject(reject) =
            validate_memory_capture(&request, policy::max_content_bytes())
        {
            return deny(&ProviderError::InvalidInput {
                field: "content".to_string(),
                reason: reject.message,
            });
        }
        let op = OP_CAPTURE;
        let digest = match payload_digest(op, &request) {
            Ok(digest) => digest,
            Err(error) => return deny(&error),
        };
        let ctx = OpContext {
            space_id: request.space_id.as_str(),
            requester_principal_id: principal_id,
            op,
            idempotency_key: Some(request.idempotency_key.as_str()),
            payload_digest: &digest,
        };
        let subject_id = request.subject_id.as_str();
        // §7.11: `capture_scope` is the admitted placement and becomes the
        // candidate's immutable authorization fence.
        let placement = match &request.capture_scope {
            schemas::memory_capture_request::PlacementScope::Shared => Placement {
                visibility: Placement::SHARED,
                context_id: None,
            },
            schemas::memory_capture_request::PlacementScope::Context(context_id) => Placement {
                visibility: Placement::CONTEXT,
                context_id: Some(context_id.as_str()),
            },
        };
        let requirements = [placement_requirement("capture", subject_id, placement)];
        let admission = Admission {
            op,
            space_id: request.space_id.as_str(),
            principal_id,
            coverage: Coverage::Requirements(&requirements),
            issuance: None,
            subject_gate: Some(subject_id),
            placement_gate: Some(placement),
            freeze_gated: true,
        };
        let now = Utc::now();
        let kind = candidate_kind(request.kind_hint.as_ref().map(|hint| hint.as_str()));
        let classification = policy::classify(
            request.content.text.as_str(),
            request.kind_hint.as_ref().map(|hint| hint.as_str()),
        );
        let content = match to_json(&request.content, "capture content") {
            Ok(content) => content,
            Err(error) => return deny(&error),
        };
        let anchors = match to_json(&request.anchors, "capture anchors") {
            Ok(anchors) => anchors,
            Err(error) => return deny(&error),
        };
        let provenance = json!({
            "source_context_ids": request.source_context_ids,
            "anchors": anchors,
            "captured_by": principal_id,
            "captured_at": storage::timestamp(now),
        });
        let classification_json = json!({
            "disposition": classification.disposition,
            "rationale_codes": classification.rationale_codes,
        });
        let occurred_at = request.occurred_at.map(storage::timestamp);
        // §7.11/§6.7: the advisory classifier can leave a capture with no
        // candidate at all; that is a committed outcome with its count recorded.
        let candidate_id = storage::mint_candidate_id();
        let candidates: Vec<NewCandidate<'_>> = if classification.disposition == "drop" {
            Vec::new()
        } else {
            vec![NewCandidate {
                candidate_id: candidate_id.as_str(),
                subject_id,
                placement,
                kind: kind.as_str(),
                content: &content,
                provenance: &provenance,
                classification: &classification_json,
                occurred_at: occurred_at.as_deref(),
                expires_at: None,
            }]
        };
        match self.store.capture(&ctx, &admission, &candidates).await {
            Ok(receipt) => settle_receipt(&receipt),
            Err(error) => deny(&store_error(error)),
        }
    }

    async fn list_candidates(
        &self,
        principal: &schemas::Principal,
        request: schemas::MemoryCandidatesListRequest,
    ) -> SpokeResult<schemas::MemoryCandidatesListResponse> {
        let principal_id = principal_id(principal);
        if let SpokeResult::Reject(reject) =
            validate_memory_candidates_list(&request, policy::MAX_PAGE_SIZE_BOUND)
        {
            return deny(&ProviderError::InvalidInput {
                field: "limit".to_string(),
                reason: reject.message,
            });
        }
        let status = request.status.map(|status| status.to_string());
        let scope = format!(
            "{}|status={}",
            cursor_scope(
                OP_CANDIDATES_LIST,
                Some(request.space_id.as_str()),
                Some(request.subject_id.as_str()),
                None,
            ),
            status.as_deref().unwrap_or("*")
        );
        // §7.0.7/MEM-R026: the cursor's offset selects the authorized page; its
        // marker is verified against the snapshot the page was read in.
        let (offset, claimed) =
            match cursor_page(cursor_text(request.cursor.as_ref()), principal_id, &scope) {
                Ok(page) => page,
                Err(error) => return deny(&error),
            };
        let limit = page_limit(request.limit);
        let (rows, marker, more) = match self
            .store
            .candidates(
                request.space_id.as_str(),
                request.subject_id.as_str(),
                status.as_deref(),
                principal_id,
                Some((limit, offset)),
            )
            .await
        {
            Ok(page) => page,
            Err(error) => return deny(&store_error(error)),
        };
        if let Err(error) = verify_cursor(claimed.as_deref(), &marker) {
            return deny(&error);
        }
        let next = more.then(|| offset.saturating_add(limit));
        let mut bodies = Vec::with_capacity(rows.len());
        for row in &rows {
            match candidate_body(row) {
                Ok(body) => bodies.push(body),
                Err(error) => return deny(&error),
            }
        }
        let next_cursor = next.map(|offset| encode_cursor(principal_id, &scope, offset, &marker));
        respond(json!({ "candidates": bodies, "next_cursor": next_cursor }))
    }

    async fn get_candidate(
        &self,
        principal: &schemas::Principal,
        request: schemas::MemoryCandidatesGetRequest,
    ) -> SpokeResult<schemas::MemoryCandidatesGetResponse> {
        let row = match self
            .store
            .candidate_for_read(
                request.space_id.as_str(),
                request.candidate_id.as_str(),
                principal_id(principal),
            )
            .await
        {
            Ok(row) => row,
            Err(error) => return deny(&store_error(error)),
        };
        match row {
            Some(row) => match candidate_body(&row) {
                Ok(body) => respond(json!({ "candidate": body })),
                Err(error) => deny(&error),
            },
            None => deny(&ProviderError::NotFound(format!(
                "candidate {}",
                request.candidate_id.as_str()
            ))),
        }
    }

    async fn dismiss_candidate(
        &self,
        principal: &schemas::Principal,
        request: schemas::MemoryCandidatesDismissRequest,
    ) -> SpokeResult<schemas::MemoryCandidatesDismissResponse> {
        // Whichever op consumes a candidate, it holds the owning bearer's
        // disposition section (fix round 3, finding F1): the legacy review drain
        // holds the same section across its consumed-check and classification.
        let _disposition = match self
            .candidate_disposition(request.space_id.as_str(), request.candidate_id.as_str())
            .await
        {
            Ok(section) => section,
            Err(error) => return deny(&error),
        };
        let principal_id = principal_id(principal);
        let op = OP_CANDIDATES_DISMISS;
        let digest = match payload_digest(op, &request) {
            Ok(digest) => digest,
            Err(error) => return deny(&error),
        };
        let ctx = OpContext {
            space_id: request.space_id.as_str(),
            requester_principal_id: principal_id,
            op,
            idempotency_key: Some(request.idempotency_key.as_str()),
            payload_digest: &digest,
        };
        // §7.14: `review` covering the candidate's immutable `capture_scope`.
        let admission = Admission {
            op,
            space_id: request.space_id.as_str(),
            principal_id,
            coverage: Coverage::Candidate {
                candidate_id: request.candidate_id.as_str(),
                capabilities: &["review"],
            },
            issuance: None,
            subject_gate: None,
            placement_gate: None,
            freeze_gated: true,
        };
        match self
            .store
            .dismiss_candidate(
                &ctx,
                &admission,
                request.candidate_id.as_str(),
                expected_revision(request.expected_revision),
            )
            .await
        {
            Ok(receipt) => settle_receipt(&receipt),
            Err(error) => deny(&store_error(error)),
        }
    }

    async fn review_candidate(
        &self,
        principal: &schemas::Principal,
        request: schemas::MemoryCandidatesReviewRequest,
    ) -> SpokeResult<schemas::MemoryCandidatesReviewResponse> {
        use schemas::memory_candidates_review_request::Decision;

        let principal_id = principal_id(principal);
        if let SpokeResult::Reject(reject) = validate_memory_candidates_review(&request) {
            return deny(&ProviderError::InvalidInput {
                field: "decision".to_string(),
                reason: reject.message,
            });
        }
        // A review consumes the candidate exactly as a dismissal does, so it
        // takes the owning bearer's disposition section too (fix round 3,
        // finding F1).
        let _disposition = match self
            .candidate_disposition(request.space_id.as_str(), request.candidate_id.as_str())
            .await
        {
            Ok(section) => section,
            Err(error) => return deny(&error),
        };
        let op = OP_CANDIDATES_REVIEW;
        let digest = match payload_digest(op, &request) {
            Ok(digest) => digest,
            Err(error) => return deny(&error),
        };
        let ctx = OpContext {
            space_id: request.space_id.as_str(),
            requester_principal_id: principal_id,
            op,
            idempotency_key: Some(request.idempotency_key.as_str()),
            payload_digest: &digest,
        };
        // §7.15: `review` *and* `records_write`, both covering the candidate's
        // capture scope — created records inherit that placement verbatim.
        let admission = Admission {
            op,
            space_id: request.space_id.as_str(),
            principal_id,
            coverage: Coverage::Candidate {
                candidate_id: request.candidate_id.as_str(),
                capabilities: &["review", "records_write"],
            },
            issuance: None,
            subject_gate: None,
            placement_gate: None,
            freeze_gated: true,
        };
        let override_content = match &request.decision {
            Decision::Promote {
                content_override: Some(content),
                ..
            } => match to_json(content, "decision content_override") {
                Ok(content) => Some(content),
                Err(error) => return deny(&error),
            },
            _ => None,
        };
        let fragments: Vec<(String, Value)> = match &request.decision {
            Decision::Fragment { fragments } => {
                let mut built = Vec::with_capacity(fragments.len());
                for fragment in fragments {
                    match to_json(&fragment.content, "fragment content") {
                        Ok(content) => built.push((fragment.kind.as_str().to_string(), content)),
                        Err(error) => return deny(&error),
                    }
                }
                built
            }
            _ => Vec::new(),
        };
        let decision = match &request.decision {
            Decision::Promote { kind, .. } => ReviewDecision::Promote {
                kind: kind.as_ref().map(|kind| kind.as_str()),
                content: override_content.as_ref(),
            },
            Decision::Fragment { .. } => ReviewDecision::Fragment { items: &fragments },
            Decision::Drop { .. } => ReviewDecision::Drop,
        };
        match self
            .store
            .review_candidate(
                &ctx,
                &admission,
                request.candidate_id.as_str(),
                expected_revision(request.expected_revision),
                &decision,
            )
            .await
        {
            Ok(receipt) => settle_receipt(&receipt),
            Err(error) => deny(&store_error(error)),
        }
    }

    async fn create_record(
        &self,
        principal: &schemas::Principal,
        request: schemas::MemoryRecordsCreateRequest,
    ) -> SpokeResult<schemas::MemoryRecordsCreateResponse> {
        let principal_id = principal_id(principal);
        if let SpokeResult::Reject(reject) =
            validate_memory_records_create(&request, policy::max_content_bytes())
        {
            return deny(&ProviderError::InvalidInput {
                field: "content".to_string(),
                reason: reject.message,
            });
        }
        let op = OP_RECORDS_CREATE;
        let digest = match payload_digest(op, &request) {
            Ok(digest) => digest,
            Err(error) => return deny(&error),
        };
        let ctx = OpContext {
            space_id: request.space_id.as_str(),
            requester_principal_id: principal_id,
            op,
            idempotency_key: Some(request.idempotency_key.as_str()),
            payload_digest: &digest,
        };
        let subject_id = request.subject_id.as_str();
        let placement = match &request.scope {
            schemas::memory_records_create_request::PlacementScope::Shared => Placement {
                visibility: Placement::SHARED,
                context_id: None,
            },
            schemas::memory_records_create_request::PlacementScope::Context(context_id) => {
                Placement {
                    visibility: Placement::CONTEXT,
                    context_id: Some(context_id.as_str()),
                }
            }
        };
        let requirements = [placement_requirement(
            "records_write",
            subject_id,
            placement,
        )];
        let admission = Admission {
            op,
            space_id: request.space_id.as_str(),
            principal_id,
            coverage: Coverage::Requirements(&requirements),
            issuance: None,
            subject_gate: Some(subject_id),
            placement_gate: Some(placement),
            freeze_gated: true,
        };
        let content = match to_json(&request.content, "record content") {
            Ok(content) => content,
            Err(error) => return deny(&error),
        };
        let anchors = match to_json(&request.anchors, "record anchors") {
            Ok(anchors) => anchors,
            Err(error) => return deny(&error),
        };
        // §6.6/§10: `captured_by`/`captured_at` are provider-populated; the
        // context claims are recorded verbatim and never authorize anything.
        let provenance = json!({
            "source_context_ids": request.source_context_ids,
            "anchors": anchors,
            "captured_by": principal_id,
            "captured_at": storage::timestamp(Utc::now()),
        });
        match self
            .store
            .create_record(
                &ctx,
                &admission,
                subject_id,
                placement,
                request.kind.as_str(),
                &content,
                &provenance,
            )
            .await
        {
            Ok(receipt) => settle_receipt(&receipt),
            Err(error) => deny(&store_error(error)),
        }
    }

    async fn get_record(
        &self,
        principal: &schemas::Principal,
        request: schemas::MemoryRecordsGetRequest,
    ) -> SpokeResult<schemas::MemoryRecordsGetResponse> {
        let row = match self
            .store
            .record_for_read(
                request.space_id.as_str(),
                request.record_id.as_str(),
                principal_id(principal),
            )
            .await
        {
            Ok(row) => row,
            Err(error) => return deny(&store_error(error)),
        };
        match row {
            Some(row) => match record_body(&row) {
                Ok(body) => respond(json!({ "record": body })),
                Err(error) => deny(&error),
            },
            None => deny(&ProviderError::NotFound(format!(
                "record {}",
                request.record_id.as_str()
            ))),
        }
    }

    #[allow(clippy::too_many_lines)]
    async fn update_record(
        &self,
        principal: &schemas::Principal,
        request: schemas::MemoryRecordsUpdateRequest,
    ) -> SpokeResult<schemas::MemoryRecordsUpdateResponse> {
        use schemas::memory_records_update_request::MemoryRecordsUpdateRequest as UpdateRequest;

        let principal_id = principal_id(principal);
        if let SpokeResult::Reject(reject) =
            validate_memory_records_update(&request, policy::max_content_bytes())
        {
            return deny(&ProviderError::InvalidInput {
                field: "content".to_string(),
                reason: reject.message,
            });
        }
        let (space_id, record_id, expected, content, kind) = match &request {
            UpdateRequest::Variant0 {
                content,
                expected_revision,
                record_id,
                space_id,
                ..
            } => match to_json(content, "record content") {
                Ok(content) => (
                    space_id.as_str(),
                    record_id.as_str(),
                    *expected_revision,
                    Some(content),
                    None,
                ),
                Err(error) => return deny(&error),
            },
            UpdateRequest::Variant1 {
                content,
                expected_revision,
                kind,
                record_id,
                space_id,
                ..
            } => {
                let content = match content {
                    Some(content) => match to_json(content, "record content") {
                        Ok(content) => Some(content),
                        Err(error) => return deny(&error),
                    },
                    None => None,
                };
                (
                    space_id.as_str(),
                    record_id.as_str(),
                    *expected_revision,
                    content,
                    Some(kind.as_str()),
                )
            }
        };
        let op = OP_RECORDS_UPDATE;
        let digest = match payload_digest(op, &request) {
            Ok(digest) => digest,
            Err(error) => return deny(&error),
        };
        let ctx = OpContext {
            space_id,
            requester_principal_id: principal_id,
            op,
            idempotency_key: Some(match &request {
                UpdateRequest::Variant0 {
                    idempotency_key, ..
                }
                | UpdateRequest::Variant1 {
                    idempotency_key, ..
                } => idempotency_key.as_str(),
            }),
            payload_digest: &digest,
        };
        let admission = Admission {
            op,
            space_id,
            principal_id,
            coverage: Coverage::Record {
                record_id,
                capabilities: &["records_write"],
                target: None,
            },
            issuance: None,
            subject_gate: None,
            placement_gate: None,
            freeze_gated: true,
        };
        match self
            .store
            .update_record(
                &ctx,
                &admission,
                record_id,
                expected_revision(expected),
                content.as_ref(),
                kind,
            )
            .await
        {
            Ok(receipt) => settle_receipt(&receipt),
            Err(error) => deny(&store_error(error)),
        }
    }

    async fn delete_record(
        &self,
        principal: &schemas::Principal,
        request: schemas::MemoryRecordsDeleteRequest,
    ) -> SpokeResult<schemas::MemoryRecordsDeleteResponse> {
        let principal_id = principal_id(principal);
        let op = OP_RECORDS_DELETE;
        let digest = match payload_digest(op, &request) {
            Ok(digest) => digest,
            Err(error) => return deny(&error),
        };
        let ctx = OpContext {
            space_id: request.space_id.as_str(),
            requester_principal_id: principal_id,
            op,
            idempotency_key: Some(request.idempotency_key.as_str()),
            payload_digest: &digest,
        };
        let admission = Admission {
            op,
            space_id: request.space_id.as_str(),
            principal_id,
            coverage: Coverage::Record {
                record_id: request.record_id.as_str(),
                capabilities: &["records_write"],
                target: None,
            },
            issuance: None,
            subject_gate: None,
            placement_gate: None,
            freeze_gated: true,
        };
        match self
            .store
            .delete_record(
                &ctx,
                &admission,
                request.record_id.as_str(),
                expected_revision(request.expected_revision),
                request.reason.as_deref(),
                principal_id,
            )
            .await
        {
            Ok(receipt) => settle_receipt(&receipt),
            Err(error) => deny(&store_error(error)),
        }
    }

    async fn list_records(
        &self,
        principal: &schemas::Principal,
        request: schemas::MemoryRecordsListRequest,
    ) -> SpokeResult<schemas::MemoryRecordsListResponse> {
        use schemas::memory_records_list_request::ReadSelection;

        let principal_id = principal_id(principal);
        if let SpokeResult::Reject(reject) =
            validate_memory_records_list(&request, policy::MAX_PAGE_SIZE_BOUND)
        {
            return deny(&ProviderError::InvalidInput {
                field: "selection".to_string(),
                reason: reject.message,
            });
        }
        let selection = match &request.selection {
            ReadSelection::SubjectAll => Selection::SubjectAll,
            ReadSelection::SharedOnly => Selection::SharedOnly,
            ReadSelection::SelectedContexts {
                context_ids,
                include_shared,
            } => Selection::SelectedContexts {
                context_ids: context_ids.clone(),
                include_shared: include_shared.unwrap_or(false),
            },
        };
        let filter: Option<Vec<String>> = request
            .filter
            .as_ref()
            .map(|filter| filter.source_context_ids.clone());
        let scope = format!(
            "{}|sel={}|filter={}|kinds={}|tombstoned={}",
            cursor_scope(
                OP_RECORDS_LIST,
                Some(request.space_id.as_str()),
                Some(request.subject_id.as_str()),
                None,
            ),
            selection_key(&selection),
            filter_key(filter.as_deref()),
            request.kinds.join(","),
            request.include_tombstoned
        );
        // §7.0.7/MEM-R026: the cursor's offset selects the SQL page; its marker
        // is verified against the snapshot the page was read in.
        let (offset, claimed) =
            match cursor_page(cursor_text(request.cursor.as_ref()), principal_id, &scope) {
                Ok(page) => page,
                Err(error) => return deny(&error),
            };
        let (rows, marker, total) = match self
            .store
            .records(
                request.space_id.as_str(),
                request.subject_id.as_str(),
                &selection,
                filter.as_deref(),
                &request.kinds,
                request.include_tombstoned,
                RecordRead::List,
                principal_id,
                Some((page_limit(request.limit), offset)),
            )
            .await
        {
            Ok(page) => page,
            Err(error) => return deny(&store_error(error)),
        };
        if let Err(error) = verify_cursor(claimed.as_deref(), &marker) {
            return deny(&error);
        }
        let consumed = offset.saturating_add(u64::try_from(rows.len()).unwrap_or(u64::MAX));
        let next = (consumed < total).then_some(consumed);
        let mut bodies = Vec::with_capacity(rows.len());
        for row in &rows {
            match record_body(row) {
                Ok(body) => bodies.push(body),
                Err(error) => return deny(&error),
            }
        }
        let next_cursor = next.map(|offset| encode_cursor(principal_id, &scope, offset, &marker));
        respond(json!({
            "records": bodies,
            "total_authorized": total,
            "next_cursor": next_cursor,
        }))
    }

    async fn promote_record(
        &self,
        principal: &schemas::Principal,
        request: schemas::MemoryRecordsPromoteRequest,
    ) -> SpokeResult<schemas::MemoryRecordsPromoteResponse> {
        let principal_id = principal_id(principal);
        let op = OP_RECORDS_PROMOTE;
        let digest = match payload_digest(op, &request) {
            Ok(digest) => digest,
            Err(error) => return deny(&error),
        };
        let ctx = OpContext {
            space_id: request.space_id.as_str(),
            requester_principal_id: principal_id,
            op,
            idempotency_key: Some(request.idempotency_key.as_str()),
            payload_digest: &digest,
        };
        let target = match &request.target_scope {
            schemas::memory_records_promote_request::PlacementScope::Shared => Placement {
                visibility: Placement::SHARED,
                context_id: None,
            },
            schemas::memory_records_promote_request::PlacementScope::Context(context_id) => {
                Placement {
                    visibility: Placement::CONTEXT,
                    context_id: Some(context_id.as_str()),
                }
            }
        };
        // §7.21: `promote` + `records_write` over BOTH placements.
        let admission = Admission {
            op,
            space_id: request.space_id.as_str(),
            principal_id,
            coverage: Coverage::Record {
                record_id: request.record_id.as_str(),
                capabilities: &["promote", "records_write"],
                target: Some(target),
            },
            issuance: None,
            subject_gate: None,
            placement_gate: None,
            freeze_gated: true,
        };
        match self
            .store
            .promote_record(
                &ctx,
                &admission,
                request.record_id.as_str(),
                expected_revision(request.expected_revision),
                target,
            )
            .await
        {
            Ok(receipt) => settle_receipt(&receipt),
            Err(error) => deny(&store_error(error)),
        }
    }

    #[allow(clippy::too_many_lines)]
    async fn recall(
        &self,
        principal: &schemas::Principal,
        request: schemas::MemoryRecallRequest,
    ) -> SpokeResult<schemas::MemoryRecallResponse> {
        use schemas::memory_recall_request::ReadSelection;

        let principal_id = principal_id(principal);
        if let SpokeResult::Reject(reject) =
            validate_memory_recall(&request, policy::MAX_PAGE_SIZE_BOUND)
        {
            return deny(&ProviderError::InvalidInput {
                field: "budget".to_string(),
                reason: reject.message,
            });
        }
        let selection = match &request.selection {
            ReadSelection::SubjectAll => Selection::SubjectAll,
            ReadSelection::SharedOnly => Selection::SharedOnly,
            ReadSelection::SelectedContexts {
                context_ids,
                include_shared,
            } => Selection::SelectedContexts {
                context_ids: context_ids.clone(),
                include_shared: include_shared.unwrap_or(false),
            },
        };
        let filter: Option<Vec<String>> = request
            .filter
            .as_ref()
            .map(|filter| filter.source_context_ids.clone());
        // §9.1: authorization + selection + filter are applied by the read path,
        // so the authorized set — never the raw table — is what gets ranked.
        let (rows, marker, total) = match self
            .store
            .records(
                request.space_id.as_str(),
                request.subject_id.as_str(),
                &selection,
                filter.as_deref(),
                &[],
                false,
                RecordRead::Recall,
                principal_id,
                None,
            )
            .await
        {
            Ok(page) => page,
            Err(error) => return deny(&store_error(error)),
        };
        // The recall budget is a byte/item bound over a Rust-ranked list.
        let total = usize::try_from(total).unwrap_or(usize::MAX);
        // §9.2: the query decides the ordering, so a cursor continues only the
        // ranking it was minted under (MEM-R026).
        let terms = policy::query_terms(request.query.as_deref().unwrap_or_default());
        let scope = format!(
            "{}|sel={}|filter={}|query={}",
            cursor_scope(
                OP_RECALL,
                Some(request.space_id.as_str()),
                Some(request.subject_id.as_str()),
                None,
            ),
            selection_key(&selection),
            filter_key(filter.as_deref()),
            terms.join(",")
        );
        let offset = match decode_cursor(
            cursor_text(request.cursor.as_ref()),
            principal_id,
            &scope,
            &marker,
        ) {
            Ok(offset) => offset,
            Err(error) => return deny(&error),
        };
        // §9.2 ranking: deterministic for identical state, request and policy.
        let mut ranked: Vec<(usize, &RecordRow)> = rows
            .iter()
            .map(|row| (policy::recall_score(&record_text(row), &terms), row))
            .collect();
        ranked.sort_by(|(left_score, left), (right_score, right)| {
            right_score
                .cmp(left_score)
                .then_with(|| right.created_at.cmp(&left.created_at))
                .then_with(|| left.record_id.cmp(&right.record_id))
        });
        // §9.3 budget over what remains after the cursor.
        let start = usize::try_from(offset).unwrap_or(total).min(total);
        let remaining = &ranked[start..];
        let sizes: Vec<(String, usize)> = remaining
            .iter()
            .map(|(_, row)| (row.record_id.clone(), row.content_json.len()))
            .collect();
        let outcome = policy::apply_budget(
            &sizes,
            request.budget.max_items.get(),
            request.budget.max_bytes.map(std::num::NonZero::get),
        );
        let mut bodies = Vec::with_capacity(outcome.kept.len());
        for index in &outcome.kept {
            let Some((_, row)) = remaining.get(*index) else {
                return deny(&ProviderError::Internal(
                    "recall budget selected an out-of-range item".to_string(),
                ));
            };
            match record_body(row) {
                Ok(body) => bodies.push(body),
                Err(error) => return deny(&error),
            }
        }
        let consumed = start + bodies.len();
        let next_cursor = (consumed < total)
            .then(|| encode_cursor(principal_id, &scope, consumed as u64, &marker));
        respond(json!({
            "records": bodies,
            "total_authorized": total,
            "truncated": consumed < total,
            "overflow_record_id": outcome.overflow_record_id,
            "next_cursor": next_cursor,
            "engine_policy_version": policy::ENGINE_POLICY_VERSION,
        }))
    }

    async fn get_profile(
        &self,
        principal: &schemas::Principal,
        request: schemas::MemoryProfileGetRequest,
    ) -> SpokeResult<schemas::MemoryProfileGetResponse> {
        let rows = match self
            .store
            .profiles(
                request.space_id.as_str(),
                request.subject_id.as_str(),
                principal_id(principal),
            )
            .await
        {
            Ok(rows) => rows,
            Err(error) => return deny(&store_error(error)),
        };
        let Some(rows) = rows else {
            return deny(&ProviderError::NotAuthorized(format!(
                "principal {} holds no profile-visible grant on subject {}",
                principal_id(principal),
                request.subject_id.as_str()
            )));
        };
        // §7.23: absent kinds are honest-empty, never invented and never an error.
        let mut bodies = Vec::new();
        for row in profile::select(rows, &request.profile_kinds) {
            match profile_body(&row) {
                Ok(body) => bodies.push(body),
                Err(error) => return deny(&error),
            }
        }
        respond(json!({ "profiles": bodies }))
    }

    async fn update_profile(
        &self,
        principal: &schemas::Principal,
        request: schemas::MemoryProfileUpdateRequest,
    ) -> SpokeResult<schemas::MemoryProfileUpdateResponse> {
        let principal_id = principal_id(principal);
        let op = OP_PROFILE_UPDATE;
        let digest = match payload_digest(op, &request) {
            Ok(digest) => digest,
            Err(error) => return deny(&error),
        };
        let subject_id = request.subject_id.as_str();
        let ctx = OpContext {
            space_id: request.space_id.as_str(),
            requester_principal_id: principal_id,
            op,
            idempotency_key: Some(request.idempotency_key.as_str()),
            payload_digest: &digest,
        };
        let requirements = [GrantRequirement {
            capability: "profile_write",
            subject_id: Some(subject_id),
            context_ids: Vec::new(),
            reads_shared: false,
            subject_all: false,
        }];
        let admission = Admission {
            op,
            space_id: request.space_id.as_str(),
            principal_id,
            coverage: Coverage::Requirements(&requirements),
            issuance: None,
            subject_gate: Some(subject_id),
            // Profiles are subject-level documents with no placement (§6.8).
            placement_gate: None,
            freeze_gated: true,
        };
        let document = match to_json(&request.document, "profile document") {
            Ok(document) => document,
            Err(error) => return deny(&error),
        };
        match self
            .store
            .upsert_profile(
                &ctx,
                &admission,
                subject_id,
                request.profile_kind.as_str(),
                request.expected_revision.map(expected_revision),
                &document,
                principal_id,
            )
            .await
        {
            Ok(receipt) => settle_receipt(&receipt),
            Err(error) => deny(&store_error(error)),
        }
    }

    async fn get_receipt(
        &self,
        principal: &schemas::Principal,
        request: schemas::MemoryReceiptsGetRequest,
    ) -> SpokeResult<schemas::MemoryReceiptsGetResponse> {
        // §7.28: the sole uncertainty-resolution channel (MEM-R010). Read-only
        // (MEM-R011), content-free (§6.10/MEM-R022), and admissible only under the
        // caller's *current* authorization (MEM-R018/R021).
        let stored = match self
            .store
            .receipt(
                principal_id(principal),
                request.space_id.as_str(),
                request.receipt_id.as_str(),
            )
            .await
        {
            Ok(stored) => stored,
            Err(error) => return deny(&store_error(error)),
        };
        pending_receipt(&stored).map_or_else(
            || match receipt_body(&stored) {
                Ok(body) => respond(json!({ "receipt": body })),
                Err(error) => deny(&error),
            },
            // §5.2 step 2: durable but not consistent yet — the channel answers
            // the pending state instead of a terminal receipt.
            |error| deny(&error),
        )
    }

    // ── Sub-capability gated (never served in N1) ──────────────────────

    async fn get_reflection_state(
        &self,
        _principal: &schemas::Principal,
        _request: schemas::MemoryReflectionsStateRequest,
    ) -> SpokeResult<schemas::MemoryReflectionsStateResponse> {
        unsupported("memory.reflections.state")
    }

    async fn list_reflections(
        &self,
        _principal: &schemas::Principal,
        _request: schemas::MemoryReflectionsListRequest,
    ) -> SpokeResult<schemas::MemoryReflectionsListResponse> {
        unsupported("memory.reflections.list")
    }

    async fn generate_reflection(
        &self,
        _principal: &schemas::Principal,
        _request: schemas::MemoryReflectionsGenerateRequest,
    ) -> SpokeResult<schemas::MemoryReflectionsGenerateResponse> {
        unsupported("memory.reflections.generate")
    }

    async fn create_export(
        &self,
        _principal: &schemas::Principal,
        _request: schemas::MemoryExportsCreateRequest,
    ) -> SpokeResult<schemas::MemoryExportsCreateResponse> {
        unsupported("memory.exports.create")
    }

    async fn get_export(
        &self,
        _principal: &schemas::Principal,
        _request: schemas::MemoryExportsGetRequest,
    ) -> SpokeResult<schemas::MemoryExportsGetResponse> {
        unsupported("memory.exports.get")
    }

    async fn stage_import(
        &self,
        _principal: &schemas::Principal,
        _request: schemas::MemoryImportsStageRequest,
    ) -> SpokeResult<schemas::MemoryImportsStageResponse> {
        unsupported("memory.imports.stage")
    }

    async fn activate_import(
        &self,
        _principal: &schemas::Principal,
        _request: schemas::MemoryImportsActivateRequest,
    ) -> SpokeResult<schemas::MemoryImportsActivateResponse> {
        unsupported("memory.imports.activate")
    }

    async fn discard_import(
        &self,
        _principal: &schemas::Principal,
        _request: schemas::MemoryImportsDiscardRequest,
    ) -> SpokeResult<schemas::MemoryImportsDiscardResponse> {
        unsupported("memory.imports.discard")
    }
}
