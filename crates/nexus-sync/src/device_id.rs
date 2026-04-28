//! Device ID generation and persistence.
//!
//! Provides a stable machine identifier (UUID v4) stored at `~/.nexus42/device-id`.
//! The identifier is generated once on first access and reused across restarts.

use std::path::Path;

/// Errors that can occur when reading or creating a device ID.
#[derive(Debug, thiserror::Error)]
pub enum DeviceIdError {
    /// Failed to read or write the device ID file.
    #[error("device ID IO error: {0}")]
    Io(#[from] std::io::Error),
    /// Failed to generate a UUID.
    #[error("UUID generation failed: {0}")]
    Uuid(#[from] uuid::Error),
}

/// Get the existing device ID or create a new one.
///
/// Uses atomic file creation (`create_new`) to eliminate the TOCTOU race
/// between the existence check and the write. If the file already exists,
/// its contents are read and returned unchanged.
pub fn get_or_create_device_id(nexus_home: &Path) -> Result<String, DeviceIdError> {
    let path = nexus_home_layout::device_id_path(nexus_home);

    // Ensure parent directory exists.
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    loop {
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                let new_id = uuid::Uuid::new_v4().to_string();
                use std::io::Write;
                file.write_all(new_id.as_bytes())?;

                // Set 0600 permissions on Unix.
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let permissions = std::fs::Permissions::from_mode(0o600);
                    std::fs::set_permissions(&path, permissions)?;
                }

                return Ok(new_id);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                let existing = std::fs::read_to_string(&path)?;
                let trimmed = existing.trim();
                if trimmed.is_empty() {
                    // Empty file — another thread may be writing.
                    // Wait briefly and retry rather than removing,
                    // to avoid NotFound races for concurrent readers.
                    std::thread::sleep(std::time::Duration::from_millis(5));
                    continue;
                }
                return Ok(trimmed.to_string());
            }
            Err(e) => return Err(e.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_call_creates_device_id_with_valid_uuid() {
        let tmp = tempfile::tempdir().unwrap();
        let id = get_or_create_device_id(tmp.path()).unwrap();
        assert!(
            uuid::Uuid::parse_str(&id).is_ok(),
            "created ID should be a valid UUID: {}",
            id
        );

        let path = nexus_home_layout::device_id_path(tmp.path());
        assert!(path.exists(), "device-id file should be created");
    }

    #[test]
    fn second_call_reads_existing_device_id() {
        let tmp = tempfile::tempdir().unwrap();
        let first = get_or_create_device_id(tmp.path()).unwrap();
        let second = get_or_create_device_id(tmp.path()).unwrap();
        assert_eq!(
            first, second,
            "subsequent calls should return the same device ID"
        );
    }

    #[test]
    fn missing_file_triggers_regeneration() {
        let tmp = tempfile::tempdir().unwrap();
        let first = get_or_create_device_id(tmp.path()).unwrap();

        let path = nexus_home_layout::device_id_path(tmp.path());
        std::fs::remove_file(&path).unwrap();

        let regenerated = get_or_create_device_id(tmp.path()).unwrap();
        assert!(
            uuid::Uuid::parse_str(&regenerated).is_ok(),
            "regenerated ID should be a valid UUID"
        );
        assert_ne!(
            first, regenerated,
            "regenerated ID should differ from the deleted one"
        );
    }

    #[cfg(unix)]
    #[test]
    fn device_id_file_has_restricted_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = tempfile::tempdir().unwrap();
        let _id = get_or_create_device_id(tmp.path()).unwrap();

        let path = nexus_home_layout::device_id_path(tmp.path());
        let metadata = std::fs::metadata(&path).unwrap();
        let mode = metadata.permissions().mode();
        assert_eq!(
            mode & 0o777,
            0o600,
            "device-id file should have 0600 permissions, got {:o}",
            mode & 0o777
        );
    }

    #[test]
    fn concurrent_calls_produce_consistent_result() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().to_path_buf();

        let handles: Vec<_> = (0..10)
            .map(|_| {
                let p = path.clone();
                std::thread::spawn(move || get_or_create_device_id(&p).unwrap())
            })
            .collect();

        let results: Vec<String> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        let first = &results[0];
        for r in &results {
            assert_eq!(
                r, first,
                "all concurrent calls should return the same device ID"
            );
        }
    }

    #[test]
    fn existing_file_is_preserved_not_overwritten() {
        let tmp = tempfile::tempdir().unwrap();
        let path = nexus_home_layout::device_id_path(tmp.path());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "pre-existing-uuid").unwrap();

        let id = get_or_create_device_id(tmp.path()).unwrap();
        assert_eq!(id, "pre-existing-uuid");

        // Ensure the file was not overwritten with a new UUID.
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "pre-existing-uuid");
    }

    #[test]
    fn parent_directory_created_automatically() {
        let tmp = tempfile::tempdir().unwrap();
        let deep = tmp.path().join("a/b/c");

        let id = get_or_create_device_id(&deep).unwrap();
        assert!(
            uuid::Uuid::parse_str(&id).is_ok(),
            "created ID should be a valid UUID"
        );

        let path = nexus_home_layout::device_id_path(&deep);
        assert!(
            path.exists(),
            "device-id file should be created in nested directory"
        );
    }
}
