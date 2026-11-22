//! Shared test helpers for seeding test data.
// note: Shared test helpers for seeding test data.
//!
//! Provides common seed functions used by both `narrative_gateway` and
//! `kb_store` seed submodules. These helpers use runtime `sqlx::query()`
//! (with SAFETY comments) to avoid duplicating sqlx cache entries across
//! crates.

use sqlx::SqlitePool;

/// note: The seedable columns of a `narrative_worlds` test row.
///
/// Build one with [`WorldSeed::new`], name only the values the fixture varies
/// through the `with_*` methods, and hand it to `seed::world_with`. The
/// defaults are the values the hand-written copies shared: `workspace_id =
/// "wrk_test"`, `visibility = "private"`, `time_policy = "manual"`, no display
/// name (the seed keeps the owner's id), and no timeline columns.
///
/// The four identifying values have no default — a seed without a `world_id`,
/// owner, title or slug is not a row any fixture wants.
pub struct WorldSeed<'a> {
    /// note: `narrative_worlds.world_id`.
    pub world_id: &'a str,
    /// note: `owner_creator_id`; also the minimal creator row's key.
    pub owner_creator_id: &'a str,
    /// note: `narrative_worlds.title`.
    pub title: &'a str,
    /// note: `narrative_worlds.slug`.
    pub slug: &'a str,
    /// note: `visibility`; defaults to `"private"`.
    pub visibility: &'a str,
    /// note: `time_policy`; defaults to `"manual"`.
    pub time_policy: &'a str,
    /// note: The workspace the row belongs to; defaults to `"wrk_test"`.
    pub workspace_id: &'a str,
    /// note: The minimal creator's display name; `None` keeps the owner's id.
    pub display_name: Option<&'a str>,
    /// note: The world's root fork branch, when the fixture drives a timeline.
    pub root_fork_branch_id: Option<&'a str>,
    /// note: The world's current timeline head, when the fixture drives a timeline.
    pub current_timeline_head_id: Option<&'a str>,
}

impl<'a> WorldSeed<'a> {
    /// note: A seed for the given row identity; every other column keeps the value
    /// the hand-written copies used.
    ///
    /// The four values are positional and required: a fixture cannot forget one
    /// and silently seed an empty string.
    #[must_use]
    pub const fn new(
        world_id: &'a str,
        owner_creator_id: &'a str,
        title: &'a str,
        slug: &'a str,
    ) -> Self {
        Self {
            world_id,
            owner_creator_id,
            title,
            slug,
            visibility: "private",
            time_policy: "manual",
            workspace_id: "wrk_test",
            display_name: None,
            root_fork_branch_id: None,
            current_timeline_head_id: None,
        }
    }

    /// note: The workspace the row belongs to.
    #[must_use]
    pub const fn with_workspace_id(mut self, workspace_id: &'a str) -> Self {
        self.workspace_id = workspace_id;
        self
    }

    /// note: `visibility`.
    #[must_use]
    pub const fn with_visibility(mut self, visibility: &'a str) -> Self {
        self.visibility = visibility;
        self
    }

    /// note: `time_policy`.
    #[must_use]
    pub const fn with_time_policy(mut self, time_policy: &'a str) -> Self {
        self.time_policy = time_policy;
        self
    }

    /// note: The minimal creator's display name.
    #[must_use]
    pub const fn with_display_name(mut self, display_name: &'a str) -> Self {
        self.display_name = Some(display_name);
        self
    }

    /// note: The world's root fork branch.
    #[must_use]
    pub const fn with_root_fork_branch_id(mut self, root_fork_branch_id: &'a str) -> Self {
        self.root_fork_branch_id = Some(root_fork_branch_id);
        self
    }

    /// note: The world's current timeline head.
    #[must_use]
    pub const fn with_current_timeline_head_id(
        mut self,
        current_timeline_head_id: &'a str,
    ) -> Self {
        self.current_timeline_head_id = Some(current_timeline_head_id);
        self
    }
}

/// note: Seed a test world row (also seeds a minimal creator for FK).
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
        WorldSeed::new(world_id, owner_creator_id, title, slug)
            .with_visibility(visibility)
            .with_time_policy(time_policy),
    )
    .await;
}

/// note: Seed a world row from a [`WorldSeed`] — the one INSERT every entry point
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
