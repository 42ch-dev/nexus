//! `memory_soul_narratives` persistence DAO (V1.81).
//!
//! Caches the on-demand Creator-SOUL LLM narrative with stale-invalidation
//! snapshot columns (`fragment_count_at_generation`,
//! `max_fragment_created_at_at_generation`).

use sqlx::SqlitePool;

use crate::error::LocalDbError;

/// A row from the `memory_soul_narratives` table.
#[derive(Debug, Clone)]
pub struct SoulNarrativeRecord {
    pub creator_id: String,
    pub narrative: String,
    pub generated_at: String,
    pub fragment_count_at_generation: i64,
    pub max_fragment_created_at_at_generation: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Fragment statistics for a creator, used for stale-detection and the
/// insufficient-data gate.
#[derive(Debug, Clone, Default)]
pub struct SoulNarrativeFragmentStats {
    pub fragment_count: i64,
    pub distinct_keyword_count: usize,
    pub max_created_at: Option<String>,
}

/// Read the cached narrative for a creator (if any).
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn get_soul_narrative(
    pool: &SqlitePool,
    creator_id: &str,
) -> Result<Option<SoulNarrativeRecord>, LocalDbError> {
    let row = sqlx::query!(
        r#"SELECT creator_id as "creator_id!", narrative as "narrative!",
                  generated_at as "generated_at!",
                  fragment_count_at_generation as "fragment_count_at_generation!",
                  max_fragment_created_at_at_generation,
                  created_at as "created_at!", updated_at as "updated_at!"
           FROM memory_soul_narratives WHERE creator_id = ?"#,
        creator_id
    )
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| SoulNarrativeRecord {
        creator_id: r.creator_id,
        narrative: r.narrative,
        generated_at: r.generated_at,
        fragment_count_at_generation: r.fragment_count_at_generation,
        max_fragment_created_at_at_generation: r.max_fragment_created_at_at_generation,
        created_at: r.created_at,
        updated_at: r.updated_at,
    }))
}

/// Insert or update the cached narrative for a creator.
///
/// Uses `INSERT OR REPLACE` so the handler can call this unconditionally
/// after synthesis.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn upsert_soul_narrative(
    pool: &SqlitePool,
    record: &SoulNarrativeRecord,
) -> Result<(), LocalDbError> {
    sqlx::query!(
        "INSERT OR REPLACE INTO memory_soul_narratives
         (creator_id, narrative, generated_at, fragment_count_at_generation,
          max_fragment_created_at_at_generation, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
        record.creator_id,
        record.narrative,
        record.generated_at,
        record.fragment_count_at_generation,
        record.max_fragment_created_at_at_generation,
        record.created_at,
        record.updated_at
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Compute fragment statistics for stale-detection and the insufficient-data gate.
///
/// Uses SQL aggregates for `fragment_count` and `max_created_at` to avoid
/// materializing all rows. The `distinct_keyword_count` uses a bounded scan
/// (capped at `KEYWORD_SCAN_CAP`) — sufficient because the gate only checks
/// `distinct_keyword_count < 20`, and a result ≥ 20 from the capped set is
/// authoritative.
///
/// Returns `SoulNarrativeFragmentStats` with the computed statistics.
///
/// # Errors
///
/// Returns `LocalDbError` if any database query fails.
pub async fn soul_narrative_fragment_stats(
    pool: &SqlitePool,
    creator_id: &str,
) -> Result<SoulNarrativeFragmentStats, LocalDbError> {
    // distinct_keyword_count: bounded scan. The insufficient-data gate
    // threshold is 20 distinct keywords; capping at 200 is well above that,
    // so the gate result is correct regardless of the cap.
    const KEYWORD_SCAN_CAP: i64 = 200;

    // fragment_count: SQL aggregate — no row materialization.
    let fragment_count = sqlx::query_scalar!(
        r#"SELECT COUNT(*) as "count!: i64" FROM memory_fragments WHERE creator_id = ?"#,
        creator_id
    )
    .fetch_one(pool)
    .await?;

    // max_created_at: SQL aggregate — no row materialization.
    let max_created_at: Option<String> = sqlx::query_scalar!(
        r#"SELECT MAX(created_at) as "max_created_at?: String" FROM memory_fragments WHERE creator_id = ?"#,
        creator_id
    )
    .fetch_one(pool)
    .await?;

    // distinct_keyword_count: bounded scan.
    let keyword_rows = sqlx::query_scalar!(
        r#"SELECT keywords as "keywords!: String" FROM memory_fragments WHERE creator_id = ? ORDER BY created_at DESC LIMIT ?"#,
        creator_id,
        KEYWORD_SCAN_CAP
    )
    .fetch_all(pool)
    .await?;

    let mut distinct: std::collections::HashSet<String> = std::collections::HashSet::new();
    for row in &keyword_rows {
        if let Ok(keywords) = serde_json::from_str::<Vec<String>>(row) {
            for kw in keywords {
                distinct.insert(kw);
            }
        }
    }

    Ok(SoulNarrativeFragmentStats {
        fragment_count,
        distinct_keyword_count: distinct.len(),
        max_created_at,
    })
}
