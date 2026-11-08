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
/// Coded refusal category for every credential-file failure (§3.1). Shared with
/// the remote dial, which refuses a proof that does not decode through the
/// published type under the same category.
pub const CREDENTIAL_INVALID_CODE: &str = "memory_credential_invalid";

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
            return Err(invalid(
                "credential_id must match [A-Za-z0-9][A-Za-z0-9_-]{0,63}",
            ));
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
    pub(crate) const fn identity_seed(&self) -> &[u8; 32] {
        &self.identity_seed
    }

    /// Optional capability-token proof for outbound invokes.
    #[allow(dead_code)] // consumed by the remote dial path (dependent plan)
    pub(crate) const fn capability_token(&self) -> Option<&serde_json::Value> {
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

/// `FILE_FLAG_OPEN_REPARSE_POINT` opens the reparse point itself instead of
/// resolving it, so a symlink/junction substitution cannot redirect the open;
/// the descriptor's reparse attribute then refuses it. A directory needs a
/// backup-semantics handle and fails the open, which is also a refusal.
#[cfg(windows)]
fn open_credential_file(path: &Path) -> CoreResult<std::fs::File> {
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};

    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;

    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|_| invalid("credential file is not a regular file reached without a symlink"))?;
    let metadata = file
        .metadata()
        .map_err(|_| invalid("credential file is missing or unreadable"))?;
    if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(invalid("credential file must not be a symlink"));
    }
    Ok(file)
}

/// No race-free no-follow open exists on this target, so the credential path
/// fails closed instead of keeping a check-then-open credential path.
#[cfg(not(any(unix, windows)))]
fn open_credential_file(_path: &Path) -> CoreResult<std::fs::File> {
    Err(invalid(
        "credential files cannot be opened without following links on this platform",
    ))
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

    // Windows carries no Unix mode bit, so the privacy policy is the DACL of
    // the already-open handle (§3.1): only the file's owner, SYSTEM and the
    // Administrators group may hold read, and an unverifiable policy refuses
    // before any byte is accepted.
    #[cfg(windows)]
    nexus_config_metadata::grants_read_only_to_trusted_principals(&file).map_err(|_| {
        invalid(
            "credential file permissions must be restricted to the owner, SYSTEM and Administrators",
        )
    })?;

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
        return Err(invalid(
            "identity_seed_hex must be 64 lowercase hex characters",
        ));
    }
    let mut seed = [0_u8; 32];
    for (slot, pair) in seed
        .iter_mut()
        .zip(identity_seed_hex.as_bytes().as_chunks::<2>().0)
    {
        *slot = (hex_digit(pair[0]) << 4) | hex_digit(pair[1]);
    }
    Ok(seed)
}

const fn hex_digit(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        _ => 0, // unreachable after the lowercase-hex check above
    }
}

/// Borrowed view of the published `spoke_connect::core::CapabilityTokenProof`
/// wire shape (§3.1): the default core graph carries no Connect edge, so the
/// value is deserialized against this mirror of the published
/// `CapabilityTokenProof`/`CapabilityClaims` fields, types and
/// `deny_unknown_fields` rules; the remote dial path re-deserializes it
/// through the published type.
// The fields exist so serde enforces the published shape; none is read.
#[allow(dead_code)]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProofView<'a> {
    v: u64,
    claims: ClaimsView<'a>,
    sig: &'a str,
}

#[allow(dead_code)]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ClaimsView<'a> {
    iss: &'a str,
    sub: &'a str,
    aud: &'a str,
    capabilities: Vec<&'a str>,
    exp: u64,
    #[serde(default)]
    iat: Option<u64>,
    #[serde(default)]
    jti: Option<&'a str>,
}

/// Validate the nullable upstream `CapabilityTokenProof` (`{v, claims, sig}`)
/// against the published serialized shape without importing it.
fn validate_capability_token(token: &serde_json::Value) -> CoreResult<()> {
    ProofView::deserialize(token)
        .map(|_| ())
        .map_err(|_| invalid("capability_token must be the published {v, claims, sig} proof shape"))
}

/// Static coded refusal: the message never interpolates file contents.
fn invalid(reason: &str) -> CoreError {
    CoreError::Coded {
        code: CREDENTIAL_INVALID_CODE.to_string(),
        message: format!("memory credential invalid: {reason}"),
    }
}
