//! Core-owned memory credential loader (`n3-provider-binding.md` §3.1).
//!
//! `credential_id` resolves to `<user_home>/.nexus42/memory-credentials/<id>.json`.
//! The file is operator-provisioned out of band; N3 adds no issuer, refresh
//! flow or generic `auth.json` credential map. The secret value stays inside
//! this loader and the remote adapter options it feeds — it is never logged,
//! persisted or returned from a public API.

use std::io::Read;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::{CoreError, CoreResult};
use crate::memory_selection::is_valid_credential_id;

/// Maximum accepted credential-file size (§3.1).
pub const MAX_CREDENTIAL_FILE_BYTES: u64 = 16 * 1024;
/// Coded refusal category for every credential-file failure (§3.1).
const CREDENTIAL_INVALID_CODE: &str = "memory_credential_invalid";

/// File shape, verbatim from §3.1. Unknown keys are refused so an ambiguous
/// document is never half-interpreted.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CredentialFile {
    identity_seed_hex: String,
    /// Declared field (§3.1), required: `null` means no proof, and an absent
    /// field is a malformed document (a plain `Option` would accept absence).
    capability_token: serde_json::Value,
}

/// A validated, loaded memory credential.
///
/// No `Clone`/`Serialize`/public accessor: the seed leaves this value only
/// through the crate-private accessor the remote dial path consumes.
pub struct MemoryCredential {
    identity_seed: [u8; 32],
    capability_token: Option<serde_json::Value>,
}

impl MemoryCredential {
    /// Load and validate the credential named by `credential_id`.
    ///
    /// # Errors
    ///
    /// Returns the coded `memory_credential_invalid` refusal — with a static
    /// message and no seed/signature material — when the id, path, file type,
    /// permissions, size or document shape is unacceptable.
    pub fn load(user_home: &Path, credential_id: &str) -> CoreResult<Self> {
        if !is_valid_credential_id(credential_id) {
            return Err(invalid("credential_id must match [A-Za-z0-9][A-Za-z0-9_-]{0,63}"));
        }
        let path = credential_file_path(user_home, credential_id);
        let bytes = read_protected_file(&path)?;
        let file: CredentialFile = serde_json::from_slice(&bytes)
            .map_err(|_| invalid("credential file is not the declared JSON document"))?;
        let identity_seed = parse_seed(&file.identity_seed_hex)?;
        let capability_token = match file.capability_token {
            serde_json::Value::Null => None,
            token => {
                validate_capability_token(&token)?;
                Some(token)
            }
        };
        Ok(Self {
            identity_seed,
            capability_token,
        })
    }

    /// Raw Ed25519 seed for the remote identity.
    #[allow(dead_code)] // consumed by the remote dial path (dependent plan)
    pub(crate) fn identity_seed(&self) -> &[u8; 32] {
        &self.identity_seed
    }

    /// Optional capability-token proof for outbound invokes.
    #[allow(dead_code)] // consumed by the remote dial path (dependent plan)
    pub(crate) fn capability_token(&self) -> Option<&serde_json::Value> {
        self.capability_token.as_ref()
    }
}

// A credential must never be printable: even a refactor that adds `{:?}` to a
// log line cannot leak the seed.
impl std::fmt::Debug for MemoryCredential {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("MemoryCredential(<redacted>)")
    }
}

fn credential_file_path(user_home: &Path, credential_id: &str) -> PathBuf {
    nexus_home_layout::nexus_root_from_home(user_home)
        .join("memory-credentials")
        .join(format!("{credential_id}.json"))
}

/// Open the credential file so the final component cannot be a symlink and a
/// substituted special file cannot block the open.
///
/// `O_NOFOLLOW` makes the refusal atomic with the open (no lstat-then-open
/// window) and `O_NONBLOCK` keeps a FIFO/device substitution from blocking
/// before the descriptor is inspected. Type, size and mode are decided from
/// the returned descriptor only.
#[cfg(unix)]
fn open_credential_file(path: &Path) -> CoreResult<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;

    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(path)
        .map_err(|_| invalid("credential file is not a regular file reached without a symlink"))
}

/// Platforms without a no-follow open flag keep a best-effort pathname check;
/// the descriptor checks below still apply. Unexercised: reported as a limit.
#[cfg(not(unix))]
fn open_credential_file(path: &Path) -> CoreResult<std::fs::File> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|_| invalid("credential file is missing or unreadable"))?;
    if metadata.file_type().is_symlink() {
        return Err(invalid("credential file must not be a symlink"));
    }
    std::fs::File::open(path).map_err(|_| invalid("credential file is missing or unreadable"))
}

fn read_protected_file(path: &Path) -> CoreResult<Vec<u8>> {
    let file = open_credential_file(path)?;
    let metadata = file
        .metadata()
        .map_err(|_| invalid("credential file is missing or unreadable"))?;
    if !metadata.is_file() {
        return Err(invalid("credential path must be a regular file"));
    }
    if metadata.len() > MAX_CREDENTIAL_FILE_BYTES {
        return Err(invalid("credential file exceeds 16384 bytes"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.mode() & 0o077 != 0 {
            return Err(invalid("credential file permissions must be 0600"));
        }
    }

    // Bound the read itself: a file that grows after the metadata check still
    // cannot make this loader allocate past the declared limit.
    let mut bytes = Vec::new();
    file.take(MAX_CREDENTIAL_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid("credential file is missing or unreadable"))?;
    if bytes.len() as u64 > MAX_CREDENTIAL_FILE_BYTES {
        return Err(invalid("credential file exceeds 16384 bytes"));
    }
    Ok(bytes)
}

fn parse_seed(identity_seed_hex: &str) -> CoreResult<[u8; 32]> {
    if identity_seed_hex.len() != 64
        || !identity_seed_hex
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(invalid("identity_seed_hex must be 64 lowercase hex characters"));
    }
    let mut seed = [0_u8; 32];
    for (slot, pair) in seed.iter_mut().zip(identity_seed_hex.as_bytes().chunks_exact(2)) {
        *slot = (hex_digit(pair[0]) << 4) | hex_digit(pair[1]);
    }
    Ok(seed)
}

fn hex_digit(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        _ => 0, // unreachable after the lowercase-hex check above
    }
}

/// Validate the nullable upstream `CapabilityTokenProof` (`{v, claims, sig}`)
/// without importing it: `spoke_connect::core::CapabilityTokenProof` is the
/// published type, but the default core graph carries no Connect edge (§6), so
/// this loader checks the same serialized shape here and the remote dial path
/// re-deserializes the value through the published type.
fn validate_capability_token(token: &serde_json::Value) -> CoreResult<()> {
    let proof = match token.as_object() {
        Some(object) if object.len() == 3 => object,
        _ => return Err(invalid("capability_token must be the {v, claims, sig} proof object")),
    };
    if !proof.get("v").is_some_and(serde_json::Value::is_u64) {
        return Err(invalid("capability_token.v must be the token format version"));
    }
    if !proof.get("claims").is_some_and(serde_json::Value::is_object) {
        return Err(invalid("capability_token.claims must be the signed claims object"));
    }
    if !proof
        .get("sig")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|sig| !sig.is_empty())
    {
        return Err(invalid("capability_token.sig must be the signature string"));
    }
    Ok(())
}

/// Static coded refusal: the message never interpolates file contents.
fn invalid(reason: &str) -> CoreError {
    CoreError::Coded {
        code: CREDENTIAL_INVALID_CODE.to_string(),
        message: format!("memory credential invalid: {reason}"),
    }
}
