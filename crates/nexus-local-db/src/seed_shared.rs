//! Shared test helpers for seeding test data.
//!
//! Provides common seed functions used by both `narrative_gateway` and
//! `kb_store` seed submodules. These helpers use runtime `sqlx::query()`
//! (with SAFETY comments) to avoid duplicating sqlx cache entries across
//! crates.

use sqlx::SqlitePool;

/// Seed a test world row (also seeds a minimal creator for FK).
///
/// Creates a minimal creator row for FK satisfaction if it does not exist.
///
/// # Panics
///
/// Panics if either database insert fails.
pub async fn world(
    pool: &SqlitePool,
    world_id: &str,
    owner_creator_id: &str,
    title: &str,
    slug: &str,
    visibility: &str,
    time_policy: &str,
) {
    // SAFETY: test-only seed helper — uses runtime query to avoid
    // duplicating sqlx cache entries across crates.
    sqlx::query(
        "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) VALUES (?, ?, 'active', datetime('now'), '{}')",
    )
    .bind(owner_creator_id)
    .bind(owner_creator_id)
    .execute(pool)
    .await
    .unwrap();

    // SAFETY: test-only seed helper — uses runtime query to avoid
    // duplicating sqlx cache entries across crates.
    sqlx::query(
        "INSERT INTO narrative_worlds
            (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, time_policy, metadata_json)
           VALUES (?, 'wrk_test', ?, ?, ?, 'active', ?, ?, '{}')",
    )
    .bind(world_id)
    .bind(owner_creator_id)
    .bind(title)
    .bind(slug)
    .bind(visibility)
    .bind(time_policy)
    .execute(pool)
    .await
    .unwrap();
}
