//! Persistent Ed25519 identity for the peer-tools daemon lane (V1.174 P0,
//! AR-67 §3.2).
//!
//! The responder's hello identity must be stable across daemon restarts so
//! dialers can preconfigure its public key and peer id. The identity is a
//! raw 32-byte Ed25519 seed persisted at `~/.nexus42/connect/identity.key`
//! (RAW bytes; created once with mode `0600` on Unix).
//!
//! `identity.key` is the SAME path the `nexus42 connect` CLI uses for its
//! Connect-host identity — but that file stores the libp2p **protobuf**
//! encoding of a libp2p `Keypair`, while this lane needs the raw 32-byte
//! seed spoke's `RemoteIdentity` is built from. The two formats are
//! disjoint, so the peer-tools daemon uses a DISTINCT file:
//! `~/.nexus42/connect/daemon_identity.key` — reusing `identity.key` would
//! corrupt the connect-host identity (a 32-byte raw seed is not a valid
//! protobuf key and vice versa).

use std::path::Path;

/// Default identity file name (kept distinct from the connect-host
/// `identity.key` — different format, different trust role; see the module
/// docs).
const DAEMON_IDENTITY_FILE: &str = "daemon_identity.key";

/// Path of the persistent daemon-side Connect identity seed.
#[must_use]
pub fn daemon_identity_key_path(home: &Path) -> std::path::PathBuf {
    nexus_home_layout::connect_dir(home).join(DAEMON_IDENTITY_FILE)
}

/// Load the persisted daemon Connect identity seed, or generate + persist a
/// fresh one on first use.
///
/// The file stores the RAW 32-byte Ed25519 seed (the `RemoteIdentity.seed`
/// material) — created once with owner-only permissions (0600 on Unix) and
/// reused across restarts so the derived peer id is stable.
///
/// `home` is the RAW user home (`$HOME`); path helpers join `.nexus42`
/// internally.
///
/// # Errors
/// Returns an I/O error when the identity directory cannot be created or
/// the file cannot be written, or when an existing file has the wrong
/// length (corrupt or foreign — fail-closed: never reuse a mis-sized file).
pub fn load_or_create_identity(home: &Path) -> Result<[u8; 32], IdentityError> {
    use std::io::Write;

    let path = daemon_identity_key_path(home);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| IdentityError::Io {
            path: parent.display().to_string(),
            source: e,
        })?;
    }

    // `create_new` eliminates the TOCTOU race between the existence check
    // and the write (same pattern as the connect-host identity helper).
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    // W-1: apply the owner-only mode at creation time (atomic
    // create-with-mode) so the seed is never observable at a permissive
    // mode — even if the process crashes between open and write. umask can
    // only tighten bits, never loosen them. On non-unix the platform
    // default applies (best available).
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }

    match options.open(&path) {
        Ok(mut file) => {
            let seed = random_seed();
            file.write_all(&seed).map_err(|e| IdentityError::Io {
                path: path.display().to_string(),
                source: e,
            })?;
            file.sync_all().map_err(|e| IdentityError::Io {
                path: path.display().to_string(),
                source: e,
            })?;
            Ok(seed)
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            // Reload path: read + validate the persisted seed.
            let bytes = std::fs::read(&path).map_err(|e| IdentityError::Io {
                path: path.display().to_string(),
                source: e,
            })?;
            let found = bytes.len();
            let seed: [u8; 32] = bytes.try_into().map_err(|_| IdentityError::Corrupt {
                path: path.display().to_string(),
                found,
            })?;
            #[cfg(unix)]
            harden_permissions(&path);
            Ok(seed)
        }
        Err(e) => Err(IdentityError::Io {
            path: path.display().to_string(),
            source: e,
        }),
    }
}

/// Fill a 32-byte seed from the OS CSPRNG.
fn random_seed() -> [u8; 32] {
    let mut seed = [0u8; 32];
    getrandom::getrandom(&mut seed).expect("OS CSPRNG must be available");
    seed
}

/// Ensure the identity file is owner-only (0600) on the reload path
/// (self-heal files created before the atomic mode fix — same hardening
/// posture as the connect-host identity module).
#[cfg(unix)]
fn harden_permissions(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let Ok(metadata) = std::fs::metadata(path) else {
        return;
    };
    let mut permissions = metadata.permissions();
    let mode = permissions.mode() & 0o777;
    if mode != 0o600 {
        permissions.set_mode(0o600);
        let _ = std::fs::set_permissions(path, permissions);
    }
}

/// Identity persistence failure.
#[derive(Debug, thiserror::Error)]
pub enum IdentityError {
    /// Filesystem failure.
    #[error("identity IO error at {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    /// Existing key file has the wrong length (foreign format — fail-closed).
    #[error("existing identity key at {path} has {found} bytes, expected 32 (foreign format)")]
    Corrupt { path: String, found: usize },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_created_once_and_reloaded_stable() {
        let home = tempfile::tempdir().expect("tempdir");
        let created = load_or_create_identity(home.path()).expect("create");
        let reloaded = load_or_create_identity(home.path()).expect("reload");
        assert_eq!(
            created, reloaded,
            "identity must persist across calls (same seed)"
        );

        let path = daemon_identity_key_path(home.path());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path)
                .expect("metadata")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600, "identity key must be owner-only (0600)");
        }
        let bytes = std::fs::read(&path).expect("read");
        assert_eq!(bytes.len(), 32, "identity file stores the raw 32-byte seed");
    }

    #[test]
    fn foreign_length_file_is_rejected() {
        let home = tempfile::tempdir().expect("tempdir");
        let path = daemon_identity_key_path(home.path());
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(&path, b"not-a-32-byte-seed").expect("write foreign bytes");
        assert!(matches!(
            load_or_create_identity(home.path()),
            Err(IdentityError::Corrupt { .. })
        ));
    }
}
