//! Portable Knowledge-pack I/O — `creator world kb pack export|import`.
//!
//! V1.146 P3 (plan `2026-07-30-v1.146-p3-knowledge-pack-io-cli`): this task
//! ships `export` only. `import` (additive, `skip` default conflict policy)
//! lands in T3.
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
use nexus_knowledge::world_kb::KbStore;
use nexus_local_db::kb_relationships::list_relationships_for_world;
use nexus_local_db::kb_store::SqliteKbStore;
use nexus_spoke_adapter::conversion::{kb_relationship_row_to_spoke, world_kb_to_spoke};
use nexus_spoke_adapter::pack::build_pack;
use sqlx::SqlitePool;
use std::collections::HashSet;
use std::path::PathBuf;

/// Default version string stamped into `modules.pack.version` when
/// `--pack-version` is not supplied.
const DEFAULT_PACK_VERSION: &str = "0.1.0";

/// Fallback author string stamped into `modules.pack.creator` when no active
/// Creator profile is resolvable (e.g. hermetic tests, ad-hoc workspaces).
const FALLBACK_CREATOR: &str = "nexus42";

/// `creator world kb pack` subcommands.
#[derive(Debug, Subcommand)]
pub enum PackCommand {
    /// Export one world's Knowledge entries and their relations to a portable
    /// Narrative Knowledge Pack JSON file.
    Export(ExportArgs),
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
    }
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
}
