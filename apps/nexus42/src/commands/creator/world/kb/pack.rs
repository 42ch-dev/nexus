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
use nexus_daemon_runtime::pack_import::{import_pack, ConflictPolicy, ImportOutcome};
use nexus_knowledge::world_kb::KbStore;
use nexus_local_db::kb_relationships::list_relationships_for_world;
use nexus_local_db::kb_store::SqliteKbStore;
use nexus_spoke_adapter::conversion::{kb_relationship_row_to_spoke, world_kb_to_spoke};
use nexus_spoke_adapter::pack::st_lorebook::{
    parse_st_lorebook, ConversionDiagnostic, DiagnosticSeverity, StLorebookError,
};
use nexus_spoke_adapter::pack::{build_pack, parse_pack};
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
        PackCommand::Import(args) => import(args, config, pool).await,
    }
}

/// Conflict-resolution policy for the import command.
///
/// Maps to [`ConflictPolicy`] in `nexus_daemon_runtime::pack_import`.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum ConflictStrategy {
    /// Skip existing entries/relations (default).
    Skip,
    /// Rename conflicting entries (disambiguate canonical name + fresh id).
    Rename,
    /// Overwrite existing entries (body upsert, lifecycle preserved).
    Overwrite,
}

/// Arguments for `creator world kb pack import`.
#[derive(Debug, clap::Args)]
pub struct ImportArgs {
    /// World reference — the world ID (e.g. `wld_abc123`).
    pub world_ref: String,

    /// Input path for the pack JSON file (required unless `--from-st`).
    #[arg(long, conflicts_with = "from_st", required_unless_present = "from_st")]
    pub r#in: Option<PathBuf>,

    /// Import a `SillyTavern` lorebook JSON file (documented format) instead
    /// of a pack — converted to a pack before the standard import path.
    #[arg(long, conflicts_with = "in", required_unless_present = "in")]
    pub from_st: Option<PathBuf>,

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
/// read or parsed, or any atom upsert/relate was rejected.
// The function is a linear CLI pipeline (owner gate → source read → convert →
// parse → import → report); splitting it would fragment the flow without
// reducing complexity.
#[allow(clippy::too_many_lines)]
async fn import(args: ImportArgs, config: &CliConfig, pool: &SqlitePool) -> Result<()> {
    let world_id = args.world_ref.as_str();

    let creator_id = super::super::active_creator_id(config)?;
    super::require_world_owner(pool, world_id, &creator_id).await?;

    // Source selection: pack JSON (`--in`) or SillyTavern lorebook
    // (`--from-st`). clap enforces exactly one. The ST converter runs before
    // `parse_pack`; its diagnostics are printed before the import summary
    // (also under `--dry-run`).
    let (value, diagnostics, source_display) = if let Some(st_path) = &args.from_st {
        let text = std::fs::read_to_string(st_path).map_err(|e| {
            CliError::Other(format!(
                "Failed to read ST lorebook file {}: {e}",
                st_path.display()
            ))
        })?;
        let json: serde_json::Value = serde_json::from_str(&text)
            .map_err(StLorebookError::NotJson)
            .map_err(|e| {
                CliError::Other(format!(
                    "Invalid ST lorebook format in {}: {e}",
                    st_path.display()
                ))
            })?;
        let outcome = parse_st_lorebook(&json).map_err(|e| {
            CliError::Other(format!(
                "Invalid ST lorebook format in {}: {e}",
                st_path.display()
            ))
        })?;
        (
            outcome.pack_input,
            outcome.diagnostics,
            st_path.display().to_string(),
        )
    } else {
        let pack_path = args
            .r#in
            .as_ref()
            .expect("clap enforces exactly one of --in / --from-st");
        let text = std::fs::read_to_string(pack_path).map_err(|e| {
            CliError::Other(format!(
                "Failed to read pack file {}: {e}",
                pack_path.display()
            ))
        })?;
        let value: serde_json::Value = serde_json::from_str(&text).map_err(|e| {
            CliError::Other(format!(
                "Invalid JSON in pack file {}: {e}",
                pack_path.display()
            ))
        })?;
        (value, Vec::new(), pack_path.display().to_string())
    };

    if !diagnostics.is_empty() {
        print!("{}", render_st_diagnostics(&diagnostics));
    }

    let parsed = parse_pack(&value)
        .map_err(|e| CliError::Other(format!("Invalid pack format in {source_display}: {e}")))?;

    let conflict = match args.conflict {
        ConflictStrategy::Skip => ConflictPolicy::Skip,
        ConflictStrategy::Rename => ConflictPolicy::Rename,
        ConflictStrategy::Overwrite => ConflictPolicy::Overwrite,
    };

    let summary = import_pack(
        pool,
        world_id,
        &creator_id,
        parsed,
        conflict,
        false,
        args.dry_run,
    )
    .await
    .map_err(|e| CliError::Other(e.to_string()))?;

    for detail in &summary.details {
        if detail.outcome == ImportOutcome::Rejected {
            if let Some(reason) = &detail.reason {
                eprintln!("  warn: {:?} {} rejected: {reason}", detail.kind, detail.id);
            }
        } else if args.dry_run {
            if let Some(reason) = &detail.reason {
                eprintln!("  [dry-run] {:?} {}: {reason}", detail.kind, detail.id);
            }
        }
    }

    let e = &summary.entries;
    let r = &summary.relations;
    let created = e.created + r.created;
    let skipped = e.skipped + r.skipped;
    let rejected = e.rejected + r.rejected;
    let renamed = e.renamed + r.renamed;
    let overwritten = e.overwritten + r.overwritten;

    if args.dry_run {
        println!(
            "[dry-run] would create: {created}, would skip: {skipped}, would rename: {renamed}, would overwrite: {overwritten}"
        );
    } else {
        println!(
            "created: {created}, skipped: {skipped}, renamed: {renamed}, overwritten: {overwritten}"
        );
    }

    if rejected > 0 {
        return Err(CliError::Other(format!(
            "Import completed with {rejected} rejected atom(s) (created: {created}, skipped: {skipped}). Check warnings above for rejection details."
        )));
    }

    Ok(())
}

/// Render the ST lorebook conversion diagnostics summary (printed before the
/// import summary, also under `--dry-run`).
fn render_st_diagnostics(diagnostics: &[ConversionDiagnostic]) -> String {
    use std::fmt::Write as _;
    let mut out = String::from("ST lorebook conversion notes:\n");
    for d in diagnostics {
        let severity = match d.severity {
            DiagnosticSeverity::Warning => "warning",
            DiagnosticSeverity::Info => "info",
        };
        let location = match (&d.entry_name, d.entry_index) {
            (Some(name), Some(idx)) => format!("entry {idx} '{name}'"),
            (Some(name), None) => format!("entry '{name}'"),
            (None, Some(idx)) => format!("entry {idx}"),
            (None, None) => "lorebook".to_string(),
        };
        match &d.field {
            Some(field) => {
                let _ = writeln!(
                    out,
                    "  {severity}: {location} field '{field}': {}",
                    d.message
                );
            }
            None => {
                let _ = writeln!(out, "  {severity}: {location}: {}", d.message);
            }
        }
    }
    out
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
    use nexus_daemon_runtime::pack_import::IMPORT_PROVENANCE;
    use nexus_knowledge::world_kb::knowledge_entry::{WorldKbBody, WorldKbEntry};
    // parse_pack is re-exported at module level from the parent `pack` module;
    // the explicit import below is a reminder of the path but resolves to the
    // same item.
    use nexus_spoke_adapter::pack::parse_pack;
    use serde_json::json;

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
        store.list_by_world(world_id).await.map_or(0, |v| v.len())
    }

    /// Count relations in a world via `list_relationships_for_world`.
    async fn count_relations(pool: &SqlitePool, world_id: &str) -> usize {
        list_relationships_for_world(pool, world_id, false, i64::MAX)
            .await
            .map_or(0, |v| v.len())
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
            r#in: Some(pack_path),
            from_st: None,
            dry_run: false,
            conflict: ConflictStrategy::Skip,
        };
        import(args, &config_with_active_creator(), &pool2)
            .await
            .expect("import must succeed");

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
            r#in: Some(pack_path.clone()),
            from_st: None,
            dry_run: false,
            conflict: ConflictStrategy::Skip,
        };
        import(args, &config_with_active_creator(), &pool2)
            .await
            .expect("first import must succeed");
        assert_eq!(count_entries(&pool2, WORLD).await, 3);
        assert_eq!(count_relations(&pool2, WORLD).await, 1);

        // Second import (idempotent).
        let args2 = ImportArgs {
            world_ref: WORLD.to_string(),
            r#in: Some(pack_path),
            from_st: None,
            dry_run: false,
            conflict: ConflictStrategy::Skip,
        };
        import(args2, &config_with_active_creator(), &pool2)
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
            r#in: Some(pack_path),
            from_st: None,
            dry_run: true,
            conflict: ConflictStrategy::Skip,
        };
        import(args, &config_with_active_creator(), &pool2)
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
            r#in: Some(pack_path),
            from_st: None,
            dry_run: false,
            conflict: ConflictStrategy::Skip,
        };
        import(args, &config_with_active_creator(), &pool2)
            .await
            .expect("import must succeed");

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
            r#in: Some(pack_path),
            from_st: None,
            dry_run: false,
            conflict: ConflictStrategy::Skip,
        };
        import(args, &config_with_active_creator(), &pool2)
            .await
            .expect("import must succeed");

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

    #[tokio::test(flavor = "multi_thread")]
    async fn import_conflict_rename_creates_disambiguated_entry() {
        let (pool, _dir, entry_ids, _rel_ids) = seeded_pool().await;
        let (pack_path, _pack_dir) = export_to_file(&pool).await;
        let carol_pack_id = &entry_ids[2];
        let alice_pack_id = &entry_ids[0];

        let mut pack_value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&pack_path).unwrap()).unwrap();
        let template = pack_value["relations"][0].clone();
        let mut carol_rel = template;
        carol_rel["relation_id"] = json!("rel_carol_alice_rename");
        carol_rel["from_id"] = json!(carol_pack_id);
        carol_rel["to_id"] = json!(alice_pack_id);
        pack_value["relations"]
            .as_array_mut()
            .unwrap()
            .push(carol_rel);
        std::fs::write(
            &pack_path,
            serde_json::to_string_pretty(&pack_value).unwrap(),
        )
        .unwrap();

        let (pool2, _dir2) = empty_world_pool().await;
        let store = SqliteKbStore::new(pool2.clone());
        let mut carol_clone = WorldKbEntry::new(WORLD, BlockType::Character, "Carol");
        carol_clone.body = Some(WorldKbBody {
            summary: Some("Pre-existing Carol".to_string()),
            ..Default::default()
        });
        store
            .insert_knowledge_entry(carol_clone)
            .await
            .expect("pre-create Carol");

        let args = ImportArgs {
            world_ref: WORLD.to_string(),
            r#in: Some(pack_path),
            from_st: None,
            dry_run: false,
            conflict: ConflictStrategy::Rename,
        };
        import(args, &config_with_active_creator(), &pool2)
            .await
            .expect("rename import must succeed");

        let entries = store.list_by_world(WORLD).await.unwrap();
        assert_eq!(
            entries.len(),
            4,
            "pre-existing Carol + Alice + Bob + renamed Carol"
        );
        let renamed = entries
            .iter()
            .find(|e| {
                e.canonical_name.ends_with(" imported") || e.canonical_name.contains(" imported ")
            })
            .expect("rename policy must create a disambiguated entry with ' imported' suffix");
        let renamed_carol_id = renamed.entry_id.clone();

        let relations = list_relationships_for_world(&pool2, WORLD, false, i64::MAX)
            .await
            .unwrap();
        let carol_rel = relations
            .iter()
            .find(|r| r.relationship_id == "rel_carol_alice_rename")
            .expect("Carol→Alice relation must import");
        assert_eq!(carol_rel.source_entity_id, renamed_carol_id);
        assert_eq!(carol_rel.target_entity_id, *alice_pack_id);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn import_conflict_overwrite_replaces_body_preserves_status() {
        let (pool, _dir, _entry_ids, _rel_ids) = seeded_pool().await;
        let (pack_path, _pack_dir) = export_to_file(&pool).await;

        // Pre-create Carol with a distinct body and non-default status.
        let (pool2, _dir2) = empty_world_pool().await;
        let store = SqliteKbStore::new(pool2.clone());
        let mut carol_clone = WorldKbEntry::new(WORLD, BlockType::Character, "Carol");
        carol_clone.status = "confirmed".to_string();
        carol_clone.body = Some(WorldKbBody {
            summary: Some("Pre-existing Carol body".to_string()),
            ..Default::default()
        });
        let res = store
            .insert_knowledge_entry(carol_clone)
            .await
            .expect("pre-create Carol");
        let preexisting_carol_id = res.entry_id;

        let args = ImportArgs {
            world_ref: WORLD.to_string(),
            r#in: Some(pack_path),
            from_st: None,
            dry_run: false,
            conflict: ConflictStrategy::Overwrite,
        };
        import(args, &config_with_active_creator(), &pool2)
            .await
            .expect("overwrite import must succeed");

        assert_eq!(
            count_entries(&pool2, WORLD).await,
            3,
            "overwrite must not add a second Carol row"
        );

        let carol = store
            .get_knowledge_entry(&preexisting_carol_id)
            .await
            .expect("pre-existing Carol must remain");
        assert_eq!(
            carol.status, "confirmed",
            "overwrite must preserve existing entry status"
        );
        assert_eq!(
            carol.body.as_ref().and_then(|b| b.summary.as_deref()),
            Some("Carol summary"),
            "overwrite must replace body with pack content"
        );
    }

    /// Greptile P1 / PR #200: same-world export → re-import must honor overwrite
    /// (not unconditionally skip on `entry_id` PK collision).
    #[tokio::test(flavor = "multi_thread")]
    async fn import_same_world_reimport_overwrite_updates_body() {
        let (pool, _dir, entry_ids, _rel_ids) = seeded_pool().await;
        let carol_id = &entry_ids[2];
        let (pack_path, _pack_dir) = export_to_file(&pool).await;

        // Mutate Carol in-place so re-import has something to overwrite.
        let store = SqliteKbStore::new(pool.clone());
        let mut carol = store
            .get_knowledge_entry(carol_id)
            .await
            .expect("Carol must exist");
        carol.body = Some(WorldKbBody {
            summary: Some("Stale Carol body".to_string()),
            ..Default::default()
        });
        store
            .update_knowledge_entry(carol)
            .await
            .expect("update Carol body");

        let args = ImportArgs {
            world_ref: WORLD.to_string(),
            r#in: Some(pack_path),
            from_st: None,
            dry_run: false,
            conflict: ConflictStrategy::Overwrite,
        };
        import(args, &config_with_active_creator(), &pool)
            .await
            .expect("same-world overwrite re-import must succeed");

        assert_eq!(
            count_entries(&pool, WORLD).await,
            3,
            "overwrite re-import must not add duplicate rows"
        );

        let carol = store
            .get_knowledge_entry(carol_id)
            .await
            .expect("Carol must remain");
        assert_eq!(
            carol.body.as_ref().and_then(|b| b.summary.as_deref()),
            Some("Carol summary"),
            "overwrite must replace body with pack content on same-world re-import"
        );
    }

    /// Greptile P1 / PR #200: same-world export → re-import under rename must
    /// mint disambiguated copies instead of skipping on `entry_id` collision.
    #[tokio::test(flavor = "multi_thread")]
    async fn import_same_world_reimport_rename_creates_disambiguated_entries() {
        let (pool, _dir, _entry_ids, _rel_ids) = seeded_pool().await;
        let (pack_path, _pack_dir) = export_to_file(&pool).await;

        let args = ImportArgs {
            world_ref: WORLD.to_string(),
            r#in: Some(pack_path),
            from_st: None,
            dry_run: false,
            conflict: ConflictStrategy::Rename,
        };
        import(args, &config_with_active_creator(), &pool)
            .await
            .expect("same-world rename re-import must succeed");

        assert_eq!(
            count_entries(&pool, WORLD).await,
            6,
            "rename re-import must duplicate all three entries"
        );

        let store = SqliteKbStore::new(pool.clone());
        let entries = store.list_by_world(WORLD).await.unwrap();
        let imported_suffix = entries
            .iter()
            .filter(|e| e.canonical_name.contains(" imported"))
            .count();
        assert_eq!(
            imported_suffix, 3,
            "each pack entry must be renamed on re-import"
        );
    }

    /// Greptile P1 / PR #193: global `entry_id` collision with a foreign-world row
    /// must not admit that id into `target_entry_ids` (no cross-world edges).
    #[tokio::test(flavor = "multi_thread")]
    async fn import_skips_foreign_world_entry_id_collision_endpoints() {
        const WORLD_B: &str = "wld_pack_b";
        const WORLD_B_TITLE: &str = "Pack World B";

        // World A seeded with 3 entries + 1 relation; export its pack.
        let (pool, _dir_a, entry_ids_a, _rel_ids_a) = seeded_pool().await;
        let (pack_path, _pack_dir) = export_to_file(&pool).await;

        // Second world in the *same* DB — entry_ids from World A already exist globally.
        nexus_local_db::kb_store::seed::world(
            &pool,
            WORLD_B,
            OWNER,
            WORLD_B_TITLE,
            "pack-world-b",
            "private",
            "manual",
        )
        .await;

        let args = ImportArgs {
            world_ref: WORLD_B.to_string(),
            r#in: Some(pack_path),
            from_st: None,
            dry_run: false,
            conflict: ConflictStrategy::Skip,
        };
        import(args, &config_with_active_creator(), &pool)
            .await
            .expect("import into World B must succeed");

        // Entries skipped (foreign-world PK collision) — none created in B.
        assert_eq!(count_entries(&pool, WORLD_B).await, 0);
        // Relations must not be inserted: endpoints were never admitted for B.
        assert_eq!(count_relations(&pool, WORLD_B).await, 0);
        // World A unchanged.
        assert_eq!(count_entries(&pool, WORLD).await, 3);
        assert_eq!(count_relations(&pool, WORLD).await, 1);
        let _ = entry_ids_a;
    }

    /// Greptile P1 follow-up: foreign `entry_id` + target-world canonical-name
    /// match must still remap pack id → target id so relations import.
    #[tokio::test(flavor = "multi_thread")]
    async fn import_foreign_entry_id_remaps_via_canonical_name() {
        const WORLD_B: &str = "wld_pack_b2";
        const WORLD_B_TITLE: &str = "Pack World B2";

        let (pool, _dir_a, entry_ids_a, _rel_ids_a) = seeded_pool().await;
        let (pack_path, _pack_dir) = export_to_file(&pool).await;

        nexus_local_db::kb_store::seed::world(
            &pool,
            WORLD_B,
            OWNER,
            WORLD_B_TITLE,
            "pack-world-b2",
            "private",
            "manual",
        )
        .await;

        // Pre-create Alice/Bob/Carol in B under *new* entry_ids (same names).
        let store = SqliteKbStore::new(pool.clone());
        let mut b_ids = Vec::new();
        for name in ["Alice", "Bob", "Carol"] {
            let mut kb = WorldKbEntry::new(WORLD_B, BlockType::Character, name);
            kb.body = Some(WorldKbBody {
                summary: Some(format!("{name} in B")),
                ..Default::default()
            });
            let res = store.insert_knowledge_entry(kb).await.unwrap();
            b_ids.push(res.entry_id);
        }

        let args = ImportArgs {
            world_ref: WORLD_B.to_string(),
            r#in: Some(pack_path),
            from_st: None,
            dry_run: false,
            conflict: ConflictStrategy::Skip,
        };
        import(args, &config_with_active_creator(), &pool)
            .await
            .expect("import must succeed with remap");

        // No new entries (all name-collided); relation Alice→Bob from pack
        // should land pointing at B's Alice/Bob ids.
        assert_eq!(count_entries(&pool, WORLD_B).await, 3);
        assert_eq!(
            count_relations(&pool, WORLD_B).await,
            1,
            "relation must import via canonical-name remap despite foreign pack entry_ids"
        );

        // SAFETY: test-only SELECT of relation endpoints.
        let row: (String, String) = sqlx::query_as(
            "SELECT source_entity_id, target_entity_id FROM kb_relationships \
             WHERE world_id = ? LIMIT 1",
        )
        .bind(WORLD_B)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.0, b_ids[0], "source remapped to B Alice");
        assert_eq!(row.1, b_ids[1], "target remapped to B Bob");
        // Pack's World A entry_ids must not appear as endpoints in B.
        assert_ne!(row.0, entry_ids_a[0]);
        assert_ne!(row.1, entry_ids_a[1]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn pack_export_import_cross_world_complementarity() {
        const WORLD_B: &str = "wld_pack_b";
        const WORLD_B_TITLE: &str = "Pack World B";
        // ── Phase 1: Seed World A with 3 entries + 1 relation ─────────
        let (pool_a, _dir_a, _entry_ids_a, _rel_ids_a) = seeded_pool().await;
        assert_eq!(count_entries(&pool_a, WORLD).await, 3);
        assert_eq!(count_relations(&pool_a, WORLD).await, 1);

        // Export World A → pack file.
        let (pack_path, _pack_dir) = export_to_file(&pool_a).await;

        // ── Phase 2: Create fresh DB with World B (different world_id) ─
        let dir_b = tempfile::tempdir().unwrap();
        let db_path_b = dir_b.path().join("state.db");
        let pool_b = crate::db::Schema::init(&db_path_b).await.unwrap();

        // Seed creator for FK satisfaction.
        // SAFETY: test-only INSERT.
        sqlx::query(
            "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) \
             VALUES (?, ?, 'active', datetime('now'), '{}')",
        )
        .bind(OWNER)
        .bind(OWNER_NAME)
        .execute(&pool_b)
        .await
        .unwrap();

        nexus_local_db::kb_store::seed::world(
            &pool_b,
            WORLD_B,
            OWNER,
            WORLD_B_TITLE,
            "pack-world-b",
            "private",
            "manual",
        )
        .await;

        assert_eq!(
            count_entries(&pool_b, WORLD_B).await,
            0,
            "World B starts empty"
        );
        assert_eq!(
            count_relations(&pool_b, WORLD_B).await,
            0,
            "World B starts with zero relations"
        );

        // ── Phase 3: Import pack into World B ─────────────────────────
        let args = ImportArgs {
            world_ref: WORLD_B.to_string(),
            r#in: Some(pack_path.clone()),
            from_st: None,
            dry_run: false,
            conflict: ConflictStrategy::Skip,
        };
        import(args, &config_with_active_creator(), &pool_b)
            .await
            .expect("import into World B must succeed");

        // ── Phase 4: Assert entries and relation present ──────────────
        assert_eq!(
            count_entries(&pool_b, WORLD_B).await,
            3,
            "World B must have all 3 imported entries"
        );
        assert_eq!(
            count_relations(&pool_b, WORLD_B).await,
            1,
            "World B must have the imported relation"
        );

        // Verify entries by entry_id from the pack (ids preserved across worlds).
        let store_b = SqliteKbStore::new(pool_b.clone());
        let entries_b = store_b.list_by_world(WORLD_B).await.unwrap();
        let imported_names: Vec<&str> = entries_b
            .iter()
            .map(|e| e.canonical_name.as_str())
            .collect();
        assert_eq!(
            imported_names,
            vec!["Alice", "Bob", "Carol"],
            "imported entries must have expected canonical names"
        );

        // ── Phase 5: Assert provenance on imported entries ────────────
        for entry in &entries_b {
            assert_eq!(
                entry.source_provenance_kind.as_deref(),
                Some(IMPORT_PROVENANCE),
                "imported entry {} must have source_provenance_kind = 'pack_import'",
                entry.entry_id
            );
        }

        // ── Phase 6: Re-import → idempotent (created: 0, all skipped) ─
        let args2 = ImportArgs {
            world_ref: WORLD_B.to_string(),
            r#in: Some(pack_path),
            from_st: None,
            dry_run: false,
            conflict: ConflictStrategy::Skip,
        };
        import(args2, &config_with_active_creator(), &pool_b)
            .await
            .expect("re-import must succeed");

        assert_eq!(
            count_entries(&pool_b, WORLD_B).await,
            3,
            "entry count unchanged on re-import (idempotent)"
        );
        assert_eq!(
            count_relations(&pool_b, WORLD_B).await,
            1,
            "relation count unchanged on re-import (idempotent)"
        );
    }

    /// V1.152 P2 dogfood: export→import round-trip on activation-carrying entries +
    /// relations preserves provenance, `modules.activation`, and skip-idempotency.
    // Long integration test; splitting would obscure the end-to-end scenario.
    #[allow(clippy::too_many_lines)]
    #[tokio::test(flavor = "multi_thread")]
    async fn dogfood_pack_round_trip_preserves_activation_and_relations() {
        const WORLD_A: &str = "wld_dogfood_a";
        const WORLD_A_TITLE: &str = "Dogfood World A";
        const WORLD_B: &str = "wld_dogfood_b";
        const WORLD_B_TITLE: &str = "Dogfood World B";

        let dir_a = tempfile::tempdir().unwrap();
        let pool_a = crate::db::Schema::init(&dir_a.path().join("state.db"))
            .await
            .unwrap();

        // SAFETY: test-only INSERT.
        sqlx::query(
            "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) \
             VALUES (?, ?, 'active', datetime('now'), '{}')",
        )
        .bind(OWNER)
        .bind(OWNER_NAME)
        .execute(&pool_a)
        .await
        .unwrap();

        nexus_local_db::kb_store::seed::world(
            &pool_a,
            WORLD_A,
            OWNER,
            WORLD_A_TITLE,
            "dogfood-a",
            "private",
            "manual",
        )
        .await;

        let store_a = SqliteKbStore::new(pool_a.clone());
        let mut entry_ids = Vec::new();
        for (name, key) in [("Alice", "alice"), ("Bob", "bob"), ("Carol", "carol")] {
            let mut kb = WorldKbEntry::new(WORLD_A, BlockType::Character, name);
            kb.body = Some(WorldKbBody {
                summary: Some(format!("{name} summary")),
                ..Default::default()
            });
            kb.modules = Some(json!({
                "activation": {"key": [key], "logic": "and_any"}
            }));
            let res = store_a.insert_knowledge_entry(kb).await.unwrap();
            entry_ids.push(res.entry_id);
        }

        let rel_id = "rel_dogfood_001".to_string();
        sqlx::query(
            "INSERT INTO kb_relationships \
                (relationship_id, world_id, source_entity_id, target_entity_id, \
                 relation_type, symmetric, confidence, source_anchor_ids, metadata, \
                 created_at, updated_at, revision, needs_review, source) \
             VALUES (?, ?, ?, ?, 'related_to', 0, NULL, '[]', '{}', \
                     datetime('now'), datetime('now'), 1, 0, 'manual')",
        )
        .bind(&rel_id)
        .bind(WORLD_A)
        .bind(&entry_ids[0])
        .bind(&entry_ids[1])
        .execute(&pool_a)
        .await
        .unwrap();

        let (pack_path, _pack_dir) = export_to_file_custom_world(&pool_a, WORLD_A).await;
        let pack_value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&pack_path).unwrap()).unwrap();
        let parsed = parse_pack(&pack_value).expect("exported pack must parse");

        let dir_b = tempfile::tempdir().unwrap();
        let pool_b = crate::db::Schema::init(&dir_b.path().join("state.db"))
            .await
            .unwrap();

        sqlx::query(
            "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) \
             VALUES (?, ?, 'active', datetime('now'), '{}')",
        )
        .bind(OWNER)
        .bind(OWNER_NAME)
        .execute(&pool_b)
        .await
        .unwrap();

        nexus_local_db::kb_store::seed::world(
            &pool_b,
            WORLD_B,
            OWNER,
            WORLD_B_TITLE,
            "dogfood-b",
            "private",
            "manual",
        )
        .await;

        let summary = import_pack(
            &pool_b,
            WORLD_B,
            OWNER,
            parsed.clone(),
            ConflictPolicy::Skip,
            false,
            false,
        )
        .await
        .expect("first import must succeed");

        assert_eq!(
            summary.entries.created, 3,
            "first import must create all seeded entries"
        );
        assert!(
            summary.relations.created >= 1,
            "first import must create at least one relation"
        );

        let store_b = SqliteKbStore::new(pool_b.clone());
        let entries_b = store_b.list_by_world(WORLD_B).await.unwrap();
        assert_eq!(entries_b.len(), 3);
        for entry in &entries_b {
            assert_eq!(
                entry.source_provenance_kind.as_deref(),
                Some(IMPORT_PROVENANCE),
                "imported entry {} must carry pack_import provenance",
                entry.entry_id
            );
        }

        let entries_a = store_a.list_by_world(WORLD_A).await.unwrap();
        for name in ["Alice", "Bob", "Carol"] {
            let a_modules = entries_a
                .iter()
                .find(|e| e.canonical_name == name)
                .expect("World A entry")
                .modules
                .clone();
            let b_modules = entries_b
                .iter()
                .find(|e| e.canonical_name == name)
                .expect("World B entry")
                .modules
                .clone();
            assert_eq!(
                a_modules, b_modules,
                "modules.activation must deep-equal A→B for {name}"
            );
        }

        assert_eq!(count_relations(&pool_b, WORLD_B).await, 1);

        let summary2 = import_pack(
            &pool_b,
            WORLD_B,
            OWNER,
            parsed,
            ConflictPolicy::Skip,
            false,
            false,
        )
        .await
        .expect("re-import must succeed");

        assert_eq!(
            summary2.entries.created, 0,
            "skip re-import must not create entries"
        );
        assert_eq!(
            summary2.relations.created, 0,
            "skip re-import must not create relations"
        );
        assert_eq!(count_entries(&pool_b, WORLD_B).await, 3);
        assert_eq!(count_relations(&pool_b, WORLD_B).await, 1);
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
            r#in: Some(pack_path),
            from_st: None,
            dry_run: false,
            conflict: ConflictStrategy::Skip,
        };
        let err = import(args, &config_with_active_creator(), &pool)
            .await
            .expect_err("import must fail for missing world");
        let msg = format!("{err}");
        assert!(
            msg.contains("not found"),
            "error must mention world not found; got: {msg}"
        );
    }

    // ── F-001: revision clearance on create ────────────────────────────

    /// Seed a pool where one entry has `revision >= 1` (simulates an entry
    /// created/updated through the spoke adapter path), export, then import
    /// into a fresh world. The import must clear `revision` to `None` before
    /// create so the spoke `validate_create_revision` gate passes.
    #[tokio::test(flavor = "multi_thread")]
    async fn import_create_clears_entry_revision() {
        let (pool, _dir, _entry_ids, _rel_ids) = seeded_pool().await;

        // SAFETY: test-only UPDATE to bump revision on one entry.
        let store = SqliteKbStore::new(pool.clone());
        let entries = store.list_by_world(WORLD).await.unwrap();
        let alice = entries
            .iter()
            .find(|e| e.canonical_name == "Alice")
            .unwrap();
        sqlx::query("UPDATE kb_key_blocks SET revision = 3 WHERE key_block_id = ?")
            .bind(&alice.entry_id)
            .execute(&pool)
            .await
            .unwrap();

        let (pack_path, _pack_dir) = export_to_file(&pool).await;

        // Import into fresh world — revision must be cleared.
        let (pool2, _dir2) = empty_world_pool().await;
        let args = ImportArgs {
            world_ref: WORLD.to_string(),
            r#in: Some(pack_path),
            from_st: None,
            dry_run: false,
            conflict: ConflictStrategy::Skip,
        };
        import(args, &config_with_active_creator(), &pool2)
            .await
            .expect("import must succeed despite revision >= 1");

        // All 3 entries should be created.
        assert_eq!(count_entries(&pool2, WORLD).await, 3);

        // Imported entries must carry pack_import provenance.
        let store2 = SqliteKbStore::new(pool2.clone());
        let imported = store2.list_by_world(WORLD).await.unwrap();
        for entry in &imported {
            assert_eq!(
                entry.source_provenance_kind.as_deref(),
                Some(IMPORT_PROVENANCE),
                "imported entry {} must have provenance 'pack_import'",
                entry.entry_id
            );
        }
    }

    // ── F-002: canonical-name collision remap ──────────────────────────

    /// Pre-create Carol under a different `entry_id` in the target world,
    /// import a pack that has Alice→Carol relation. After import, the
    /// relation must point at the **existing** Carol id (remap), not the
    /// pack's Carol id.
    #[tokio::test(flavor = "multi_thread")]
    async fn import_remaps_relation_endpoints_on_name_collision() {
        // Build a pool that has Carol→Alice relation (instead of Alice→Bob).
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("state.db");
        let pool = crate::db::Schema::init(&db_path).await.unwrap();

        // Seed creator + world.
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
        for name in ["Alice", "Bob", "Carol"] {
            let mut kb = WorldKbEntry::new(WORLD, BlockType::Character, name);
            kb.body = Some(WorldKbBody {
                summary: Some(format!("{name} summary")),
                ..Default::default()
            });
            let res = store.insert_knowledge_entry(kb).await.unwrap();
            entry_ids.push(res.entry_id);
        }
        let alice_id = &entry_ids[0];
        let carol_id = &entry_ids[2];

        // Carol→Alice relation (instead of Alice→Bob).
        let rel_id = "rel_carol_to_alice".to_string();
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
        .bind(carol_id)
        .bind(alice_id)
        .execute(&pool)
        .await
        .unwrap();

        let (pack_path, _pack_dir) = export_to_file(&pool).await;

        // ── Target world: pre-create Carol with a different id ─────────
        let (pool2, _dir2) = empty_world_pool().await;
        let store2 = SqliteKbStore::new(pool2.clone());
        let mut carol_clone = WorldKbEntry::new(WORLD, BlockType::Character, "Carol");
        carol_clone.body = Some(WorldKbBody {
            summary: Some("Pre-existing Carol".to_string()),
            ..Default::default()
        });
        let res = store2.insert_knowledge_entry(carol_clone).await.unwrap();
        let preexisting_carol_id = res.entry_id;
        assert_ne!(
            preexisting_carol_id, *carol_id,
            "pre-created Carol must have different id than pack Carol"
        );

        // ── Import ─────────────────────────────────────────────────────
        let args = ImportArgs {
            world_ref: WORLD.to_string(),
            r#in: Some(pack_path),
            from_st: None,
            dry_run: false,
            conflict: ConflictStrategy::Skip,
        };
        import(args, &config_with_active_creator(), &pool2)
            .await
            .expect("import must succeed");

        // Alice and Bob created (2 new), Carol skipped (name collision) → 3 total.
        assert_eq!(count_entries(&pool2, WORLD).await, 3);

        // Carol→Alice relation must exist, with from_id = pre-existing Carol id.
        let relations = list_relationships_for_world(&pool2, WORLD, false, i64::MAX)
            .await
            .unwrap();
        assert_eq!(relations.len(), 1, "exactly one relation imported");
        let rel = &relations[0];
        assert_eq!(rel.relationship_id, rel_id);
        assert_eq!(
            rel.source_entity_id, preexisting_carol_id,
            "relation from_id must be pre-existing Carol id (remapped)"
        );
        assert_eq!(
            rel.target_entity_id, *alice_id,
            "relation to_id must be pack Alice id (unchanged)"
        );
    }

    // ── F-006: CLI error-path tests ────────────────────────────────────

    #[tokio::test]
    async fn import_errors_on_missing_file() {
        let (pool, _dir) = empty_world_pool().await;
        let args = ImportArgs {
            world_ref: WORLD.to_string(),
            r#in: Some(PathBuf::from("/nonexistent/pack.json")),
            from_st: None,
            dry_run: false,
            conflict: ConflictStrategy::Skip,
        };
        let err = import(args, &config_with_active_creator(), &pool)
            .await
            .expect_err("import must fail for missing file");
        let msg = format!("{err}");
        assert!(
            msg.contains("Failed to read pack file"),
            "error must mention read failure; got: {msg}"
        );
    }

    #[tokio::test]
    async fn import_errors_on_invalid_json() {
        let (pool, _dir) = empty_world_pool().await;
        let dir = tempfile::tempdir().unwrap();
        let pack_path = dir.path().join("bad.json");
        std::fs::write(&pack_path, "not json at all").unwrap();

        let args = ImportArgs {
            world_ref: WORLD.to_string(),
            r#in: Some(pack_path),
            from_st: None,
            dry_run: false,
            conflict: ConflictStrategy::Skip,
        };
        let err = import(args, &config_with_active_creator(), &pool)
            .await
            .expect_err("import must fail for invalid JSON");
        let msg = format!("{err}");
        assert!(
            msg.contains("Invalid JSON"),
            "error must mention Invalid JSON; got: {msg}"
        );
    }

    // ── V1.146 P4 T5: modules activation round-trip ────────────────────

    /// Modules (activation) survive pack export → import round-trip, and
    /// `apply_activation` correctly classifies imported entries by their
    /// carried `modules.activation` fire-conditions.
    ///
    /// Proves:
    /// 1. `modules_json` survives the full pack I/O path (closes R-V1146P3-001).
    /// 2. The activation engine works on entries that arrived via pack import.
    // Long integration test; splitting would obscure the end-to-end scenario.
    #[allow(clippy::too_many_lines)]
    #[tokio::test(flavor = "multi_thread")]
    async fn pack_io_modules_preserved_and_activation_round_trip() {
        use nexus_spoke_adapter::adapter::activation;
        const WORLD_A: &str = "wld_activation_a";
        const WORLD_B: &str = "wld_activation_b";

        // ── Phase 1: Seed world A with entries carrying modules ─────────
        let dir_a = tempfile::tempdir().unwrap();
        let db_path_a = dir_a.path().join("state.db");
        let pool_a = crate::db::Schema::init(&db_path_a).await.unwrap();

        // Reuse owner/creator seeding from the shared helpers.
        // SAFETY: test-only INSERT.
        sqlx::query(
            "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) \
             VALUES (?, ?, 'active', datetime('now'), '{}')",
        )
        .bind(OWNER)
        .bind(OWNER_NAME)
        .execute(&pool_a)
        .await
        .unwrap();

        nexus_local_db::kb_store::seed::world(
            &pool_a,
            WORLD_A,
            OWNER,
            "Activation World A",
            "activation-world-a",
            "private",
            "manual",
        )
        .await;

        let store_a = SqliteKbStore::new(pool_a.clone());

        // Entry "Dragon" — activation key ["dragon"], logic "and_any".
        let mut dragon = WorldKbEntry::new(WORLD_A, BlockType::Character, "Dragon");
        dragon.body = Some(WorldKbBody {
            summary: Some("A fearsome fire-breathing dragon".to_string()),
            ..Default::default()
        });
        dragon.modules = Some(serde_json::json!({
            "activation": {"key": ["dragon"], "logic": "and_any"}
        }));
        let _dragon_res = store_a.insert_knowledge_entry(dragon).await.unwrap();

        // Entry "Ghost" — activation key ["haunt"], must NOT match
        // a stage0 that only mentions "dragon". Summary deliberately
        // avoids the substring "haunt" so the entry does not self-match.
        let mut ghost = WorldKbEntry::new(WORLD_A, BlockType::Character, "Ghost");
        ghost.body = Some(WorldKbBody {
            summary: Some("A spooky translucent apparition".to_string()),
            ..Default::default()
        });
        ghost.modules = Some(serde_json::json!({
            "activation": {"key": ["haunt"], "logic": "and_any"}
        }));
        let _ghost_res = store_a.insert_knowledge_entry(ghost).await.unwrap();

        // ── Phase 2: Export world A → pack file ─────────────────────────
        let (pack_path, _pack_dir) = export_to_file_custom_world(&pool_a, WORLD_A).await;

        // ── Phase 3: Fresh DB with world B, import pack ─────────────────
        let dir_b = tempfile::tempdir().unwrap();
        let db_path_b = dir_b.path().join("state.db");
        let pool_b = crate::db::Schema::init(&db_path_b).await.unwrap();

        // SAFETY: test-only INSERT.
        sqlx::query(
            "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) \
             VALUES (?, ?, 'active', datetime('now'), '{}')",
        )
        .bind(OWNER)
        .bind(OWNER_NAME)
        .execute(&pool_b)
        .await
        .unwrap();

        nexus_local_db::kb_store::seed::world(
            &pool_b,
            WORLD_B,
            OWNER,
            "Activation World B",
            "activation-world-b",
            "private",
            "manual",
        )
        .await;

        let import_args = ImportArgs {
            world_ref: WORLD_B.to_string(),
            r#in: Some(pack_path),
            from_st: None,
            dry_run: false,
            conflict: ConflictStrategy::Skip,
        };
        import(import_args, &config_with_active_creator(), &pool_b)
            .await
            .expect("import must succeed");

        // ── Phase 4: Verify modules survived the round-trip ─────────────
        let store_b = SqliteKbStore::new(pool_b.clone());
        let entries_b = store_b.list_by_world(WORLD_B).await.unwrap();
        assert_eq!(entries_b.len(), 2, "both entries imported");

        // Dragon: modules must be preserved verbatim.
        let imported_dragon = entries_b
            .iter()
            .find(|e| e.canonical_name == "Dragon")
            .expect("Dragon entry must exist after import");
        assert!(
            imported_dragon.modules.is_some(),
            "Dragon.modules must survive pack round-trip — R-V1146P3-001 gate"
        );
        let dragon_modules = imported_dragon.modules.as_ref().unwrap();
        let activation = dragon_modules
            .get("activation")
            .expect("activation sub-module must be present");
        assert_eq!(
            activation["logic"], "and_any",
            "activation.logic must round-trip"
        );
        assert!(
            activation["key"]
                .as_array()
                .unwrap()
                .contains(&serde_json::json!("dragon")),
            "activation.key must contain 'dragon' after round-trip"
        );

        // Ghost: must also preserve modules.
        let imported_ghost = entries_b
            .iter()
            .find(|e| e.canonical_name == "Ghost")
            .expect("Ghost entry must exist after import");
        assert!(
            imported_ghost.modules.is_some(),
            "Ghost.modules must survive pack round-trip"
        );

        // ── Phase 5: Activation with stage0 mentioning "dragon" only ────
        let stage0_dragon = "The story begins where a fearsome dragon awakens.";
        let result = activation::apply_activation(&entries_b, stage0_dragon, &[]);

        // Dragon: has activation key "dragon", present in stage0 → matched.
        assert_eq!(
            result.matched.len(),
            1,
            "only Dragon should match when stage0 contains 'dragon' but not 'haunt'"
        );
        assert_eq!(result.unmatched.len(), 1, "Ghost should be unmatched");

        assert!(
            result.matched.iter().any(|e| e.canonical_name == "Dragon"),
            "Dragon must be in matched set"
        );

        assert!(
            result.unmatched.iter().any(|e| e.canonical_name == "Ghost"),
            "Ghost must be in unmatched set"
        );

        // Trace must record the classification.
        let dragon_trace = result
            .trace
            .iter()
            .find(|t| t.canonical_name == "Dragon")
            .expect("Dragon must have a trace entry");
        assert!(dragon_trace.accepted, "Dragon trace must show accepted");
        assert!(
            dragon_trace.reason.contains("matched"),
            "Dragon trace reason must indicate key match: {}",
            dragon_trace.reason
        );

        let ghost_trace = result
            .trace
            .iter()
            .find(|t| t.canonical_name == "Ghost")
            .expect("Ghost must have a trace entry");
        assert!(!ghost_trace.accepted, "Ghost trace must show NOT accepted");
        assert!(
            ghost_trace.reason.contains("no key matched"),
            "Ghost trace reason must indicate no match: {}",
            ghost_trace.reason
        );

        // ── Phase 6: Activation with stage0 mentioning both keys ────────
        let stage0_both = "A dragon battles a ghost that haunts the ruins.";
        let result2 = activation::apply_activation(&entries_b, stage0_both, &[]);

        assert_eq!(
            result2.matched.len(),
            2,
            "both entries should match when stage0 contains both 'dragon' and 'haunt'"
        );
        assert!(
            result2.unmatched.is_empty(),
            "no entries should be unmatched"
        );
    }

    /// Helper: export pool's entries for a given world to a temp pack file.
    /// Mirrors [`export_to_file`] but accepts a custom `world_id`.
    async fn export_to_file_custom_world(
        pool: &SqlitePool,
        world_id: &str,
    ) -> (PathBuf, tempfile::TempDir) {
        let tmp_dir = tempfile::tempdir().unwrap();
        let out_path = tmp_dir.path().join("test_pack.json");

        let args = ExportArgs {
            world_ref: world_id.to_string(),
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

    #[tokio::test(flavor = "multi_thread")]
    async fn import_skips_unknown_entry_type_without_nonzero_exit() {
        let (pool, _dir) = empty_world_pool().await;
        let dir = tempfile::tempdir().unwrap();
        let pack_path = dir.path().join("unknown_type_pack.json");
        let pack_json = json!({
            "modules": { "pack": { "title": "Test", "version": "0.1.0", "creator": "test" } },
            "entries": [
                {
                    "entry_id": "kb_valid_entry",
                    "schema_version": 1,
                    "entry_type": "character",
                    "canonical_name": "Valid",
                    "status": "confirmed",
                    "body": { "summary": "valid" },
                    "extensions": { "nexus": { "world_id": "wld_pack" } }
                },
                {
                    "entry_id": "kb_bad_type",
                    "schema_version": 1,
                    "entry_type": "not_a_real_block_type",
                    "canonical_name": "BadType",
                    "status": "confirmed",
                    "body": { "summary": "bad" },
                    "extensions": { "nexus": { "world_id": "wld_pack" } }
                }
            ],
            "relations": []
        });
        std::fs::write(
            &pack_path,
            serde_json::to_string_pretty(&pack_json).unwrap(),
        )
        .unwrap();

        let parsed = parse_pack(&pack_json).expect("pack must parse");
        let summary = import_pack(
            &pool,
            WORLD,
            OWNER,
            parsed,
            ConflictPolicy::Skip,
            false,
            false,
        )
        .await
        .expect("unknown entry_type must not fail import under skip");
        assert_eq!(summary.entries.skipped, 1);
        assert_eq!(summary.entries.rejected, 0);
        assert_eq!(summary.entries.created, 1);

        let args = ImportArgs {
            world_ref: WORLD.to_string(),
            r#in: Some(pack_path),
            from_st: None,
            dry_run: false,
            conflict: ConflictStrategy::Skip,
        };
        import(args, &config_with_active_creator(), &pool)
            .await
            .expect("CLI import must succeed with skipped unknown entry_type");
        assert_eq!(count_entries(&pool, WORLD).await, 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn import_dry_run_rename_reports_counts_without_writes() {
        let (pool, _dir, _entry_ids, _rel_ids) = seeded_pool().await;
        let (pack_path, _pack_dir) = export_to_file(&pool).await;
        let (pool2, _dir2) = empty_world_pool().await;
        let store = SqliteKbStore::new(pool2.clone());
        store
            .insert_knowledge_entry(WorldKbEntry::new(WORLD, BlockType::Character, "Carol"))
            .await
            .unwrap();
        let pre = count_entries(&pool2, WORLD).await;
        let parsed = parse_pack(
            &serde_json::from_str::<serde_json::Value>(
                &std::fs::read_to_string(&pack_path).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
        let summary = import_pack(
            &pool2,
            WORLD,
            OWNER,
            parsed,
            ConflictPolicy::Rename,
            false,
            true,
        )
        .await
        .unwrap();
        assert!(summary.entries.renamed >= 1);
        assert_eq!(count_entries(&pool2, WORLD).await, pre);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn import_dry_run_overwrite_reports_counts_without_writes() {
        let (pool, _dir, _entry_ids, _rel_ids) = seeded_pool().await;
        let (pack_path, _pack_dir) = export_to_file(&pool).await;
        let (pool2, _dir2) = empty_world_pool().await;
        let store = SqliteKbStore::new(pool2.clone());
        store
            .insert_knowledge_entry(WorldKbEntry::new(WORLD, BlockType::Character, "Carol"))
            .await
            .unwrap();
        let pre = count_entries(&pool2, WORLD).await;
        let parsed = parse_pack(
            &serde_json::from_str::<serde_json::Value>(
                &std::fs::read_to_string(&pack_path).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
        let summary = import_pack(
            &pool2,
            WORLD,
            OWNER,
            parsed,
            ConflictPolicy::Overwrite,
            false,
            true,
        )
        .await
        .unwrap();
        assert!(summary.entries.overwritten >= 1);
        assert_eq!(count_entries(&pool2, WORLD).await, pre);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn import_rename_disambiguates_near_max_length_canonical_name() {
        let long_name = "x".repeat(250);
        let (pool, _dir) = empty_world_pool().await;
        let store = SqliteKbStore::new(pool.clone());
        store
            .insert_knowledge_entry(WorldKbEntry::new(WORLD, BlockType::Character, &long_name))
            .await
            .unwrap();
        let pack_json = json!({
            "modules": { "pack": { "title": "Long", "version": "0.1.0", "creator": "test" } },
            "entries": [{
                "entry_id": "kb_long_pack",
                "schema_version": 1,
                "entry_type": "character",
                "canonical_name": long_name,
                "status": "confirmed",
                "body": { "summary": "long" },
                "extensions": { "nexus": { "world_id": WORLD } }
            }],
            "relations": []
        });
        let summary = import_pack(
            &pool,
            WORLD,
            OWNER,
            parse_pack(&pack_json).unwrap(),
            ConflictPolicy::Rename,
            false,
            false,
        )
        .await
        .unwrap();
        assert_eq!(summary.entries.renamed, 1);
        assert_eq!(summary.entries.rejected, 0);
        let entries = store.list_by_world(WORLD).await.unwrap();
        assert_eq!(entries.len(), 2);
        let renamed = entries
            .iter()
            .find(|e| e.canonical_name.contains("imported"))
            .expect("renamed");
        assert!(renamed.canonical_name.len() <= 256);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn import_overwrite_relation_cas_marks_overwritten() {
        let (pool, _dir, _entry_ids, rel_ids) = seeded_pool().await;
        let (pack_path, _pack_dir) = export_to_file(&pool).await;
        let (pool2, _dir2) = empty_world_pool().await;
        let store = SqliteKbStore::new(pool2.clone());
        let mut target_ids = Vec::new();
        for name in ["Alice", "Bob", "Carol"] {
            let res = store
                .insert_knowledge_entry(WorldKbEntry::new(WORLD, BlockType::Character, name))
                .await
                .unwrap();
            target_ids.push(res.entry_id);
        }
        let rel_id = &rel_ids[0];
        sqlx::query("INSERT INTO kb_relationships (relationship_id, world_id, source_entity_id, target_entity_id, relation_type, symmetric, confidence, source_anchor_ids, metadata, created_at, updated_at, revision, needs_review, source) VALUES (?, ?, ?, ?, 'related_to', 0, NULL, '[]', '{}', datetime('now'), datetime('now'), 1, 0, 'manual')")
            .bind(rel_id).bind(WORLD).bind(&target_ids[0]).bind(&target_ids[1]).execute(&pool2).await.unwrap();
        let parsed = parse_pack(
            &serde_json::from_str::<serde_json::Value>(
                &std::fs::read_to_string(&pack_path).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
        let summary = import_pack(
            &pool2,
            WORLD,
            OWNER,
            parsed,
            ConflictPolicy::Overwrite,
            false,
            false,
        )
        .await
        .unwrap();
        assert!(summary.relations.overwritten >= 1);
        assert_eq!(count_relations(&pool2, WORLD).await, 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn pack_io_modules_preserved_on_rename_and_overwrite_collision() {
        use nexus_spoke_adapter::adapter::activation;
        const WORLD_A: &str = "wld_activation_rename";
        let dir_a = tempfile::tempdir().unwrap();
        let pool_a = crate::db::Schema::init(&dir_a.path().join("state.db"))
            .await
            .unwrap();
        sqlx::query("INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) VALUES (?, ?, 'active', datetime('now'), '{}')").bind(OWNER).bind(OWNER_NAME).execute(&pool_a).await.unwrap();
        nexus_local_db::kb_store::seed::world(
            &pool_a,
            WORLD_A,
            OWNER,
            "Activation Rename",
            "activation-rename",
            "private",
            "manual",
        )
        .await;
        let store_a = SqliteKbStore::new(pool_a.clone());
        let mut dragon = WorldKbEntry::new(WORLD_A, BlockType::Character, "Dragon");
        dragon.modules = Some(json!({"activation": {"key": ["dragon"], "logic": "and_any"}}));
        store_a.insert_knowledge_entry(dragon).await.unwrap();
        let (pack_path, _pack_dir) = export_to_file_custom_world(&pool_a, WORLD_A).await;
        let (pool_rename, _dir_r) = empty_world_pool().await;
        let store_r = SqliteKbStore::new(pool_rename.clone());
        store_r
            .insert_knowledge_entry(WorldKbEntry::new(WORLD, BlockType::Character, "Dragon"))
            .await
            .unwrap();
        let parsed = parse_pack(
            &serde_json::from_str::<serde_json::Value>(
                &std::fs::read_to_string(&pack_path).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
        import_pack(
            &pool_rename,
            WORLD,
            OWNER,
            parsed,
            ConflictPolicy::Rename,
            false,
            false,
        )
        .await
        .unwrap();
        let renamed = store_r
            .list_by_world(WORLD)
            .await
            .unwrap()
            .into_iter()
            .find(|e| e.canonical_name.contains("imported"))
            .expect("renamed Dragon");
        assert!(renamed.modules.is_some());
        let _ = activation::apply_activation(&[renamed], "a dragon appears", &[]);
        let (pool_over, _dir_o) = empty_world_pool().await;
        let store_o = SqliteKbStore::new(pool_over.clone());
        let mut pre2 = WorldKbEntry::new(WORLD, BlockType::Character, "Dragon");
        pre2.modules = Some(json!({"activation": {"key": ["stale"], "logic": "and_any"}}));
        store_o.insert_knowledge_entry(pre2).await.unwrap();
        let parsed2 = parse_pack(
            &serde_json::from_str::<serde_json::Value>(
                &std::fs::read_to_string(&pack_path).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
        import_pack(
            &pool_over,
            WORLD,
            OWNER,
            parsed2,
            ConflictPolicy::Overwrite,
            false,
            false,
        )
        .await
        .unwrap();
        let overwritten = store_o
            .list_by_world(WORLD)
            .await
            .unwrap()
            .into_iter()
            .find(|e| e.canonical_name == "Dragon")
            .expect("overwritten Dragon");
        assert_eq!(
            overwritten
                .modules
                .as_ref()
                .and_then(|m| m.get("activation"))
                .and_then(|a| a.get("key"))
                .and_then(|k| k.get(0))
                .and_then(|v| v.as_str()),
            Some("dragon")
        );
    }

    #[tokio::test]
    async fn import_errors_on_invalid_pack_shape() {
        let (pool, _dir) = empty_world_pool().await;
        let dir = tempfile::tempdir().unwrap();
        let pack_path = dir.path().join("bad_pack.json");
        // Valid JSON but missing the required `modules.pack` key.
        let pack_json = serde_json::json!({
            "entries": [],
            "relations": []
        });
        std::fs::write(
            &pack_path,
            serde_json::to_string_pretty(&pack_json).unwrap(),
        )
        .unwrap();

        let args = ImportArgs {
            world_ref: WORLD.to_string(),
            r#in: Some(pack_path),
            from_st: None,
            dry_run: false,
            conflict: ConflictStrategy::Skip,
        };
        let err = import(args, &config_with_active_creator(), &pool)
            .await
            .expect_err("import must fail for invalid pack shape");
        let msg = format!("{err}");
        assert!(
            msg.contains("Invalid pack format"),
            "error must mention Invalid pack format; got: {msg}"
        );
    }

    // ── DF-80: ST lorebook import (`--from-st`) ────────────────────────

    /// `--from-st` and `--in` are mutually exclusive at the clap level;
    /// exactly one source is required.
    #[test]
    fn import_from_st_conflicts_with_in_at_clap() {
        use crate::cli::Cli;
        use clap::Parser;

        let from_st_only = Cli::try_parse_from([
            "nexus42",
            "creator",
            "world",
            "kb",
            "pack",
            "import",
            "wld_1",
            "--from-st",
            "lorebook.json",
        ]);
        assert!(from_st_only.is_ok(), "--from-st alone must parse");

        let both = Cli::try_parse_from([
            "nexus42",
            "creator",
            "world",
            "kb",
            "pack",
            "import",
            "wld_1",
            "--in",
            "pack.json",
            "--from-st",
            "lorebook.json",
        ]);
        assert!(both.is_err(), "--in + --from-st must be rejected by clap");

        let neither = Cli::try_parse_from([
            "nexus42", "creator", "world", "kb", "pack", "import", "wld_1",
        ]);
        assert!(neither.is_err(), "import without a source must fail");
    }

    /// A documented-format ST lorebook imports through the standard pack
    /// path: entries land with `canonical_name` from `comment`, `body.summary`
    /// from `content`, and `modules.activation` from keys/constant.
    #[tokio::test(flavor = "multi_thread")]
    async fn import_from_st_creates_entries_with_activation() {
        let (pool, _dir) = empty_world_pool().await;
        let dir = tempfile::tempdir().unwrap();
        let st_path = dir.path().join("lorebook.json");
        std::fs::write(
            &st_path,
            r#"{
                "name": "ST World",
                "entries": [
                    { "uid": 0, "key": "dragon", "content": "Dragons are ancient.", "comment": "Dragon lore" },
                    { "uid": 1, "keys": ["slime", "slimes"], "content": "Slimes bounce.", "comment": "Slime", "constant": true }
                ]
            }"#,
        )
        .unwrap();

        let args = ImportArgs {
            world_ref: WORLD.to_string(),
            r#in: None,
            from_st: Some(st_path),
            dry_run: false,
            conflict: ConflictStrategy::Skip,
        };
        import(args, &config_with_active_creator(), &pool)
            .await
            .expect("ST lorebook import must succeed");

        let store = SqliteKbStore::new(pool.clone());
        let entries = store.list_by_world(WORLD).await.unwrap();
        assert_eq!(entries.len(), 2);
        let dragon = entries
            .iter()
            .find(|e| e.canonical_name == "Dragon lore")
            .expect("dragon entry");
        assert_eq!(
            dragon.body.as_ref().and_then(|b| b.summary.as_deref()),
            Some("Dragons are ancient.")
        );
        let activation = dragon
            .modules
            .as_ref()
            .and_then(|m| m.get("activation"))
            .expect("activation module");
        assert_eq!(activation["keys"], serde_json::json!(["dragon"]));
        assert_eq!(activation["constant"], serde_json::json!(false));
        let slime = entries
            .iter()
            .find(|e| e.canonical_name == "Slime")
            .expect("slime entry");
        let activation = slime
            .modules
            .as_ref()
            .and_then(|m| m.get("activation"))
            .expect("activation module");
        assert_eq!(activation["keys"], serde_json::json!(["slime", "slimes"]));
        assert_eq!(activation["constant"], serde_json::json!(true));
    }

    /// Unknown/undocumented ST fields produce diagnostics but do not abort
    /// the import — all entries still land.
    #[tokio::test(flavor = "multi_thread")]
    async fn import_from_st_unknown_fields_import_continues() {
        let (pool, _dir) = empty_world_pool().await;
        let dir = tempfile::tempdir().unwrap();
        let st_path = dir.path().join("lorebook.json");
        std::fs::write(
            &st_path,
            r#"{
                "entries": [
                    { "uid": 0, "key": "harbor", "content": "The harbor gates.", "comment": "Harbor", "favorite_color": "blue" }
                ]
            }"#,
        )
        .unwrap();

        let args = ImportArgs {
            world_ref: WORLD.to_string(),
            r#in: None,
            from_st: Some(st_path),
            dry_run: false,
            conflict: ConflictStrategy::Skip,
        };
        import(args, &config_with_active_creator(), &pool)
            .await
            .expect("unknown fields must not abort the import");
        assert_eq!(count_entries(&pool, WORLD).await, 1);
    }

    /// Malformed ST lorebook files abort before any write (no partial import).
    #[tokio::test]
    async fn import_from_st_malformed_file_aborts_before_write() {
        let (pool, _dir) = empty_world_pool().await;
        let dir = tempfile::tempdir().unwrap();
        let st_path = dir.path().join("lorebook.json");
        // Valid JSON but `entries` is an object, not an array.
        std::fs::write(&st_path, r#"{ "entries": { "0": {} } }"#).unwrap();

        let args = ImportArgs {
            world_ref: WORLD.to_string(),
            r#in: None,
            from_st: Some(st_path),
            dry_run: false,
            conflict: ConflictStrategy::Skip,
        };
        let err = import(args, &config_with_active_creator(), &pool)
            .await
            .expect_err("malformed ST lorebook must fail");
        let msg = format!("{err}");
        assert!(
            msg.contains("Invalid ST lorebook format"),
            "error must mention ST lorebook format; got: {msg}"
        );
        assert_eq!(count_entries(&pool, WORLD).await, 0, "no partial import");
    }

    /// `--conflict` passes through to `import_pack` unchanged on the ST path:
    /// a rename policy disambiguates a canonical-name collision.
    #[tokio::test(flavor = "multi_thread")]
    async fn import_from_st_conflict_rename_passthrough() {
        let (pool, _dir) = empty_world_pool().await;
        let store = SqliteKbStore::new(pool.clone());
        let mut existing = WorldKbEntry::new(WORLD, BlockType::InfoPoint, "Dragon lore");
        existing.body = Some(WorldKbBody {
            summary: Some("Pre-existing dragon lore.".to_string()),
            ..Default::default()
        });
        store.insert_knowledge_entry(existing).await.unwrap();

        let dir = tempfile::tempdir().unwrap();
        let st_path = dir.path().join("lorebook.json");
        std::fs::write(
            &st_path,
            r#"{
                "entries": [
                    { "uid": 0, "key": "dragon", "content": "Dragons are ancient.", "comment": "Dragon lore" }
                ]
            }"#,
        )
        .unwrap();

        let args = ImportArgs {
            world_ref: WORLD.to_string(),
            r#in: None,
            from_st: Some(st_path),
            dry_run: false,
            conflict: ConflictStrategy::Rename,
        };
        import(args, &config_with_active_creator(), &pool)
            .await
            .expect("rename policy must apply on the ST path");
        let entries = store.list_by_world(WORLD).await.unwrap();
        assert_eq!(entries.len(), 2, "rename must create a disambiguated entry");
        assert!(
            entries.iter().any(|e| e.canonical_name != "Dragon lore"),
            "renamed entry must carry a disambiguated canonical name"
        );
    }
}
