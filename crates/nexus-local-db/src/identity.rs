//! Local identity CRUD operations for SQLite.
//!
//! Provides functions to create, read, list, and update local identities
//! in the `local_identities` table. These are used by both CLI and daemon.

use rusqlite::Connection;

use crate::error::LocalDbError;

/// Row representation for the `local_identities` table.
#[derive(Debug, Clone)]
pub struct LocalIdentityRow {
    pub creator_id: String,
    pub identity_type: String,
    pub display_name: Option<String>,
    pub created_at: String,
    pub platform_linked: bool,
    pub platform_creator_id: Option<String>,
}

/// Create a new local identity in the database.
///
/// # Errors
///
/// Returns `LocalDbError` if the insert fails (e.g. duplicate creator_id).
pub fn create_local_identity(
    conn: &Connection,
    creator_id: &str,
    identity_type: &str,
    display_name: Option<&str>,
    created_at: &str,
) -> Result<LocalIdentityRow, LocalDbError> {
    conn.execute(
        "INSERT INTO local_identities (creator_id, identity_type, display_name, created_at, platform_linked)
         VALUES (?1, ?2, ?3, ?4, 0)",
        rusqlite::params![creator_id, identity_type, display_name, created_at],
    )?;

    Ok(LocalIdentityRow {
        creator_id: creator_id.to_string(),
        identity_type: identity_type.to_string(),
        display_name: display_name.map(|s| s.to_string()),
        created_at: created_at.to_string(),
        platform_linked: false,
        platform_creator_id: None,
    })
}

/// Get a local identity by creator_id.
///
/// Returns `None` if no identity exists with the given ID.
pub fn get_local_identity(
    conn: &Connection,
    creator_id: &str,
) -> Result<Option<LocalIdentityRow>, LocalDbError> {
    let result = conn.query_row(
        "SELECT creator_id, identity_type, display_name, created_at, platform_linked, platform_creator_id
         FROM local_identities WHERE creator_id = ?1",
        rusqlite::params![creator_id],
        |row| {
            Ok(LocalIdentityRow {
                creator_id: row.get(0)?,
                identity_type: row.get(1)?,
                display_name: row.get(2)?,
                created_at: row.get(3)?,
                platform_linked: row.get::<_, i32>(4)? != 0,
                platform_creator_id: row.get(5)?,
            })
        },
    );

    match result {
        Ok(row) => Ok(Some(row)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(LocalDbError::from(e)),
    }
}

/// List all local identities.
///
/// Returns all identities sorted by creation time (oldest first).
pub fn list_local_identities(conn: &Connection) -> Result<Vec<LocalIdentityRow>, LocalDbError> {
    let mut stmt = conn.prepare(
        "SELECT creator_id, identity_type, display_name, created_at, platform_linked, platform_creator_id
         FROM local_identities ORDER BY created_at",
    )?;

    let rows = stmt
        .query_map([], |row| {
            Ok(LocalIdentityRow {
                creator_id: row.get(0)?,
                identity_type: row.get(1)?,
                display_name: row.get(2)?,
                created_at: row.get(3)?,
                platform_linked: row.get::<_, i32>(4)? != 0,
                platform_creator_id: row.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(rows)
}

/// Link a local identity to a platform Creator.
///
/// Sets `platform_linked = true` and stores the `platform_creator_id`.
///
/// # Errors
///
/// Returns `LocalDbError` if the identity does not exist.
pub fn link_to_platform(
    conn: &Connection,
    creator_id: &str,
    platform_creator_id: &str,
) -> Result<(), LocalDbError> {
    let affected = conn.execute(
        "UPDATE local_identities SET platform_linked = 1, platform_creator_id = ?1
         WHERE creator_id = ?2 AND platform_linked = 0",
        rusqlite::params![platform_creator_id, creator_id],
    )?;

    if affected == 0 {
        // Check if identity exists but is already linked
        let existing = get_local_identity(conn, creator_id)?;
        match existing {
            Some(row) if row.platform_linked => {
                return Err(LocalDbError::Rusqlite(rusqlite::Error::QueryReturnedNoRows));
            }
            None => {
                return Err(LocalDbError::Rusqlite(rusqlite::Error::QueryReturnedNoRows));
            }
            _ => {
                return Err(LocalDbError::Rusqlite(rusqlite::Error::QueryReturnedNoRows));
            }
        }
    }

    Ok(())
}

/// Delete a local identity by creator_id.
///
/// # Errors
///
/// Returns `LocalDbError` if the delete fails.
pub fn delete_local_identity(conn: &Connection, creator_id: &str) -> Result<bool, LocalDbError> {
    let affected = conn.execute(
        "DELETE FROM local_identities WHERE creator_id = ?1",
        rusqlite::params![creator_id],
    )?;

    Ok(affected > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{init, RuntimeRole};

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init(&conn, RuntimeRole::Cli).unwrap();
        conn
    }

    #[test]
    fn create_and_get_identity() {
        let conn = setup_db();
        let row = create_local_identity(
            &conn,
            "ctr_localTest123",
            "persistent",
            Some("Test User"),
            "2026-01-01T00:00:00Z",
        )
        .unwrap();

        assert_eq!(row.creator_id, "ctr_localTest123");
        assert_eq!(row.identity_type, "persistent");
        assert_eq!(row.display_name, Some("Test User".to_string()));

        let fetched = get_local_identity(&conn, "ctr_localTest123").unwrap();
        assert!(fetched.is_some());
        let fetched = fetched.unwrap();
        assert_eq!(fetched.creator_id, "ctr_localTest123");
    }

    #[test]
    fn create_anonymous_identity() {
        let conn = setup_db();
        let row = create_local_identity(
            &conn,
            "ctr_anonTest456",
            "anonymous",
            None,
            "2026-01-01T00:00:00Z",
        )
        .unwrap();

        assert_eq!(row.identity_type, "anonymous");
        assert!(row.display_name.is_none());
        assert!(!row.platform_linked);
    }

    #[test]
    fn get_nonexistent_identity() {
        let conn = setup_db();
        let result = get_local_identity(&conn, "ctr_nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn list_identities() {
        let conn = setup_db();

        create_local_identity(
            &conn,
            "ctr_anonAaa",
            "anonymous",
            None,
            "2026-01-01T00:00:00Z",
        )
        .unwrap();
        create_local_identity(
            &conn,
            "ctr_localBbb",
            "persistent",
            Some("User B"),
            "2026-01-02T00:00:00Z",
        )
        .unwrap();

        let list = list_local_identities(&conn).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].creator_id, "ctr_anonAaa");
        assert_eq!(list[1].creator_id, "ctr_localBbb");
    }

    #[test]
    fn list_identities_empty() {
        let conn = setup_db();
        let list = list_local_identities(&conn).unwrap();
        assert!(list.is_empty());
    }

    #[test]
    fn link_identity_to_platform() {
        let conn = setup_db();
        create_local_identity(
            &conn,
            "ctr_localLink",
            "persistent",
            Some("Test"),
            "2026-01-01T00:00:00Z",
        )
        .unwrap();

        link_to_platform(&conn, "ctr_localLink", "ctr_Platform123").unwrap();

        let row = get_local_identity(&conn, "ctr_localLink").unwrap().unwrap();
        assert!(row.platform_linked);
        assert_eq!(row.platform_creator_id, Some("ctr_Platform123".to_string()));
    }

    #[test]
    fn link_nonexistent_identity() {
        let conn = setup_db();
        let result = link_to_platform(&conn, "ctr_nonexistent", "ctr_Platform123");
        assert!(result.is_err());
    }

    #[test]
    fn delete_identity() {
        let conn = setup_db();
        create_local_identity(
            &conn,
            "ctr_localDel",
            "persistent",
            None,
            "2026-01-01T00:00:00Z",
        )
        .unwrap();

        assert!(get_local_identity(&conn, "ctr_localDel").unwrap().is_some());

        let deleted = delete_local_identity(&conn, "ctr_localDel").unwrap();
        assert!(deleted);

        assert!(get_local_identity(&conn, "ctr_localDel").unwrap().is_none());
    }

    #[test]
    fn delete_nonexistent_identity() {
        let conn = setup_db();
        let deleted = delete_local_identity(&conn, "ctr_nonexistent").unwrap();
        assert!(!deleted);
    }
}
