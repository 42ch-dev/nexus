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
use nexus_contracts::generated::daemon_api::characters::memory::character_pending_review_info::CharacterPendingReviewInfo;
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
use nexus_contracts::generated::daemon_api::memory::delete_pending_review_response::DeletePendingReviewResponse;
use nexus_contracts::generated::daemon_api::memory::list_memory_fragments_response::ListMemoryFragmentsResponse;
use nexus_contracts::generated::daemon_api::memory::list_pending_reviews_response::ListPendingReviewsResponse;
use nexus_contracts::generated::daemon_api::memory::memory_fragment_info::MemoryFragmentInfo;
use nexus_contracts::generated::daemon_api::memory::pending_review_info::PendingReviewInfo;
use nexus_contracts::generated::daemon_api::memory::review_request::ReviewRequest;
use nexus_contracts::generated::daemon_api::memory::review_response::ReviewResponse;
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
    MemoryPipelineCtx, REVIEW_BATCH_LIMIT,
};
use crate::principal::Principal;
use crate::service::CoreService;

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

fn decode_fragment_keywords(raw: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(raw).unwrap_or_default()
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

        Ok(map_wire(json!({
            "success": true,
            "pending_id": pending_id,
        }))?)
    }

    /// List one owned Character's pending reviews (retained read; any
    /// lifecycle status), offset-paginated.
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
        let pool = &self.inner.pool;
        let ctx = MemoryPipelineCtx::character_read(
            pool,
            principal.creator_id(),
            &character_id,
            binding_id.as_deref(),
        )
        .await?;
        let _ = ctx;

        let fetch_limit = i64::from(limit) + 1;
        let rows = nexus_local_db::list_character_pending_reviews(
            pool,
            principal.creator_id(),
            &character_id,
            binding_id.as_deref(),
            fetch_limit,
            i64::from(offset),
        )
        .await
        .map_err(map_local_db_error)?;
        let has_more = rows.len() > limit as usize;
        let next_cursor = has_more.then(|| format!("v1:{}", offset.saturating_add(limit)));
        let items: Vec<CharacterPendingReviewInfo> = rows
            .into_iter()
            .take(limit as usize)
            .map(|r| {
                map_wire(json!({
                    "pending_id": r.pending_id,
                    "session_id": r.session_id,
                    "character_id": r.character_id,
                    "binding_id": r.actor_world_binding_id,
                    "task_kind": r.task_kind,
                    "raw_digest": r.raw_digest,
                    "created_at": r.created_at,
                    "source_operation_id": r.source_operation_id,
                }))
            })
            .collect::<CoreResult<_>>()?;
        Ok(map_wire(json!({
            "items": items,
            "pagination": pagination_info(limit, has_more, next_cursor.as_deref()),
        }))?)
    }

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
        let pool = &self.inner.pool;
        let ctx = MemoryPipelineCtx::character_read(
            pool,
            principal.creator_id(),
            &character_id,
            binding_id.as_deref(),
        )
        .await?;
        let _ = ctx;
        let count = nexus_local_db::count_character_pending_reviews(
            pool,
            principal.creator_id(),
            &character_id,
            binding_id.as_deref(),
        )
        .await
        .map_err(map_local_db_error)?;
        Ok(map_wire(json!({
            "count": i64::try_from(count).unwrap_or(i64::MAX),
        }))?)
    }

    /// Delete one pending row from an owned Character's queue. Holds the
    /// per-Character activity lease; a missing row is `404` and mutates
    /// nothing.
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
        let deleted = nexus_local_db::delete_character_pending_review(
            &self.inner.pool,
            principal.creator_id(),
            &character_id,
            &pending_id,
        )
        .await
        .map_err(map_local_db_error)?;
        if !deleted {
            return Err(not_found("pending review", &format!("'{pending_id}'")));
        }
        Ok(map_wire(json!({
            "success": true,
            "pending_id": pending_id,
        }))?)
    }

    /// List one owned Character's memory fragments (retained read),
    /// offset-paginated.
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
        let pool = &self.inner.pool;
        let ctx = MemoryPipelineCtx::character_read(
            pool,
            principal.creator_id(),
            &character_id,
            binding_id.as_deref(),
        )
        .await?;
        let _ = ctx;

        let fetch_limit = i64::from(limit) + 1;
        let rows = nexus_local_db::list_character_fragments(
            pool,
            principal.creator_id(),
            &character_id,
            binding_id.as_deref(),
            fetch_limit,
            i64::from(offset),
        )
        .await
        .map_err(map_local_db_error)?;
        let has_more = rows.len() > limit as usize;
        let next_cursor = has_more.then(|| format!("v1:{}", offset.saturating_add(limit)));
        let fragments: Vec<CharacterMemoryFragmentInfo> = rows
            .into_iter()
            .take(limit as usize)
            .map(|r| character_fragment_info(&r))
            .collect::<CoreResult<_>>()?;
        Ok(map_wire(json!({
            "fragments": fragments,
            "pagination": pagination_info(limit, has_more, next_cursor.as_deref()),
        }))?)
    }

    /// Promote a binding-local Character fragment to shared Character memory
    /// behind the per-Character activity lease. Revision-checked and atomic:
    /// a stale `expected_revision` writes nothing (409 `version_mismatch`),
    /// success clears the binding provenance and bumps the revision, and an
    /// already-shared fragment is a stable
    /// `character_fragment_already_shared` conflict.
    ///
    /// # Errors
    /// As [`Self::acquire_actor_activity`], plus the mapped storage conflicts
    /// (`version_mismatch`, `character_fragment_already_shared`).
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
        // The repository commits and returns the authoritative promoted
        // record (same fragment id, cleared binding provenance, bumped
        // revision). Map it directly — no post-commit re-query or panic path.
        let promoted = nexus_local_db::promote_character_fragment_to_shared(
            &self.inner.pool,
            principal.creator_id(),
            &character_id,
            &fragment_id,
            expected_revision,
        )
        .await
        .map_err(map_promote_error)?;
        Ok(map_wire(json!({
            "fragment": character_fragment_info(&promoted)?,
        }))?)
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
    pub async fn review_character_memory(
        &self,
        principal: &Principal,
        character_id: String,
        request: nexus_contracts::generated::daemon_api::characters::memory::review_character_memory_request::ReviewCharacterMemoryRequest,
    ) -> CoreResult<ReviewCharacterMemoryResponse> {
        self.verify_principal(principal)?;
        let binding_id = optional_str(request.binding_id.as_ref());
        let pool = &self.inner.pool;
        // Review drains the queue and promotes/writes files: hold a writable
        // activity context across the whole batch (includes file promotion).
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
        let nexus_home = self.nexus_home();
        // Fetch batch_limit + 1 so the extra row proves more rows exist;
        // truncate the processing slice back to the documented batch bound
        // (mirrors the Creator memory review handler — no off-by-one on
        // has_more).
        let fetch_limit = REVIEW_BATCH_LIMIT + 1;
        let mut rows = nexus_local_db::list_character_pending_reviews(
            pool,
            principal.creator_id(),
            &character_id,
            binding_id,
            fetch_limit,
            0,
        )
        .await
        .map_err(map_local_db_error)?;
        let batch_limit = usize::try_from(REVIEW_BATCH_LIMIT).unwrap_or(usize::MAX);
        let more_in_db = rows.len() > batch_limit;
        if more_in_db {
            rows.truncate(batch_limit);
        }
        let processing_slice = rows.len();
        let inputs: Vec<nexus_creator_memory::review::PendingReviewInput> = rows
            .into_iter()
            .map(|r| nexus_creator_memory::review::PendingReviewInput {
                pending_id: r.pending_id,
                session_id: r.session_id,
                bearer_id: r.character_id,
                scope_id: r.actor_world_binding_id,
                task_kind: r.task_kind,
                raw_digest: r.raw_digest,
                created_at: r.created_at,
            })
            .collect();
        let deadline = tokio::time::Instant::now() + REVIEW_CALL_TIMEOUT;
        let mut outcome =
            process_bearer_review_batch(&inputs, &nexus_home, &ctx, pool, deadline).await?;
        drop(ctx);
        let deadline_stopped = outcome.processed < processing_slice;
        outcome.has_more = more_in_db || deadline_stopped || outcome.any_row_remained_pending;
        outcome.more_in_db = more_in_db;
        outcome.processing_slice = processing_slice;

        Ok(map_wire(json!({
            "promoted": outcome.promoted,
            "fragmented": outcome.fragmented,
            "dropped": outcome.dropped,
            "has_more": outcome.has_more,
            "processed": i64::try_from(outcome.processed).unwrap_or(i64::MAX),
        }))?)
    }
}

fn validate_capture_input(
    request: &CaptureCharacterPendingReviewRequest,
    raw_digest: &str,
) -> CoreResult<()> {
    let pending_id = request.pending_id.as_str();
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
    let session_id = request.session_id.as_str();
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
    if let Some(kind) = optional_str(request.task_kind.as_ref()) {
        if kind.len() > 64 {
            return Err(invalid_input("task_kind must be at most 64 characters"));
        }
    }
    Ok(())
}

fn character_fragment_info(
    r: &nexus_local_db::CharacterMemoryFragmentRecord,
) -> CoreResult<CharacterMemoryFragmentInfo> {
    map_wire(json!({
        "fragment_id": r.fragment_id,
        "session_id": r.session_id,
        "character_id": r.character_id,
        "binding_id": r.actor_world_binding_id,
        "summary": r.summary,
        "keywords": decode_fragment_keywords(&r.keywords),
        "created_at": r.created_at,
        "ttl": r.ttl,
        "revision": r.revision,
    }))
}

/// Retained promote-error texture: the CAS miss is the stable
/// `version_mismatch` conflict, everything else keeps the generic
/// local-db mapping.
fn map_promote_error(e: LocalDbError) -> CoreError {
    match e {
        LocalDbError::VersionMismatch { .. } => CoreError::ActorConflict {
            code: "version_mismatch".to_string(),
            message: e.to_string(),
        },
        other => crate::error::actor_db_err(other),
    }
}

// ── Creator memory family ────────────────────────────────────────────────

impl CoreService {
    /// List the principal's pending reviews (keyset on
    /// `(created_at DESC, pending_id DESC)`, bounded at `limit + 1`).
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
        let fetch_limit = i64::try_from(limit + 1).unwrap_or(i64::MAX);
        let mut items = fetch_pending_reviews_page(
            &self.inner.pool,
            principal.creator_id(),
            cursor.as_deref(),
            fetch_limit,
        )
        .await
        .map_err(|e| {
            internal_err(
                "database_error",
                format!("failed to list pending reviews: {e}"),
            )
        })?;
        let next_cursor = if items.len() > limit {
            items.truncate(limit);
            items.last().map(|i| i.pending_id.clone())
        } else {
            None
        };
        // serde round-trip between the shared and response-local generated
        // clones (the retained `wire_cast` translation, now core-side).
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
        let row = sqlx::query_scalar!(
            r#"SELECT COUNT(*) as "count!" FROM memory_pending_review WHERE creator_id = ?"#,
            principal.creator_id()
        )
        .fetch_one(&self.inner.pool)
        .await
        .map_err(|e| {
            internal_err(
                "database_error",
                format!("failed to count pending reviews: {e}"),
            )
        })?;
        Ok(CountPendingReviewsResponse { count: row })
    }

    /// Delete one of the principal's pending reviews after an ownership
    /// re-read; a missing row is `404` and a foreign row is the retained
    /// `pending_review` 403. Zero mutation on either.
    ///
    /// # Errors
    /// [`CoreError::NotFound`] / [`CoreError::ForbiddenReason`] as retained,
    /// plus the mapped storage error.
    pub async fn delete_pending_review(
        &self,
        principal: &Principal,
        pending_id: String,
    ) -> CoreResult<DeletePendingReviewResponse> {
        self.verify_principal(principal)?;
        let review = sqlx::query!(
            r#"SELECT pending_id as "pending_id!", session_id, creator_id, world_id, task_kind, raw_digest, created_at
         FROM memory_pending_review WHERE pending_id = ?"#, // sqlx R3: use ? instead of ?1
            pending_id.as_str()
        )
        .fetch_optional(&self.inner.pool)
        .await
        .map_err(|e| internal_err("database_error", format!("failed to lookup pending review: {e}")))?;

        match review {
            None => {
                return Err(CoreError::NotFound {
                    resource: format!("pending review '{pending_id}' not found"),
                });
            }
            Some(ref r) if r.creator_id != principal.creator_id() => {
                return Err(CoreError::ForbiddenReason {
                    resource: "pending_review".to_string(),
                    reason: format!(
                        "pending review '{}' does not belong to creator '{}'",
                        pending_id,
                        principal.creator_id()
                    ),
                });
            }
            _ => {}
        }

        sqlx::query!(
            "DELETE FROM memory_pending_review WHERE pending_id = ?",
            pending_id
        )
        .execute(&self.inner.pool)
        .await
        .map_err(|e| {
            internal_err(
                "database_error",
                format!("failed to delete pending review: {e}"),
            )
        })?;
        Ok(DeletePendingReviewResponse {
            success: true,
            pending_id,
        })
    }

    /// List the principal's memory fragments with optional keyword/world
    /// filtering, bounded at `limit`.
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
        let records = if keyword.is_some() {
            let limit_u32 = u32::try_from(limit).unwrap_or(u32::MAX);
            nexus_local_db::memory_fragment::list_fragments_filtered(
                &self.inner.pool,
                principal.creator_id(),
                keyword.as_deref(),
                world_id.as_deref(),
                limit_u32,
            )
            .await
            .map_err(|e| {
                internal_err(
                    "database_error",
                    format!("failed to list memory fragments: {e}"),
                )
            })?
        } else {
            let limit_i64 = i64::try_from(limit).unwrap_or(i64::MAX);
            nexus_local_db::memory_fragment::list_fragments_limited(
                &self.inner.pool,
                principal.creator_id(),
                world_id.as_deref(),
                limit_i64,
            )
            .await
            .map_err(|e| {
                internal_err(
                    "database_error",
                    format!("failed to list memory fragments: {e}"),
                )
            })?
        };

        let fragments: Vec<MemoryFragmentInfo> = records
            .into_iter()
            .map(|r| MemoryFragmentInfo {
                fragment_id: r.fragment_id,
                summary: r.summary,
                world_id: r.world_id,
                keywords: decode_fragment_keywords(&r.keywords),
                created_at: Some(r.created_at),
            })
            .collect();
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
        let nexus_home = self.nexus_home();
        // Bounded fetch: REVIEW_BATCH_LIMIT + 1 overfetch drives `has_more`.
        let fetch_limit = REVIEW_BATCH_LIMIT + 1;
        let mut rows =
            fetch_pending_reviews_page(&self.inner.pool, principal.creator_id(), None, fetch_limit)
                .await
                .map_err(|e| {
                    internal_err(
                        "database_error",
                        format!("failed to fetch pending reviews for review: {e}"),
                    )
                })?;
        let batch_limit = usize::try_from(REVIEW_BATCH_LIMIT).unwrap_or(usize::MAX);
        let more_in_db = rows.len() > batch_limit;
        if more_in_db {
            rows.truncate(batch_limit);
        }
        let processing_slice = rows.len();

        let deadline = tokio::time::Instant::now() + REVIEW_CALL_TIMEOUT;
        let inputs: Vec<nexus_creator_memory::review::PendingReviewInput> = rows
            .iter()
            .map(|row| nexus_creator_memory::review::PendingReviewInput {
                pending_id: row.pending_id.clone(),
                session_id: row.session_id.clone(),
                bearer_id: row.creator_id.clone(),
                scope_id: row.world_id.clone(),
                task_kind: row.task_kind.clone(),
                raw_digest: row.raw_digest.clone(),
                created_at: row.created_at.clone(),
            })
            .collect();
        let ctx = MemoryPipelineCtx::creator(principal.creator_id(), None);
        let mut batch =
            process_bearer_review_batch(&inputs, &nexus_home, &ctx, &self.inner.pool, deadline)
                .await?;
        drop(ctx);

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

/// Fetch one bounded keyset page of a creator's pending reviews.
///
/// The cursor is a `pending_id`; pagination is a keyset on
/// `(created_at DESC, pending_id DESC)` with a point lookup for the cursor
/// row's `created_at` (a deleted cursor restarts from the top, preserving the
/// prior `position() == None` behavior). `fetch_limit` is `page_limit + 1`
/// from the caller; the extra row drives `has_more` / `next_cursor`.
async fn fetch_pending_reviews_page(
    pool: &SqlitePool,
    creator_id: &str,
    cursor: Option<&str>,
    fetch_limit: i64,
) -> Result<Vec<PendingReviewInfo>, sqlx::Error> {
    let cursor_created_at: Option<String> = if let Some(cursor_pid) = cursor {
        sqlx::query_scalar!(
            "SELECT created_at FROM memory_pending_review
             WHERE creator_id = ? AND pending_id = ?",
            creator_id,
            cursor_pid
        )
        .fetch_optional(pool)
        .await?
    } else {
        None
    };

    let rows: Vec<PendingReviewInfo> = if let (Some(cursor_pid), Some(cursor_ca)) =
        (cursor, cursor_created_at)
    {
        sqlx::query!(
            r#"SELECT pending_id as "pending_id!", session_id, creator_id, world_id, task_kind, raw_digest, created_at
             FROM memory_pending_review
             WHERE creator_id = ?
               AND (created_at < ? OR (created_at = ? AND pending_id < ?))
             ORDER BY created_at DESC, pending_id DESC
             LIMIT ?"#,
            creator_id,
            cursor_ca,
            cursor_ca,
            cursor_pid,
            fetch_limit
        )
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| PendingReviewInfo {
            pending_id: row.pending_id,
            session_id: row.session_id,
            creator_id: row.creator_id,
            world_id: row.world_id,
            task_kind: row.task_kind,
            raw_digest: row.raw_digest,
            created_at: row.created_at,
        })
        .collect()
    } else {
        sqlx::query!(
            r#"SELECT pending_id as "pending_id!", session_id, creator_id, world_id, task_kind, raw_digest, created_at
             FROM memory_pending_review
             WHERE creator_id = ?
             ORDER BY created_at DESC, pending_id DESC
             LIMIT ?"#,
            creator_id,
            fetch_limit
        )
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| PendingReviewInfo {
            pending_id: row.pending_id,
            session_id: row.session_id,
            creator_id: row.creator_id,
            world_id: row.world_id,
            task_kind: row.task_kind,
            raw_digest: row.raw_digest,
            created_at: row.created_at,
        })
        .collect()
    };

    Ok(rows)
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
    pub revision: Option<i64>,
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
                r#"SELECT key_block_id AS "key_block_id!", revision, status AS "status!",
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
                r#"SELECT key_block_id AS "key_block_id!", revision, status AS "status!",
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
            r#"SELECT key_block_id AS "key_block_id!", revision, status AS "status!",
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
        Ok(RecordCharacterTomResponse::builder()
            .carrier_entry_id(carrier_entry_id)
            .mind_state_id(mind_state_id)
            .revision(revision)
            .try_into()
            .map_err(wire_err)?)
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
            .next_cursor(page.next_cursor.clone())
            .try_into()
            .map_err(wire_err)?;
        Ok(ListCharacterTomResponse::builder()
            .items(items)
            .pagination(pagination)
            .try_into()
            .map_err(wire_err)?)
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
