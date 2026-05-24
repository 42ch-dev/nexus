//! Narrative write repository — product write path for worlds and timeline events.
//!
//! Provides `create_world` and `append_event` as public API for CLI commands.
//! These functions write directly to the workspace `state.db`.
//!
//! Uses runtime `sqlx::query()` with SAFETY comments for new write queries
//! to avoid requiring `cargo sqlx prepare` for each new query. Read paths
//! in `narrative_gateway.rs` and `kb_store.rs` use compile-time checked queries.
//!
//! The `NarrativeGateway` trait in `nexus-narrative` remains **read-only**;
//! writes go through this module, not through the gateway trait.

use sqlx::SqlitePool;

/// Error type for narrative write operations.
#[derive(Debug)]
pub enum NarrativeWriteError {
    /// A database operation failed.
    Database(String),
    /// An ID format validation failed (e.g. not `wld_*` or `evt_*` prefix).
    InvalidId {
        field: String,
        value: String,
        reason: String,
    },
    /// A required FK reference was not found (e.g. creator or world).
    FkNotFound { table: String, id: String },
    /// A sequence conflict occurred (duplicate `sequence_no` for same world+branch).
    SequenceConflict {
        world_id: String,
        branch_id: String,
        sequence_no: i64,
    },
}

impl std::fmt::Display for NarrativeWriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Database(msg) => write!(f, "database error: {msg}"),
            Self::InvalidId {
                field,
                value,
                reason,
            } => {
                write!(f, "invalid {field} '{value}': {reason}")
            }
            Self::FkNotFound { table, id } => {
                write!(f, "referenced {table} '{id}' not found")
            }
            Self::SequenceConflict {
                world_id,
                branch_id,
                sequence_no,
            } => {
                write!(
                    f,
                    "sequence conflict: event already exists at ({world_id}, {branch_id}, {sequence_no})"
                )
            }
        }
    }
}

impl std::error::Error for NarrativeWriteError {}

impl From<sqlx::Error> for NarrativeWriteError {
    fn from(err: sqlx::Error) -> Self {
        Self::Database(err.to_string())
    }
}

/// Result of a successful world creation.
#[derive(Debug)]
pub struct CreateWorldResult {
    /// The newly created `world_id` (prefixed `wld_`).
    pub world_id: String,
    /// The default root branch ID (generated as `fbk_root_<uuid>`).
    pub root_fork_branch_id: String,
    /// Timestamp of creation.
    pub created_at: String,
}

/// Result of a successful event append.
#[derive(Debug)]
pub struct AppendEventResult {
    /// The newly created event ID (prefixed `evt_`).
    pub event_id: String,
    /// The allocated sequence number.
    pub sequence_no: i64,
    /// Timestamp of creation.
    pub created_at: String,
}

/// Generate a new `wld_` prefixed world ID.
fn generate_world_id() -> String {
    format!("wld_{}", uuid::Uuid::new_v4().to_string().replace('-', ""))
}

/// Generate a new `evt_` prefixed event ID.
fn generate_event_id() -> String {
    format!("evt_{}", uuid::Uuid::new_v4().to_string().replace('-', ""))
}

/// Generate a root fork branch ID.
fn generate_root_branch_id() -> String {
    format!(
        "fbk_root_{}",
        &uuid::Uuid::new_v4().to_string().replace('-', "")[..8]
    )
}

/// Validate that an ID has the expected prefix.
fn validate_id_prefix(
    value: &str,
    prefix: &str,
    field_name: &str,
) -> Result<(), NarrativeWriteError> {
    if value.starts_with(prefix) && value.len() > prefix.len() {
        Ok(())
    } else {
        Err(NarrativeWriteError::InvalidId {
            field: field_name.to_string(),
            value: value.to_string(),
            reason: format!("must start with '{prefix}' followed by alphanumeric characters"),
        })
    }
}

/// Create a new world in the database.
///
/// Generates a `wld_*` ID, inserts into `narrative_worlds`, and sets a
/// default `root_fork_branch_id`. Validates that `owner_creator_id`
/// references an existing creator row.
///
/// # Errors
///
/// Returns `NarrativeWriteError` if:
/// - The creator FK does not exist
/// - A database error occurs
pub async fn create_world(
    pool: &SqlitePool,
    owner_creator_id: &str,
    title: &str,
    slug: &str,
    visibility: &str,
    time_policy: &str,
) -> Result<CreateWorldResult, NarrativeWriteError> {
    // Validate creator FK
    // SAFETY: simple EXISTS query against known table schema
    let creator_exists: i64 =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM creators WHERE creator_id = ?)")
            .bind(owner_creator_id)
            .fetch_one(pool)
            .await?;

    if creator_exists == 0 {
        return Err(NarrativeWriteError::FkNotFound {
            table: "creator".to_string(),
            id: owner_creator_id.to_string(),
        });
    }

    let world_id = generate_world_id();
    let root_fork_branch_id = generate_root_branch_id();
    let created_at = chrono::Utc::now().to_rfc3339();

    // SAFETY: INSERT matches narrative_worlds DDL in 20260524_narrative_worlds.sql
    sqlx::query(
        "INSERT INTO narrative_worlds \
            (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
             time_policy, root_fork_branch_id, metadata_json, created_at) \
           VALUES (?, 'wrk_local', ?, ?, ?, 'active', ?, ?, ?, '{}', ?)",
    )
    .bind(&world_id)
    .bind(owner_creator_id)
    .bind(title)
    .bind(slug)
    .bind(visibility)
    .bind(time_policy)
    .bind(&root_fork_branch_id)
    .bind(&created_at)
    .execute(pool)
    .await?;

    Ok(CreateWorldResult {
        world_id,
        root_fork_branch_id,
        created_at,
    })
}

/// Append a timeline event to a world's branch.
///
/// Allocates the next `sequence_no` for `(world_id, branch_id)` automatically
/// by querying `MAX(sequence_no) + 1`. Validates that `world_id` references
/// an existing world row.
///
/// # Errors
///
/// Returns `NarrativeWriteError` if:
/// - `world_id` is not found
/// - A sequence conflict occurs (UNIQUE constraint violation)
/// - A database error occurs
pub async fn append_event(
    pool: &SqlitePool,
    world_id: &str,
    branch_id: &str,
    event_type: &str,
    title: Option<&str>,
    summary: Option<&str>,
) -> Result<AppendEventResult, NarrativeWriteError> {
    // Validate world_id prefix
    validate_id_prefix(world_id, "wld_", "world_id")?;

    // Validate world FK exists
    // SAFETY: simple EXISTS query against known table schema
    let world_exists: i64 =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM narrative_worlds WHERE world_id = ?)")
            .bind(world_id)
            .fetch_one(pool)
            .await?;

    if world_exists == 0 {
        return Err(NarrativeWriteError::FkNotFound {
            table: "world".to_string(),
            id: world_id.to_string(),
        });
    }

    // Allocate next sequence_no
    // SAFETY: MAX aggregate query against known table schema
    let max_seq: Option<i64> = sqlx::query_scalar(
        "SELECT MAX(sequence_no) FROM narrative_timeline_events WHERE world_id = ? AND branch_id = ?",
    )
    .bind(world_id)
    .bind(branch_id)
    .fetch_one(pool)
    .await?;

    let sequence_no = max_seq.unwrap_or(-1) + 1;

    let event_id = generate_event_id();
    let created_at = chrono::Utc::now().to_rfc3339();

    // SAFETY: INSERT matches narrative_timeline_events DDL in 20260524_narrative_worlds.sql
    let result = sqlx::query(
        "INSERT INTO narrative_timeline_events \
            (timeline_event_id, world_id, branch_id, event_type, status, sequence_no, \
             title, summary, metadata_json, created_at) \
           VALUES (?, ?, ?, ?, 'provisional', ?, ?, ?, '{}', ?)",
    )
    .bind(&event_id)
    .bind(world_id)
    .bind(branch_id)
    .bind(event_type)
    .bind(sequence_no)
    .bind(title)
    .bind(summary)
    .bind(&created_at)
    .execute(pool)
    .await;

    match result {
        Ok(_) => Ok(AppendEventResult {
            event_id,
            sequence_no,
            created_at,
        }),
        Err(e) => {
            // Check for UNIQUE constraint violation on (world_id, branch_id, sequence_no)
            if let sqlx::Error::Database(ref db_err) = e {
                if db_err.code().as_deref() == Some("2067") {
                    return Err(NarrativeWriteError::SequenceConflict {
                        world_id: world_id.to_string(),
                        branch_id: branch_id.to_string(),
                        sequence_no,
                    });
                }
            }
            Err(NarrativeWriteError::from(e))
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{open_pool, run_migrations};
    use nexus_narrative::NarrativeGateway;

    async fn fresh_pool() -> (SqlitePool, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = open_pool(&db_path).await.unwrap();
        run_migrations(&pool).await.unwrap();
        (pool, dir)
    }

    async fn seed_creator(pool: &SqlitePool) {
        // SAFETY: test-only seed — uses runtime query for FK setup
        sqlx::query(
            "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) \
             VALUES (?, ?, 'active', datetime('now'), '{}')",
        )
        .bind("ctr_test")
        .bind("Test Creator")
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn test_create_world_success() {
        let (pool, _dir) = fresh_pool().await;
        seed_creator(&pool).await;

        let result = create_world(
            &pool, "ctr_test", "My World", "my-world", "private", "manual",
        )
        .await
        .unwrap();

        assert!(result.world_id.starts_with("wld_"));
        assert!(result.root_fork_branch_id.starts_with("fbk_root_"));
        assert!(!result.created_at.is_empty());
    }

    #[tokio::test]
    async fn test_create_world_fk_violation() {
        let (pool, _dir) = fresh_pool().await;

        let result = create_world(
            &pool,
            "ctr_nonexistent",
            "My World",
            "my-world",
            "private",
            "manual",
        )
        .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, NarrativeWriteError::FkNotFound { ref table, .. } if table == "creator")
        );
    }

    #[tokio::test]
    async fn test_append_event_success() {
        let (pool, _dir) = fresh_pool().await;
        seed_creator(&pool).await;

        let world = create_world(
            &pool,
            "ctr_test",
            "Test World",
            "test-world",
            "private",
            "manual",
        )
        .await
        .unwrap();

        let event = append_event(
            &pool,
            &world.world_id,
            &world.root_fork_branch_id,
            "story_advance",
            Some("The Beginning"),
            Some("A story begins."),
        )
        .await
        .unwrap();

        assert!(event.event_id.starts_with("evt_"));
        assert_eq!(event.sequence_no, 0);
    }

    #[tokio::test]
    async fn test_append_event_auto_sequence() {
        let (pool, _dir) = fresh_pool().await;
        seed_creator(&pool).await;

        let world = create_world(
            &pool,
            "ctr_test",
            "Test World",
            "test-world",
            "private",
            "manual",
        )
        .await
        .unwrap();

        let evt1 = append_event(
            &pool,
            &world.world_id,
            &world.root_fork_branch_id,
            "story_advance",
            None,
            None,
        )
        .await
        .unwrap();
        assert_eq!(evt1.sequence_no, 0);

        let evt2 = append_event(
            &pool,
            &world.world_id,
            &world.root_fork_branch_id,
            "story_advance",
            None,
            None,
        )
        .await
        .unwrap();
        assert_eq!(evt2.sequence_no, 1);

        let evt3 = append_event(
            &pool,
            &world.world_id,
            &world.root_fork_branch_id,
            "story_advance",
            None,
            None,
        )
        .await
        .unwrap();
        assert_eq!(evt3.sequence_no, 2);
    }

    #[tokio::test]
    async fn test_append_event_world_not_found() {
        let (pool, _dir) = fresh_pool().await;

        let result = append_event(
            &pool,
            "wld_nonexistent",
            "fbk_root",
            "story_advance",
            None,
            None,
        )
        .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, NarrativeWriteError::FkNotFound { ref table, .. } if table == "world")
        );
    }

    #[tokio::test]
    async fn test_append_event_invalid_world_id_prefix() {
        let (pool, _dir) = fresh_pool().await;

        let result = append_event(
            &pool,
            "invalid_prefix",
            "fbk_root",
            "story_advance",
            None,
            None,
        )
        .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, NarrativeWriteError::InvalidId { ref field, .. } if field == "world_id")
        );
    }

    #[tokio::test]
    async fn test_create_and_list_worlds_roundtrip() {
        let (pool, _dir) = fresh_pool().await;
        seed_creator(&pool).await;

        create_world(&pool, "ctr_test", "World A", "world-a", "private", "manual")
            .await
            .unwrap();
        create_world(&pool, "ctr_test", "World B", "world-b", "private", "manual")
            .await
            .unwrap();

        // Verify via the read gateway
        let gw = crate::narrative_gateway::SqliteNarrativeGateway::new(pool);
        let worlds = gw.list_worlds().await.unwrap();
        assert_eq!(worlds.len(), 2);
    }

    #[tokio::test]
    async fn test_append_and_read_timeline_roundtrip() {
        let (pool, _dir) = fresh_pool().await;
        seed_creator(&pool).await;

        let world = create_world(
            &pool,
            "ctr_test",
            "Test World",
            "test-world",
            "private",
            "manual",
        )
        .await
        .unwrap();

        append_event(
            &pool,
            &world.world_id,
            &world.root_fork_branch_id,
            "story_advance",
            Some("Chapter 1"),
            None,
        )
        .await
        .unwrap();
        append_event(
            &pool,
            &world.world_id,
            &world.root_fork_branch_id,
            "story_advance",
            Some("Chapter 2"),
            None,
        )
        .await
        .unwrap();

        // Verify via the read gateway
        let gw = crate::narrative_gateway::SqliteNarrativeGateway::new(pool);
        let events = gw
            .get_timeline(&world.world_id, Some(&world.root_fork_branch_id))
            .await
            .unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].sequence_no, 0);
        assert_eq!(events[0].title.as_deref(), Some("Chapter 1"));
        assert_eq!(events[1].sequence_no, 1);
        assert_eq!(events[1].title.as_deref(), Some("Chapter 2"));
    }
}
