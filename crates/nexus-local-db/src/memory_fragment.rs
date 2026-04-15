//! Memory fragment CRUD operations for review pipeline.
//!
//! Manages lightweight keyword-indexed fragments from review decisions.
//! See creator-memory-soul-lifecycle-v1.md §7.2.

use rusqlite::{params, Connection};

/// Memory fragment record — mirrors DB row.
#[derive(Debug, Clone)]
pub struct MemoryFragmentRecord {
    /// Unique identifier for this fragment.
    pub fragment_id: String,
    /// ACP session ID that generated this fragment.
    pub session_id: String,
    /// Creator ID for ownership.
    pub creator_id: String,
    /// Keywords extracted from digest (stored as JSON array).
    pub keywords: String,
    /// Short summary of the fragment.
    pub summary: String,
    /// Creation timestamp.
    pub created_at: String,
    /// Optional TTL (e.g., "30d", "90d").
    pub ttl: Option<String>,
}

/// Create a new memory fragment.
///
/// Inserts the fragment into the memory_fragments table.
pub fn create_fragment(
    conn: &Connection,
    fragment: &MemoryFragmentRecord,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT INTO memory_fragments (fragment_id, session_id, creator_id, keywords, summary, created_at, ttl)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            fragment.fragment_id,
            fragment.session_id,
            fragment.creator_id,
            fragment.keywords,
            fragment.summary,
            fragment.created_at,
            fragment.ttl,
        ],
    )?;
    Ok(())
}

/// List all fragments for a creator.
///
/// Returns records ordered by created_at descending (most recent first).
pub fn list_fragments(
    conn: &Connection,
    creator_id: &str,
) -> Result<Vec<MemoryFragmentRecord>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT fragment_id, session_id, creator_id, keywords, summary, created_at, ttl
         FROM memory_fragments WHERE creator_id = ?1 ORDER BY created_at DESC",
    )?;
    let records = stmt.query_map(params![creator_id], |row| {
        Ok(MemoryFragmentRecord {
            fragment_id: row.get(0)?,
            session_id: row.get(1)?,
            creator_id: row.get(2)?,
            keywords: row.get(3)?,
            summary: row.get(4)?,
            created_at: row.get(5)?,
            ttl: row.get(6)?,
        })
    })?;
    records.collect()
}

/// List all fragments for a specific session.
///
/// Returns records for a given session_id (useful for session-level review).
pub fn list_fragments_by_session(
    conn: &Connection,
    session_id: &str,
) -> Result<Vec<MemoryFragmentRecord>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT fragment_id, session_id, creator_id, keywords, summary, created_at, ttl
         FROM memory_fragments WHERE session_id = ?1 ORDER BY created_at DESC",
    )?;
    let records = stmt.query_map(params![session_id], |row| {
        Ok(MemoryFragmentRecord {
            fragment_id: row.get(0)?,
            session_id: row.get(1)?,
            creator_id: row.get(2)?,
            keywords: row.get(3)?,
            summary: row.get(4)?,
            created_at: row.get(5)?,
            ttl: row.get(6)?,
        })
    })?;
    records.collect()
}

/// Delete a fragment by ID.
///
/// Returns true if a record was deleted, false if it didn't exist.
pub fn delete_fragment(conn: &Connection, fragment_id: &str) -> Result<bool, rusqlite::Error> {
    let affected = conn.execute(
        "DELETE FROM memory_fragments WHERE fragment_id = ?1",
        params![fragment_id],
    )?;
    Ok(affected > 0)
}

/// Get all deduped keywords for a creator.
///
/// Returns a union of all keywords from all fragments for this creator.
/// Used for context assembly §9.2 fragment keywords block.
pub fn get_all_keywords(
    conn: &Connection,
    creator_id: &str,
) -> Result<Vec<String>, rusqlite::Error> {
    let keywords_json: Vec<String> = conn
        .prepare("SELECT keywords FROM memory_fragments WHERE creator_id = ?1")?
        .query_map(params![creator_id], |row| row.get(0))?
        .flatten()
        .collect();

    // Parse each JSON array and collect unique keywords
    let mut all_keywords: Vec<String> = Vec::new();
    for json in keywords_json {
        if let Ok(keywords) = serde_json::from_str::<Vec<String>>(&json) {
            for kw in keywords {
                if !all_keywords.contains(&kw) {
                    all_keywords.push(kw);
                }
            }
        }
    }

    Ok(all_keywords)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::schema::MEMORY_FRAGMENTS_TABLE)
            .unwrap();
        conn
    }

    fn sample_fragment(fragment_id: &str) -> MemoryFragmentRecord {
        MemoryFragmentRecord {
            fragment_id: fragment_id.to_string(),
            session_id: "sess_test".to_string(),
            creator_id: "ctr_test".to_string(),
            keywords: "[\"keyword1\", \"keyword2\"]".to_string(),
            summary: "Test fragment summary".to_string(),
            created_at: "2026-04-14T10:00:00Z".to_string(),
            ttl: Some("30d".to_string()),
        }
    }

    #[test]
    fn test_create_and_list_fragment() {
        let conn = setup_db();
        let fragment = sample_fragment("frag_001");
        create_fragment(&conn, &fragment).unwrap();

        let list = list_fragments(&conn, "ctr_test").unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].fragment_id, "frag_001");
    }

    #[test]
    fn test_list_fragments_by_session() {
        let conn = setup_db();

        // Create fragments for different sessions
        let fragment1 = sample_fragment("frag_001");
        let fragment2 = MemoryFragmentRecord {
            fragment_id: "frag_002".to_string(),
            session_id: "sess_other".to_string(),
            ..sample_fragment("frag_002")
        };

        create_fragment(&conn, &fragment1).unwrap();
        create_fragment(&conn, &fragment2).unwrap();

        let list = list_fragments_by_session(&conn, "sess_test").unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].fragment_id, "frag_001");
    }

    #[test]
    fn test_delete_fragment() {
        let conn = setup_db();
        let fragment = sample_fragment("frag_001");
        create_fragment(&conn, &fragment).unwrap();

        assert!(delete_fragment(&conn, "frag_001").unwrap());
        let list = list_fragments(&conn, "ctr_test").unwrap();
        assert!(list.is_empty());
    }

    #[test]
    fn test_delete_nonexistent_returns_false() {
        let conn = setup_db();
        assert!(!delete_fragment(&conn, "frag_ghost").unwrap());
    }

    #[test]
    fn test_get_all_keywords_dedupes() {
        let conn = setup_db();

        let fragment1 = MemoryFragmentRecord {
            fragment_id: "frag_001".to_string(),
            keywords: "[\"alpha\", \"beta\"]".to_string(),
            ..sample_fragment("frag_001")
        };
        let fragment2 = MemoryFragmentRecord {
            fragment_id: "frag_002".to_string(),
            keywords: "[\"beta\", \"gamma\"]".to_string(),
            ..sample_fragment("frag_002")
        };

        create_fragment(&conn, &fragment1).unwrap();
        create_fragment(&conn, &fragment2).unwrap();

        let keywords = get_all_keywords(&conn, "ctr_test").unwrap();
        assert_eq!(keywords.len(), 3);
        assert!(keywords.contains(&"alpha".to_string()));
        assert!(keywords.contains(&"beta".to_string()));
        assert!(keywords.contains(&"gamma".to_string()));
    }

    #[test]
    fn test_get_all_keywords_empty_creator() {
        let conn = setup_db();
        let keywords = get_all_keywords(&conn, "ctr_ghost").unwrap();
        assert!(keywords.is_empty());
    }

    #[test]
    fn test_fragment_with_null_ttl() {
        let conn = setup_db();
        let fragment = MemoryFragmentRecord {
            fragment_id: "frag_null".to_string(),
            ttl: None,
            ..sample_fragment("frag_null")
        };
        create_fragment(&conn, &fragment).unwrap();

        let list = list_fragments(&conn, "ctr_test").unwrap();
        assert!(list[0].ttl.is_none());
    }

    #[test]
    fn test_fragment_ordering_by_created_at() {
        let conn = setup_db();

        let fragment1 = sample_fragment("frag_001");
        let fragment2 = MemoryFragmentRecord {
            fragment_id: "frag_002".to_string(),
            created_at: "2026-04-14T12:00:00Z".to_string(),
            ..sample_fragment("frag_002")
        };

        create_fragment(&conn, &fragment1).unwrap();
        create_fragment(&conn, &fragment2).unwrap();

        let list = list_fragments(&conn, "ctr_test").unwrap();
        assert_eq!(list[0].fragment_id, "frag_002"); // Later timestamp first
        assert_eq!(list[1].fragment_id, "frag_001");
    }

    #[test]
    fn test_get_all_keywords_handles_invalid_json() {
        let conn = setup_db();

        // Insert with valid JSON
        let fragment1 = sample_fragment("frag_001");
        create_fragment(&conn, &fragment1).unwrap();

        // Insert with invalid JSON (should be ignored)
        conn.execute(
            "INSERT INTO memory_fragments (fragment_id, session_id, creator_id, keywords)
             VALUES ('frag_bad', 'sess_test', 'ctr_test', 'not valid json')",
            [],
        )
        .unwrap();

        let keywords = get_all_keywords(&conn, "ctr_test").unwrap();
        // Should still return keywords from valid fragment
        assert!(keywords.contains(&"keyword1".to_string()));
        assert!(keywords.contains(&"keyword2".to_string()));
    }
}
