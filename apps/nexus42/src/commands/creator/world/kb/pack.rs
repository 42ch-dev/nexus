//! Portable Knowledge-pack I/O — `creator world kb pack export|import`.
//!
//! V1.146 P3 (plan `2026-07-30-v1.146-p3-knowledge-pack-io-cli`): ships both
//! `export` (T2) and `import` (T3, additive, `skip` default conflict policy).
//!
//! # Why under `creator world kb pack`
//!
//! Per the pack-IO product behavior doc (`.mstar/iterations/v1.146/specs/
//! pack-io-product-behavior.md`): "Pack is a World-lore transport, not a
//! platform/user knowledge surface and not a top-level `pack` command." World
//! KB already lives under `creator world kb *` (list/show/edit/...), so pack
//! I/O is a nested subcommand here.
//!
//! # Export shape
//!
//! A Narrative Knowledge Pack (spoke handbook `domain-profile-narrative-
//! knowledge-pack.md`) is a single JSON file:
//!
//! ```text
//! {
//!   "modules": { "pack": { "title", "version", "creator", "description?" } },
//!   "entries": [ /* KnowledgeEntry[] ordered by canonical_name ASC */ ],
//!   "relations": [ /* Relation[] ordered by relationship_id ASC */ ],
//!   "source_anchors": [ /* optional; omitted unless --include-anchors set */ ]
//! }
//! ```
//!
//! Pack build/parse helpers live in [`nexus_spoke_adapter::pack`]; this module
//! is the CLI wiring only.

use crate::config::CliConfig;
use crate::errors::{CliError, Result};
use clap::Subcommand;
use nexus_contracts::BlockType;
use nexus_knowledge::world_kb::KbStore;
use nexus_local_db::kb_relationships::{get_relationship, list_relationships_for_world};
use nexus_local_db::kb_store::SqliteKbStore;
use nexus_spoke_adapter::conversion::{kb_relationship_row_to_spoke, world_kb_to_spoke};
use nexus_spoke_adapter::pack::{build_pack, parse_pack};
use nexus_spoke_adapter::{
    extensions, orchestrate_relate, orchestrate_upsert, KnowledgeEntry, NexusAdapter,
    RelateRequest, Relation, RelationExtensionsKey, UpsertRequest,
};
use sqlx::SqlitePool;
use std::collections::HashSet;
use std::path::PathBuf;

/// Default version string stamped into `modules.pack.version` when
/// `--pack-version` is not supplied.
const DEFAULT_PACK_VERSION: &str = "0.1.0";

/// Fallback author string stamped into `modules.pack.creator` when no active
/// Creator profile is resolvable (e.g. hermetic tests, ad-hoc workspaces).
const FALLBACK_CREATOR: &str = "nexus42";

/// Provenance stamp on imported Knowledge entries.
///
/// Product lock (`pack-io-product-behavior.md` §Interfaces): import-created
/// entries must carry `source_provenance_kind = "pack_import"`. The DB CHECK
/// (expanded in migration `20260731000001`) now includes this value.
const IMPORT_PROVENANCE: &str = "pack_import";

/// `creator world kb pack` subcommands.
#[derive(Debug, Subcommand)]
pub enum PackCommand {
    /// Export one world's Knowledge entries and their relations to a portable
    /// Narrative Knowledge Pack JSON file.
    Export(ExportArgs),
    /// Import Knowledge entries and relations from a Narrative Knowledge Pack
    /// JSON file into a world (additive — never deletes existing atoms).
    Import(ImportArgs),
}

/// Arguments for `creator world kb pack export`.
#[derive(Debug, clap::Args)]
pub struct ExportArgs {
    /// World reference — the world ID (e.g. `wld_abc123`)
    pub world_ref: String,

    /// Output path for the pack JSON file (required).
    #[arg(long)]
    pub out: PathBuf,

    /// Pack title override (default: the world's title).
    #[arg(long)]
    pub title: Option<String>,

    /// Pack version string written into `modules.pack.version`.
    #[arg(long, default_value = DEFAULT_PACK_VERSION)]
    pub pack_version: String,

    /// Include deprecated (inactive) Knowledge entries. By default only active
    /// (non-deleted / non-merged / non-deprecated) entries are exported.
    #[arg(long)]
    pub include_deprecated: bool,

    /// Include `source_anchors` in the pack. The nexus local store does not
    /// yet persist a `SourceAnchor` store, so this flag emits an empty array and
    /// is accepted for forward-compatibility with the spoke handbook shape.
    #[arg(long)]
    pub include_anchors: bool,
}

/// Dispatch a `creator world kb pack` subcommand.
///
/// `pool` is the already-opened workspace pool (the parent `kb::run` resolves
/// it once so we don't re-open per subcommand).
///
/// # Errors
///
/// Returns `CliError` on world-not-found, store I/O failure, JSON write
/// failure, or when the active creator is required but unresolvable.
// CLI entry-point runs on a single-threaded tokio runtime — Send not required.
#[allow(clippy::future_not_send)]
pub async fn run(cmd: PackCommand, config: &CliConfig, pool: &SqlitePool) -> Result<()> {
    match cmd {
        PackCommand::Export(args) => export(args, config, pool).await,
        PackCommand::Import(args) => import(args, pool).await,
    }
}

/// Conflict-resolution policy for the import command.
///
/// Only `Skip` is implemented in V1.146 P3; `Rename` and `Overwrite` are
/// accepted by clap but produce a "not yet implemented" runtime error.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum ConflictStrategy {
    /// Skip existing entries/relations (default).
    Skip,
    /// Rename conflicting entries (not yet implemented).
    Rename,
    /// Overwrite existing entries (not yet implemented).
    Overwrite,
}

/// Arguments for `creator world kb pack import`.
#[derive(Debug, clap::Args)]
pub struct ImportArgs {
    /// World reference — the world ID (e.g. `wld_abc123`).
    pub world_ref: String,

    /// Input path for the pack JSON file (required).
    #[arg(long)]
    pub r#in: PathBuf,

    /// Print the create/skip plan without performing any writes.
    #[arg(long)]
    pub dry_run: bool,

    /// Conflict-resolution policy when an `entry_id` or canonical name already
    /// exists in the target world.
    #[arg(long, value_enum, default_value_t = ConflictStrategy::Skip)]
    pub conflict: ConflictStrategy,
}

/// `creator world kb pack export` implementation.
///
/// # Errors
///
/// Returns `CliError` if the world cannot be resolved, the KB store query
/// fails, relation listing fails, or writing the pack file fails.
async fn export(args: ExportArgs, config: &CliConfig, pool: &SqlitePool) -> Result<()> {
    let world_id = args.world_ref.as_str();

    // ── Resolve world title (for default pack title) ──────────────────
    let world_title = resolve_world_title(pool, world_id).await?;

    // ── Resolve creator string (active creator id/name, else fallback) ─
    let creator = resolve_creator_string(pool, config.active_creator_id.as_deref()).await?;

    // ── Load Knowledge entries ────────────────────────────────────────
    let store = SqliteKbStore::new(pool.clone());
    let mut entries = if args.include_deprecated {
        store
            .list_by_world_including_deprecated(world_id)
            .await
            .map_err(|e| CliError::Other(format!("World KB list failed for {world_id}: {e}")))?
    } else {
        store
            .list_by_world(world_id)
            .await
            .map_err(|e| CliError::Other(format!("World KB list failed for {world_id}: {e}")))?
    };

    // Stable order: by canonical_name ascending (deterministic packs for
    // diffability — product behavior doc §Export defaults).
    entries.sort_by(|a, b| a.canonical_name.cmp(&b.canonical_name));

    let entry_ids: HashSet<String> = entries.iter().map(|e| e.entry_id.clone()).collect();

    // ── Load relations, filter to both-endpoints-in-set ───────────────
    //
    // Product behavior doc (pack-io-product-behavior.md §Export defaults):
    // "relations where BOTH endpoints are in the exported entry set". We
    // list confirmed (non-suggested) relations for the world and intersect.
    // `list_relationships_for_world(..., include_suggested=false, ...)`
    // excludes `needs_review = 1` extraction suggestions; `--include-deprecated`
    // does not widen relations (relations are not deprecated individually).
    let relation_rows = list_relationships_for_world(pool, world_id, false, i64::MAX)
        .await
        .map_err(|e| CliError::Other(format!("Failed to list relations for {world_id}: {e}")))?;

    let mut relations: Vec<nexus_spoke_adapter::Relation> = relation_rows
        .iter()
        .filter(|r| {
            entry_ids.contains(&r.source_entity_id) && entry_ids.contains(&r.target_entity_id)
        })
        .map(kb_relationship_row_to_spoke)
        .collect();

    // Stable order: by relationship_id ascending (deterministic packs).
    relations.sort_by(|a, b| a.relation_id.cmp(&b.relation_id));

    // ── Convert entries to spoke KnowledgeEntry ───────────────────────
    let spoke_entries: Vec<nexus_spoke_adapter::KnowledgeEntry> =
        entries.iter().map(world_kb_to_spoke).collect();

    // ── Anchors ───────────────────────────────────────────────────────
    // nexus has no persisted SourceAnchor store; accept the flag but emit
    // an empty array (per task brief — do NOT fabricate anchors).
    let anchors: Option<&[nexus_spoke_adapter::SourceAnchor]> = if args.include_anchors {
        Some(&[])
    } else {
        None
    };

    // ── Pack metadata ─────────────────────────────────────────────────
    let title = args.title.unwrap_or(world_title);

    let pack_value = build_pack(
        &spoke_entries,
        &relations,
        anchors,
        &title,
        &args.pack_version,
        &creator,
        None,
        None,
    );

    // ── Write to disk ─────────────────────────────────────────────────
    let out_path = &args.out;
    if let Some(parent) = out_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| {
                CliError::Other(format!(
                    "Failed to create output directory {}: {e}",
                    parent.display()
                ))
            })?;
        }
    }
    let json_str = serde_json::to_string_pretty(&pack_value)?;
    std::fs::write(out_path, json_str.as_bytes()).map_err(|e| {
        CliError::Other(format!(
            "Failed to write pack file {}: {e}",
            out_path.display()
        ))
    })?;

    // ── Success summary ───────────────────────────────────────────────
    println!("✓ Knowledge pack exported: {}", out_path.display());
    println!("  Title:     {title}");
    println!("  Version:   {}", args.pack_version);
    println!("  Creator:   {creator}");
    println!("  Entries:   {}", spoke_entries.len());
    println!("  Relations: {}", relations.len());
    if args.include_anchors {
        println!("  Anchors:   0 (no persisted SourceAnchor store in nexus)");
    }

    Ok(())
}

// ── Import ─────────────────────────────────────────────────────────────

/// `creator world kb pack import` implementation.
///
/// # Errors
///
/// Returns `CliError` if the world cannot be resolved, the pack file cannot be
/// read or parsed, or the conflict strategy is `rename`/`overwrite` (not yet
/// implemented).
// Entry + relation phases in one function mirrors export's single-function
// style. Extracting sub-phases would add indirection with no reuse benefit.
#[allow(clippy::too_many_lines)]
async fn import(args: ImportArgs, pool: &SqlitePool) -> Result<()> {
    // ── Conflict strategy gate ─────────────────────────────────────────
    if !matches!(args.conflict, ConflictStrategy::Skip) {
        return Err(CliError::Other(
            "Conflict strategy 'rename' / 'overwrite' is not yet implemented. \
             Only 'skip' (the default) is available in V1.146 P3."
                .to_string(),
        ));
    }

    let world_id = args.world_ref.as_str();

    // ── Verify world exists ────────────────────────────────────────────
    let _title = resolve_world_title(pool, world_id).await?;

    // ── Read and parse pack ────────────────────────────────────────────
    let text = std::fs::read_to_string(&args.r#in).map_err(|e| {
        CliError::Other(format!(
            "Failed to read pack file {}: {e}",
            args.r#in.display()
        ))
    })?;
    let value: serde_json::Value = serde_json::from_str(&text).map_err(|e| {
        CliError::Other(format!(
            "Invalid JSON in pack file {}: {e}",
            args.r#in.display()
        ))
    })?;
    let parsed = parse_pack(&value).map_err(|e| {
        CliError::Other(format!(
            "Invalid pack format in {}: {e}",
            args.r#in.display()
        ))
    })?;

    let store = SqliteKbStore::new(pool.clone());

    // ── Phase 1: Import entries ────────────────────────────────────────
    let mut entries_created = 0u32;
    let mut entries_skipped = 0u32;

    // Track which entry_ids are present in the target world after the
    // entry pass — both pre-existing and newly created — for the relation
    // endpoint resolution below.
    let mut target_entry_ids: HashSet<String> = HashSet::new();

    // Pre-populate with entries already existing in the target world.
    // This uses the same LIMIT as list_by_world, which is conservative but
    // works for the scope of this slice (export→import round-trip).
    if let Ok(existing) = store.list_by_world(world_id).await {
        for e in &existing {
            target_entry_ids.insert(e.entry_id.clone());
        }
    }

    for mut entry in parsed.entries {
        let entry_type = parse_entry_type(&entry.entry_type);

        // ── Collision check ──────────────────────────────────────────
        let collision = check_entry_collision(&store, world_id, &entry, entry_type).await;
        if collision {
            if args.dry_run {
                eprintln!(
                    "  [dry-run] skip entry {} ({:?}): already exists in target world",
                    entry.entry_id, entry.canonical_name
                );
            }
            entries_skipped += 1;
            // Still track the id for relation endpoint resolution.
            target_entry_ids.insert(entry.entry_id.clone());
            continue;
        }

        if args.dry_run {
            eprintln!(
                "  [dry-run] would create entry {} ({:?})",
                entry.entry_id, entry.canonical_name
            );
            entries_created += 1;
            target_entry_ids.insert(entry.entry_id.clone());
            continue;
        }

        // ── Create via orchestrate_upsert ────────────────────────────
        // Rebind to target world + stamp provenance.
        extensions::set_world_id(&mut entry, world_id.to_string());
        extensions::set_provenance(&mut entry, None, None, Some(IMPORT_PROVENANCE.to_string()));

        let upsert_req = build_import_upsert_request(&entry);
        let adapter = NexusAdapter::new(pool.clone());
        match orchestrate_upsert(&adapter, upsert_req) {
            nexus_spoke_adapter::SpokeResult::Ok(_) => {
                entries_created += 1;
                target_entry_ids.insert(entry.entry_id.clone());
            }
            nexus_spoke_adapter::SpokeResult::Reject(reject) => {
                eprintln!(
                    "  warn: orchestrate_upsert rejected entry {} ({:?}): {}: {}",
                    entry.entry_id, entry.canonical_name, reject.code, reject.message
                );
                entries_skipped += 1;
                // Still track so relations referencing this entry resolve.
                target_entry_ids.insert(entry.entry_id.clone());
            }
        }
    }

    // ── Phase 2: Import relations ──────────────────────────────────────
    let mut relations_created = 0u32;
    let mut relations_skipped = 0u32;

    for mut relation in parsed.relations {
        // ── Endpoint resolution ──────────────────────────────────────
        let source_ok = target_entry_ids.contains(&relation.from_id);
        let target_ok = target_entry_ids.contains(&relation.to_id);
        if !source_ok || !target_ok {
            if args.dry_run {
                let reason = if !source_ok && !target_ok {
                    "both endpoints missing"
                } else if !source_ok {
                    "source endpoint missing"
                } else {
                    "target endpoint missing"
                };
                eprintln!(
                    "  [dry-run] skip relation {} ({} → {}): {reason} from target world",
                    relation.relation_id, relation.from_id, relation.to_id
                );
            }
            relations_skipped += 1;
            continue;
        }

        // ── Collision check ─────────────────────────────────────────
        if get_relationship(pool, &relation.relation_id).await.is_ok() {
            if args.dry_run {
                eprintln!(
                    "  [dry-run] skip relation {}: already exists in target world",
                    relation.relation_id
                );
            }
            relations_skipped += 1;
            continue;
        }

        if args.dry_run {
            eprintln!(
                "  [dry-run] would create relation {} ({} → {})",
                relation.relation_id, relation.from_id, relation.to_id
            );
            relations_created += 1;
            continue;
        }

        // ── Create via orchestrate_relate ───────────────────────────
        // Rebind to target world + set create path revision.
        update_relation_world_id(&mut relation, world_id);
        // Use the pack's original relationship_id (globally unique).
        // Collision detection above ensures no duplicate IDs; preserving
        // the pack IDs makes re-import idempotent.
        relation.revision = None;

        let relate_req = build_import_relate_request(&relation);
        let adapter = NexusAdapter::new(pool.clone());
        match orchestrate_relate(&adapter, relate_req) {
            nexus_spoke_adapter::SpokeResult::Ok(_) => {
                relations_created += 1;
            }
            nexus_spoke_adapter::SpokeResult::Reject(reject) => {
                eprintln!(
                    "  warn: orchestrate_relate rejected relation {} → {}: {}: {}",
                    relation.relation_id, relation.to_id, reject.code, reject.message
                );
                relations_skipped += 1;
            }
        }
    }

    // ── Report ─────────────────────────────────────────────────────────
    let created = entries_created + relations_created;
    let skipped = entries_skipped + relations_skipped;
    if args.dry_run {
        println!("[dry-run] would create: {created}, would skip: {skipped}");
    } else {
        println!("created: {created}, skipped: {skipped}");
    }

    Ok(())
}

/// Check whether a pack entry would collide with an existing entry in the
/// target world — by `entry_id` or by unique key (`world_id`, `block_type`,
/// `canonical_name`).
async fn check_entry_collision(
    store: &SqliteKbStore,
    world_id: &str,
    entry: &KnowledgeEntry,
    entry_type: BlockType,
) -> bool {
    // Check by entry_id.
    if store.get_knowledge_entry(&entry.entry_id).await.is_ok() {
        return true;
    }
    // Check by active unique key.
    if let Ok(Some(_existing)) = store
        .get_active_by_unique_key(world_id, &entry.canonical_name, entry_type)
        .await
    {
        return true;
    }
    false
}

/// Parse a pack `entry_type` string to [`BlockType`] (unknown values →
/// default).
fn parse_entry_type(s: &str) -> BlockType {
    serde_json::from_value(serde_json::Value::String(s.to_string())).unwrap_or_default()
}

/// Update the nexus `world_id` in a pack relation's extensions for the target
/// world.
fn update_relation_world_id(relation: &mut Relation, world_id: &str) {
    let key = RelationExtensionsKey::try_from("nexus")
        .expect("\"nexus\" matches the extensions-key regex");
    let ns = relation.extensions.entry(key).or_default();
    ns.insert(
        "world_id".to_string(),
        serde_json::Value::String(world_id.to_string()),
    );
}

/// Wrap a [`KnowledgeEntry`] into an [`UpsertRequest`] via JSON round-trip.
///
/// Mirrors the daemon's `build_spoke_upsert_request` pattern: the spoke codegen
/// emits a distinct struct per wire shape, so the entry is serialized then
/// re-fit into the `UpsertRequest.knowledge_entries` slot.
fn build_import_upsert_request(entry: &KnowledgeEntry) -> UpsertRequest {
    let wire = serde_json::to_value(entry).unwrap_or_else(|_| serde_json::json!({}));
    serde_json::from_value(serde_json::json!({ "knowledge_entries": [wire] }))
        .expect("KnowledgeEntry fits UpsertRequest shape")
}

/// Wrap a [`Relation`] into a [`RelateRequest`] via JSON round-trip.
///
/// Mirrors the daemon's `build_spoke_relate_request` pattern.
fn build_import_relate_request(relation: &Relation) -> RelateRequest {
    let wire = serde_json::to_value(relation).unwrap_or_else(|_| serde_json::json!({}));
    serde_json::from_value(serde_json::json!({ "relation": wire }))
        .expect("Relation fits RelateRequest shape")
}

/// Resolve a world's human title from `narrative_worlds`.
///
/// Returns a clean `CliError::Other` (with a hint listing existing worlds)
/// when the world row is absent, matching the style used elsewhere in
/// `creator world show`.
async fn resolve_world_title(pool: &SqlitePool, world_id: &str) -> Result<String> {
    // SAFETY: static SELECT against known narrative_worlds table schema.
    let title: Option<String> =
        sqlx::query_scalar("SELECT title FROM narrative_worlds WHERE world_id = ?")
            .bind(world_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| CliError::Other(format!("Failed to query world '{world_id}': {e}")))?
            .flatten();

    title.ok_or_else(|| {
        CliError::Other(format!(
            "World '{world_id}' not found.\n  \
                 ↳ List existing worlds: nexus42 creator world list"
        ))
    })
}

/// Resolve the `modules.pack.creator` string.
///
/// Locked policy (product behavior doc): active Creator profile id/name if
/// resolvable from the workspace config + creators table; else the string
/// `"nexus42"`. We prefer the human `display_name` when available so packs
/// authored by a named profile carry the name, falling back to the raw
/// `creator_id` when the `display_name` is missing, then to the `nexus42`
/// fallback.
async fn resolve_creator_string(
    pool: &SqlitePool,
    active_creator_id: Option<&str>,
) -> Result<String> {
    let Some(cid) = active_creator_id else {
        return Ok(FALLBACK_CREATOR.to_string());
    };

    // SAFETY: static SELECT against known creators table schema.
    let display_name: Option<String> =
        sqlx::query_scalar("SELECT display_name FROM creators WHERE creator_id = ?")
            .bind(cid)
            .fetch_optional(pool)
            .await
            .map_err(|e| CliError::Other(format!("Failed to resolve creator '{cid}': {e}")))?
            .flatten();

    Ok(display_name
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| cid.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_contracts::BlockType;
    use nexus_knowledge::world_kb::knowledge_entry::{WorldKbBody, WorldKbEntry};
    // parse_pack is re-exported at module level from the parent `pack` module;
    // the explicit import below is a reminder of the path but resolves to the
    // same item.
    use nexus_spoke_adapter::pack::parse_pack;

    const OWNER: &str = "ctr_owner";
    const OWNER_NAME: &str = "Owner Name";
    const WORLD: &str = "wld_pack";
    const WORLD_TITLE: &str = "Pack World";

    /// Build a fresh migrated pool + seed a world owned by [`OWNER`] with two
    /// confirmed Knowledge entries and one relation between them. Returns the
    /// pool, the temp dir (kept alive for the test), and the entry/relation
    /// ids.
    async fn seeded_pool() -> (
        sqlx::SqlitePool,
        tempfile::TempDir,
        Vec<String>,
        Vec<String>,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("state.db");
        let pool = crate::db::Schema::init(&db_path).await.unwrap();

        // Seed creator with a human display_name.
        // SAFETY: test-only INSERT.
        sqlx::query(
            "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) \
             VALUES (?, ?, 'active', datetime('now'), '{}')",
        )
        .bind(OWNER)
        .bind(OWNER_NAME)
        .execute(&pool)
        .await
        .unwrap();

        nexus_local_db::kb_store::seed::world(
            &pool,
            WORLD,
            OWNER,
            WORLD_TITLE,
            "pack-world",
            "private",
            "manual",
        )
        .await;

        let store = SqliteKbStore::new(pool.clone());

        let mut entry_ids = Vec::new();
        for (i, name) in ["Alice", "Bob", "Carol"].iter().enumerate() {
            let mut kb = WorldKbEntry::new(WORLD, BlockType::Character, name);
            kb.body = Some(WorldKbBody {
                summary: Some(format!("{name} summary")),
                ..Default::default()
            });
            let res = store.insert_knowledge_entry(kb).await.unwrap();
            entry_ids.push(res.entry_id);
            // Stable ordering for deterministic relation target ids.
            let _ = i;
        }

        // Seed one relation Alice → Bob (both confirmed — must be exported).
        // SAFETY: test-only INSERT into kb_relationships.
        let rel_id = "rel_export_001".to_string();
        sqlx::query(
            "INSERT INTO kb_relationships \
                (relationship_id, world_id, source_entity_id, target_entity_id, \
                 relation_type, symmetric, confidence, source_anchor_ids, metadata, \
                 created_at, updated_at, revision, needs_review, source) \
             VALUES (?, ?, ?, ?, 'related_to', 0, NULL, '[]', '{}', \
                     datetime('now'), datetime('now'), 1, 0, 'manual')",
        )
        .bind(&rel_id)
        .bind(WORLD)
        .bind(&entry_ids[0])
        .bind(&entry_ids[1])
        .execute(&pool)
        .await
        .unwrap();

        // Note: FK constraint prevents creating a relation with a non-existent
        // target_entity_id — the database itself enforces both-endpoints integrity.
        // The both-endpoints-in-set filter is demonstrated by the single valid
        // relation above; cross-world exclusions would need a second world.

        let rel_ids = vec![rel_id];
        (pool, dir, entry_ids, rel_ids)
    }

    /// Build a `CliConfig` that points at the seeded active creator.
    fn config_with_active_creator() -> CliConfig {
        CliConfig {
            active_creator_id: Some(OWNER.to_string()),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn export_writes_valid_pack_with_expected_shape() {
        let (pool, _dir, _entry_ids, _rel_ids) = seeded_pool().await;
        let tmp_out = tempfile::NamedTempFile::new().unwrap();
        let out_path = tmp_out.path().to_path_buf();
        // NamedTempFile creates an empty file; remove it so export writes fresh.
        drop(tmp_out);

        let args = ExportArgs {
            world_ref: WORLD.to_string(),
            out: out_path.clone(),
            title: None,
            pack_version: DEFAULT_PACK_VERSION.to_string(),
            include_deprecated: false,
            include_anchors: false,
        };

        export(args, &config_with_active_creator(), &pool)
            .await
            .expect("export must succeed");

        let text = std::fs::read_to_string(&out_path).unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();

        // Handbook shape: top-level keys present.
        assert!(value.get("modules").is_some(), "modules key must exist");
        assert!(value.get("entries").is_some(), "entries key must exist");
        assert!(value.get("relations").is_some(), "relations key must exist");
        // Anchors omitted without --include-anchors.
        assert!(
            value.get("source_anchors").is_none(),
            "source_anchors must be omitted when flag is unset"
        );

        // modules.pack required metadata.
        let pack = value["modules"]["pack"]
            .as_object()
            .expect("modules.pack must be an object");
        assert_eq!(pack["title"], WORLD_TITLE);
        assert_eq!(pack["version"], DEFAULT_PACK_VERSION);
        assert_eq!(pack["creator"], OWNER_NAME);

        // parse_pack validates against the spoke handbook shape.
        let parsed = parse_pack(&value).expect("written pack must parse via parse_pack");
        assert_eq!(parsed.entries.len(), 3, "all 3 confirmed entries exported");
        assert_eq!(
            parsed.relations.len(),
            1,
            "only one relation (non-dangling) exported"
        );

        // Stable ordering: entries sorted by canonical_name ascending.
        let names: Vec<&str> = parsed
            .entries
            .iter()
            .map(|e| e.canonical_name.as_str())
            .collect();
        assert_eq!(names, vec!["Alice", "Bob", "Carol"]);

        // The surviving relation is Alice → Bob.
        assert_eq!(parsed.relations[0].relation_id, "rel_export_001");
    }

    #[tokio::test]
    async fn export_includes_anchors_key_when_flag_set() {
        let (pool, _dir, _entry_ids, _rel_ids) = seeded_pool().await;
        let tmp_out = tempfile::NamedTempFile::new().unwrap();
        let out_path = tmp_out.path().to_path_buf();
        drop(tmp_out);

        let args = ExportArgs {
            world_ref: WORLD.to_string(),
            out: out_path.clone(),
            title: None,
            pack_version: DEFAULT_PACK_VERSION.to_string(),
            include_deprecated: false,
            include_anchors: true,
        };

        export(args, &config_with_active_creator(), &pool)
            .await
            .expect("export must succeed");

        let text = std::fs::read_to_string(&out_path).unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        let anchors = value["source_anchors"]
            .as_array()
            .expect("source_anchors must be an array when --include-anchors is set");
        assert!(
            anchors.is_empty(),
            "anchors array is empty (no SourceAnchor store)"
        );
    }

    #[tokio::test]
    async fn export_surfaces_clean_error_when_world_missing() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("state.db");
        let pool = crate::db::Schema::init(&db_path).await.unwrap();

        let args = ExportArgs {
            world_ref: "wld_nonexistent".to_string(),
            out: dir.path().join("out.json"),
            title: None,
            pack_version: DEFAULT_PACK_VERSION.to_string(),
            include_deprecated: false,
            include_anchors: false,
        };

        let err = export(args, &config_with_active_creator(), &pool)
            .await
            .expect_err("export must fail for missing world");
        let msg = format!("{err}");
        assert!(
            msg.contains("not found"),
            "error must mention world not found; got: {msg}"
        );
    }

    #[tokio::test]
    async fn export_falls_back_to_nexus42_creator_when_no_active_creator() {
        let (pool, _dir, _entry_ids, _rel_ids) = seeded_pool().await;
        let tmp_out = tempfile::NamedTempFile::new().unwrap();
        let out_path = tmp_out.path().to_path_buf();
        drop(tmp_out);

        let args = ExportArgs {
            world_ref: WORLD.to_string(),
            out: out_path.clone(),
            title: None,
            pack_version: DEFAULT_PACK_VERSION.to_string(),
            include_deprecated: false,
            include_anchors: false,
        };

        // No active creator set.
        let config = CliConfig::default();
        export(args, &config, &pool)
            .await
            .expect("export must succeed");

        let text = std::fs::read_to_string(&out_path).unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["modules"]["pack"]["creator"], FALLBACK_CREATOR);
    }

    // ── Import helpers ──────────────────────────────────────────────────

    /// Build a seeded pool with no entries/relations (empty world for import).
    async fn empty_world_pool() -> (SqlitePool, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("state.db");
        let pool = crate::db::Schema::init(&db_path).await.unwrap();

        // Seed creator so that the world seed works.
        // SAFETY: test-only INSERT.
        sqlx::query(
            "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) \
             VALUES (?, ?, 'active', datetime('now'), '{}')",
        )
        .bind(OWNER)
        .bind(OWNER_NAME)
        .execute(&pool)
        .await
        .unwrap();

        nexus_local_db::kb_store::seed::world(
            &pool,
            WORLD,
            OWNER,
            WORLD_TITLE,
            "pack-world",
            "private",
            "manual",
        )
        .await;

        (pool, dir)
    }

    /// Export a seeded pool's entries to a pack JSON file. Returns the file
    /// path (the temp dir keeps it alive).
    async fn export_to_file(pool: &SqlitePool) -> (PathBuf, tempfile::TempDir) {
        let tmp_dir = tempfile::tempdir().unwrap();
        let out_path = tmp_dir.path().join("test_pack.json");

        let args = ExportArgs {
            world_ref: WORLD.to_string(),
            out: out_path.clone(),
            title: None,
            pack_version: DEFAULT_PACK_VERSION.to_string(),
            include_deprecated: false,
            include_anchors: false,
        };
        let config = config_with_active_creator();
        export(args, &config, pool)
            .await
            .expect("export must succeed for test fixture");

        (out_path, tmp_dir)
    }

    /// Count entries in a world via `list_by_world`.
    async fn count_entries(pool: &SqlitePool, world_id: &str) -> usize {
        let store = SqliteKbStore::new(pool.clone());
        store
            .list_by_world(world_id)
            .await
            .map(|v| v.len())
            .unwrap_or(0)
    }

    /// Count relations in a world via `list_relationships_for_world`.
    async fn count_relations(pool: &SqlitePool, world_id: &str) -> usize {
        list_relationships_for_world(pool, world_id, false, i64::MAX)
            .await
            .map(|v| v.len())
            .unwrap_or(0)
    }

    // ── Import tests ────────────────────────────────────────────────────

    #[tokio::test(flavor = "multi_thread")]
    async fn import_creates_entries_and_relations_from_pack() {
        // Phase 1: build a seeded world, export to pack.
        let (pool, _dir, _entry_ids, _rel_ids) = seeded_pool().await;
        assert_eq!(count_entries(&pool, WORLD).await, 3);
        assert_eq!(count_relations(&pool, WORLD).await, 1);

        let (pack_path, _pack_dir) = export_to_file(&pool).await;

        // Phase 2: new empty world (same WORLD id but fresh DB), import.
        let (pool2, _dir2) = empty_world_pool().await;
        assert_eq!(count_entries(&pool2, WORLD).await, 0);

        let args = ImportArgs {
            world_ref: WORLD.to_string(),
            r#in: pack_path,
            dry_run: false,
            conflict: ConflictStrategy::Skip,
        };
        import(args, &pool2).await.expect("import must succeed");

        assert_eq!(count_entries(&pool2, WORLD).await, 3);
        assert_eq!(count_relations(&pool2, WORLD).await, 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn import_idempotent_second_run_creates_zero() {
        let (pool, _dir, _entry_ids, _rel_ids) = seeded_pool().await;
        let (pack_path, _pack_dir) = export_to_file(&pool).await;

        // Import into fresh DB.
        let (pool2, _dir2) = empty_world_pool().await;
        let args = ImportArgs {
            world_ref: WORLD.to_string(),
            r#in: pack_path.clone(),
            dry_run: false,
            conflict: ConflictStrategy::Skip,
        };
        import(args, &pool2)
            .await
            .expect("first import must succeed");
        assert_eq!(count_entries(&pool2, WORLD).await, 3);
        assert_eq!(count_relations(&pool2, WORLD).await, 1);

        // Second import (idempotent).
        let args2 = ImportArgs {
            world_ref: WORLD.to_string(),
            r#in: pack_path,
            dry_run: false,
            conflict: ConflictStrategy::Skip,
        };
        import(args2, &pool2)
            .await
            .expect("second import must succeed");
        // Counts unchanged — all collisions skipped.
        assert_eq!(
            count_entries(&pool2, WORLD).await,
            3,
            "entry count unchanged on re-import"
        );
        assert_eq!(
            count_relations(&pool2, WORLD).await,
            1,
            "relation count unchanged on re-import"
        );
    }

    #[tokio::test]
    async fn import_dry_run_performs_zero_writes() {
        let (pool, _dir, _entry_ids, _rel_ids) = seeded_pool().await;
        let (pack_path, _pack_dir) = export_to_file(&pool).await;

        let (pool2, _dir2) = empty_world_pool().await;
        let pre_entries = count_entries(&pool2, WORLD).await;
        let pre_relations = count_relations(&pool2, WORLD).await;

        let args = ImportArgs {
            world_ref: WORLD.to_string(),
            r#in: pack_path,
            dry_run: true,
            conflict: ConflictStrategy::Skip,
        };
        import(args, &pool2)
            .await
            .expect("dry-run import must succeed");

        let post_entries = count_entries(&pool2, WORLD).await;
        let post_relations = count_relations(&pool2, WORLD).await;
        assert_eq!(post_entries, pre_entries, "dry-run must not create entries");
        assert_eq!(
            post_relations, pre_relations,
            "dry-run must not create relations"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn import_skips_on_canonical_name_collision() {
        // Seed a world with one entry, export a pack containing that entry
        // plus another, then import into a world that already has a
        // different entry_id but same canonical_name.
        let (pool, _dir, entry_ids, _rel_ids) = seeded_pool().await;
        let alice_id = &entry_ids[0]; // "Alice"
        let bob_id = &entry_ids[1]; // "Bob"
        let carol_id = &entry_ids[2]; // "Carol"
        let (pack_path, _pack_dir) = export_to_file(&pool).await;

        // New world: pre-create a "Carol" entry with a DIFFERENT entry_id
        // but same canonical_name. Then import the pack (which also has Carol
        // under the original entry_id).
        let (pool2, _dir2) = empty_world_pool().await;
        let store = SqliteKbStore::new(pool2.clone());
        let mut carol_clone = WorldKbEntry::new(WORLD, BlockType::Character, "Carol");
        carol_clone.body = Some(WorldKbBody {
            summary: Some("Cloned Carol".to_string()),
            ..Default::default()
        });
        let res = store
            .insert_knowledge_entry(carol_clone)
            .await
            .expect("pre-create Carol clone");
        let different_carol_id = res.entry_id;
        assert_ne!(different_carol_id, *carol_id);

        let args = ImportArgs {
            world_ref: WORLD.to_string(),
            r#in: pack_path,
            dry_run: false,
            conflict: ConflictStrategy::Skip,
        };
        import(args, &pool2).await.expect("import must succeed");

        // Should create 2 entries (Alice, Bob — not Carol because
        // canonical_name collision), and skip Carol's entry_id.
        assert_eq!(count_entries(&pool2, WORLD).await, 3); // clone Carol + Alice + Bob
                                                           // Carol's entry_id from the pack should NOT exist.
        assert!(
            store.get_knowledge_entry(carol_id).await.is_err(),
            "pack's Carol entry_id must not be imported"
        );
        // Alice and Bob's entry_ids SHOULD exist.
        assert!(
            store.get_knowledge_entry(alice_id).await.is_ok(),
            "pack's Alice must be imported"
        );
        assert!(
            store.get_knowledge_entry(bob_id).await.is_ok(),
            "pack's Bob must be imported"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn import_provenance_stamp_applied_on_created_entries() {
        let (pool, _dir, _entry_ids, _rel_ids) = seeded_pool().await;
        let (pack_path, _pack_dir) = export_to_file(&pool).await;

        let (pool2, _dir2) = empty_world_pool().await;
        let args = ImportArgs {
            world_ref: WORLD.to_string(),
            r#in: pack_path,
            dry_run: false,
            conflict: ConflictStrategy::Skip,
        };
        import(args, &pool2).await.expect("import must succeed");

        // Verify provenance on created entries.
        let store = SqliteKbStore::new(pool2.clone());
        let entries = store.list_by_world(WORLD).await.unwrap();
        assert!(!entries.is_empty(), "import must create entries");
        for entry in &entries {
            assert_eq!(
                entry.source_provenance_kind.as_deref(),
                Some(IMPORT_PROVENANCE),
                "imported entry {} must have source_provenance_kind = 'pack_import'",
                entry.entry_id
            );
        }
    }

    #[tokio::test]
    async fn import_conflict_rename_errors_not_implemented() {
        let (pool, _dir, _entry_ids, _rel_ids) = seeded_pool().await;
        let (pack_path, _pack_dir) = export_to_file(&pool).await;

        let (pool2, _dir2) = empty_world_pool().await;
        let args = ImportArgs {
            world_ref: WORLD.to_string(),
            r#in: pack_path,
            dry_run: false,
            conflict: ConflictStrategy::Rename,
        };
        let err = import(args, &pool2)
            .await
            .expect_err("rename conflict strategy must error");
        let msg = format!("{err}");
        assert!(
            msg.contains("not yet implemented"),
            "rename must emit 'not yet implemented'; got: {msg}"
        );
    }

    #[tokio::test]
    async fn import_conflict_overwrite_errors_not_implemented() {
        let (pool, _dir, _entry_ids, _rel_ids) = seeded_pool().await;
        let (pack_path, _pack_dir) = export_to_file(&pool).await;

        let (pool2, _dir2) = empty_world_pool().await;
        let args = ImportArgs {
            world_ref: WORLD.to_string(),
            r#in: pack_path,
            dry_run: false,
            conflict: ConflictStrategy::Overwrite,
        };
        let err = import(args, &pool2)
            .await
            .expect_err("overwrite conflict strategy must error");
        let msg = format!("{err}");
        assert!(
            msg.contains("not yet implemented"),
            "overwrite must emit 'not yet implemented'; got: {msg}"
        );
    }

    #[tokio::test]
    async fn import_surfaces_clean_error_when_world_missing() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("state.db");
        let pool = crate::db::Schema::init(&db_path).await.unwrap();

        // Create a minimal valid pack file.
        let pack_path = dir.path().join("empty_pack.json");
        let pack_json = serde_json::json!({
            "modules": { "pack": { "title": "Empty", "version": "0.1.0", "creator": "test" } },
            "entries": [],
            "relations": []
        });
        std::fs::write(
            &pack_path,
            serde_json::to_string_pretty(&pack_json).unwrap(),
        )
        .unwrap();

        let args = ImportArgs {
            world_ref: "wld_nonexistent".to_string(),
            r#in: pack_path,
            dry_run: false,
            conflict: ConflictStrategy::Skip,
        };
        let err = import(args, &pool)
            .await
            .expect_err("import must fail for missing world");
        let msg = format!("{err}");
        assert!(
            msg.contains("not found"),
            "error must mention world not found; got: {msg}"
        );
    }
}
