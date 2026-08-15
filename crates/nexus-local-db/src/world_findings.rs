//! World-scoped check findings storage (V1.165 P1 T1 / DR-68) — the
//! `world_findings` table (AR-1 lock,
//! `20260815000001_create_world_findings.sql`).
//!
//! **Pure storage (AR-1 / V1.164 P2 layering):** this module is spoke-free —
//! it persists raw column values and performs no wire-shape or vocabulary
//! validation. The spoke `Finding` → `world_findings` mapping and the
//! `extensions.nexus` routing gate live at the adapter boundary
//! (`nexus-spoke-adapter::adapter::finding_port` — the sole
//! `spoke-operations` consumer).
//!
//! **Vocabulary lock (AR-1):** spoke severity/status persist **and** read
//! verbatim on the world path (`info|warning|error` / `open|resolved|dismissed`)
//! — no nexus mapping for world rows (world findings are advisory check
//! output, not Work manuscript findings). The work-path mapping is unchanged
//! and lives in the adapter.
//!
//! **No `creator_id` column (AR-1):** isolation is world ownership
//! (`narrative_worlds.owner_creator_id`); the stamped
//! `extensions.nexus.creator_id` rides `extensions_json` verbatim as
//! provenance — no per-creator query consumer exists.
//!
//! **Batch inserts run in the caller's transaction:** `insert_world_finding_in_tx`
//! joins the adapter's W-1 `BEGIN`/`COMMIT` (AR-2 batch atomicity) — a
//! mid-batch failure rolls work- and world-scoped rows back together.
//! `created_at` / `updated_at` are Unix-epoch seconds (legacy `findings`
//! convention) provided by the caller (the adapter converts spoke RFC 3339
//! timestamps, mirroring `map_spoke_to_nexus`).

use crate::LocalDbError;
use sqlx::{Sqlite, SqlitePool, Transaction};

/// Row type matching the `world_findings` DDL
/// (`20260815000001_create_world_findings.sql`).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct WorldFindingRow {
    /// PK — spoke `Finding.finding_id` (`fnd_*`; uuid-suffixed per emit).
    pub finding_id: String,
    /// Owning `narrative_worlds.world_id` — FK, `ON DELETE CASCADE`.
    pub world_id: String,
    /// Spoke `Finding.schema_version`.
    pub schema_version: i64,
    /// Spoke severity vocabulary verbatim (`info` | `warning` | `error`).
    pub severity: String,
    /// Spoke status vocabulary verbatim (`open` | `resolved` | `dismissed`).
    pub status: String,
    /// Short human-readable label.
    pub title: String,
    /// Detailed finding body.
    pub description: String,
    /// Optional checker kind (e.g. `stale_belief_drift`).
    pub kind: Option<String>,
    /// Optional entry id the finding is scoped to.
    pub target_entry_id: Option<String>,
    /// Serialized spoke `SourceAnchor` object (verbatim JSON, when present).
    pub source_anchor_json: Option<String>,
    /// Optional remediation text.
    pub suggested_fix: Option<String>,
    /// Serialized spoke `Map` (required, default `{}`).
    pub text_position_json: String,
    /// Serialized spoke `ExtensionMap` verbatim (incl. stamped
    /// `extensions.nexus.world_id` / `creator_id`).
    pub extensions_json: String,
    /// Unix-epoch seconds (legacy `findings` convention).
    pub created_at: i64,
    /// Unix-epoch seconds.
    pub updated_at: i64,
}

/// Insert a world-scoped finding row from raw column values.
///
/// Pure storage: no validation is performed here — the caller (the adapter
/// `finding_port` routing gate) is responsible for the spoke wire-shape and
/// `extensions.nexus` discriminator checks. Runs **inside the caller's
/// transaction** so a batch of work- and world-scoped rows commits (or rolls
/// back) atomically (AR-2 W-1 batch atomicity). `created_at` / `updated_at`
/// are Unix-epoch seconds provided by the caller (the adapter converts spoke
/// RFC 3339 timestamps, mirroring `map_spoke_to_nexus`).
///
/// # Errors
///
/// Returns [`LocalDbError::Sqlx`] on database failure — including a
/// duplicate `finding_id` primary key and an unknown `world_id` foreign key.
#[allow(clippy::too_many_arguments)] // raw column values — the full row shape
pub async fn insert_world_finding_in_tx(
    tx: &mut Transaction<'_, Sqlite>,
    finding_id: &str,
    world_id: &str,
    schema_version: i64,
    severity: &str,
    status: &str,
    title: &str,
    description: &str,
    kind: Option<&str>,
    target_entry_id: Option<&str>,
    source_anchor_json: Option<&str>,
    suggested_fix: Option<&str>,
    text_position_json: &str,
    extensions_json: &str,
    created_at: i64,
    updated_at: i64,
) -> Result<(), LocalDbError> {
    sqlx::query!(
        "INSERT INTO world_findings (
            finding_id, world_id, schema_version, severity, status, title,
            description, kind, target_entry_id, source_anchor_json,
            suggested_fix, text_position_json, extensions_json,
            created_at, updated_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        finding_id,
        world_id,
        schema_version,
        severity,
        status,
        title,
        description,
        kind,
        target_entry_id,
        source_anchor_json,
        suggested_fix,
        text_position_json,
        extensions_json,
        created_at,
        updated_at,
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Fetch a single `world_findings` row by `finding_id`.
///
/// Returns `Ok(None)` when no row matches.
///
/// # Errors
///
/// Returns [`LocalDbError::Sqlx`] on database failure.
pub async fn get_world_finding(
    pool: &SqlitePool,
    finding_id: &str,
) -> Result<Option<WorldFindingRow>, LocalDbError> {
    // `finding_id as "finding_id!"` — SQLite describes a TEXT PRIMARY KEY as
    // nullable (sqlx 0.8.6), the non-null coercion matches the field.
    let row = sqlx::query_as!(
        WorldFindingRow,
        "SELECT finding_id as \"finding_id!\", world_id, schema_version, severity, status, \
                title, description, kind, target_entry_id, source_anchor_json, \
                suggested_fix, text_position_json, extensions_json, created_at, updated_at \
         FROM world_findings \
         WHERE finding_id = ?",
        finding_id,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// List world-scoped findings for a world, newest-first.
///
/// Ordering is `created_at DESC, finding_id ASC` — the legacy findings list
/// convention (`findings.rs:498`); `idx_world_findings_world_created` serves
/// the world prefix and the `finding_id ASC` tiebreak makes the order
/// deterministic. Empty world → `Ok(vec![])`.
///
/// # Bounding (Bugbot 4bad2fca)
///
/// The caller owns the upper bound: `limit` is bound into the SQL `LIMIT ?`
/// so the read surface never loads an unbounded result set. Pass a probe of
/// `cap + 1` to detect truncation without fetching the full table. `limit`
/// is clamped to `>= 0` — `SQLite` treats a negative `LIMIT` as "no limit",
/// which would silently reintroduce the unbounded query.
///
/// # Errors
///
/// Returns [`LocalDbError::Sqlx`] on database failure.
pub async fn list_world_findings_by_world(
    pool: &SqlitePool,
    world_id: &str,
    limit: i64,
) -> Result<Vec<WorldFindingRow>, LocalDbError> {
    // SQLite treats a negative `LIMIT` as "no limit" — clamp so a buggy
    // caller can never silently reintroduce the unbounded query.
    let limit = limit.max(0);
    let rows = sqlx::query_as!(
        WorldFindingRow,
        "SELECT finding_id as \"finding_id!\", world_id, schema_version, severity, status, \
                title, description, kind, target_entry_id, source_anchor_json, \
                suggested_fix, text_position_json, extensions_json, created_at, updated_at \
         FROM world_findings \
         WHERE world_id = ? \
         ORDER BY created_at DESC, finding_id ASC \
         LIMIT ?",
        world_id,
        limit,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// List world-scoped findings for a target entry within a world.
///
/// `idx_world_findings_world_target (world_id, target_entry_id)` serves the
/// lookup; ordering matches [`list_world_findings_by_world`]
/// (`created_at DESC, finding_id ASC`). No rows for the target →
/// `Ok(vec![])`.
///
/// # Bounding (Bugbot 4bad2fca)
///
/// Same caller-owned bound as [`list_world_findings_by_world`]: `limit` is
/// bound into the SQL `LIMIT ?` (clamped to `>= 0` — `SQLite` treats a
/// negative `LIMIT` as "no limit").
///
/// # Errors
///
/// Returns [`LocalDbError::Sqlx`] on database failure.
pub async fn list_world_findings_by_target(
    pool: &SqlitePool,
    world_id: &str,
    target_entry_id: &str,
    limit: i64,
) -> Result<Vec<WorldFindingRow>, LocalDbError> {
    // SQLite treats a negative `LIMIT` as "no limit" — clamp so a buggy
    // caller can never silently reintroduce the unbounded query.
    let limit = limit.max(0);
    let rows = sqlx::query_as!(
        WorldFindingRow,
        "SELECT finding_id as \"finding_id!\", world_id, schema_version, severity, status, \
                title, description, kind, target_entry_id, source_anchor_json, \
                suggested_fix, text_position_json, extensions_json, created_at, updated_at \
         FROM world_findings \
         WHERE world_id = ? AND target_entry_id = ? \
         ORDER BY created_at DESC, finding_id ASC \
         LIMIT ?",
        world_id,
        target_entry_id,
        limit,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
