//! Content-addressed preset source identity.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Content-addressed preset source identity (A2/A7).
///
/// The content hash is over the manifest **and** referenced prompt/template
/// bytes — not the YAML hash alone — so a changed template invalidates the
/// identity and recovery refuses to fall back to current bytes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PresetSourceIdentity {
    /// Compiled-in embedded preset.
    Embedded {
        /// Preset id.
        preset_id: String,
        /// blake3 over manifest + referenced template bytes.
        content_hash: [u8; 32],
    },
    /// On-disk preset bundle (user/system directory).
    Directory {
        /// Resolved bundle root.
        root: PathBuf,
        /// blake3 over manifest + referenced template bytes.
        content_hash: [u8; 32],
    },
}
