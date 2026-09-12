//! World KB graph/patch/candidates implementation (ported from daemon handlers).

use std::collections::HashMap;
use nexus_contracts::{
    world_kb_patch_entity_request::{
        NexusWorldKbEntityPatch, NexusWorldKbEntityPatchModulesKey,
        NexusWorldKbEntityPatchModulesValue,
    },
    BlockType, PaginationInfo, WorldKbCandidateProjection, WorldKbCandidatesResponse,
    WorldKbEntityProjection, WorldKbGraphResponse, WorldKbPatchEntityRequest,
    WorldKbPatchEntityResponse, WorldKbRelationshipProjection, WorldKbSourceAnchorProjection,
};
use nexus_knowledge::world_kb::knowledge_entry::{KnowledgeEntryBody, KnowledgeEntryRecord};
use nexus_knowledge::world_kb::validation::{
    validate_body, validate_canonical_name, ValidationMode,
};
use nexus_knowledge::world_kb::store::KbStoreError;
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
use tracing::warn;

use crate::error::{CoreError, CoreResult};

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

fn project_candidate(c: &nexus_local_db::kb_extract_job::KbExtractPromotion) -> WorldKbCandidateProjection {
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

mod guards {
    use super::*;

    pub(super) async fn require_world_owner(
        executor: impl sqlx::Executor<'_, Database = Sqlite>,
        world_id: &str,
        creator_id: &str,
    ) -> CoreResult<()> {
        let owner: Option<Option<String>> =
            sqlx::query_scalar("SELECT owner_creator_id FROM narrative_worlds WHERE world_id = ?")
                .bind(world_id)
                .fetch_optional(executor)
                .await
                .map_err(db_err)?;
        match owner {
            None => Err(CoreError::NotFound { resource: format!("world {world_id}") }),
            Some(Some(owner_id)) if owner_id == creator_id => Ok(()),
            Some(Some(_)) => Err(CoreError::Forbidden {
                resource: format!("world {world_id}: active creator does not own this world; cross-author World KB edits are forbidden"),
            }),
            Some(None) => Err(CoreError::Forbidden {
                resource: format!("world {world_id}: world has no owner_creator_id; cannot authorize World KB edit"),
            }),
        }
    }
}

pub(crate) mod graph {
    use super::*;

    pub(crate) async fn get_graph(
        pool: &SqlitePool,
        creator_id: &str,
        world_id: &str,
        include_suggested: bool,
    ) -> CoreResult<WorldKbGraphResponse> {
        let mut tx = nexus_local_db::begin_immediate(pool).await.map_err(local_db_err)?;
        guards::require_world_owner(&mut *tx, world_id, creator_id).await?;
        let _snapshot_watermark: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(sequence), 0) FROM core_changes",
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(db_err)?;

        let blocks = list_by_world_in_tx(&mut tx, world_id, GRAPH_ENTITY_CAP)
            .await
            .map_err(store_err)?;
        let fetch_limit = i64::try_from(GRAPH_RELATIONSHIP_CAP + 1).unwrap_or(i64::MAX);
        let rows = list_relationships_for_world_in_tx(
            &mut tx,
            world_id,
            include_suggested,
            fetch_limit,
        )
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
        tx.commit().await.map_err(db_err)?;
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
            projections.push(project_relationship(&row, "stored"));
            if row.symmetric != 0 {
                let mut reverse = row.clone();
                std::mem::swap(&mut reverse.source_entity_id, &mut reverse.target_entity_id);
                projections.push(project_relationship(&reverse, "symmetric_reverse"));
            }
        }
        projections
    }

    fn project_relationship(
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

        let relation_type: WorldKbRelationshipKind = row.relation_type.parse().unwrap_or_else(|_| {
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

pub(crate) mod candidates {
    use super::*;

    pub(crate) async fn get_candidates(
        pool: &SqlitePool,
        creator_id: &str,
        world_id: &str,
        limit: Option<i64>,
        cursor: Option<String>,
    ) -> CoreResult<WorldKbCandidatesResponse> {
        guards::require_world_owner(pool, world_id, creator_id).await?;

        let limit = limit
            .unwrap_or(DEFAULT_CANDIDATE_LIMIT)
            .clamp(1, MAX_CANDIDATE_LIMIT);
        let limit_us = usize::try_from(limit).unwrap_or(usize::MAX);
        let (cursor_created_at, cursor_job_id) = decode_candidate_cursor(cursor.as_ref())?;

        let pending = list_pending_for_world_after(
            pool,
            world_id,
            cursor_created_at.as_deref(),
            cursor_job_id.as_deref(),
            limit + 1,
        )
        .await
        .map_err(db_err)?;

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
        let stripped = raw.strip_prefix(CANDIDATE_CURSOR_PREFIX).ok_or_else(|| {
            CoreError::InvalidInput {
                field: "cursor".to_string(),
                reason: "invalid candidates cursor; pass next_cursor unchanged".to_string(),
            }
        })?;
        let mut parts = stripped.splitn(2, '|');
        let created_at = parts
            .next()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| CoreError::InvalidInput {
                field: "cursor".to_string(),
                reason: "invalid candidates cursor: missing created_at".to_string(),
            })?;
        let job_id = parts
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

pub(crate) mod patch {
    use super::*;

    pub(crate) async fn patch_entity(
        pool: &SqlitePool,
        creator_id: &str,
        world_id: &str,
        req: WorldKbPatchEntityRequest,
    ) -> CoreResult<WorldKbPatchEntityResponse> {
        let mut tx = nexus_local_db::begin_immediate(pool).await.map_err(local_db_err)?;
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
                reason: "at least one of title/body/aliases/block_type/modules must be provided"
                    .to_string(),
            });
        }

        let new_name = req.patch.title.as_ref().map(|t| t.to_string());
        let new_block_type = req.patch.block_type;
        let body_for_validation = compute_body(&kb, &req.patch)?;

        if let Some(ref name) = new_name {
            validate_canonical_name(name)
                .map_err(|e| CoreError::world_kb_validation_failed(&[e.to_string()], &[]))?;
        }
        let validation_block_type = new_block_type.map_or(kb.block_type, |bt| wire_cast(bt));
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
        let new_version = persisted.revision.unwrap_or(0);
        let response = WorldKbPatchEntityResponse {
            entity: wire_cast(project_entity(&persisted)),
            version: new_version,
            validation_summary: wire_cast(validation_summary(&[], &[])),
        };
        tx.commit().await.map_err(db_err)?;
        Ok(response)
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

        let entity_id_conforms = req
            .entity_id
            .strip_prefix("kb_")
            .is_some_and(|suffix| !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_hexdigit()));
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

        let spoke_entry = build_spoke_entry(&fresh);
        let put_result = put_knowledge_entry_in_tx(pool, &mut tx, spoke_entry, None).await;
        let persisted = match map_put_response(put_result, &req.entity_id, &mut tx).await {
            Ok(entry) => entry,
            Err(e) => {
                return Err(e);
            }
        };
        let new_version = persisted.revision.unwrap_or(0);
        let response = WorldKbPatchEntityResponse {
            entity: wire_cast(project_entity(&persisted)),
            version: new_version,
            validation_summary: wire_cast(validation_summary(&[], &[])),
        };
        tx.commit().await.map_err(db_err)?;
        Ok(response)
    }

    async fn fetch_entry_in_tx(
        tx: &mut sqlx::Transaction<'_, Sqlite>,
        entity_id: &str,
    ) -> CoreResult<Option<KnowledgeEntryRecord>> {
        match get_knowledge_entry_in_tx(tx, entity_id).await {
            Ok(row) => Ok(Some(row)),
            Err(KbStoreError::NotFound(_)) => Ok(None),
            Err(e) => Err(store_err(e)),
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

    fn extract_store_revision(reject: &SpokeReject) -> Option<u64> {
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
                    reject.code,
                    reject.message
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
        patch.title.is_none()
            && patch.body.is_empty()
            && patch.aliases.is_empty()
            && patch.block_type.is_none()
            && patch.modules.is_empty()
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
            let obj = value.as_object_mut().ok_or_else(|| CoreError::InvalidInput {
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


fn store_err(e: KbStoreError) -> CoreError {
    CoreError::Internal {
        category: format!("kb_store: {e}"),
    }
}

fn is_sqlite_busy(err: &sqlx::Error) -> bool {
    match err {
        sqlx::Error::Database(db) => {
            db.code().as_deref() == Some("5")
                || db.message().contains("database is locked")
                || db.message().contains("SQLITE_BUSY")
        }
        _ => false,
    }
}

fn db_err(e: sqlx::Error) -> CoreError {
    if is_sqlite_busy(&e) {
        CoreError::Busy
    } else {
        CoreError::Internal {
            category: format!("database_error: {e}"),
        }
    }
}

fn local_db_err(e: nexus_local_db::LocalDbError) -> CoreError {
    match e {
        nexus_local_db::LocalDbError::Sqlx(err) if is_sqlite_busy(&err) => CoreError::Busy,
        other => CoreError::Internal {
            category: format!("database_error: {other}"),
        },
    }
}
