//! Shared test helpers for seeding test data.
//!
//! Provides common seed functions used by both `narrative_gateway` and
//! `kb_store` seed submodules. These helpers use runtime `sqlx::query()`
//! (with SAFETY comments) to avoid duplicating sqlx cache entries across
//! crates.

use sqlx::SqlitePool;

/// The seedable columns of a `narrative_worlds` test row.
///
/// A fixture names only the values it varies and takes the rest from
/// [`WorldSeed::default`], so a difference between fixtures is a field rather
/// than a forked INSERT. The defaults are the values the hand-written copies
/// shared: `workspace_id = "wrk_test"`, `visibility = "private"`,
/// `time_policy = "manual"`, no display name (the seed keeps the owner's id),
/// and no timeline columns.
///
/// `world_id`, `owner_creator_id`, `title` and `slug` default to `""`; every
/// call site names them.
pub struct WorldSeed<'a> {
    /// `narrative_worlds.world_id`.
    pub world_id: &'a str,
    /// `owner_creator_id`; also the minimal creator row's key.
    pub owner_creator_id: &'a str,
    /// `narrative_worlds.title`.
    pub title: &'a str,
    /// `narrative_worlds.slug`.
    pub slug: &'a str,
    /// `visibility`; defaults to `"private"`.
    pub visibility: &'a str,
    /// `time_policy`; defaults to `"manual"`.
    pub time_policy: &'a str,
    /// The workspace the row belongs to; defaults to `"wrk_test"`.
    pub workspace_id: &'a str,
    /// The minimal creator's display name; `None` keeps the owner's id.
    pub display_name: Option<&'a str>,
    /// The world's root fork branch, when the fixture drives a timeline.
    pub root_fork_branch_id: Option<&'a str>,
    /// The world's current timeline head, when the fixture drives a timeline.
    pub current_timeline_head_id: Option<&'a str>,
}

impl<'a> Default for WorldSeed<'a> {
    fn default() -> Self {
        Self {
            world_id: "",
            owner_creator_id: "",
            title: "",
            slug: "",
            visibility: "private",
            time_policy: "manual",
            workspace_id: "wrk_test",
            display_name: None,
            root_fork_branch_id: None,
            current_timeline_head_id: None,
        }
    }
}

/// Seed a test world row (also seeds a minimal creator for FK).
///
/// Creates a minimal creator row for FK satisfaction if it does not exist,
/// with the owner's id as its display name.
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
    world_with(
        pool,
        WorldSeed {
            world_id,
            owner_creator_id,
            title,
            slug,
            visibility,
            time_policy,
            ..WorldSeed::default()
        },
    )
    .await;
}

/// [`world`] with an explicit creator display name.
///
/// A fixture that pins the display name passes it here rather than forking the
/// inserts; [`world`] keeps the creator id, so its callers seed the row they
/// always did.
///
/// # Panics
///
/// Panics if either database insert fails.
#[allow(clippy::too_many_arguments)] // kept for the call sites that predate `WorldSeed`
pub async fn world_with_display_name(
    pool: &SqlitePool,
    world_id: &str,
    owner_creator_id: &str,
    display_name: &str,
    title: &str,
    slug: &str,
    visibility: &str,
    time_policy: &str,
) {
    world_with(
        pool,
        WorldSeed {
            world_id,
            owner_creator_id,
            title,
            slug,
            visibility,
            time_policy,
            display_name: Some(display_name),
            ..WorldSeed::default()
        },
    )
    .await;
}

/// Seed a world row from a [`WorldSeed`] — the one INSERT every entry point
/// above delegates to.
///
/// # Panics
///
/// Panics if either database insert fails.
pub async fn world_with(pool: &SqlitePool, seed: WorldSeed<'_>) {
    // SAFETY: test-only seed helper — uses runtime query to avoid
    // duplicating sqlx cache entries across crates.
    sqlx::query(
        "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) VALUES (?, ?, 'active', datetime('now'), '{}')",
    )
    .bind(seed.owner_creator_id)
    .bind(seed.display_name.unwrap_or(seed.owner_creator_id))
    .execute(pool)
    .await
    .unwrap();

    // SAFETY: test-only seed helper — uses runtime query to avoid
    // duplicating sqlx cache entries across crates.
    sqlx::query(
        "INSERT INTO narrative_worlds
            (world_id, workspace_id, owner_creator_id, title, slug, status, visibility,
             time_policy, root_fork_branch_id, current_timeline_head_id, metadata_json)
           VALUES (?, ?, ?, ?, ?, 'active', ?, ?, ?, ?, '{}')",
    )
    .bind(seed.world_id)
    .bind(seed.workspace_id)
    .bind(seed.owner_creator_id)
    .bind(seed.title)
    .bind(seed.slug)
    .bind(seed.visibility)
    .bind(seed.time_policy)
    .bind(seed.root_fork_branch_id)
    .bind(seed.current_timeline_head_id)
    .execute(pool)
    .await
    .unwrap();
}
