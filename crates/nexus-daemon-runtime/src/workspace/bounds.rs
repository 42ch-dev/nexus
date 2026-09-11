//! Manifest and metadata bounds (v1.188 P3 L2).

use super::session::SessionError;

/// Max relative path length (bytes).
pub const MAX_PATH_LEN: usize = 4096;
/// Max SHA-256 hex length.
pub const MAX_HASH_LEN: usize = 64;
/// Max base64-encoded body before decode.
pub const MAX_ENCODED_BODY: usize = 1_398_104; // ceil(1MiB * 4/3)
/// Max serialized intent entries JSON length.
pub const MAX_ENTRIES_JSON: usize = 512_000;

pub fn validate_relative_path(path: &str) -> Result<(), SessionError> {
    if path.is_empty() {
        return Err(SessionError::ManifestInvalid("path must not be empty".into()));
    }
    if path.len() > MAX_PATH_LEN {
        return Err(SessionError::ManifestInvalid("path too long".into()));
    }
    if path.contains("..") || path.starts_with('/') {
        return Err(SessionError::ManifestInvalid("path must be relative".into()));
    }
    Ok(())
}

pub fn validate_hash_hex(hash: &str) -> Result<(), SessionError> {
    if hash.is_empty() || hash.len() > MAX_HASH_LEN {
        return Err(SessionError::ManifestInvalid("invalid expectedHash".into()));
    }
    if !hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(SessionError::ManifestInvalid("expectedHash must be hex".into()));
    }
    Ok(())
}

pub fn validate_encoded_body(encoded: &str) -> Result<(), SessionError> {
    if encoded.len() > MAX_ENCODED_BODY {
        return Err(SessionError::ManifestInvalid("contentBase64 too large".into()));
    }
    Ok(())
}

pub fn validate_entries_json_len(json: &str) -> Result<(), SessionError> {
    if json.len() > MAX_ENTRIES_JSON {
        return Err(SessionError::ManifestInvalid("intent metadata too large".into()));
    }
    Ok(())
}
