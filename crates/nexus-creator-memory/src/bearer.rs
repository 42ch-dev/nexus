//! Closed memory bearer reference (v1.184 P3 Task 2).
//!
//! The Creator/Character memory pipeline is keyed by a **closed**
//! `MemoryBearerRef` — exactly two arms, no generic Actor framework:
//!
//! ```text
//! MemoryBearerRef =
//!   Creator { creator_id }
//!   | Character { owner_creator_id, character_id }
//! ```
//!
//! Every pipeline stage (`soul_io`, `memory_io`, `review`,
//! `experience_aggregation`, `personality_sync`, `soul_narrative`) dispatches
//! path resolution through this type. The Creator arm reproduces the legacy
//! `~/.nexus42/creators/<creator_id>/…` layout byte-for-byte; the Character
//! arm resolves to the spec layout
//! `~/.nexus42/creators/<owner_creator_id>/characters/<character_id>/…`
//! (character-memory spec §2). All path construction goes through
//! `nexus-home-layout`; no caller builds bearer paths ad hoc.

use crate::errors::MemoryError;
use nexus_creator::is_valid_creator_id;
use std::path::{Path, PathBuf};

/// Memory directory relative to the bearer's home layout root.
const MEMORY_SUBDIR: &str = "memory/long-term";

/// Closed bearer reference for the shared Creator/Character memory pipeline.
///
/// Copyable borrowed form; pipeline functions take it by value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryBearerRef<'a> {
    /// The Creator bearer: files under `~/.nexus42/creators/<creator_id>/`.
    Creator(&'a str),
    /// A Character bearer: files under
    /// `~/.nexus42/creators/<owner_creator_id>/characters/<character_id>/`.
    /// `owner_creator_id` is the actor/owner provenance; the daemon wiring
    /// resolves it from the active Creator and verifies ownership via
    /// `nexus-local-db` before any Character persistence.
    Character {
        /// Owning Creator id (provenance; also the path parent).
        owner_creator_id: &'a str,
        /// Character id (`chr_` + 32 lowercase hex, as minted).
        character_id: &'a str,
    },
}

impl MemoryBearerRef<'_> {
    /// The bearer's primary id (`ctr_…` for Creator, `chr_…` for Character).
    #[must_use]
    pub fn id(&self) -> &str {
        match *self {
            Self::Creator(creator_id) => creator_id,
            Self::Character { character_id, .. } => character_id,
        }
    }

    /// Validate both id components for format and path safety.
    ///
    /// Creator arm: `^ctr_[a-zA-Z0-9]+$` (legacy rule, unchanged message).
    /// Character arm: owner must be a valid Creator id and the Character id
    /// must match the minted format `^chr_[0-9a-f]{32}$`.
    ///
    /// # Errors
    ///
    /// Returns `MemoryError::InvalidIdFormat` for malformed or path-unsafe ids.
    pub fn validate(&self) -> Result<(), MemoryError> {
        match *self {
            Self::Creator(creator_id) => validate_creator_id(creator_id),
            Self::Character {
                owner_creator_id,
                character_id,
            } => {
                validate_creator_id(owner_creator_id)?;
                if nexus_home_layout::is_valid_character_id(character_id) {
                    Ok(())
                } else {
                    Err(MemoryError::InvalidIdFormat(format!(
                        "character_id '{character_id}' is not a valid CharacterId (must match ^chr_[0-9a-f]{{32}}$ and contain no path separators or control characters)"
                    )))
                }
            }
        }
    }

    /// Resolve the SOUL.md path for this bearer via the home layout.
    ///
    /// # Panics (defense-in-depth)
    ///
    /// The layout helpers panic on path-traversal id components. Callers
    /// should run [`Self::validate`] first (all `soul_io`/`memory_io`
    /// entrypoints do).
    #[must_use]
    pub fn soul_path(&self, home: &Path) -> PathBuf {
        match *self {
            Self::Creator(creator_id) => {
                nexus_home_layout::creator_soul_md_path(home, creator_id)
            }
            Self::Character {
                owner_creator_id,
                character_id,
            } => nexus_home_layout::character_soul_md_path(home, owner_creator_id, character_id),
        }
    }

    /// Resolve the long-term memory directory for this bearer.
    ///
    /// The Creator arm is byte-identical to the legacy
    /// `memory_io::memory_dir` construction.
    ///
    /// # Panics (defense-in-depth)
    ///
    /// Same contract as [`Self::soul_path`].
    #[must_use]
    pub fn long_term_memory_dir(&self, home: &Path) -> PathBuf {
        match *self {
            Self::Creator(creator_id) => nexus_home_layout::nexus_root_from_home(home)
                .join("creators")
                .join(creator_id)
                .join(MEMORY_SUBDIR),
            Self::Character {
                owner_creator_id,
                character_id,
            } => nexus_home_layout::character_long_term_memory_dir(
                home,
                owner_creator_id,
                character_id,
            ),
        }
    }

    /// Resolve the full path for a long-term memory file (`<slug>.md`).
    ///
    /// # Panics (defense-in-depth)
    ///
    /// Same contract as [`Self::soul_path`]; slug safety is enforced by
    /// `memory_io` (`slug_is_safe`) before any I/O.
    #[must_use]
    pub fn long_term_memory_path(&self, home: &Path, slug: &str) -> PathBuf {
        self.long_term_memory_dir(home).join(format!("{slug}.md"))
    }
}

/// Validate that a Creator id is safe to use in filesystem paths.
///
/// Rejects ids containing path separators, `..` components, backslashes,
/// or control characters, and requires the standard `ctr_` prefix.
/// Message text is the pre-refactor `soul_io`/`memory_io` wording — kept
/// byte-identical for Creator-facing behavior.
fn validate_creator_id(creator_id: &str) -> Result<(), MemoryError> {
    if is_valid_creator_id(creator_id) {
        Ok(())
    } else {
        Err(MemoryError::InvalidIdFormat(format!(
            "creator_id '{creator_id}' is not a valid CreatorId (must match ^ctr_[a-zA-Z0-9]+$ and contain no path separators or control characters)"
        )))
    }
}
