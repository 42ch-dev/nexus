//! Pending review CRUD operations for memory pipeline.
//!
//! Manages session-end captured queue entries for review → memory promotion.
//! See creator-memory-soul-lifecycle-v1.md §6.2.

use rusqlite::{params, Connection};

/// Pending review record — mirrors DB row.
#[derive(Debug, Clone)]
pub struct PendingReviewRecord {
    /// Unique identifier for this pending entry.
    pub pending_id: String,
    /// ACP session ID that triggered the capture.
    pub session_id: String,
    /// Creator ID for ownership.
    pub creator_id: String,
    /// Optional world ID for context.
    pub world_id: Option<String>,
    /// Task kind heuristic (brainstorm, outline, chapter, research, unknown).
    pub task_kind: String,
    /// Raw digest extracted from session.
    pub raw_digest: String,
    /// Creation timestamp.
    pub created_at: String,
}

/// Create a new pending review record.
///
/// Inserts the record into the memory_pending_review table.
pub fn create_pending_review(
    conn: &Connection,
    record: &PendingReviewRecord,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT INTO memory_pending_review (pending_id, session_id, creator_id, world_id, task_kind, raw_digest, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            record.pending_id,
            record.session_id,
            record.creator_id,
            record.world_id,
            record.task_kind,
            record.raw_digest,
            record.created_at,
        ],
    )?;
    Ok(())
}

/// List all pending reviews for a creator.
///
/// Returns records ordered by created_at descending (most recent first).
pub fn list_pending_reviews(
    conn: &Connection,
    creator_id: &str,
) -> Result<Vec<PendingReviewRecord>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT pending_id, session_id, creator_id, world_id, task_kind, raw_digest, created_at
         FROM memory_pending_review WHERE creator_id = ?1 ORDER BY created_at DESC",
    )?;
    let records = stmt.query_map(params![creator_id], |row| {
        Ok(PendingReviewRecord {
            pending_id: row.get(0)?,
            session_id: row.get(1)?,
            creator_id: row.get(2)?,
            world_id: row.get(3)?,
            task_kind: row.get(4)?,
            raw_digest: row.get(5)?,
            created_at: row.get(6)?,
        })
    })?;
    records.collect()
}

/// Get a specific pending review by ID.
///
/// Returns None if the record doesn't exist.
pub fn get_pending_review(
    conn: &Connection,
    pending_id: &str,
) -> Result<Option<PendingReviewRecord>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT pending_id, session_id, creator_id, world_id, task_kind, raw_digest, created_at
         FROM memory_pending_review WHERE pending_id = ?1",
    )?;
    let result = stmt.query_row(params![pending_id], |row| {
        Ok(PendingReviewRecord {
            pending_id: row.get(0)?,
            session_id: row.get(1)?,
            creator_id: row.get(2)?,
            world_id: row.get(3)?,
            task_kind: row.get(4)?,
            raw_digest: row.get(5)?,
            created_at: row.get(6)?,
        })
    });
    match result {
        Ok(record) => Ok(Some(record)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e),
    }
}

/// Delete a pending review by ID.
///
/// Returns true if a record was deleted, false if it didn't exist.
pub fn delete_pending_review(conn: &Connection, pending_id: &str) -> Result<bool, rusqlite::Error> {
    let affected = conn.execute(
        "DELETE FROM memory_pending_review WHERE pending_id = ?1",
        params![pending_id],
    )?;
    Ok(affected > 0)
}

/// Count pending reviews for a creator.
///
/// Used for queue depth monitoring and review scheduling.
pub fn count_pending_reviews(
    conn: &Connection,
    creator_id: &str,
) -> Result<usize, rusqlite::Error> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM memory_pending_review WHERE creator_id = ?1",
        params![creator_id],
        |row| row.get(0),
    )?;
    Ok(count as usize)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::schema::MEMORY_PENDING_REVIEW_TABLE)
            .unwrap();
        conn
    }

    fn sample_record(pending_id: &str) -> PendingReviewRecord {
        PendingReviewRecord {
            pending_id: pending_id.to_string(),
            session_id: format!("sess_{}", pending_id), // Unique per record
            creator_id: "ctr_test".to_string(),
            world_id: Some("wld_test".to_string()),
            task_kind: "brainstorm".to_string(),
            raw_digest: "Test digest content".to_string(),
            created_at: "2026-04-14T10:00:00Z".to_string(),
        }
    }

    #[test]
    fn test_create_and_get_pending_review() {
        let conn = setup_db();
        let record = sample_record("pending_001");
        create_pending_review(&conn, &record).unwrap();

        let fetched = get_pending_review(&conn, "pending_001").unwrap().unwrap();
        assert_eq!(fetched.pending_id, "pending_001");
        assert_eq!(fetched.creator_id, "ctr_test");
        assert_eq!(fetched.task_kind, "brainstorm");
    }

    #[test]
    fn test_get_nonexistent_returns_none() {
        let conn = setup_db();
        assert!(get_pending_review(&conn, "pending_ghost")
            .unwrap()
            .is_none());
    }

    #[test]
    fn test_list_pending_reviews_by_creator() {
        let conn = setup_db();

        // Create multiple records for different creators
        let record1 = sample_record("pending_001");
        let record2 = PendingReviewRecord {
            pending_id: "pending_002".to_string(),
            session_id: "sess_pending_002".to_string(), // Unique session_id
            creator_id: "ctr_other".to_string(),
            ..sample_record("pending_002")
        };
        let record3 = PendingReviewRecord {
            pending_id: "pending_003".to_string(),
            session_id: "sess_pending_003".to_string(), // Unique session_id
            created_at: "2026-04-14T12:00:00Z".to_string(),
            ..sample_record("pending_003")
        };

        create_pending_review(&conn, &record1).unwrap();
        create_pending_review(&conn, &record2).unwrap();
        create_pending_review(&conn, &record3).unwrap();

        // List for ctr_test should return 2 records (001 and 003)
        let list = list_pending_reviews(&conn, "ctr_test").unwrap();
        assert_eq!(list.len(), 2);

        // Ordered by created_at DESC
        assert_eq!(list[0].pending_id, "pending_003"); // Later timestamp
        assert_eq!(list[1].pending_id, "pending_001");
    }

    #[test]
    fn test_delete_pending_review() {
        let conn = setup_db();
        let record = sample_record("pending_001");
        create_pending_review(&conn, &record).unwrap();

        assert!(delete_pending_review(&conn, "pending_001").unwrap());
        assert!(get_pending_review(&conn, "pending_001").unwrap().is_none());
    }

    #[test]
    fn test_delete_nonexistent_returns_false() {
        let conn = setup_db();
        assert!(!delete_pending_review(&conn, "pending_ghost").unwrap());
    }

    #[test]
    fn test_count_pending_reviews() {
        let conn = setup_db();

        assert_eq!(count_pending_reviews(&conn, "ctr_test").unwrap(), 0);

        let record1 = sample_record("pending_001");
        let record2 = sample_record("pending_002"); // Unique session_id
        create_pending_review(&conn, &record1).unwrap();
        create_pending_review(&conn, &record2).unwrap();

        assert_eq!(count_pending_reviews(&conn, "ctr_test").unwrap(), 2);
        assert_eq!(count_pending_reviews(&conn, "ctr_other").unwrap(), 0);
    }

    #[test]
    fn test_pending_review_with_null_world_id() {
        let conn = setup_db();
        let record = PendingReviewRecord {
            pending_id: "pending_null".to_string(),
            session_id: "sess_pending_null".to_string(), // Unique session_id
            world_id: None,
            ..sample_record("pending_null")
        };
        create_pending_review(&conn, &record).unwrap();

        let fetched = get_pending_review(&conn, "pending_null").unwrap().unwrap();
        assert!(fetched.world_id.is_none());
    }
}
