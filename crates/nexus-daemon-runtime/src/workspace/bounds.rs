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
/// Max stage/backup basename length.
pub const MAX_BASENAME_LEN: usize = 256;

pub fn validate_relative_path(path: &str) -> Result<(), SessionError> {
    if path.is_empty() {
        return Err(SessionError::ManifestInvalid(
            "path must not be empty".into(),
        ));
    }
    if path.len() > MAX_PATH_LEN {
        return Err(SessionError::ManifestInvalid("path too long".into()));
    }
    if path.contains("..") || path.starts_with('/') {
        return Err(SessionError::ManifestInvalid(
            "path must be relative".into(),
        ));
    }
    Ok(())
}

pub fn validate_hash_hex(hash: &str) -> Result<(), SessionError> {
    if hash.len() != MAX_HASH_LEN {
        return Err(SessionError::ManifestInvalid(
            "expectedHash must be 64-char lowercase hex".into(),
        ));
    }
    if !hash.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')) {
        return Err(SessionError::ManifestInvalid(
            "expectedHash must be lowercase hex".into(),
        ));
    }
    Ok(())
}

pub fn validate_encoded_body(encoded: &str) -> Result<(), SessionError> {
    if encoded.len() > MAX_ENCODED_BODY {
        return Err(SessionError::ManifestInvalid(
            "contentBase64 too large".into(),
        ));
    }
    Ok(())
}

pub fn validate_entries_json_len(json: &str) -> Result<(), SessionError> {
    if json.len() > MAX_ENTRIES_JSON {
        return Err(SessionError::ManifestInvalid(
            "intent metadata too large".into(),
        ));
    }
    Ok(())
}

pub fn validate_stage_basename(name: &str) -> Result<(), SessionError> {
    if name.is_empty() || name.len() > MAX_BASENAME_LEN {
        return Err(SessionError::ManifestInvalid(
            "invalid stage basename".into(),
        ));
    }
    if name.contains('/') || name.contains('\\') || name.contains("..") {
        return Err(SessionError::ManifestInvalid(
            "invalid stage basename".into(),
        ));
    }
    if !name.starts_with(".nexus-") {
        return Err(SessionError::ManifestInvalid(
            "invalid stage basename".into(),
        ));
    }
    Ok(())
}

pub fn validate_intent_op(op: &str) -> Result<(), SessionError> {
    match op {
        "create" | "modify" | "delete" => Ok(()),
        _ => Err(SessionError::ManifestInvalid(format!(
            "invalid intent op: {op}"
        ))),
    }
}
