//! Connect Host peer allowlist (`~/.nexus42/connect/allowlist.json` +
//! repeatable `--allow-peer` overlay).
//!
//! N-C0 product contract (draft §2.3): the allowlist is the trust root.
//! File shape: `{ "peer_ids": ["12D3…", …] }`. A missing file ⇒ empty list
//! ⇒ **fail-closed** (spoke-connect rejects every remote peer). The operator
//! edits the allowlist out-of-band; there is no online enroll endpoint in
//! N-C0.

use crate::errors::{CliError, Result};
use libp2p::PeerId;
use serde::Deserialize;
use std::path::Path;

/// On-disk allowlist shape (`allowlist.json`).
#[derive(Debug, Deserialize)]
struct AllowlistFile {
    peer_ids: Vec<String>,
}

/// Load the effective allowlist: file entries ∪ `--allow-peer` CLI entries.
///
/// A missing file is not an error — it contributes nothing (fail-closed).
/// An unreadable/malformed file or an unparseable peer id is a hard error so
/// a typo cannot silently open or lock the host.
///
/// # Errors
/// Returns [`CliError::Io`] when the file exists but cannot be read, or
/// [`CliError::Config`] when the file is malformed or an entry is not a
/// valid libp2p `PeerId`.
pub fn load(nexus_home: &Path, cli_peers: &[String]) -> Result<Vec<PeerId>> {
    let path = nexus_home_layout::connect_allowlist_path(nexus_home);

    let mut entries = Vec::new();
    match std::fs::read_to_string(&path) {
        Ok(content) => {
            let parsed: AllowlistFile = serde_json::from_str(&content).map_err(|e| {
                CliError::Config(format!("invalid allowlist at {}: {e}", path.display()))
            })?;
            entries.extend(parsed.peer_ids);
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // Missing file ⇒ empty allowlist ⇒ fail-closed.
        }
        Err(e) => return Err(CliError::Io(e)),
    }
    entries.extend(cli_peers.iter().cloned());

    entries
        .iter()
        .map(|peer| {
            peer.parse::<PeerId>().map_err(|e| {
                CliError::Config(format!("invalid peer id {peer:?} in allowlist: {e}"))
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peer_id(seed: u8) -> PeerId {
        libp2p::identity::Keypair::ed25519_from_bytes([seed; 32])
            .expect("seed is a valid ed25519 secret")
            .public()
            .to_peer_id()
    }

    #[test]
    fn missing_file_is_empty_and_fail_closed() {
        let temp = tempfile::tempdir().expect("tempdir");
        let allowlist = load(temp.path(), &[]).expect("missing file loads as empty");
        assert!(
            allowlist.is_empty(),
            "missing allowlist file must resolve to an empty (fail-closed) allowlist"
        );
    }

    #[test]
    fn cli_peers_overlay_on_empty_file() {
        let temp = tempfile::tempdir().expect("tempdir");
        let peer = peer_id(1);
        let allowlist = load(temp.path(), &[peer.to_string()]).expect("cli peer loads");
        assert_eq!(allowlist, vec![peer]);
    }

    #[test]
    fn file_entries_are_unioned_with_cli_peers() {
        let temp = tempfile::tempdir().expect("tempdir");
        let file_peer = peer_id(2);
        let cli_peer = peer_id(3);
        let path = nexus_home_layout::connect_allowlist_path(temp.path());
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(
            &path,
            serde_json::json!({ "peer_ids": [file_peer.to_string()] }).to_string(),
        )
        .expect("write allowlist");

        let allowlist = load(temp.path(), &[cli_peer.to_string()]).expect("load");
        assert_eq!(allowlist, vec![file_peer, cli_peer]);
    }

    #[test]
    fn malformed_file_is_a_config_error() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = nexus_home_layout::connect_allowlist_path(temp.path());
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(&path, "{ not json").expect("write");

        let err = load(temp.path(), &[]).expect_err("malformed allowlist rejected");
        assert!(matches!(err, CliError::Config(_)), "got {err:?}");
    }

    #[test]
    fn invalid_peer_id_is_a_config_error() {
        let temp = tempfile::tempdir().expect("tempdir");
        let err =
            load(temp.path(), &["not-a-peer-id".into()]).expect_err("invalid peer id rejected");
        assert!(matches!(err, CliError::Config(_)), "got {err:?}");
    }
}
