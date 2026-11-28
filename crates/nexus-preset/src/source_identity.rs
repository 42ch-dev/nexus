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
        /// How `root` was obtained; the bytes always came from the root
        /// descriptor, this only qualifies the recorded location.
        #[serde(default, skip_serializing_if = "RootLocation::is_descriptor")]
        location: RootLocation,
    },
}

/// How a [`PresetSourceIdentity::Directory`] root path was obtained.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RootLocation {
    /// The path the reads' own descriptor reports, so the recorded location and
    /// the hashed bytes describe one object.
    #[default]
    Descriptor,
    /// The platform could not name the descriptor (no `/proc/self/fd`), so the
    /// caller's path was canonicalised instead. The reads still used the
    /// descriptor; only the recorded location is a path query, and it may name
    /// a different directory if that path is later replaced.
    PathQuery,
}

impl RootLocation {
    /// Whether this location came from the root descriptor.
    // serde calls `skip_serializing_if` predicates with a reference.
    #[allow(clippy::trivially_copy_pass_by_ref)]
    const fn is_descriptor(&self) -> bool {
        matches!(self, Self::Descriptor)
    }
}
