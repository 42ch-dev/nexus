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
/// Returns `(fragment_count, distinct_keyword_count, max_created_at)` for the
/// given creator. Keyword distinct counting decodes JSON arrays in Rust.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn soul_narrative_fragment_stats(
    pool: &SqlitePool,
    creator_id: &str,
) -> Result<SoulNarrativeFragmentStats, LocalDbError> {
    let rows = sqlx::query!(
        "SELECT keywords as \"keywords!\", created_at as \"created_at!\"
         FROM memory_fragments WHERE creator_id = ?",
        creator_id
    )
    .fetch_all(pool)
    .await?;

    let mut distinct: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut max_created_at: Option<String> = None;
    let fragment_count = i64::try_from(rows.len()).unwrap_or(i64::MAX);

    for row in &rows {
        // Decode the JSON keywords array.
        if let Ok(keywords) = serde_json::from_str::<Vec<String>>(&row.keywords) {
            for kw in keywords {
                distinct.insert(kw);
            }
        }
        // Track the max created_at.
        if let Some(ref current) = max_created_at {
            if row.created_at.as_str() > current.as_str() {
                max_created_at = Some(row.created_at.clone());
            }
        } else {
            max_created_at = Some(row.created_at.clone());
        }
    }

    Ok(SoulNarrativeFragmentStats {
        fragment_count,
        distinct_keyword_count: distinct.len(),
        max_created_at,
    })
}
