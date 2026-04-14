//! SOUL metadata persistence (local SQLite).
//!
//! Tracks per-creator SOUL.md metadata for fast lookups without file I/O.

use rusqlite::{params, Connection};

/// SOUL metadata record.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SoulMeta {
    pub creator_id: String,
    pub file_path: String,
    pub schema_version: u32,
    pub personality_hash: Option<String>,
    pub experience_hash: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Upsert SOUL metadata (insert or update).
#[allow(dead_code)]
pub fn upsert(conn: &Connection, meta: &SoulMeta) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT INTO soul_meta (creator_id, file_path, schema_version, personality_hash, experience_hash, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(creator_id) DO UPDATE SET
           file_path = excluded.file_path,
           schema_version = excluded.schema_version,
           personality_hash = excluded.personality_hash,
           experience_hash = excluded.experience_hash,
           updated_at = excluded.updated_at",
        params![
            meta.creator_id,
            meta.file_path,
            meta.schema_version,
            meta.personality_hash,
            meta.experience_hash,
            meta.created_at,
            meta.updated_at,
        ],
    )?;
    Ok(())
}

/// Get SOUL metadata for a creator.
#[allow(dead_code)]
pub fn get(conn: &Connection, creator_id: &str) -> Result<Option<SoulMeta>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT creator_id, file_path, schema_version, personality_hash, experience_hash, created_at, updated_at
         FROM soul_meta WHERE creator_id = ?1",
    )?;
    let result = stmt.query_row(params![creator_id], |row| {
        Ok(SoulMeta {
            creator_id: row.get(0)?,
            file_path: row.get(1)?,
            schema_version: row.get(2)?,
            personality_hash: row.get(3)?,
            experience_hash: row.get(4)?,
            created_at: row.get(5)?,
            updated_at: row.get(6)?,
        })
    });
    match result {
        Ok(meta) => Ok(Some(meta)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e),
    }
}

/// Delete SOUL metadata for a creator.
#[allow(dead_code)]
pub fn delete(conn: &Connection, creator_id: &str) -> Result<bool, rusqlite::Error> {
    let affected = conn.execute(
        "DELETE FROM soul_meta WHERE creator_id = ?1",
        params![creator_id],
    )?;
    Ok(affected > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::schema::SOUL_META_TABLE).unwrap();
        conn
    }

    #[test]
    fn upsert_and_get() {
        let conn = setup_db();
        let meta = SoulMeta {
            creator_id: "ctr_test".to_string(),
            file_path: "/tmp/SOUL.md".to_string(),
            schema_version: 1,
            personality_hash: Some("abc".to_string()),
            experience_hash: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };
        upsert(&conn, &meta).unwrap();

        let fetched = get(&conn, "ctr_test").unwrap().unwrap();
        assert_eq!(fetched.creator_id, "ctr_test");
        assert_eq!(fetched.personality_hash.as_deref(), Some("abc"));
        assert_eq!(fetched.experience_hash, None);
    }

    #[test]
    fn upsert_updates_existing() {
        let conn = setup_db();
        let meta = SoulMeta {
            creator_id: "ctr_test".to_string(),
            file_path: "/tmp/SOUL.md".to_string(),
            schema_version: 1,
            personality_hash: None,
            experience_hash: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };
        upsert(&conn, &meta).unwrap();

        let updated = SoulMeta {
            personality_hash: Some("new_hash".to_string()),
            updated_at: "2026-01-02T00:00:00Z".to_string(),
            ..meta
        };
        upsert(&conn, &updated).unwrap();

        let fetched = get(&conn, "ctr_test").unwrap().unwrap();
        assert_eq!(fetched.personality_hash.as_deref(), Some("new_hash"));
        assert_eq!(fetched.updated_at, "2026-01-02T00:00:00Z");
    }

    #[test]
    fn get_nonexistent_returns_none() {
        let conn = setup_db();
        assert!(get(&conn, "ctr_ghost").unwrap().is_none());
    }

    #[test]
    fn delete_existing() {
        let conn = setup_db();
        let meta = SoulMeta {
            creator_id: "ctr_del".to_string(),
            file_path: "/tmp/SOUL.md".to_string(),
            schema_version: 1,
            personality_hash: None,
            experience_hash: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };
        upsert(&conn, &meta).unwrap();
        assert!(delete(&conn, "ctr_del").unwrap());
        assert!(get(&conn, "ctr_del").unwrap().is_none());
    }

    #[test]
    fn delete_nonexistent_returns_false() {
        let conn = setup_db();
        assert!(!delete(&conn, "ctr_ghost").unwrap());
    }
}
