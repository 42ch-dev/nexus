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
/// If `~/.nexus42/device-id` exists, reads and returns the UUID string.
/// If not, generates a new UUID v4, writes it to the file (with 0600
/// permissions on Unix), and returns it.
pub fn get_or_create_device_id(nexus_home: &Path) -> Result<String, DeviceIdError> {
    let path = nexus_home_layout::device_id_path(nexus_home);

    if path.exists() {
        let existing = std::fs::read_to_string(&path)?;
        let trimmed = existing.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
        // File exists but is empty — regenerate below.
    }

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let new_id = uuid::Uuid::new_v4().to_string();
    std::fs::write(&path, &new_id)?;

    // Set 0600 permissions on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let permissions = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(&path, permissions)?;
    }

    Ok(new_id)
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
}
