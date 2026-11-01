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
//! `memory.candidates.*`, `memory.records.*`, `memory.recall` and
//! `memory.profile.*` are served. The sub-capability gated families
//! (reflection, transfer) and `memory.receipts.get` are rejected as not served.

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
    grant_covers, jcs_sha256_hex, validate_memory_candidates_list,
    validate_memory_candidates_review, validate_memory_capture, validate_memory_grants_grant,
    validate_memory_grants_list, validate_memory_recall, validate_memory_records_create,
    validate_memory_records_list, validate_memory_records_update, validate_memory_spaces_list,
    validate_memory_subjects_fence, GrantRequirement, IssuedGrant, MemoryPort, SpokeRejectCode,
    SpokeResult,
};

use crate::error::{CoreError, CoreResult};
use storage::{
    Admission, CandidateRow, Coverage, Fence, FenceAction, FenceTarget, GrantRow, MemoryStore,
    NewCandidate, NewGrant, OpContext, Placement, ProfileRow, ReceiptRow, RecordRead, RecordRow,
    ReviewDecision, Selection, SpaceRow, StoreError, SubjectRow,
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
    Internal(String),
}

impl ProviderError {
    const fn reject_code(&self) -> SpokeRejectCode {
        match self {
            Self::NotFound(_) => SpokeRejectCode::NotFound,
            Self::NotAuthorized(_) => SpokeRejectCode::NotAuthorized,
            Self::SpaceFrozen(_) => SpokeRejectCode::SpaceFrozen,
            Self::SubjectFenced { .. } => SpokeRejectCode::SubjectFenced,
            Self::RevisionConflict { .. } => SpokeRejectCode::RevisionConflict,
            Self::IdempotencyConflict => SpokeRejectCode::IdempotencyConflict,
            Self::CandidateConsumed(_) => SpokeRejectCode::CandidateConsumed,
            Self::ProfileIncomplete { .. } => SpokeRejectCode::ProfileIncomplete,
            Self::InvalidInput { .. } => SpokeRejectCode::InvalidInput,
            Self::StatusTransition(_) => SpokeRejectCode::InvalidStatusTransition,
            Self::CursorInvalid => SpokeRejectCode::CursorInvalid,
            Self::OutcomeUncertain(_) => SpokeRejectCode::OutcomeUncertain,
            Self::Internal(_) => SpokeRejectCode::InternalError,
        }
    }

    /// §14.2: `INTERNAL_ERROR` is retryable only because this provider raises it
    /// strictly before the effect commits; an uncertain outcome is its own class.
    const fn retry_class(&self) -> &'static str {
        match self {
            Self::Internal(_) => "retryable",
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
            | Self::SpaceFrozen(reason)
            | Self::StatusTransition(reason)
            | Self::CandidateConsumed(reason)
            | Self::OutcomeUncertain(reason)
            | Self::Internal(reason) => reason.clone(),
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
        StoreError::CandidateConsumed(reason) => ProviderError::CandidateConsumed(reason),
        StoreError::StatusTransition(reason) => ProviderError::StatusTransition(reason),
        StoreError::CommitUnknown(reason) => ProviderError::OutcomeUncertain(reason),
        StoreError::Corrupt(reason) => ProviderError::Internal(reason),
        StoreError::Database(error) => ProviderError::Internal(error.to_string()),
    }
}

/// A refusal is a reject, never a success value (§14.1).
fn deny<T>(error: &ProviderError) -> SpokeResult<T> {
    spoke_operations::spoke_reject(error.reject_code(), error.message(), Some(error.details()))
}

/// The sub-capability gated families (reflection, transfer) and the NXM-2
/// receipt read: the dispatch-level denial shape. A reject can only carry a
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

/// §6.10 receipt projection, as every mutating op returns it.
fn receipt_body(receipt: &ReceiptRow) -> Result<Value, ProviderError> {
    let result = match receipt.result_json.as_deref() {
        Some(raw) => serde_json::from_str::<Value>(raw)
            .map_err(|e| ProviderError::Internal(format!("receipt result: {e}")))?,
        None => Value::Null,
    };
    Ok(json!({
        "schema_version": 1,
        "receipt_id": receipt.receipt_id,
        "op": receipt.op,
        "space_id": receipt.space_id,
        "requester_principal_id": receipt.requester_principal_id,
        "idempotency_key": receipt.idempotency_key,
        "payload_digest": receipt.payload_digest,
        "status": receipt.status,
        "result": result,
        "created_at": receipt.created_at,
        "completed_at": receipt.completed_at,
        "extensions": {},
    }))
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

/// Does any live grant reach this subject at all? §7.6/§7.2 ask for "any
/// capability on the subject", which `grant_covers` cannot express (it always
/// names one required capability), so the §6.5 applicability filters are
/// applied directly.
fn any_grant_for_subject(
    grants: &[GrantRow],
    principal_id: &str,
    space_id: &str,
    subject_id: &str,
    now: DateTime<Utc>,
) -> bool {
    let now_text = storage::timestamp(now);
    grants.iter().any(|grant| {
        grant.principal_id == principal_id
            && grant.space_id == space_id
            && grant.revoked_at.is_none()
            && grant
                .expires_at
                .as_deref()
                .is_none_or(|expires_at| expires_at > now_text.as_str())
            && grant
                .subject_id
                .as_deref()
                .is_none_or(|scoped| scoped == subject_id)
    })
}

pub struct LocalMemoryProvider {
    store: MemoryStore,
    space_id: String,
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
        let store = MemoryStore::open(&db_path, creator_id)
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
            .profiles(space_id, subject_id, principal_id(principal), Utc::now())
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

    /// §14.2/§14.3 proof seam: force the next committing transaction to fail at
    /// the commit boundary (`after_commit: false` → before `COMMIT`, no effect;
    /// `true` → after `COMMIT`, effect committed). Compiled out of a production
    /// build.
    #[cfg(feature = "test-hooks")]
    #[doc(hidden)]
    pub fn inject_commit_fault(&self, after_commit: bool) {
        self.store.inject_commit_fault(after_commit);
    }

    async fn space(&self, space_id: &str) -> Result<SpaceRow, ProviderError> {
        self.store
            .find_space(space_id)
            .await
            .map_err(store_error)?
            .ok_or_else(|| ProviderError::NotFound(format!("space {space_id}")))
    }

    async fn grants_in_space(&self, space_id: &str) -> Result<Vec<GrantRow>, ProviderError> {
        self.store
            .grants(space_id, None, None)
            .await
            .map_err(store_error)
    }

    /// §6.5 coverage for a read: at least one live grant of the principal admits
    /// the op. Writes evaluate the same rule inside their effect transaction.
    async fn require_grant(
        &self,
        principal_id: &str,
        space: &SpaceRow,
        requirement: &GrantRequirement<'_>,
    ) -> Result<(), ProviderError> {
        let rows = self.grants_in_space(&space.space_id).await?;
        let now = Utc::now();
        for row in &rows {
            let grant = grant_wire(row)?;
            if grant_covers(&grant, principal_id, &space.space_id, now, requirement) {
                return Ok(());
            }
        }
        Err(ProviderError::NotAuthorized(format!(
            "principal {principal_id} holds no grant covering {} in space {}",
            requirement.capability, space.space_id
        )))
    }

    async fn require_admin(
        &self,
        principal_id: &str,
        space: &SpaceRow,
        subject_id: Option<&str>,
    ) -> Result<(), ProviderError> {
        self.require_grant(principal_id, space, &admin_requirement(subject_id))
            .await
    }

    async fn require_subject(
        &self,
        space_id: &str,
        subject_id: &str,
    ) -> Result<SubjectRow, ProviderError> {
        self.store
            .find_subject(space_id, subject_id)
            .await
            .map_err(store_error)?
            .ok_or_else(|| ProviderError::NotFound(format!("subject {subject_id}")))
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
const fn admin_requirement(subject_id: Option<&str>) -> GrantRequirement<'_> {
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
        let (spaces, marker) = match self.store.visible_spaces(principal_id, Utc::now()).await {
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
        let space = match self.space(request.space_id.as_str()).await {
            Ok(space) => space,
            Err(error) => return deny(&error),
        };
        let rows = match self.grants_in_space(&space.space_id).await {
            Ok(rows) => rows,
            Err(error) => return deny(&error),
        };
        if !any_grant_for_subject(
            &rows,
            principal_id,
            &space.space_id,
            request.subject_id.as_str(),
            Utc::now(),
        ) {
            return deny(&ProviderError::NotAuthorized(format!(
                "principal {principal_id} holds no grant on subject {}",
                request.subject_id.as_str()
            )));
        }
        match self
            .require_subject(&space.space_id, request.subject_id.as_str())
            .await
        {
            Ok(subject) => match subject_body(&subject) {
                Ok(body) => respond(json!({ "subject": body })),
                Err(error) => deny(&error),
            },
            Err(error) => deny(&error),
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
        let space = match self.space(request.space_id.as_str()).await {
            Ok(space) => space,
            Err(error) => return deny(&error),
        };
        let admin = self
            .require_admin(
                principal_id,
                &space,
                request.subject_id.as_ref().map(|value| value.as_str()),
            )
            .await
            .is_ok();
        if !admin && request.principal_id.is_some() {
            return deny(&ProviderError::NotAuthorized(
                "the principal_id filter is admin-only (§7.8)".to_string(),
            ));
        }
        let principal_filter = request.principal_id.as_ref().map(|value| value.as_str());
        let subject_filter = request.subject_id.as_ref().map(|value| value.as_str());
        let filter_principal = if admin {
            principal_filter
        } else {
            Some(principal_id)
        };
        // §7.0.7: the page and its invalidation marker are one read, and the
        // cursor binds every filter that selected them.
        let (rows, marker) = match self
            .store
            .grants_page(&space.space_id, subject_filter, filter_principal)
            .await
        {
            Ok(page) => page,
            Err(error) => return deny(&store_error(error)),
        };
        let scope = cursor_scope(
            "memory.grants.list",
            Some(space.space_id.as_str()),
            subject_filter,
            filter_principal,
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
        let (page, next) = page(rows, page_limit(request.limit), offset);
        let mut bodies = Vec::with_capacity(page.len());
        for row in &page {
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
        let (rows, marker) = match self
            .store
            .candidates(
                request.space_id.as_str(),
                request.subject_id.as_str(),
                status.as_deref(),
                principal_id,
                Utc::now(),
            )
            .await
        {
            Ok(page) => page,
            Err(error) => return deny(&store_error(error)),
        };
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
        let offset = match decode_cursor(
            cursor_text(request.cursor.as_ref()),
            principal_id,
            &scope,
            &marker,
        ) {
            Ok(offset) => offset,
            Err(error) => return deny(&error),
        };
        let (page, next) = page(rows, page_limit(request.limit), offset);
        let mut bodies = Vec::with_capacity(page.len());
        for row in &page {
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
                Utc::now(),
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
                Utc::now(),
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
        let (rows, marker) = match self
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
                Utc::now(),
            )
            .await
        {
            Ok(page) => page,
            Err(error) => return deny(&store_error(error)),
        };
        let total = rows.len();
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
        let offset = match decode_cursor(
            cursor_text(request.cursor.as_ref()),
            principal_id,
            &scope,
            &marker,
        ) {
            Ok(offset) => offset,
            Err(error) => return deny(&error),
        };
        let (page, next) = page(rows, page_limit(request.limit), offset);
        let mut bodies = Vec::with_capacity(page.len());
        for row in &page {
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
        let (rows, marker) = match self
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
                Utc::now(),
            )
            .await
        {
            Ok(page) => page,
            Err(error) => return deny(&store_error(error)),
        };
        let total = rows.len();
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
                Utc::now(),
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
        _principal: &schemas::Principal,
        _request: schemas::MemoryReceiptsGetRequest,
    ) -> SpokeResult<schemas::MemoryReceiptsGetResponse> {
        // NXM-2 owns the receipt read channel.
        unsupported("memory.receipts.get")
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
