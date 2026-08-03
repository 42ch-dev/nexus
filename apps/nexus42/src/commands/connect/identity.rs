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
/// # Errors
/// Returns [`CliError::Io`] on filesystem failure, or [`CliError::Config`]
/// when an existing key file is corrupt or unreadable.
pub fn load_or_create_identity(nexus_home: &Path) -> Result<Keypair> {
    use std::io::Write;

    let path = nexus_home_layout::connect_identity_key_path(nexus_home);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // `create_new` eliminates the TOCTOU race between the existence check and
    // the write (same pattern as the device-id file in `nexus-home-layout`).
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
    {
        Ok(mut file) => {
            let keypair = Keypair::generate_ed25519();
            let encoded = keypair
                .to_protobuf_encoding()
                .map_err(|e| CliError::Config(format!("identity key serialization failed: {e}")))?;
            file.write_all(&encoded)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
            }
            Ok(keypair)
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            let bytes = std::fs::read(&path)?;
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
