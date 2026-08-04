//! Connect Host identity key persistence (`~/.nexus42/connect/identity.key`).
//!
//! N-C0 product contract (draft §2.1): an Ed25519 libp2p identity keypair,
//! created once with owner-only permissions, reused across restarts so the
//! derived `PeerId` (the network identity and hello signer) is stable for
//! the installation.

use crate::errors::{CliError, Result};
use libp2p::identity::Keypair;
use std::path::Path;

/// Load the persisted Connect identity key, or generate + persist a fresh
/// Ed25519 keypair on first use (create-once, 0600 on Unix).
///
/// The file stores the libp2p protobuf key encoding (`Keypair::to_bytes`),
/// the canonical libp2p key serialization. spoke-connect exposes no
/// identity-persistence helper, so this module owns the file format.
///
/// # Parameters
/// `home` is the **raw user home** (`$HOME`); this fn joins `.nexus42`
/// internally via `connect_identity_key_path`, so callers MUST NOT pre-join
/// `~/.nexus42`.
///
/// # Errors
/// Returns [`CliError::Io`] on filesystem failure, or [`CliError::Config`]
/// when an existing key file is corrupt or unreadable.
pub fn load_or_create_identity(home: &Path) -> Result<Keypair> {
    use std::io::Write;

    let path = nexus_home_layout::connect_identity_key_path(home);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Serialize before touching the filesystem so a serialization failure can
    // never leave a partial identity file behind.
    let keypair = Keypair::generate_ed25519();
    let encoded = keypair
        .to_protobuf_encoding()
        .map_err(|e| CliError::Config(format!("identity key serialization failed: {e}")))?;

    // `create_new` eliminates the TOCTOU race between the existence check and
    // the write (same pattern as the device-id file in `nexus-home-layout`).
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    // W-1: apply the owner-only mode at creation time (atomic create-with-mode)
    // so the key is never observable at a permissive mode — not even if the
    // process crashes between open and write (the old open-then-chmod window).
    // umask can only tighten the bits, never loosen them. On non-unix, the
    // platform default applies (best available).
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    match options.open(&path) {
        Ok(mut file) => {
            if let Err(e) = file.write_all(&encoded) {
                // S-1: never leave a partial key file behind — a corrupt key
                // would block every later start (the reload path rejects it).
                let _ = std::fs::remove_file(&path);
                return Err(CliError::Io(e));
            }
            Ok(keypair)
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            let bytes = std::fs::read(&path)?;
            #[cfg(unix)]
            harden_identity_key_permissions(&path)?;
            Keypair::from_protobuf_encoding(&bytes).map_err(|e| {
                CliError::Config(format!(
                    "invalid Connect identity key at {}: {e}",
                    path.display()
                ))
            })
        }
        Err(e) => Err(CliError::Io(e)),
    }
}

/// Ensure the identity key file is owner-only (0600) on the reload path.
///
/// Files created before the atomic `mode(0o600)` fix (open-then-chmod) may
/// still sit at a permissive mode if a previous process crashed between the
/// two calls. Hardening is preferred over erroring: it self-heals existing
/// installations instead of refusing to start over a permissions issue.
#[cfg(unix)]
fn harden_identity_key_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mode = std::fs::metadata(path)?.permissions().mode();
    if (mode & 0o777) > 0o600 {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_created_once_and_reloaded_stable() {
        let temp = tempfile::tempdir().expect("tempdir");
        let home = temp.path();

        let created = load_or_create_identity(home).expect("create");
        let created_peer = created.public().to_peer_id();

        // Second call must reload the same keypair (stable PeerId).
        let reloaded = load_or_create_identity(home).expect("reload");
        assert_eq!(
            reloaded.public().to_peer_id(),
            created_peer,
            "identity must persist across calls"
        );

        // The persisted file is the protobuf encoding (reloadable directly).
        let path = nexus_home_layout::connect_identity_key_path(home);
        let bytes = std::fs::read(&path).expect("identity file exists");
        let from_file = Keypair::from_protobuf_encoding(&bytes).expect("protobuf key parses");
        assert_eq!(from_file.public().to_peer_id(), created_peer);
    }

    #[cfg(unix)]
    #[test]
    fn identity_file_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().expect("tempdir");
        let home = temp.path();
        let _ = load_or_create_identity(home).expect("create");
        let path = nexus_home_layout::connect_identity_key_path(home);
        let mode = std::fs::metadata(&path)
            .expect("metadata")
            .permissions()
            .mode();
        assert_eq!(
            mode & 0o777,
            0o600,
            "identity key must be owner-only (0600)"
        );
    }

    #[cfg(unix)]
    #[test]
    fn reload_hardens_permissive_key_mode() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().expect("tempdir");
        let home = temp.path();
        let path = nexus_home_layout::connect_identity_key_path(home);

        // Simulate a key left behind by the old open-then-chmod path (crash
        // between open and chmod ⇒ file stuck at 0644).
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        let keypair = Keypair::generate_ed25519();
        std::fs::write(&path, keypair.to_protobuf_encoding().expect("encode")).expect("write key");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).expect("chmod");

        let reloaded = load_or_create_identity(home).expect("reload");
        assert_eq!(
            reloaded.public().to_peer_id(),
            keypair.public().to_peer_id(),
            "reload must still yield the persisted keypair"
        );
        let mode = std::fs::metadata(&path)
            .expect("metadata")
            .permissions()
            .mode();
        assert_eq!(
            mode & 0o777,
            0o600,
            "reload must harden a permissive key mode to 0600"
        );
    }

    #[test]
    fn corrupt_identity_file_is_rejected() {
        let temp = tempfile::tempdir().expect("tempdir");
        let home = temp.path();
        let path = nexus_home_layout::connect_identity_key_path(home);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(&path, b"not-a-keypair").expect("write");

        let err = load_or_create_identity(home).expect_err("corrupt key must be rejected");
        assert!(
            matches!(err, CliError::Config(_)),
            "corrupt identity key is a configuration error: {err:?}"
        );
    }
}
