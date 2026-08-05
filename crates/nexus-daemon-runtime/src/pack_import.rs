//! Shared Knowledge Pack import orchestration (V1.152 P0, DF-77).
//!
//! Extracted from the V1.146 CLI `pack.rs::import` core so CLI and daemon
//! routes share one conflict-policy path. Caller MUST have already passed the
//! owner gate — this module does not re-check ownership (matches
//! [`crate::directive_store::LocalDirectiveStore`] precedent).

use nexus_contracts::BlockType;
use nexus_knowledge::world_kb::validation::CANONICAL_NAME_MAX_LEN;
use nexus_knowledge::world_kb::KbStore;
use nexus_knowledge::world_kb::WorldKbEntry;
use nexus_local_db::kb_relationships::{generate_relationship_id, get_relationship};
use nexus_local_db::kb_store::SqliteKbStore;
use nexus_spoke_adapter::pack::ParsedPack;
use nexus_spoke_adapter::{
    extensions, orchestrate_relate, orchestrate_upsert, KnowledgeEntry, NexusAdapter,
    RelateRequest, Relation, RelationExtensionsKey, UpsertRequest,
};
use sqlx::SqlitePool;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use thiserror::Error;

/// Outcome of a single orchestrator persist call, with optional reject detail.
struct PersistOutcome {
    outcome: ImportOutcome,
    reject_reason: Option<String>,
}

const DISAMBIGUATE_MAX_ATTEMPTS: u32 = 100;

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
#[allow(
    clippy::too_many_lines,
    clippy::too_many_arguments,
    clippy::if_not_else
)]
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
            tracing::warn!(
                entry_id = %pack_entry_id,
                canonical_name = %entry.canonical_name.as_str(),
                entry_type = %entry.entry_type,
                "unknown entry_type in pack import, skipping"
            );
            record_entry(
                &mut summary,
                &pack_entry_id,
                ImportOutcome::Skipped,
                Some(reason),
            );
            summary.entries.skipped += 1;
            continue;
        };

        // ── Entry ID collision (global PK, world-scoped semantics) ─────
        if let Ok(existing_by_id) = store.get_knowledge_entry(&entry.entry_id).await {
            if existing_by_id.world_id == world_id {
                // Same-world re-import: when entry_id and canonical_name both
                // match, this is the same entry — honor conflict policy.
                // If canonical_name differs (ambiguous), keep conservative skip.
                let is_same_entry = existing_by_id.canonical_name == entry.canonical_name.as_str()
                    && existing_by_id.block_type == entry_type;

                if is_same_entry {
                    match conflict {
                        ConflictPolicy::Skip => {
                            remap.insert(pack_entry_id.clone(), existing_by_id.entry_id.clone());
                            target_entry_ids.insert(existing_by_id.entry_id.clone());
                            record_entry(
                                &mut summary,
                                &pack_entry_id,
                                ImportOutcome::Skipped,
                                Some(if dry_run {
                                    "dry-run: same-world re-import (entry_id + canonical_name match)"
                                        .to_string()
                                } else {
                                    "same-world re-import (entry_id + canonical_name match)"
                                        .to_string()
                                }),
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
                                &existing_by_id,
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
            PersistOutcome {
                outcome: ImportOutcome::Created,
                ..
            } => {
                summary.entries.created += 1;
                target_entry_ids.insert(entry.entry_id.clone());
                record_entry(&mut summary, &pack_entry_id, ImportOutcome::Created, None);
            }
            PersistOutcome {
                outcome: ImportOutcome::Rejected,
                reject_reason,
            } => {
                summary.entries.rejected += 1;
                record_entry(
                    &mut summary,
                    &pack_entry_id,
                    ImportOutcome::Rejected,
                    reject_reason,
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
                        relation.revision =
                            Some(u64::try_from(existing_rel.revision).map_err(|e| {
                                PackImportError::Storage(format!(
                                    "invalid relation revision for {}: {e}",
                                    existing_rel.relationship_id
                                ))
                            })?);
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
                            PersistOutcome {
                                outcome: ImportOutcome::Overwritten,
                                ..
                            } => {
                                summary.relations.overwritten += 1;
                                record_relation(
                                    &mut summary,
                                    &pack_relation_id,
                                    ImportOutcome::Overwritten,
                                    None,
                                );
                            }
                            PersistOutcome {
                                outcome: ImportOutcome::Rejected,
                                reject_reason,
                            } => {
                                summary.relations.rejected += 1;
                                record_relation(
                                    &mut summary,
                                    &pack_relation_id,
                                    ImportOutcome::Rejected,
                                    reject_reason,
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
            PersistOutcome {
                outcome: ImportOutcome::Created,
                ..
            } => {
                if renamed {
                    summary.relations.renamed += 1;
                    record_relation(
                        &mut summary,
                        &pack_relation_id,
                        ImportOutcome::Renamed,
                        None,
                    );
                } else {
                    summary.relations.created += 1;
                    record_relation(
                        &mut summary,
                        &pack_relation_id,
                        ImportOutcome::Created,
                        None,
                    );
                }
            }
            PersistOutcome {
                outcome: ImportOutcome::Rejected,
                reject_reason,
            } => {
                summary.relations.rejected += 1;
                record_relation(
                    &mut summary,
                    &pack_relation_id,
                    ImportOutcome::Rejected,
                    reject_reason,
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

#[allow(clippy::too_many_arguments)]
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
        match disambiguate_canonical_name(&store, world_id, &original_name, entry_type).await {
            Ok(name) => name,
            Err(e) => {
                summary.entries.rejected += 1;
                record_entry(
                    summary,
                    pack_entry_id,
                    ImportOutcome::Rejected,
                    Some(e.to_string()),
                );
                return Ok(());
            }
        };
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

    entry.entry_id.clone_from(&fresh_id);
    let parsed_name = match disambiguated.parse() {
        Ok(name) => name,
        Err(e) => {
            summary.entries.rejected += 1;
            record_entry(
                summary,
                pack_entry_id,
                ImportOutcome::Rejected,
                Some(format!("invalid disambiguated canonical_name: {e}")),
            );
            return Ok(());
        }
    };
    entry.canonical_name = parsed_name;
    prepare_create_entry(entry, world_id);
    remap.insert(pack_entry_id.to_string(), fresh_id.clone());

    match persist_entry_upsert(pool, entry) {
        PersistOutcome {
            outcome: ImportOutcome::Created,
            ..
        } => {
            summary.entries.renamed += 1;
            target_entry_ids.insert(fresh_id);
            record_entry(
                summary,
                pack_entry_id,
                ImportOutcome::Renamed,
                Some(format!("renamed to {disambiguated}")),
            );
        }
        PersistOutcome {
            outcome: ImportOutcome::Rejected,
            reject_reason,
        } => {
            summary.entries.rejected += 1;
            record_entry(
                summary,
                pack_entry_id,
                ImportOutcome::Rejected,
                reject_reason,
            );
        }
        _ => {}
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
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
        PersistOutcome {
            outcome: ImportOutcome::Created,
            ..
        } => {
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
        PersistOutcome {
            outcome: ImportOutcome::Rejected,
            reject_reason,
        } => {
            record_entry(
                summary,
                pack_entry_id,
                ImportOutcome::Rejected,
                reject_reason,
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
        "UPDATE kb_key_blocks SET source_provenance_kind = ? WHERE key_block_id = ? AND world_id = ?",
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

fn truncated_name_with_suffix(base: &str, suffix: &str) -> String {
    let max_base_len = CANONICAL_NAME_MAX_LEN.saturating_sub(suffix.len());
    let fitted_base = fit_canonical_name_base(base, max_base_len);
    format!("{fitted_base}{suffix}")
}

fn fit_canonical_name_base(base: &str, max_len: usize) -> String {
    if base.len() <= max_len {
        return base.to_string();
    }
    if max_len == 0 {
        return String::new();
    }
    #[allow(clippy::items_after_statements)]
    const HASH_TAG_LEN: usize = 9; // "~" + 8 hex chars
    if max_len <= HASH_TAG_LEN {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        base.hash(&mut hasher);
        let hash = format!("{:x}", hasher.finish());
        return hash.chars().take(max_len).collect();
    }
    let prefix_len = max_len - HASH_TAG_LEN;
    let prefix = truncate_to_char_boundary(base, prefix_len);
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    base.hash(&mut hasher);
    let hash = format!("{:08x}", hasher.finish() & 0xffff_ffff);
    format!("{prefix}~{hash}")
}

fn truncate_to_char_boundary(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

async fn disambiguate_canonical_name(
    store: &SqliteKbStore,
    world_id: &str,
    original: &str,
    entry_type: BlockType,
) -> Result<String, PackImportError> {
    let first_suffix = " imported";
    let first = truncated_name_with_suffix(original, first_suffix);
    if first.len() <= CANONICAL_NAME_MAX_LEN
        && name_available(store, world_id, &first, entry_type).await?
    {
        return Ok(first);
    }
    for n in 2..=DISAMBIGUATE_MAX_ATTEMPTS {
        let suffix = format!(" imported {n}");
        let candidate = truncated_name_with_suffix(original, &suffix);
        if candidate.len() <= CANONICAL_NAME_MAX_LEN
            && name_available(store, world_id, &candidate, entry_type).await?
        {
            return Ok(candidate);
        }
    }
    Err(PackImportError::Storage(format!(
        "failed to disambiguate canonical_name '{original}' after {DISAMBIGUATE_MAX_ATTEMPTS} attempts"
    )))
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
    format!("kb_{}", uuid::Uuid::new_v4().to_string().replace('-', ""))
}

fn prepare_create_entry(entry: &mut KnowledgeEntry, world_id: &str) {
    entry.revision = None;
    extensions::set_world_id(entry, world_id.to_string());
    extensions::set_provenance(entry, None, None, Some(IMPORT_PROVENANCE.to_string()));
}

fn persist_entry_upsert(pool: &SqlitePool, entry: &KnowledgeEntry) -> PersistOutcome {
    let upsert_req = build_import_upsert_request(entry);
    let adapter = NexusAdapter::new(pool.clone());
    match orchestrate_upsert(&adapter, upsert_req) {
        nexus_spoke_adapter::SpokeResult::Ok(_) => PersistOutcome {
            outcome: ImportOutcome::Created,
            reject_reason: None,
        },
        nexus_spoke_adapter::SpokeResult::Reject(reject) => {
            tracing::warn!(
                entry_id = %entry.entry_id,
                code = %reject.code,
                "orchestrate_upsert rejected pack import entry: {}",
                reject.message
            );
            PersistOutcome {
                outcome: ImportOutcome::Rejected,
                reject_reason: Some(format!("{}: {}", reject.code, reject.message)),
            }
        }
    }
}

fn persist_relation_relate(pool: &SqlitePool, relation: &Relation) -> PersistOutcome {
    let relate_req = build_import_relate_request(relation);
    let adapter = NexusAdapter::new(pool.clone());
    match orchestrate_relate(&adapter, relate_req) {
        nexus_spoke_adapter::SpokeResult::Ok(_) => PersistOutcome {
            outcome: if relation.revision.is_some() {
                ImportOutcome::Overwritten
            } else {
                ImportOutcome::Created
            },
            reject_reason: None,
        },
        nexus_spoke_adapter::SpokeResult::Reject(reject) => {
            tracing::warn!(
                relation_id = %relation.relation_id,
                code = %reject.code,
                "orchestrate_relate rejected pack import relation: {}",
                reject.message
            );
            PersistOutcome {
                outcome: ImportOutcome::Rejected,
                reject_reason: Some(format!("{}: {}", reject.code, reject.message)),
            }
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
