//! Shared Knowledge Pack import orchestration (V1.152 P0, DF-77).
//!
//! Extracted from the V1.146 CLI `pack.rs::import` core so CLI and daemon
//! routes share one conflict-policy path. Caller MUST have already passed the
//! owner gate — this module does not re-check ownership (matches
//! [`crate::directive_store::LocalDirectiveStore`] precedent).

use nexus_contracts::BlockType;
use nexus_knowledge::world_kb::KbStore;
use nexus_knowledge::world_kb::WorldKbEntry;
use nexus_local_db::kb_relationships::{generate_relationship_id, get_relationship};
use nexus_local_db::kb_store::SqliteKbStore;
use nexus_spoke_adapter::pack::ParsedPack;
use nexus_spoke_adapter::{
    extensions, orchestrate_relate, orchestrate_upsert, KnowledgeEntry, NexusAdapter, RelateRequest,
    Relation, RelationExtensionsKey, UpsertRequest,
};
use sqlx::SqlitePool;
use std::collections::{HashMap, HashSet};
use thiserror::Error;

/// Provenance stamp on imported Knowledge entries and relations.
pub const IMPORT_PROVENANCE: &str = "pack_import";

/// Conflict policy shared by CLI + daemon (replaces CLI-only `ConflictStrategy`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictPolicy {
    /// Skip colliding entries/relations.
    Skip,
    /// Disambiguate colliding canonical names and mint fresh ids.
    Rename,
    /// Replace colliding entry/relation bodies via orchestrator CAS upsert.
    Overwrite,
}

/// Per-atom outcome counters for import summary reporting.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AtomCounts {
    pub created: u32,
    pub skipped: u32,
    pub rejected: u32,
    pub renamed: u32,
    pub overwritten: u32,
}

/// Whether an import detail row refers to an entry or a relation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportAtomKind {
    Entry,
    Relation,
}

/// Outcome of importing one pack atom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportOutcome {
    Created,
    Skipped,
    Rejected,
    Renamed,
    Overwritten,
}

/// One row in the structured import report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportDetail {
    pub kind: ImportAtomKind,
    pub id: String,
    pub outcome: ImportOutcome,
    pub reason: Option<String>,
}

/// Structured import result returned to CLI and daemon callers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportSummary {
    pub entries: AtomCounts,
    pub relations: AtomCounts,
    pub details: Vec<ImportDetail>,
}

/// Fatal errors from [`import_pack`] (storage / parse failures).
#[derive(Debug, Error)]
pub enum PackImportError {
    #[error("pack import storage error: {0}")]
    Storage(String),
}

/// Import a parsed pack into a world under a conflict policy.
///
/// Caller (CLI or daemon handler) MUST have already passed the owner gate —
/// this function does NOT re-check ownership.
///
/// # Errors
///
/// Returns [`PackImportError`] on storage failures. Individual atom rejections
/// are recorded in [`ImportSummary`] (`rejected` counts + `details`).
pub async fn import_pack(
    pool: &SqlitePool,
    world_id: &str,
    _creator_id: &str,
    pack: ParsedPack,
    conflict: ConflictPolicy,
    include_anchors: bool,
    dry_run: bool,
) -> Result<ImportSummary, PackImportError> {
    let store = SqliteKbStore::new(pool.clone());
    let mut summary = ImportSummary {
        entries: AtomCounts::default(),
        relations: AtomCounts::default(),
        details: Vec::new(),
    };

    let mut target_entry_ids: HashSet<String> = HashSet::new();
    let mut remap: HashMap<String, String> = HashMap::new();

    let existing = store.list_by_world(world_id).await.map_err(|e| {
        PackImportError::Storage(format!(
            "Failed to list existing entries for {world_id}: {e}"
        ))
    })?;
    for e in &existing {
        target_entry_ids.insert(e.entry_id.clone());
    }

    // Pack-level source anchors: only persisted when requested. Nexus has no
    // standalone SourceAnchor store yet — skip silently (export emits empty).
    let _anchors = if include_anchors {
        pack.source_anchors.as_deref()
    } else {
        None
    };

    for mut entry in pack.entries {
        let pack_entry_id = entry.entry_id.clone();
        let Some(entry_type) = parse_entry_type(&entry.entry_type) else {
            let reason = format!("unknown entry_type '{}'", entry.entry_type);
            record_entry(
                &mut summary,
                &pack_entry_id,
                ImportOutcome::Rejected,
                Some(reason),
            );
            summary.entries.rejected += 1;
            continue;
        };

        // ── Entry ID collision (global PK, world-scoped semantics) ─────
        if let Ok(existing_by_id) = store.get_knowledge_entry(&entry.entry_id).await {
            if existing_by_id.world_id == world_id {
                handle_entry_id_collision_in_target(
                    &mut summary,
                    &mut target_entry_ids,
                    &pack_entry_id,
                    dry_run,
                );
                continue;
            }

            // Foreign-world PK: remap via canonical-name match when possible.
            if let Ok(Some(existing_name_match)) = store
                .get_active_by_unique_key(world_id, &entry.canonical_name, entry_type)
                .await
            {
                match conflict {
                    ConflictPolicy::Skip => {
                        remap.insert(pack_entry_id.clone(), existing_name_match.entry_id.clone());
                        target_entry_ids.insert(existing_name_match.entry_id.clone());
                        record_entry(
                            &mut summary,
                            &pack_entry_id,
                            ImportOutcome::Skipped,
                            Some(format!(
                                "foreign entry_id; remapped to {}",
                                existing_name_match.entry_id
                            )),
                        );
                        summary.entries.skipped += 1;
                    }
                    ConflictPolicy::Rename => {
                        import_renamed_entry(
                            pool,
                            world_id,
                            &mut entry,
                            entry_type,
                            &pack_entry_id,
                            &mut summary,
                            &mut target_entry_ids,
                            &mut remap,
                            dry_run,
                        )
                        .await?;
                    }
                    ConflictPolicy::Overwrite => {
                        import_overwritten_entry(
                            pool,
                            world_id,
                            &mut entry,
                            &existing_name_match,
                            &pack_entry_id,
                            &mut summary,
                            &mut target_entry_ids,
                            &mut remap,
                            dry_run,
                        )
                        .await?;
                    }
                }
                continue;
            }

            record_entry(
                &mut summary,
                &pack_entry_id,
                ImportOutcome::Skipped,
                Some(format!(
                    "entry_id owned by world {} (no target-world name match)",
                    existing_by_id.world_id
                )),
            );
            summary.entries.skipped += 1;
            continue;
        }

        // ── Canonical-name collision ─────────────────────────────────────
        if let Ok(Some(existing_name)) = store
            .get_active_by_unique_key(world_id, &entry.canonical_name, entry_type)
            .await
        {
            match conflict {
                ConflictPolicy::Skip => {
                    remap.insert(pack_entry_id.clone(), existing_name.entry_id.clone());
                    target_entry_ids.insert(existing_name.entry_id.clone());
                    record_entry(
                        &mut summary,
                        &pack_entry_id,
                        ImportOutcome::Skipped,
                        Some(format!(
                            "canonical-name collision with {}",
                            existing_name.entry_id
                        )),
                    );
                    summary.entries.skipped += 1;
                }
                ConflictPolicy::Rename => {
                    import_renamed_entry(
                        pool,
                        world_id,
                        &mut entry,
                        entry_type,
                        &pack_entry_id,
                        &mut summary,
                        &mut target_entry_ids,
                        &mut remap,
                        dry_run,
                    )
                    .await?;
                }
                ConflictPolicy::Overwrite => {
                    import_overwritten_entry(
                        pool,
                        world_id,
                        &mut entry,
                        &existing_name,
                        &pack_entry_id,
                        &mut summary,
                        &mut target_entry_ids,
                        &mut remap,
                        dry_run,
                    )
                    .await?;
                }
            }
            continue;
        }

        // ── No collision → create ──────────────────────────────────────
        if dry_run {
            record_entry(
                &mut summary,
                &pack_entry_id,
                ImportOutcome::Created,
                Some("dry-run: would create".to_string()),
            );
            summary.entries.created += 1;
            target_entry_ids.insert(entry.entry_id.clone());
            continue;
        }

        prepare_create_entry(&mut entry, world_id);
        match persist_entry_upsert(pool, &entry) {
            ImportOutcome::Created => {
                summary.entries.created += 1;
                target_entry_ids.insert(entry.entry_id.clone());
                record_entry(&mut summary, &pack_entry_id, ImportOutcome::Created, None);
            }
            ImportOutcome::Rejected => {
                summary.entries.rejected += 1;
                record_entry(
                    &mut summary,
                    &pack_entry_id,
                    ImportOutcome::Rejected,
                    Some("orchestrate_upsert rejected entry".to_string()),
                );
            }
            _ => {}
        }
    }

    // ── Phase 2: relations ─────────────────────────────────────────────
    for mut relation in pack.relations {
        let pack_relation_id = relation.relation_id.clone();
        let resolved_from = remap
            .get(&relation.from_id)
            .cloned()
            .unwrap_or_else(|| relation.from_id.clone());
        let resolved_to = remap
            .get(&relation.to_id)
            .cloned()
            .unwrap_or_else(|| relation.to_id.clone());

        let source_ok = target_entry_ids.contains(&resolved_from);
        let target_ok = target_entry_ids.contains(&resolved_to);
        if !source_ok || !target_ok {
            let reason = if !source_ok && !target_ok {
                "both endpoints missing from target world"
            } else if !source_ok {
                "source endpoint missing from target world"
            } else {
                "target endpoint missing from target world"
            };
            record_relation(
                &mut summary,
                &pack_relation_id,
                ImportOutcome::Skipped,
                Some(reason.to_string()),
            );
            summary.relations.skipped += 1;
            continue;
        }

        relation.from_id = resolved_from;
        relation.to_id = resolved_to;

        if let Ok(existing_rel) = get_relationship(pool, &relation.relation_id).await {
            if existing_rel.world_id == world_id {
                match conflict {
                    ConflictPolicy::Skip => {
                        record_relation(
                            &mut summary,
                            &pack_relation_id,
                            ImportOutcome::Skipped,
                            Some("relation already exists in target world".to_string()),
                        );
                        summary.relations.skipped += 1;
                        continue;
                    }
                    ConflictPolicy::Rename => {
                        relation.relation_id = generate_relationship_id();
                    }
                    ConflictPolicy::Overwrite => {
                        relation.revision = Some(u64::try_from(existing_rel.revision).map_err(
                            |e| {
                                PackImportError::Storage(format!(
                                    "invalid relation revision for {}: {e}",
                                    existing_rel.relationship_id
                                ))
                            },
                        )?);
                        if dry_run {
                            record_relation(
                                &mut summary,
                                &pack_relation_id,
                                ImportOutcome::Overwritten,
                                Some("dry-run: would overwrite relation body".to_string()),
                            );
                            summary.relations.overwritten += 1;
                            continue;
                        }
                        update_relation_world_id(&mut relation, world_id);
                        match persist_relation_relate(pool, &relation) {
                            ImportOutcome::Overwritten => {
                                summary.relations.overwritten += 1;
                                record_relation(
                                    &mut summary,
                                    &pack_relation_id,
                                    ImportOutcome::Overwritten,
                                    None,
                                );
                            }
                            ImportOutcome::Rejected => {
                                summary.relations.rejected += 1;
                                record_relation(
                                    &mut summary,
                                    &pack_relation_id,
                                    ImportOutcome::Rejected,
                                    Some("orchestrate_relate rejected relation".to_string()),
                                );
                            }
                            _ => {}
                        }
                        continue;
                    }
                }
            } else {
                relation.relation_id = generate_relationship_id();
            }
        }

        if dry_run {
            let outcome = if relation.relation_id != pack_relation_id {
                ImportOutcome::Renamed
            } else {
                ImportOutcome::Created
            };
            if outcome == ImportOutcome::Renamed {
                summary.relations.renamed += 1;
            } else {
                summary.relations.created += 1;
            }
            record_relation(
                &mut summary,
                &pack_relation_id,
                outcome,
                Some("dry-run: would create".to_string()),
            );
            continue;
        }

        update_relation_world_id(&mut relation, world_id);
        relation.revision = None;

        let renamed = relation.relation_id != pack_relation_id;
        match persist_relation_relate(pool, &relation) {
            ImportOutcome::Created => {
                if renamed {
                    summary.relations.renamed += 1;
                    record_relation(&mut summary, &pack_relation_id, ImportOutcome::Renamed, None);
                } else {
                    summary.relations.created += 1;
                    record_relation(&mut summary, &pack_relation_id, ImportOutcome::Created, None);
                }
            }
            ImportOutcome::Rejected => {
                summary.relations.rejected += 1;
                record_relation(
                    &mut summary,
                    &pack_relation_id,
                    ImportOutcome::Rejected,
                    Some("orchestrate_relate rejected relation".to_string()),
                );
            }
            _ => {}
        }
    }

    Ok(summary)
}

fn handle_entry_id_collision_in_target(
    summary: &mut ImportSummary,
    target_entry_ids: &mut HashSet<String>,
    pack_entry_id: &str,
    dry_run: bool,
) {
    record_entry(
        summary,
        pack_entry_id,
        ImportOutcome::Skipped,
        Some(if dry_run {
            "dry-run: entry_id already exists in target world".to_string()
        } else {
            "entry_id already exists in target world".to_string()
        }),
    );
    summary.entries.skipped += 1;
    target_entry_ids.insert(pack_entry_id.to_string());
}

async fn import_renamed_entry(
    pool: &SqlitePool,
    world_id: &str,
    entry: &mut KnowledgeEntry,
    entry_type: BlockType,
    pack_entry_id: &str,
    summary: &mut ImportSummary,
    target_entry_ids: &mut HashSet<String>,
    remap: &mut HashMap<String, String>,
    dry_run: bool,
) -> Result<(), PackImportError> {
    let store = SqliteKbStore::new(pool.clone());
    let original_name = entry.canonical_name.to_string();
    let disambiguated =
        disambiguate_canonical_name(&store, world_id, &original_name, entry_type).await?;
    let fresh_id = mint_entry_id();

    if dry_run {
        record_entry(
            summary,
            pack_entry_id,
            ImportOutcome::Renamed,
            Some(format!(
                "dry-run: would rename to {disambiguated} as {fresh_id}"
            )),
        );
        summary.entries.renamed += 1;
        remap.insert(pack_entry_id.to_string(), fresh_id.clone());
        target_entry_ids.insert(fresh_id);
        return Ok(());
    }

    entry.entry_id = fresh_id.clone();
    entry.canonical_name = disambiguated.parse().map_err(|e| {
        PackImportError::Storage(format!("invalid disambiguated canonical_name: {e}"))
    })?;
    prepare_create_entry(entry, world_id);
    remap.insert(pack_entry_id.to_string(), fresh_id.clone());

    match persist_entry_upsert(pool, entry) {
        ImportOutcome::Created => {
            summary.entries.renamed += 1;
            target_entry_ids.insert(fresh_id);
            record_entry(
                summary,
                pack_entry_id,
                ImportOutcome::Renamed,
                Some(format!("renamed to {disambiguated}")),
            );
        }
        ImportOutcome::Rejected => {
            summary.entries.rejected += 1;
            record_entry(
                summary,
                pack_entry_id,
                ImportOutcome::Rejected,
                Some("orchestrate_upsert rejected entry".to_string()),
            );
        }
        _ => {}
    }
    Ok(())
}

async fn import_overwritten_entry(
    pool: &SqlitePool,
    world_id: &str,
    entry: &mut KnowledgeEntry,
    existing: &WorldKbEntry,
    pack_entry_id: &str,
    summary: &mut ImportSummary,
    target_entry_ids: &mut HashSet<String>,
    remap: &mut HashMap<String, String>,
    dry_run: bool,
) -> Result<(), PackImportError> {
    let existing_id = existing.entry_id.clone();

    if dry_run {
        record_entry(
            summary,
            pack_entry_id,
            ImportOutcome::Overwritten,
            Some(format!("dry-run: would overwrite {existing_id}")),
        );
        summary.entries.overwritten += 1;
        remap.insert(pack_entry_id.to_string(), existing_id.clone());
        target_entry_ids.insert(existing_id);
        return Ok(());
    }

    entry.entry_id = existing_id.clone();
    entry.status = existing.status.clone();
    entry.revision = Some(existing.revision.unwrap_or(0));
    extensions::set_world_id(entry, world_id.to_string());
    extensions::set_provenance(entry, None, None, Some(IMPORT_PROVENANCE.to_string()));

    match persist_entry_upsert(pool, entry) {
        ImportOutcome::Created => {
            stamp_import_provenance_column(pool, world_id, &existing_id).await?;
            remap.insert(pack_entry_id.to_string(), existing_id.clone());
            target_entry_ids.insert(existing_id.clone());
            summary.entries.overwritten += 1;
            record_entry(
                summary,
                pack_entry_id,
                ImportOutcome::Overwritten,
                Some(format!("overwrote {existing_id}")),
            );
        }
        ImportOutcome::Rejected => {
            record_entry(
                summary,
                pack_entry_id,
                ImportOutcome::Rejected,
                Some("orchestrate_upsert rejected entry".to_string()),
            );
            summary.entries.rejected += 1;
        }
        _ => {}
    }
    Ok(())
}


async fn stamp_import_provenance_column(
    pool: &SqlitePool,
    world_id: &str,
    entry_id: &str,
) -> Result<(), PackImportError> {
    // SAFETY: UPDATE against known kb_key_blocks schema.
    sqlx::query(
        "UPDATE kb_key_blocks SET source_provenance_kind = ?          WHERE key_block_id = ? AND world_id = ?",
    )
    .bind(IMPORT_PROVENANCE)
    .bind(entry_id)
    .bind(world_id)
    .execute(pool)
    .await
    .map_err(|e| {
        PackImportError::Storage(format!(
            "failed to stamp pack_import provenance on {entry_id}: {e}"
        ))
    })?;
    Ok(())
}

async fn disambiguate_canonical_name(
    store: &SqliteKbStore,
    world_id: &str,
    original: &str,
    entry_type: BlockType,
) -> Result<String, PackImportError> {
    let first = format!("{original} imported");
    if name_available(store, world_id, &first, entry_type).await? {
        return Ok(first);
    }
    let mut n = 2u32;
    loop {
        let candidate = format!("{original} imported {n}");
        if name_available(store, world_id, &candidate, entry_type).await? {
            return Ok(candidate);
        }
        n += 1;
    }
}

async fn name_available(
    store: &SqliteKbStore,
    world_id: &str,
    canonical_name: &str,
    entry_type: BlockType,
) -> Result<bool, PackImportError> {
    let existing = store
        .get_active_by_unique_key(world_id, canonical_name, entry_type)
        .await
        .map_err(|e| PackImportError::Storage(format!("unique-key lookup failed: {e}")))?;
    Ok(existing.is_none())
}

fn mint_entry_id() -> String {
    format!(
        "kb_{}",
        uuid::Uuid::new_v4().to_string().replace('-', "")
    )
}

fn prepare_create_entry(entry: &mut KnowledgeEntry, world_id: &str) {
    entry.revision = None;
    extensions::set_world_id(entry, world_id.to_string());
    extensions::set_provenance(entry, None, None, Some(IMPORT_PROVENANCE.to_string()));
}

fn persist_entry_upsert(pool: &SqlitePool, entry: &KnowledgeEntry) -> ImportOutcome {
    let upsert_req = build_import_upsert_request(entry);
    let adapter = NexusAdapter::new(pool.clone());
    match orchestrate_upsert(&adapter, upsert_req) {
        nexus_spoke_adapter::SpokeResult::Ok(_) => ImportOutcome::Created,
        nexus_spoke_adapter::SpokeResult::Reject(reject) => {
tracing::warn!(
                entry_id = %entry.entry_id,
                code = %reject.code,
                "orchestrate_upsert rejected pack import entry: {}",
                reject.message
            );
            ImportOutcome::Rejected
        }
    }
}

fn persist_relation_relate(pool: &SqlitePool, relation: &Relation) -> ImportOutcome {
    let relate_req = build_import_relate_request(relation);
    let adapter = NexusAdapter::new(pool.clone());
    match orchestrate_relate(&adapter, relate_req) {
        nexus_spoke_adapter::SpokeResult::Ok(_) => {
            if relation.revision.is_some() {
                ImportOutcome::Overwritten
            } else {
                ImportOutcome::Created
            }
        }
        nexus_spoke_adapter::SpokeResult::Reject(reject) => {
            tracing::warn!(
                relation_id = %relation.relation_id,
                code = %reject.code,
                "orchestrate_relate rejected pack import relation: {}",
                reject.message
            );
            ImportOutcome::Rejected
        }
    }
}

fn record_entry(
    summary: &mut ImportSummary,
    id: &str,
    outcome: ImportOutcome,
    reason: Option<String>,
) {
    summary.details.push(ImportDetail {
        kind: ImportAtomKind::Entry,
        id: id.to_string(),
        outcome,
        reason,
    });
}

fn record_relation(
    summary: &mut ImportSummary,
    id: &str,
    outcome: ImportOutcome,
    reason: Option<String>,
) {
    summary.details.push(ImportDetail {
        kind: ImportAtomKind::Relation,
        id: id.to_string(),
        outcome,
        reason,
    });
}

fn parse_entry_type(s: &str) -> Option<BlockType> {
    serde_json::from_value(serde_json::Value::String(s.to_string())).ok()
}

fn update_relation_world_id(relation: &mut Relation, world_id: &str) {
    let key = RelationExtensionsKey::try_from("nexus")
        .expect("\"nexus\" matches the extensions-key regex");
    let ns = relation.extensions.entry(key).or_default();
    ns.insert(
        "world_id".to_string(),
        serde_json::Value::String(world_id.to_string()),
    );
}

fn build_import_upsert_request(entry: &KnowledgeEntry) -> UpsertRequest {
    let wire = serde_json::to_value(entry).unwrap_or_else(|_| serde_json::json!({}));
    serde_json::from_value(serde_json::json!({ "knowledge_entries": [wire] }))
        .expect("KnowledgeEntry fits UpsertRequest shape")
}

fn build_import_relate_request(relation: &Relation) -> RelateRequest {
    let wire = serde_json::to_value(relation).unwrap_or_else(|_| serde_json::json!({}));
    serde_json::from_value(serde_json::json!({ "relation": wire }))
        .expect("Relation fits RelateRequest shape")
}
