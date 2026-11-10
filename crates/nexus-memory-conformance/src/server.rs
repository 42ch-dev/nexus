//! Full `MemoryPort` assembly, the signed memory service and its admission.
//!
//! Every one of the 25 base operations delegates to the private SQLite
//! implementation the earlier slices own — this module adds no base-operation
//! semantics of its own. What it does own is the protocol frame around them:
//!
//! - **One transaction per mutation.** A mutating op resolves replay, runs the
//!   private `*_in_tx` effect and stages its content-free receipt, dedup row and
//!   receipt target on a single `BEGIN IMMEDIATE` transaction (MEM-R009/R021),
//!   in that order: current authorization, then replay, then the effect with its
//!   superseded CAS/fence gates. Nothing wraps an already-committed convenience
//!   method in a second receipt transaction.
//! - **The D15 request scope.** The served request's exact digest arrives from
//!   [`crate::raw_invoke`]; every mutation takes it with
//!   [`raw_invoke::admitted`](crate::raw_invoke::admitted) and fails closed when
//!   the scope is missing or names another op or provider.
//! - **The real target.** Each receipt target is built from the operation's
//!   actual subject/placement/selection facets — never inferred from the op
//!   string or from whichever grant survives — so a receipt body is only
//!   disclosable to a principal whose current grant still covers what the
//!   operation acted on.
//! - **The wire envelope.** Provider refusals gain the §14.2
//!   `details.retry_class` at the service boundary without losing their code,
//!   message or existing details.
//!
//! The reflection family is served (advertised exactly when the provider is
//! constructed advertised); the optional transfer family is not served: its
//! methods answer the SDK's terminal capability refusal and `memory.describe`
//! never advertises it.

use std::num::NonZeroU64;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::de::DeserializeOwned;
use serde_json::{json, Map, Value};
use spoke_connect::remote::{MemoryAdmission, MemoryPortService, RemoteMemoryService};
use spoke_operations::spoke_schemas::memory_records_create_request::PlacementScope as CreateScope;
use spoke_operations::spoke_schemas::memory_records_promote_request::PlacementScope as PromoteScope;
use spoke_operations::spoke_schemas::memory_subjects_fence_request::Target as FenceTarget;
use spoke_operations::spoke_schemas::{
    MemoryCandidatesDismissRequest, MemoryCandidatesDismissResponse, MemoryCandidatesGetRequest,
    MemoryCandidatesGetResponse, MemoryCandidatesListRequest, MemoryCandidatesListResponse,
    MemoryCandidatesReviewRequest, MemoryCandidatesReviewResponse, MemoryCaptureRequest,
    MemoryCaptureResponse, MemoryDescribeRequest, MemoryDescribeResponse,
    MemoryExportsCreateRequest, MemoryExportsCreateResponse, MemoryExportsGetRequest,
    MemoryExportsGetResponse, MemoryGrant, MemoryGrantsGrantRequest, MemoryGrantsGrantResponse,
    MemoryGrantsListRequest, MemoryGrantsListResponse, MemoryGrantsRevokeRequest,
    MemoryGrantsRevokeResponse, MemoryImportsActivateRequest, MemoryImportsActivateResponse,
    MemoryImportsDiscardRequest, MemoryImportsDiscardResponse, MemoryImportsStageRequest,
    MemoryImportsStageResponse, MemoryProfileGetRequest, MemoryProfileGetResponse,
    MemoryProfileUpdateRequest, MemoryProfileUpdateResponse, MemoryRecallRequest,
    MemoryRecallResponse, MemoryReceipt, MemoryReceiptsGetRequest, MemoryReceiptsGetResponse,
    MemoryRecordsCreateRequest, MemoryRecordsCreateResponse, MemoryRecordsDeleteRequest,
    MemoryRecordsDeleteResponse, MemoryRecordsGetRequest, MemoryRecordsGetResponse,
    MemoryRecordsListRequest, MemoryRecordsListResponse, MemoryRecordsPromoteRequest,
    MemoryRecordsPromoteResponse, MemoryRecordsUpdateRequest, MemoryRecordsUpdateResponse,
    MemoryReflectionsGenerateRequest, MemoryReflectionsGenerateResponse,
    MemoryReflectionsListRequest, MemoryReflectionsListResponse, MemoryReflectionsStateRequest,
    MemoryReflectionsStateResponse, MemorySpacesFreezeRequest, MemorySpacesFreezeResponse,
    MemorySpacesListRequest, MemorySpacesListResponse, MemorySpacesUnfreezeRequest,
    MemorySpacesUnfreezeResponse, MemorySubjectsEnsureRequest, MemorySubjectsEnsureResponse,
    MemorySubjectsFenceRequest, MemorySubjectsFenceResponse, MemorySubjectsGetRequest,
    MemorySubjectsGetResponse, Principal,
};
use spoke_operations::{
    grant_covers, GrantRequirement, MemoryPort, SpokeReject, SpokeRejectCode, SpokeResult,
};
use sqlx::{Row, SqliteConnection};

use crate::candidates::{
    receipt_id, release_expired_binding_in_tx, required_capabilities, resolve_replay,
    stage_receipt_in_tx, Admitted, Mutation, ReceiptState, ReceiptTarget, ReplayResolution,
};
use crate::policy::describe_body;
use crate::provider::FixtureMemoryProvider;
use crate::raw_invoke;
use crate::spaces::{internal, internal_failure, load_grants};

impl FixtureMemoryProvider {
    /// The signed Connect memory service for this provider: the unchanged
    /// [`MemoryPortService`] mapping wrapped by the D15 request-scope decorator.
    ///
    /// `admission` is the operator's authenticated peer→principal table (see
    /// [`crate::policy::PeerDirectory`]); the payload never supplies the
    /// requester.
    #[must_use]
    pub fn server(self: &Arc<Self>, admission: MemoryAdmission) -> Arc<dyn RemoteMemoryService> {
        let port: Arc<dyn MemoryPort + Send + Sync> = self.clone();
        Arc::new(FixtureMemoryService {
            inner: MemoryPortService::new(port, admission),
            provider: self.identity.authority_host_id.clone(),
        })
    }
}

/// The D15 decorator: compute the exact received-object digest once, move the
/// same request object into the unchanged SDK service, and normalize the
/// answer's error envelope.
struct FixtureMemoryService {
    inner: MemoryPortService,
    /// The provider identity a request scope must name to be trusted.
    provider: String,
}

#[async_trait]
impl RemoteMemoryService for FixtureMemoryService {
    async fn invoke(&self, peer_id: &str, op: &str, payload: Value) -> SpokeResult<Value> {
        let scope = match raw_invoke::admission(&payload, op, &self.provider) {
            Ok(scope) => scope,
            // The digest could not be taken over the received object, so no
            // typed decode may run: refuse the request instead of admitting it
            // without its replay identity.
            Err(reject) => return raw_invoke::normalize(SpokeResult::Reject(reject)),
        };
        let result = match scope {
            Some(scope) => {
                // `payload` is moved into the service: the digest input and the
                // executed request are the same object, never a copy.
                raw_invoke::with_scope(scope, self.inner.invoke(peer_id, op, payload)).await
            }
            None => self.inner.invoke(peer_id, op, payload).await,
        };
        raw_invoke::normalize(result)
    }
}

// ---------------------------------------------------------------------------
// Response plumbing
// ---------------------------------------------------------------------------

fn respond<T: DeserializeOwned>(body: Value) -> SpokeResult<T> {
    match serde_json::from_value(body) {
        Ok(value) => SpokeResult::Ok(value),
        Err(error) => SpokeResult::Reject(internal_failure(error)),
    }
}

/// A mutation's wire answer is its content-free receipt (§7.0.2).
fn respond_receipt<T: DeserializeOwned>(receipt: &MemoryReceipt) -> SpokeResult<T> {
    match serde_json::to_value(receipt) {
        Ok(body) => respond(body),
        Err(error) => SpokeResult::Reject(internal_failure(error)),
    }
}

/// A commit that failed may still have taken effect (§14.3): the caller cannot
/// be told whether anything committed, so it is `OUTCOME_UNCERTAIN` — never a
/// clean `retryable` internal error, and never a driver message on the wire.
fn commit_outcome_unknown() -> SpokeReject {
    SpokeReject {
        code: SpokeRejectCode::OutcomeUncertain,
        message: "the transaction's commit state is unknown; resolve with the same idempotency \
                  key or memory.receipts.get"
            .to_owned(),
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

fn non_zero(value: u64) -> NonZeroU64 {
    NonZeroU64::new(value).unwrap_or(NonZeroU64::MIN)
}

/// The §14.2 terminal refusal for an op whose capability family this provider
/// does not serve (mirrors the local provider's published refusal shape).
fn unsupported<T>(op: &str) -> SpokeResult<T> {
    let mut details = Map::new();
    details.insert("retry_class".to_owned(), json!("terminal"));
    details.insert("op".to_owned(), json!(op));
    SpokeResult::Reject(SpokeReject {
        code: SpokeRejectCode::CapabilityPortMissing,
        message: format!("{op} is not served by this provider"),
        details: Some(details),
    })
}

// ---------------------------------------------------------------------------
// Mutation frame: target, authorization-before-replay, one-transaction effect
// ---------------------------------------------------------------------------

/// The content-free requirement one mutation had to satisfy: the published
/// capability list plus the subject, placement-context, shared-visibility and
/// whole-selection axes.
///
/// One value serves both duties the operation needs: the receipt target staged
/// with the effect and the current-authorization check that must run before
/// replay (MEM-R018/R021 — a revoked principal neither replays nor reads). Both
/// evaluate `grant_covers` on a single grant, so no capability is satisfied by
/// combining grants and no second policy exists here.
struct OpTarget {
    capabilities: &'static [&'static str],
    subject_id: Option<String>,
    context_ids: Vec<String>,
    reads_shared: bool,
    subject_all: bool,
}

impl OpTarget {
    fn new(
        capabilities: &'static [&'static str],
        subject_id: Option<&str>,
        context_ids: &[&str],
        reads_shared: bool,
        subject_all: bool,
    ) -> Self {
        Self {
            capabilities,
            subject_id: subject_id.map(str::to_owned),
            context_ids: context_ids.iter().map(|id| (*id).to_owned()).collect(),
            reads_shared,
            subject_all,
        }
    }

    /// A space-wide requirement: no subject, no named context and the
    /// unrestricted-coverage facet, so a narrow grant never covers it.
    fn whole_space(capabilities: &'static [&'static str]) -> Self {
        Self::new(capabilities, None, &[], false, true)
    }

    /// A requirement over the record placement named by the operation.
    fn placement(
        capabilities: &'static [&'static str],
        subject_id: &str,
        visibility: &str,
        context_id: Option<&str>,
    ) -> Self {
        match visibility {
            "context" => Self::new(
                capabilities,
                Some(subject_id),
                &[context_id.unwrap_or_default()],
                false,
                false,
            ),
            _ => Self::new(capabilities, Some(subject_id), &[], true, false),
        }
    }

    fn receipt(&self) -> Result<ReceiptTarget, SpokeReject> {
        let contexts: Vec<&str> = self.context_ids.iter().map(String::as_str).collect();
        ReceiptTarget::new(
            self.capabilities,
            self.subject_id.as_deref(),
            &contexts,
            self.reads_shared,
            self.subject_all,
        )
    }

    /// Whether one current grant carries every required capability over the
    /// recorded facets (MEM-R006: grants never union).
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

/// The published §7 capability list of one mutating op.
fn capabilities_of(op: &str) -> Result<&'static [&'static str], SpokeReject> {
    required_capabilities(op)
        .ok_or_else(|| internal(format!("{op} has no published capability list")))
}

/// How the precheck refuses when the caller's current grants do not cover the
/// operation's target.
///
/// By-id record refusals stay indistinguishable from a missing id, exactly as
/// the records slice answers, so introducing the precheck cannot turn an
/// authorization failure into an existence oracle.
#[derive(Clone, Copy)]
enum PrecheckRefusal {
    /// The operation's own authorization refusal.
    Target,
    /// The records slice's single by-id refusal.
    RecordAbsent,
}

impl PrecheckRefusal {
    fn reject(self) -> SpokeReject {
        match self {
            Self::Target => not_authorized(
                "the authenticated principal holds no current grant covering this operation's target",
            ),
            // Mirrors `records::record_absent` so a foreign record and a missing
            // one answer one indistinguishable refusal.
            Self::RecordAbsent => SpokeReject {
                code: SpokeRejectCode::NotFound,
                message: "the requested record does not exist in this space".to_owned(),
                details: None,
            },
        }
    }
}

impl FixtureMemoryProvider {
    /// Execute the revoke and answer with its receipt: the effect, its dedup row
    /// and the receipt share this one transaction, and a commit that failed keeps
    /// the honest unknown outcome.
    async fn revoke_effect(
        &self,
        mut transaction: sqlx::Transaction<'_, sqlx::Sqlite>,
        principal_id: &str,
        space_id: &str,
        admitted: &Admitted<MemoryGrantsRevokeRequest>,
        target: &OpTarget,
        now: DateTime<Utc>,
    ) -> SpokeResult<MemoryGrantsRevokeResponse> {
        let op = "memory.grants.revoke";
        let idempotency_key = admitted.request().idempotency_key.as_str().to_owned();
        let revoked = match self
            .grants_revoke_in_tx(&mut transaction, principal_id, admitted.request(), now)
            .await
        {
            Ok(revoked) => revoked,
            Err(reject) => return SpokeResult::Reject(reject),
        };
        let result = json!({
            "grant_id": revoked.grant_id,
            "revoked": revoked.revoked,
            "revision": non_zero(revoked.revision),
        });
        let receipt = match Self::stage_committed(
            &mut transaction,
            space_id,
            principal_id,
            op,
            &idempotency_key,
            admitted,
            target,
            result,
            now,
        )
        .await
        {
            Ok(receipt) => receipt,
            Err(reject) => return SpokeResult::Reject(reject),
        };
        if transaction.commit().await.is_err() {
            return SpokeResult::Reject(commit_outcome_unknown());
        }
        respond_receipt(&receipt)
    }

    /// The grant a revoke names, as the receipt target of the grant's own
    /// facets, or the revoke slice's own refusal when no such row exists.
    ///
    /// A live admin is checked first so a non-admin never learns whether an id
    /// exists; an unserved space is the producer's `NOT_FOUND`, not a grant
    /// decision (§7.19).
    async fn revoke_target(
        &self,
        tx: &mut SqliteConnection,
        principal_id: &str,
        space_id: &str,
        grant_id: &str,
        op: &str,
        now: DateTime<Utc>,
    ) -> Result<Option<OpTarget>, SpokeReject> {
        self.require_served_space(space_id)?;
        let grants = load_grants(tx, space_id).await?;
        if !has_live_admin(&grants, principal_id, space_id, now) {
            return Err(not_authorized(
                "revocation requires an admin grant in this space",
            ));
        }
        grants
            .iter()
            .find(|grant| grant_id_of(grant) == grant_id)
            .map(grant_facets)
            .map(|target| capabilities_of(op).map(|caps| target.with_capabilities(caps)))
            .transpose()
    }

    /// Answer a revoke that names no grant row through the revoke slice itself,
    /// discarding the transaction: the slice owns the refusal vocabulary
    /// (`NOT_FOUND` for a whole-space admin, one indistinguishable refusal
    /// otherwise).
    async fn revoke_unknown_grant(
        &self,
        mut transaction: sqlx::Transaction<'_, sqlx::Sqlite>,
        principal_id: &str,
        request: &MemoryGrantsRevokeRequest,
        now: DateTime<Utc>,
    ) -> SpokeResult<MemoryGrantsRevokeResponse> {
        let outcome = self
            .grants_revoke_in_tx(&mut transaction, principal_id, request, now)
            .await;
        let _ = transaction.rollback().await;
        match outcome {
            Err(reject) => SpokeResult::Reject(reject),
            Ok(_) => SpokeResult::Reject(internal("a grant revoke succeeded without a grant row")),
        }
    }
}

/// §7.21: the receipt target of a promotion covers *both* placements — the
/// union of the source and target contexts, the shared facet of either side,
/// and both the `promote` and `records_write` capabilities.
fn promote_target(
    subject_id: &str,
    source_visibility: &str,
    source_context: Option<&str>,
    target: &PromoteScope,
) -> Result<OpTarget, SpokeReject> {
    // §7.21 authorizes one grant over *both* placements: the receipt target
    // records the union of the source and target contexts, the shared facet
    // of either side, and both required capabilities.
    let (target_visibility, target_context) = match target {
        PromoteScope::Shared => ("shared", None),
        PromoteScope::Context(context) => ("context", Some(context.as_str())),
    };
    let mut contexts: Vec<String> = Vec::new();
    for context in [source_context, target_context].into_iter().flatten() {
        if !contexts.iter().any(|known| known == context) {
            contexts.push(context.to_owned());
        }
    }
    let context_refs: Vec<&str> = contexts.iter().map(String::as_str).collect();
    let capabilities = capabilities_of("memory.records.promote")?;
    Ok(OpTarget::new(
        capabilities,
        Some(subject_id),
        &context_refs,
        source_visibility == "shared" || target_visibility == "shared",
        false,
    ))
}

/// What the replay step decided for this admitted mutation.
enum ReplayStep {
    /// No binding, or a stale one already released inside this transaction.
    Execute,
    /// The identical admitted replay: return the stored receipt, no effect.
    Replayed(Box<MemoryReceipt>),
}

impl FixtureMemoryProvider {
    /// The provider identity a request scope must name.
    fn scope_provider(&self) -> &str {
        &self.identity.authority_host_id
    }

    /// Take this op's exact received-object digest from the D15 scope.
    fn admitted<Request>(
        &self,
        op: &str,
        request: Request,
    ) -> Result<Admitted<Request>, SpokeReject> {
        raw_invoke::admitted(op, self.scope_provider(), request)
    }

    /// Current authorization, then replay, then (for a fresh request) the
    /// caller's effect — all on the caller's transaction.
    ///
    /// Authorization runs first so a principal that lost coverage can neither
    /// replay nor read its old receipt; replay runs before the effect so a
    /// committed outcome is never re-executed against superseded CAS/fence
    /// preconditions; a key whose whole lifetime ended is released inside this
    /// transaction, so a later refusal rolls that release back with it.
    #[allow(clippy::too_many_arguments)]
    async fn replay_step<Request: Sync>(
        &self,
        conn: &mut SqliteConnection,
        principal_id: &str,
        space_id: &str,
        op: &'static str,
        idempotency_key: &str,
        admitted: &Admitted<Request>,
        target: &OpTarget,
        refusal: PrecheckRefusal,
        now: DateTime<Utc>,
    ) -> Result<ReplayStep, SpokeReject> {
        // The producer's own served-space refusal comes first: an unserved
        // space is NOT_FOUND, never a grant decision.
        self.require_served_space(space_id)?;
        let mutation = Mutation::keyed(op, idempotency_key, admitted);
        match resolve_replay(conn, space_id, principal_id, &mutation, now).await? {
            // An existing binding authorized against its *retained* target
            // inside `resolve_replay`, in this same transaction. The operation's
            // current entity is irrelevant to it and is never re-derived here.
            ReplayResolution::Replayed(receipt) => Ok(ReplayStep::Replayed(receipt)),
            ReplayResolution::KeyReusable { receipt_id } => {
                release_expired_binding_in_tx(conn, space_id, principal_id, &mutation, &receipt_id)
                    .await?;
                // A released key is a fresh execution: the current target must
                // authorize it before the effect.
                self.require_target_authority(conn, principal_id, space_id, target, refusal, now)
                    .await?;
                Ok(ReplayStep::Execute)
            }
            ReplayResolution::Fresh => {
                self.require_target_authority(conn, principal_id, space_id, target, refusal, now)
                    .await?;
                Ok(ReplayStep::Execute)
            }
        }
    }

    /// The operation's current authority for a genuinely fresh execution.
    async fn require_target_authority(
        &self,
        conn: &mut SqliteConnection,
        principal_id: &str,
        space_id: &str,
        target: &OpTarget,
        refusal: PrecheckRefusal,
        now: DateTime<Utc>,
    ) -> Result<(), SpokeReject> {
        let grants = load_grants(conn, space_id).await?;
        if target.admits(&grants, principal_id, space_id, now) {
            Ok(())
        } else {
            Err(refusal.reject())
        }
    }

    /// Stage one committed receipt, its dedup row and its target on the
    /// caller's transaction, so they commit with the effect exactly once.
    #[allow(clippy::too_many_arguments)]
    async fn stage_committed<Request: Sync>(
        conn: &mut SqliteConnection,
        space_id: &str,
        principal_id: &str,
        op: &'static str,
        idempotency_key: &str,
        admitted: &Admitted<Request>,
        target: &OpTarget,
        result: Value,
        now: DateTime<Utc>,
    ) -> Result<MemoryReceipt, SpokeReject> {
        let mutation = Mutation::keyed(op, idempotency_key, admitted);
        stage_receipt_in_tx(
            conn,
            space_id,
            principal_id,
            &mutation,
            &receipt_id(),
            &target.receipt()?,
            ReceiptState::Committed(result),
            now,
        )
        .await
    }

    /// Open the one writer transaction a mutation owns.
    pub(crate) async fn begin_write(
        &self,
    ) -> Result<sqlx::Transaction<'_, sqlx::Sqlite>, SpokeReject> {
        self.store()
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(internal_failure)
    }

    /// Content-free placement of one stored record (`subject_id`,
    /// `scope_visibility`, `scope_context_id`), read for the receipt target of
    /// the record mutations whose requirement covers the *current* placement.
    ///
    /// The record table and its row structures stay private to the records
    /// slice, whose published storage shape the recall slice reads as well; this
    /// authorizes nothing and only names the placement the operation applied to.
    /// A missing row yields `None`, and the operation itself then answers its own
    /// uniform refusal.
    async fn record_placement(
        &self,
        conn: &mut SqliteConnection,
        space_id: &str,
        record_id: &str,
    ) -> Result<Option<(String, String, Option<String>)>, SpokeReject> {
        let row = sqlx::query(
            "SELECT subject_id, scope_visibility, scope_context_id FROM records \
             WHERE space_id = ? AND record_id = ?",
        )
        .bind(space_id)
        .bind(record_id)
        .fetch_optional(conn)
        .await
        .map_err(internal_failure)?;
        row.map(|row| {
            Ok((
                row.try_get("subject_id").map_err(internal_failure)?,
                row.try_get("scope_visibility").map_err(internal_failure)?,
                row.try_get("scope_context_id").map_err(internal_failure)?,
            ))
        })
        .transpose()
    }
}

// ---------------------------------------------------------------------------
// The 25 base operations (§7.1–§7.28)
// ---------------------------------------------------------------------------

#[async_trait]
impl MemoryPort for FixtureMemoryProvider {
    async fn describe(
        &self,
        _principal: &Principal,
        _request: MemoryDescribeRequest,
    ) -> SpokeResult<MemoryDescribeResponse> {
        // §7.1: no grant is required, only authenticated admission — which the
        // service face already performed. The capability list is honest for
        // this provider's construction shape.
        respond(describe_body(self.advertises_reflection()))
    }

    async fn list_spaces(
        &self,
        principal: &Principal,
        request: MemorySpacesListRequest,
    ) -> SpokeResult<MemorySpacesListResponse> {
        match self
            .spaces_list(principal_id(principal), &request, Utc::now())
            .await
        {
            Ok(page) => respond(json!({
                "spaces": page.spaces,
                "next_cursor": page.next_cursor,
            })),
            Err(reject) => SpokeResult::Reject(reject),
        }
    }

    async fn freeze_space(
        &self,
        principal: &Principal,
        request: MemorySpacesFreezeRequest,
    ) -> SpokeResult<MemorySpacesFreezeResponse> {
        let op = "memory.spaces.freeze";
        let principal_id = principal_id(principal);
        let idempotency_key = request.idempotency_key.as_str().to_owned();
        let space_id = request.space_id.as_str().to_owned();
        let admitted = match self.admitted(op, request) {
            Ok(admitted) => admitted,
            Err(reject) => return SpokeResult::Reject(reject),
        };
        let now = Utc::now();
        let target = match capabilities_of(op) {
            Ok(capabilities) => OpTarget::whole_space(capabilities),
            Err(reject) => return SpokeResult::Reject(reject),
        };
        let mut transaction = match self.begin_write().await {
            Ok(transaction) => transaction,
            Err(reject) => return SpokeResult::Reject(reject),
        };
        match Self::replay_step(
            self,
            &mut transaction,
            principal_id,
            &space_id,
            op,
            &idempotency_key,
            &admitted,
            &target,
            PrecheckRefusal::Target,
            now,
        )
        .await
        {
            Ok(ReplayStep::Replayed(receipt)) => {
                let _ = transaction.rollback().await;
                return respond_receipt(&receipt);
            }
            Ok(ReplayStep::Execute) => {}
            Err(reject) => return SpokeResult::Reject(reject),
        }
        let change = match self
            .spaces_freeze_in_tx(&mut transaction, principal_id, admitted.request(), now)
            .await
        {
            Ok(change) => change,
            Err(reject) => return SpokeResult::Reject(reject),
        };
        let result = json!({
            "status": change.status,
            "pending_receipts": change.pending_receipts,
            "revision": non_zero(change.revision),
        });
        let receipt = match Self::stage_committed(
            &mut transaction,
            &space_id,
            principal_id,
            op,
            &idempotency_key,
            &admitted,
            &target,
            result,
            now,
        )
        .await
        {
            Ok(receipt) => receipt,
            Err(reject) => return SpokeResult::Reject(reject),
        };
        if transaction.commit().await.is_err() {
            return SpokeResult::Reject(commit_outcome_unknown());
        }
        respond_receipt(&receipt)
    }

    async fn unfreeze_space(
        &self,
        principal: &Principal,
        request: MemorySpacesUnfreezeRequest,
    ) -> SpokeResult<MemorySpacesUnfreezeResponse> {
        let op = "memory.spaces.unfreeze";
        let principal_id = principal_id(principal);
        let idempotency_key = request.idempotency_key.as_str().to_owned();
        let space_id = request.space_id.as_str().to_owned();
        let admitted = match self.admitted(op, request) {
            Ok(admitted) => admitted,
            Err(reject) => return SpokeResult::Reject(reject),
        };
        let now = Utc::now();
        let target = match capabilities_of(op) {
            Ok(capabilities) => OpTarget::whole_space(capabilities),
            Err(reject) => return SpokeResult::Reject(reject),
        };
        let mut transaction = match self.begin_write().await {
            Ok(transaction) => transaction,
            Err(reject) => return SpokeResult::Reject(reject),
        };
        match Self::replay_step(
            self,
            &mut transaction,
            principal_id,
            &space_id,
            op,
            &idempotency_key,
            &admitted,
            &target,
            PrecheckRefusal::Target,
            now,
        )
        .await
        {
            Ok(ReplayStep::Replayed(receipt)) => {
                let _ = transaction.rollback().await;
                return respond_receipt(&receipt);
            }
            Ok(ReplayStep::Execute) => {}
            Err(reject) => return SpokeResult::Reject(reject),
        }
        let change = match self
            .spaces_unfreeze_in_tx(&mut transaction, principal_id, admitted.request(), now)
            .await
        {
            Ok(change) => change,
            Err(reject) => return SpokeResult::Reject(reject),
        };
        let result = json!({
            "status": change.status,
            "pending_receipts": change.pending_receipts,
            "revision": non_zero(change.revision),
        });
        let receipt = match Self::stage_committed(
            &mut transaction,
            &space_id,
            principal_id,
            op,
            &idempotency_key,
            &admitted,
            &target,
            result,
            now,
        )
        .await
        {
            Ok(receipt) => receipt,
            Err(reject) => return SpokeResult::Reject(reject),
        };
        if transaction.commit().await.is_err() {
            return SpokeResult::Reject(commit_outcome_unknown());
        }
        respond_receipt(&receipt)
    }

    async fn ensure_subject(
        &self,
        principal: &Principal,
        request: MemorySubjectsEnsureRequest,
    ) -> SpokeResult<MemorySubjectsEnsureResponse> {
        // §7.0.5's sole keyless mutation: no dedup namespace row exists, so a
        // keyless receipt records this outcome and can never be replayed.
        let op = "memory.subjects.ensure";
        let principal_id = principal_id(principal);
        let space_id = request.space_id.as_str().to_owned();
        let subject_id = request.subject_id.as_str().to_owned();
        let admitted = match self.admitted(op, request) {
            Ok(admitted) => admitted,
            Err(reject) => return SpokeResult::Reject(reject),
        };
        let now = Utc::now();
        let target = match capabilities_of(op) {
            Ok(capabilities) => OpTarget::new(capabilities, Some(&subject_id), &[], false, false),
            Err(reject) => return SpokeResult::Reject(reject),
        };
        let mut transaction = match self.begin_write().await {
            Ok(transaction) => transaction,
            Err(reject) => return SpokeResult::Reject(reject),
        };
        let change = match self
            .subjects_ensure_in_tx(&mut transaction, principal_id, admitted.request(), now)
            .await
        {
            Ok(change) => change,
            Err(reject) => return SpokeResult::Reject(reject),
        };
        let result = json!({
            "subject_id": change.subject_id,
            "created": change.created,
            "status": change.status,
            "revision": non_zero(change.revision),
        });
        let mutation = Mutation::keyless(op, &admitted);
        let receipt_target = match target.receipt() {
            Ok(receipt_target) => receipt_target,
            Err(reject) => return SpokeResult::Reject(reject),
        };
        let receipt = match stage_receipt_in_tx(
            &mut transaction,
            &space_id,
            principal_id,
            &mutation,
            &receipt_id(),
            &receipt_target,
            ReceiptState::Committed(result),
            now,
        )
        .await
        {
            Ok(receipt) => receipt,
            Err(reject) => return SpokeResult::Reject(reject),
        };
        if transaction.commit().await.is_err() {
            return SpokeResult::Reject(commit_outcome_unknown());
        }
        respond_receipt(&receipt)
    }

    async fn get_subject(
        &self,
        principal: &Principal,
        request: MemorySubjectsGetRequest,
    ) -> SpokeResult<MemorySubjectsGetResponse> {
        match self
            .subjects_get(principal_id(principal), &request, Utc::now())
            .await
        {
            Ok(subject) => respond(json!({ "subject": subject })),
            Err(reject) => SpokeResult::Reject(reject),
        }
    }

    async fn fence_subject(
        &self,
        principal: &Principal,
        request: MemorySubjectsFenceRequest,
    ) -> SpokeResult<MemorySubjectsFenceResponse> {
        let op = "memory.subjects.fence";
        let principal_id = principal_id(principal);
        let idempotency_key = request.idempotency_key.as_str().to_owned();
        let space_id = request.space_id.as_str().to_owned();
        let subject_id = request.subject_id.as_str().to_owned();
        // The fence authority covers the operation's real target: a whole-subject
        // transition needs unrestricted coverage of the subject, a context
        // transition needs that one placement context.
        let target = match (capabilities_of(op), &request.target) {
            (Ok(capabilities), FenceTarget::Subject) => {
                OpTarget::new(capabilities, Some(&subject_id), &[], false, true)
            }
            (Ok(capabilities), FenceTarget::Context(context)) => OpTarget::new(
                capabilities,
                Some(&subject_id),
                &[context.as_str()],
                false,
                false,
            ),
            (Err(reject), _) => return SpokeResult::Reject(reject),
        };
        let admitted = match self.admitted(op, request) {
            Ok(admitted) => admitted,
            Err(reject) => return SpokeResult::Reject(reject),
        };
        // Test-only, op-targeted admission pause: after the typed decode and the
        // authenticated principal, before the operation's clock and its writer
        // transaction. Outcome-neutral; reads and other ops never reach it.
        self.faults().wait_target(op).await;
        let now = Utc::now();
        let mut transaction = match self.begin_write().await {
            Ok(transaction) => transaction,
            Err(reject) => return SpokeResult::Reject(reject),
        };
        match Self::replay_step(
            self,
            &mut transaction,
            principal_id,
            &space_id,
            op,
            &idempotency_key,
            &admitted,
            &target,
            PrecheckRefusal::Target,
            now,
        )
        .await
        {
            Ok(ReplayStep::Replayed(receipt)) => {
                let _ = transaction.rollback().await;
                return respond_receipt(&receipt);
            }
            Ok(ReplayStep::Execute) => {}
            Err(reject) => return SpokeResult::Reject(reject),
        }
        let change = match self
            .subjects_fence_in_tx(&mut transaction, principal_id, admitted.request(), now)
            .await
        {
            Ok(change) => change,
            Err(reject) => return SpokeResult::Reject(reject),
        };
        let result = json!({
            "subject_id": change.subject_id,
            "status": change.status,
            "fenced_contexts": change.fenced_contexts,
            "revision": non_zero(change.revision),
        });
        let receipt = match Self::stage_committed(
            &mut transaction,
            &space_id,
            principal_id,
            op,
            &idempotency_key,
            &admitted,
            &target,
            result,
            now,
        )
        .await
        {
            Ok(receipt) => receipt,
            Err(reject) => return SpokeResult::Reject(reject),
        };
        if transaction.commit().await.is_err() {
            return SpokeResult::Reject(commit_outcome_unknown());
        }
        respond_receipt(&receipt)
    }

    async fn list_grants(
        &self,
        principal: &Principal,
        request: MemoryGrantsListRequest,
    ) -> SpokeResult<MemoryGrantsListResponse> {
        match self
            .grants_list(principal_id(principal), &request, Utc::now())
            .await
        {
            Ok(page) => respond(json!({
                "grants": page.grants,
                "next_cursor": page.next_cursor,
            })),
            Err(reject) => SpokeResult::Reject(reject),
        }
    }

    async fn grant(
        &self,
        principal: &Principal,
        request: MemoryGrantsGrantRequest,
    ) -> SpokeResult<MemoryGrantsGrantResponse> {
        let op = "memory.grants.grant";
        let principal_id = principal_id(principal);
        let facets = issued_facets(&request);
        let target = match capabilities_of(op) {
            Ok(capabilities) => OpTarget::new(
                capabilities,
                facets.subject_id.as_deref(),
                &facets.contexts(),
                facets.shared,
                facets.context_ids.is_empty(),
            ),
            Err(reject) => return SpokeResult::Reject(reject),
        };
        let space_id = facets.space_id.clone();
        let idempotency_key = facets.idempotency_key.clone();
        let admitted = match self.admitted(op, request) {
            Ok(admitted) => admitted,
            Err(reject) => return SpokeResult::Reject(reject),
        };
        let now = Utc::now();
        let mut transaction = match self.begin_write().await {
            Ok(transaction) => transaction,
            Err(reject) => return SpokeResult::Reject(reject),
        };
        match Self::replay_step(
            self,
            &mut transaction,
            principal_id,
            &space_id,
            op,
            &idempotency_key,
            &admitted,
            &target,
            PrecheckRefusal::Target,
            now,
        )
        .await
        {
            Ok(ReplayStep::Replayed(receipt)) => {
                let _ = transaction.rollback().await;
                return respond_receipt(&receipt);
            }
            Ok(ReplayStep::Execute) => {}
            Err(reject) => return SpokeResult::Reject(reject),
        }
        let issued = match self
            .grants_grant_in_tx(&mut transaction, principal_id, admitted.request(), now)
            .await
        {
            Ok(issued) => issued,
            Err(reject) => return SpokeResult::Reject(reject),
        };
        let result = json!({
            "grant_id": issued.grant_id,
            "revision": non_zero(issued.revision),
        });
        let receipt = match Self::stage_committed(
            &mut transaction,
            &space_id,
            principal_id,
            op,
            &idempotency_key,
            &admitted,
            &target,
            result,
            now,
        )
        .await
        {
            Ok(receipt) => receipt,
            Err(reject) => return SpokeResult::Reject(reject),
        };
        if transaction.commit().await.is_err() {
            return SpokeResult::Reject(commit_outcome_unknown());
        }
        respond_receipt(&receipt)
    }

    async fn revoke_grant(
        &self,
        principal: &Principal,
        request: MemoryGrantsRevokeRequest,
    ) -> SpokeResult<MemoryGrantsRevokeResponse> {
        let op = "memory.grants.revoke";
        let principal_id = principal_id(principal);
        let space_id = request.space_id.as_str().to_owned();
        let idempotency_key = request.idempotency_key.as_str().to_owned();
        let admitted = match self.admitted(op, request) {
            Ok(admitted) => admitted,
            Err(reject) => return SpokeResult::Reject(reject),
        };
        // Test-only, op-targeted admission pause: after the authenticated typed
        // admission and before the operation's clock and writer transaction.
        // Outcome-neutral; reads and other ops never reach it.
        self.faults().wait_target(op).await;
        let now = Utc::now();
        let mut transaction = match self.begin_write().await {
            Ok(transaction) => transaction,
            Err(reject) => return SpokeResult::Reject(reject),
        };
        let precheck = self
            .revoke_target(
                &mut transaction,
                principal_id,
                &space_id,
                admitted.request().grant_id.as_str(),
                op,
                now,
            )
            .await;
        let target = match precheck {
            Ok(Some(target)) => target,
            Ok(None) => {
                return self
                    .revoke_unknown_grant(transaction, principal_id, admitted.request(), now)
                    .await
            }
            Err(reject) => return SpokeResult::Reject(reject),
        };
        match Self::replay_step(
            self,
            &mut transaction,
            principal_id,
            &space_id,
            op,
            &idempotency_key,
            &admitted,
            &target,
            PrecheckRefusal::Target,
            now,
        )
        .await
        {
            Ok(ReplayStep::Replayed(receipt)) => {
                let _ = transaction.rollback().await;
                return respond_receipt(&receipt);
            }
            Ok(ReplayStep::Execute) => {}
            // Only an authorization refusal is translated into the revoke
            // slice's one indistinguishable wording. A replay conflict, an
            // expired/retained outcome or a storage failure keeps its real
            // code and details (§14.2).
            Err(reject) if reject.code == SpokeRejectCode::NotAuthorized => {
                return SpokeResult::Reject(not_authorized(
                    "the grant is not revocable by the authenticated principal",
                ))
            }
            Err(reject) => return SpokeResult::Reject(reject),
        }
        self.revoke_effect(
            transaction,
            principal_id,
            &space_id,
            &admitted,
            &target,
            now,
        )
        .await
    }

    async fn capture(
        &self,
        principal: &Principal,
        request: MemoryCaptureRequest,
    ) -> SpokeResult<MemoryCaptureResponse> {
        let op = "memory.capture";
        let admitted = match self.admitted(op, request) {
            Ok(admitted) => admitted,
            Err(reject) => return SpokeResult::Reject(reject),
        };
        match Self::capture(self, principal_id(principal), &admitted, Utc::now()).await {
            Ok(receipt) => respond_receipt(&receipt),
            Err(reject) => SpokeResult::Reject(reject),
        }
    }

    async fn list_candidates(
        &self,
        principal: &Principal,
        request: MemoryCandidatesListRequest,
    ) -> SpokeResult<MemoryCandidatesListResponse> {
        match self
            .candidates_list(principal_id(principal), &request, Utc::now())
            .await
        {
            Ok(page) => respond(json!({
                "candidates": page.candidates,
                "next_cursor": page.next_cursor,
            })),
            Err(reject) => SpokeResult::Reject(reject),
        }
    }

    async fn get_candidate(
        &self,
        principal: &Principal,
        request: MemoryCandidatesGetRequest,
    ) -> SpokeResult<MemoryCandidatesGetResponse> {
        match self
            .candidates_get(principal_id(principal), &request, Utc::now())
            .await
        {
            Ok(candidate) => respond(json!({ "candidate": candidate })),
            Err(reject) => SpokeResult::Reject(reject),
        }
    }

    async fn dismiss_candidate(
        &self,
        principal: &Principal,
        request: MemoryCandidatesDismissRequest,
    ) -> SpokeResult<MemoryCandidatesDismissResponse> {
        let op = "memory.candidates.dismiss";
        let admitted = match self.admitted(op, request) {
            Ok(admitted) => admitted,
            Err(reject) => return SpokeResult::Reject(reject),
        };
        match self
            .candidates_dismiss(principal_id(principal), &admitted, Utc::now())
            .await
        {
            Ok(receipt) => respond_receipt(&receipt),
            Err(reject) => SpokeResult::Reject(reject),
        }
    }

    async fn review_candidate(
        &self,
        principal: &Principal,
        request: MemoryCandidatesReviewRequest,
    ) -> SpokeResult<MemoryCandidatesReviewResponse> {
        let op = "memory.candidates.review";
        let admitted = match self.admitted(op, request) {
            Ok(admitted) => admitted,
            Err(reject) => return SpokeResult::Reject(reject),
        };
        // Test-only, op-targeted admission pause: after the typed decode and the
        // authenticated principal, before the private slice opens its writer
        // transaction. Outcome-neutral; reads and other ops never reach it.
        self.faults().wait_target(op).await;
        match self
            .candidates_review(principal_id(principal), &admitted, Utc::now())
            .await
        {
            Ok(receipt) => respond_receipt(&receipt),
            Err(reject) => SpokeResult::Reject(reject),
        }
    }

    async fn create_record(
        &self,
        principal: &Principal,
        request: MemoryRecordsCreateRequest,
    ) -> SpokeResult<MemoryRecordsCreateResponse> {
        let op = "memory.records.create";
        let principal_id = principal_id(principal);
        let space_id = request.space_id.as_str().to_owned();
        let idempotency_key = request.idempotency_key.as_str().to_owned();
        let subject_id = request.subject_id.as_str().to_owned();
        let (visibility, context_id) = match &request.scope {
            CreateScope::Shared => ("shared", None),
            CreateScope::Context(context) => ("context", Some(context.as_str())),
        };
        let target = match capabilities_of(op) {
            Ok(capabilities) => {
                OpTarget::placement(capabilities, &subject_id, visibility, context_id)
            }
            Err(reject) => return SpokeResult::Reject(reject),
        };
        let admitted = match self.admitted(op, request) {
            Ok(admitted) => admitted,
            Err(reject) => return SpokeResult::Reject(reject),
        };
        let now = Utc::now();
        let mut transaction = match self.begin_write().await {
            Ok(transaction) => transaction,
            Err(reject) => return SpokeResult::Reject(reject),
        };
        match Self::replay_step(
            self,
            &mut transaction,
            principal_id,
            &space_id,
            op,
            &idempotency_key,
            &admitted,
            &target,
            PrecheckRefusal::Target,
            now,
        )
        .await
        {
            Ok(ReplayStep::Replayed(receipt)) => {
                let _ = transaction.rollback().await;
                return respond_receipt(&receipt);
            }
            Ok(ReplayStep::Execute) => {}
            Err(reject) => return SpokeResult::Reject(reject),
        }
        let created = match self
            .records_create_in_tx(&mut transaction, principal_id, admitted.request(), now)
            .await
        {
            Ok(created) => created,
            Err(reject) => return SpokeResult::Reject(reject),
        };
        let result = json!({
            "record_id": created.record_id,
            "revision": non_zero(created.revision),
        });
        let receipt = match Self::stage_committed(
            &mut transaction,
            &space_id,
            principal_id,
            op,
            &idempotency_key,
            &admitted,
            &target,
            result,
            now,
        )
        .await
        {
            Ok(receipt) => receipt,
            Err(reject) => return SpokeResult::Reject(reject),
        };
        if transaction.commit().await.is_err() {
            return SpokeResult::Reject(commit_outcome_unknown());
        }
        respond_receipt(&receipt)
    }

    async fn get_record(
        &self,
        principal: &Principal,
        request: MemoryRecordsGetRequest,
    ) -> SpokeResult<MemoryRecordsGetResponse> {
        match self
            .records_get(principal_id(principal), &request, Utc::now())
            .await
        {
            Ok(record) => respond(json!({ "record": record })),
            Err(reject) => SpokeResult::Reject(reject),
        }
    }

    async fn update_record(
        &self,
        principal: &Principal,
        request: MemoryRecordsUpdateRequest,
    ) -> SpokeResult<MemoryRecordsUpdateResponse> {
        let op = "memory.records.update";
        let principal_id = principal_id(principal);
        let space_id = record_update_space(&request);
        let record_id = record_update_id(&request);
        let idempotency_key = record_update_key(&request);
        let admitted = match self.admitted(op, request) {
            Ok(admitted) => admitted,
            Err(reject) => return SpokeResult::Reject(reject),
        };
        let now = Utc::now();
        let mut transaction = match self.begin_write().await {
            Ok(transaction) => transaction,
            Err(reject) => return SpokeResult::Reject(reject),
        };
        let placement = match self
            .record_placement(&mut transaction, &space_id, &record_id)
            .await
        {
            Ok(placement) => placement,
            Err(reject) => return SpokeResult::Reject(reject),
        };
        let Some((subject_id, visibility, context)) = placement else {
            // No stored placement to name a target from: let the records slice
            // answer its own uniform refusal, then discard the transaction.
            let outcome = self
                .records_update_in_tx(&mut transaction, principal_id, admitted.request(), now)
                .await;
            let _ = transaction.rollback().await;
            return match outcome {
                Err(reject) => SpokeResult::Reject(reject),
                Ok(_) => SpokeResult::Reject(internal(
                    "a record update succeeded without a readable placement",
                )),
            };
        };
        let target = match capabilities_of(op) {
            Ok(capabilities) => {
                OpTarget::placement(capabilities, &subject_id, &visibility, context.as_deref())
            }
            Err(reject) => return SpokeResult::Reject(reject),
        };
        match Self::replay_step(
            self,
            &mut transaction,
            principal_id,
            &space_id,
            op,
            &idempotency_key,
            &admitted,
            &target,
            PrecheckRefusal::RecordAbsent,
            now,
        )
        .await
        {
            Ok(ReplayStep::Replayed(receipt)) => {
                let _ = transaction.rollback().await;
                return respond_receipt(&receipt);
            }
            Ok(ReplayStep::Execute) => {}
            Err(reject) => return SpokeResult::Reject(reject),
        }
        let updated = match self
            .records_update_in_tx(&mut transaction, principal_id, admitted.request(), now)
            .await
        {
            Ok(updated) => updated,
            Err(reject) => return SpokeResult::Reject(reject),
        };
        let result = json!({
            "record_id": updated.record_id,
            "revision": non_zero(updated.revision),
        });
        let receipt = match Self::stage_committed(
            &mut transaction,
            &space_id,
            principal_id,
            op,
            &idempotency_key,
            &admitted,
            &target,
            result,
            now,
        )
        .await
        {
            Ok(receipt) => receipt,
            Err(reject) => return SpokeResult::Reject(reject),
        };
        if transaction.commit().await.is_err() {
            return SpokeResult::Reject(commit_outcome_unknown());
        }
        respond_receipt(&receipt)
    }

    async fn delete_record(
        &self,
        principal: &Principal,
        request: MemoryRecordsDeleteRequest,
    ) -> SpokeResult<MemoryRecordsDeleteResponse> {
        let op = "memory.records.delete";
        let principal_id = principal_id(principal);
        let space_id = request.space_id.as_str().to_owned();
        let record_id = request.record_id.as_str().to_owned();
        let idempotency_key = request.idempotency_key.as_str().to_owned();
        let admitted = match self.admitted(op, request) {
            Ok(admitted) => admitted,
            Err(reject) => return SpokeResult::Reject(reject),
        };
        let now = Utc::now();
        let mut transaction = match self.begin_write().await {
            Ok(transaction) => transaction,
            Err(reject) => return SpokeResult::Reject(reject),
        };
        let placement = match self
            .record_placement(&mut transaction, &space_id, &record_id)
            .await
        {
            Ok(placement) => placement,
            Err(reject) => return SpokeResult::Reject(reject),
        };
        let Some((subject_id, visibility, context)) = placement else {
            let outcome = self
                .records_delete_in_tx(&mut transaction, principal_id, admitted.request(), now)
                .await;
            let _ = transaction.rollback().await;
            return match outcome {
                Err(reject) => SpokeResult::Reject(reject),
                Ok(_) => SpokeResult::Reject(internal(
                    "a record delete succeeded without a readable placement",
                )),
            };
        };
        let target = match capabilities_of(op) {
            Ok(capabilities) => {
                OpTarget::placement(capabilities, &subject_id, &visibility, context.as_deref())
            }
            Err(reject) => return SpokeResult::Reject(reject),
        };
        match Self::replay_step(
            self,
            &mut transaction,
            principal_id,
            &space_id,
            op,
            &idempotency_key,
            &admitted,
            &target,
            PrecheckRefusal::RecordAbsent,
            now,
        )
        .await
        {
            Ok(ReplayStep::Replayed(receipt)) => {
                let _ = transaction.rollback().await;
                return respond_receipt(&receipt);
            }
            Ok(ReplayStep::Execute) => {}
            Err(reject) => return SpokeResult::Reject(reject),
        }
        let deleted = match self
            .records_delete_in_tx(&mut transaction, principal_id, admitted.request(), now)
            .await
        {
            Ok(deleted) => deleted,
            Err(reject) => return SpokeResult::Reject(reject),
        };
        let result = json!({
            "record_id": deleted.record_id,
            "tombstoned": deleted.tombstoned,
        });
        let receipt = match Self::stage_committed(
            &mut transaction,
            &space_id,
            principal_id,
            op,
            &idempotency_key,
            &admitted,
            &target,
            result,
            now,
        )
        .await
        {
            Ok(receipt) => receipt,
            Err(reject) => return SpokeResult::Reject(reject),
        };
        if transaction.commit().await.is_err() {
            return SpokeResult::Reject(commit_outcome_unknown());
        }
        respond_receipt(&receipt)
    }

    async fn list_records(
        &self,
        principal: &Principal,
        request: MemoryRecordsListRequest,
    ) -> SpokeResult<MemoryRecordsListResponse> {
        match self
            .records_list(principal_id(principal), &request, Utc::now())
            .await
        {
            Ok(page) => respond(json!({
                "records": page.records,
                "total_authorized": page.total_authorized,
                "next_cursor": page.next_cursor,
            })),
            Err(reject) => SpokeResult::Reject(reject),
        }
    }

    async fn promote_record(
        &self,
        principal: &Principal,
        request: MemoryRecordsPromoteRequest,
    ) -> SpokeResult<MemoryRecordsPromoteResponse> {
        let op = "memory.records.promote";
        let principal_id = principal_id(principal);
        let space_id = request.space_id.as_str().to_owned();
        let record_id = request.record_id.as_str().to_owned();
        let idempotency_key = request.idempotency_key.as_str().to_owned();
        let admitted = match self.admitted(op, request) {
            Ok(admitted) => admitted,
            Err(reject) => return SpokeResult::Reject(reject),
        };
        let now = Utc::now();
        let mut transaction = match self.begin_write().await {
            Ok(transaction) => transaction,
            Err(reject) => return SpokeResult::Reject(reject),
        };
        let placement = match self
            .record_placement(&mut transaction, &space_id, &record_id)
            .await
        {
            Ok(placement) => placement,
            Err(reject) => return SpokeResult::Reject(reject),
        };
        let Some((subject_id, source_visibility, source_context)) = placement else {
            let outcome = self
                .records_promote_in_tx(&mut transaction, principal_id, admitted.request(), now)
                .await;
            let _ = transaction.rollback().await;
            return match outcome {
                Err(reject) => SpokeResult::Reject(reject),
                Ok(_) => SpokeResult::Reject(internal(
                    "a record promotion succeeded without a readable placement",
                )),
            };
        };
        let target = match promote_target(
            &subject_id,
            &source_visibility,
            source_context.as_deref(),
            &admitted.request().target_scope,
        ) {
            Ok(target) => target,
            Err(reject) => return SpokeResult::Reject(reject),
        };
        match Self::replay_step(
            self,
            &mut transaction,
            principal_id,
            &space_id,
            op,
            &idempotency_key,
            &admitted,
            &target,
            PrecheckRefusal::RecordAbsent,
            now,
        )
        .await
        {
            Ok(ReplayStep::Replayed(receipt)) => {
                let _ = transaction.rollback().await;
                return respond_receipt(&receipt);
            }
            Ok(ReplayStep::Execute) => {}
            Err(reject) => return SpokeResult::Reject(reject),
        }
        let promoted = match self
            .records_promote_in_tx(&mut transaction, principal_id, admitted.request(), now)
            .await
        {
            Ok(promoted) => promoted,
            Err(reject) => return SpokeResult::Reject(reject),
        };
        let result = json!({
            "record_id": promoted.record_id,
            "revision": non_zero(promoted.revision),
            "scope": promoted.scope,
        });
        let receipt = match Self::stage_committed(
            &mut transaction,
            &space_id,
            principal_id,
            op,
            &idempotency_key,
            &admitted,
            &target,
            result,
            now,
        )
        .await
        {
            Ok(receipt) => receipt,
            Err(reject) => return SpokeResult::Reject(reject),
        };
        if transaction.commit().await.is_err() {
            return SpokeResult::Reject(commit_outcome_unknown());
        }
        respond_receipt(&receipt)
    }

    async fn recall(
        &self,
        principal: &Principal,
        request: MemoryRecallRequest,
    ) -> SpokeResult<MemoryRecallResponse> {
        match Self::recall(self, principal_id(principal), &request, Utc::now()).await {
            Ok(page) => respond(json!({
                "records": page.records,
                "total_authorized": page.total_authorized,
                "truncated": page.truncated,
                "overflow_record_id": page.overflow_record_id,
                "next_cursor": page.next_cursor,
                "engine_policy_version": page.engine_policy_version,
            })),
            Err(reject) => SpokeResult::Reject(reject),
        }
    }

    async fn get_profile(
        &self,
        principal: &Principal,
        request: MemoryProfileGetRequest,
    ) -> SpokeResult<MemoryProfileGetResponse> {
        match self
            .profile_get(principal_id(principal), &request, Utc::now())
            .await
        {
            Ok(page) => respond(json!({ "profiles": page.profiles })),
            Err(reject) => SpokeResult::Reject(reject),
        }
    }

    async fn update_profile(
        &self,
        principal: &Principal,
        request: MemoryProfileUpdateRequest,
    ) -> SpokeResult<MemoryProfileUpdateResponse> {
        let op = "memory.profile.update";
        let principal_id = principal_id(principal);
        let space_id = request.space_id.as_str().to_owned();
        let subject_id = request.subject_id.as_str().to_owned();
        let idempotency_key = request.idempotency_key.as_str().to_owned();
        // A profile is one subject-level trusted document: its write covers
        // shared document access, exactly as the profile slice authorizes it.
        let target = match capabilities_of(op) {
            Ok(capabilities) => OpTarget::new(capabilities, Some(&subject_id), &[], true, false),
            Err(reject) => return SpokeResult::Reject(reject),
        };
        let admitted = match self.admitted(op, request) {
            Ok(admitted) => admitted,
            Err(reject) => return SpokeResult::Reject(reject),
        };
        let now = Utc::now();
        let mut transaction = match self.begin_write().await {
            Ok(transaction) => transaction,
            Err(reject) => return SpokeResult::Reject(reject),
        };
        match Self::replay_step(
            self,
            &mut transaction,
            principal_id,
            &space_id,
            op,
            &idempotency_key,
            &admitted,
            &target,
            PrecheckRefusal::Target,
            now,
        )
        .await
        {
            Ok(ReplayStep::Replayed(receipt)) => {
                let _ = transaction.rollback().await;
                return respond_receipt(&receipt);
            }
            Ok(ReplayStep::Execute) => {}
            Err(reject) => return SpokeResult::Reject(reject),
        }
        let change = match self
            .profile_update_in_tx(&mut transaction, principal_id, admitted.request(), now)
            .await
        {
            Ok(change) => change,
            Err(reject) => return SpokeResult::Reject(reject),
        };
        let result = json!({
            "profile_kind": change.profile_kind,
            "revision": non_zero(change.revision),
        });
        let receipt = match Self::stage_committed(
            &mut transaction,
            &space_id,
            principal_id,
            op,
            &idempotency_key,
            &admitted,
            &target,
            result,
            now,
        )
        .await
        {
            Ok(receipt) => receipt,
            Err(reject) => return SpokeResult::Reject(reject),
        };
        if transaction.commit().await.is_err() {
            return SpokeResult::Reject(commit_outcome_unknown());
        }
        respond_receipt(&receipt)
    }

    async fn get_reflection_state(
        &self,
        principal: &Principal,
        request: MemoryReflectionsStateRequest,
    ) -> SpokeResult<MemoryReflectionsStateResponse> {
        let view = match self
            .reflection_state(principal_id(principal), &request, Utc::now())
            .await
        {
            Ok(view) => view,
            Err(reject) => return SpokeResult::Reject(reject),
        };
        let mut body = json!({
            "basis": view.basis,
            "data_sufficiency": if view.sufficient { "sufficient" } else { "insufficient" },
            "missing_required_profile_kinds": view.missing_required_profile_kinds,
            "stale": view.stale,
            "state_digest": view.state_digest,
        });
        if let Some(last_reflection_id) = view.last_reflection_id {
            body["last_reflection_id"] = json!(last_reflection_id);
        }
        respond(body)
    }

    async fn list_reflections(
        &self,
        principal: &Principal,
        request: MemoryReflectionsListRequest,
    ) -> SpokeResult<MemoryReflectionsListResponse> {
        match self
            .reflection_list(principal_id(principal), &request, Utc::now())
            .await
        {
            Ok(page) => respond(json!({
                "reflections": page.reflections,
                "next_cursor": page.next_cursor,
            })),
            Err(reject) => SpokeResult::Reject(reject),
        }
    }

    async fn generate_reflection(
        &self,
        principal: &Principal,
        request: MemoryReflectionsGenerateRequest,
    ) -> SpokeResult<MemoryReflectionsGenerateResponse> {
        let admitted = match self.admitted("memory.reflections.generate", request) {
            Ok(admitted) => admitted,
            Err(reject) => return SpokeResult::Reject(reject),
        };
        match self
            .reflection_generate(principal_id(principal), &admitted, Utc::now())
            .await
        {
            Ok(receipt) => respond_receipt(&receipt),
            Err(reject) => SpokeResult::Reject(reject),
        }
    }

    async fn get_receipt(
        &self,
        principal: &Principal,
        request: MemoryReceiptsGetRequest,
    ) -> SpokeResult<MemoryReceiptsGetResponse> {
        match self
            .receipts_get(principal_id(principal), &request, Utc::now())
            .await
        {
            Ok(receipt) => respond(json!({ "receipt": receipt })),
            Err(reject) => SpokeResult::Reject(reject),
        }
    }

    async fn create_export(
        &self,
        _principal: &Principal,
        _request: MemoryExportsCreateRequest,
    ) -> SpokeResult<MemoryExportsCreateResponse> {
        unsupported("memory.exports.create")
    }

    async fn get_export(
        &self,
        _principal: &Principal,
        _request: MemoryExportsGetRequest,
    ) -> SpokeResult<MemoryExportsGetResponse> {
        unsupported("memory.exports.get")
    }

    async fn stage_import(
        &self,
        _principal: &Principal,
        _request: MemoryImportsStageRequest,
    ) -> SpokeResult<MemoryImportsStageResponse> {
        unsupported("memory.imports.stage")
    }

    async fn activate_import(
        &self,
        _principal: &Principal,
        _request: MemoryImportsActivateRequest,
    ) -> SpokeResult<MemoryImportsActivateResponse> {
        unsupported("memory.imports.activate")
    }

    async fn discard_import(
        &self,
        _principal: &Principal,
        _request: MemoryImportsDiscardRequest,
    ) -> SpokeResult<MemoryImportsDiscardResponse> {
        unsupported("memory.imports.discard")
    }
}

// ---------------------------------------------------------------------------
// Request helpers shared by the mutation wrappers
// ---------------------------------------------------------------------------

/// The space, record and key of an update request (its variant selects the
/// payload, never the identity).
fn record_update_space(request: &MemoryRecordsUpdateRequest) -> String {
    match request {
        MemoryRecordsUpdateRequest::Variant0 { space_id, .. }
        | MemoryRecordsUpdateRequest::Variant1 { space_id, .. } => space_id.as_str().to_owned(),
    }
}

fn record_update_id(request: &MemoryRecordsUpdateRequest) -> String {
    match request {
        MemoryRecordsUpdateRequest::Variant0 { record_id, .. }
        | MemoryRecordsUpdateRequest::Variant1 { record_id, .. } => record_id.as_str().to_owned(),
    }
}

fn record_update_key(request: &MemoryRecordsUpdateRequest) -> String {
    match request {
        MemoryRecordsUpdateRequest::Variant0 {
            idempotency_key, ..
        }
        | MemoryRecordsUpdateRequest::Variant1 {
            idempotency_key, ..
        } => idempotency_key.as_str().to_owned(),
    }
}

/// The issuer-side facets of one `memory.grants.grant` request: what the request
/// asks to hand out, which is exactly what the granter's admin coverage must
/// contain (§6.5 non-amplification).
struct IssuedFacets {
    space_id: String,
    idempotency_key: String,
    subject_id: Option<String>,
    context_ids: Vec<String>,
    shared: bool,
}

impl IssuedFacets {
    fn contexts(&self) -> Vec<&str> {
        self.context_ids.iter().map(String::as_str).collect()
    }
}

fn issued_facets(request: &MemoryGrantsGrantRequest) -> IssuedFacets {
    match request {
        MemoryGrantsGrantRequest::Variant0 {
            space_id,
            idempotency_key,
            subject_id,
            context_ids,
            shared_access,
            ..
        } => IssuedFacets {
            space_id: space_id.as_str().to_owned(),
            idempotency_key: idempotency_key.as_str().to_owned(),
            subject_id: subject_id.as_ref().map(|id| id.as_str().to_owned()),
            context_ids: context_ids.clone(),
            shared: shared_access.unwrap_or(false),
        },
        MemoryGrantsGrantRequest::Variant1 {
            space_id,
            idempotency_key,
            subject_id,
            ..
        } => IssuedFacets {
            space_id: space_id.as_str().to_owned(),
            idempotency_key: idempotency_key.as_str().to_owned(),
            subject_id: subject_id.as_ref().map(|id| id.as_str().to_owned()),
            context_ids: Vec::new(),
            shared: false,
        },
    }
}

/// Whether the principal holds any live `admin` grant in the space, whatever
/// its facets — the revoke slice's stage-1 gate, repeated here because the
/// precheck must refuse with exactly that vocabulary.
fn has_live_admin(
    grants: &[MemoryGrant],
    principal_id: &str,
    space_id: &str,
    now: DateTime<Utc>,
) -> bool {
    grants.iter().any(|grant| {
        let (grant_principal, grant_space, capabilities, expires_at, revoked) = match grant {
            MemoryGrant::RestrictedGrant(grant) => (
                grant.principal_id.as_str(),
                grant.space_id.as_str(),
                &grant.capabilities,
                grant.expires_at.as_ref(),
                grant.revoked_at.is_some(),
            ),
            MemoryGrant::UnrestrictedGrant(grant) => (
                grant.principal_id.as_str(),
                grant.space_id.as_str(),
                &grant.capabilities,
                grant.expires_at.as_ref(),
                grant.revoked_at.is_some(),
            ),
        };
        grant_principal == principal_id
            && grant_space == space_id
            && !revoked
            && expires_at.is_none_or(|expires| **expires > now)
            && capabilities.iter().any(|capability| capability == "admin")
    })
}

fn grant_id_of(grant: &MemoryGrant) -> &str {
    match grant {
        MemoryGrant::RestrictedGrant(grant) => grant.grant_id.as_str(),
        MemoryGrant::UnrestrictedGrant(grant) => grant.grant_id.as_str(),
    }
}

/// The recorded target facets of an existing grant — the requirement a
/// revocation of it has to satisfy.
fn grant_facets(grant: &MemoryGrant) -> OpTarget {
    match grant {
        MemoryGrant::RestrictedGrant(grant) => {
            let contexts: Vec<&str> = grant.context_ids.iter().map(String::as_str).collect();
            OpTarget::new(
                &[],
                grant.subject_id.as_ref().map(|id| id.as_str()),
                &contexts,
                grant.shared_access.unwrap_or(false),
                contexts.is_empty(),
            )
        }
        MemoryGrant::UnrestrictedGrant(grant) => OpTarget::new(
            &[],
            grant.subject_id.as_ref().map(|id| id.as_str()),
            &[],
            false,
            true,
        ),
    }
}

impl OpTarget {
    const fn with_capabilities(mut self, capabilities: &'static [&'static str]) -> Self {
        self.capabilities = capabilities;
        self
    }
}

/// The requester identity of one call: the admission context the service face
/// already resolved from the authenticated peer (MEM-R003).
fn principal_id(principal: &Principal) -> &str {
    principal.principal_id.as_str()
}
