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
//! **Error shape.** The memory family carries its errors *inside* the op response
//! (§14: every generated `memory.*` response is a oneOf of the success payload
//! and a `MemoryErrorEnvelope` carrying `details.retry_class`). The ops-layer
//! `SpokeRejectCode` vocabulary has no `op_unsupported`/`capability_missing` at
//! all (§14.1 assigns those to the dispatch layer), so this provider expresses
//! every memory outcome — success and refusal — through the response envelope it
//! is defined to speak. A refusal is therefore never "silent success": it is the
//! protocol's typed `{ error: { code, details.retry_class, message } }`.
//!
//! **Round split.** Round A serves `memory.describe`, `memory.spaces.*`,
//! `memory.subjects.*` and `memory.grants.*`. The round-B families
//! (capture/candidates/records/recall/profile) and the sub-capability gated
//! families (reflection, transfer) refuse `op_unsupported`.

pub mod policy;
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
    grant_covers, is_freeze_allowlisted, jcs_sha256_hex, validate_grant_issuance,
    validate_memory_grants_grant, validate_memory_grants_list, validate_memory_spaces_list,
    validate_memory_subjects_fence, GrantRequirement, IssuedGrant, MemoryPort, SpokeRejectCode,
    SpokeResult,
};

use crate::error::{CoreError, CoreResult};
use storage::{
    FenceAction, FenceTarget, GrantRow, MemoryStore, NewGrant, OpContext, ReceiptRow, SpaceRow,
    StoreError, SubjectRow,
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

/// Why an op was refused, in protocol vocabulary (§14.1).
///
/// Every variant maps onto a wire `code` plus its §14.2 retry class, so the
/// mapping lives in one place and cannot drift between ops.
#[derive(Debug)]
enum ProviderError {
    NotFound(String),
    NotAuthorized(String),
    SpaceFrozen(String),
    RevisionConflict { current: i64 },
    IdempotencyConflict,
    InvalidInput { field: String, reason: String },
    StatusTransition(String),
    CursorInvalid,
    Internal(String),
}

impl ProviderError {
    const fn code(&self) -> &'static str {
        match self {
            Self::NotFound(_) => "NOT_FOUND",
            Self::NotAuthorized(_) => "NOT_AUTHORIZED",
            Self::SpaceFrozen(_) => "SPACE_FROZEN",
            Self::RevisionConflict { .. } => "REVISION_CONFLICT",
            Self::IdempotencyConflict => "IDEMPOTENCY_CONFLICT",
            Self::InvalidInput { .. } => "INVALID_INPUT",
            Self::StatusTransition(_) => "INVALID_STATUS_TRANSITION",
            Self::CursorInvalid => "CURSOR_INVALID",
            Self::Internal(_) => "INTERNAL_ERROR",
        }
    }

    /// §14.2: `INTERNAL_ERROR` is retryable only because this provider asserts no
    /// effect committed when it raises one; everything else is terminal.
    const fn retry_class(&self) -> &'static str {
        match self {
            Self::Internal(_) => "retryable",
            _ => "terminal",
        }
    }

    fn message(&self) -> String {
        match self {
            Self::NotFound(subject) => format!("{subject} not found"),
            Self::NotAuthorized(reason)
            | Self::SpaceFrozen(reason)
            | Self::StatusTransition(reason)
            | Self::Internal(reason) => reason.clone(),
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

    fn details(&self) -> Value {
        match self {
            Self::InvalidInput { field, .. } => json!({ "field": field }),
            Self::RevisionConflict { current } => json!({ "current_revision": current }),
            _ => json!({}),
        }
    }
}

fn store_error(error: StoreError) -> ProviderError {
    match error {
        StoreError::NotFound(subject) => ProviderError::NotFound(subject),
        StoreError::RevisionConflict { current } => ProviderError::RevisionConflict { current },
        StoreError::IdempotencyConflict => ProviderError::IdempotencyConflict,
        StoreError::StatusTransition(reason) => ProviderError::StatusTransition(reason),
        StoreError::Corrupt(reason) => ProviderError::Internal(reason),
        StoreError::Database(error) => ProviderError::Internal(error.to_string()),
    }
}

/// The protocol's error envelope, built from a code (§14.1) and retry class.
fn refusal<T: DeserializeOwned>(
    code: &str,
    retry_class: &str,
    message: String,
    details: Value,
) -> SpokeResult<T> {
    let mut envelope_details = match details {
        Value::Object(map) => map,
        _ => Map::new(),
    };
    envelope_details.insert(
        "retry_class".to_string(),
        Value::String(retry_class.to_string()),
    );
    let body = json!({
        "error": {
            "code": code,
            "details": envelope_details,
            "extensions": {},
            "message": message,
        }
    });
    match serde_json::from_value(body) {
        Ok(response) => spoke_operations::spoke_ok(response),
        // The envelope shape is schema-generated; a failure here means the
        // generated surface changed under this build, which must surface as a
        // host-level reject rather than a fabricated answer.
        Err(error) => spoke_operations::spoke_reject(
            SpokeRejectCode::InternalError,
            format!("memory error envelope projection: {error}"),
            None,
        ),
    }
}

fn deny<T: DeserializeOwned>(error: &ProviderError) -> SpokeResult<T> {
    refusal(
        error.code(),
        error.retry_class(),
        error.message(),
        error.details(),
    )
}

/// Round B's families and the sub-capability gated families: a typed refusal,
/// never an empty success.
fn unsupported<T: DeserializeOwned>(op: &str) -> SpokeResult<T> {
    refusal(
        "op_unsupported",
        "terminal",
        format!("{op} is not served by this provider build"),
        json!({ "op": op }),
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

/// §6.1 `payload_digest`: SHA-256 over the JCS canonicalization of the request
/// payload object. The op name is already part of the dedup namespace, so the
/// digest binds the payload only.
fn payload_digest<T: Serialize>(request: &T) -> Result<String, ProviderError> {
    let payload = serde_json::to_value(request)
        .map_err(|e| ProviderError::Internal(format!("payload encode: {e}")))?;
    match jcs_sha256_hex(&payload) {
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

/// Opaque page cursor bound to principal + listing scope + state window
/// (§7.0.7, MEM-R026). It is plain JSON: cursors are opaque strings to clients,
/// and JSON keeps the binding inspectable in tests.
#[derive(serde::Serialize, serde::Deserialize)]
struct PageCursor {
    principal: String,
    scope: String,
    offset: u64,
    snapshot: i64,
}

fn encode_cursor(principal: &str, scope: &str, offset: u64, snapshot: i64) -> String {
    let cursor = PageCursor {
        principal: principal.to_string(),
        scope: scope.to_string(),
        offset,
        snapshot,
    };
    serde_json::to_string(&cursor).expect("a page cursor always serializes")
}

fn decode_cursor(
    raw: Option<&str>,
    principal: &str,
    scope: &str,
    snapshot: i64,
) -> Result<u64, ProviderError> {
    let Some(raw) = raw else {
        return Ok(0);
    };
    let cursor: PageCursor = serde_json::from_str(raw).map_err(|_| ProviderError::CursorInvalid)?;
    if cursor.principal != principal || cursor.scope != scope || cursor.snapshot != snapshot {
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

    async fn space(&self, space_id: &str) -> Result<SpaceRow, ProviderError> {
        self.store
            .find_space(space_id)
            .await
            .map_err(store_error)?
            .ok_or_else(|| ProviderError::NotFound(format!("space {space_id}")))
    }

    /// §8.2: a frozen space admits only the read-only ops and the maintenance
    /// allowlist; everything else fails `SPACE_FROZEN` before any effect.
    fn admit_op(op: &str, space: &SpaceRow) -> Result<(), ProviderError> {
        if space.status == "frozen" && !is_freeze_allowlisted(op) {
            return Err(ProviderError::SpaceFrozen(format!(
                "space {} is frozen; {op} is outside the maintenance allowlist",
                space.space_id
            )));
        }
        Ok(())
    }

    async fn grants_in_space(&self, space_id: &str) -> Result<Vec<GrantRow>, ProviderError> {
        self.store
            .grants(space_id, None, None)
            .await
            .map_err(store_error)
    }

    /// §6.5 coverage: at least one live grant of the principal admits the op.
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
        self.require_grant(
            principal_id,
            space,
            &GrantRequirement {
                capability: "admin",
                subject_id,
                context_ids: Vec::new(),
                reads_shared: false,
                subject_all: false,
            },
        )
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

    /// §7.3/§7.4. The space lifecycle ops are not gated by their own freeze: §7.3
    /// names re-freezing an already-frozen space as a no-op success, and §8.2
    /// admits `unfreeze` explicitly. Every other mutating op goes through
    /// [`Self::admit_op`].
    async fn change_space_status<T: DeserializeOwned>(
        &self,
        principal: &schemas::Principal,
        op: &str,
        space_id: &str,
        expected_revision: i64,
        idempotency_key: Option<&str>,
        status: &str,
        request: &impl Serialize,
    ) -> SpokeResult<T> {
        let principal_id = principal_id(principal);
        let space = match self.space(space_id).await {
            Ok(space) => space,
            Err(error) => return deny(&error),
        };
        if let Err(error) = self.require_admin(principal_id, &space, None).await {
            return deny(&error);
        }
        let digest = match payload_digest(request) {
            Ok(digest) => digest,
            Err(error) => return deny(&error),
        };
        let ctx = OpContext {
            space_id: &space.space_id,
            requester_principal_id: principal_id,
            op,
            idempotency_key,
            payload_digest: &digest,
        };
        match self
            .store
            .set_space_status(&ctx, expected_revision, status, Utc::now())
            .await
        {
            Ok(receipt) => match receipt_body(&receipt) {
                Ok(body) => respond(body),
                Err(error) => deny(&error),
            },
            Err(error) => deny(&store_error(error)),
        }
    }
}

fn bootstrap_grant_id(creator_id: &str) -> String {
    format!("grt_bootstrap_{creator_id}")
}

/// Round A's error taxonomy is deliberately narrower than `CoreError`: opening
/// maps it onto the core taxonomy, while op refusals travel in the protocol
/// envelope, so only the open path needs this bridge.
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
        let spaces = match self.store.visible_spaces(principal_id, Utc::now()).await {
            Ok(spaces) => spaces,
            Err(error) => return deny(&store_error(error)),
        };
        let snapshot = i64::try_from(spaces.len()).unwrap_or(i64::MAX);
        let offset = match decode_cursor(
            request.cursor.as_deref().map(|cursor| cursor.as_str()),
            principal_id,
            "memory.spaces.list",
            snapshot,
        ) {
            Ok(offset) => offset,
            Err(error) => return deny(&error),
        };
        let (page, next) = page(spaces, page_limit(request.limit), offset);
        let bodies: Vec<Value> = page.iter().map(space_body).collect();
        let next_cursor =
            next.map(|offset| encode_cursor(principal_id, "memory.spaces.list", offset, snapshot));
        respond(json!({ "spaces": bodies, "next_cursor": next_cursor }))
    }

    async fn freeze_space(
        &self,
        principal: &schemas::Principal,
        request: schemas::MemorySpacesFreezeRequest,
    ) -> SpokeResult<schemas::MemorySpacesFreezeResponse> {
        self.change_space_status(
            principal,
            OP_FREEZE_SPACE,
            request.space_id.as_str(),
            expected_revision(request.expected_revision),
            Some(request.idempotency_key.as_str()),
            "frozen",
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
            OP_UNFREEZE_SPACE,
            request.space_id.as_str(),
            expected_revision(request.expected_revision),
            Some(request.idempotency_key.as_str()),
            "active",
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
        let space = match self.space(request.space_id.as_str()).await {
            Ok(space) => space,
            Err(error) => return deny(&error),
        };
        // Authorization first: an uncovered principal must not learn the space's
        // freeze state (§14.1 — NOT_AUTHORIZED also declines to reveal existence).
        if let Err(error) = self
            .require_grant(
                principal_id,
                &space,
                &GrantRequirement {
                    capability: "capture",
                    subject_id: Some(request.subject_id.as_str()),
                    context_ids: Vec::new(),
                    reads_shared: false,
                    subject_all: false,
                },
            )
            .await
        {
            return deny(&error);
        }
        if let Err(error) = Self::admit_op(OP_ENSURE_SUBJECT, &space) {
            return deny(&error);
        }
        let digest = match payload_digest(&request) {
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
            space_id: &space.space_id,
            requester_principal_id: principal_id,
            op: OP_ENSURE_SUBJECT,
            idempotency_key: None,
            payload_digest: &digest,
        };
        match self
            .store
            .ensure_subject(&ctx, request.subject_id.as_str(), kind, Utc::now())
            .await
        {
            Ok(receipt) => match receipt_body(&receipt) {
                Ok(body) => respond(body),
                Err(error) => deny(&error),
            },
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
        let space = match self.space(request.space_id.as_str()).await {
            Ok(space) => space,
            Err(error) => return deny(&error),
        };
        if let Err(error) = self
            .require_admin(principal_id, &space, Some(request.subject_id.as_str()))
            .await
        {
            return deny(&error);
        }
        if let Err(error) = Self::admit_op(OP_FENCE_SUBJECT, &space) {
            return deny(&error);
        }
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
        let digest = match payload_digest(&request) {
            Ok(digest) => digest,
            Err(error) => return deny(&error),
        };
        let ctx = OpContext {
            space_id: &space.space_id,
            requester_principal_id: principal_id,
            op: OP_FENCE_SUBJECT,
            idempotency_key: Some(request.idempotency_key.as_str()),
            payload_digest: &digest,
        };
        match self
            .store
            .fence_subject(
                &ctx,
                request.subject_id.as_str(),
                &target,
                action,
                expected_revision(request.expected_revision),
                Utc::now(),
            )
            .await
        {
            Ok(receipt) => match receipt_body(&receipt) {
                Ok(body) => respond(body),
                Err(error) => deny(&error),
            },
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
        let rows = match self
            .store
            .grants(&space.space_id, subject_filter, filter_principal)
            .await
        {
            Ok(rows) => rows,
            Err(error) => return deny(&store_error(error)),
        };
        let snapshot = match self.store.grant_snapshot(&space.space_id).await {
            Ok(snapshot) => snapshot,
            Err(error) => return deny(&store_error(error)),
        };
        let offset = match decode_cursor(
            request.cursor.as_deref().map(|cursor| cursor.as_str()),
            principal_id,
            "memory.grants.list",
            snapshot,
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
        let next_cursor =
            next.map(|offset| encode_cursor(principal_id, "memory.grants.list", offset, snapshot));
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
        // Both request shapes name the same facts; the restricted shape adds the
        // placement-context restriction (§6.5).
        let (
            space_id,
            grantee,
            capabilities,
            context_ids,
            shared_access,
            subject_id,
            expires_at,
            idempotency_key,
        ) = match &request {
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
            } => (
                space_id.as_str(),
                grantee.as_str(),
                capabilities.clone(),
                context_ids.clone(),
                shared_access.unwrap_or(false),
                subject_id.as_ref().map(|value| value.as_str()),
                *expires_at,
                idempotency_key.as_str(),
            ),
            schemas::MemoryGrantsGrantRequest::Variant1 {
                capabilities,
                expires_at,
                idempotency_key,
                principal_id: grantee,
                space_id,
                subject_id,
                ..
            } => (
                space_id.as_str(),
                grantee.as_str(),
                capabilities.clone(),
                Vec::new(),
                false,
                subject_id.as_ref().map(|value| value.as_str()),
                expires_at.as_ref().map(|value| **value),
                idempotency_key.as_str(),
            ),
        };
        let space = match self.space(space_id).await {
            Ok(space) => space,
            Err(error) => return deny(&error),
        };
        if let Err(error) = self.require_admin(principal_id, &space, None).await {
            return deny(&error);
        }
        if let Err(error) = Self::admit_op(OP_GRANT, &space) {
            return deny(&error);
        }
        let rows = match self.grants_in_space(&space.space_id).await {
            Ok(rows) => rows,
            Err(error) => return deny(&error),
        };
        let mut granter_grants = Vec::with_capacity(rows.len());
        for row in &rows {
            match grant_wire(row) {
                Ok(grant) => granter_grants.push(grant),
                Err(error) => return deny(&error),
            }
        }
        let capability_refs: Vec<&str> = capabilities.iter().map(String::as_str).collect();
        let issued = IssuedGrant {
            capabilities: &capability_refs,
            subject_id,
            context_ids: &context_ids,
            shared_access,
            expires_at,
        };
        if let SpokeResult::Reject(reject) = validate_grant_issuance(
            &granter_grants,
            principal_id,
            &space.space_id,
            Utc::now(),
            &issued,
        ) {
            return deny(&ProviderError::NotAuthorized(reject.message));
        }
        let digest = match payload_digest(&request) {
            Ok(digest) => digest,
            Err(error) => return deny(&error),
        };
        let grant_id = format!("grt_{}", uuid::Uuid::new_v4().simple());
        let ctx = OpContext {
            space_id: &space.space_id,
            requester_principal_id: principal_id,
            op: OP_GRANT,
            idempotency_key: Some(idempotency_key),
            payload_digest: &digest,
        };
        let new_grant = NewGrant {
            grant_id: &grant_id,
            space_id: &space.space_id,
            subject_id,
            principal_id: grantee,
            capabilities: &capabilities,
            context_ids: &context_ids,
            shared_access,
            expires_at,
        };
        match self.store.insert_grant(&ctx, &new_grant, Utc::now()).await {
            Ok(receipt) => match receipt_body(&receipt) {
                Ok(body) => respond(body),
                Err(error) => deny(&error),
            },
            Err(error) => deny(&store_error(error)),
        }
    }

    async fn revoke_grant(
        &self,
        principal: &schemas::Principal,
        request: schemas::MemoryGrantsRevokeRequest,
    ) -> SpokeResult<schemas::MemoryGrantsRevokeResponse> {
        let principal_id = principal_id(principal);
        let space = match self.space(request.space_id.as_str()).await {
            Ok(space) => space,
            Err(error) => return deny(&error),
        };
        // `memory.grants.revoke` is on the §8.2 allowlist, so an incident can be
        // contained while the space is frozen.
        if let Err(error) = self.require_admin(principal_id, &space, None).await {
            return deny(&error);
        }
        let digest = match payload_digest(&request) {
            Ok(digest) => digest,
            Err(error) => return deny(&error),
        };
        let ctx = OpContext {
            space_id: &space.space_id,
            requester_principal_id: principal_id,
            op: OP_REVOKE_GRANT,
            idempotency_key: Some(request.idempotency_key.as_str()),
            payload_digest: &digest,
        };
        match self
            .store
            .revoke_grant(
                &ctx,
                request.grant_id.as_str(),
                expected_revision(request.expected_revision),
                Utc::now(),
            )
            .await
        {
            Ok(receipt) => match receipt_body(&receipt) {
                Ok(body) => respond(body),
                Err(error) => deny(&error),
            },
            Err(error) => deny(&store_error(error)),
        }
    }

    // ── Round B ────────────────────────────────────────────────────────
    //
    // capture / candidates / records / recall / profile / receipts land with the
    // content families over `memory_pending_review` + `memory_fragments`.

    async fn capture(
        &self,
        _principal: &schemas::Principal,
        _request: schemas::MemoryCaptureRequest,
    ) -> SpokeResult<schemas::MemoryCaptureResponse> {
        unsupported("memory.capture")
    }

    async fn list_candidates(
        &self,
        _principal: &schemas::Principal,
        _request: schemas::MemoryCandidatesListRequest,
    ) -> SpokeResult<schemas::MemoryCandidatesListResponse> {
        unsupported("memory.candidates.list")
    }

    async fn get_candidate(
        &self,
        _principal: &schemas::Principal,
        _request: schemas::MemoryCandidatesGetRequest,
    ) -> SpokeResult<schemas::MemoryCandidatesGetResponse> {
        unsupported("memory.candidates.get")
    }

    async fn dismiss_candidate(
        &self,
        _principal: &schemas::Principal,
        _request: schemas::MemoryCandidatesDismissRequest,
    ) -> SpokeResult<schemas::MemoryCandidatesDismissResponse> {
        unsupported("memory.candidates.dismiss")
    }

    async fn review_candidate(
        &self,
        _principal: &schemas::Principal,
        _request: schemas::MemoryCandidatesReviewRequest,
    ) -> SpokeResult<schemas::MemoryCandidatesReviewResponse> {
        unsupported("memory.candidates.review")
    }

    async fn create_record(
        &self,
        _principal: &schemas::Principal,
        _request: schemas::MemoryRecordsCreateRequest,
    ) -> SpokeResult<schemas::MemoryRecordsCreateResponse> {
        unsupported("memory.records.create")
    }

    async fn get_record(
        &self,
        _principal: &schemas::Principal,
        _request: schemas::MemoryRecordsGetRequest,
    ) -> SpokeResult<schemas::MemoryRecordsGetResponse> {
        unsupported("memory.records.get")
    }

    async fn update_record(
        &self,
        _principal: &schemas::Principal,
        _request: schemas::MemoryRecordsUpdateRequest,
    ) -> SpokeResult<schemas::MemoryRecordsUpdateResponse> {
        unsupported("memory.records.update")
    }

    async fn delete_record(
        &self,
        _principal: &schemas::Principal,
        _request: schemas::MemoryRecordsDeleteRequest,
    ) -> SpokeResult<schemas::MemoryRecordsDeleteResponse> {
        unsupported("memory.records.delete")
    }

    async fn list_records(
        &self,
        _principal: &schemas::Principal,
        _request: schemas::MemoryRecordsListRequest,
    ) -> SpokeResult<schemas::MemoryRecordsListResponse> {
        unsupported("memory.records.list")
    }

    async fn promote_record(
        &self,
        _principal: &schemas::Principal,
        _request: schemas::MemoryRecordsPromoteRequest,
    ) -> SpokeResult<schemas::MemoryRecordsPromoteResponse> {
        unsupported("memory.records.promote")
    }

    async fn recall(
        &self,
        _principal: &schemas::Principal,
        _request: schemas::MemoryRecallRequest,
    ) -> SpokeResult<schemas::MemoryRecallResponse> {
        unsupported("memory.recall")
    }

    async fn get_profile(
        &self,
        _principal: &schemas::Principal,
        _request: schemas::MemoryProfileGetRequest,
    ) -> SpokeResult<schemas::MemoryProfileGetResponse> {
        unsupported("memory.profile.get")
    }

    async fn update_profile(
        &self,
        _principal: &schemas::Principal,
        _request: schemas::MemoryProfileUpdateRequest,
    ) -> SpokeResult<schemas::MemoryProfileUpdateResponse> {
        unsupported("memory.profile.update")
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
