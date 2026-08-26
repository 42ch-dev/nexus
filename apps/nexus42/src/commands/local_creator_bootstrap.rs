//! Shared local-creator bootstrap helper (V1.176 P0 T1, AR-88) + the global
//! identity-store accessors it needs.
//!
//! V1.176 P0 QC fix wave (qc1 W#2): the helper used to live in
//! `commands/creator/bootstrap.rs` — a module titled for the V1.45 Work
//! onboarding composite command — which created a crate-internal cycle
//! (`system::identity` → `creator::bootstrap` → `system::identity`). It
//! moved here so identity-mint logic lives in a module about identity-mint,
//! and `commands::system::identity` depends on this module one-way.
//!
//! This module owns the single identity-mint + workspace-row materialization
//! sequence ([`bootstrap_local_creator`]) both named local entry points
//! (`creator register --local --name <n>`, `system identity create
//! --persistent [--name <n>]`) call, plus the global identity store accessors
//! ([`open_global_db`] / [`global_db_path`] / [`open_global_db_read_only`])
//! that used to live in `commands::system::identity`. It also owns the
//! display-name validation seam ([`validate_display_name`]) so the rejection
//! literal lives once (qc1 S#1/S#6, qc2 S#4).

use crate::config::{self, CliConfig};
use crate::errors::{CliError, Result};
use nexus_creator::local_identity::LocalIdentity;
use nexus_local_db::create_local_identity;

/// Maximum length for a creator display name (WS-B T4) — the single bound
/// shared by the platform register path and the local bootstrap helper, so
/// both sides reject the same over-long display token (qc2 S#4 parity).
pub(crate) const MAX_CREATOR_NAME_LENGTH: usize = 64;

/// Validate a display name at the identity front door — the one copy of the
/// rejection logic (qc1 S#1/S#6). Rejects:
/// - empty / whitespace-only names (R3, AR-89 #1);
/// - names containing control characters — including internal newlines / NULs
///   that would break the human `creator list` table and the collision stderr
///   line (qc2 S#4);
/// - names longer than [`MAX_CREATOR_NAME_LENGTH`] bytes (parity with the
///   platform register's WS-B T4 bound).
///
/// Returns the trimmed name for `Some(<non-empty, clean, bounded>)` input,
/// `Ok(None)` when no name was provided (nameless mint). Byte-exactness is
/// preserved — no case-fold, no Unicode normalization (AR-89 #1).
pub(crate) fn validate_display_name(name: Option<&str>) -> Result<Option<&str>> {
    let Some(raw) = name else {
        return Ok(None);
    };
    if raw.trim().is_empty() {
        return Err(CliError::Other(
            "Display name cannot be empty or whitespace-only.".to_string(),
        ));
    }
    if raw.chars().any(char::is_control) {
        return Err(CliError::Other(
            "Display name cannot contain control characters.".to_string(),
        ));
    }
    if raw.len() > MAX_CREATOR_NAME_LENGTH {
        return Err(CliError::Other(format!(
            "Creator name exceeds maximum length ({MAX_CREATOR_NAME_LENGTH} bytes)"
        )));
    }
    Ok(Some(raw.trim()))
}

/// Resolve the global identity database path at `~/.nexus42/state.db`.
///
/// `pub(crate)`: read-only surfaces (e.g. `creator list`) check existence via
/// this path so they do not materialize the db when there are no local rows
/// to merge.
pub(crate) fn global_db_path() -> Result<std::path::PathBuf> {
    let home = config::user_home_dir()?;
    Ok(home.join(".nexus42").join("state.db"))
}

/// Open or create the global identity database at `~/.nexus42/state.db`.
///
/// Writable open: creates `~/.nexus42/`, runs migrations, and seeds version
/// keys (`Schema::init`) — used by mint legs that actually insert. Read-only
/// surfaces and the fully-converged no-op verification use
/// [`open_global_db_read_only`] instead so they never write `workspace_meta`
/// seed-version keys (qc3 F-002).
pub(crate) async fn open_global_db() -> Result<nexus_local_db::SqlitePool> {
    let db_path = global_db_path()?;

    // Ensure ~/.nexus42/ exists
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    crate::db::Schema::init(&db_path)
        .await
        .map_err(CliError::from)
}

/// Open the global identity database read-only (`mode=ro`), without
/// migrations or seed-version writes.
///
/// Used by read-only verification legs (the fully-converged no-op, qc3
/// F-002) and read surfaces like `creator list` (qc3 S-003). Fails honestly
/// (no dir creation, no migrations) when the store is absent, locked, or
/// corrupt.
pub(crate) async fn open_global_db_read_only() -> Result<nexus_local_db::SqlitePool> {
    let db_path = global_db_path()?;
    nexus_local_db::open_pool_read_only(&db_path)
        .await
        .map_err(CliError::from)
}

/// Shared local-creator bootstrap helper (V1.176 P0 T1, AR-88).
///
/// Owns the single identity-mint + workspace-row materialization sequence
/// for both named local entry points: `creator register --local --name <n>`
/// and `system identity create --persistent [--name <n>]`. There is exactly
/// one minting + materialization sequence in the crate.
///
/// Converged end state (compass PL-3, checked by tests, not implied):
/// 1. a persistent `ctr_local*` row in `~/.nexus42/state.db` `local_identities`;
/// 2. that id is `active_creator_id` in the CLI config;
/// 3. the workspace `creators` row exists in the per-creator+workspace db
///    resolved by `config::resolve_state_db_path` (the same db `creator world
///    create` FK-prechecks), written via `nexus_local_db::ensure_creator_row`.
///
/// The `creator-identities.json` cache is **not** written (AR-88 #3): it is
/// best-effort display metadata for the platform path only; local display
/// SSOT is `local_identities`.
///
/// Re-entrancy is idempotent (AR-88 #6 / AR-89): a crash between stores
/// leaves exactly the DF-83 partial (identity without row), which the next
/// run repairs — the named 1-match leg converges that id (row upsert +
/// activation), and the nameless path converges the already-active
/// persistent identity. A name shared by 2+ persistent identities is an
/// honest `creator_name_collision` error, never a silent takeover.
///
/// # Errors
///
/// Returns `CliError` if the identity database, config, or workspace db
/// operations fail, if the display name fails validation, or if the display
/// name collides with 2+ existing persistent identities
/// (`CreatorNameCollision`).
pub(crate) async fn bootstrap_local_creator(name: Option<String>) -> Result<()> {
    // Validation lives once at the helper front door (qc1 S#1/S#6, qc2 S#4):
    // empty/whitespace-only, control chars (incl. internal newlines), and
    // over-length names are rejected here — both named local entry points
    // route through this seam (AR-89 #1).
    let trimmed_name = validate_display_name(name.as_deref())?;

    // qc3 F-002: read-only decision tree. When the global identity store
    // already exists we read it through a `mode=ro` pool (no migrations, no
    // seed-version writes) so a fully-converged no-op never writes
    // `workspace_meta` keys. The writable `open_global_db` (Schema::init) is
    // used only on mint legs that actually insert a row.
    if let Some(trimmed) = trimmed_name {
        let matches = if global_db_path()?.exists() {
            let read_only_pool = open_global_db_read_only().await?;
            persistent_rows_with_name(&read_only_pool, trimmed).await?
        } else {
            Vec::new()
        };
        match matches.len() {
            0 => {
                let pool = open_global_db().await?;
                mint_and_materialize(&pool, Some(trimmed)).await
            }
            1 => converge_identity(&matches[0]).await,
            _ => Err(CliError::CreatorNameCollision {
                display_name: trimmed.to_string(),
                matches: matches.into_iter().map(|r| r.creator_id).collect(),
            }),
        }
    } else {
        let cli_config = CliConfig::load()?;
        if let Some(active_id) = &cli_config.active_creator_id {
            if global_db_path()?.exists() {
                let read_only_pool = open_global_db_read_only().await?;
                if let Some(row) =
                    nexus_local_db::get_local_identity(&read_only_pool, active_id).await?
                {
                    if row.identity_type == "persistent" {
                        return converge_identity(&row).await;
                    }
                }
            }
        }
        let pool = open_global_db().await?;
        mint_and_materialize(&pool, None).await
    }
}

/// Persistent `local_identities` rows whose display name is byte-exactly
/// `trimmed` (AR-89 #1: `str::trim` + `==` — no case-fold, no Unicode
/// normalization, no prefix/substring matching).
async fn persistent_rows_with_name(
    pool: &nexus_local_db::SqlitePool,
    trimmed: &str,
) -> Result<Vec<nexus_local_db::LocalIdentityRow>> {
    let rows = nexus_local_db::list_local_identities(pool).await?;
    Ok(rows
        .into_iter()
        .filter(|r| r.identity_type == "persistent" && r.display_name.as_deref() == Some(trimmed))
        .collect())
}

/// AR-89 mint leg: fresh persistent identity + INSERT + set active +
/// `ensure_creator_row`. Prints the recognizable "Created persistent
/// identity: …" shape (AR-88 #5 / AR-89 #5) — only after all three stores
/// have committed (qc2 S#5).
async fn mint_and_materialize(
    pool: &nexus_local_db::SqlitePool,
    trimmed_name: Option<&str>,
) -> Result<()> {
    let identity = LocalIdentity::create_persistent(trimmed_name);
    if let Err(err) = create_local_identity(
        pool,
        &identity.creator_id,
        identity.identity_type.as_str(),
        identity.display_name.as_deref(),
        &identity.created_at,
    )
    .await
    {
        // qc3 S-002: the unique partial index on persistent `display_name`
        // rejected this INSERT — a concurrent process minted the same name
        // between our 0-match read and this insert (TOCTOU). Surface the same
        // honest collision the 2+ decision-tree leg produces.
        if let nexus_local_db::LocalDbError::Sqlx(sqlx::Error::Database(db_err)) = &err {
            if db_err.is_unique_violation() {
                let display_name = identity
                    .display_name
                    .clone()
                    .unwrap_or_else(|| identity.creator_id.clone());
                let matches = persistent_rows_with_name(pool, &display_name).await?;
                return Err(CliError::CreatorNameCollision {
                    display_name,
                    matches: matches.into_iter().map(|r| r.creator_id).collect(),
                });
            }
        }
        return Err(err.into());
    }

    // Set as active creator (store 2 of 3).
    let mut cli_config = CliConfig::load()?;
    cli_config.active_creator_id = Some(identity.creator_id.clone());
    cli_config.save()?;

    // Materialize the workspace `creators` row (store 3 of 3) in the same
    // per-creator+workspace db `creator world create` FK-prechecks.
    let db_path = crate::config::resolve_state_db_path(&cli_config)?;
    let workspace_pool = crate::db::Schema::init(&db_path).await?;
    let row_display_name = identity
        .display_name
        .clone()
        .unwrap_or_else(|| identity.creator_id.clone());
    nexus_local_db::ensure_creator_row(&workspace_pool, &identity.creator_id, &row_display_name)
        .await?;

    // qc2 S#5: announce only after all three stores committed — a failure on
    // any store exits non-zero with no "Created" claim on stdout. The wording
    // is unchanged on success (AR-88 #5 / AR-89 #5).
    println!("Created persistent identity: {}", identity.creator_id);
    if let Some(name) = &identity.display_name {
        println!("  Name: {name}");
    }
    println!("  Stored in ~/.nexus42/state.db");
    println!("  Set as active identity.");

    Ok(())
}

/// AR-89 no-op / repair leg: converge `row`'s identity — ensure the
/// workspace `creators` row (repair if missing) and activate if not already
/// active (session selection). Never prints "Created" (PL-5 hard pin).
async fn converge_identity(row: &nexus_local_db::LocalIdentityRow) -> Result<()> {
    let creator_id = &row.creator_id;
    let row_display_name = row
        .display_name
        .clone()
        .unwrap_or_else(|| creator_id.clone());

    let mut cli_config = CliConfig::load()?;
    let already_active = cli_config.active_creator_id.as_deref() == Some(creator_id.as_str());
    if !already_active {
        cli_config.active_creator_id = Some(creator_id.clone());
    }

    // The workspace db is per-creator (ADR-014): resolve it for the matched
    // identity, not the currently-active one.
    let db_path = crate::config::resolve_state_db_path(&cli_config)?;

    // qc3 F-002: the fully-converged no-op must be db-write-free. When the
    // per-creator+workspace db already exists, verify the `creators` row
    // through a `mode=ro` pool (no migrations, no seed-version writes) and
    // return without touching `Schema::init` when the row is present + active.
    if db_path.exists() {
        let read_only_pool = nexus_local_db::open_pool_read_only(&db_path)
            .await
            .map_err(CliError::from)?;
        // Bugbot Medium (V1.176 PR wave): static SQL must use the
        // compile-time macro (SQLX_OFFLINE=true drift gate) — the runtime
        // `query_scalar` form is unchecked against the schema.
        let row_exists: i64 = sqlx::query_scalar!(
            "SELECT EXISTS(SELECT 1 FROM creators WHERE creator_id = ? AND status = 'active')",
            creator_id
        )
        .fetch_one(&read_only_pool)
        .await?;
        if row_exists == 1 && already_active {
            println!(
                "Identity {creator_id} is already converged (active + workspace row present)."
            );
            return Ok(());
        }
    }

    let workspace_pool = crate::db::Schema::init(&db_path).await?;
    let row_exists: i64 = sqlx::query_scalar!(
        "SELECT EXISTS(SELECT 1 FROM creators WHERE creator_id = ? AND status = 'active')",
        creator_id
    )
    .fetch_one(&workspace_pool)
    .await?;
    let repaired = row_exists == 0;
    // True no-op: row present + already active → read-only verification, no
    // workspace-row write (no `cached_at` churn). Write only when the row is
    // missing (repair) or a different identity is being activated (session
    // selection).
    if repaired || !already_active {
        nexus_local_db::ensure_creator_row(&workspace_pool, creator_id, &row_display_name).await?;
    }

    if !already_active {
        cli_config.save()?;
    }

    if repaired {
        println!("Workspace creators row materialized for {creator_id}.");
    } else if !already_active {
        println!("Identity {creator_id} is already registered; set as active identity.");
    } else {
        println!("Identity {creator_id} is already converged (active + workspace row present).");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── V1.176 P0 T1 (AR-88): shared bootstrap helper ──────────────

    /// The helper converges all three stores (compass PL-3): a persistent
    /// `ctr_local*` row in `local_identities`, that id as `active_creator_id`,
    /// and the workspace `creators` row in the per-creator+workspace db —
    /// so `creator world create` succeeds immediately after.
    #[tokio::test]
    async fn bootstrap_local_creator_converges_three_stores() {
        let _home = crate::testutil::isolated_home();

        bootstrap_local_creator(Some("  Alice  ".to_string()))
            .await
            .expect("bootstrap should succeed");

        // Store 1: persistent row in the global identity store.
        let pool = open_global_db().await.expect("open global db");
        let identities = nexus_local_db::list_local_identities(&pool)
            .await
            .expect("list local identities");
        assert_eq!(identities.len(), 1, "exactly one identity minted");
        let row = &identities[0];
        assert!(
            row.creator_id.starts_with("ctr_local"),
            "expected ctr_local* id, got {}",
            row.creator_id
        );
        assert_eq!(row.identity_type, "persistent");
        assert_eq!(
            row.display_name.as_deref(),
            Some("Alice"),
            "R3-trimmed name"
        );

        // Store 2: active creator id in the CLI config.
        let config = CliConfig::load().expect("reload config");
        assert_eq!(
            config.active_creator_id.as_deref(),
            Some(row.creator_id.as_str()),
            "minted id must be active"
        );

        // Store 3: workspace `creators` row in the same db `creator world
        // create` FK-prechecks.
        let db_path = crate::config::resolve_state_db_path(&config).expect("resolve state db path");
        let workspace_pool = crate::db::Schema::init(&db_path)
            .await
            .expect("init workspace pool");
        let creator_exists: i64 = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM creators WHERE creator_id = ? AND status = 'active')",
        )
        .bind(&row.creator_id)
        .fetch_one(&workspace_pool)
        .await
        .expect("query workspace creators row");
        assert_eq!(
            creator_exists, 1,
            "workspace creators row must exist for the bootstrapped creator"
        );

        // `creator world create` succeeds immediately (no FK miss).
        let result = nexus_local_db::create_world(
            &workspace_pool,
            &row.creator_id,
            "Test World",
            "test-world",
            "public",
            "manual",
        )
        .await
        .expect("create_world must succeed after bootstrap");
        assert!(result.world_id.starts_with("wld_"));
    }

    /// Nameless mint: the workspace row `display_name` falls back to the
    /// `creator_id` string itself (AR-88 #4 — never empty).
    #[tokio::test]
    async fn bootstrap_local_creator_nameless_uses_creator_id_as_display_name() {
        let _home = crate::testutil::isolated_home();

        bootstrap_local_creator(None)
            .await
            .expect("bootstrap succeeds");

        let pool = open_global_db().await.expect("open global db");
        let identities = nexus_local_db::list_local_identities(&pool)
            .await
            .expect("list local identities");
        assert_eq!(identities.len(), 1);
        let row = &identities[0];
        assert!(row.display_name.is_none(), "nameless mint stores no name");

        let config = CliConfig::load().expect("reload config");
        let db_path = crate::config::resolve_state_db_path(&config).expect("resolve state db path");
        let workspace_pool = crate::db::Schema::init(&db_path)
            .await
            .expect("init workspace pool");
        let display_name: String =
            sqlx::query_scalar("SELECT display_name FROM creators WHERE creator_id = ?")
                .bind(&row.creator_id)
                .fetch_one(&workspace_pool)
                .await
                .expect("query creators display_name");
        assert_eq!(
            display_name, row.creator_id,
            "nameless mint row display_name = creator_id (AR-88 #4)"
        );
    }

    /// The identity-cache store (`creator-identities.json`) is NOT written
    /// (AR-88 #3): local identities carry no handle; display SSOT is
    /// `local_identities`.
    #[tokio::test]
    async fn bootstrap_local_creator_does_not_write_identity_cache() {
        let _home = crate::testutil::isolated_home();

        bootstrap_local_creator(Some("Cache Free".to_string()))
            .await
            .expect("bootstrap succeeds");

        let cache_path = crate::creator_identity::cache_path().expect("cache path");
        assert!(
            !cache_path.exists(),
            "creator-identities.json must not be written by the local bootstrap"
        );
    }

    /// Whitespace-only names are rejected at the helper front door (R3).
    #[tokio::test]
    async fn bootstrap_local_creator_rejects_whitespace_only_name() {
        let _home = crate::testutil::isolated_home();

        let err = bootstrap_local_creator(Some("   ".to_string()))
            .await
            .expect_err("whitespace-only name must be rejected");
        let display = format!("{err}");
        assert!(
            display.contains("Display name cannot be empty or whitespace-only."),
            "unexpected error: {display}"
        );
    }

    /// qc2 S#4: names with internal control characters (e.g. newlines) are
    /// rejected at the helper front door — a newline would break the human
    /// `creator list` table and the collision stderr line.
    #[tokio::test]
    async fn bootstrap_local_creator_rejects_name_with_control_character() {
        let _home = crate::testutil::isolated_home();

        let err = bootstrap_local_creator(Some("Alice\nBob".to_string()))
            .await
            .expect_err("name with internal newline must be rejected");
        let display = format!("{err}");
        assert!(
            display.contains("control character"),
            "unexpected error: {display}"
        );
    }

    /// qc2 S#4: names longer than `MAX_CREATOR_NAME_LENGTH` (64, parity with
    /// the platform register's WS-B T4 bound) are rejected at the helper
    /// front door.
    #[tokio::test]
    async fn bootstrap_local_creator_rejects_overlong_name() {
        let _home = crate::testutil::isolated_home();

        let long_name = "a".repeat(65);
        let err = bootstrap_local_creator(Some(long_name))
            .await
            .expect_err("over-long name must be rejected");
        let display = format!("{err}");
        assert!(
            display.contains("exceeds maximum length"),
            "unexpected error: {display}"
        );
    }

    // ── V1.176 P0 T2 (AR-89): idempotent re-register + partial-bootstrap recovery ──

    /// No-op success: re-running the same name against the already-converged
    /// identity must not mint a second identity (stdout honesty — no
    /// "Created" — is pinned at the e2e level where stdout is captured).
    #[tokio::test]
    async fn bootstrap_local_creator_noop_keeps_single_identity() {
        let _home = crate::testutil::isolated_home();

        bootstrap_local_creator(Some("  Alice  ".to_string()))
            .await
            .expect("first bootstrap mints");
        let pool = open_global_db().await.expect("open global db");
        let first = nexus_local_db::list_local_identities(&pool)
            .await
            .expect("list local identities");
        assert_eq!(first.len(), 1);
        let first_id = first[0].creator_id.clone();

        bootstrap_local_creator(Some("Alice".to_string()))
            .await
            .expect("re-run converges, no collision");

        let after = nexus_local_db::list_local_identities(&pool)
            .await
            .expect("list local identities");
        assert_eq!(after.len(), 1, "no second identity minted");
        assert_eq!(after[0].creator_id, first_id, "same identity id");

        // Still fully converged: active + workspace row present.
        let config = CliConfig::load().expect("reload config");
        assert_eq!(config.active_creator_id.as_deref(), Some(first_id.as_str()));
        let db_path = crate::config::resolve_state_db_path(&config).expect("resolve state db path");
        let workspace_pool = crate::db::Schema::init(&db_path)
            .await
            .expect("init workspace pool");
        let row_exists: i64 = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM creators WHERE creator_id = ? AND status = 'active')",
        )
        .bind(&first_id)
        .fetch_one(&workspace_pool)
        .await
        .expect("query workspace creators row");
        assert_eq!(row_exists, 1, "workspace row still present");
    }

    /// Store-level no-op pin: the no-op leg must not UPDATE the workspace
    /// `creators` row (no `cached_at` churn). A sentinel `cached_at` survives
    /// the re-run untouched.
    #[tokio::test]
    async fn bootstrap_local_creator_noop_does_not_touch_workspace_row() {
        let _home = crate::testutil::isolated_home();

        bootstrap_local_creator(Some("Alice".to_string()))
            .await
            .expect("first bootstrap mints");
        let pool = open_global_db().await.expect("open global db");
        let identities = nexus_local_db::list_local_identities(&pool)
            .await
            .expect("list local identities");
        assert_eq!(identities.len(), 1);
        let id = identities[0].creator_id.clone();

        // Stamp the workspace row with a sentinel timestamp; a no-op re-run
        // must leave it untouched (read-only verification).
        let config = CliConfig::load().expect("reload config");
        let db_path = crate::config::resolve_state_db_path(&config).expect("resolve state db path");
        let workspace_pool = crate::db::Schema::init(&db_path)
            .await
            .expect("init workspace pool");
        sqlx::query("UPDATE creators SET cached_at = ? WHERE creator_id = ?")
            .bind("2000-01-01T00:00:00Z")
            .bind(&id)
            .execute(&workspace_pool)
            .await
            .expect("stamp sentinel cached_at");

        bootstrap_local_creator(Some("Alice".to_string()))
            .await
            .expect("no-op re-run converges");

        let cached_at: String =
            sqlx::query_scalar("SELECT cached_at FROM creators WHERE creator_id = ?")
                .bind(&id)
                .fetch_one(&workspace_pool)
                .await
                .expect("query cached_at");
        assert_eq!(
            cached_at, "2000-01-01T00:00:00Z",
            "no-op must not rewrite the workspace row (cached_at churn)"
        );
    }

    /// qc3 F-002 sentinel: a fully-converged no-op re-run must be
    /// db-write-free — no `workspace_meta` seed-version churn on the global
    /// identity store, no `creators` churn on the workspace db. Pinned via
    /// file size + mtime of both `state.db` files across the no-op, plus
    /// absent/zero-size `-wal` files after it (qc3 R-2 — WAL-only writes).
    #[tokio::test]
    async fn bootstrap_local_creator_noop_is_write_free() {
        let _home = crate::testutil::isolated_home();

        bootstrap_local_creator(Some("Alice".to_string()))
            .await
            .expect("first bootstrap mints");

        let global_path = global_db_path().expect("global db path");
        let config = CliConfig::load().expect("reload config");
        let workspace_path =
            crate::config::resolve_state_db_path(&config).expect("workspace db path");
        assert!(global_path.exists(), "global db materialized");
        assert!(workspace_path.exists(), "workspace db materialized");

        // Force a full WAL checkpoint before fingerprinting. sqlx keeps a
        // writable pool alive past the handle drop, so the first mint's
        // freshly-written pages can still sit in `-wal`; without this the
        // later deferred checkpoint would be miscounted as a no-op write.
        for db_path in [&global_path, &workspace_path] {
            let pool = nexus_local_db::open_pool(db_path)
                .await
                .expect("open for checkpoint");
            sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
                .fetch_all(&pool)
                .await
                .expect("force WAL checkpoint");
        }

        let fingerprint = |p: &std::path::Path| -> (u64, u128) {
            let md = std::fs::metadata(p).expect("stat db file");
            let mtime = md
                .modified()
                .expect("mtime")
                .duration_since(std::time::UNIX_EPOCH)
                .expect("post-epoch")
                .as_nanos();
            (md.len(), mtime)
        };
        let global_before = fingerprint(&global_path);
        let workspace_before = fingerprint(&workspace_path);

        bootstrap_local_creator(Some("Alice".to_string()))
            .await
            .expect("no-op re-run converges");

        let global_after = fingerprint(&global_path);
        let workspace_after = fingerprint(&workspace_path);
        assert_eq!(
            global_before, global_after,
            "no-op must not write the global identity db (no seed-version churn)"
        );
        assert_eq!(
            workspace_before, workspace_after,
            "no-op must not write the workspace db"
        );

        // qc3 R-2: WAL-only writes would slip past the main-file fingerprint
        // (a deferred checkpoint could fold them in later) — a no-op must
        // also leave both `-wal` files absent or zero-size after the
        // TRUNCATE checkpoint + re-run.
        for db_path in [&global_path, &workspace_path] {
            let wal_path = db_path.with_extension("db-wal");
            match std::fs::metadata(&wal_path) {
                Ok(md) => assert_eq!(
                    md.len(),
                    0,
                    "no-op must not leave WAL content for {}",
                    db_path.display()
                ),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => panic!("stat {}: {e}", wal_path.display()),
            }
        }
    }

    /// Match-key negative (case): `Alice` vs `alice` are distinct byte-exact
    /// keys (AR-89 #1 — no case-fold). Seeding both and re-running `Alice`
    /// converges the exact `Alice` row (1 match) — never a collision.
    #[tokio::test]
    async fn bootstrap_local_creator_match_key_is_case_sensitive() {
        let _home = crate::testutil::isolated_home();

        let pool = open_global_db().await.expect("open global db");
        create_local_identity(
            &pool,
            "ctr_localcase1",
            "persistent",
            Some("Alice"),
            "2026-08-26T00:00:00Z",
        )
        .await
        .expect("seed Alice");
        create_local_identity(
            &pool,
            "ctr_localcase2",
            "persistent",
            Some("alice"),
            "2026-08-26T00:00:00Z",
        )
        .await
        .expect("seed alice");

        // Byte-exact: "Alice" matches only the "Alice" row → converge, no
        // collision (a case-folded key would match 2 rows and error).
        bootstrap_local_creator(Some("Alice".to_string()))
            .await
            .expect("exact-case match converges");

        let config = CliConfig::load().expect("reload config");
        assert_eq!(
            config.active_creator_id.as_deref(),
            Some("ctr_localcase1"),
            "exact byte match activated"
        );
    }

    /// Match-key negative (Unicode): no normalization — a decomposed twin
    /// (`Cafe\u{301}`, NFD) does not match the NFC row (`Café`) → 0 matches
    /// → mint a distinct identity (AR-89 #1).
    #[tokio::test]
    async fn bootstrap_local_creator_match_key_does_not_normalize_unicode() {
        let _home = crate::testutil::isolated_home();

        let pool = open_global_db().await.expect("open global db");
        create_local_identity(
            &pool,
            "ctr_localnfc1",
            "persistent",
            Some("Café"),
            "2026-08-26T00:00:00Z",
        )
        .await
        .expect("seed NFC row");

        // NFD twin: `e` + U+0301 combining acute. Byte-exact `==` → 0 matches.
        bootstrap_local_creator(Some("Cafe\u{301}".to_string()))
            .await
            .expect("decomposed twin mints a new identity");

        let identities = nexus_local_db::list_local_identities(&pool)
            .await
            .expect("list local identities");
        assert_eq!(identities.len(), 2, "decomposed twin is a distinct name");
        assert!(
            identities.iter().any(|r| r.creator_id == "ctr_localnfc1"),
            "NFC row still present"
        );
    }

    /// Repair: simulate the DF-83 partial (identity present, workspace row
    /// missing) by deleting the workspace row, then re-run — the row is
    /// materialized again and no new identity is minted.
    #[tokio::test]
    async fn bootstrap_local_creator_repairs_missing_workspace_row() {
        let _home = crate::testutil::isolated_home();

        bootstrap_local_creator(Some("Repair Me".to_string()))
            .await
            .expect("first bootstrap mints");
        let pool = open_global_db().await.expect("open global db");
        let identities = nexus_local_db::list_local_identities(&pool)
            .await
            .expect("list local identities");
        assert_eq!(identities.len(), 1);
        let id = identities[0].creator_id.clone();

        // Simulate the partial: delete the workspace `creators` row.
        let config = CliConfig::load().expect("reload config");
        let db_path = crate::config::resolve_state_db_path(&config).expect("resolve state db path");
        let workspace_pool = crate::db::Schema::init(&db_path)
            .await
            .expect("init workspace pool");
        sqlx::query("DELETE FROM creators WHERE creator_id = ?")
            .bind(&id)
            .execute(&workspace_pool)
            .await
            .expect("delete workspace row");

        // Re-run the same name → repair leg.
        bootstrap_local_creator(Some("Repair Me".to_string()))
            .await
            .expect("re-run repairs");

        // Same identity, no new mint.
        let after = nexus_local_db::list_local_identities(&pool)
            .await
            .expect("list local identities");
        assert_eq!(after.len(), 1, "no second identity minted");
        assert_eq!(after[0].creator_id, id);

        // Row is back.
        let row_exists: i64 = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM creators WHERE creator_id = ? AND status = 'active')",
        )
        .bind(&id)
        .fetch_one(&workspace_pool)
        .await
        .expect("query workspace creators row");
        assert_eq!(row_exists, 1, "workspace row repaired");
    }

    /// Session selection: a single name match on a *different* identity than
    /// the active one converges that id (activates it) — never a collision,
    /// never a silent takeover of an unmatched id (AR-89 #2).
    #[tokio::test]
    async fn bootstrap_local_creator_single_match_activates_matched_identity() {
        let _home = crate::testutil::isolated_home();

        // Mint one identity with a name, then switch active to a different
        // persistent identity.
        bootstrap_local_creator(Some("Target Name".to_string()))
            .await
            .expect("mint target");
        let pool = open_global_db().await.expect("open global db");
        let identities = nexus_local_db::list_local_identities(&pool)
            .await
            .expect("list local identities");
        assert_eq!(identities.len(), 1);
        let target_id = identities[0].creator_id.clone();

        // Seed a second persistent identity and make it active.
        create_local_identity(
            &pool,
            "ctr_localother",
            "persistent",
            Some("Other"),
            "2026-08-26T00:00:00Z",
        )
        .await
        .expect("seed other identity");
        let mut cli_config = CliConfig::load().expect("load config");
        cli_config.active_creator_id = Some("ctr_localother".to_string());
        cli_config.save().expect("save config");

        // Re-run the target name → single match → converge (activate) the target.
        bootstrap_local_creator(Some("Target Name".to_string()))
            .await
            .expect("single match converges");

        let config = CliConfig::load().expect("reload config");
        assert_eq!(
            config.active_creator_id.as_deref(),
            Some(target_id.as_str()),
            "matched identity activated (session selection)"
        );
    }

    /// Nameless `--persistent` converges the already-active persistent
    /// identity (AR-89 #2) — no new mint; a missing workspace row is repaired.
    #[tokio::test]
    async fn bootstrap_local_creator_nameless_converges_active_persistent() {
        let _home = crate::testutil::isolated_home();

        bootstrap_local_creator(Some("Active One".to_string()))
            .await
            .expect("mint active identity");
        let pool = open_global_db().await.expect("open global db");
        let identities = nexus_local_db::list_local_identities(&pool)
            .await
            .expect("list local identities");
        assert_eq!(identities.len(), 1);
        let active_id = identities[0].creator_id.clone();

        // Simulate the partial: delete the workspace row.
        let config = CliConfig::load().expect("reload config");
        let db_path = crate::config::resolve_state_db_path(&config).expect("resolve state db path");
        let workspace_pool = crate::db::Schema::init(&db_path)
            .await
            .expect("init workspace pool");
        sqlx::query("DELETE FROM creators WHERE creator_id = ?")
            .bind(&active_id)
            .execute(&workspace_pool)
            .await
            .expect("delete workspace row");

        // Nameless re-run → converge the active persistent identity (repair).
        bootstrap_local_creator(None)
            .await
            .expect("nameless converges active persistent");

        let after = nexus_local_db::list_local_identities(&pool)
            .await
            .expect("list local identities");
        assert_eq!(after.len(), 1, "no new mint");
        assert_eq!(after[0].creator_id, active_id);

        let row_exists: i64 = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM creators WHERE creator_id = ? AND status = 'active')",
        )
        .bind(&active_id)
        .fetch_one(&workspace_pool)
        .await
        .expect("query workspace creators row");
        assert_eq!(row_exists, 1, "workspace row repaired");
    }

    // ── V1.176 P0 QC fix wave (qc3 S-002): unique partial index ────

    /// qc3 S-002: the unique partial index on persistent `display_name` makes
    /// a second INSERT with the same name fail honestly — the mint leg maps
    /// the `SQLITE_CONSTRAINT` to `CreatorNameCollision` (the same shape the
    /// 2+ decision-tree leg produces). Simulates the concurrent race by
    /// inserting one row directly, then calling the mint leg for the same
    /// name (bypassing the 1-match decision-tree leg).
    #[tokio::test]
    async fn mint_leg_unique_index_maps_to_collision() {
        let _home = crate::testutil::isolated_home();

        let pool = open_global_db().await.expect("open global db");
        create_local_identity(
            &pool,
            "ctr_localdup1",
            "persistent",
            Some("Raced Name"),
            "2026-08-26T00:00:00Z",
        )
        .await
        .expect("seed first persistent row");

        let err = mint_and_materialize(&pool, Some("Raced Name"))
            .await
            .expect_err("unique partial index must reject the second same-name INSERT");
        match err {
            CliError::CreatorNameCollision {
                display_name,
                matches,
            } => {
                assert_eq!(display_name, "Raced Name");
                assert_eq!(matches, vec!["ctr_localdup1".to_string()]);
            }
            other => panic!("expected CreatorNameCollision, got {other:?}"),
        }
    }
}
