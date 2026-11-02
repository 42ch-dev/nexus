//! Bearer-isolated Creator/Character memory and Character `ToM` commands
//! (v1.190 P2-T2).
//!
//! Moved from the daemon `api::handlers::{memory,character_memory}` handlers
//! and the `character_tom` composer: pending-review capture/list/count/delete,
//! fragment listing and revision-checked promotion for both memory bearers,
//! the bounded Creator review drain, and the Character `ToM` carrier
//! record/list family. Request bodies never carry `owner_creator_id`; the
//! principal's creator is the only admitted caller and every Character
//! mutation admits a per-Character activity lease ([`crate::ActorActivityLease`])
//! held across the DB/file/provider effects. Storage access is always scoped
//! to the caller's own bearer rows — cross-bearer reads and writes are
//! structurally unreachable.

use nexus_contracts::generated::daemon_api::characters::memory::capture_character_pending_review_request::CaptureCharacterPendingReviewRequest;
use nexus_contracts::generated::daemon_api::characters::memory::capture_character_pending_review_response::CaptureCharacterPendingReviewResponse;
use nexus_contracts::generated::daemon_api::characters::memory::character_memory_fragment_info::CharacterMemoryFragmentInfo;
use nexus_contracts::generated::daemon_api::characters::memory::count_character_pending_reviews_response::CountCharacterPendingReviewsResponse;
use nexus_contracts::generated::daemon_api::characters::memory::delete_character_pending_review_response::DeleteCharacterPendingReviewResponse;
use nexus_contracts::generated::daemon_api::characters::memory::list_character_memory_fragments_response::ListCharacterMemoryFragmentsResponse;
use nexus_contracts::generated::daemon_api::characters::memory::list_character_pending_reviews_response::ListCharacterPendingReviewsResponse;
use nexus_contracts::generated::daemon_api::characters::memory::promote_character_fragment_response::PromoteCharacterFragmentResponse;
use nexus_contracts::generated::daemon_api::characters::memory::review_character_memory_response::ReviewCharacterMemoryResponse;
use nexus_contracts::generated::daemon_api::characters::tom::list_character_tom_query::ListCharacterTomQuery;
use nexus_contracts::generated::daemon_api::characters::tom::list_character_tom_response::{
    ListCharacterTomResponse, NexusCharacterTomBeliefItem as ListedBeliefItem,
    NexusPaginationInfo as ListedPagination,
};
use nexus_contracts::generated::daemon_api::characters::tom::record_character_tom_request::RecordCharacterTomRequest;
use nexus_contracts::generated::daemon_api::characters::tom::record_character_tom_response::RecordCharacterTomResponse;
use nexus_contracts::generated::daemon_api::memory::count_pending_reviews_response::CountPendingReviewsResponse;
use nexus_contracts::generated::daemon_api::memory::create_pending_review_request::CreatePendingReviewRequest;
use nexus_contracts::generated::daemon_api::memory::create_pending_review_response::CreatePendingReviewResponse;
use nexus_contracts::generated::daemon_api::memory::delete_pending_review_response::DeletePendingReviewResponse;
use nexus_contracts::generated::daemon_api::memory::list_memory_fragments_response::ListMemoryFragmentsResponse;
use nexus_contracts::generated::daemon_api::memory::list_pending_reviews_response::ListPendingReviewsResponse;
use nexus_contracts::generated::daemon_api::memory::memory_fragment_info::MemoryFragmentInfo;
use nexus_contracts::generated::daemon_api::memory::review_request::ReviewRequest;
use nexus_contracts::generated::daemon_api::memory::review_response::ReviewResponse;
use nexus_creator_memory::experience_aggregation::ExperienceEntry;
use nexus_creator_memory::personality_sync::PERSONALITY_MEMORY_SLUG;
use nexus_creator_memory::soul::SoulDocument;
use nexus_knowledge::world_kb::knowledge_entry::{
    validate_character_tom_belief_row, BeliefPropositionRaw, KnowledgeEntryRecord,
    KnowledgeOwnerRef,
};
use nexus_knowledge::world_kb::store::{KbStore, KbStoreError};
use nexus_local_db::kb_store::SqliteKbStore;
use nexus_local_db::RUN_PENDING_ID_PREFIX;
use nexus_local_db::LocalDbError;
use nexus_spoke_adapter::adapter::mind_state::atomic_cas_carrier_modules_and_insert_mind_state_in_tx;
use serde_json::{json, Value};
use sqlx::SqlitePool;

use crate::actors::AdmittedActor;
use crate::error::{CoreError, CoreResult};
use crate::memory_pipeline::{
    internal_err, map_local_db_error, process_bearer_review_batch, sqlx_internal,
    MemoryPipelineCtx, ReviewBatchRow, REVIEW_BATCH_LIMIT,
};
use nexus_creator_memory::bearer::MemoryBearerRef;
use std::collections::HashSet;
use std::sync::Arc;

use crate::memory_provider::legacy;
use crate::memory_provider::policy;
use crate::memory_provider::profile;
use crate::memory_provider::{created_at_cmp, LocalMemoryProvider, MemoryDisposition};
use crate::principal::Principal;
use crate::service::CoreService;
use spoke_operations::{spoke_schemas, MemoryPort, SpokeReject, SpokeRejectCode, SpokeResult};

/// Per-Character review-call budget (mirrors the Creator review handler — the
/// batch drains at most one bounded slice per call and reports `has_more`).
const REVIEW_CALL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

const MAX_DIGEST_BYTES: usize = 64 * 1024;

fn wire_err(err: impl std::fmt::Display) -> CoreError {
    internal_err("character_tom_wire_invalid", err)
}

fn map_wire<T: serde::de::DeserializeOwned>(value: impl serde::Serialize) -> CoreResult<T> {
    let json = serde_json::to_value(value).map_err(wire_err)?;
    serde_json::from_value(json).map_err(wire_err)
}

fn not_found(resource: &str, id: &str) -> CoreError {
    CoreError::NotFound {
        resource: format!("{resource} {id}"),
    }
}

fn invalid_input(message: impl Into<String>) -> CoreError {
    CoreError::ActorInput(message.into())
}

fn optional_str(value: Option<&impl std::ops::Deref<Target = String>>) -> Option<&str> {
    value.map(|s| s.as_str())
}

fn pagination_info(limit: u32, has_more: bool, next_cursor: Option<&str>) -> serde_json::Value {
    serde_json::json!({
        "limit": limit,
        "has_more": has_more,
        "next_cursor": next_cursor,
    })
}

// ── Provider seam (NXM-5 Task 2) ─────────────────────────────────────────
//
// The read and dismissal handlers below serve the memory family through the
// `MemoryPort` the open pinned ([`CoreService::memory_provider`]) instead of
// the legacy tables. This block is the existing-DTO/error adapter: a handler
// keeps its retained request/response shape (compass D5) while the adapter
// records the legacy page/keyset/offset semantics on top of provider reads and
// maps a port refusal back onto the core error taxonomy.
//
// The provider projects the unmigrated legacy rows through one merged read
// (the NXM-5 Task 1 bridge), so a handler serves migrated and not-yet-migrated
// content alike. Legacy metadata rides in `content.structured.nexus_legacy`,
// which is what lets the retained DTOs render the legacy display fields
// (`pending_id`, `fragment_id`, `world_id`, keywords) rather than provider ids.

/// The provider space this workspace serves (`memory.describe` reports it).
const MEMORY_SPACE_ID: &str = crate::memory_provider::DEFAULT_SPACE_ID;

/// Rows the adapter asks for per internal provider page: the provider's own
/// advertised `max_page_size`, so a legacy DTO never names a bigger bound.
const MEMORY_PAGE_SIZE: u64 = 100;

/// The one authored profile kind (§4.2): the `SOUL.md` document.
const SOUL_PROFILE_KIND: &str = "personality";

/// Map one port refusal onto the retained core error taxonomy.
fn memory_reject(reject: &SpokeReject) -> CoreError {
    match reject.code {
        SpokeRejectCode::NotFound => CoreError::NotFound {
            resource: reject.message.clone(),
        },
        SpokeRejectCode::NotAuthorized => CoreError::ForbiddenReason {
            resource: "memory".to_string(),
            reason: reject.message.clone(),
        },
        SpokeRejectCode::InvalidInput => CoreError::InvalidInput {
            field: "memory".to_string(),
            reason: reject.message.clone(),
        },
        // A CAS miss keeps the retained `version_mismatch` conflict code the
        // legacy promotion path already renders (409 at the HTTP adapters), so
        // the same stale-revision answer reads the same on either path.
        SpokeRejectCode::RevisionConflict => CoreError::ActorConflict {
            code: "version_mismatch".to_string(),
            message: reject.message.clone(),
        },
        _ => internal_err("memory_provider", reject.message.clone()),
    }
}

/// The success body of a port result as JSON, or its refusal as a core error.
fn memory_ok<T: serde::Serialize>(result: SpokeResult<T>) -> CoreResult<Value> {
    match result {
        SpokeResult::Ok(value) => serde_json::to_value(&value).map_err(wire_err),
        SpokeResult::Reject(reject) => Err(memory_reject(&reject)),
    }
}

/// One pending-review row as the retained DTOs render it.
struct PendingMemoryRow {
    pending_id: String,
    session_id: String,
    task_kind: String,
    raw_digest: String,
    world_id: Option<String>,
    source_operation_id: Option<String>,
    binding_id: Option<String>,
    shared: bool,
    created_at: String,
    /// The provider candidate this row stands for: the review drain consumes it.
    candidate_id: String,
    /// The candidate revision the read served (the review CAS precondition).
    revision: u64,
}

impl PendingMemoryRow {
    /// Project one provider candidate onto the legacy pending-review row it
    /// stands for; a candidate with no legacy origin is not part of this DTO.
    fn from_candidate(candidate: &Value) -> Option<Self> {
        let origin = candidate.pointer("/proposed/content/structured/nexus_legacy")?;
        let scope = &candidate["capture_scope"];
        Some(Self {
            candidate_id: candidate["candidate_id"].as_str()?.to_string(),
            revision: candidate["revision"].as_u64().unwrap_or(1),
            pending_id: origin["pending_id"].as_str()?.to_string(),
            session_id: origin["session_id"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            task_kind: origin["task_kind"].as_str().unwrap_or_default().to_string(),
            raw_digest: origin["raw_digest"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            world_id: origin["world_id"].as_str().map(str::to_string),
            source_operation_id: origin["source_operation_id"].as_str().map(str::to_string),
            binding_id: scope["context_id"].as_str().map(str::to_string),
            shared: scope["visibility"].as_str() == Some("shared"),
            created_at: origin["created_at"]
                .as_str()
                .or_else(|| candidate["created_at"].as_str())
                .unwrap_or_default()
                .to_string(),
        })
    }
}

impl PendingMemoryRow {
    /// The E11 classifier input for this candidate: the retained pending-review
    /// fields, with the bearer's own id and its scope provenance.
    fn review_input(&self, bearer_id: &str) -> nexus_creator_memory::review::PendingReviewInput {
        nexus_creator_memory::review::PendingReviewInput {
            pending_id: self.pending_id.clone(),
            session_id: self.session_id.clone(),
            bearer_id: bearer_id.to_string(),
            // The Creator arm's world id, or the Character arm's binding id.
            scope_id: self.binding_id.clone().or_else(|| self.world_id.clone()),
            task_kind: self.task_kind.clone(),
            raw_digest: self.raw_digest.clone(),
            created_at: self.created_at.clone(),
        }
    }
}

/// Whether one projected pending row belongs to the requested retained scope:
/// `None` is the Character-shared queue, `Some(binding)` that binding's own.
fn pending_row_in_scope(row: &PendingMemoryRow, binding_id: Option<&str>) -> bool {
    binding_id.map_or(row.shared, |binding| {
        !row.shared && row.binding_id.as_deref() == Some(binding)
    })
}

/// One provider subject standing for a legacy memory bearer, plus the
/// authenticated principal that addresses it.
///
/// A subject opts into the bridge's legacy projection by naming its bearer in
/// its own kind (`legacy::CREATOR_KIND_NS` / `CHARACTER_KIND_NS`), so every E4
/// handler ensures that subject before it reads or mutates; the id comes from
/// the bridge's deterministic scheme, which P5's migration key shares.
///
/// E6's Character mind projection (`crate::soul`) reads through the same
/// adapter: one merged-read address for both entry points (NXM-7).
pub struct MemoryAdapter {
    provider: Arc<LocalMemoryProvider>,
    principal: spoke_schemas::Principal,
    subject_id: String,
}

impl MemoryAdapter {
    /// The adapter for one bearer, get-or-creating its provider subject.
    ///
    /// Ensuring the subject is the write that opts the bearer into the bridge's
    /// legacy projection, so it runs only on a writable provider: a read-only
    /// core (NXM-5 Task 2 fix round 1, finding F6) serves the projection of the
    /// subject an earlier writer open already ensured, and writes nothing.
    pub async fn open(
        provider: Arc<LocalMemoryProvider>,
        principal: &Principal,
        subject_id: String,
        kind_ns: &str,
        kind_id: &str,
    ) -> CoreResult<Self> {
        let spoke_principal: spoke_schemas::Principal =
            map_wire(json!({ "principal_id": principal.creator_id(), "extensions": {} }))?;
        if provider.is_writable() {
            let request: spoke_schemas::MemorySubjectsEnsureRequest = map_wire(json!({
                "space_id": MEMORY_SPACE_ID,
                "subject_id": subject_id,
                "kind": { "ns": kind_ns, "id": kind_id },
            }))?;
            if let SpokeResult::Reject(reject) =
                provider.ensure_subject(&spoke_principal, request).await
            {
                return Err(memory_reject(&reject));
            }
        }
        Ok(Self {
            provider,
            principal: spoke_principal,
            subject_id,
        })
    }

    /// The bearer's memory-disposition section: a review drain holds it across
    /// the consumed-check, the classification and the persistence of every row
    /// it processes (NXM-5 Task 2 fix round 2, finding F1).
    async fn review_claim(&self) -> CoreResult<MemoryDisposition> {
        self.provider.review_disposition(&self.subject_id).await
    }

    /// The provider subject this adapter speaks for: the bearer a write under
    /// this adapter may mutate (R4-F2).
    const fn subject_id(&self) -> &str {
        self.subject_id.as_str()
    }

    /// Every pending candidate of the subject, newest first in the retained
    /// `(created_at DESC, pending_id DESC)` order.
    ///
    /// Walks the provider's own bounded pages to the end of the authorized set:
    /// the retained DTO orders newest-first while the provider pages
    /// oldest-first, so the head of the retained order is the tail of the walk.
    /// There is no scan cap to fail on — a page that exists in the provider is
    /// served (fix round 1, finding F5).
    async fn pending_rows(&self) -> CoreResult<Vec<PendingMemoryRow>> {
        let mut rows = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let request: spoke_schemas::MemoryCandidatesListRequest = map_wire(json!({
                "space_id": MEMORY_SPACE_ID,
                "subject_id": self.subject_id,
                "status": "pending",
                "limit": MEMORY_PAGE_SIZE,
                "cursor": cursor,
            }))?;
            let body = memory_ok(
                self.provider
                    .list_candidates(&self.principal, request)
                    .await,
            )?;
            rows.extend(
                body["candidates"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(PendingMemoryRow::from_candidate),
            );
            match body["next_cursor"].as_str() {
                Some(next) => cursor = Some(next.to_string()),
                None => break,
            }
        }
        rows.sort_by(|left, right| {
            created_at_cmp(&right.created_at, &left.created_at)
                .then_with(|| right.pending_id.cmp(&left.pending_id))
        });
        Ok(rows)
    }

    /// The subject's records in the provider's `created_at DESC` order: at most
    /// `want` rows, or the whole subject set when `want` is `None`.
    ///
    /// A `None` bound is the honest one for a caller that filters after the read
    /// (the keyword scan): the provider has no keyword index, so stopping early
    /// would silently drop matches the retained SQL found (fix round 1, finding
    /// F5).
    async fn records(
        &self,
        selection: Value,
        kinds: &[&str],
        filter: Option<&[String]>,
        want: Option<usize>,
    ) -> CoreResult<Vec<Value>> {
        let mut rows: Vec<Value> = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let request: spoke_schemas::MemoryRecordsListRequest = map_wire(json!({
                "space_id": MEMORY_SPACE_ID,
                "subject_id": self.subject_id,
                "selection": selection,
                "kinds": kinds,
                "filter": filter.map(|ids| json!({ "source_context_ids": ids })),
                "limit": MEMORY_PAGE_SIZE,
                "cursor": cursor,
            }))?;
            let body = memory_ok(self.provider.list_records(&self.principal, request).await)?;
            rows.extend(body["records"].as_array().cloned().unwrap_or_default());
            if want.is_some_and(|want| rows.len() >= want) {
                break;
            }
            match body["next_cursor"].as_str() {
                Some(next) => cursor = Some(next.to_string()),
                None => break,
            }
        }
        Ok(rows)
    }

    /// One kind-filtered family read of the subject's retained records, as the
    /// upstream contract DTO (the data-target `RetainedRecord`) E6 and E10
    /// consume (NXM-7 fix round 2).
    ///
    /// `memory.recall` ranks every record kind together and offers no kind
    /// filter, so a family read through it has to share pages with unrelated
    /// kinds — events, the derived narrative cache — and any page budget can
    /// then silently truncate the family. `memory.records.list` takes the kind
    /// filter (`kinds`), which is the same authorized read path the E4 bridge
    /// uses. The family is read to exhaustion: a caller that selects inside it
    /// under the legacy `created_at` TEXT key cannot prove a bounded provider
    /// prefix holds every row that ranks under that key (the provider pages by
    /// parsed instant, which mixed formats can order differently — fix round 4,
    /// finding R4-F1).
    ///
    /// **Recall's subject-state rule (§7.7) is re-applied here:** `recall`
    /// excludes an archived subject's records while `memory.records.list` is the
    /// audit listing that keeps serving them, so this replacement for recall on
    /// the E6 path must exclude them itself (fix round 3, finding R3-F2).
    pub(crate) async fn family(
        &self,
        selection: Value,
        kinds: &[&str],
    ) -> CoreResult<Vec<spoke_schemas::RetainedRecord>> {
        if self.subject_archived().await? {
            return Ok(Vec::new());
        }
        self.records(selection, kinds, None, None)
            .await?
            .into_iter()
            .map(|record| serde_json::from_value(record).map_err(wire_err))
            .collect()
    }

    /// Whether the subject is `archived` — the one subject state that removes
    /// its records from recall (§7.7). A subject the provider has never seen
    /// carries no records either way, so its absence is not an archive.
    async fn subject_archived(&self) -> CoreResult<bool> {
        let request: spoke_schemas::MemorySubjectsGetRequest = map_wire(json!({
            "space_id": MEMORY_SPACE_ID,
            "subject_id": self.subject_id,
        }))?;
        match self.provider.get_subject(&self.principal, request).await {
            SpokeResult::Ok(body) => {
                let body = serde_json::to_value(&body).map_err(wire_err)?;
                Ok(body["subject"]["status"].as_str() == Some("archived"))
            }
            SpokeResult::Reject(reject) if reject.code == SpokeRejectCode::NotFound => Ok(false),
            SpokeResult::Reject(reject) => Err(memory_reject(&reject)),
        }
    }

    /// The subject's `personality` profile body — `SOUL.md` as the provider
    /// renders it. `None` is the honest-empty answer for an absent document.
    pub(crate) async fn personality(&self) -> CoreResult<Option<String>> {
        let request: spoke_schemas::MemoryProfileGetRequest = map_wire(json!({
            "space_id": MEMORY_SPACE_ID,
            "subject_id": self.subject_id,
            "profile_kinds": [SOUL_PROFILE_KIND],
        }))?;
        let body = memory_ok(self.provider.get_profile(&self.principal, request).await)?;
        Ok(body["profiles"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|profile| profile["profile_kind"] == SOUL_PROFILE_KIND)
            .and_then(|profile| profile["document"]["body"].as_str())
            .map(str::to_string))
    }

    /// One candidate by its provider id (the bridge's projected row included);
    /// `None` is the retained not-found answer.
    async fn candidate(&self, candidate_id: &str) -> CoreResult<Option<Value>> {
        let request: spoke_schemas::MemoryCandidatesGetRequest = map_wire(json!({
            "space_id": MEMORY_SPACE_ID,
            "candidate_id": candidate_id,
        }))?;
        match self.provider.get_candidate(&self.principal, request).await {
            SpokeResult::Ok(body) => Ok(serde_json::to_value(&body)
                .map_err(wire_err)?
                .get("candidate")
                .cloned()),
            SpokeResult::Reject(reject) if reject.code == SpokeRejectCode::NotFound => Ok(None),
            SpokeResult::Reject(reject) => Err(memory_reject(&reject)),
        }
    }

    /// Dismiss one legacy-derived pending row: adopt it under the bridge's
    /// deterministic id and consume it **in one guarded transaction** (NXM-5
    /// Task 2 fix round 1, finding F4).
    ///
    /// `Ok(false)` means nothing was pending under that id — absent, already
    /// consumed, or concurrently dismissed — which the handlers render as their
    /// retained not-found answer.
    async fn dismiss_legacy(&self, candidate_id: &str) -> CoreResult<bool> {
        let digest = match spoke_operations::jcs_sha256_hex(&json!(candidate_id)) {
            SpokeResult::Ok(digest) => digest,
            SpokeResult::Reject(reject) => return Err(memory_reject(&reject)),
        };
        match self
            .provider
            .dismiss_legacy_candidate(
                &self.principal,
                &self.subject_id,
                candidate_id,
                None,
                &format!("e4dismiss:{digest}"),
            )
            .await
        {
            SpokeResult::Ok(_) => Ok(true),
            // Only consumption bumps a candidate's revision, so a CAS miss is
            // the same answer as a consumed row: it is no longer pending (a
            // concurrent dismissal won), which the retained contract renders as
            // the row being gone.
            SpokeResult::Reject(reject)
                if matches!(
                    reject.code,
                    SpokeRejectCode::NotFound
                        | SpokeRejectCode::CandidateConsumed
                        | SpokeRejectCode::RevisionConflict
                ) =>
            {
                Ok(false)
            }
            SpokeResult::Reject(reject) => Err(memory_reject(&reject)),
        }
    }

    /// Dismiss one provider-native pending candidate (a row captured through
    /// [`CoreService::create_pending_review`]): its candidate id is minted, so
    /// the retained `pending_id` can only address it through the row itself.
    ///
    /// `Ok(false)` folds the same outcomes as [`Self::dismiss_legacy`]: absent,
    /// already consumed, or lost the revision race.
    async fn dismiss_native(&self, candidate_id: &str, revision: u64) -> CoreResult<bool> {
        let request: spoke_schemas::MemoryCandidatesDismissRequest = map_wire(json!({
            "space_id": MEMORY_SPACE_ID,
            "candidate_id": candidate_id,
            "expected_revision": revision,
            "idempotency_key": format!("e4dismiss-native:{candidate_id}"),
        }))?;
        match self
            .provider
            .dismiss_candidate(&self.principal, request)
            .await
        {
            SpokeResult::Ok(_) => Ok(true),
            SpokeResult::Reject(reject)
                if matches!(
                    reject.code,
                    SpokeRejectCode::NotFound
                        | SpokeRejectCode::CandidateConsumed
                        | SpokeRejectCode::RevisionConflict
                ) =>
            {
                Ok(false)
            }
            SpokeResult::Reject(reject) => Err(memory_reject(&reject)),
        }
    }

    /// Consume one pending candidate with the review decision the drain built,
    /// under the bearer's disposition section the caller already holds.
    ///
    /// The decision is the §7.15 wire decision (`promote` / `fragment` /
    /// `drop`). A legacy-derived candidate is adopted under the bridge's
    /// deterministic id inside the same guarded transaction that consumes it
    /// (NXM-5's mutation-time adoption, reused — not redefined — by NXM-8).
    pub(crate) async fn review(
        &self,
        candidate_id: &str,
        expected_revision: u64,
        decision: Value,
        section: &MemoryDisposition,
    ) -> CoreResult<()> {
        let request: spoke_schemas::MemoryCandidatesReviewRequest = map_wire(json!({
            "space_id": MEMORY_SPACE_ID,
            "candidate_id": candidate_id,
            "expected_revision": expected_revision,
            "decision": decision,
            "idempotency_key": idempotency_key("review", candidate_id)?,
        }))?;
        match self
            .provider
            .review_legacy_candidate(&self.principal, &self.subject_id, request, section)
            .await
        {
            SpokeResult::Ok(_) => Ok(()),
            SpokeResult::Reject(reject) => Err(memory_reject(&reject)),
        }
    }

    /// Every Creator long-term memory record (the nine authored kinds only), in
    /// the provider's `created_at DESC` order.
    async fn memory_records(&self) -> CoreResult<Vec<Value>> {
        let kinds = creator_record_kinds();
        let kinds: Vec<&str> = kinds.iter().map(String::as_str).collect();
        self.records(json!({ "mode": "shared_only" }), &kinds, None, None)
            .await
    }

    /// One long-term memory record by its authored slug.
    ///
    /// The slug is authored content (`nexus_legacy.slug`) and the provider has
    /// no slug index, so the match happens after the read — exactly the
    /// directory scan the CLI used to perform.
    async fn memory_by_slug(&self, slug: &str) -> CoreResult<Option<Value>> {
        Ok(self
            .memory_records()
            .await?
            .into_iter()
            .find(|record| creator_slug(record) == Some(slug)))
    }

    /// The Creator's `personality` profile (`SOUL.md`), if authored.
    async fn personality_profile(&self) -> CoreResult<Option<Value>> {
        let request: spoke_schemas::MemoryProfileGetRequest = map_wire(json!({
            "space_id": MEMORY_SPACE_ID,
            "subject_id": self.subject_id,
            "profile_kinds": [profile::SOUL_PROFILE_KIND],
        }))?;
        let body = memory_ok(self.provider.get_profile(&self.principal, request).await)?;
        Ok(body["profiles"]
            .as_array()
            .and_then(|profiles| profiles.first())
            .cloned())
    }

    /// Create one Creator long-term memory record (shared placement).
    async fn create_memory_record(
        &self,
        kind: &str,
        content: Value,
        idempotency_key: &str,
    ) -> CoreResult<()> {
        let request: spoke_schemas::MemoryRecordsCreateRequest = map_wire(json!({
            "space_id": MEMORY_SPACE_ID,
            "subject_id": self.subject_id,
            "kind": kind,
            "content": content,
            "scope": { "visibility": "shared" },
            "source_context_ids": [],
            "idempotency_key": idempotency_key,
        }))?;
        memory_ok(self.provider.create_record(&self.principal, request).await).map(|_| ())
    }

    /// Replace one Creator record's content on the expected revision.
    async fn update_memory_record(
        &self,
        record_id: &str,
        expected_revision: u64,
        content: Value,
        idempotency_key: &str,
    ) -> CoreResult<()> {
        let request: spoke_schemas::MemoryRecordsUpdateRequest = map_wire(json!({
            "space_id": MEMORY_SPACE_ID,
            "record_id": record_id,
            "expected_revision": expected_revision,
            "content": content,
            "idempotency_key": idempotency_key,
        }))?;
        memory_ok(self.provider.update_record(&self.principal, request).await).map(|_| ())
    }

    /// Tombstone one Creator record on the expected revision.
    async fn delete_memory_record(
        &self,
        record_id: &str,
        expected_revision: u64,
        idempotency_key: &str,
    ) -> CoreResult<()> {
        let request: spoke_schemas::MemoryRecordsDeleteRequest = map_wire(json!({
            "space_id": MEMORY_SPACE_ID,
            "record_id": record_id,
            "expected_revision": expected_revision,
            "reason": "deleted through the creator memory CLI",
            "idempotency_key": idempotency_key,
        }))?;
        memory_ok(self.provider.delete_record(&self.principal, request).await).map(|_| ())
    }

    /// Write the `personality` profile document.
    ///
    /// A *projected* `SOUL.md` (unmigrated) has no provider row, so its
    /// projected revision is not a stored one: the revision-checked write cannot
    /// meet it, while the create-only write can — and that write can only
    /// succeed when no stored row exists, so a genuine stale-revision conflict
    /// still surfaces instead of being papered over. The bridge keeps a stored
    /// kind over the projection, so the adoption replaces the projected document
    /// rather than duplicating it.
    async fn write_personality_profile(
        &self,
        body: &str,
        expected_revision: Option<u64>,
        idempotency_key: &str,
    ) -> CoreResult<()> {
        match self
            .upsert_personality_profile(body, expected_revision, idempotency_key)
            .await
        {
            Err(error) if expected_revision.is_some() && memory_conflict(&error) => {
                self.upsert_personality_profile(body, None, &format!("{idempotency_key}:adopt"))
                    .await
            }
            other => other,
        }
    }

    /// One `memory.profile.update` at the exact expected revision.
    async fn upsert_personality_profile(
        &self,
        body: &str,
        expected_revision: Option<u64>,
        idempotency_key: &str,
    ) -> CoreResult<()> {
        let request: spoke_schemas::MemoryProfileUpdateRequest = map_wire(json!({
            "space_id": MEMORY_SPACE_ID,
            "subject_id": self.subject_id,
            "profile_kind": profile::SOUL_PROFILE_KIND,
            "expected_revision": expected_revision,
            "document": { "format": "markdown", "body": body },
            "idempotency_key": idempotency_key,
        }))?;
        memory_ok(self.provider.update_profile(&self.principal, request).await).map(|_| ())
    }

    /// One provider record (a projected legacy row included) by id.
    async fn record(&self, record_id: &str) -> CoreResult<Option<Value>> {
        let request: spoke_schemas::MemoryRecordsGetRequest = map_wire(json!({
            "space_id": MEMORY_SPACE_ID,
            "record_id": record_id,
        }))?;
        match self.provider.get_record(&self.principal, request).await {
            SpokeResult::Ok(body) => Ok(serde_json::to_value(&body)
                .map_err(wire_err)?
                .get("record")
                .cloned()),
            SpokeResult::Reject(reject) if reject.code == SpokeRejectCode::NotFound => Ok(None),
            SpokeResult::Reject(reject) => Err(memory_reject(&reject)),
        }
    }

    /// One Character fragment record by the id the fragment DTO serves.
    ///
    /// A provider-native fragment is addressed by its own record id; a projected
    /// legacy fragment is addressed by the bridge's deterministic external id
    /// while the DTO serves its legacy `fragment_id` (D5), so the listed id is
    /// resolved through the bridge id before it can name a record (R4-F1).
    async fn character_fragment(&self, fragment_id: &str) -> CoreResult<Option<Value>> {
        if let Some(record) = self.record(fragment_id).await? {
            return Ok(Some(record));
        }
        self.record(&legacy::character_fragment_record_id(fragment_id))
            .await
    }

    /// §7.21 explicit placement change to shared, revision-checked, adopting a
    /// still-projected legacy row inside the promotion's transaction. A CAS miss
    /// keeps the retained `version_mismatch` conflict; an absent record keeps
    /// the retained not-found answer.
    async fn promote_fragment(&self, record_id: &str, expected_revision: u64) -> CoreResult<()> {
        match self
            .provider
            .promote_character_fragment(
                &self.principal,
                &self.subject_id,
                record_id,
                expected_revision,
                &idempotency_key("promote", record_id)?,
            )
            .await
        {
            SpokeResult::Ok(_) => Ok(()),
            SpokeResult::Reject(reject) => Err(match reject.code {
                SpokeRejectCode::NotFound => {
                    not_found("character memory fragment", &format!("'{record_id}'"))
                }
                SpokeRejectCode::RevisionConflict => CoreError::ActorConflict {
                    code: "version_mismatch".to_string(),
                    message: format!("fragment revision mismatch: {}", reject.message),
                },
                _ => memory_reject(&reject),
            }),
        }
    }

    /// The placement a committed promotion left for one record, while the
    /// SOUL-narrative invalidation it owes has not run (R4-F3 round 2).
    async fn soul_invalidation_binding(&self, record_id: &str) -> CoreResult<Option<String>> {
        self.provider.soul_invalidation_binding(record_id).await
    }

    /// Clear that recovery row once the invalidation ran.
    async fn finish_soul_invalidation(&self, record_id: &str) -> CoreResult<()> {
        self.provider.finish_soul_invalidation(record_id).await
    }
}

/// A deterministic idempotency key for one memory write: `<op>:<digest>`.
///
/// The provider's key namespace is `(space, requester, op, key)` and a key is
/// bounded at 128 characters, so an object id — which can be a long projected
/// legacy id — rides as its digest instead.
fn idempotency_key(op: &str, object_id: &str) -> CoreResult<String> {
    match spoke_operations::jcs_sha256_hex(&json!(object_id)) {
        SpokeResult::Ok(digest) => Ok(format!("{op}:{digest}")),
        SpokeResult::Reject(reject) => Err(memory_reject(&reject)),
    }
}

// ── Review disposition authority (NXM-5 Task 2 fix round 2) ──────────────
//
// A provider dismissal and a legacy review drain decide the same digest, so
// both hold the bearer's [`MemoryDisposition`] section: the drain across its
// consumed-check and classification, the dismissal across its consume. The
// step-point below lets a regression drive the exact interleaving a dismissal
// would need and observe the section is not available to it.

/// A step-point inside a review drain's disposition section (test builds only).
#[cfg(feature = "test-hooks")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryReviewPause {
    /// The fetch-time check selected the slice; the drain is about to take the
    /// bearer's disposition section and re-check it there — the exact window a
    /// dismissal could otherwise slip into.
    AfterFetchCheck,
    /// The slice was re-checked inside the section and is about to be
    /// classified and persisted.
    AfterRecheck,
}

/// The rendezvous a test arms for one [`MemoryReviewPause`].
#[cfg(feature = "test-hooks")]
#[derive(Debug, Clone)]
pub struct MemoryReviewRendezvous {
    /// The step-point this rendezvous arms.
    pub at: MemoryReviewPause,
    /// Fires once the drain is parked at `at`.
    pub entered: Arc<tokio::sync::Notify>,
    /// Fires to resume the parked drain.
    pub release: Arc<tokio::sync::Notify>,
}

#[cfg(feature = "test-hooks")]
impl CoreService {
    /// Test-only: park the next Creator or Character review drain at
    /// `rendezvous.at`. Compiled out of a production build.
    #[doc(hidden)]
    pub fn pause_memory_review(&self, rendezvous: MemoryReviewRendezvous) {
        *self
            .inner
            .memory_review_pause
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(rendezvous);
    }

    /// Test-only: fail the next retained SOUL-narrative invalidation, which runs
    /// after its promotion already committed, so a regression can drive the
    /// failure/retry boundary (R4-F3). Compiled out of a production build.
    #[doc(hidden)]
    pub fn fail_next_soul_narrative_invalidation(&self) {
        self.inner
            .soul_narrative_invalidation_fault
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// Park at `at` when a test armed that step-point.
    async fn park_memory_review(&self, at: MemoryReviewPause) {
        // Only the armed step-point consumes the rendezvous: the drain walks
        // every step-point, and taking a later one's rendezvous here would drop
        // it before the drain reached it.
        let armed = {
            let mut slot = self
                .inner
                .memory_review_pause
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if slot.as_ref().is_some_and(|rendezvous| rendezvous.at == at) {
                slot.take()
            } else {
                None
            }
        };
        if let Some(rendezvous) = armed {
            rendezvous.entered.notify_one();
            rendezvous.release.notified().await;
        }
    }
}

impl CoreService {
    /// The provider adapter for the principal's own Creator bearer, over the
    /// pinned `MemoryPort` (a read-only core serves reads from it too).
    async fn creator_memory(&self, principal: &Principal) -> CoreResult<MemoryAdapter> {
        MemoryAdapter::open(
            self.memory_provider()?,
            principal,
            legacy::creator_subject_id(principal.creator_id()),
            legacy::CREATOR_KIND_NS,
            principal.creator_id(),
        )
        .await
    }

    /// The adapter the dismissal write paths use: a read-only core refuses here
    /// with the retained typed error instead of attempting the write.
    async fn creator_memory_writer(&self, principal: &Principal) -> CoreResult<MemoryAdapter> {
        MemoryAdapter::open(
            self.memory_writer()?,
            principal,
            legacy::creator_subject_id(principal.creator_id()),
            legacy::CREATOR_KIND_NS,
            principal.creator_id(),
        )
        .await
    }

    /// The provider adapter for one Character bearer.
    async fn character_memory(
        &self,
        principal: &Principal,
        character_id: &str,
    ) -> CoreResult<MemoryAdapter> {
        MemoryAdapter::open(
            self.memory_provider()?,
            principal,
            legacy::character_subject_id(character_id),
            legacy::CHARACTER_KIND_NS,
            character_id,
        )
        .await
    }

    /// The Character adapter the dismissal write path uses (see
    /// [`Self::creator_memory_writer`]).
    async fn character_memory_writer(
        &self,
        principal: &Principal,
        character_id: &str,
    ) -> CoreResult<MemoryAdapter> {
        MemoryAdapter::open(
            self.memory_writer()?,
            principal,
            legacy::character_subject_id(character_id),
            legacy::CHARACTER_KIND_NS,
            character_id,
        )
        .await
    }
}

/// Whether the provider currently reports this candidate as pending.
async fn pending_now(memory: &MemoryAdapter, candidate_id: &str) -> CoreResult<bool> {
    Ok(memory
        .candidate(candidate_id)
        .await?
        .is_some_and(|candidate| candidate["status"].as_str() == Some("pending")))
}

/// The subset of `candidate_ids` the provider has already consumed.
///
/// A dismissal consumes the provider's row while the legacy row the bridge
/// projected it from stays in place (a provider write never touches the legacy
/// surface — compass D6), so the legacy review drain would otherwise classify a
/// digest the caller explicitly rejected. This is the dismissal-vs-drain
/// authority check (NXM-5 Task 2 fix round 1, finding F1), and it is a plain
/// provider read: no journal, no receipt, no write.
///
/// A row the provider cannot see (never projected, or hidden by authorization)
/// is *not* consumed: the drain keeps the behavior it had before the reroute.
async fn consumed_candidates(
    memory: &MemoryAdapter,
    candidate_ids: impl IntoIterator<Item = String>,
) -> CoreResult<HashSet<String>> {
    let mut consumed = HashSet::new();
    for candidate_id in candidate_ids {
        match memory.candidate(&candidate_id).await? {
            Some(candidate) if candidate["status"].as_str() != Some("pending") => {
                consumed.insert(candidate_id);
            }
            _ => {}
        }
    }
    Ok(consumed)
}

/// The `nexus_legacy` metadata block of a projected role, if this row carries
/// one (a provider-native row of another origin does not).
fn nexus_legacy(record: &Value) -> &Value {
    record
        .pointer("/content/structured/nexus_legacy")
        .unwrap_or(&NULL_LEGACY)
}

/// Absent `nexus_legacy` block: indexing it yields `null` for every field, so
/// the readers below can stay branch-free.
static NULL_LEGACY: Value = Value::Null;

fn record_keywords(record: &Value) -> Vec<String> {
    nexus_legacy(record)["keywords"]
        .as_array()
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// The world provenance of one Creator record: the legacy column when the row
/// is projected, else the record's own source context.
fn creator_world_id(record: &Value) -> Option<String> {
    nexus_legacy(record)["world_id"]
        .as_str()
        .map(str::to_string)
        .or_else(|| {
            record
                .pointer("/provenance/source_context_ids/0")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
}

// ── Creator long-term memory + SOUL profile family (NXM-6/NXM-16) ────────
//
// The CLI's E1 (`creator memory list|create|show|edit|delete`) and E2
// (`creator soul …`) leaves used to reach the bearer files directly. They are
// provider clients now: a Creator's long-term memories are the `nexus.*`
// records the nine-kind bijection names (D-7), and `SOUL.md` is the one
// authored `personality` profile (§6.8, D-12). The bridge projects both from
// the unmigrated files, so one read path serves migrated and not-yet-migrated
// content alike (D6); every write lands on the provider, never on the legacy
// surface.
//
// The authored identity the CLI renders — the memory slug, its `memory_id` and
// the authored kind — rides in `content.structured.nexus_legacy`, the same
// block the bridge projects, so one rendered row reads the same before and
// after P5's migration.

/// The `nexus_legacy` slug of a Creator record.
fn creator_slug(record: &Value) -> Option<&str> {
    nexus_legacy(record)["slug"].as_str()
}

/// The nine authored record kinds, namespaced (D-7's lossless bijection).
fn creator_record_kinds() -> Vec<String> {
    policy::NEXUS_MEMORY_KINDS
        .iter()
        .map(|kind| policy::namespaced_record_kind(kind))
        .collect()
}

/// `task_kind` → authored memory kind: the promotion mapping the review
/// pipeline documents (`nexus_creator_memory::review::promote_to_long_term`).
fn task_kind_to_memory_kind(task_kind: &str) -> &'static str {
    match task_kind.to_lowercase().as_str() {
        "brainstorm" | "chapter" => "story_summary",
        "outline" => "plot_outline",
        "research" => "research_material",
        _ => "custom",
    }
}

/// A stable memory idempotency key: the operation, its subject and its payload
/// decide the key, so a retried write replays instead of duplicating.
fn creator_memory_key(op: &str, subject: &str, body: &str) -> String {
    match spoke_operations::jcs_sha256_hex(&json!({ "op": op, "subject": subject, "body": body })) {
        SpokeResult::Ok(digest) => format!("e4memory:{digest}"),
        // The three-string payload always canonicalizes; keep the spelling
        // total rather than reporting an unreachable interior failure.
        SpokeResult::Reject(_) => format!("e4memory:{op}:{subject}"),
    }
}

/// A provider CAS refusal (`expected_revision` did not match the stored one).
fn memory_conflict(error: &CoreError) -> bool {
    matches!(error, CoreError::ActorConflict { code, .. } if code == "version_mismatch")
}

/// The authored memory id of a freshly created record: `mem_<uuid>`, the
/// frontmatter identity `LongTermMemory` mints and P5's migration preserves.
fn mint_creator_memory_id() -> String {
    format!("mem_{}", uuid::Uuid::new_v4().to_string().replace('-', ""))
}

/// The provider identity and CAS revision of one record body.
fn creator_record_identity(record: &Value) -> CoreResult<(String, u64)> {
    let record_id = record["record_id"]
        .as_str()
        .ok_or_else(|| internal_err("memory_record", "a provider record carries no record_id"))?;
    Ok((record_id.to_string(), record["revision"].as_u64().unwrap_or(1)))
}

/// The authored identity of one Creator memory record: `(memory_id, kind,
/// source_session_ids)`, from the `nexus_legacy` block the bridge projects.
fn creator_memory_identity(record: &Value) -> (String, String, Vec<String>) {
    let legacy = nexus_legacy(record);
    let memory_id = legacy["memory_id"].as_str().map_or_else(
        || {
            record["record_id"]
                .as_str()
                .unwrap_or_default()
                .to_string()
        },
        str::to_string,
    );
    let kind = legacy["memory_kind"]
        .as_str()
        .map(str::to_string)
        .or_else(|| {
            record["kind"]
                .as_str()
                .and_then(policy::memory_kind_of)
                .map(str::to_string)
        })
        .unwrap_or_else(|| "custom".to_string());
    let source_session_ids = legacy["source_session_ids"]
        .as_array()
        .map(|ids| {
            ids.iter()
                .filter_map(|id| id.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    (memory_id, kind, source_session_ids)
}

/// The record content of one authored long-term memory.
fn creator_memory_content(
    slug: &str,
    kind: &str,
    memory_id: &str,
    body: &str,
    source_session_ids: &[String],
) -> Value {
    json!({
        "text": body,
        "structured": { "nexus_legacy": {
            "slug": slug,
            "memory_id": memory_id,
            "memory_kind": kind,
            "nexus_memory_version": 1,
            "updated_at": chrono::Utc::now().to_rfc3339(),
            "source_session_ids": source_session_ids,
        }},
    })
}

/// Memory kinds that feed the SOUL `## Experience` section (the aggregation's
/// authored set).
const EXPERIENCE_MEMORY_KINDS: [&str; 6] = [
    "story_summary",
    "review_note",
    "character_note",
    "world_building",
    "plot_outline",
    "theme_analysis",
];

/// Longest body excerpt one experience entry carries.
const EXPERIENCE_EXCERPT_CHARS: usize = 200;

/// The deterministic `## Experience` body for a set of Creator records, and how
/// many memories contributed.
///
/// shortcut: the rendering mirrors
/// `nexus_creator_memory::experience_aggregation` (whose collector reads the
/// legacy files); NXM-14 deletes that reader, leaving this as the only copy.
fn experience_markdown(records: &[Value]) -> (String, usize) {
    let mut entries: Vec<ExperienceEntry> = records
        .iter()
        .filter_map(|record| {
            let (_, kind, _) = creator_memory_identity(record);
            if !EXPERIENCE_MEMORY_KINDS.contains(&kind.as_str()) {
                return None;
            }
            let legacy = nexus_legacy(record);
            Some(ExperienceEntry {
                memory_kind: kind,
                slug: legacy["slug"].as_str().unwrap_or_default().to_string(),
                body: record
                    .pointer("/content/text")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                updated_at: legacy["updated_at"].as_str().map_or_else(
                    || {
                        record["updated_at"]
                            .as_str()
                            .unwrap_or_default()
                            .to_string()
                    },
                    str::to_string,
                ),
            })
        })
        .collect();
    entries.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    let processed = entries.len();
    if entries.is_empty() {
        return (String::new(), 0);
    }
    let mut parts = Vec::new();
    for entry in &entries {
        parts.push(format!(
            "### {}: {}",
            experience_kind_label(&entry.memory_kind),
            entry.slug
        ));
        parts.push(experience_excerpt(&entry.body));
    }
    (parts.join("\n\n"), processed)
}

/// `story_summary` → `Story Summary`: the heading the aggregation renders.
fn experience_kind_label(kind: &str) -> String {
    kind.split('_')
        .map(|word| {
            let mut chars = word.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().collect::<String>() + chars.as_str()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Truncate an entry body the way the aggregation does: the first
/// [`EXPERIENCE_EXCERPT_CHARS`] characters, broken at the last space, `...`.
fn experience_excerpt(body: &str) -> String {
    if body.chars().count() <= EXPERIENCE_EXCERPT_CHARS {
        return body.to_string();
    }
    let end: String = body.chars().take(EXPERIENCE_EXCERPT_CHARS).collect();
    end.rfind(' ')
        .map_or_else(|| format!("{end}..."), |pos| format!("{}...", &end[..pos]))
}

impl CoreService {
    /// Capture one session digest into the principal's pending-review queue.
    ///
    /// This is the Creator arm of the Character capture family and the explicit
    /// capture parity NXM-16 adds: Host auto-`remember` stays refused (D-4), and
    /// the write lands on the pinned `MemoryPort` like every other memory
    /// mutation, so the candidate the review family consumes is the provider's
    /// own row.
    ///
    /// The capture is mapped as the plan pins it: shared placement, `world_id`
    /// as the provenance context, `session_id` as a source anchor, `task_kind`
    /// through the promotion mapping as the kind hint, and the retained capture
    /// fields in `content.structured.nexus_legacy` so the pending-review DTOs
    /// keep rendering them.
    ///
    /// # Errors
    /// [`CoreError::ForbiddenReason`] when `request.creator_id` is not the
    /// admitted principal, `ActorInput` for a malformed capture field, and the
    /// mapped storage error otherwise.
    pub async fn create_pending_review(
        &self,
        principal: &Principal,
        request: CreatePendingReviewRequest,
    ) -> CoreResult<CreatePendingReviewResponse> {
        self.verify_principal(principal)?;
        if request.creator_id.as_str() != principal.creator_id() {
            return Err(CoreError::ForbiddenReason {
                resource: "pending_review".to_string(),
                reason: format!(
                    "creator_id '{}' does not match active creator '{}'",
                    request.creator_id,
                    principal.creator_id()
                ),
            });
        }
        let pending_id = request.pending_id.as_str().to_string();
        let session_id = request.session_id.as_str().to_string();
        let world_id = request.world_id.as_deref().map(str::to_string);
        let task_kind = request
            .task_kind
            .as_deref()
            .unwrap_or("unknown")
            .to_string();
        let raw_digest = request.raw_digest.as_str().to_string();
        let created_at = request.created_at.as_deref().map_or_else(
            || chrono::Utc::now().to_rfc3339(),
            str::to_string,
        );
        validate_capture_fields(
            &pending_id,
            &session_id,
            &raw_digest,
            Some(&task_kind),
            world_id.as_deref(),
        )?;

        let memory = self.creator_memory_writer(principal).await?;
        let digest = match spoke_operations::jcs_sha256_hex(&json!(pending_id)) {
            SpokeResult::Ok(digest) => digest,
            SpokeResult::Reject(reject) => return Err(memory_reject(&reject)),
        };
        let capture: spoke_schemas::MemoryCaptureRequest = map_wire(json!({
            "space_id": MEMORY_SPACE_ID,
            "subject_id": memory.subject_id.as_str(),
            "content": {
                "text": raw_digest.as_str(),
                "structured": { "nexus_legacy": {
                    "pending_id": pending_id.as_str(),
                    "session_id": session_id.as_str(),
                    "creator_id": principal.creator_id(),
                    "world_id": world_id.as_deref(),
                    "task_kind": task_kind.as_str(),
                    "raw_digest": raw_digest.as_str(),
                    "created_at": created_at.as_str(),
                }},
            },
            "capture_scope": { "visibility": "shared" },
            "source_context_ids": world_id.iter().collect::<Vec<_>>(),
            "anchors": [{
                "source_id": session_id.as_str(),
                "schema_version": 1,
                "extensions": {},
            }],
            "kind_hint": task_kind_to_memory_kind(&task_kind),
            "idempotency_key": format!("e4capture:{digest}"),
        }))?;
        memory_ok(memory.provider.capture(&memory.principal, capture).await)?;
        map_wire(json!({ "success": true, "pending_id": pending_id }))
    }

    /// Every Creator long-term memory record, newest first.
    ///
    /// # Errors
    /// [`CoreError::AuthRequired`] when the principal fails verification and
    /// the mapped storage error otherwise.
    pub async fn list_creator_memories(
        &self,
        principal: &Principal,
    ) -> CoreResult<Vec<spoke_schemas::RetainedRecord>> {
        self.verify_principal(principal)?;
        let memory = self.creator_memory(principal).await?;
        memory
            .memory_records()
            .await?
            .iter()
            .map(map_wire)
            .collect()
    }

    /// One Creator long-term memory record by its authored slug.
    ///
    /// # Errors
    /// As [`Self::list_creator_memories`].
    pub async fn get_creator_memory(
        &self,
        principal: &Principal,
        slug: &str,
    ) -> CoreResult<Option<spoke_schemas::RetainedRecord>> {
        self.verify_principal(principal)?;
        let memory = self.creator_memory(principal).await?;
        memory
            .memory_by_slug(slug)
            .await?
            .as_ref()
            .map(map_wire)
            .transpose()
    }

    /// Create one Creator long-term memory from its authored slug, kind and
    /// body. `Ok(false)` reports that the slug is already taken — the retained
    /// CLI refusal — and writes nothing.
    ///
    /// # Errors
    /// As [`Self::list_creator_memories`].
    pub async fn create_creator_memory(
        &self,
        principal: &Principal,
        slug: &str,
        kind: &str,
        body: &str,
    ) -> CoreResult<bool> {
        self.verify_principal(principal)?;
        let memory = self.creator_memory_writer(principal).await?;
        if memory.memory_by_slug(slug).await?.is_some() {
            return Ok(false);
        }
        let content =
            creator_memory_content(slug, kind, &mint_creator_memory_id(), body, &[]);
        memory
            .create_memory_record(
                &policy::namespaced_record_kind(kind),
                content,
                &creator_memory_key("create", slug, body),
            )
            .await?;
        Ok(true)
    }

    /// Replace one Creator memory's body, addressed by its authored slug, on the
    /// revision read with it.
    ///
    /// `Ok(false)` reports that no mutable record carries the slug: absent, or
    /// still the bridge's projection of an unmigrated file — a projection is
    /// read-only by design (`legacy.rs`), and P5's migration is what turns it
    /// into a mutable provider row with the same identity.
    ///
    /// # Errors
    /// As [`Self::list_creator_memories`].
    pub async fn update_creator_memory(
        &self,
        principal: &Principal,
        slug: &str,
        body: &str,
    ) -> CoreResult<bool> {
        self.verify_principal(principal)?;
        let memory = self.creator_memory_writer(principal).await?;
        let Some(record) = memory.memory_by_slug(slug).await? else {
            return Ok(false);
        };
        let (record_id, revision) = creator_record_identity(&record)?;
        let (memory_id, kind, source_session_ids) = creator_memory_identity(&record);
        let content = creator_memory_content(slug, &kind, &memory_id, body, &source_session_ids);
        match memory
            .update_memory_record(
                &record_id,
                revision,
                content,
                &creator_memory_key("update", slug, body),
            )
            .await
        {
            Ok(()) => Ok(true),
            Err(CoreError::NotFound { .. }) => Ok(false),
            Err(error) => Err(error),
        }
    }

    /// Delete one Creator memory, addressed by its authored slug.
    ///
    /// `Ok(false)` carries the same answer as [`Self::update_creator_memory`]:
    /// no mutable record carries the slug.
    ///
    /// # Errors
    /// As [`Self::list_creator_memories`].
    pub async fn delete_creator_memory(
        &self,
        principal: &Principal,
        slug: &str,
    ) -> CoreResult<bool> {
        self.verify_principal(principal)?;
        let memory = self.creator_memory_writer(principal).await?;
        let Some(record) = memory.memory_by_slug(slug).await? else {
            return Ok(false);
        };
        let (record_id, revision) = creator_record_identity(&record)?;
        match memory
            .delete_memory_record(
                &record_id,
                revision,
                &creator_memory_key("delete", slug, ""),
            )
            .await
        {
            Ok(()) => Ok(true),
            Err(CoreError::NotFound { .. }) => Ok(false),
            Err(error) => Err(error),
        }
    }

    /// The Creator's authored SOUL document (the `personality` profile).
    ///
    /// # Errors
    /// As [`Self::list_creator_memories`].
    pub async fn get_creator_soul(
        &self,
        principal: &Principal,
    ) -> CoreResult<Option<spoke_schemas::ProfileDocument>> {
        self.verify_principal(principal)?;
        let memory = self.creator_memory(principal).await?;
        memory
            .personality_profile()
            .await?
            .as_ref()
            .map(map_wire)
            .transpose()
    }

    /// Write the Creator's SOUL document body: create-only when
    /// `expected_revision` is `None`, revision-checked otherwise.
    ///
    /// # Errors
    /// As [`Self::list_creator_memories`], plus the provider's CAS conflict.
    pub async fn set_creator_soul(
        &self,
        principal: &Principal,
        body: &str,
        expected_revision: Option<u64>,
    ) -> CoreResult<()> {
        self.verify_principal(principal)?;
        let memory = self.creator_memory_writer(principal).await?;
        memory
            .write_personality_profile(
                body,
                expected_revision,
                &creator_memory_key("soul", &expected_revision.map_or_else(String::new, |r| r.to_string()), body),
            )
            .await
    }

    /// Refresh the SOUL `## Experience` section from the Creator's memory
    /// records (deterministic, no LLM). Returns the rendered section and how
    /// many memories contributed.
    ///
    /// # Errors
    /// [`CoreError::NotFound`] when no SOUL document is authored, plus the
    /// mapped storage error.
    pub async fn refresh_creator_experience(
        &self,
        principal: &Principal,
    ) -> CoreResult<(String, usize)> {
        self.verify_principal(principal)?;
        let memory = self.creator_memory_writer(principal).await?;
        let Some(profile) = memory.personality_profile().await? else {
            return Err(CoreError::NotFound {
                resource: format!("SOUL.md for creator '{}'", principal.creator_id()),
            });
        };
        let body = profile.pointer("/document/body").and_then(Value::as_str);
        let mut soul = SoulDocument::parse(body.unwrap_or_default())
            .map_err(|error| internal_err("soul_parse", error))?;
        let (markdown, processed) = experience_markdown(&memory.memory_records().await?);
        soul.set_experience(markdown.clone());
        memory
            .write_personality_profile(
                &soul.render(),
                profile["revision"].as_u64(),
                &creator_memory_key("experience", "", &markdown),
            )
            .await?;
        Ok((markdown, processed))
    }

    /// Push the SOUL personality section down into the `personality-core`
    /// long-term memory. The SOUL document is the anchor, so its next push
    /// overwrites the memory (the authored `memory_id` and the empty
    /// `source_session_ids` are preserved).
    ///
    /// # Errors
    /// [`CoreError::ActorInput`] for an empty section, plus the mapped storage
    /// error.
    pub async fn push_creator_personality(
        &self,
        principal: &Principal,
        personality: &str,
    ) -> CoreResult<spoke_schemas::RetainedRecord> {
        self.verify_principal(principal)?;
        let personality = personality.trim();
        if personality.is_empty() {
            return Err(invalid_input(
                "cannot push an empty personality section to memory",
            ));
        }
        let memory = self.creator_memory_writer(principal).await?;
        let existing = memory.memory_by_slug(PERSONALITY_MEMORY_SLUG).await?;
        let key = creator_memory_key("personality", PERSONALITY_MEMORY_SLUG, personality);
        let memory_id = if let Some(record) = &existing {
            let (record_id, revision) = creator_record_identity(record)?;
            let (memory_id, _, _) = creator_memory_identity(record);
            let content = creator_memory_content(
                PERSONALITY_MEMORY_SLUG,
                "personality_core",
                &memory_id,
                personality,
                &[],
            );
            memory
                .update_memory_record(&record_id, revision, content, &key)
                .await?;
            memory_id
        } else {
            let memory_id = mint_creator_memory_id();
            let content = creator_memory_content(
                PERSONALITY_MEMORY_SLUG,
                "personality_core",
                &memory_id,
                personality,
                &[],
            );
            memory
                .create_memory_record(
                    &policy::namespaced_record_kind("personality_core"),
                    content,
                    &key,
                )
                .await?;
            memory_id
        };
        let record = memory
            .memory_by_slug(PERSONALITY_MEMORY_SLUG)
            .await?
            .ok_or_else(|| CoreError::NotFound {
                resource: format!("memory '{PERSONALITY_MEMORY_SLUG}' ({memory_id})"),
            })?;
        map_wire(record)
    }
}

// ── Character memory family ──────────────────────────────────────────────

impl CoreService {
    /// Capture one session digest into an owned Character's pending-review
    /// queue. Holds the per-Character activity lease across the guarded
    /// insert (404 foreign/missing, 409 `character_inactive` for an archived
    /// Character).
    ///
    /// # Errors
    /// As [`Self::acquire_actor_activity`], plus `invalid_input` for malformed
    /// capture fields and the mapped storage error otherwise.
    pub async fn capture_character_pending_review(
        &self,
        principal: &Principal,
        character_id: String,
        request: CaptureCharacterPendingReviewRequest,
    ) -> CoreResult<CaptureCharacterPendingReviewResponse> {
        self.verify_principal(principal)?;
        let binding_id = optional_str(request.binding_id.as_ref());
        // The activity lease is admitted before input validation, exactly as
        // the daemon handler admitted it before local-db (a foreign Character
        // is 404 even for a malformed body).
        let _lease = self
            .acquire_actor_activity(
                principal,
                &AdmittedActor::Character {
                    character_id: character_id.clone(),
                },
            )
            .await?;

        let pending_id = request.pending_id.as_str().to_string();
        let session_id = request.session_id.as_str().to_string();
        let task_kind = optional_str(request.task_kind.as_ref())
            .unwrap_or("unknown")
            .to_string();
        let raw_digest = request.raw_digest.as_str().to_string();
        let created_at = request
            .created_at
            .map_or_else(|| chrono::Utc::now().to_rfc3339(), |t| t.to_rfc3339());

        validate_capture_input(&request, &raw_digest)?;

        let record = nexus_local_db::CharacterPendingReviewRecord {
            pending_id: pending_id.clone(),
            session_id,
            character_id: character_id.clone(),
            actor_world_binding_id: binding_id.map(str::to_string),
            task_kind,
            raw_digest,
            created_at,
            source_operation_id: None,
        };
        nexus_local_db::create_character_pending_review(
            &self.inner.pool,
            principal.creator_id(),
            &record,
        )
        .await
        .map_err(map_local_db_error)?;

        map_wire(json!({
            "success": true,
            "pending_id": pending_id,
        }))
    }

    #[allow(clippy::significant_drop_tightening)] // the guard deliberately spans the whole operation
    /// List one owned Character's pending reviews (retained read; any
    /// lifecycle status), offset-paginated.
    ///
    /// Served through the pinned `MemoryPort` (NXM-5 Task 2): the provider's
    /// merged read projects the Character's unmigrated pending rows, and this
    /// adapter keeps the retained offset/`v1:{offset}` pagination.
    ///
    /// # Errors
    /// [`CoreError::NotFound`] for a foreign/missing Character and the mapped
    /// storage error otherwise.
    pub async fn list_character_pending_reviews(
        &self,
        principal: &Principal,
        character_id: String,
        binding_id: Option<String>,
        limit: u32,
        offset: u32,
    ) -> CoreResult<ListCharacterPendingReviewsResponse> {
        self.verify_principal(principal)?;
        // Retained bearer admission (404 for a foreign/missing Character)
        // precedes the provider read.
        let _admission = MemoryPipelineCtx::character_read(
            &self.inner.pool,
            principal.creator_id(),
            &character_id,
            binding_id.as_deref(),
        )
        .await?;
        let memory = self.character_memory(principal, &character_id).await?;
        let scoped: Vec<PendingMemoryRow> = memory
            .pending_rows()
            .await?
            .into_iter()
            .filter(|row| pending_row_in_scope(row, binding_id.as_deref()))
            .collect();
        let start = usize::try_from(offset).unwrap_or(usize::MAX);
        let page_size = usize::try_from(limit).unwrap_or(usize::MAX);
        let page: Vec<&PendingMemoryRow> = scoped.iter().skip(start).take(page_size).collect();
        let consumed = start.saturating_add(page.len());
        let has_more = consumed < scoped.len();
        let next_cursor = has_more.then(|| format!("v1:{consumed}"));
        let items: Vec<Value> = page
            .iter()
            .map(|row| {
                json!({
                    "pending_id": row.pending_id,
                    "session_id": row.session_id,
                    "character_id": character_id.as_str(),
                    "binding_id": row.binding_id,
                    "task_kind": row.task_kind,
                    "raw_digest": row.raw_digest,
                    "created_at": row.created_at,
                    "source_operation_id": row.source_operation_id,
                })
            })
            .collect();
        map_wire(json!({
            "items": items,
            "pagination": pagination_info(limit, has_more, next_cursor.as_deref()),
        }))
    }

    #[allow(clippy::significant_drop_tightening)] // the guard deliberately spans the whole operation
    /// Count one owned Character's pending reviews (retained read).
    ///
    /// # Errors
    /// [`CoreError::NotFound`] for a foreign/missing Character and the mapped
    /// storage error otherwise.
    pub async fn count_character_pending_reviews(
        &self,
        principal: &Principal,
        character_id: String,
        binding_id: Option<String>,
    ) -> CoreResult<CountCharacterPendingReviewsResponse> {
        self.verify_principal(principal)?;
        let _admission = MemoryPipelineCtx::character_read(
            &self.inner.pool,
            principal.creator_id(),
            &character_id,
            binding_id.as_deref(),
        )
        .await?;
        let memory = self.character_memory(principal, &character_id).await?;
        let count = memory
            .pending_rows()
            .await?
            .into_iter()
            .filter(|row| pending_row_in_scope(row, binding_id.as_deref()))
            .count();
        map_wire(json!({
            "count": i64::try_from(count).unwrap_or(i64::MAX),
        }))
    }

    /// Delete one pending row from an owned Character's queue. Holds the
    /// per-Character activity lease; a missing row is `404` and mutates
    /// nothing.
    ///
    /// Served through the pinned `MemoryPort`: the row is adopted under the
    /// bridge's deterministic id and consumed in the same guarded transaction,
    /// so a dismissal can never leave an adopted pending row behind (NXM-5 Task
    /// 2 fix round 1, finding F4).
    ///
    /// # Errors
    /// As [`Self::acquire_actor_activity`], plus [`CoreError::NotFound`] when
    /// the row is absent.
    pub async fn delete_character_pending_review(
        &self,
        principal: &Principal,
        character_id: String,
        pending_id: String,
    ) -> CoreResult<DeleteCharacterPendingReviewResponse> {
        self.verify_principal(principal)?;
        let _lease = self
            .acquire_actor_activity(
                principal,
                &AdmittedActor::Character {
                    character_id: character_id.clone(),
                },
            )
            .await?;
        let memory = self
            .character_memory_writer(principal, &character_id)
            .await?;
        let candidate_id = legacy::character_pending_review_candidate_id(&pending_id);
        let missing = || not_found("pending review", &format!("'{pending_id}'"));
        if !pending_now(&memory, &candidate_id).await? {
            return Err(missing());
        }
        if !memory.dismiss_legacy(&candidate_id).await? {
            return Err(missing());
        }
        map_wire(json!({
            "success": true,
            "pending_id": pending_id,
        }))
    }

    #[allow(clippy::significant_drop_tightening)] // the guard deliberately spans the whole operation
    /// List one owned Character's memory fragments (retained read),
    /// offset-paginated.
    ///
    /// Served through the pinned `MemoryPort`: the retained scope split
    /// (`binding_id` present = that binding's local fragments, absent = the
    /// Character-shared ones) becomes the provider's placement selection.
    ///
    /// # Errors
    /// [`CoreError::NotFound`] for a foreign/missing Character and the mapped
    /// storage error otherwise.
    pub async fn list_character_memory_fragments(
        &self,
        principal: &Principal,
        character_id: String,
        binding_id: Option<String>,
        limit: u32,
        offset: u32,
    ) -> CoreResult<ListCharacterMemoryFragmentsResponse> {
        self.verify_principal(principal)?;
        let _admission = MemoryPipelineCtx::character_read(
            &self.inner.pool,
            principal.creator_id(),
            &character_id,
            binding_id.as_deref(),
        )
        .await?;
        let memory = self.character_memory(principal, &character_id).await?;
        let selection = binding_id.as_deref().map_or_else(
            || json!({ "mode": "shared_only" }),
            |binding| json!({ "mode": "selected_contexts", "context_ids": [binding] }),
        );
        let start = usize::try_from(offset).unwrap_or(usize::MAX);
        let page_size = usize::try_from(limit).unwrap_or(usize::MAX);
        // One row past the page proves more rows exist (the retained
        // `limit + 1` overfetch). The page window is served as asked: clamping
        // it to an internal bound used to answer an empty page with
        // `has_more = false` while records existed past the window (fix round 1,
        // finding F5).
        let want = start.saturating_add(page_size).saturating_add(1);
        let records = memory
            .records(selection, &["fact"], None, Some(want))
            .await?;
        let page: Vec<&Value> = records.iter().skip(start).take(page_size).collect();
        let has_more = records.len() > start.saturating_add(page.len());
        let next_cursor = has_more.then(|| format!("v1:{}", offset.saturating_add(limit)));
        let fragments: Vec<CharacterMemoryFragmentInfo> = page
            .iter()
            .map(|record| character_fragment_from_provider(record, &character_id))
            .collect::<CoreResult<_>>()?;
        map_wire(json!({
            "fragments": fragments,
            "pagination": pagination_info(limit, has_more, next_cursor.as_deref()),
        }))
    }

    /// Promote a binding-local Character fragment to shared Character memory
    /// behind the per-Character activity lease. Revision-checked and atomic:
    /// a stale `expected_revision` writes nothing (409 `version_mismatch`),
    /// success clears the binding provenance and bumps the revision, and an
    /// already-shared fragment is a stable
    /// `character_fragment_already_shared` conflict.
    ///
    /// Served through the pinned `MemoryPort`: the explicit placement change is
    /// `memory.records.promote` with `target_scope: shared`. The DTO keeps the
    /// legacy 0→1 sequence — a provider record at revision `N` projects as the
    /// DTO's `N - 1`, so the caller's expected DTO revision is the provider CAS
    /// value `expected + 1`.
    ///
    /// The id is the one the fragment DTO served, so a legacy-projected fragment
    /// resolves through the bridge's external id and is adopted inside the
    /// promotion's own transaction (R4-F1); the record must belong to this
    /// Character and be a `fact` fragment (R4-F2). The retained SOUL-narrative
    /// invalidation runs on the already-shared path too, so a retry after a
    /// failed invalidation reaches one consistent state (R4-F3).
    ///
    /// # Errors
    /// As [`Self::acquire_actor_activity`], plus the mapped storage conflicts
    /// (`version_mismatch`, `character_fragment_already_shared`) and the
    /// retained not-found answer for an id that names no fragment of this
    /// Character.
    pub async fn promote_character_fragment(
        &self,
        principal: &Principal,
        character_id: String,
        fragment_id: String,
        expected_revision: i64,
    ) -> CoreResult<PromoteCharacterFragmentResponse> {
        self.verify_principal(principal)?;
        let _lease = self
            .acquire_actor_activity(
                principal,
                &AdmittedActor::Character {
                    character_id: character_id.clone(),
                },
            )
            .await?;
        let memory = self
            .character_memory_writer(principal, &character_id)
            .await?;
        // The retained conflict is decided on the source placement: provider
        // same-target promotion is a §7.0.4 no-op, so the adapter refuses an
        // already-shared fragment itself.
        let absent = || not_found("character memory fragment", &format!("'{fragment_id}'"));
        // The listed id resolves through the bridge, so a legacy-projected
        // fragment is promotable at all: the DTO serves the legacy
        // `fragment_id` while the provider names that row by its external id
        // (R4-F1).
        let current = memory
            .character_fragment(&fragment_id)
            .await?
            .ok_or_else(absent)?;
        let record_id = current["record_id"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        // R4-F2: the lease above names one Character, so the record must be that
        // Character's own and a `fact` fragment. The read alone is authorized by
        // the principal's grants, which also cover every other Character it
        // owns, so the owner and kind are checked here — before any mutation.
        if current["subject_id"].as_str() != Some(memory.subject_id()) {
            return Err(absent());
        }
        if current["kind"].as_str() != Some("fact") {
            return Err(invalid_input(
                "fragment_id must name a Character memory fragment",
            ));
        }
        // The placement the promotion leaves. While the record is still
        // binding-local its own scope names it; once the promotion committed it
        // names the target, and its provenance is a client claim that need not
        // name the placement at all — so the promotion's transaction recorded the
        // old binding and a retry reads that recovery row instead of guessing
        // (R4-F3 round 2).
        let Some(source_binding) = current
            .pointer("/scope/context_id")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            // The promotion already committed and only its invalidation is
            // outstanding: finish it before answering the stable conflict, so a
            // retry never leaves the shared fragment behind a stale cache.
            let binding = memory.soul_invalidation_binding(&record_id).await?;
            self.invalidate_character_soul_narratives(&character_id, binding.as_deref())
                .await?;
            memory.finish_soul_invalidation(&record_id).await?;
            return Err(CoreError::ActorConflict {
                code: "character_fragment_already_shared".to_string(),
                message: "character memory fragment is already shared".to_string(),
            });
        };
        let provider_revision = expected_revision
            .checked_add(1)
            .filter(|revision| *revision > 0)
            .ok_or_else(|| {
                invalid_input("expected_revision must be a non-negative fragment revision")
            })?;
        memory
            .promote_fragment(
                &record_id,
                u64::try_from(provider_revision).unwrap_or(u64::MAX),
            )
            .await?;
        // Retained cache semantics: exactly the affected SOUL-narrative scopes
        // (shared + the old binding) are invalidated.
        self.invalidate_character_soul_narratives(&character_id, Some(source_binding.as_str()))
            .await?;
        memory.finish_soul_invalidation(&record_id).await?;
        let promoted = memory.record(&record_id).await?.ok_or_else(absent)?;
        map_wire(json!({
            "fragment": character_fragment_from_provider(&promoted, &character_id)?,
        }))
    }

    /// Retained cache semantics on fragment promotion: exactly the affected
    /// SOUL-narrative scopes (the shared scope and the old binding) are
    /// invalidated.
    ///
    /// The promotion this follows is already committed, so the retry path
    /// ([`Self::promote_character_fragment`] on an already-shared fragment) runs
    /// the same invalidation: the delete is a function of the record's state,
    /// not of the call that promoted it (R4-F3). The old binding comes from the
    /// promote transaction's recovery row for a retry — the record no longer
    /// names it and its provenance never had to (R4-F3 round 2).
    ///
    /// shortcut: this is the one legacy-table write the rerouted promotion
    /// keeps; it disappears with the projection reroute that serves the
    /// narrative from records.
    async fn invalidate_character_soul_narratives(
        &self,
        character_id: &str,
        binding_id: Option<&str>,
    ) -> CoreResult<()> {
        // Test-only fault: the promotion it follows has already committed, so a
        // regression can prove the retry still finishes the invalidation.
        #[cfg(feature = "test-hooks")]
        if self
            .inner
            .soul_narrative_invalidation_fault
            .swap(false, std::sync::atomic::Ordering::SeqCst)
        {
            return Err(internal_err(
                "test-hook",
                "injected SOUL narrative invalidation failure",
            ));
        }
        sqlx::query(
            "DELETE FROM character_soul_narratives \
             WHERE character_id = ? AND (actor_world_binding_id IS NULL OR actor_world_binding_id = ?)",
        )
        .bind(character_id)
        .bind(binding_id)
        .execute(&self.inner.pool)
        .await
        .map_err(sqlx_internal)?;
        Ok(())
    }

    /// Drain a bounded slice of an owned Character's pending-review queue
    /// (v1.190 P2-T2 contract API). Holds the per-Character activity lease
    /// across the whole batch, including file promotion and any synthesis.
    ///
    /// The fetch is bounded at [`REVIEW_BATCH_LIMIT`] + 1 so the extra row
    /// proves more rows exist; `has_more` is true when the queue may not be
    /// fully drained (more rows in the DB, the call budget expired, or any
    /// fetched row remained pending).
    ///
    /// # Errors
    /// As [`Self::acquire_actor_activity`], plus the mapped pipeline/storage
    /// errors otherwise.
    #[allow(clippy::too_many_lines)] // the bounded fetch + disposition section are one flow
    pub async fn review_character_memory(
        &self,
        principal: &Principal,
        character_id: String,
        request: nexus_contracts::generated::daemon_api::characters::memory::review_character_memory_request::ReviewCharacterMemoryRequest,
    ) -> CoreResult<ReviewCharacterMemoryResponse> {
        self.verify_principal(principal)?;
        let binding_id = optional_str(request.binding_id.as_ref());
        // Review drains the queue and writes provider records: hold a writable
        // activity context — with its per-Character lease — across the batch.
        let ctx = MemoryPipelineCtx::character_write(
            self.acquire_actor_activity(
                principal,
                &AdmittedActor::Character {
                    character_id: character_id.clone(),
                },
            )
            .await?,
            principal.creator_id(),
            &character_id,
            binding_id,
        )?;
        // The dismissal-vs-drain authority check (fix round 1, finding F1):
        // the provider read serves pending candidates only, so a row a
        // dismissal consumed is no longer queued at all.
        let memory = self
            .character_memory_writer(principal, &character_id)
            .await?;
        let pending: Vec<PendingMemoryRow> = memory
            .pending_rows()
            .await?
            .into_iter()
            .filter(|row| pending_row_in_scope(row, binding_id))
            .collect();
        // The retained batch bound: process at most REVIEW_BATCH_LIMIT rows;
        // the extra row proves more exist.
        let batch_limit = usize::try_from(REVIEW_BATCH_LIMIT).unwrap_or(usize::MAX);
        let more_in_db = pending.len() > batch_limit;
        let mut rows: Vec<PendingMemoryRow> = pending.into_iter().take(batch_limit).collect();
        // The check/use boundary (fix round 2, finding F1): the fetch above only
        // selects the slice. This re-check runs inside the bearer's disposition
        // section, so a dismissal can neither commit between it and the
        // classification below nor be classified after it: one that already
        // committed is observed here and its row is dropped.
        #[cfg(feature = "test-hooks")]
        self.park_memory_review(MemoryReviewPause::AfterFetchCheck)
            .await;
        let section = memory.review_claim().await?;
        let rechecked: Vec<String> = rows.iter().map(|row| row.candidate_id.clone()).collect();
        let consumed = consumed_candidates(&memory, rechecked).await?;
        rows.retain(|row| !consumed.contains(&row.candidate_id));
        let processing_slice = rows.len();
        #[cfg(feature = "test-hooks")]
        self.park_memory_review(MemoryReviewPause::AfterRecheck)
            .await;
        let batch_rows: Vec<ReviewBatchRow> = rows
            .iter()
            .map(|row| ReviewBatchRow {
                candidate_id: row.candidate_id.clone(),
                expected_revision: row.revision,
                input: row.review_input(&character_id),
                binding_local: !row.shared,
            })
            .collect();
        let bearer = MemoryBearerRef::Character {
            owner_creator_id: principal.creator_id(),
            character_id: character_id.as_str(),
        };
        let deadline = tokio::time::Instant::now() + REVIEW_CALL_TIMEOUT;
        let mut outcome =
            process_bearer_review_batch(&batch_rows, bearer, &memory, &section, deadline).await?;
        drop(section);
        drop(ctx);
        let deadline_stopped = outcome.processed < processing_slice;
        outcome.has_more = more_in_db || deadline_stopped || outcome.any_row_remained_pending;
        outcome.more_in_db = more_in_db;
        outcome.processing_slice = processing_slice;

        map_wire(json!({
            "promoted": outcome.promoted,
            "fragmented": outcome.fragmented,
            "dropped": outcome.dropped,
            "has_more": outcome.has_more,
            "processed": i64::try_from(outcome.processed).unwrap_or(i64::MAX),
        }))
    }
}

fn validate_capture_input(
    request: &CaptureCharacterPendingReviewRequest,
    raw_digest: &str,
) -> CoreResult<()> {
    validate_capture_fields(
        request.pending_id.as_str(),
        request.session_id.as_str(),
        raw_digest,
        optional_str(request.task_kind.as_ref()),
        None,
    )
}

/// The capture-field validation both memory bearers share (a Character capture
/// has no world, so `world_id` is `None` there).
fn validate_capture_fields(
    pending_id: &str,
    session_id: &str,
    raw_digest: &str,
    task_kind: Option<&str>,
    world_id: Option<&str>,
) -> CoreResult<()> {
    if pending_id.is_empty() || pending_id.len() > 128 {
        return Err(invalid_input(
            "pending_id must be between 1 and 128 characters",
        ));
    }
    if pending_id.starts_with(RUN_PENDING_ID_PREFIX) {
        return Err(invalid_input(format!(
            "pending_id cannot use the reserved {RUN_PENDING_ID_PREFIX} prefix"
        )));
    }
    if session_id.is_empty() || session_id.len() > 128 {
        return Err(invalid_input(
            "session_id must be between 1 and 128 characters",
        ));
    }
    if raw_digest.is_empty() || raw_digest.len() > MAX_DIGEST_BYTES {
        return Err(invalid_input(format!(
            "raw_digest must be between 1 and {MAX_DIGEST_BYTES} bytes"
        )));
    }
    if let Some(kind) = task_kind {
        if kind.len() > 64 {
            return Err(invalid_input("task_kind must be at most 64 characters"));
        }
    }
    if world_id.is_some_and(|world| world.len() > 128) {
        return Err(invalid_input("world_id must be at most 128 characters"));
    }
    Ok(())
}

/// One provider record rendered as the retained Character fragment DTO.
///
/// The bridge stores the legacy display fields in `nexus_legacy` and serves a
/// Character fragment's legacy revision as `legacy + 1` (so the 0→1 promotion
/// step survives the seam); the DTO exposes the legacy numbering again. A
/// provider-native record of another origin falls back to its own id/revision.
fn character_fragment_from_provider(
    record: &Value,
    character_id: &str,
) -> CoreResult<CharacterMemoryFragmentInfo> {
    let origin = nexus_legacy(record);
    let fragment_id = origin["fragment_id"]
        .as_str()
        .or_else(|| record["record_id"].as_str())
        .unwrap_or_default();
    let revision = record["revision"].as_u64().unwrap_or(1).saturating_sub(1);
    map_wire(json!({
        "fragment_id": fragment_id,
        "session_id": origin["session_id"].as_str().unwrap_or_default(),
        "character_id": character_id,
        "binding_id": record.pointer("/scope/context_id"),
        "summary": record.pointer("/content/text").and_then(Value::as_str).unwrap_or_default(),
        "keywords": record_keywords(record),
        "created_at": origin["created_at"]
            .as_str()
            .or_else(|| record["created_at"].as_str())
            .unwrap_or_default(),
        "ttl": origin["ttl"],
        "revision": revision,
    }))
}

/// One provider record rendered as the retained Creator fragment DTO.
///
/// The bridge keeps the legacy display fields (fragment id, keywords, world)
/// in `nexus_legacy`, so the retained projection is identical for a projected
/// and an adopted row; a provider-native record of another origin falls back to
/// its own id and source context.
fn creator_fragment_from_provider(record: &Value) -> CoreResult<MemoryFragmentInfo> {
    let origin = nexus_legacy(record);
    let fragment_id = origin["fragment_id"]
        .as_str()
        .or_else(|| record["record_id"].as_str())
        .unwrap_or_default();
    map_wire(json!({
        "fragment_id": fragment_id,
        "summary": record.pointer("/content/text").and_then(Value::as_str).unwrap_or_default(),
        "world_id": creator_world_id(record),
        "keywords": record_keywords(record),
        "created_at": origin["created_at"]
            .as_str()
            .or_else(|| record["created_at"].as_str()),
    }))
}

// ── Creator memory family ────────────────────────────────────────────────

impl CoreService {
    /// List the principal's pending reviews (keyset on
    /// `(created_at DESC, pending_id DESC)`, bounded at `limit + 1`).
    ///
    /// Served through the pinned `MemoryPort` (NXM-5 Task 2): the provider's
    /// merged read projects the principal's unmigrated pending rows, and this
    /// adapter keeps the retained `pending_id` keyset cursor (a cursor whose row
    /// is gone restarts from the top, as the retained contract promises).
    ///
    /// # Errors
    /// [`CoreError::AuthRequired`] when the principal fails verification and
    /// the mapped storage error otherwise.
    pub async fn list_pending_reviews(
        &self,
        principal: &Principal,
        cursor: Option<String>,
        limit: usize,
    ) -> CoreResult<ListPendingReviewsResponse> {
        self.verify_principal(principal)?;
        let memory = self.creator_memory(principal).await?;
        let rows = memory.pending_rows().await?;
        let start = cursor
            .as_deref()
            .and_then(|pending_id| rows.iter().position(|row| row.pending_id == pending_id))
            .map_or(0, |position| position.saturating_add(1));
        let page: Vec<&PendingMemoryRow> = rows.iter().skip(start).take(limit).collect();
        let has_more = !page.is_empty() && start.saturating_add(page.len()) < rows.len();
        let next_cursor = has_more
            .then(|| page.last().map(|row| row.pending_id.clone()))
            .flatten();
        let items: Vec<Value> = page
            .iter()
            .map(|row| {
                json!({
                    "pending_id": row.pending_id,
                    "session_id": row.session_id,
                    "creator_id": principal.creator_id(),
                    "world_id": row.world_id,
                    "task_kind": row.task_kind,
                    "raw_digest": row.raw_digest,
                    "created_at": row.created_at,
                })
            })
            .collect();
        map_wire(json!({
            "items": items,
            "pagination": {
                "limit": i64::try_from(limit).unwrap_or(i64::MAX),
                "has_more": next_cursor.is_some(),
                "next_cursor": next_cursor,
            },
        }))
    }

    /// Count the principal's pending reviews.
    ///
    /// # Errors
    /// As [`Self::list_pending_reviews`].
    pub async fn count_pending_reviews(
        &self,
        principal: &Principal,
    ) -> CoreResult<CountPendingReviewsResponse> {
        self.verify_principal(principal)?;
        let memory = self.creator_memory(principal).await?;
        let count = memory.pending_rows().await?.len();
        Ok(CountPendingReviewsResponse {
            count: i64::try_from(count).unwrap_or(i64::MAX),
        })
    }

    /// Delete one of the principal's pending reviews; a missing row is `404`
    /// and mutates nothing.
    ///
    /// Served through the pinned `MemoryPort`. The row is adopted under the
    /// bridge's deterministic id and consumed in the same guarded transaction,
    /// so the legacy table stays write-idle, the id can never resurface through
    /// the bridge, and a dismissal can never leave an adopted pending row behind
    /// (NXM-5 Task 2 fix round 1, finding F4).
    ///
    /// A row that belongs to another Creator keeps the retained
    /// `pending_review` `ForbiddenReason`: the provider cannot see it, so the
    /// ownership re-read that distinguishes `403` from `404` is the legacy
    /// table's — read-only, exactly as before the reroute (fix round 1,
    /// finding F2).
    ///
    /// # Errors
    /// [`CoreError::NotFound`] when the row is absent,
    /// [`CoreError::ForbiddenReason`] when it belongs to another Creator,
    /// [`CoreError::Forbidden`] under read-only core access, and the retained
    /// `database_error` internal when the ownership re-read fails
    /// (NXM-5 QC1-F3).
    pub async fn delete_pending_review(
        &self,
        principal: &Principal,
        pending_id: String,
    ) -> CoreResult<DeletePendingReviewResponse> {
        self.verify_principal(principal)?;
        let memory = self.creator_memory_writer(principal).await?;
        let candidate_id = legacy::pending_review_candidate_id(&pending_id);
        // The retained answer for a row that is no longer queued is `404`
        // (before the reroute the row was deleted, so a repeat dismissal found
        // nothing). The authoritative decision is still the guarded dismissal
        // below; this read only keeps that answer.
        if pending_now(&memory, &candidate_id).await? && memory.dismiss_legacy(&candidate_id).await? {
            return Ok(DeletePendingReviewResponse {
                success: true,
                pending_id,
            });
        }
        // A row captured through the typed Creator path (NXM-16) is
        // provider-native: its candidate id is minted, so the retained
        // `pending_id` addresses it through the row itself instead of the
        // bridge's deterministic id.
        if let Some(row) = memory
            .pending_rows()
            .await?
            .into_iter()
            .find(|row| row.pending_id == pending_id)
        {
            if memory.dismiss_native(&row.candidate_id, row.revision).await? {
                return Ok(DeletePendingReviewResponse {
                    success: true,
                    pending_id,
                });
            }
        }
        Err(self.pending_review_refusal(principal, &pending_id).await?)
    }

    /// The retained refusal for a dismissal that found nothing to consume: a
    /// row another Creator owns is the `pending_review` `ForbiddenReason`, an
    /// absent or already-consumed row the `NotFound` it always was. The
    /// ownership re-read is the same read-only legacy lookup the pre-reroute
    /// handler performed, so its failure keeps the pre-reroute
    /// `database_error` mapping instead of masquerading as an absence
    /// (NXM-5 QC1-F3).
    async fn pending_review_refusal(
        &self,
        principal: &Principal,
        pending_id: &str,
    ) -> CoreResult<CoreError> {
        let owner: Option<String> =
            sqlx::query_scalar("SELECT creator_id FROM memory_pending_review WHERE pending_id = ?")
                .bind(pending_id)
                .fetch_optional(&self.inner.pool)
                .await
                .map_err(|e| {
                    internal_err(
                        "database_error",
                        format!("failed to lookup pending review: {e}"),
                    )
                })?;
        Ok(match owner {
            Some(owner) if owner != principal.creator_id() => CoreError::ForbiddenReason {
                resource: "pending_review".to_string(),
                reason: format!(
                    "pending review '{pending_id}' does not belong to creator '{}'",
                    principal.creator_id()
                ),
            },
            _ => CoreError::NotFound {
                resource: format!("pending review '{pending_id}' not found"),
            },
        })
    }

    /// List the principal's memory fragments with optional keyword/world
    /// filtering, bounded at `limit`.
    ///
    /// Served through the pinned `MemoryPort`: the provider's `fact` records
    /// are the reviewed fragments, and the retained world filter is the record's
    /// source-context predicate. The provider has no keyword index, so a keyword
    /// filter walks the subject's whole fragment set before applying its
    /// predicate — the retained SQL did the same over every row, and stopping
    /// early would silently drop matches (fix round 1, finding F5).
    ///
    /// # Errors
    /// As [`Self::list_pending_reviews`].
    pub async fn list_memory_fragments(
        &self,
        principal: &Principal,
        keyword: Option<String>,
        world_id: Option<String>,
        limit: usize,
    ) -> CoreResult<ListMemoryFragmentsResponse> {
        self.verify_principal(principal)?;
        if limit == 0 {
            return map_wire(json!({ "fragments": [] }));
        }
        let memory = self.creator_memory(principal).await?;
        let filter: Option<Vec<String>> = world_id.as_ref().map(|world| vec![world.clone()]);
        // A keyword read filters after the provider read, so it must see the
        // whole set; an unfiltered read needs only its own page.
        let want = keyword.is_none().then_some(limit);
        let records = memory
            .records(
                json!({ "mode": "shared_only" }),
                &["fact"],
                filter.as_deref(),
                want,
            )
            .await?;
        let mut fragments = Vec::new();
        for record in &records {
            if let Some(keyword) = keyword.as_deref() {
                if !record_keywords(record).iter().any(|value| value == keyword) {
                    continue;
                }
            }
            fragments.push(creator_fragment_from_provider(record)?);
            if fragments.len() == limit {
                break;
            }
        }
        map_wire(json!({ "fragments": fragments }))
    }

    /// Drain a bounded slice of the principal's pending-review queue
    /// (V1.80 REL-01 semantics preserved: bounded fetch, deadline-aware
    /// partial progress, `has_more` drain-completion contract).
    ///
    /// Cross-request serialization is a host concern (the daemon keeps its
    /// per-creator lock around this call); the queue-advance transactions
    /// remain exactly-one-row so concurrent callers can never duplicate
    /// fragments.
    ///
    /// # Errors
    /// As [`Self::review_character_memory`] for the Creator arm.
    #[allow(clippy::too_many_lines)] // the bounded fetch + disposition section are one flow
    pub async fn review_memory(
        &self,
        principal: &Principal,
        request: ReviewRequest,
    ) -> CoreResult<ReviewResponse> {
        self.verify_principal(principal)?;
        if request.creator_id != principal.creator_id() {
            return Err(CoreError::ForbiddenReason {
                resource: "memory_review".to_string(),
                reason: format!(
                    "creator_id '{}' does not match active creator '{}'",
                    request.creator_id,
                    principal.creator_id()
                ),
            });
        }
        // The dismissal-vs-drain authority check (fix round 1, finding F1):
        // the provider read serves pending candidates only, so a row a
        // dismissal consumed is no longer queued at all.
        let memory = self.creator_memory_writer(principal).await?;
        let pending = memory.pending_rows().await?;
        // The retained batch bound: process at most REVIEW_BATCH_LIMIT rows;
        // the extra row proves more exist.
        let batch_limit = usize::try_from(REVIEW_BATCH_LIMIT).unwrap_or(usize::MAX);
        let more_in_db = pending.len() > batch_limit;
        let mut rows: Vec<PendingMemoryRow> = pending.into_iter().take(batch_limit).collect();
        // The check/use boundary (fix round 2, finding F1): the fetch above only
        // selects the slice. This re-check runs inside the bearer's disposition
        // section, so a dismissal can neither commit between it and the
        // classification below nor be classified after it: one that already
        // committed is observed here and its row is dropped.
        #[cfg(feature = "test-hooks")]
        self.park_memory_review(MemoryReviewPause::AfterFetchCheck)
            .await;
        let section = memory.review_claim().await?;
        let rechecked: Vec<String> = rows.iter().map(|row| row.candidate_id.clone()).collect();
        let consumed = consumed_candidates(&memory, rechecked).await?;
        rows.retain(|row| !consumed.contains(&row.candidate_id));
        let processing_slice = rows.len();
        #[cfg(feature = "test-hooks")]
        self.park_memory_review(MemoryReviewPause::AfterRecheck)
            .await;

        let deadline = tokio::time::Instant::now() + REVIEW_CALL_TIMEOUT;
        let batch_rows: Vec<ReviewBatchRow> = rows
            .iter()
            .map(|row| ReviewBatchRow {
                candidate_id: row.candidate_id.clone(),
                expected_revision: row.revision,
                input: row.review_input(principal.creator_id()),
                // The whole Creator arm promotes; only a binding-local Character
                // capture is coerced to a fragment.
                binding_local: false,
            })
            .collect();
        let mut batch = process_bearer_review_batch(
            &batch_rows,
            MemoryBearerRef::Creator(principal.creator_id()),
            &memory,
            &section,
            deadline,
        )
        .await?;
        drop(section);

        // `has_more` is the drain-completion contract: `true` when the queue
        // may not be fully drained (more rows in the DB, the budget expired
        // mid-batch, or any fetched row remained pending).
        let deadline_stopped = batch.processed < processing_slice;
        let has_more = more_in_db || deadline_stopped || batch.any_row_remained_pending;
        batch.has_more = has_more;
        batch.more_in_db = more_in_db;
        batch.processing_slice = processing_slice;

        Ok(ReviewResponse {
            promoted: batch.promoted,
            fragmented: batch.fragmented,
            dropped: batch.dropped,
            has_more: Some(batch.has_more),
            processed: Some(i64::try_from(batch.processed).unwrap_or(i64::MAX)),
        })
    }
}

// ── Character ToM carrier family ─────────────────────────────────────────

const CURSOR_PREFIX: &str = "tom3:";
const CURSOR_SEP: char = '\u{1f}';
/// Fixed query-work bounds (fix round 1, review I4): the corpus is admitted
/// only up to these caps; exceeding them fails closed before materialization.
const MAX_CARRIERS_PER_SCOPE: u32 = 200;
const MAX_BELIEF_ROWS_PER_CARRIER: usize = 200;
/// `MAX_BELIEF_ROWS_PER_CARRIER` as `i64` for compile-time SQL bind args.
const MAX_BELIEF_ROWS_PER_CARRIER_I64: i64 = 200;

/// One belief row in keyset order `(order, carrier_entry_id, row_ordinal)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharacterTomBeliefRow {
    pub carrier_entry_id: String,
    pub row_ordinal: u32,
    pub belief: BeliefPropositionRaw,
    pub carrier_recorded_at: Option<String>,
}

/// Bounded keyset page.
#[derive(Debug, Clone)]
pub struct CharacterTomPage {
    pub items: Vec<CharacterTomBeliefRow>,
    pub limit: u32,
    pub has_more: bool,
    pub next_cursor: Option<String>,
}

/// Admitted list query.
#[derive(Debug, Clone)]
pub struct CharacterTomListQuery {
    pub world_id: String,
    pub binding_id: String,
    pub limit: u32,
    pub cursor: Option<String>,
    /// Optional order filter (`1` = L1, `2` = L2) so the mind projection can
    /// fill each slot with an independent bounded fetch (QC fix round 1,
    /// F-003). The public list route always passes `None`.
    pub order: Option<i64>,
}

/// Admitted record mutation (after wire DTO mapping).
#[derive(Debug, Clone)]
pub struct CharacterTomRecordInput {
    pub world_id: String,
    pub binding_id: String,
    pub carrier_entry_id: String,
    pub expected_revision: i64,
    pub belief: BeliefPropositionRaw,
    pub occurred_at: Option<String>,
    pub sort_key: Option<String>,
    pub event_id: Option<String>,
}

/// Admitted carrier probe row: typed base columns only, never `modules_json`,
/// so the pre-parse bound holds before any serde materialization.
#[derive(sqlx::FromRow, Debug, Clone)]
pub struct ProbeCarrier {
    pub key_block_id: String,
    pub status: String,
    pub character_id: Option<String>,
    pub actor_world_binding_id: Option<String>,
}

/// Stored `modules_json` violation category, used to classify a carrier that
/// fails the probe-ok predicate with the matching fail-closed error.
#[derive(Debug, Clone, Copy)]
enum ViolationKind {
    InvalidJson,
    Malformed,
    Oversized,
}

/// Reusable Character `ToM` composer (record + query).
pub struct CharacterTomService {
    views: crate::actor_knowledge::ActorKnowledgeViewService,
    store: SqliteKbStore,
    pool: SqlitePool,
}

impl CharacterTomService {
    /// Bind the service to a workspace pool.
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            views: crate::actor_knowledge::ActorKnowledgeViewService::new(pool.clone()),
            store: SqliteKbStore::new(pool.clone()),
            pool,
        }
    }

    /// Resolve limit (1..=100, default 50).
    ///
    /// # Errors
    ///
    /// Returns `invalid_input` when `raw` is outside `1..=100`.
    pub fn resolve_limit(raw: Option<i64>) -> CoreResult<u32> {
        crate::actor_knowledge::ActorKnowledgeViewService::resolve_limit(raw)
    }

    /// Decode opaque `(order, carrier_entry_id, row_ordinal)` cursor.
    ///
    /// # Errors
    ///
    /// Returns `invalid_input` when the cursor is malformed.
    pub fn decode_cursor(cursor: &Option<String>) -> CoreResult<Option<(i64, String, u32)>> {
        match cursor {
            None => Ok(None),
            Some(raw) => {
                let rest = raw.strip_prefix(CURSOR_PREFIX).ok_or_else(invalid_cursor)?;
                let parts: Vec<&str> = rest.split(CURSOR_SEP).collect();
                if parts.len() != 3
                    || parts[0].is_empty()
                    || parts[1].is_empty()
                    || parts[2].is_empty()
                {
                    return Err(invalid_cursor());
                }
                let order = parts[0].parse::<i64>().map_err(|_| invalid_cursor())?;
                let ordinal = parts[2].parse::<u32>().map_err(|_| invalid_cursor())?;
                Ok(Some((order, parts[1].to_string(), ordinal)))
            }
        }
    }

    /// Encode keyset cursor.
    #[must_use]
    pub fn encode_cursor(order: i64, carrier_entry_id: &str, row_ordinal: u32) -> String {
        format!("{CURSOR_PREFIX}{order}{CURSOR_SEP}{carrier_entry_id}{CURSOR_SEP}{row_ordinal}")
    }

    /// Keyset page over pre-sorted rows.
    #[must_use]
    pub fn paginate(
        mut rows: Vec<(i64, String, u32, CharacterTomBeliefRow)>,
        cursor: Option<(i64, String, u32)>,
        limit: u32,
    ) -> CharacterTomPage {
        rows.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then_with(|| a.1.cmp(&b.1))
                .then_with(|| a.2.cmp(&b.2))
        });
        if let Some((order, carrier, ordinal)) = cursor {
            rows.retain(|(o, c, ord, _)| {
                (*o, c.as_str(), *ord) > (order, carrier.as_str(), ordinal)
            });
        }
        let limit_us = usize::try_from(limit).unwrap_or(usize::MAX);
        let has_more = rows.len() > limit_us;
        rows.truncate(limit_us);
        let items: Vec<CharacterTomBeliefRow> =
            rows.into_iter().map(|(_, _, _, row)| row).collect();
        let next_cursor = if has_more {
            items.last().map(|row| {
                Self::encode_cursor(
                    row.belief.order.unwrap_or(0),
                    &row.carrier_entry_id,
                    row.row_ordinal,
                )
            })
        } else {
            None
        };
        CharacterTomPage {
            items,
            limit,
            has_more,
            next_cursor,
        }
    }

    /// List Character `ToM` rows from authorized carriers only.
    ///
    /// Work is bounded before materialization: each owner scope admits at
    /// most `MAX_CARRIERS_PER_SCOPE` carriers and each carrier at most
    /// `MAX_BELIEF_ROWS_PER_CARRIER` belief rows; exceeding either cap fails
    /// closed. `row_ordinal` is the physical `modules.belief` array index —
    /// malformed legacy elements are skipped without renumbering so keyset
    /// cursors never skip/duplicate around them.
    ///
    /// # Errors
    ///
    /// Returns an admission/validation [`CoreError`] on unauthorized or
    /// invalid input.
    pub async fn list(
        &self,
        caller_creator_id: &str,
        viewer_character_id: &str,
        query: CharacterTomListQuery,
    ) -> CoreResult<CharacterTomPage> {
        // ToM list is a retained read (§11.2): read the owned Character's
        // stored bindings and carriers with the owner + stored binding tuple,
        // not liveness. No L2 subject-reactivation requirement on historical
        // rows. Foreign/missing rows and a missing/foreign World still fail
        // closed (indistinguishable).
        self.admit_viewer_retained(
            caller_creator_id,
            viewer_character_id,
            &query.world_id,
            &query.binding_id,
        )
        .await?;
        let cursor = Self::decode_cursor(&query.cursor)?;
        let order_filter = query.order;
        // DB-side pre-parse bounds (fix round 3): carrier counts, belief-array
        // lengths, and modules JSON validity are enforced with compile-time SQL
        // before any `modules_json` text is parsed or materialized into records.
        let mut admitted_ids: Vec<String> = Vec::new();
        for owner in [
            KnowledgeOwnerRef::character(viewer_character_id),
            KnowledgeOwnerRef::actor_world_binding(&query.binding_id),
        ] {
            self.probe_scope_violations(&owner).await?;
            let admitted = self.probe_scope_carriers(&owner).await?;
            admitted_ids.extend(admitted.into_iter().map(|c| c.key_block_id));
        }
        // The timestamp lookup is constrained to exactly this concrete admitted
        // carrier-id snapshot — never an owner-scope rescan — so carriers
        // inserted/changed after the probe cannot enter the result (fix round 4).
        let recorded = self.carrier_recorded_at_map(&admitted_ids).await?;
        // Materialize exactly the probe-admitted id snapshot — never a second
        // owner-scope rescan (QC fix round 1, F-001). Status/ownership drift,
        // invalid `modules_json`, and oversized belief arrays discovered here
        // fail closed instead of entering or silently dropping rows.
        let carriers = self
            .materialize_admitted_carriers(&admitted_ids, viewer_character_id, &query.binding_id)
            .await?;
        let mut keyed = Vec::new();
        for (entry_id, modules) in carriers {
            let recorded_at = recorded.get(&entry_id).cloned().flatten();
            let rows = carrier_belief_elements(modules.as_ref())?;
            for (ordinal, element) in rows.iter().enumerate() {
                let Ok(belief) = serde_json::from_value::<BeliefPropositionRaw>(element.clone())
                else {
                    continue; // malformed legacy element: skip, keep physical ordinal
                };
                if validate_character_tom_belief_row(&belief, viewer_character_id).is_err() {
                    continue;
                }
                let order = belief.order.unwrap_or(0);
                if let Some(want) = order_filter {
                    if order != want {
                        continue;
                    }
                }
                let ordinal = u32::try_from(ordinal)
                    .map_err(|_| corpus_exceeded("belief rows per carrier"))?;
                keyed.push((
                    order,
                    entry_id.clone(),
                    ordinal,
                    CharacterTomBeliefRow {
                        carrier_entry_id: entry_id.clone(),
                        row_ordinal: ordinal,
                        belief,
                        carrier_recorded_at: recorded_at.clone(),
                    },
                ));
            }
        }
        Ok(Self::paginate(keyed, cursor, query.limit))
    }

    /// Record one L1/L2 belief on an authorized carrier (atomic CAS + `MindState`).
    ///
    /// # Errors
    ///
    /// Returns `invalid_input`/`not_found`/`conflict` on bad input, stale
    /// scopes, or CAS/ownership drift.
    pub async fn record(
        &self,
        caller_creator_id: &str,
        viewer_character_id: &str,
        input: CharacterTomRecordInput,
    ) -> CoreResult<(String, u64, String)> {
        self.admit_viewer(
            caller_creator_id,
            viewer_character_id,
            &input.world_id,
            &input.binding_id,
        )
        .await?;
        validate_character_tom_belief_row(&input.belief, viewer_character_id)
            .map_err(|e| invalid_input(e.to_string()))?;
        if input.belief.order == Some(2) {
            let subject = input.belief.holder.as_deref().unwrap_or_default();
            self.require_active_subject_binding(caller_creator_id, subject, &input.world_id)
                .await?;
        }
        // Pre-parse carrier probe (fix round 3): the stored `modules_json`
        // value is type/length/validity checked via compile-time SQL before any
        // `get_knowledge_entry` / serde parse. Invalid text, a present non-array
        // `belief`, or an oversized array fail closed without mutation.
        let probe = self
            .probe_carrier(&input.carrier_entry_id)
            .await?
            .ok_or_else(|| not_found("carrier_entry", &input.carrier_entry_id))?;
        if matches!(probe.status.as_str(), "deleted" | "merged" | "deprecated") {
            return Err(not_found("carrier_entry", &input.carrier_entry_id));
        }
        match &probe.character_id {
            Some(id) if id == viewer_character_id => {}
            _ => match &probe.actor_world_binding_id {
                Some(id) if id == &input.binding_id => {}
                _ if probe.character_id.is_none() && probe.actor_world_binding_id.is_none() => {
                    return Err(invalid_input(
                        "World-owned KnowledgeEntry cannot be a Character ToM carrier",
                    ));
                }
                _ => return Err(not_found("carrier_entry", &input.carrier_entry_id)),
            },
        }
        // The CAS bump is a checked `+ 1` on this i64 revision; only the exact
        // `i64::MAX` input can overflow it, so reject that single value
        // deterministically.
        if input.expected_revision == i64::MAX {
            return Err(invalid_input(
                "expected_revision exceeds the CAS increment domain",
            ));
        }
        let carrier = self
            .require_admitted_carrier(
                viewer_character_id,
                &input.binding_id,
                &input.carrier_entry_id,
            )
            .await?;
        let mut modules = carrier.modules.clone().unwrap_or_else(|| json!({}));
        append_belief_row(&mut modules, &input.belief)?;
        let modules_str = serde_json::to_string(&modules).map_err(|e| wire_err(&e))?;
        let mind_state_id = format!("ms_{}", uuid::Uuid::new_v4().simple());
        let mind_state_wire =
            build_derivative_mind_state_wire(&input.carrier_entry_id, &mind_state_id, &input)?;
        let mut tx = self.pool.begin().await.map_err(sqlx_internal)?;
        // PR #240 finding 3: the pre-transaction admission can go stale before
        // commit. Revalidate the complete live scope — active owned viewer
        // Character, active owned World, active selected binding, and the L2
        // subject's own active Character + binding — inside the same
        // transaction as the CAS, so a removed/deactivated binding or
        // lifecycle flip rolls back instead of committing under a stale
        // viewpoint.
        let l2_subject = if input.belief.order == Some(2) {
            Some(input.belief.holder.as_deref().unwrap_or_default())
        } else {
            None
        };
        Self::revalidate_live_scope_in_tx(
            &mut tx,
            caller_creator_id,
            viewer_character_id,
            &input.world_id,
            &input.binding_id,
            l2_subject,
        )
        .await?;
        let new_revision = atomic_cas_carrier_modules_and_insert_mind_state_in_tx(
            &mut tx,
            &input.carrier_entry_id,
            input.expected_revision,
            &modules_str,
            &mind_state_wire,
            viewer_character_id,
            &input.binding_id,
        )
        .await
        .map_err(map_tom_local_db)?;
        tx.commit().await.map_err(sqlx_internal)?;
        Ok((input.carrier_entry_id, new_revision, mind_state_id))
    }

    /// Stored admission: active owned viewer Character, active owned World,
    /// and the viewer's active selected binding (P2 `ActorAdmissionService`
    /// parity). Foreign/missing rows are 404; inactive rows are 409.
    async fn admit_viewer(
        &self,
        caller_creator_id: &str,
        viewer_character_id: &str,
        world_id: &str,
        binding_id: &str,
    ) -> CoreResult<()> {
        crate::actors::require_active_owned_character(
            &self.pool,
            caller_creator_id,
            viewer_character_id,
        )
        .await?;
        crate::actors::require_active_owned_world(&self.pool, caller_creator_id, world_id).await?;
        self.views
            .require_active_binding(viewer_character_id, binding_id, world_id)
            .await?;
        Ok(())
    }

    /// Retained-data read admission (v1.185 P0 Task 2): owned Character (any
    /// status), the stored binding tuple (`binding_id` belongs to the viewer
    /// Character and targets `world_id`, any binding status), and an owned
    /// World (any status). Liveness is not required for a read; missing,
    /// foreign, or cross-Character rows are indistinguishable from missing and
    /// fail closed.
    async fn admit_viewer_retained(
        &self,
        caller_creator_id: &str,
        viewer_character_id: &str,
        world_id: &str,
        binding_id: &str,
    ) -> CoreResult<()> {
        let row = nexus_local_db::get_character(&self.pool, caller_creator_id, viewer_character_id)
            .await
            .map_err(map_tom_local_db)?;
        if row.is_none() {
            return Err(not_found("character", viewer_character_id));
        }
        let world = sqlx::query!(
            r#"SELECT owner_creator_id AS "owner_creator_id!"
               FROM narrative_worlds WHERE world_id = ?"#,
            world_id
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlx_internal)?;
        match world {
            Some(stored) if stored.owner_creator_id == caller_creator_id => {}
            _ => return Err(not_found("world", world_id)),
        }
        self.views
            .require_stored_binding_tuple(viewer_character_id, binding_id, world_id)
            .await?;
        Ok(())
    }

    /// In-transaction revalidation of the complete live record scope
    /// (PR #240 finding 3). Runs inside the CAS transaction; any drift
    /// (inactive/foreign/missing Character, World, binding, or L2 subject)
    /// returns an error so the caller drops the transaction uncommitted.
    async fn revalidate_live_scope_in_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        caller_creator_id: &str,
        viewer_character_id: &str,
        world_id: &str,
        binding_id: &str,
        l2_subject: Option<&str>,
    ) -> CoreResult<()> {
        let chr = sqlx::query!(
            r#"SELECT status AS "status!" FROM characters
               WHERE character_id = ? AND owner_creator_id = ?"#,
            viewer_character_id,
            caller_creator_id
        )
        .fetch_optional(&mut **tx)
        .await
        .map_err(sqlx_internal)?;
        match chr {
            Some(row) if row.status == "active" => {}
            Some(row) => {
                return Err(CoreError::ActorConflict {
                    code: "character_inactive".to_string(),
                    message: format!("character {viewer_character_id} is {}", row.status),
                });
            }
            None => return Err(not_found("character", viewer_character_id)),
        }
        let world = sqlx::query!(
            r#"SELECT owner_creator_id AS "owner_creator_id!", status AS "status!"
               FROM narrative_worlds WHERE world_id = ?"#,
            world_id
        )
        .fetch_optional(&mut **tx)
        .await
        .map_err(sqlx_internal)?;
        match world {
            Some(row) if row.owner_creator_id == caller_creator_id && row.status == "active" => {}
            Some(row) if row.owner_creator_id == caller_creator_id => {
                return Err(CoreError::ActorConflict {
                    code: "world_inactive".to_string(),
                    message: format!("world {world_id} is {}", row.status),
                });
            }
            _ => return Err(not_found("world", world_id)),
        }
        let binding = sqlx::query!(
            r#"SELECT character_id AS "character_id!", world_id AS "world_id!",
                      status AS "status!"
               FROM actor_world_bindings WHERE binding_id = ?"#,
            binding_id
        )
        .fetch_optional(&mut **tx)
        .await
        .map_err(sqlx_internal)?;
        match binding {
            Some(row)
                if row.character_id == viewer_character_id
                    && row.world_id == world_id
                    && row.status == "active" => {}
            _ => return Err(not_found("actor_world_binding", binding_id)),
        }
        if let Some(subject) = l2_subject {
            let subj = sqlx::query!(
                r#"SELECT status AS "status!" FROM characters
                   WHERE character_id = ? AND owner_creator_id = ?"#,
                subject,
                caller_creator_id
            )
            .fetch_optional(&mut **tx)
            .await
            .map_err(sqlx_internal)?;
            match subj {
                Some(row) if row.status == "active" => {}
                Some(row) => {
                    return Err(CoreError::ActorConflict {
                        code: "character_inactive".to_string(),
                        message: format!("character {subject} is {}", row.status),
                    });
                }
                None => return Err(not_found("character", subject)),
            }
            let subject_binding = sqlx::query_scalar!(
                r#"SELECT binding_id AS "binding_id!" FROM actor_world_bindings
                   WHERE character_id = ? AND world_id = ? AND status = 'active' LIMIT 1"#,
                subject,
                world_id
            )
            .fetch_optional(&mut **tx)
            .await
            .map_err(sqlx_internal)?;
            if subject_binding.is_none() {
                return Err(not_found("character_world_binding", subject));
            }
        }
        Ok(())
    }

    async fn require_active_subject_binding(
        &self,
        caller_creator_id: &str,
        subject_character_id: &str,
        world_id: &str,
    ) -> CoreResult<()> {
        crate::actors::require_active_owned_character(
            &self.pool,
            caller_creator_id,
            subject_character_id,
        )
        .await?;
        let row = sqlx::query_scalar!(
            r#"SELECT binding_id AS "binding_id!" FROM actor_world_bindings
               WHERE character_id = ? AND world_id = ? AND status = 'active' LIMIT 1"#,
            subject_character_id,
            world_id
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlx_internal)?;
        if row.is_some() {
            Ok(())
        } else {
            Err(not_found("character_world_binding", subject_character_id))
        }
    }

    /// Materialize exactly the probe-admitted carrier ids (QC fix round 1,
    /// F-001) in probe-admission order. Each row is revalidated at
    /// materialization: still live, still owned by the admitted
    /// Character/binding, `modules_json` still valid JSON, belief array still
    /// within the row cap. Any drift fails closed with
    /// `carrier_scope_drifted` / `carrier_modules_invalid_json` /
    /// `carrier_modules_malformed` / `view_incomplete` — never a silent
    /// omission, never an unadmitted carrier.
    async fn materialize_admitted_carriers(
        &self,
        admitted_ids: &[String],
        viewer_character_id: &str,
        binding_id: &str,
    ) -> CoreResult<Vec<(String, Option<Value>)>> {
        #[derive(sqlx::FromRow)]
        struct AdmittedCarrierRow {
            key_block_id: String,
            status: String,
            character_id: Option<String>,
            actor_world_binding_id: Option<String>,
            modules_json: Option<String>,
        }
        if admitted_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids_json = serde_json::to_string(admitted_ids).map_err(|e| wire_err(&e))?;
        let rows = sqlx::query_as!(
            AdmittedCarrierRow,
            r#"SELECT key_block_id AS "key_block_id!", status AS "status!",
                      character_id, actor_world_binding_id, modules_json
               FROM kb_key_blocks WHERE key_block_id IN (SELECT value FROM json_each(?))"#,
            ids_json
        )
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_internal)?;
        let mut by_id: std::collections::HashMap<String, AdmittedCarrierRow> = rows
            .into_iter()
            .map(|row| (row.key_block_id.clone(), row))
            .collect();
        let mut out = Vec::with_capacity(admitted_ids.len());
        for id in admitted_ids {
            let row = by_id.remove(id).ok_or_else(|| carrier_scope_drifted(id))?;
            if matches!(row.status.as_str(), "deleted" | "merged" | "deprecated") {
                return Err(carrier_scope_drifted(id));
            }
            let owner_ok = row.character_id.as_deref() == Some(viewer_character_id)
                || row.actor_world_binding_id.as_deref() == Some(binding_id);
            if !owner_ok {
                return Err(carrier_scope_drifted(id));
            }
            let modules = match &row.modules_json {
                None => None,
                Some(text) => {
                    Some(serde_json::from_str::<Value>(text).map_err(|_| invalid_modules_json())?)
                }
            };
            // Re-check the per-carrier row cap on the materialized array: a
            // carrier admitted by the probe but appended before this read must
            // still fail closed instead of materializing oversized work.
            if carrier_belief_elements(modules.as_ref())?.len() > MAX_BELIEF_ROWS_PER_CARRIER {
                return Err(corpus_exceeded("belief rows per carrier"));
            }
            out.push((id.clone(), modules));
        }
        Ok(out)
    }

    async fn require_admitted_carrier(
        &self,
        viewer_character_id: &str,
        binding_id: &str,
        carrier_entry_id: &str,
    ) -> CoreResult<KnowledgeEntryRecord> {
        let carrier = self
            .store
            .get_knowledge_entry(carrier_entry_id)
            .await
            .map_err(|err| match err {
                KbStoreError::NotFound(_) => not_found("carrier_entry", carrier_entry_id),
                other => map_kb_store(&other),
            })?;
        if matches!(carrier.status.as_str(), "deleted" | "merged" | "deprecated") {
            return Err(not_found("carrier_entry", carrier_entry_id));
        }
        match &carrier.owner {
            KnowledgeOwnerRef::Character(id) if id == viewer_character_id => Ok(carrier),
            KnowledgeOwnerRef::ActorWorldBinding(id) if id == binding_id => Ok(carrier),
            KnowledgeOwnerRef::World(_) => Err(invalid_input(
                "World-owned KnowledgeEntry cannot be a Character ToM carrier",
            )),
            _ => Err(not_found("carrier_entry", carrier_entry_id)),
        }
    }

    /// Latest derivative `MindState` `occurred_at` per carrier in the concrete
    /// admitted carrier-id set (fix round 4).
    async fn carrier_recorded_at_map(
        &self,
        admitted_ids: &[String],
    ) -> CoreResult<std::collections::HashMap<String, Option<String>>> {
        #[derive(sqlx::FromRow)]
        struct LatestDerivative {
            carrier_entry_id: String,
            occurred_at: Option<String>,
        }
        if admitted_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        let ids_json = serde_json::to_string(admitted_ids).map_err(|e| wire_err(&e))?;
        let rows = sqlx::query_as!(
            LatestDerivative,
            r#"SELECT m.holder_entry_id AS "carrier_entry_id!", m.occurred_at
               FROM mind_states m
               WHERE m.holder_entry_id IN (SELECT value FROM json_each(?))
                 AND NOT EXISTS (
                   SELECT 1 FROM mind_states m2
                   WHERE m2.holder_entry_id = m.holder_entry_id
                     AND (m2.created_at > m.created_at
                          OR (m2.created_at = m.created_at
                              AND m2.mind_state_id > m.mind_state_id))
                 )"#,
            ids_json
        )
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_internal)?;
        // Exactly one latest derivative row per admitted id, matching the
        // `json_each` id snapshot — never a scope rescan.
        Ok(rows
            .into_iter()
            .map(|row| (row.carrier_entry_id, row.occurred_at))
            .collect())
    }

    /// Admitted carrier probe row (typed base columns only — never selects or
    /// parses `modules_json`, so the pre-parse bound holds).
    async fn probe_scope_carriers(
        &self,
        owner: &KnowledgeOwnerRef,
    ) -> CoreResult<Vec<ProbeCarrier>> {
        let rows: Vec<ProbeCarrier> = match owner {
            KnowledgeOwnerRef::Character(id) => sqlx::query_as!(
                ProbeCarrier,
                r#"SELECT key_block_id AS "key_block_id!", status AS "status!",
                          character_id, actor_world_binding_id
                   FROM kb_key_blocks
                   WHERE character_id = ?
                     AND status NOT IN ('deleted', 'merged', 'deprecated')
                     AND (
                       modules_json IS NULL
                       OR (
                         json_valid(modules_json)
                         AND (
                           json_type(modules_json, '$.belief') IS NULL
                           OR json_type(modules_json, '$.belief') = 'array'
                         )
                         AND COALESCE(json_array_length(modules_json, '$.belief'), 0) <= ?
                       )
                     )
                   ORDER BY created_at ASC, key_block_id ASC
                   LIMIT 201"#,
                id,
                MAX_BELIEF_ROWS_PER_CARRIER_I64
            )
            .fetch_all(&self.pool)
            .await
            .map_err(sqlx_internal)?,
            KnowledgeOwnerRef::ActorWorldBinding(id) => sqlx::query_as!(
                ProbeCarrier,
                r#"SELECT key_block_id AS "key_block_id!", status AS "status!",
                          character_id, actor_world_binding_id
                   FROM kb_key_blocks
                   WHERE actor_world_binding_id = ?
                     AND status NOT IN ('deleted', 'merged', 'deprecated')
                     AND (
                       modules_json IS NULL
                       OR (
                         json_valid(modules_json)
                         AND (
                           json_type(modules_json, '$.belief') IS NULL
                           OR json_type(modules_json, '$.belief') = 'array'
                         )
                         AND COALESCE(json_array_length(modules_json, '$.belief'), 0) <= ?
                       )
                     )
                   ORDER BY created_at ASC, key_block_id ASC
                   LIMIT 201"#,
                id,
                MAX_BELIEF_ROWS_PER_CARRIER_I64
            )
            .fetch_all(&self.pool)
            .await
            .map_err(sqlx_internal)?,
            KnowledgeOwnerRef::World(_) => {
                return Err(internal_err(
                    "character_tom_scope_invalid",
                    "ToM carrier scope is never World-owned",
                ));
            }
        };
        if rows.len() > MAX_CARRIERS_PER_SCOPE as usize {
            return Err(corpus_exceeded(match owner {
                KnowledgeOwnerRef::Character(_) => "character-owned carriers",
                _ => "binding-owned carriers",
            }));
        }
        Ok(rows)
    }

    /// Detect non-probe-ok carriers in an owner scope and map them to the
    /// matching fail-closed error, before any modules parse.
    async fn probe_scope_violations(&self, owner: &KnowledgeOwnerRef) -> CoreResult<()> {
        let invalid = self
            .scope_violation_count(owner, ViolationKind::InvalidJson)
            .await?;
        if invalid > 0 {
            return Err(invalid_modules_json());
        }
        let malformed = self
            .scope_violation_count(owner, ViolationKind::Malformed)
            .await?;
        if malformed > 0 {
            return Err(modules_malformed());
        }
        let oversized = self
            .scope_violation_count(owner, ViolationKind::Oversized)
            .await?;
        if oversized > 0 {
            return Err(corpus_exceeded("belief rows per carrier"));
        }
        Ok(())
    }

    /// One typed `COUNT(*)` over the scope + a violation predicate.
    async fn scope_violation_count(
        &self,
        owner: &KnowledgeOwnerRef,
        kind: ViolationKind,
    ) -> CoreResult<i64> {
        match (owner, kind) {
            (KnowledgeOwnerRef::Character(id), ViolationKind::InvalidJson) => sqlx::query_scalar!(
                r#"SELECT COUNT(*) FROM kb_key_blocks
                   WHERE character_id = ?
                     AND status NOT IN ('deleted', 'merged', 'deprecated')
                     AND modules_json IS NOT NULL AND NOT json_valid(modules_json)"#,
                id
            )
            .fetch_one(&self.pool)
            .await
            .map_err(sqlx_internal),
            (KnowledgeOwnerRef::Character(id), ViolationKind::Malformed) => sqlx::query_scalar!(
                r#"SELECT COUNT(*) FROM kb_key_blocks
                   WHERE character_id = ?
                     AND status NOT IN ('deleted', 'merged', 'deprecated')
                     AND json_valid(modules_json)
                     AND json_type(modules_json, '$.belief') IS NOT NULL
                     AND json_type(modules_json, '$.belief') <> 'array'"#,
                id
            )
            .fetch_one(&self.pool)
            .await
            .map_err(sqlx_internal),
            (KnowledgeOwnerRef::Character(id), ViolationKind::Oversized) => sqlx::query_scalar!(
                r#"SELECT COUNT(*) FROM kb_key_blocks
                   WHERE character_id = ?
                     AND status NOT IN ('deleted', 'merged', 'deprecated')
                     AND json_valid(modules_json)
                     AND json_type(modules_json, '$.belief') = 'array'
                     AND json_array_length(modules_json, '$.belief') > ?"#,
                id,
                MAX_BELIEF_ROWS_PER_CARRIER_I64
            )
            .fetch_one(&self.pool)
            .await
            .map_err(sqlx_internal),
            (KnowledgeOwnerRef::ActorWorldBinding(id), ViolationKind::InvalidJson) => {
                sqlx::query_scalar!(
                    r#"SELECT COUNT(*) FROM kb_key_blocks
                   WHERE actor_world_binding_id = ?
                     AND status NOT IN ('deleted', 'merged', 'deprecated')
                     AND modules_json IS NOT NULL AND NOT json_valid(modules_json)"#,
                    id
                )
                .fetch_one(&self.pool)
                .await
                .map_err(sqlx_internal)
            }
            (KnowledgeOwnerRef::ActorWorldBinding(id), ViolationKind::Malformed) => {
                sqlx::query_scalar!(
                    r#"SELECT COUNT(*) FROM kb_key_blocks
                   WHERE actor_world_binding_id = ?
                     AND status NOT IN ('deleted', 'merged', 'deprecated')
                     AND json_valid(modules_json)
                     AND json_type(modules_json, '$.belief') IS NOT NULL
                     AND json_type(modules_json, '$.belief') <> 'array'"#,
                    id
                )
                .fetch_one(&self.pool)
                .await
                .map_err(sqlx_internal)
            }
            (KnowledgeOwnerRef::ActorWorldBinding(id), ViolationKind::Oversized) => {
                sqlx::query_scalar!(
                    r#"SELECT COUNT(*) FROM kb_key_blocks
                   WHERE actor_world_binding_id = ?
                     AND status NOT IN ('deleted', 'merged', 'deprecated')
                     AND json_valid(modules_json)
                     AND json_type(modules_json, '$.belief') = 'array'
                     AND json_array_length(modules_json, '$.belief') > ?"#,
                    id,
                    MAX_BELIEF_ROWS_PER_CARRIER_I64
                )
                .fetch_one(&self.pool)
                .await
                .map_err(sqlx_internal)
            }
            (KnowledgeOwnerRef::World(_), _) => Err(internal_err(
                "character_tom_scope_invalid",
                "ToM carrier scope is never World-owned",
            )),
        }
    }

    /// Single-carrier probe for the record path. Returns the typed admitted
    /// carrier row when probe-ok, or `None` when the carrier is absent.
    async fn probe_carrier(&self, carrier_entry_id: &str) -> CoreResult<Option<ProbeCarrier>> {
        let row = sqlx::query_as!(
            ProbeCarrier,
            r#"SELECT key_block_id AS "key_block_id!", status AS "status!",
                      character_id, actor_world_binding_id
               FROM kb_key_blocks
               WHERE key_block_id = ?
                 AND (
                   modules_json IS NULL
                   OR (
                     json_valid(modules_json)
                     AND (
                       json_type(modules_json, '$.belief') IS NULL
                       OR json_type(modules_json, '$.belief') = 'array'
                     )
                     AND COALESCE(json_array_length(modules_json, '$.belief'), 0) <= ?
                   )
                 )"#,
            carrier_entry_id,
            MAX_BELIEF_ROWS_PER_CARRIER_I64
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlx_internal)?;
        if row.is_some() {
            return Ok(row);
        }
        // No probe-ok row: classify the violation or report the carrier as
        // absent. Each check is a separate typed COUNT query.
        if self
            .carrier_violation_count(carrier_entry_id, ViolationKind::InvalidJson)
            .await?
            > 0
        {
            return Err(invalid_modules_json());
        }
        if self
            .carrier_violation_count(carrier_entry_id, ViolationKind::Malformed)
            .await?
            > 0
        {
            return Err(modules_malformed());
        }
        if self
            .carrier_violation_count(carrier_entry_id, ViolationKind::Oversized)
            .await?
            > 0
        {
            return Err(corpus_exceeded("belief rows per carrier"));
        }
        Ok(None)
    }

    async fn carrier_violation_count(
        &self,
        carrier_entry_id: &str,
        kind: ViolationKind,
    ) -> CoreResult<i64> {
        match kind {
            ViolationKind::InvalidJson => sqlx::query_scalar!(
                r#"SELECT COUNT(*) FROM kb_key_blocks
                   WHERE key_block_id = ?
                     AND modules_json IS NOT NULL AND NOT json_valid(modules_json)"#,
                carrier_entry_id
            )
            .fetch_one(&self.pool)
            .await
            .map_err(sqlx_internal),
            ViolationKind::Malformed => sqlx::query_scalar!(
                r#"SELECT COUNT(*) FROM kb_key_blocks
                   WHERE key_block_id = ?
                     AND json_valid(modules_json)
                     AND json_type(modules_json, '$.belief') IS NOT NULL
                     AND json_type(modules_json, '$.belief') <> 'array'"#,
                carrier_entry_id
            )
            .fetch_one(&self.pool)
            .await
            .map_err(sqlx_internal),
            ViolationKind::Oversized => sqlx::query_scalar!(
                r#"SELECT COUNT(*) FROM kb_key_blocks
                   WHERE key_block_id = ?
                     AND json_valid(modules_json)
                     AND json_type(modules_json, '$.belief') = 'array'
                     AND json_array_length(modules_json, '$.belief') > ?"#,
                carrier_entry_id,
                MAX_BELIEF_ROWS_PER_CARRIER_I64
            )
            .fetch_one(&self.pool)
            .await
            .map_err(sqlx_internal),
        }
    }
}

/// Borrow the carrier's `modules.belief` array elements.
///
/// Malformed stored shapes fail closed: a present non-object `modules` value
/// or a present non-array `belief` member is a deterministic
/// `carrier_modules_malformed` conflict — never a panic, never a silent
/// rewrite. Absent `modules`/`belief` yields an empty slice.
fn carrier_belief_elements(modules: Option<&Value>) -> CoreResult<&[Value]> {
    let Some(modules) = modules else {
        return Ok(&[]);
    };
    let obj = modules.as_object().ok_or_else(modules_malformed)?;
    obj.get("belief").map_or_else(
        || Ok(&[][..]),
        |value| {
            value
                .as_array()
                .map(Vec::as_slice)
                .ok_or_else(modules_malformed)
        },
    )
}

/// Non-NULL `modules_json` that is not valid JSON text — distinguishable from
/// absent modules and from shape-malformed modules; fail-closed, never
/// overwritten (fix round 2).
fn invalid_modules_json() -> CoreError {
    CoreError::ActorConflict {
        code: "carrier_modules_invalid_json".to_string(),
        message: "carrier modules_json is not valid JSON text; refusing to read or overwrite it"
            .to_string(),
    }
}

fn modules_malformed() -> CoreError {
    CoreError::ActorConflict {
        code: "carrier_modules_malformed".to_string(),
        message: "carrier modules must be an object and modules.belief, when present, an array"
            .to_string(),
    }
}

/// A probe-admitted carrier changed status or ownership before
/// materialization (QC fix round 1, F-001): refuse the inconsistent snapshot.
fn carrier_scope_drifted(id: &str) -> CoreError {
    CoreError::ActorConflict {
        code: "carrier_scope_drifted".to_string(),
        message: format!(
            "admitted carrier {id} changed status or ownership before materialization;              refusing an inconsistent ToM snapshot"
        ),
    }
}

fn corpus_exceeded(what: &str) -> CoreError {
    CoreError::ActorConflict {
        code: "view_incomplete".to_string(),
        message: format!(
            "Character ToM corpus exceeds the fixed {what} bound; refusing unbounded work"
        ),
    }
}

/// Append `row` to `modules.belief`, preserving every unknown sibling module
/// key and existing element verbatim.
fn append_belief_row(modules: &mut Value, row: &BeliefPropositionRaw) -> CoreResult<()> {
    if !modules.is_object() {
        return Err(modules_malformed());
    }
    let obj = modules.as_object_mut().expect("checked is_object above");
    if let Some(existing) = obj.get("belief") {
        if !existing.is_array() {
            return Err(modules_malformed());
        }
    }
    let belief = obj.entry("belief").or_insert_with(|| json!([]));
    let rows = belief.as_array_mut().expect("checked is_array above");
    // QC fix round 1 (F-002): never append the row that would exceed the
    // per-carrier cap — a carrier at the cap rejects here, before any CAS or
    // MindState write, so the corpus can never become un-listable.
    if rows.len() >= MAX_BELIEF_ROWS_PER_CARRIER {
        return Err(corpus_exceeded("belief rows per carrier"));
    }
    rows.push(serde_json::to_value(row).map_err(|e| wire_err(&e))?);
    Ok(())
}

fn build_derivative_mind_state_wire(
    carrier_entry_id: &str,
    mind_state_id: &str,
    input: &CharacterTomRecordInput,
) -> CoreResult<Value> {
    let occurred_at = input
        .occurred_at
        .clone()
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
    let mut wire = json!({
        "schema_version": 1,
        "mind_state_id": mind_state_id,
        "holder_entry_id": carrier_entry_id,
        "canonical_name": input.belief.proposition.clone().unwrap_or_default(),
        "occurred_at": occurred_at,
        "sort_key": input.sort_key.clone().unwrap_or_else(|| "0001".to_string()),
        "snapshot": {
            "belief": serde_json::to_value(&input.belief).map_err(|e| wire_err(&e))?
        },
        "deltas": [],
        "extensions": { "nexus": { "character_tom": true } }
    });
    if let Some(event_id) = &input.event_id {
        wire["source_anchor"] = json!({ "event_id": event_id });
    }
    Ok(wire)
}

/// Map a generated record request into domain input + belief row.
///
/// # Errors
///
/// Returns `invalid_input` when the wire record is malformed.
pub fn record_input_from_request(
    req: &RecordCharacterTomRequest,
    expected_revision: i64,
) -> CoreResult<CharacterTomRecordInput> {
    let belief = BeliefPropositionRaw {
        holder: Some(newtype_wire_string(&req.holder)),
        proposition: Some(newtype_wire_string(&req.proposition)),
        order: Some(
            i64::try_from(req.order.get())
                .map_err(|_| invalid_input("order exceeds the i64 domain"))?,
        ),
        truth: req.truth.as_ref().map(enum_wire_string),
        access: req.access.as_ref().map(enum_wire_string),
        representation: req.representation.as_ref().map(enum_wire_string),
        content_type: req.content_type.as_ref().map(enum_wire_string),
        source: req.source.as_ref().map(enum_wire_string),
        context: req.context.as_ref().map(enum_wire_string),
    };
    Ok(CharacterTomRecordInput {
        world_id: newtype_wire_string(&req.world_id),
        binding_id: newtype_wire_string(&req.binding_id),
        carrier_entry_id: req.carrier_entry_id.clone(),
        expected_revision,
        belief,
        occurred_at: req.occurred_at.map(|dt| dt.to_rfc3339()),
        sort_key: req.sort_key.clone(),
        event_id: req.event_id.as_ref().map(newtype_wire_string),
    })
}

fn newtype_wire_string<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

fn enum_wire_string<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_else(|| "Unknown".to_string())
}

fn invalid_cursor() -> CoreError {
    invalid_input("cursor is not a valid opaque Character ToM keyset token")
}

fn map_kb_store(err: &KbStoreError) -> CoreError {
    internal_err("character_tom_kb_failed", err)
}

fn map_tom_local_db(err: LocalDbError) -> CoreError {
    match err {
        LocalDbError::VersionMismatch { .. } => CoreError::ActorConflict {
            code: "version_mismatch".to_string(),
            message: err.to_string(),
        },
        LocalDbError::ValidationError(msg) => invalid_input(msg),
        other => internal_err("character_tom_db_failed", other),
    }
}

// ── CoreService ToM commands ─────────────────────────────────────────────

impl CoreService {
    /// Record one L1/L2 belief on an authorized carrier behind the
    /// per-Character activity lease (atomic CAS + derivative `MindState`).
    ///
    /// # Errors
    /// As [`Self::acquire_actor_activity`], plus the mapped admission/CAS
    /// conflicts otherwise.
    pub async fn record_character_tom(
        &self,
        principal: &Principal,
        character_id: String,
        request: RecordCharacterTomRequest,
    ) -> CoreResult<RecordCharacterTomResponse> {
        self.verify_principal(principal)?;
        let expected_revision = i64::try_from(request.expected_revision)
            .map_err(|_| invalid_input("expected_revision is out of range"))?;
        let input = record_input_from_request(&request, expected_revision)?;
        // ToM record is a mutation: hold the per-Character activity lease
        // across the CAS transaction (404 foreign/missing, 409 archived
        // `character_inactive`).
        let _lease = self
            .acquire_actor_activity(
                principal,
                &AdmittedActor::Character {
                    character_id: character_id.clone(),
                },
            )
            .await?;
        let service = CharacterTomService::new(self.inner.pool.clone());
        let (carrier_entry_id, revision, mind_state_id) = service
            .record(principal.creator_id(), &character_id, input)
            .await?;
        RecordCharacterTomResponse::builder()
            .carrier_entry_id(carrier_entry_id)
            .mind_state_id(mind_state_id)
            .revision(revision)
            .try_into()
            .map_err(wire_err)
    }

    /// List Character `ToM` belief rows from authorized carriers (retained
    /// read; no activity lease).
    ///
    /// # Errors
    /// [`CoreError::ActorInput`] for a malformed limit/cursor and the mapped
    /// admission/storage errors otherwise.
    pub async fn list_character_tom(
        &self,
        principal: &Principal,
        character_id: String,
        query: ListCharacterTomQuery,
    ) -> CoreResult<ListCharacterTomResponse> {
        self.verify_principal(principal)?;
        let service = CharacterTomService::new(self.inner.pool.clone());
        let page = service
            .list(
                principal.creator_id(),
                &character_id,
                CharacterTomListQuery {
                    world_id: query.world_id.to_string(),
                    binding_id: query.binding_id.to_string(),
                    limit: CharacterTomService::resolve_limit(query.limit)?,
                    cursor: query.cursor.clone(),
                    order: None,
                },
            )
            .await?;
        let items: Vec<ListedBeliefItem> = page
            .items
            .iter()
            .map(|row| map_wire(tom_item_value(row)?))
            .collect::<CoreResult<_>>()?;
        let pagination: ListedPagination = ListedPagination::builder()
            .limit(i64::from(page.limit))
            .has_more(page.has_more)
            .next_cursor(page.next_cursor)
            .try_into()
            .map_err(wire_err)?;
        ListCharacterTomResponse::builder()
            .items(items)
            .pagination(pagination)
            .try_into()
            .map_err(wire_err)
    }
}

/// Project one `ToM` belief row to its wire value.
fn tom_item_value(row: &CharacterTomBeliefRow) -> CoreResult<serde_json::Value> {
    let mut value = json!({
        "carrier_entry_id": row.carrier_entry_id,
        "row_ordinal": i64::from(row.row_ordinal),
        "holder": row.belief.holder,
        "proposition": row.belief.proposition,
        "order": row.belief.order,
        "truth": row.belief.truth,
        "access": row.belief.access,
        "representation": row.belief.representation,
        "content_type": row.belief.content_type,
        "source": row.belief.source,
        "context": row.belief.context,
    });
    if let Some(raw) = &row.carrier_recorded_at {
        let parsed: chrono::DateTime<chrono::Utc> =
            nexus_knowledge::world_kb::knowledge_entry::parse_stored_created_at(raw)
                .map_err(wire_err)?;
        value["carrier_recorded_at"] = serde_json::json!(parsed);
    }
    Ok(value)
}
