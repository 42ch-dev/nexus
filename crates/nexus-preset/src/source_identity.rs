//! Content-addressed preset source identity.
// invariant: the hash covers the manifest plus every referenced asset byte, never the YAML alone.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// note: Content-addressed preset source identity (A2/A7).
///
/// The content hash is over the manifest **and** referenced prompt/template
/// bytes — not the YAML hash alone — so a changed template invalidates the
/// identity and recovery refuses to fall back to current bytes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PresetSourceIdentity {
    /// note: Compiled-in embedded preset.
    Embedded {
        /// note: Preset id.
        preset_id: String,
        /// note: blake3 over manifest + referenced template bytes.
        content_hash: [u8; 32],
    },
    /// note: On-disk preset bundle (user/system directory).
    Directory {
        /// note: Resolved bundle root.
        root: PathBuf,
        /// note: blake3 over manifest + referenced template bytes.
        content_hash: [u8; 32],
    },
}
