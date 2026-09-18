//! World KB graph/patch/candidates/promote/relationship implementation
//! (ported from daemon handlers).

use nexus_contracts::{
    world_kb_patch_entity_request::{
        NexusWorldKbEntityPatch, NexusWorldKbEntityPatchAudience,
        NexusWorldKbEntityPatchModulesKey, NexusWorldKbEntityPatchModulesValue,
    },
    BlockType, PaginationInfo, WorldKbCandidateProjection, WorldKbCandidatesResponse,
    WorldKbEntityProjection, WorldKbGraphResponse, WorldKbKeyBlockStateResponse,
    WorldKbPatchEntityRequest, WorldKbPatchEntityResponse, WorldKbPatchRelationshipRequest,
    WorldKbPatchRelationshipResponse, WorldKbPromoteCandidateRequest,
    WorldKbPromoteCandidateResponse, WorldKbRelationshipProjection, WorldKbSourceAnchorProjection,
};
use nexus_knowledge::world_kb::knowledge_entry::{
    resolve_authored_governance, KnowledgeAudience, KnowledgeAuthoringOp, KnowledgeEntryBody,
    KnowledgeGovernance, KnowledgeEntryRecord,
};
use nexus_knowledge::world_kb::store::KbStoreError;
use nexus_knowledge::world_kb::validation::{
    validate_body, validate_canonical_name, ValidationMode,
};
use nexus_local_db::kb_extract_job::list_pending_for_world_after;
use nexus_local_db::kb_relationships::list_relationships_for_world_in_tx;
use nexus_local_db::kb_store::{get_knowledge_entry_in_tx, list_by_world_in_tx};
use nexus_spoke_adapter::conversion::{knowledge_record_to_spoke, spoke_to_knowledge_record};
use nexus_spoke_adapter::extensions::set_nexus_body;
use nexus_spoke_adapter::{
    is_world_conflict_reject, put_knowledge_entry_in_tx, KnowledgeEntry as SpokeKnowledgeEntry,
    SpokeReject, SpokeRejectCode, SpokeResult,
};
use sqlx::{Sqlite, SqlitePool};
use std::collections::HashMap;
use tracing::warn;

use crate::error::{db_err, local_db_err, CoreError, CoreResult};
use crate::principal::Principal;
use crate::service::CoreService;
use crate::CoreAccess;

const GRAPH_ENTITY_CAP: usize = 500;
const GRAPH_RELATIONSHIP_CAP: usize = 1000;
const DEFAULT_CANDIDATE_LIMIT: i64 = 50;
const MAX_CANDIDATE_LIMIT: i64 = 250;
const CANDIDATE_CURSOR_PREFIX: &str = "kbp:";

fn wire_cast<T, S>(value: S) -> T
where
    T: serde::de::DeserializeOwned,
    S: serde::Serialize,
{
    serde_json::from_value(serde_json::to_value(value).expect("wire_cast: serialize"))
        .expect("wire_cast: deserialize")
}

fn validation_summary(errors: &[String], warnings: &[String]) -> serde_json::Value {
    serde_json::json!({ "errors": errors, "warnings": warnings })
}

pub fn project_entity(kb: &KnowledgeEntryRecord) -> WorldKbEntityProjection {
    let body_value = kb
        .body
        .as_ref()
        .map(|b| serde_json::to_value(b).unwrap_or_default());
    let aliases = body_value
        .as_ref()
        .and_then(|v| v.get("attributes"))
        .and_then(|a| a.get("aliases"))
        .and_then(|a| a.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(std::string::ToString::to_string))
                .collect::<Vec<_>>()
        });
    let source_anchor_count = u64::from(kb.source_work_id.is_some());
    WorldKbEntityProjection {
        key_block_id: kb.entry_id.clone(),
        world_id: kb.world_id().map(str::to_string).unwrap_or_default(),
        block_type: wire_cast(kb.block_type),
        canonical_name: wire_cast(kb.canonical_name.clone()),
        status: kb.status.clone(),
        version: kb.revision.unwrap_or(0),
        body: body_value
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default(),
        modules: kb
            .modules
            .as_ref()
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default(),
        aliases: aliases.unwrap_or_default(),
        source_anchor_count: Some(source_anchor_count),
        updated_at: kb.updated_at.clone(),
        // v1.191 P1 T2: the native governance pair is projected verbatim
        // (both absent for an in-scope shared row); it is never derived from
        // the narrative owner. T9 owns the read-policy polish.
        holder_entry_id: wire_cast(kb.holder_entry_id.clone()),
        disclosure: wire_cast(kb.disclosure.clone()),
    }
}

fn parse_block_type(s: &str) -> BlockType {
    use BlockType::{
        Ability, Act, Beat, Conflict, Deity, Dialogue, EconomyTier, Event, Faction, InfoPoint,
        Item, Level, MagicSystem, Organization, Scene, Species, Technology,
    };
    match s {
        "ability" => Ability,
        "scene" => Scene,
        "organization" => Organization,
        "item" => Item,
        "conflict" => Conflict,
        "info_point" => InfoPoint,
        "event" => Event,
        "species" => Species,
        "faction" => Faction,
        "magic_system" => MagicSystem,
        "technology" => Technology,
        "deity" => Deity,
        "level" => Level,
        "economy_tier" => EconomyTier,
        "dialogue" => Dialogue,
        "beat" => Beat,
        "act" => Act,
        _ => BlockType::Character,
    }
}

fn project_candidate(
    c: &nexus_local_db::kb_extract_job::KbExtractPromotion,
) -> WorldKbCandidateProjection {
    WorldKbCandidateProjection {
        candidate_id: c.job_id.clone(),
        job_id: c.job_id.clone(),
        world_id: c.world_id.clone(),
        block_type: wire_cast(parse_block_type(
            c.block_type_guess.as_deref().unwrap_or("character"),
        )),
        canonical_name: c.canonical_name_guess.clone().unwrap_or_default(),
        status: Some(c.promotion_status.clone()),
        version: u64::try_from(c.version).unwrap_or(0),
        source_anchor_count: Some(u64::from(c.work_id.is_some())),
        created_at: Some(c.created_at.clone()),
    }
}

pub mod guards {
    use super::{db_err, Sqlite};
    use crate::error::{CoreError, CoreResult};

    /// Failure of the shared world-ownership guard: a storage error already
    /// mapped onto the core taxonomy, or a typed ownership denial.
    pub enum WorldOwnerGuardFailure {
        Db(CoreError),
        Denied(WorldOwnerDenial),
    }

    impl WorldOwnerGuardFailure {
        /// World-KB family rendering (kb / pack / fork / rules / findings
        /// routes): retained `world {id}` 404 plus the cross-author and
        /// unowned 403 reasons.
        pub(crate) fn into_core_error(self) -> CoreError {
            match self {
                Self::Db(e) => e,
                Self::Denied(d) => d.into_core_error(),
            }
        }

        /// Timeline-events family rendering: retained
        /// `world {id} not found` 404 and `you do not own this world` 403.
        pub(crate) fn into_timeline_error(self) -> CoreError {
            match self {
                Self::Db(e) => e,
                Self::Denied(d) => d.into_timeline_error(),
            }
        }
    }

    /// Typed world-ownership denial from the shared
    /// `narrative_worlds.owner_creator_id` guard. The world id and the
    /// refusal stay split so each transport family renders its retained
    /// envelope verbatim at the adapter boundary.
    pub enum WorldOwnerDenial {
        /// No `narrative_worlds` row for the id.
        Missing { world_id: String },
        /// Row exists; `owner_creator_id` names another creator.
        Foreign { world_id: String },
        /// Row exists; `owner_creator_id` is NULL.
        Unowned { world_id: String },
    }

    impl WorldOwnerDenial {
        /// World-KB family rendering of a pure denial.
        fn into_core_error(self) -> CoreError {
            match self {
                Self::Missing { world_id } => {
                    CoreError::NotFound { resource: format!("world {world_id}") }
                }
                Self::Foreign { world_id } => CoreError::WorldOwnerDenied {
                    world_id,
                    reason: "active creator does not own this world; cross-author World KB edits are forbidden".to_string(),
                },
                Self::Unowned { world_id } => CoreError::WorldOwnerDenied {
                    world_id,
                    reason: "world has no owner_creator_id; cannot authorize World KB edit".to_string(),
                },
            }
        }

        /// Timeline-events family rendering of a pure denial.
        fn into_timeline_error(self) -> CoreError {
            match self {
                Self::Missing { world_id } => CoreError::NotFound {
                    resource: format!("world {world_id} not found"),
                },
                Self::Foreign { world_id } | Self::Unowned { world_id } => {
                    CoreError::WorldOwnerDenied {
                        world_id,
                        reason: "you do not own this world".to_string(),
                    }
                }
            }
        }
    }

    /// Shared world-ownership guard: the world must exist and be owned by
    /// `creator_id`. One SQL authority; callers pick the family rendering.
    pub async fn check_world_owner(
        executor: impl sqlx::Executor<'_, Database = Sqlite>,
        world_id: &str,
        creator_id: &str,
    ) -> Result<(), WorldOwnerGuardFailure> {
        let owner: Option<Option<String>> =
            sqlx::query_scalar("SELECT owner_creator_id FROM narrative_worlds WHERE world_id = ?")
                .bind(world_id)
                .fetch_optional(executor)
                .await
                .map_err(|e| WorldOwnerGuardFailure::Db(db_err(&e)))?;
        match owner {
            None => Err(WorldOwnerGuardFailure::Denied(WorldOwnerDenial::Missing {
                world_id: world_id.to_string(),
            })),
            Some(Some(owner_id)) if owner_id == creator_id => Ok(()),
            Some(Some(_)) => Err(WorldOwnerGuardFailure::Denied(WorldOwnerDenial::Foreign {
                world_id: world_id.to_string(),
            })),
            Some(None) => Err(WorldOwnerGuardFailure::Denied(WorldOwnerDenial::Unowned {
                world_id: world_id.to_string(),
            })),
        }
    }

    /// World-KB family convenience wrapper over [`check_world_owner`].
    pub async fn require_world_owner(
        executor: impl sqlx::Executor<'_, Database = Sqlite>,
        world_id: &str,
        creator_id: &str,
    ) -> CoreResult<()> {
        check_world_owner(executor, world_id, creator_id)
            .await
            .map_err(WorldOwnerGuardFailure::into_core_error)
    }
}

pub mod graph {
    use super::{
        db_err, guards, list_by_world_in_tx, list_relationships_for_world_in_tx, local_db_err,
        project_entity, store_err, warn, wire_cast, CoreAccess, CoreResult, SqlitePool,
        WorldKbGraphResponse, WorldKbRelationshipProjection, WorldKbSourceAnchorProjection,
        GRAPH_ENTITY_CAP, GRAPH_RELATIONSHIP_CAP,
    };

    pub async fn get_graph(
        pool: &SqlitePool,
        _access: CoreAccess,
        creator_id: &str,
        world_id: &str,
        include_suggested: bool,
    ) -> CoreResult<WorldKbGraphResponse> {
        let mut tx = pool.begin().await.map_err(|e| db_err(&e))?;
        guards::require_world_owner(&mut *tx, world_id, creator_id).await?;

        let blocks = list_by_world_in_tx(&mut tx, world_id, GRAPH_ENTITY_CAP)
            .await
            .map_err(|e| store_err(&e))?;
        let fetch_limit = i64::try_from(GRAPH_RELATIONSHIP_CAP + 1).unwrap_or(i64::MAX);
        let rows =
            list_relationships_for_world_in_tx(&mut tx, world_id, include_suggested, fetch_limit)
                .await
                .map_err(local_db_err)?;

        let mut entities = Vec::with_capacity(blocks.len().min(GRAPH_ENTITY_CAP));
        let mut source_anchors = Vec::new();
        for kb in blocks.into_iter().take(GRAPH_ENTITY_CAP) {
            if kb.status == "deleted" {
                continue;
            }
            if kb.source_work_id.is_some() {
                let reference = match kb.source_chapter {
                    Some(ch) => format!(
                        "work:{},chapter:{ch}",
                        kb.source_work_id.clone().unwrap_or_default()
                    ),
                    None => format!("work:{}", kb.source_work_id.clone().unwrap_or_default()),
                };
                source_anchors.push(WorldKbSourceAnchorProjection {
                    source_anchor_id: format!("sa_{}", kb.entry_id),
                    key_block_id: kb.entry_id.clone(),
                    source_type: kb
                        .source_provenance_kind
                        .clone()
                        .unwrap_or_else(|| "manual".to_string()),
                    reference,
                    created_at: Some(kb.created_at.clone()),
                });
            }
            entities.push(project_entity(&kb));
        }

        let relationships = project_relationships_for_world(&rows, include_suggested);
        tx.commit().await.map_err(|e| db_err(&e))?;
        Ok(WorldKbGraphResponse {
            entities: wire_cast(entities),
            source_anchors: wire_cast(source_anchors),
            relationships: wire_cast(relationships),
        })
    }

    fn project_relationships_for_world(
        rows: &[nexus_local_db::kb_relationships::KbRelationshipRow],
        include_suggested: bool,
    ) -> Vec<WorldKbRelationshipProjection> {
        let observed = rows.len();
        if observed > GRAPH_RELATIONSHIP_CAP {
            warn!(
                metric = "world_kb_graph_relationships_truncated",
                include_suggested,
                cap = GRAPH_RELATIONSHIP_CAP,
                observed_count = observed,
                "graph relationship cap reached"
            );
        }

        let mut projections = Vec::with_capacity(rows.len().min(GRAPH_RELATIONSHIP_CAP) * 2);
        for row in rows.iter().take(GRAPH_RELATIONSHIP_CAP) {
            projections.push(project_relationship(row, "stored"));
            if row.symmetric != 0 {
                let mut reverse = row.clone();
                std::mem::swap(&mut reverse.source_entity_id, &mut reverse.target_entity_id);
                projections.push(project_relationship(&reverse, "symmetric_reverse"));
            }
        }
        projections
    }

    pub(super) fn project_relationship(
        row: &nexus_local_db::kb_relationships::KbRelationshipRow,
        direction: &str,
    ) -> WorldKbRelationshipProjection {
        use nexus_contracts::WorldKbRelationshipKind;

        let source_anchor_ids = row
            .source_anchor_ids
            .as_deref()
            .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
            .unwrap_or_default();
        let metadata = row
            .metadata
            .as_deref()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok());

        let relation_type: WorldKbRelationshipKind =
            row.relation_type.parse().unwrap_or_else(|_| {
                warn!(
                    metric = "world_kb_relation_type_coercion",
                    relationship_id = row.relationship_id,
                    world_id = row.world_id,
                    relation_type = row.relation_type,
                    direction,
                    "unknown relation_type stored in kb_relationships; projecting as Custom"
                );
                WorldKbRelationshipKind::Custom
            });

        WorldKbRelationshipProjection {
            relationship_id: row.relationship_id.clone(),
            world_id: row.world_id.clone(),
            source_entity_id: row.source_entity_id.clone(),
            target_entity_id: row.target_entity_id.clone(),
            relation_type: wire_cast(relation_type),
            custom_label: row.custom_label.clone(),
            symmetric: row.symmetric != 0,
            confidence: row.confidence,
            source_anchor_ids,
            metadata: metadata
                .and_then(|v| v.as_object().cloned())
                .unwrap_or_default(),
            needs_review: row.needs_review != 0,
            source: wire_cast(row.source.clone()),
            version: u64::try_from(row.revision).unwrap_or(0),
            updated_at: row.updated_at.clone(),
            projection_direction: wire_cast(direction.to_string()),
        }
    }
}

pub mod candidates {
    use super::{
        db_err, guards, list_pending_for_world_after, project_candidate, wire_cast, CoreError,
        CoreResult, PaginationInfo, SqlitePool, WorldKbCandidateProjection,
        WorldKbCandidatesResponse, CANDIDATE_CURSOR_PREFIX, DEFAULT_CANDIDATE_LIMIT,
        MAX_CANDIDATE_LIMIT,
    };

    pub async fn get_candidates(
        pool: &SqlitePool,
        creator_id: &str,
        world_id: &str,
        limit: Option<i64>,
        cursor: Option<String>,
    ) -> CoreResult<WorldKbCandidatesResponse> {
        let mut tx = pool.begin().await.map_err(|e| db_err(&e))?;
        guards::require_world_owner(&mut *tx, world_id, creator_id).await?;

        let limit = limit
            .unwrap_or(DEFAULT_CANDIDATE_LIMIT)
            .clamp(1, MAX_CANDIDATE_LIMIT);
        let limit_us = usize::try_from(limit).unwrap_or(usize::MAX);
        let (cursor_created_at, cursor_job_id) = decode_candidate_cursor(cursor.as_ref())?;

        let pending = list_pending_for_world_after(
            &mut *tx,
            world_id,
            cursor_created_at.as_deref(),
            cursor_job_id.as_deref(),
            limit + 1,
        )
        .await
        .map_err(|e| db_err(&e))?;

        let next_cursor = if pending.len() > limit_us {
            let last = &pending[limit_us - 1];
            Some(encode_candidate_cursor(&last.created_at, &last.job_id))
        } else {
            None
        };
        let has_more = next_cursor.is_some();

        let items: Vec<WorldKbCandidateProjection> = pending
            .iter()
            .take(limit_us)
            .map(project_candidate)
            .collect();

        tx.commit().await.map_err(|e| db_err(&e))?;
        Ok(WorldKbCandidatesResponse {
            items: wire_cast(items),
            pagination: wire_cast(PaginationInfo {
                limit,
                has_more,
                next_cursor,
            }),
        })
    }

    fn decode_candidate_cursor(
        cursor: Option<&String>,
    ) -> CoreResult<(Option<String>, Option<String>)> {
        let Some(raw) = cursor else {
            return Ok((None, None));
        };
        let stripped =
            raw.strip_prefix(CANDIDATE_CURSOR_PREFIX)
                .ok_or_else(|| CoreError::InvalidInput {
                    field: "cursor".to_string(),
                    reason: "invalid candidates cursor; pass next_cursor unchanged".to_string(),
                })?;
        let mut parts = stripped.splitn(2, '|');
        let created_at =
            parts
                .next()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| CoreError::InvalidInput {
                    field: "cursor".to_string(),
                    reason: "invalid candidates cursor: missing created_at".to_string(),
                })?;
        let job_id =
            parts
                .next()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| CoreError::InvalidInput {
                    field: "cursor".to_string(),
                    reason: "invalid candidates cursor: missing job_id".to_string(),
                })?;
        Ok((Some(created_at.to_string()), Some(job_id.to_string())))
    }

    fn encode_candidate_cursor(created_at: &str, job_id: &str) -> String {
        format!("{CANDIDATE_CURSOR_PREFIX}{created_at}|{job_id}")
    }
}

pub mod patch {
    use super::{
        db_err, get_knowledge_entry_in_tx, guards, is_world_conflict_reject,
        knowledge_record_to_spoke, local_db_err, project_entity, put_knowledge_entry_in_tx,
        resolve_authored_governance, set_nexus_body, spoke_to_knowledge_record, store_err,
        validate_body, validate_canonical_name, validation_summary, wire_cast, BlockType,
        CoreError, CoreResult, HashMap, KbStoreError, KnowledgeAudience, KnowledgeAuthoringOp,
        KnowledgeEntryBody, KnowledgeEntryRecord, KnowledgeGovernance, NexusWorldKbEntityPatch,
        NexusWorldKbEntityPatchModulesKey, NexusWorldKbEntityPatchModulesValue, SpokeKnowledgeEntry,
        NexusWorldKbEntityPatchAudience, SpokeReject, SpokeRejectCode, SpokeResult, Sqlite,
        SqlitePool, ValidationMode, WorldKbPatchEntityRequest, WorldKbPatchEntityResponse,
    };

    pub async fn patch_entity(
        pool: &SqlitePool,
        creator_id: &str,
        world_id: &str,
        req: WorldKbPatchEntityRequest,
    ) -> CoreResult<WorldKbPatchEntityResponse> {
        let mut tx = nexus_local_db::begin_immediate(pool)
            .await
            .map_err(local_db_err)?;
        guards::require_world_owner(&mut *tx, world_id, creator_id).await?;

        let kb_opt = fetch_entry_in_tx(&mut tx, &req.entity_id).await?;

        if kb_opt.is_none() {
            if req.expected_version == 0 {
                return patch_entity_create_in_tx(pool, tx, world_id, creator_id, &req).await;
            }
            return Err(CoreError::world_kb_conflict(
                0,
                &req.entity_id,
                "version",
                "the entity does not exist; refetch the World KB graph and reapply",
            ));
        }

        let kb = kb_opt.expect("checked above");
        if kb.world_id() != Some(world_id) {
            return Err(CoreError::NotFound {
                resource: format!("entity {} in world {world_id}", req.entity_id),
            });
        }
        if kb.status == "deleted" || kb.status == "merged" {
            return Err(CoreError::world_kb_validation_failed(
                &["terminal entities cannot be patched".to_string()],
                &[],
            ));
        }

        let current_version = kb.revision.unwrap_or(0);
        if req.expected_version != current_version {
            return Err(CoreError::world_kb_conflict(
                current_version,
                req.entity_id.clone(),
                "version",
                "refetch the World KB graph and reapply",
            ));
        }
        if patch_is_empty(&req.patch) {
            return Err(CoreError::InvalidInput {
                field: "patch".to_string(),
                reason: "at least one of title/body/aliases/block_type/modules/audience must be \
                         provided"
                    .to_string(),
            });
        }
        // v1.191 P1 T7 (durable §3): the admitted audience mutation moves the
        // native governance pair in the same revision as the content edit.
        let governance = resolve_world_patch_governance(
            pool,
            creator_id,
            world_id,
            req.patch.audience.as_ref(),
        )
        .await?;
        if !patch_has_content(&req.patch) && governance_is_stored(&kb, governance.as_ref()) {
            // No-op: identical content and identical governance. Both
            // revisions stay untouched (durable §3).
            let response = WorldKbPatchEntityResponse {
                entity: wire_cast(project_entity(&kb)),
                version: current_version,
                validation_summary: wire_cast(validation_summary(&[], &[])),
            };
            tx.commit().await.map_err(|e| db_err(&e))?;
            return Ok(response);
        }

        let new_name = req.patch.title.as_ref().map(|t| t.to_string());
        let new_block_type = req.patch.block_type;
        let body_for_validation = compute_body(&kb, &req.patch)?;

        if let Some(ref name) = new_name {
            validate_canonical_name(name)
                .map_err(|e| CoreError::world_kb_validation_failed(&[e.to_string()], &[]))?;
        }
        let validation_block_type = new_block_type.map_or(kb.block_type, wire_cast);
        if let Some(ref body) = body_for_validation {
            validate_body(validation_block_type, Some(body), ValidationMode::Novel)
                .map_err(|e| CoreError::world_kb_validation_failed(&[e.to_string()], &[]))?;
        }

        let post_patch = build_post_patch(&kb, &req.patch, body_for_validation.as_ref());
        let spoke_entry = build_spoke_entry(&post_patch);
        let put_result =
            put_knowledge_entry_in_tx(pool, &mut tx, spoke_entry, Some(current_version)).await;
        let persisted = match map_put_response(put_result, &req.entity_id, &mut tx).await {
            Ok(entry) => entry,
            Err(e) => {
                return Err(e);
            }
        };
        // The governance half of the same authoring transaction: write the
        // resolved pair (or preserve it), re-assert the reverse WorldSheet
        // rule, and bump the owning World's knowledge revision exactly once.
        nexus_local_db::kb_store::author_world_knowledge_governance_tx(
            &mut tx,
            world_id,
            &req.entity_id,
            governance.as_ref(),
            nexus_local_db::kb_store::AuthoringRevision::Bump,
        )
        .await
        .map_err(authoring_db_err)?;
        let new_version = persisted.revision.unwrap_or(0);
        let response = WorldKbPatchEntityResponse {
            entity: wire_cast(project_entity(&persisted)),
            version: new_version,
            validation_summary: wire_cast(validation_summary(&[], &[])),
        };
        tx.commit().await.map_err(|e| db_err(&e))?;
        Ok(response)
    }

    /// Resolve the authored World-row audience into the native governance pair
    /// (durable §3).
    ///
    /// Admission — never the author — resolves the holder: `author-only` is
    /// the admitted controlling Creator's holder, and `character-private`
    /// requires an owned **active** Character that holds an active binding to
    /// this owned World. A malformed or unpermitted identity refuses with no
    /// mutation.
    ///
    /// # Errors
    ///
    /// [`CoreError::InvalidInput`] for a malformed/unpermitted audience,
    /// [`CoreError::NotFound`] for a foreign/missing Character, and the
    /// retained activation conflicts otherwise.
    async fn resolve_world_patch_governance(
        pool: &SqlitePool,
        creator_id: &str,
        world_id: &str,
        wire: Option<&NexusWorldKbEntityPatchAudience>,
    ) -> CoreResult<Option<KnowledgeGovernance>> {
        let audience = match wire {
            None => None,
            Some(NexusWorldKbEntityPatchAudience::Shared) => Some(KnowledgeAudience::Shared),
            Some(NexusWorldKbEntityPatchAudience::AuthorOnly) => Some(KnowledgeAudience::AuthorOnly),
            Some(NexusWorldKbEntityPatchAudience::CharacterPrivate(character_id)) => Some(
                KnowledgeAudience::character_private(character_id.to_string()).map_err(|e| {
                    CoreError::InvalidInput {
                        field: "patch.audience".to_string(),
                        reason: e.to_string(),
                    }
                })?,
            ),
        };
        let resolved_holder = match &audience {
            None | Some(KnowledgeAudience::Shared) => None,
            Some(KnowledgeAudience::AuthorOnly) => Some(
                crate::actors::require_actor_holder(
                    pool,
                    creator_id,
                    &crate::actors::AdmittedActor::Creator {
                        creator_id: creator_id.to_string(),
                    },
                )
                .await?,
            ),
            Some(KnowledgeAudience::CharacterPrivate { character_id }) => {
                crate::actors::require_active_owned_character(pool, creator_id, character_id)
                    .await?;
                if !nexus_local_db::has_active_binding_to_world(
                    pool,
                    creator_id,
                    character_id,
                    world_id,
                )
                .await
                .map_err(crate::error::actor_db_err)?
                {
                    return Err(CoreError::InvalidInput {
                        field: "patch.audience".to_string(),
                        reason: format!(
                            "character-private audience requires the named Character to hold an \
                             active binding to world {world_id}"
                        ),
                    });
                }
                Some(
                    crate::actors::require_actor_holder(
                        pool,
                        creator_id,
                        &crate::actors::AdmittedActor::Character {
                            character_id: character_id.clone(),
                        },
                    )
                    .await?,
                )
            }
        };
        resolve_authored_governance(
            KnowledgeAuthoringOp::Patch,
            audience.as_ref(),
            resolved_holder.as_deref(),
        )
        .map_err(|e| CoreError::InvalidInput {
            field: "patch.audience".to_string(),
            reason: e.to_string(),
        })
    }

    /// Whether the patch asks for any material content field.
    fn patch_has_content(patch: &NexusWorldKbEntityPatch) -> bool {
        patch.title.is_some()
            || !patch.body.is_empty()
            || !patch.aliases.is_empty()
            || patch.block_type.is_some()
            || !patch.modules.is_empty()
    }

    /// Whether the resolved governance equals the stored pair — an authored
    /// audience that changes nothing.
    fn governance_is_stored(kb: &KnowledgeEntryRecord, governance: Option<&KnowledgeGovernance>) -> bool {
        match governance {
            None => true,
            Some(governance) => {
                kb.holder_entry_id == governance.holder_entry_id
                    && kb.disclosure == governance.disclosure
            }
        }
    }

    /// Map the admitted World authoring side-car refusal: the stable
    /// `invalid_world_sheet` contract conflict keeps its wire code, everything
    /// else keeps the retained world-kb storage mapping.
    fn authoring_db_err(err: nexus_local_db::LocalDbError) -> CoreError {
        if matches!(
            &err,
            nexus_local_db::LocalDbError::ActorContractConflict {
                code: nexus_local_db::ActorContractConflict::InvalidWorldSheet,
            }
        ) {
            return crate::error::actor_db_err(err);
        }
        local_db_err(err)
    }

    async fn patch_entity_create_in_tx(
        pool: &SqlitePool,
        mut tx: sqlx::Transaction<'_, Sqlite>,
        world_id: &str,
        creator_id: &str,
        req: &WorldKbPatchEntityRequest,
    ) -> CoreResult<WorldKbPatchEntityResponse> {
        guards::require_world_owner(&mut *tx, world_id, creator_id).await?;

        let Some(title) = req.patch.title.as_ref() else {
            return Err(CoreError::world_kb_validation_failed(
                &["create requires patch.title (canonical_name)".to_string()],
                &[],
            ));
        };
        let Some(block_type) = req.patch.block_type else {
            return Err(CoreError::world_kb_validation_failed(
                &["create requires patch.block_type".to_string()],
                &[],
            ));
        };
        let block_type: BlockType = wire_cast(block_type);

        if title.trim().is_empty() {
            return Err(CoreError::world_kb_validation_failed(
                &[
                    "create requires a non-empty patch.title (whitespace-only titles are rejected)"
                        .to_string(),
                ],
                &[],
            ));
        }

        validate_canonical_name(title.as_str())
            .map_err(|e| CoreError::world_kb_validation_failed(&[e.to_string()], &[]))?;

        let entity_id_conforms = req.entity_id.strip_prefix("kb_").is_some_and(|suffix| {
            !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_hexdigit())
        });
        if !entity_id_conforms {
            return Err(CoreError::world_kb_validation_failed(
                &["entity_id must follow kb_<hex> convention".to_string()],
                &[],
            ));
        }

        let mut fresh = KnowledgeEntryRecord::new(world_id, block_type, title.as_str());
        fresh.entry_id = req.entity_id.clone();
        fresh.revision = Some(0);

        let body_for_validation = compute_body(&fresh, &req.patch)?;
        if let Some(body) = &body_for_validation {
            validate_body(block_type, Some(body), ValidationMode::Novel)
                .map_err(|e| CoreError::world_kb_validation_failed(&[e.to_string()], &[]))?;
        }
        fresh.body = body_for_validation;
        apply_patch_modules(&mut fresh, None, &req.patch);

        // v1.191 P1 T7 (durable §3): resolve the authored audience before the
        // row is written, so an unpermitted identity refuses with no mutation.
        let governance = resolve_world_patch_governance(
            pool,
            creator_id,
            world_id,
            req.patch.audience.as_ref(),
        )
        .await?;

        let spoke_entry = build_spoke_entry(&fresh);
        let put_result = put_knowledge_entry_in_tx(pool, &mut tx, spoke_entry, None).await;
        let persisted = match map_put_response(put_result, &req.entity_id, &mut tx).await {
            Ok(entry) => entry,
            Err(e) => {
                return Err(e);
            }
        };
        // A create authors governance under the same transaction as the row.
        // The revision pair stays put: it is the invalidation signal for the
        // entries an admitted snapshot may already hold, and T5's accepted
        // admission fixture pins `world: 0` across creates (durable §4.3
        // speaks of changes to a stored entry's governance).
        nexus_local_db::kb_store::author_world_knowledge_governance_tx(
            &mut tx,
            world_id,
            &req.entity_id,
            governance.as_ref(),
            nexus_local_db::kb_store::AuthoringRevision::Keep,
        )
        .await
        .map_err(authoring_db_err)?;
        let new_version = persisted.revision.unwrap_or(0);
        let response = WorldKbPatchEntityResponse {
            entity: wire_cast(project_entity(&persisted)),
            version: new_version,
            validation_summary: wire_cast(validation_summary(&[], &[])),
        };
        tx.commit().await.map_err(|e| db_err(&e))?;
        Ok(response)
    }

    async fn fetch_entry_in_tx(
        tx: &mut sqlx::Transaction<'_, Sqlite>,
        entity_id: &str,
    ) -> CoreResult<Option<KnowledgeEntryRecord>> {
        match get_knowledge_entry_in_tx(tx, entity_id).await {
            Ok(row) => Ok(Some(row)),
            Err(KbStoreError::NotFound(_)) => Ok(None),
            Err(e) => Err(store_err(&e)),
        }
    }

    async fn map_put_response(
        result: SpokeResult<SpokeKnowledgeEntry>,
        entity_id: &str,
        tx: &mut sqlx::Transaction<'_, Sqlite>,
    ) -> CoreResult<KnowledgeEntryRecord> {
        match result {
            SpokeResult::Ok(spoke_wire) => {
                let wire = serde_json::to_value(&spoke_wire).map_err(|e| CoreError::Internal {
                    category: format!("spoke decode: {e}"),
                })?;
                let spoke_entry: SpokeKnowledgeEntry =
                    serde_json::from_value(wire).map_err(|e| CoreError::Internal {
                        category: format!("spoke shape: {e}"),
                    })?;
                spoke_to_knowledge_record(spoke_entry).map_err(|e| CoreError::Internal {
                    category: format!("spoke owner ref: {e}"),
                })
            }
            SpokeResult::Reject(reject) => Err(map_put_reject(reject, entity_id, tx).await),
        }
    }

    pub(super) fn extract_store_revision(reject: &SpokeReject) -> Option<u64> {
        let details = reject.details.as_ref()?;
        for key in ["actualRevision", "storeRevision"] {
            if let Some(v) = details.get(key) {
                if let Some(n) = v.as_u64() {
                    return Some(n);
                }
            }
        }
        None
    }

    async fn reread_entity_revision_in_tx(
        tx: &mut sqlx::Transaction<'_, Sqlite>,
        entity_id: &str,
    ) -> u64 {
        fetch_entry_in_tx(tx, entity_id)
            .await
            .ok()
            .flatten()
            .and_then(|e| e.revision)
            .unwrap_or(0)
    }

    async fn map_put_reject(
        reject: SpokeReject,
        entity_id: &str,
        tx: &mut sqlx::Transaction<'_, Sqlite>,
    ) -> CoreError {
        let current = match extract_store_revision(&reject) {
            Some(r) => r,
            None => reread_entity_revision_in_tx(tx, entity_id).await,
        };
        if is_world_conflict_reject(&reject) {
            return CoreError::world_kb_conflict(
                current,
                entity_id,
                "world",
                "the entry moved to another world; refetch it in its stored world and reapply",
            );
        }
        match reject.code {
            SpokeRejectCode::StoredRevisionStale
            | SpokeRejectCode::RevisionConflict
            | SpokeRejectCode::KnowledgeEntryAlreadyExists => CoreError::world_kb_conflict(
                current,
                entity_id,
                "version",
                "refetch the World KB graph and reapply",
            ),
            SpokeRejectCode::KnowledgeEntryTerminalStatus
            | SpokeRejectCode::InvalidKnowledgeEntryStatus
            | SpokeRejectCode::InvalidKnowledgeEntryStatusTransition
            | SpokeRejectCode::EmptyCanonicalName
            | SpokeRejectCode::MissingRequiredField => {
                CoreError::world_kb_validation_failed(&[reject.message], &[])
            }
            SpokeRejectCode::InvalidInput => CoreError::InvalidInput {
                field: "knowledge_entry".to_string(),
                reason: reject.message,
            },
            SpokeRejectCode::InternalError => CoreError::Internal {
                category: format!("put_knowledge_entry: {}", reject.message),
            },
            _ => CoreError::Internal {
                category: format!(
                    "put_knowledge_entry rejected: {}: {}",
                    reject.code, reject.message
                ),
            },
        }
    }

    fn build_spoke_entry(entry: &KnowledgeEntryRecord) -> SpokeKnowledgeEntry {
        let mut spoke_entry: SpokeKnowledgeEntry = knowledge_record_to_spoke(entry);
        let body_value = entry
            .body
            .as_ref()
            .map(|b| serde_json::to_value(b).unwrap_or_default());
        set_nexus_body(&mut spoke_entry, body_value.as_ref());
        spoke_entry
    }

    fn patch_is_empty(patch: &NexusWorldKbEntityPatch) -> bool {
        !patch_has_content(patch) && patch.audience.is_none()
    }

    fn build_post_patch(
        kb: &KnowledgeEntryRecord,
        patch: &NexusWorldKbEntityPatch,
        body_for_validation: Option<&KnowledgeEntryBody>,
    ) -> KnowledgeEntryRecord {
        let mut post_patch = kb.clone();
        if let Some(ref name) = patch.title {
            post_patch.canonical_name = name.to_string();
        }
        if let Some(bt) = patch.block_type {
            post_patch.block_type = wire_cast(bt);
        }
        if let Some(body) = body_for_validation {
            post_patch.body = Some(body.clone());
        }
        post_patch.revision = Some(kb.revision.unwrap_or(0));
        apply_patch_modules(&mut post_patch, kb.modules.as_ref(), patch);
        post_patch
    }

    fn apply_patch_modules(
        post_patch: &mut KnowledgeEntryRecord,
        base: Option<&serde_json::Value>,
        patch: &NexusWorldKbEntityPatch,
    ) {
        if !patch.modules.is_empty() {
            post_patch.modules = merge_modules(base, &patch.modules);
        }
    }

    fn merge_modules(
        base: Option<&serde_json::Value>,
        provided: &HashMap<NexusWorldKbEntityPatchModulesKey, NexusWorldKbEntityPatchModulesValue>,
    ) -> Option<serde_json::Value> {
        if provided.is_empty() {
            return base.cloned();
        }
        let mut map = base
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();
        for (key, value) in provided {
            let json_val = match value {
                NexusWorldKbEntityPatchModulesValue::Object(obj) => {
                    serde_json::Value::Object(obj.clone())
                }
                NexusWorldKbEntityPatchModulesValue::Array(arr) => {
                    serde_json::Value::Array(arr.clone())
                }
            };
            map.insert(key.to_string(), json_val);
        }
        Some(serde_json::Value::Object(map))
    }

    fn compute_body(
        kb: &KnowledgeEntryRecord,
        patch: &NexusWorldKbEntityPatch,
    ) -> CoreResult<Option<KnowledgeEntryBody>> {
        if patch.body.is_empty() && patch.aliases.is_empty() {
            return Ok(None);
        }
        let mut value = if patch.body.is_empty() {
            kb.body.as_ref().map_or_else(
                || serde_json::json!({}),
                |b| serde_json::to_value(b).unwrap_or_default(),
            )
        } else {
            serde_json::Value::Object(patch.body.clone())
        };
        if !patch.aliases.is_empty() {
            let obj = value
                .as_object_mut()
                .ok_or_else(|| CoreError::InvalidInput {
                    field: "body".to_string(),
                    reason: "body must be a JSON object to set aliases".to_string(),
                })?;
            let attrs = obj
                .entry("attributes")
                .or_insert_with(|| serde_json::json!({}));
            attrs["aliases"] = serde_json::Value::Array(
                patch
                    .aliases
                    .iter()
                    .map(|a| serde_json::Value::String(a.clone()))
                    .collect(),
            );
        }
        let body: KnowledgeEntryBody =
            serde_json::from_value(value).map_err(|e| CoreError::InvalidInput {
                field: "body".to_string(),
                reason: format!("body is not a valid KnowledgeEntryBody: {e}"),
            })?;
        Ok(Some(body))
    }
}

fn store_err(e: &KbStoreError) -> CoreError {
    CoreError::Internal {
        category: format!("kb_store: {e}"),
    }
}

/// World KB promote/relationship ownership + canonical mutation surface
/// (v1.190 P0-T1). The service methods keep only auth/DTO translation on the
/// HTTP side; ownership, OCC CAS, canonical spoke orchestration and the
/// durable `core_changes` trail (outbox triggers) run here in private
/// transactions.
impl CoreService {
    /// Adopt/reject/merge a pending World KB candidate (entity-scope-model
    /// §5.5).
    ///
    /// # Errors
    /// Returns [`CoreError::AuthRequired`] when the principal or the on-disk
    /// selection fails verification, [`CoreError::Forbidden`] under read-only
    /// access or when the caller does not own the world,
    /// [`CoreError::NotFound`] for an unknown promotion job,
    /// [`CoreError::WorldKbValidation`] for terminal/duplicate candidates,
    /// and [`CoreError::WorldKbConflict`] when `expected_version` is stale.
    pub async fn promote_world_kb_candidate(
        &self,
        principal: &Principal,
        world_id: String,
        request: WorldKbPromoteCandidateRequest,
    ) -> CoreResult<WorldKbPromoteCandidateResponse> {
        self.verify_principal(principal)?;
        if self.inner.access == CoreAccess::ReadOnly {
            return Err(CoreError::Forbidden {
                resource: "world_kb_promote: read-only core access".to_string(),
            });
        }
        promote::promote_candidate(&self.inner.pool, principal.creator_id(), &world_id, request)
            .await
    }

    /// Add/update/remove a typed relationship between two World KB entities.
    ///
    /// # Errors
    /// Returns [`CoreError::AuthRequired`] when the principal or the on-disk
    /// selection fails verification, [`CoreError::Forbidden`] under read-only
    /// access, cross-world rows, or when the caller does not own the world,
    /// [`CoreError::NotFound`] for unknown relationships,
    /// [`CoreError::WorldKbValidation`] for invalid payloads, and
    /// [`CoreError::WorldKbConflict`] when `expected_version` is stale.
    pub async fn patch_world_kb_relationship(
        &self,
        principal: &Principal,
        world_id: String,
        request: WorldKbPatchRelationshipRequest,
    ) -> CoreResult<WorldKbPatchRelationshipResponse> {
        self.verify_principal(principal)?;
        if self.inner.access == CoreAccess::ReadOnly {
            return Err(CoreError::Forbidden {
                resource: "world_kb_relationship: read-only core access".to_string(),
            });
        }
        relationship::patch_relationship(
            &self.inner.pool,
            principal.creator_id(),
            &world_id,
            request,
        )
        .await
    }

    /// Computable `KnowledgeEntry` `body.state` read with the per-row OCC
    /// revision.
    ///
    /// # Errors
    /// Returns [`CoreError::AuthRequired`] when the principal or the on-disk
    /// selection fails verification, [`CoreError::Forbidden`] when the caller
    /// does not own the world, [`CoreError::NotFound`] for an unknown or
    /// out-of-world key block, and the mapped storage error otherwise.
    pub async fn world_kb_key_block_state(
        &self,
        principal: &Principal,
        world_id: String,
        key_block_id: String,
    ) -> CoreResult<WorldKbKeyBlockStateResponse> {
        self.verify_principal(principal)?;
        key_block_state::get_key_block_state(
            &self.inner.pool,
            principal.creator_id(),
            &world_id,
            &key_block_id,
        )
        .await
    }
}

pub mod promote {
    //! Candidate promotion state machine (adopt/reject/merge), ported from
    //! the daemon handler — adopt routes through `orchestrate_promote` with
    //! the job CAS flip in one transaction; reject/merge keep their CAS +
    //! reread contracts verbatim.

    use nexus_contracts::{
        WorldKbExtractJobProjection, WorldKbPromoteCandidateRequest,
        WorldKbPromoteCandidateResponse,
    };
    use nexus_knowledge::world_kb::KbStore;
    use nexus_local_db::kb_extract_job::{get_promotion, mark_confirmed_in_tx_with_cas};
    use nexus_local_db::kb_store::SqliteKbStore;
    use nexus_spoke_adapter::conversion::knowledge_record_to_spoke;
    use nexus_spoke_adapter::{
        orchestrate_promote, NexusAdapter, PromoteRequest, PromoteResponse, SpokeReject,
        SpokeRejectCode, SpokeResult,
    };
    use std::sync::{Arc, Mutex};

    use super::{
        db_err, guards, local_db_err, parse_block_type, project_entity, store_err,
        validation_summary, wire_cast, CoreError, CoreResult, KnowledgeEntryBody,
        KnowledgeEntryRecord, SpokeKnowledgeEntry, ValidationMode,
    };
    use crate::error::CoreError as RootCoreError;

    pub async fn promote_candidate(
        pool: &sqlx::SqlitePool,
        creator_id: &str,
        world_id: &str,
        req: WorldKbPromoteCandidateRequest,
    ) -> CoreResult<WorldKbPromoteCandidateResponse> {
        guards::require_world_owner(pool, world_id, creator_id).await?;

        // Load the promotion candidate.
        let candidate = get_promotion(pool, &req.job_id)
            .await
            .map_err(|e| db_err(&e))?
            .ok_or_else(|| CoreError::NotFound {
                resource: format!("promotion job {}", req.job_id),
            })?;
        if candidate.world_id != world_id {
            return Err(CoreError::NotFound {
                resource: format!("promotion job {} in world {world_id}", req.job_id),
            });
        }

        // Retry-safe idempotency: a prior adopt may have committed (entry +
        // job confirmed) while the client saw an error or is replaying.
        if candidate.promotion_status == "confirmed" && req.action.as_str() == "adopt" {
            if let Some(response) =
                try_idempotent_confirmed_adopt_response(pool, &candidate, &req).await?
            {
                return Ok(response);
            }
        }

        // Promotion transition validity: candidate must be pending.
        if candidate.promotion_status != "pending" {
            return Err(CoreError::world_kb_validation_failed(
                &[format!(
                    "candidate is in terminal state '{}' (entity-scope-model §5.5.2); \
                     only pending candidates can be adopted/rejected/merged",
                    candidate.promotion_status
                )],
                &[],
            ));
        }

        // OCC precondition on kb_extract_jobs.version.
        let current_version = u64::try_from(candidate.version).unwrap_or(0);
        if req.expected_version != current_version {
            return Err(CoreError::world_kb_conflict(
                current_version,
                &req.job_id,
                "version",
                "refetch the candidates list and reapply",
            ));
        }

        match req.action.as_str() {
            "adopt" => promote_adopt(pool, world_id, &candidate, &req).await,
            "reject" => promote_reject(pool, &candidate, &req).await,
            "merge" => promote_merge(pool, world_id, &candidate, &req).await,
            other => Err(CoreError::InvalidInput {
                field: "action".to_string(),
                reason: format!("action must be adopt|reject|merge, got '{other}'"),
            }),
        }
    }

    /// Resolved adopt inputs (parsed payload + optional patch refinements).
    struct AdoptPlan {
        body: KnowledgeEntryBody,
        block_type: nexus_contracts::BlockType,
        canonical_name: String,
    }

    /// Parse the candidate `proposed_payload` and apply optional `patch`
    /// refinements (`title`/`body`/`aliases`/`block_type`) into a validated
    /// adopt plan.
    fn build_adopt_plan(
        candidate: &nexus_local_db::kb_extract_job::KbExtractPromotion,
        req: &WorldKbPromoteCandidateRequest,
    ) -> CoreResult<AdoptPlan> {
        let mut body: KnowledgeEntryBody = serde_json::from_str(
            candidate.proposed_payload.as_deref().unwrap_or("{}"),
        )
        .map_err(|e| CoreError::Internal {
            category: format!("proposed_payload is not a valid KnowledgeEntryBody: {e}"),
        })?;
        let block_type = req.patch.as_ref().and_then(|p| p.block_type).map_or_else(
            || parse_block_type(candidate.block_type_guess.as_deref().unwrap_or("character")),
            wire_cast::<nexus_contracts::BlockType, _>,
        );
        let canonical_name = req
            .patch
            .as_ref()
            .and_then(|p| p.title.as_ref().map(|t| t.to_string()))
            .or_else(|| candidate.canonical_name_guess.clone())
            .ok_or_else(|| {
                CoreError::world_kb_validation_failed(
                    &["candidate has no canonical_name_guess and no patch.title".to_string()],
                    &[],
                )
            })?;
        if let Some(ref p) = req.patch {
            // Adopt refinements only consume title/body/aliases/block_type.
            // Reject instead of silently dropping provided modules (the
            // entity patch the request `$ref`s carries the key).
            if !p.modules.is_empty() {
                return Err(CoreError::InvalidInput {
                    field: "patch.modules".to_string(),
                    reason: "modules cannot be refined on promote-adopt; PATCH the KB entity \
                             directly after adoption"
                        .to_string(),
                });
            }
            if !p.body.is_empty() {
                body = serde_json::from_value(serde_json::Value::Object(p.body.clone())).map_err(
                    |e| CoreError::InvalidInput {
                        field: "patch.body".to_string(),
                        reason: format!("not a valid KnowledgeEntryBody: {e}"),
                    },
                )?;
            }
            if !p.aliases.is_empty() {
                merge_aliases_into_body(&mut body, &p.aliases);
            }
        }
        Ok(AdoptPlan {
            body,
            block_type,
            canonical_name,
        })
    }

    /// Set `body.attributes.aliases` in place.
    fn merge_aliases_into_body(body: &mut KnowledgeEntryBody, aliases: &[String]) {
        let mut value = serde_json::to_value(&*body).unwrap_or_default();
        if let Some(obj) = value.as_object_mut() {
            let attrs = obj
                .entry("attributes")
                .or_insert_with(|| serde_json::json!({}));
            attrs["aliases"] = serde_json::Value::Array(
                aliases
                    .iter()
                    .map(|a| serde_json::Value::String(a.clone()))
                    .collect(),
            );
        }
        if let Ok(merged) = serde_json::from_value::<KnowledgeEntryBody>(value) {
            *body = merged;
        }
    }

    /// Adopt: build a provisional candidate, route through
    /// `orchestrate_promote` (validates → promote acceptance → confirmed +
    /// revision bump → persists via the production adapter), then flip the
    /// promotion job in the **same** SQLite transaction.
    #[allow(clippy::too_many_lines)] // ported orchestration path
    async fn promote_adopt(
        pool: &sqlx::SqlitePool,
        world_id: &str,
        candidate: &nexus_local_db::kb_extract_job::KbExtractPromotion,
        req: &WorldKbPromoteCandidateRequest,
    ) -> CoreResult<WorldKbPromoteCandidateResponse> {
        let AdoptPlan {
            body,
            block_type,
            canonical_name,
        } = build_adopt_plan(candidate, req)?;

        crate::world_kb::validate_canonical_name(&canonical_name)
            .map_err(|e| CoreError::world_kb_validation_failed(&[e.to_string()], &[]))?;
        crate::world_kb::validate_body(block_type, Some(&body), ValidationMode::Novel)
            .map_err(|e| CoreError::world_kb_validation_failed(&[e.to_string()], &[]))?;

        // Build the candidate with `status = "provisional"` — the orchestrator
        // flips it to "confirmed" via `apply_promote_acceptance`.
        let mut kb = KnowledgeEntryRecord::new(world_id, block_type, &canonical_name);
        kb.body = Some(body);
        kb.status = "provisional".to_string();
        kb.created_at = chrono::Utc::now().to_rfc3339();
        kb.source_work_id = candidate.work_id.clone();
        kb.source_chapter = candidate.source_chapter_id;
        kb.source_provenance_kind = if candidate.llm_confidence.is_some() {
            Some("review_time_extract".to_string())
        } else {
            Some("manual".to_string())
        };
        kb.created_from_command_id = Some(req.job_id.clone());

        let tx = pool.begin().await.map_err(|e| db_err(&e))?;
        let tx_cell = Arc::new(Mutex::new(Some(tx)));
        let adapter = NexusAdapter::new(pool.clone()).with_tx_cell(Arc::clone(&tx_cell));

        let spoke_req = build_spoke_promote_request(&kb);
        let result = adapter
            .with_bound_tx(|| orchestrate_promote(&adapter, spoke_req))
            .await;
        let knowledge_entry = match map_promote_response(result, pool, &req.job_id, &kb).await {
            Ok(PromoteAdoptOrchestrateOutcome::RecoveredConfirmed(knowledge_entry)) => {
                if let Some(tx) = tx_cell.lock().ok().and_then(|mut guard| guard.take()) {
                    let _ = tx.rollback().await;
                }
                return build_promote_adopt_response(
                    pool,
                    &knowledge_entry,
                    candidate,
                    &req.job_id,
                )
                .await;
            }
            Ok(PromoteAdoptOrchestrateOutcome::Fresh(entry)) => entry,
            Err(e) => {
                let rollback_tx = tx_cell.lock().ok().and_then(|mut guard| guard.take());
                if let Some(tx) = rollback_tx {
                    let _ = tx.rollback().await;
                }
                return Err(e);
            }
        };

        let mut tx = tx_cell
            .lock()
            .expect("promote_adopt tx mutex poisoned")
            .take()
            .expect("promote_adopt tx cell must hold the open transaction");

        let expected_version = i64::try_from(req.expected_version).unwrap_or(0);
        match mark_confirmed_in_tx_with_cas(&mut tx, &req.job_id, expected_version).await {
            Ok(true) => match tx.commit().await {
                Ok(()) => {}
                Err(e) => {
                    handle_promote_adopt_commit_ambiguity(pool, &req.job_id, e).await?;
                }
            },
            Ok(false) => {
                let _ = tx.rollback().await;
                return Err(CoreError::world_kb_validation_failed(
                    &[
                        "candidate was no longer pending (already confirmed/rejected); \
                         rolled back the adopt transaction"
                            .to_string(),
                    ],
                    &[],
                ));
            }
            Err(e) => {
                let _ = tx.rollback().await;
                return Err(map_cas_err(e, &req.job_id, "version"));
            }
        }

        let store = SqliteKbStore::new(pool.clone());
        let kb_id = knowledge_entry.entry_id.clone();
        let updated_kb = store
            .get_knowledge_entry(&kb_id)
            .await
            .map_err(|e| store_err(&e))?;
        build_promote_adopt_response_from_kb(pool, &updated_kb, candidate, &req.job_id).await
    }

    async fn build_promote_adopt_response(
        pool: &sqlx::SqlitePool,
        knowledge_entry: &SpokeKnowledgeEntry,
        candidate: &nexus_local_db::kb_extract_job::KbExtractPromotion,
        job_id: &str,
    ) -> CoreResult<WorldKbPromoteCandidateResponse> {
        let store = SqliteKbStore::new(pool.clone());
        let updated_kb = store
            .get_knowledge_entry(&knowledge_entry.entry_id)
            .await
            .map_err(|e| store_err(&e))?;
        build_promote_adopt_response_from_kb(pool, &updated_kb, candidate, job_id).await
    }

    async fn build_promote_adopt_response_from_kb(
        pool: &sqlx::SqlitePool,
        updated_kb: &KnowledgeEntryRecord,
        candidate: &nexus_local_db::kb_extract_job::KbExtractPromotion,
        job_id: &str,
    ) -> CoreResult<WorldKbPromoteCandidateResponse> {
        let job = get_promotion(pool, job_id)
            .await
            .map_err(|e| db_err(&e))?
            .unwrap_or_else(|| candidate.clone());
        let new_version = u64::try_from(job.version).unwrap_or(0);

        Ok(WorldKbPromoteCandidateResponse {
            entity: Some(wire_cast(project_entity(updated_kb))),
            job: wire_cast(project_job(&job)),
            version: new_version,
            validation_summary: wire_cast(validation_summary(&[], &[])),
        })
    }

    /// Return the adopt response when the job is already confirmed and the
    /// active entry is attributed to this promotion job (client retry after
    /// durable success).
    async fn try_idempotent_confirmed_adopt_response(
        pool: &sqlx::SqlitePool,
        candidate: &nexus_local_db::kb_extract_job::KbExtractPromotion,
        req: &WorldKbPromoteCandidateRequest,
    ) -> CoreResult<Option<WorldKbPromoteCandidateResponse>> {
        let AdoptPlan {
            block_type,
            canonical_name,
            ..
        } = build_adopt_plan(candidate, req)?;
        let Some(existing) =
            find_active_entry_for(pool, &candidate.world_id, &canonical_name, block_type).await?
        else {
            return Ok(None);
        };
        if existing.created_from_command_id.as_deref() != Some(req.job_id.as_str()) {
            return Ok(None);
        }
        tracing::info!(
            job_id = %req.job_id,
            entry_id = %existing.entry_id,
            "promote_adopt idempotent retry: job already confirmed with attributed entry"
        );
        build_promote_adopt_response_from_kb(pool, &existing, candidate, &req.job_id)
            .await
            .map(Some)
    }

    /// Result of routing `orchestrate_promote` inside [`promote_adopt`].
    enum PromoteAdoptOrchestrateOutcome {
        /// Orchestrator created a new confirmed entry in the bound transaction.
        Fresh(SpokeKnowledgeEntry),
        /// Retry-safe recovery: job already confirmed and entry attributed to
        /// this job. Caller must skip job CAS and roll back the unused outer
        /// transaction.
        RecoveredConfirmed(SpokeKnowledgeEntry),
    }

    /// Action to take when `tx.commit()` fails after a successful in-tx adopt.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum PromoteAdoptCommitAmbiguityAction {
        /// Commit likely landed; treat adopt as success.
        TreatAsSuccess,
        /// Commit did not land (or outcome unclear); surface error to caller.
        Fail,
    }

    /// Pure decision table for commit-ack failures (unit-tested).
    fn promote_adopt_commit_ambiguity_action(
        job_status: Option<&str>,
    ) -> PromoteAdoptCommitAmbiguityAction {
        match job_status {
            Some("confirmed") => PromoteAdoptCommitAmbiguityAction::TreatAsSuccess,
            _ => PromoteAdoptCommitAmbiguityAction::Fail,
        }
    }

    /// Outcome of resolving a commit-ack failure after re-reading the job.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum PromoteAdoptCommitAmbiguityResolution {
        TreatAsSuccess,
        Fail,
    }

    /// Pure decision after a status re-read (unit-tested). `Err` means all
    /// re-read attempts failed — fail because the commit outcome is ambiguous.
    fn resolve_promote_adopt_commit_ambiguity_after_reread(
        reread: Result<Option<&str>, ()>,
    ) -> PromoteAdoptCommitAmbiguityResolution {
        match reread {
            Ok(status) => match promote_adopt_commit_ambiguity_action(status) {
                PromoteAdoptCommitAmbiguityAction::TreatAsSuccess => {
                    PromoteAdoptCommitAmbiguityResolution::TreatAsSuccess
                }
                PromoteAdoptCommitAmbiguityAction::Fail => {
                    PromoteAdoptCommitAmbiguityResolution::Fail
                }
            },
            Err(()) => PromoteAdoptCommitAmbiguityResolution::Fail,
        }
    }

    /// Re-read the promotion job after a commit error and apply the pure
    /// decision table.
    async fn handle_promote_adopt_commit_ambiguity(
        pool: &sqlx::SqlitePool,
        job_id: &str,
        commit_err: sqlx::Error,
    ) -> CoreResult<()> {
        let reread = reread_promotion_status_with_retry(pool, job_id).await;
        let resolution = resolve_promote_adopt_commit_ambiguity_after_reread(
            reread
                .as_ref()
                .map(|status| status.as_deref())
                .map_err(|_| ()),
        );

        match resolution {
            PromoteAdoptCommitAmbiguityResolution::TreatAsSuccess => {
                tracing::warn!(
                    job_id = %job_id,
                    "promote_adopt: commit returned error but job is confirmed; treating adopt as success"
                );
                Ok(())
            }
            PromoteAdoptCommitAmbiguityResolution::Fail => Err(
                promote_adopt_commit_ambiguity_error(&commit_err, reread.err().as_ref()),
            ),
        }
    }

    /// Re-read `kb_extract_jobs.promotion_status` with short retries after
    /// commit ack failures so transient read errors do not force an incorrect
    /// outcome.
    async fn reread_promotion_status_with_retry(
        pool: &sqlx::SqlitePool,
        job_id: &str,
    ) -> Result<Option<String>, sqlx::Error> {
        const MAX_ATTEMPTS: u32 = 3;
        let mut last_err = None;
        for attempt in 0..MAX_ATTEMPTS {
            if attempt > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(10 * u64::from(attempt))).await;
            }
            match get_promotion(pool, job_id).await {
                Ok(job) => return Ok(job.map(|j| j.promotion_status)),
                Err(e) => last_err = Some(e),
            }
        }
        Err(last_err.unwrap_or_else(|| sqlx::Error::RowNotFound))
    }

    /// Combine commit and optional re-read errors for ambiguity failure paths.
    #[allow(clippy::option_if_let_else)] // the match keeps both error branches side by side
    fn promote_adopt_commit_ambiguity_error(
        commit_err: &sqlx::Error,
        reread_err: Option<&sqlx::Error>,
    ) -> CoreError {
        match reread_err {
            Some(reread) => CoreError::Internal {
                category: format!(
                    "promote_adopt commit failed ({commit_err}) and status re-read failed ({reread})"
                ),
            },
            None => db_err(commit_err),
        }
    }

    /// Build a spoke [`PromoteRequest`] from a nexus [`KnowledgeEntryRecord`]
    /// candidate via the sole `knowledge_record_to_spoke` conversion seam,
    /// round-tripped through JSON to fit the `PromoteRequest.candidate` wire
    /// shape (the spoke codegen emits a distinct struct per wire shape).
    ///
    /// # Panics
    ///
    /// Panics if the round-trip fails — the candidate has already been through
    /// nexus validation, so a failure here indicates a wire-shape drift, not a
    /// runtime input error.
    fn build_spoke_promote_request(candidate: &KnowledgeEntryRecord) -> PromoteRequest {
        let spoke_entry: SpokeKnowledgeEntry = knowledge_record_to_spoke(candidate);
        let wire = serde_json::to_value(&spoke_entry).unwrap_or_else(|_| serde_json::json!({}));
        serde_json::from_value(serde_json::json!({ "candidate": wire }))
            .expect("KnowledgeEntry-derived candidate fits PromoteRequest.candidate shape")
    }

    /// Map `orchestrate_promote`'s result to the confirmed
    /// [`SpokeKnowledgeEntry`] on success, or to a [`CoreError`] on reject.
    async fn map_promote_response(
        result: SpokeResult<PromoteResponse>,
        pool: &sqlx::SqlitePool,
        job_id: &str,
        candidate_lookup: &KnowledgeEntryRecord,
    ) -> CoreResult<PromoteAdoptOrchestrateOutcome> {
        match result {
            SpokeResult::Ok(PromoteResponse::Variant0 { knowledge_entry, .. }) => {
                // Round-trip through JSON to coerce the response-only type
                // into the canonical data type downstream nexus code consumes.
                let wire = serde_json::to_value(&knowledge_entry).map_err(|e| {
                    CoreError::Internal {
                        category: format!(
                            "orchestrate_promote returned a non-serializable KnowledgeEntry: {e}"
                        ),
                    }
                })?;
                serde_json::from_value::<SpokeKnowledgeEntry>(wire)
                    .map(PromoteAdoptOrchestrateOutcome::Fresh)
                    .map_err(|e| CoreError::Internal {
                        category: format!(
                            "orchestrate_promote response did not match KnowledgeEntry shape: {e}"
                        ),
                    })
            }
            // Defensive: the orchestrator always surfaces errors via
            // `SpokeResult::Reject` (not Variant1).
            SpokeResult::Ok(PromoteResponse::Variant1 { .. }) => Err(CoreError::Internal {
                category: format!(
                    "orchestrate_promote returned a PromoteResponse::Variant1 error envelope for {job_id}; \
                     expected a SpokeResult::Reject (spoke wire-shape drift)"
                ),
            }),
            SpokeResult::Reject(reject) => {
                map_promote_reject(reject, pool, job_id, candidate_lookup).await
            }
        }
    }

    /// Reject-handling tail of [`map_promote_response`]: the retry-safe
    /// branch (unique-index fired) is async (re-reads job + entry).
    async fn map_promote_reject(
        reject: SpokeReject,
        pool: &sqlx::SqlitePool,
        job_id: &str,
        candidate_lookup: &KnowledgeEntryRecord,
    ) -> CoreResult<PromoteAdoptOrchestrateOutcome> {
        if reject.code == SpokeRejectCode::KnowledgeEntryAlreadyExists
            || reject.code == SpokeRejectCode::DuplicateActiveKnowledgeEntry
        {
            // Disambiguate via the promotion job's status: a confirmed job
            // means a prior partial attempt completed successfully.
            let job = get_promotion(pool, job_id).await.map_err(|e| db_err(&e))?;
            if let Some(job) = job {
                if job.promotion_status == "confirmed" {
                    if let Some(existing) = find_active_entry_for(
                        pool,
                        candidate_lookup.world_id().unwrap_or_default(),
                        &candidate_lookup.canonical_name,
                        candidate_lookup.block_type,
                    )
                    .await?
                    {
                        if existing.created_from_command_id.as_deref() == Some(job_id) {
                            tracing::info!(
                                job_id = %job_id,
                                entry_id = %existing.entry_id,
                                "promote_adopt retry-safe: returning existing confirmed entry from prior partial attempt"
                            );
                            return Ok(PromoteAdoptOrchestrateOutcome::RecoveredConfirmed(
                                knowledge_record_to_spoke(&existing),
                            ));
                        }
                        tracing::warn!(
                            job_id = %job_id,
                            entry_id = %existing.entry_id,
                            "promote_adopt retry-safe: confirmed job but active entry is not attributed to this job"
                        );
                    }
                    tracing::warn!(
                        job_id = %job_id,
                        "promote_adopt retry-safe: job is confirmed but no matching active entry found"
                    );
                } else if job.promotion_status == "pending" {
                    return Err(CoreError::world_kb_validation_failed(
                        &[
                            "an active KnowledgeEntryRecord with the same name/type already exists in this \
                             world while the promotion job is still pending (wait for the in-flight \
                             adopt to finish, refresh the candidates list, or use merge)"
                                .to_string(),
                        ],
                        &[],
                    ));
                }
            }
            return Err(CoreError::world_kb_validation_failed(
                &[
                    "an active KnowledgeEntryRecord with the same name/type already exists in this world \
                     (refresh the candidates list and retry)"
                        .to_string(),
                ],
                &[],
            ));
        }
        Err(spoke_reject_to_core_error(reject, pool, job_id).await)
    }

    /// Look up the active [`KnowledgeEntryRecord`] matching the same unique
    /// key as the candidate (retry-safe recovery lookup).
    async fn find_active_entry_for(
        pool: &sqlx::SqlitePool,
        world_id: &str,
        canonical_name: &str,
        block_type: nexus_contracts::BlockType,
    ) -> CoreResult<Option<KnowledgeEntryRecord>> {
        let store = SqliteKbStore::new(pool.clone());
        store
            .get_active_by_unique_key(world_id, canonical_name, block_type)
            .await
            .map_err(|e| store_err(&e))
    }

    /// Map a (non-retry-safe) [`SpokeReject`] to a [`CoreError`].
    async fn spoke_reject_to_core_error(
        reject: SpokeReject,
        pool: &sqlx::SqlitePool,
        job_id: &str,
    ) -> CoreError {
        // A world-mismatch CAS miss maps to the 409 conflict family with
        // kind "world" — never a fabricated version conflict.
        if nexus_spoke_adapter::is_world_conflict_reject(&reject) {
            let current = reread_promotion_version(pool, job_id).await.unwrap_or(0);
            return RootCoreError::world_kb_conflict(
                current,
                job_id,
                "world",
                "the entry moved to another world; refetch it in its stored world and reapply",
            );
        }
        match reject.code {
            SpokeRejectCode::MissingRequiredField
            | SpokeRejectCode::EmptyCanonicalName
            | SpokeRejectCode::CandidateTerminalStatus
            | SpokeRejectCode::CandidateNotProvisional
            | SpokeRejectCode::MergeTargetSelf
            | SpokeRejectCode::InvalidKnowledgeEntryStatus
            | SpokeRejectCode::InvalidKnowledgeEntryStatusTransition
            | SpokeRejectCode::DuplicateActiveKnowledgeEntry
            | SpokeRejectCode::KnowledgeEntryTerminalStatus => {
                CoreError::world_kb_validation_failed(&[reject.message], &[])
            }
            SpokeRejectCode::InvalidInput => CoreError::InvalidInput {
                field: "promotion".to_string(),
                reason: reject.message,
            },
            SpokeRejectCode::RevisionConflict | SpokeRejectCode::StoredRevisionStale => {
                let current = reread_promotion_version(pool, job_id).await.unwrap_or(0);
                RootCoreError::world_kb_conflict(
                    current,
                    job_id,
                    "version",
                    "refetch the candidates list and reapply",
                )
            }
            SpokeRejectCode::InternalError => CoreError::Internal {
                category: format!("orchestrate_promote internal error: {}", reject.message),
            },
            _ => CoreError::Internal {
                category: format!(
                    "orchestrate_promote rejected: {}: {}",
                    reject.code, reject.message
                ),
            },
        }
    }

    /// Reject: CAS flip pending → rejected (with version guard).
    async fn promote_reject(
        pool: &sqlx::SqlitePool,
        candidate: &nexus_local_db::kb_extract_job::KbExtractPromotion,
        req: &WorldKbPromoteCandidateRequest,
    ) -> CoreResult<WorldKbPromoteCandidateResponse> {
        // SAFETY: runtime UPDATE with version guard — mirrors the V1.51 CAS
        // pattern (kept verbatim from the migrated daemon handler).
        let result = sqlx::query(
            "UPDATE kb_extract_jobs \
             SET promotion_status = 'rejected', version = version + 1 \
             WHERE job_id = ? AND promotion_status = 'pending' AND version = ?",
        )
        .bind(&req.job_id)
        .bind(i64::try_from(req.expected_version).unwrap_or(0))
        .execute(pool)
        .await
        .map_err(|e| db_err(&e))?;
        if result.rows_affected() != 1 {
            // CAS miss: re-read the actual current version so the client
            // retries against the NEW version.
            let current = reread_promotion_version(pool, &req.job_id).await?;
            return Err(RootCoreError::world_kb_conflict(
                current,
                &req.job_id,
                "version",
                "refetch the candidates list and reapply",
            ));
        }
        let job = get_promotion(pool, &req.job_id)
            .await
            .map_err(|e| db_err(&e))?
            .unwrap_or_else(|| candidate.clone());
        let new_version = u64::try_from(job.version).unwrap_or(0);
        Ok(WorldKbPromoteCandidateResponse {
            entity: None,
            job: wire_cast(project_job(&job)),
            version: new_version,
            validation_summary: wire_cast(validation_summary(&[], &[])),
        })
    }

    /// Merge: fold the candidate summary into an existing confirmed target,
    /// then dismiss the candidate. Atomic: CAS-update target `kb_key_blocks`
    /// body + CAS-reject candidate `kb_extract_jobs` in one transaction.
    //
    // simplify: merge folds the candidate summary into the target body and
    // rejects the candidate job. Full attribute-level merge with conflict
    // surfacing is deferred (V1.73 β decision, retained).
    async fn promote_merge(
        pool: &sqlx::SqlitePool,
        world_id: &str,
        candidate: &nexus_local_db::kb_extract_job::KbExtractPromotion,
        req: &WorldKbPromoteCandidateRequest,
    ) -> CoreResult<WorldKbPromoteCandidateResponse> {
        let target_id = req
            .merge_target_id
            .as_deref()
            .ok_or_else(|| CoreError::InvalidInput {
                field: "merge_target_id".to_string(),
                reason: "merge requires merge_target_id".to_string(),
            })?;
        let store = SqliteKbStore::with_validation_mode(pool.clone(), ValidationMode::Novel);
        let target = store
            .get_knowledge_entry(target_id)
            .await
            .map_err(|e| store_err(&e))?;
        if target.world_id() != Some(world_id) {
            return Err(CoreError::NotFound {
                resource: format!("merge target {target_id} in world {world_id}"),
            });
        }
        if target.status != "confirmed" && target.status != "manual" {
            return Err(CoreError::world_kb_validation_failed(
                &[format!(
                    "merge target must be confirmed or manual; got '{}'",
                    target.status
                )],
                &[],
            ));
        }

        // Fold the candidate summary into the target body summary.
        let target_body = target.body.clone().unwrap_or_default();
        let body_json_str = merge_candidate_summary(&target_body, candidate);
        let target_version = target.revision.unwrap_or(0);

        let mut tx = pool.begin().await.map_err(|e| db_err(&e))?;
        // Target CAS miss is tagged "merge_target" (not the candidate's
        // "version") so the client refreshes the target, not the candidate.
        // The world-aware predicate binds the request's world.
        let _new_target_version = nexus_local_db::kb_store::cas_update_key_block_fields(
            &mut tx,
            target_id,
            i64::try_from(target_version).unwrap_or(0),
            world_id,
            &nexus_local_db::kb_store::CasKeyBlockFieldUpdate {
                canonical_name: None,
                block_type: None,
                body_json: Some(&body_json_str),
                status: None,
                source_anchor_json: None,
                extensions_nexus_json: None,
                modules_json: None,
                source_provenance_kind: None,
            },
        )
        .await
        .map_err(|e| map_cas_err(e, target_id, "merge_target"))?;
        let reject = sqlx::query(
            "UPDATE kb_extract_jobs \
             SET promotion_status = 'rejected', version = version + 1 \
             WHERE job_id = ? AND promotion_status = 'pending' AND version = ?",
        )
        .bind(&req.job_id)
        .bind(i64::try_from(req.expected_version).unwrap_or(0))
        .execute(&mut *tx)
        .await
        .map_err(|e| db_err(&e))?;
        if reject.rows_affected() != 1 {
            // CAS miss: roll back the target fold and re-read the candidate's
            // actual current version.
            let _ = tx.rollback().await;
            let current = reread_promotion_version(pool, &req.job_id).await?;
            return Err(RootCoreError::world_kb_conflict(
                current,
                &req.job_id,
                "version",
                "refetch the candidates list and reapply",
            ));
        }
        tx.commit().await.map_err(|e| db_err(&e))?;

        let updated_target = store
            .get_knowledge_entry(target_id)
            .await
            .map_err(|e| store_err(&e))?;
        let job = get_promotion(pool, &req.job_id)
            .await
            .map_err(|e| db_err(&e))?
            .unwrap_or_else(|| candidate.clone());
        let new_version = u64::try_from(job.version).unwrap_or(0);

        Ok(WorldKbPromoteCandidateResponse {
            entity: Some(wire_cast(project_entity(&updated_target))),
            job: wire_cast(project_job(&job)),
            version: new_version,
            validation_summary: wire_cast(validation_summary(&[], &[])),
        })
    }

    /// Fold the candidate's proposed summary into the target body summary
    /// (promote-merge): append as a `— merged: …` paragraph, or seed it when
    /// the target has no summary. Returns the serialized body JSON string.
    fn merge_candidate_summary(
        target_body: &KnowledgeEntryBody,
        candidate: &nexus_local_db::kb_extract_job::KbExtractPromotion,
    ) -> String {
        let candidate_summary = candidate
            .proposed_payload
            .as_deref()
            .and_then(|p| serde_json::from_str::<KnowledgeEntryBody>(p).ok())
            .and_then(|b| b.summary);
        let mut merged = target_body.clone();
        if let Some(cs) = candidate_summary {
            let summary = merged.summary.as_ref().map_or_else(
                || format!("— merged: {cs}"),
                |existing| format!("{existing}\n\n— merged: {cs}"),
            );
            merged.summary = Some(summary);
        }
        serde_json::to_string(&serde_json::to_value(&merged).unwrap_or_default())
            .unwrap_or_default()
    }

    /// Build the extract-job projection after a promotion action.
    fn project_job(
        c: &nexus_local_db::kb_extract_job::KbExtractPromotion,
    ) -> WorldKbExtractJobProjection {
        WorldKbExtractJobProjection {
            job_id: c.job_id.clone(),
            world_id: c.world_id.clone(),
            status: c.promotion_status.clone(),
            version: u64::try_from(c.version).unwrap_or(0),
            candidate_ids: vec![],
            updated_at: c.auto_promoted_at.clone(),
        }
    }

    /// Re-read the actual current `kb_extract_jobs.version` after a
    /// promote-path CAS miss, normalized like the outer OCC precondition.
    async fn reread_promotion_version(pool: &sqlx::SqlitePool, job_id: &str) -> CoreResult<u64> {
        Ok(get_promotion(pool, job_id)
            .await
            .map_err(|e| local_db_err(nexus_local_db::LocalDbError::Sqlx(e)))?
            .map_or(0, |j| u64::try_from(j.version).unwrap_or(0)))
    }

    /// Map a `LocalDbError::VersionMismatch` to a 409-shaped conflict;
    /// world-mismatch CAS misses to the "world" conflict family.
    fn map_cas_err(
        e: nexus_local_db::LocalDbError,
        entity_id: &str,
        conflicting_path: &str,
    ) -> CoreError {
        match e {
            nexus_local_db::LocalDbError::VersionMismatch { actual, .. } => {
                RootCoreError::world_kb_conflict(
                    actual.unwrap_or(0).max(0).cast_unsigned(),
                    entity_id,
                    conflicting_path,
                    "refetch the World KB graph and reapply",
                )
            }
            nexus_local_db::LocalDbError::WorldConflict { .. } => RootCoreError::world_kb_conflict(
                0,
                entity_id,
                "world",
                "the target moved to another world; refetch it in its stored world and reapply",
            ),
            other => local_db_err(other),
        }
    }
    #[cfg(test)]
    mod internal_tests {
        use super::{
            promote_adopt_commit_ambiguity_action,
            resolve_promote_adopt_commit_ambiguity_after_reread, PromoteAdoptCommitAmbiguityAction,
            PromoteAdoptCommitAmbiguityResolution,
        };

        #[test]
        fn confirmed_job_treats_commit_error_as_success() {
            assert_eq!(
                promote_adopt_commit_ambiguity_action(Some("confirmed")),
                PromoteAdoptCommitAmbiguityAction::TreatAsSuccess
            );
            assert_eq!(
                resolve_promote_adopt_commit_ambiguity_after_reread(Ok(Some("confirmed"))),
                PromoteAdoptCommitAmbiguityResolution::TreatAsSuccess
            );
        }

        #[test]
        fn pending_job_fails_on_commit_error() {
            assert_eq!(
                promote_adopt_commit_ambiguity_action(Some("pending")),
                PromoteAdoptCommitAmbiguityAction::Fail
            );
            assert_eq!(
                resolve_promote_adopt_commit_ambiguity_after_reread(Ok(Some("pending"))),
                PromoteAdoptCommitAmbiguityResolution::Fail
            );
        }

        #[test]
        fn missing_job_fails_on_commit_error() {
            assert_eq!(
                promote_adopt_commit_ambiguity_action(None),
                PromoteAdoptCommitAmbiguityAction::Fail
            );
            assert_eq!(
                resolve_promote_adopt_commit_ambiguity_after_reread(Ok(None)),
                PromoteAdoptCommitAmbiguityResolution::Fail
            );
        }

        #[test]
        fn rejected_job_fails_on_commit_error() {
            assert_eq!(
                promote_adopt_commit_ambiguity_action(Some("rejected")),
                PromoteAdoptCommitAmbiguityAction::Fail
            );
            assert_eq!(
                resolve_promote_adopt_commit_ambiguity_after_reread(Ok(Some("rejected"))),
                PromoteAdoptCommitAmbiguityResolution::Fail
            );
        }

        #[test]
        fn reread_failed_fails() {
            assert_eq!(
                resolve_promote_adopt_commit_ambiguity_after_reread(Err(())),
                PromoteAdoptCommitAmbiguityResolution::Fail
            );
        }
    }
}

pub mod relationship {
    //! Typed relationship add/update/remove, ported from the daemon handler.
    //! Add/update route through `orchestrate_relate` (Surface B); remove keeps
    //! the CAS delete (spoke `RelationPort` has no delete).

    use nexus_contracts::world_kb_patch_relationship_request::{
        NexusWorldKbRelationshipInput, NexusWorldKbRelationshipKind,
    };
    use nexus_contracts::{WorldKbPatchRelationshipRequest, WorldKbPatchRelationshipResponse};
    use nexus_local_db::kb_relationships::{
        delete_relationship_in_tx, generate_relationship_id, get_relationship, SOURCE_MANUAL,
    };
    use nexus_local_db::LocalDbError;
    use nexus_spoke_adapter::{
        orchestrate_relate, NexusAdapter, RelateRequest, RelateResponse, Relation as SpokeRelation,
        RelationExtensionsKey, SpokeResult,
    };
    use std::collections::HashMap;
    use std::num::NonZeroU64;

    use super::{
        db_err, guards, local_db_err, validation_summary, wire_cast, CoreError, CoreResult,
    };

    pub async fn patch_relationship(
        pool: &sqlx::SqlitePool,
        creator_id: &str,
        world_id: &str,
        req: WorldKbPatchRelationshipRequest,
    ) -> CoreResult<WorldKbPatchRelationshipResponse> {
        guards::require_world_owner(pool, world_id, creator_id).await?;

        match req.action.as_str() {
            "add" => patch_relationship_add(pool, world_id, req.relationship).await,
            "update" => {
                patch_relationship_update(
                    pool,
                    world_id,
                    req.relationship_id.as_deref(),
                    req.expected_version,
                    req.relationship,
                )
                .await
            }
            "remove" => {
                patch_relationship_remove(
                    pool,
                    world_id,
                    req.relationship_id.as_deref(),
                    req.expected_version,
                )
                .await
            }
            other => Err(CoreError::InvalidInput {
                field: "action".to_string(),
                reason: format!("unknown action '{other}'; expected add, update, or remove"),
            }),
        }
    }

    async fn patch_relationship_add(
        pool: &sqlx::SqlitePool,
        world_id: &str,
        input: Option<NexusWorldKbRelationshipInput>,
    ) -> CoreResult<WorldKbPatchRelationshipResponse> {
        let input = input.ok_or_else(|| CoreError::InvalidInput {
            field: "relationship".to_string(),
            reason: "relationship payload is required for add".to_string(),
        })?;
        validate_relationship_input(&input)?;
        require_entities_in_world(
            pool,
            world_id,
            &input.source_entity_id,
            &input.target_entity_id,
        )
        .await?;
        require_valid_source_anchors(pool, world_id, Some(input.source_anchor_ids.as_slice()))
            .await?;

        // Create path: candidate `revision = None` (the P1 port seeds 1);
        // `source` is always manual for author creates.
        let relationship_id = generate_relationship_id();
        let relation = nexus_input_to_spoke_relation(
            &relationship_id,
            world_id,
            &input,
            None,
            Some(SOURCE_MANUAL),
        );
        let spoke_req = build_spoke_relate_request(&relation);
        let adapter = NexusAdapter::new(pool.clone());
        let result = adapter
            .with_bound_tx(|| orchestrate_relate(&adapter, spoke_req))
            .await;
        let row = map_relate_response(result, pool, &relationship_id).await?;

        Ok(WorldKbPatchRelationshipResponse {
            relationship: Some(wire_cast(super::graph::project_relationship(
                &row, "stored",
            ))),
            version: u64::try_from(row.revision).unwrap_or(0),
            validation_summary: wire_cast(validation_summary(&[], &[])),
        })
    }

    async fn patch_relationship_update(
        pool: &sqlx::SqlitePool,
        world_id: &str,
        relationship_id: Option<&str>,
        expected_version: Option<u64>,
        input: Option<NexusWorldKbRelationshipInput>,
    ) -> CoreResult<WorldKbPatchRelationshipResponse> {
        let relationship_id = relationship_id.ok_or_else(|| CoreError::InvalidInput {
            field: "relationship_id".to_string(),
            reason: "relationship_id is required for update".to_string(),
        })?;
        let expected_version = expected_version.ok_or_else(|| CoreError::InvalidInput {
            field: "expected_version".to_string(),
            reason: "expected_version is required for update".to_string(),
        })?;
        let input = input.ok_or_else(|| CoreError::InvalidInput {
            field: "relationship".to_string(),
            reason: "relationship payload is required for update".to_string(),
        })?;

        validate_relationship_input(&input)?;
        require_entities_in_world(
            pool,
            world_id,
            &input.source_entity_id,
            &input.target_entity_id,
        )
        .await?;
        require_valid_source_anchors(pool, world_id, Some(input.source_anchor_ids.as_slice()))
            .await?;

        // Scope check: the row must belong to this world. This MUST run
        // before the orchestrator (cross-world access is forbidden, not a
        // spoke reject).
        let existing = get_relationship(pool, relationship_id)
            .await
            .map_err(|e| match e {
                LocalDbError::Sqlx(sqlx::Error::RowNotFound) => CoreError::NotFound {
                    resource: format!("relationship {relationship_id}"),
                },
                other => local_db_err(other),
            })?;
        if existing.world_id != world_id {
            return Err(CoreError::Forbidden {
                resource: format!(
                    "relationship {relationship_id}: belongs to world {}; cross-world access is forbidden",
                    existing.world_id
                ),
            });
        }

        // OCC precondition (client view vs current row): a stale
        // `expected_version` must surface as a conflict here.
        let current_revision = u64::try_from(existing.revision).unwrap_or(0);
        if expected_version != current_revision {
            return Err(CoreError::world_kb_conflict(
                current_revision,
                relationship_id,
                "version",
                "refetch the World KB graph and reapply",
            ));
        }

        // Update path: candidate `revision = Some(current_revision)` (the
        // orchestrator asserts it, then the port CAS-bumps). `source` omitted:
        // the port preserves the stored provenance.
        //
        // `needs_review` preservation: when the input omits it, default to the
        // existing flag so a routine edit does not silently confirm an
        // extraction suggestion.
        let mut input = input;
        let needs_review_for_relation = input.needs_review.unwrap_or(existing.needs_review != 0);
        input.needs_review = Some(needs_review_for_relation);
        let mut relation = nexus_input_to_spoke_relation(
            relationship_id,
            world_id,
            &input,
            Some(current_revision),
            None,
        );

        // Preserve unknown `extensions.nexus` keys from the existing row so a
        // routine update does not silently drop them.
        let known_nexus_keys: &[&str] = &[
            "world_id",
            "symmetric",
            "confidence",
            "source_anchor_ids",
            "needs_review",
            "source",
        ];
        if let Some(json) = &existing.extensions_nexus_json {
            if let Ok(stored_ns) =
                serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(json)
            {
                let key = RelationExtensionsKey::try_from("nexus")
                    .expect("\"nexus\" matches the extensions-key regex");
                if let Some(nexus_ns) = relation.extensions.get_mut(&key) {
                    for (k, v) in stored_ns {
                        if !known_nexus_keys.contains(&k.as_str()) {
                            nexus_ns.entry(k).or_insert(v);
                        }
                    }
                }
            }
        }

        let spoke_req = build_spoke_relate_request(&relation);
        let adapter = NexusAdapter::new(pool.clone());
        let result = adapter
            .with_bound_tx(|| orchestrate_relate(&adapter, spoke_req))
            .await;
        let row = map_relate_response(result, pool, relationship_id).await?;

        Ok(WorldKbPatchRelationshipResponse {
            relationship: Some(wire_cast(super::graph::project_relationship(
                &row, "stored",
            ))),
            version: u64::try_from(row.revision).unwrap_or(0),
            validation_summary: wire_cast(validation_summary(&[], &[])),
        })
    }

    async fn patch_relationship_remove(
        pool: &sqlx::SqlitePool,
        world_id: &str,
        relationship_id: Option<&str>,
        expected_version: Option<u64>,
    ) -> CoreResult<WorldKbPatchRelationshipResponse> {
        let relationship_id = relationship_id.ok_or_else(|| CoreError::InvalidInput {
            field: "relationship_id".to_string(),
            reason: "relationship_id is required for remove".to_string(),
        })?;
        let expected_version = expected_version.ok_or_else(|| CoreError::InvalidInput {
            field: "expected_version".to_string(),
            reason: "expected_version is required for remove".to_string(),
        })?;

        // Scope check: the row must belong to this world.
        let existing = get_relationship(pool, relationship_id)
            .await
            .map_err(|e| match e {
                LocalDbError::Sqlx(sqlx::Error::RowNotFound) => CoreError::NotFound {
                    resource: format!("relationship {relationship_id}"),
                },
                other => local_db_err(other),
            })?;
        if existing.world_id != world_id {
            return Err(CoreError::Forbidden {
                resource: format!(
                    "relationship {relationship_id}: belongs to world {}; cross-world access is forbidden",
                    existing.world_id
                ),
            });
        }

        let mut tx = pool.begin().await.map_err(|e| db_err(&e))?;
        delete_relationship_in_tx(
            &mut tx,
            relationship_id,
            i64::try_from(expected_version).unwrap_or(0),
        )
        .await
        .map_err(|e| map_relationship_cas_err(e, relationship_id))?;
        tx.commit().await.map_err(|e| db_err(&e))?;

        Ok(WorldKbPatchRelationshipResponse {
            relationship: None,
            version: expected_version,
            validation_summary: wire_cast(validation_summary(&[], &[])),
        })
    }

    /// Domain validation for a relationship payload.
    fn validate_relationship_input(input: &NexusWorldKbRelationshipInput) -> CoreResult<()> {
        if input.source_entity_id == input.target_entity_id {
            return Err(CoreError::world_kb_validation_failed(
                &["source_entity_id and target_entity_id must be different".to_string()],
                &[],
            ));
        }
        if input.relation_type == NexusWorldKbRelationshipKind::Custom
            && input.custom_label.is_none()
        {
            return Err(CoreError::world_kb_validation_failed(
                &["custom relation_type requires custom_label".to_string()],
                &[],
            ));
        }
        if let Some(confidence) = input.confidence {
            if !(0.0..=1.0).contains(&confidence) {
                return Err(CoreError::world_kb_validation_failed(
                    &["confidence must be between 0.0 and 1.0".to_string()],
                    &[],
                ));
            }
        }
        Ok(())
    }

    /// Verify both endpoint entities exist in the world and are not deleted.
    async fn require_entities_in_world(
        pool: &sqlx::SqlitePool,
        world_id: &str,
        source_id: &str,
        target_id: &str,
    ) -> CoreResult<()> {
        // SAFETY: runtime query with static column names and bind params.
        // Relationship endpoints must be World-owned (owner_kind='world') AND
        // live in the same stored world — fail-closed.
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT key_block_id, status FROM kb_key_blocks \
             WHERE owner_kind = 'world' AND world_id = ? AND key_block_id IN (?, ?)",
        )
        .bind(world_id)
        .bind(source_id)
        .bind(target_id)
        .fetch_all(pool)
        .await
        .map_err(|e| db_err(&e))?;

        let mut missing = Vec::new();
        let mut deleted = Vec::new();
        for id in [source_id, target_id] {
            match rows.iter().find(|(k, _)| k == id) {
                None => missing.push(id.to_string()),
                Some((_, status)) if status == "deleted" => deleted.push(id.to_string()),
                Some(_) => {}
            }
        }

        if !missing.is_empty() {
            return Err(CoreError::world_kb_validation_failed(
                &[format!(
                    "entities not found in world {world_id}: {}",
                    missing.join(", ")
                )],
                &[],
            ));
        }
        if !deleted.is_empty() {
            return Err(CoreError::world_kb_validation_failed(
                &[format!(
                    "cannot relate deleted entities: {}",
                    deleted.join(", ")
                )],
                &[],
            ));
        }
        Ok(())
    }

    /// Row shape for source-anchor validation lookups.
    #[derive(sqlx::FromRow)]
    struct AnchorValidationRow {
        key_block_id: String,
        source_work_id: Option<String>,
    }

    /// Verify every source-anchor projection id references a
    /// `KnowledgeEntryRecord` in the world that actually has provenance
    /// (`source_work_id IS NOT NULL`). Anchor ids use the graph projection
    /// format `sa_<key_block_id>`.
    async fn require_valid_source_anchors(
        pool: &sqlx::SqlitePool,
        world_id: &str,
        anchor_ids: Option<&[String]>,
    ) -> CoreResult<()> {
        let ids = anchor_ids.unwrap_or_default();
        if ids.is_empty() {
            return Ok(());
        }

        let mut key_block_ids = Vec::with_capacity(ids.len());
        for id in ids {
            let Some(kb_id) = id.strip_prefix("sa_") else {
                return Err(CoreError::world_kb_validation_failed(
                    &[format!(
                        "source_anchor_id '{id}' is not a valid anchor projection id"
                    )],
                    &[],
                ));
            };
            key_block_ids.push(kb_id.to_string());
        }

        // SAFETY: runtime query with a dynamic JSON array binding. The SQL is
        // otherwise static; compile-time macros cannot bind a variable-length
        // list.
        let rows: Vec<AnchorValidationRow> = sqlx::query_as(
            "SELECT key_block_id, source_work_id FROM kb_key_blocks \
             WHERE world_id = ? AND key_block_id IN (SELECT value FROM json_each(?))",
        )
        .bind(world_id)
        .bind(serde_json::to_string(&key_block_ids).unwrap_or_default())
        .fetch_all(pool)
        .await
        .map_err(|e| db_err(&e))?;

        let mut errors = Vec::new();
        for id in ids {
            let kb_id = id.strip_prefix("sa_").unwrap_or(id);
            match rows.iter().find(|r| r.key_block_id == kb_id) {
                None => errors.push(format!(
                    "source anchor '{id}' does not reference an entity in this world"
                )),
                Some(row) if row.source_work_id.is_none() => errors.push(format!(
                    "source anchor '{id}' references an entity without provenance"
                )),
                Some(_) => {}
            }
        }

        if !errors.is_empty() {
            return Err(CoreError::world_kb_validation_failed(&errors, &[]));
        }
        Ok(())
    }

    /// Build a spoke [`SpokeRelation`] from the nexus patch-relationship
    /// input (the sole nexus→spoke conversion seam for relations).
    fn nexus_input_to_spoke_relation(
        relationship_id: &str,
        world_id: &str,
        input: &NexusWorldKbRelationshipInput,
        revision: Option<u64>,
        source: Option<&str>,
    ) -> SpokeRelation {
        // extensions.nexus carries the nexus-locals the RelationPort reads
        // back via `extract_nexus_locals`.
        let mut nexus_ns = serde_json::Map::new();
        nexus_ns.insert(
            "world_id".to_string(),
            serde_json::Value::String(world_id.to_string()),
        );
        nexus_ns.insert(
            "symmetric".to_string(),
            serde_json::Value::Bool(input.symmetric),
        );
        if let Some(confidence) = input.confidence {
            if let Some(num) = serde_json::Number::from_f64(confidence) {
                nexus_ns.insert("confidence".to_string(), serde_json::Value::Number(num));
            }
        }
        nexus_ns.insert(
            "source_anchor_ids".to_string(),
            serde_json::Value::Array(
                input
                    .source_anchor_ids
                    .iter()
                    .cloned()
                    .map(serde_json::Value::String)
                    .collect(),
            ),
        );
        // V1.76 extraction-suggestion gate: the caller resolves the tri-state
        // (`patch_relationship_update` defaults an omitted flag to the stored
        // one), and the relation must carry the resolved value — the store
        // reads it back out of `extensions.nexus`. Dropping it here persisted
        // every update as `needs_review = 0`, silently promoting a suggestion
        // the caller never confirmed.
        nexus_ns.insert(
            "needs_review".to_string(),
            serde_json::Value::Bool(input.needs_review.unwrap_or(false)),
        );
        if let Some(src) = source {
            nexus_ns.insert(
                "source".to_string(),
                serde_json::Value::String(src.to_string()),
            );
        }

        let key = RelationExtensionsKey::try_from("nexus")
            .expect("\"nexus\" matches the extensions-key regex");
        let mut extensions = HashMap::new();
        extensions.insert(key, nexus_ns);

        SpokeRelation {
            schema_version: NonZeroU64::new(1).expect("1 is non-zero"),
            relation_id: relationship_id.to_string(),
            from_id: input.source_entity_id.clone(),
            to_id: input.target_entity_id.clone(),
            relation_type: input.relation_type.as_str().to_string(),
            label: input.custom_label.as_ref().map(|c| c.to_string()),
            metadata: input.metadata.clone(),
            revision,
            created_at: None,
            updated_at: None,
            extensions,
        }
    }

    /// Wrap a spoke [`SpokeRelation`] into a [`RelateRequest`] via JSON
    /// round-trip (the codegen emits a distinct struct per wire shape).
    fn build_spoke_relate_request(relation: &SpokeRelation) -> RelateRequest {
        let wire = serde_json::to_value(relation).unwrap_or_else(|_| serde_json::json!({}));
        serde_json::from_value(serde_json::json!({ "relation": wire }))
            .expect("Relation-derived relation fits RelateRequest.relation shape")
    }

    /// Map `orchestrate_relate`'s result to the persisted
    /// [`nexus_local_db::kb_relationships::KbRelationshipRow`] on success, or
    /// to a [`CoreError`] on reject. On success the canonical row is re-read
    /// for the response projection.
    async fn map_relate_response(
        result: SpokeResult<RelateResponse>,
        pool: &sqlx::SqlitePool,
        relationship_id: &str,
    ) -> CoreResult<nexus_local_db::kb_relationships::KbRelationshipRow> {
        match result {
            SpokeResult::Ok(RelateResponse::Variant0 { .. }) => {
                // Re-read the persisted row (P1 commits before returning).
                get_relationship(pool, relationship_id)
                    .await
                    .map_err(|e| match e {
                        LocalDbError::Sqlx(sqlx::Error::RowNotFound) => {
                            CoreError::Internal {
                                category: format!(
                                    "orchestrate_relate returned success but the row for {relationship_id} is absent"
                                ),
                            }
                        }
                        other => local_db_err(other),
                    })
            }
            // Defensive: the orchestrator always surfaces errors via
            // `SpokeResult::Reject` (not Variant1).
            SpokeResult::Ok(RelateResponse::Variant1 { .. }) => Err(CoreError::Internal {
                category: format!(
                    "orchestrate_relate returned a RelateResponse::Variant1 error envelope for {relationship_id}; \
                     expected a SpokeResult::Reject (spoke wire-shape drift)"
                ),
            }),
            SpokeResult::Reject(reject) => Err(map_relate_reject(reject, pool, relationship_id).await),
        }
    }

    /// Map the spoke reject codes reachable on the relate add/update path to
    /// [`CoreError`]. The conflict `current_version` prefers the revision the
    /// reject carries, falling back to a row re-read.
    async fn map_relate_reject(
        reject: nexus_spoke_adapter::SpokeReject,
        pool: &sqlx::SqlitePool,
        relationship_id: &str,
    ) -> CoreError {
        use nexus_spoke_adapter::{is_world_conflict_reject, SpokeRejectCode};

        // World-mismatch CAS miss → 409 conflict family kind "world".
        if is_world_conflict_reject(&reject) {
            let current = match super::patch::extract_store_revision(&reject) {
                Some(rev) => rev,
                None => reread_relation_revision_sync(pool, relationship_id).await,
            };
            return CoreError::world_kb_conflict(
                current,
                relationship_id,
                "world",
                "the relationship moved to another world; refetch it in its stored world and reapply",
            );
        }
        match reject.code {
            SpokeRejectCode::RelationAlreadyExists
            | SpokeRejectCode::StoredRevisionStale
            | SpokeRejectCode::RevisionConflict => {
                let current = match super::patch::extract_store_revision(&reject) {
                    Some(rev) => rev,
                    None => reread_relation_revision_sync(pool, relationship_id).await,
                };
                CoreError::world_kb_conflict(
                    current,
                    relationship_id,
                    "version",
                    "refetch the World KB graph and reapply",
                )
            }
            // Known misclassification (low residual): the production port
            // raises `InvalidInput` for BOTH validation and storage failures.
            // On this path every validation `InvalidInput` is pre-checked, so
            // only storage failures reach here; mapped to InvalidInput as
            // shipped (tracked for a spoke-level fix in a later iteration).
            SpokeRejectCode::InvalidInput => CoreError::InvalidInput {
                field: "relationship".to_string(),
                reason: reject.message,
            },
            // Defensive: pre-checked by `validate_relationship_input` /
            // endpoint existence; mapped to validation (not internal) keeps
            // the contract honest if a future caller bypasses the guard.
            SpokeRejectCode::RelationSelfEdge
            | SpokeRejectCode::RelationMissingEndpoint
            | SpokeRejectCode::MissingRequiredField => {
                CoreError::world_kb_validation_failed(&[reject.message], &[])
            }
            SpokeRejectCode::RelationNotFound => CoreError::NotFound {
                resource: format!("relationship {relationship_id}"),
            },
            SpokeRejectCode::InternalError => CoreError::Internal {
                category: format!("orchestrate_relate internal error: {}", reject.message),
            },
            _ => CoreError::Internal {
                category: format!(
                    "orchestrate_relate rejected: {}: {}",
                    reject.code, reject.message
                ),
            },
        }
    }

    /// Re-read the current `kb_relationships.revision` (fallback when a CAS
    /// reject's details omit the store revision).
    async fn reread_relation_revision_sync(pool: &sqlx::SqlitePool, relationship_id: &str) -> u64 {
        get_relationship(pool, relationship_id)
            .await
            .ok()
            .and_then(|r| u64::try_from(r.revision).ok())
            .unwrap_or(0)
    }

    /// Map a relationship CAS miss to a 409-shaped conflict; `RowNotFound` to
    /// not-found; other DB errors to the storage mapping.
    fn map_relationship_cas_err(e: LocalDbError, relationship_id: &str) -> CoreError {
        match e {
            LocalDbError::VersionMismatch { actual, .. } => CoreError::world_kb_conflict(
                actual.unwrap_or(0).max(0).cast_unsigned(),
                relationship_id,
                "version",
                "refetch the World KB graph and reapply",
            ),
            LocalDbError::Sqlx(sqlx::Error::RowNotFound) => CoreError::NotFound {
                resource: format!("relationship {relationship_id}"),
            },
            other => local_db_err(other),
        }
    }
}

pub mod key_block_state {
    //! Computable-entity `body.state` read (V1.114 P2 surface).

    use nexus_contracts::WorldKbKeyBlockStateResponse;
    use nexus_knowledge::world_kb::KbStore;
    use nexus_local_db::kb_store::SqliteKbStore;

    use super::{guards, store_err, CoreError, CoreResult, KbStoreError};

    pub async fn get_key_block_state(
        pool: &sqlx::SqlitePool,
        creator_id: &str,
        world_id: &str,
        key_block_id: &str,
    ) -> CoreResult<WorldKbKeyBlockStateResponse> {
        guards::require_world_owner(pool, world_id, creator_id).await?;

        let store = SqliteKbStore::new(pool.clone());
        let kb = store
            .get_knowledge_entry(key_block_id)
            .await
            .map_err(|e| match e {
                KbStoreError::NotFound(_) => CoreError::NotFound {
                    resource: format!("key block {key_block_id} in world {world_id}"),
                },
                other => store_err(&other),
            })?;

        // Scope check: the row must live in the path world. Treat a row
        // belonging to a different world as not found (same as patch_entity).
        if kb.world_id() != Some(world_id) {
            return Err(CoreError::NotFound {
                resource: format!("key block {key_block_id} in world {world_id}"),
            });
        }

        let is_computable = kb.body.as_ref().and_then(|b| b.computable).unwrap_or(false);
        let state = if is_computable {
            kb.body
                .as_ref()
                .and_then(|b| b.state.clone())
                .unwrap_or(serde_json::Value::Null)
        } else {
            serde_json::Value::Null
        };

        Ok(WorldKbKeyBlockStateResponse {
            state: state.as_object().cloned(),
            is_computable,
            version: kb.revision.unwrap_or(0),
        })
    }
}
