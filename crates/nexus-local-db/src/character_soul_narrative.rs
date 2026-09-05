//! Character SOUL narrative cache persistence (v1.184 P3 Task 1).
//!
//! Character counterpart to [`crate::soul_narrative`]: caches the on-demand
//! Character SOUL narrative on the dedicated `character_soul_narratives`
//! table, keyed per `(character_id, actor_world_binding_id)` scope (NULL
//! binding = shared Character scope). Staleness fingerprints and fragment
//! counts are computed inside the same bearer/binding scope. The
//! bearer-agnostic [`crate::soul_narrative::SoulNarrativeFragmentStats`] and
//! [`crate::soul_narrative::build_stats_fingerprint`] are reused unchanged.

use futures_util::TryStreamExt;
use sqlx::SqlitePool;

use crate::actor_world_binding::require_active_owned_provenance_pool;
use crate::character::{require_active_owned_character, require_owned_character_pool};
use crate::error::LocalDbError;
use crate::soul_narrative::{build_stats_fingerprint, SoulNarrativeFragmentStats};

/// A row from the `character_soul_narratives` table.
///
/// `narrative` and `generated_at` are `None` for stats-only rows (fingerprint
/// cache for under-gate / above-gate ungenerated Characters).
#[derive(Debug, Clone)]
pub struct CharacterSoulNarrativeRecord {
    pub character_id: String,
    /// `None` = shared Character scope; `Some(b)` = binding-local scope.
    pub actor_world_binding_id: Option<String>,
    pub narrative: Option<String>,
    pub generated_at: Option<String>,
    pub fragment_count_at_generation: i64,
    pub max_fragment_created_at_at_generation: Option<String>,
    pub distinct_keyword_count_cache: i64,
    pub stats_fingerprint: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[allow(clippy::too_many_arguments)] // raw column values — the full row shape
const fn record_from_row(
    character_id: String,
    actor_world_binding_id: Option<String>,
    narrative: Option<String>,
    generated_at: Option<String>,
    fragment_count_at_generation: i64,
    max_fragment_created_at_at_generation: Option<String>,
    distinct_keyword_count_cache: i64,
    stats_fingerprint: Option<String>,
    created_at: String,
    updated_at: String,
) -> CharacterSoulNarrativeRecord {
    CharacterSoulNarrativeRecord {
        character_id,
        actor_world_binding_id,
        narrative,
        generated_at,
        fragment_count_at_generation,
        max_fragment_created_at_at_generation,
        distinct_keyword_count_cache,
        stats_fingerprint,
        created_at,
        updated_at,
    }
}

async fn load_narrative(
    pool: &SqlitePool,
    character_id: &str,
    binding_id: Option<&str>,
) -> Result<Option<CharacterSoulNarrativeRecord>, LocalDbError> {
    let row = sqlx::query!(
        r#"SELECT character_id as "character_id!", actor_world_binding_id, narrative, generated_at,
                  fragment_count_at_generation as "fragment_count_at_generation!",
                  max_fragment_created_at_at_generation,
                  distinct_keyword_count_cache as "distinct_keyword_count_cache!",
                  stats_fingerprint,
                  created_at as "created_at!", updated_at as "updated_at!"
           FROM character_soul_narratives WHERE character_id = ? AND actor_world_binding_id IS ?"#,
        character_id,
        binding_id
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| {
        record_from_row(
            r.character_id,
            r.actor_world_binding_id,
            r.narrative,
            r.generated_at,
            r.fragment_count_at_generation,
            r.max_fragment_created_at_at_generation,
            r.distinct_keyword_count_cache,
            r.stats_fingerprint,
            r.created_at,
            r.updated_at,
        )
    }))
}

/// Read the cached narrative for a Character scope.
///
/// `binding_id = None` reads the shared Character scope; `Some(b)` reads the
/// binding-local scope and requires the exact active binding.
///
/// # Errors
///
/// Returns `LocalDbError::ActorNotFound` when the Character (or, for a
/// binding-scoped read, the binding) is missing, foreign, or inactive;
/// `LocalDbError` on database failure.
pub async fn get_character_soul_narrative(
    pool: &SqlitePool,
    owner_creator_id: &str,
    character_id: &str,
    binding_id: Option<&str>,
) -> Result<Option<CharacterSoulNarrativeRecord>, LocalDbError> {
    require_owned_character_pool(pool, owner_creator_id, character_id).await?;
    if let Some(binding_id) = binding_id {
        require_active_owned_provenance_pool(pool, owner_creator_id, character_id, binding_id)
            .await?;
    }
    load_narrative(pool, character_id, binding_id).await
}

/// Insert or update the cached narrative for a Character scope.
///
/// Runs in a write-serialized transaction that validates Character ownership
/// and, for a binding-local scope, binding provenance. `INSERT OR REPLACE`
/// hits the composite PK / partial UNIQUE conflict target correctly (NULL
/// binding for the shared scope).
///
/// # Errors
///
/// Returns `LocalDbError::ActorNotFound` for a foreign Character or invalid
/// binding; `LocalDbError` on constraint or database failure.
pub async fn upsert_character_soul_narrative(
    pool: &SqlitePool,
    owner_creator_id: &str,
    record: &CharacterSoulNarrativeRecord,
) -> Result<(), LocalDbError> {
    let mut tx = crate::begin_immediate(pool).await?;
    let result = async {
        require_active_owned_character(&mut tx, owner_creator_id, &record.character_id).await?;
        crate::actor_world_binding::require_valid_provenance_tx(
            &mut tx,
            owner_creator_id,
            &record.character_id,
            record.actor_world_binding_id.as_deref(),
        )
        .await?;
        sqlx::query!(
            "INSERT OR REPLACE INTO character_soul_narratives
             (character_id, actor_world_binding_id, narrative, generated_at, fragment_count_at_generation,
              max_fragment_created_at_at_generation,
              distinct_keyword_count_cache, stats_fingerprint,
              created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            record.character_id,
            record.actor_world_binding_id,
            record.narrative,
            record.generated_at,
            record.fragment_count_at_generation,
            record.max_fragment_created_at_at_generation,
            record.distinct_keyword_count_cache,
            record.stats_fingerprint,
            record.created_at,
            record.updated_at
        )
        .execute(&mut *tx)
        .await?;
        Ok(())
    }
    .await;
    match result {
        Ok(()) => {
            tx.commit().await?;
            Ok(())
        }
        Err(err) => {
            let _ = tx.rollback().await;
            Err(err)
        }
    }
}

/// Distinct-keyword scan for a Character scope, saturated at 20.
///
/// Streams keyword rows one at a time, decoding JSON, accumulating into a
/// `HashSet`. Stops as soon as the gate threshold (20 distinct) is reached
/// and returns the threshold as a saturated count. Bounds recompute cost by
/// the number of rows needed to observe 20 distinct keywords.
async fn compute_distinct_keyword_count(
    pool: &SqlitePool,
    character_id: &str,
    binding_id: Option<&str>,
) -> Result<usize, LocalDbError> {
    const DISTINCT_KEYWORD_THRESHOLD: usize = 20;

    let mut stream = sqlx::query!(
        r#"SELECT keywords as "keywords!" FROM character_memory_fragments
           WHERE character_id = ? AND actor_world_binding_id IS ?
           ORDER BY created_at DESC"#,
        character_id,
        binding_id
    )
    .fetch(pool);

    let mut distinct: std::collections::HashSet<String> = std::collections::HashSet::new();
    while let Some(row) = stream.try_next().await? {
        if let Ok(keywords) = serde_json::from_str::<Vec<String>>(&row.keywords) {
            for kw in keywords {
                distinct.insert(kw);
            }
        }
        if distinct.len() >= DISTINCT_KEYWORD_THRESHOLD {
            return Ok(DISTINCT_KEYWORD_THRESHOLD);
        }
    }

    Ok(distinct.len())
}

/// Persist stats cache columns for a Character scope, creating a stats-only
/// row if none exists.
async fn update_stats_cache(
    pool: &SqlitePool,
    character_id: &str,
    binding_id: Option<&str>,
    distinct_keyword_count: i64,
    fingerprint: &str,
    cached: Option<&CharacterSoulNarrativeRecord>,
) -> Result<(), LocalDbError> {
    if cached.is_some() {
        sqlx::query!(
            "UPDATE character_soul_narratives
             SET distinct_keyword_count_cache = ?, stats_fingerprint = ?
             WHERE character_id = ? AND actor_world_binding_id IS ?",
            distinct_keyword_count,
            fingerprint,
            character_id,
            binding_id
        )
        .execute(pool)
        .await?;
    } else {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query!(
            "INSERT OR REPLACE INTO character_soul_narratives
             (character_id, actor_world_binding_id, narrative, generated_at, fragment_count_at_generation,
              max_fragment_created_at_at_generation,
              distinct_keyword_count_cache, stats_fingerprint,
              created_at, updated_at)
             VALUES (?, ?, NULL, NULL, 0, NULL, ?, ?, ?, ?)",
            character_id,
            binding_id,
            distinct_keyword_count,
            fingerprint,
            now,
            now
        )
        .execute(pool)
        .await?;
    }
    Ok(())
}

/// Compute fragment statistics for a Character scope, honoring the
/// fingerprint cache.
///
/// Mirrors [`crate::soul_narrative::soul_narrative_fragment_stats`] over the
/// Character tables: cheap SQL aggregates build a fingerprint; on a cache hit
/// the cached distinct count is returned with no keyword decode; on a miss the
/// distinct count is computed soundly (threshold-saturated) and persisted.
///
/// `binding_id = None` computes over the shared Character scope (NULL
/// binding); `Some(b)` over that binding's fragment subset.
///
/// # Errors
///
/// Returns `LocalDbError::ActorNotFound` when the Character (or, for a
/// binding-scoped read, the binding) is missing, foreign, or inactive;
/// `LocalDbError` on database failure.
pub async fn character_soul_narrative_fragment_stats(
    pool: &SqlitePool,
    owner_creator_id: &str,
    character_id: &str,
    binding_id: Option<&str>,
) -> Result<
    (
        SoulNarrativeFragmentStats,
        Option<CharacterSoulNarrativeRecord>,
    ),
    LocalDbError,
> {
    require_owned_character_pool(pool, owner_creator_id, character_id).await?;
    if let Some(binding_id) = binding_id {
        require_active_owned_provenance_pool(pool, owner_creator_id, character_id, binding_id)
            .await?;
    }

    // 1. Cheap SQL aggregates — O(1) index scan, no row materialization.
    let fragment_count = sqlx::query_scalar!(
        r#"SELECT COUNT(*) as "count!: i64" FROM character_memory_fragments
           WHERE character_id = ? AND actor_world_binding_id IS ?"#,
        character_id,
        binding_id
    )
    .fetch_one(pool)
    .await?;

    let max_created_at: Option<String> = sqlx::query_scalar!(
        r#"SELECT MAX(created_at) as "max_created_at?: String" FROM character_memory_fragments
           WHERE character_id = ? AND actor_world_binding_id IS ?"#,
        character_id,
        binding_id
    )
    .fetch_one(pool)
    .await?;

    // 2. Build fingerprint from cheap aggregates.
    let fingerprint = build_stats_fingerprint(fragment_count, max_created_at.as_deref());

    // 3. Check the fingerprint cache for this Character scope.
    let cached = load_narrative(pool, character_id, binding_id).await?;

    if let Some(ref c) = cached {
        if c.stats_fingerprint.as_deref() == Some(&fingerprint) {
            return Ok((
                SoulNarrativeFragmentStats {
                    fragment_count,
                    distinct_keyword_count: usize::try_from(c.distinct_keyword_count_cache)
                        .unwrap_or(0),
                    max_created_at,
                },
                cached,
            ));
        }
    }

    // 4. Fingerprint mismatch or no cache row → compute soundly.
    let distinct_keyword_count =
        compute_distinct_keyword_count(pool, character_id, binding_id).await?;

    // 5. Persist stats cache.
    update_stats_cache(
        pool,
        character_id,
        binding_id,
        i64::try_from(distinct_keyword_count).unwrap_or(0),
        &fingerprint,
        cached.as_ref(),
    )
    .await?;

    let cached = if cached.is_some() {
        cached
    } else {
        load_narrative(pool, character_id, binding_id).await?
    };

    Ok((
        SoulNarrativeFragmentStats {
            fragment_count,
            distinct_keyword_count,
            max_created_at,
        },
        cached,
    ))
}
