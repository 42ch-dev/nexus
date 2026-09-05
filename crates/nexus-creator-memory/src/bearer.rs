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
    pub const fn id(&self) -> &str {
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
    /// # Panics
    ///
    /// Defense-in-depth:
    ///
    /// The layout helpers panic on path-traversal id components. Callers
    /// should run [`Self::validate`] first (all `soul_io`/`memory_io`
    /// entrypoints do).
    #[must_use]
    pub fn soul_path(&self, home: &Path) -> PathBuf {
        match *self {
            Self::Creator(creator_id) => nexus_home_layout::creator_soul_md_path(home, creator_id),
            Self::Character {
                owner_creator_id,
                character_id,
            } => nexus_home_layout::character_soul_md_path(home, owner_creator_id, character_id),
        }
    }

    /// Resolve the long-term memory directory after validating bearer ids.
    ///
    /// Prefer this over [`Self::long_term_memory_dir`] at public I/O entrypoints
    /// so path construction is gated on closed Creator/Character id formats.
    ///
    /// # Errors
    ///
    /// Returns [`MemoryError::InvalidIdFormat`] when either id component is
    /// malformed or path-unsafe.
    pub fn validated_long_term_memory_dir(&self, home: &Path) -> Result<PathBuf, MemoryError> {
        self.validate()?;
        Ok(self.long_term_memory_dir(home))
    }

    /// Resolve the long-term memory directory for this bearer.
    ///
    /// The Creator arm is byte-identical to the legacy
    /// `memory_io::memory_dir` construction.
    ///
    /// # Panics
    ///
    /// Defense-in-depth:
    ///
    /// Both id components are path-safety-asserted at the builder boundary —
    /// the Creator id is run through the home-layout validator (rejecting
    /// `/`, `\`, `..`, control chars) and the Character id through the
    /// Character home-layout helper — so a direct caller cannot obtain a path
    /// outside the intended bearer root.
    #[must_use]
    pub fn long_term_memory_dir(&self, home: &Path) -> PathBuf {
        match *self {
            Self::Creator(creator_id) => {
                if let Err(msg) = nexus_home_layout::validate_creator_id_safe(creator_id) {
                    panic!("{msg}");
                }
                nexus_home_layout::nexus_root_from_home(home)
                    .join("creators")
                    .join(creator_id)
                    .join(MEMORY_SUBDIR)
            }
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
    /// # Panics
    ///
    /// Defense-in-depth:
    ///
    /// The slug is validated with [`crate::long_term_memory::slug_is_safe`] at
    /// the builder boundary (rejecting `..`, `/`, `\`, empty, and control
    /// chars) so a direct caller cannot construct a traversal-form slug path.
    #[must_use]
    pub fn long_term_memory_path(&self, home: &Path, slug: &str) -> PathBuf {
        assert!(
            crate::long_term_memory::slug_is_safe(slug),
            "slug '{slug}' is not path-safe (rejected: contains '..', '/', '\\', control characters, or is empty)"
        );
        self.long_term_memory_dir(home).join(format!("{slug}.md"))
    }

    /// Resolve the ACP worker identity for this bearer.
    ///
    /// The `creator_id` is the Creator that owns a registered worker (== the
    /// bearer id for the Creator arm; the Character's `owner_creator_id` for
    /// the Character arm, so a Character reflection is routed by its owner
    /// Creator, never by its `chr_…` storage id). `character_id` is `Some`
    /// only for a Character bearer and preserves the storage/bearer identity.
    #[must_use]
    pub const fn identity(&self) -> BearerIdentity<'_> {
        match *self {
            Self::Creator(creator_id) => BearerIdentity {
                creator_id,
                character_id: None,
            },
            Self::Character {
                owner_creator_id,
                character_id,
            } => BearerIdentity {
                creator_id: owner_creator_id,
                character_id: Some(character_id),
            },
        }
    }
}

/// The ACP worker identity of a bearer: the owner Creator that routes to the
/// worker registry plus the optional Character being reflected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BearerIdentity<'a> {
    /// Owner Creator id used for ACP worker routing (`registry.get`).
    pub creator_id: &'a str,
    /// Character id when the bearer is a Character; `None` for the Creator arm.
    pub character_id: Option<&'a str>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validated_long_term_memory_dir_rejects_invalid_bearer() {
        let home = PathBuf::from("/h");
        let result = MemoryBearerRef::Creator("../../etc").validated_long_term_memory_dir(&home);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("invalid ID format"));
    }

    #[test]
    fn creator_path_builder_rejects_traversal_id() {
        let home = PathBuf::from("/h");
        for bad in ["../ctr_x", "ctr_a/b", "ctr_a\\b", "ctr_\u{7}bell"] {
            let r = std::panic::catch_unwind(|| {
                let _ = MemoryBearerRef::Creator(bad).long_term_memory_dir(&home);
            });
            assert!(r.is_err(), "creator id {bad:?} must be rejected");
        }
    }

    #[test]
    fn character_path_builder_rejects_traversal_ids() {
        let home = PathBuf::from("/h");
        let owner = "ctr_ownerx";
        for bad_chr in ["chr_../escape", "chr_a/b", "chr_a\\b", ""] {
            let r = std::panic::catch_unwind(|| {
                let _ = MemoryBearerRef::Character {
                    owner_creator_id: owner,
                    character_id: bad_chr,
                }
                .long_term_memory_dir(&home);
            });
            assert!(r.is_err(), "character id {bad_chr:?} must be rejected");
        }
        for bad_owner in ["../ctr_x", "ctr_a/b", "ctr_.."] {
            let r = std::panic::catch_unwind(|| {
                let _ = MemoryBearerRef::Character {
                    owner_creator_id: bad_owner,
                    character_id: "chr_0123456789abcdef0123456789abcdef",
                }
                .long_term_memory_dir(&home);
            });
            assert!(r.is_err(), "owner id {bad_owner:?} must be rejected");
        }
    }

    #[test]
    fn memory_path_builder_rejects_unsafe_slug() {
        let home = PathBuf::from("/h");
        let bearer = MemoryBearerRef::Creator("ctr_x");
        for bad in ["../etc", "a/b", "a\\b", "..", "", "with\u{0}null"] {
            let r = std::panic::catch_unwind(|| {
                let _ = bearer.long_term_memory_path(&home, bad);
            });
            assert!(r.is_err(), "slug {bad:?} must be rejected");
        }
    }

    #[test]
    fn memory_path_builder_keeps_safe_slugs() {
        let home = PathBuf::from("/h");
        let bearer = MemoryBearerRef::Creator("ctr_x");
        assert_eq!(
            bearer.long_term_memory_path(&home, "some-slug"),
            PathBuf::from("/h/.nexus42/creators/ctr_x/memory/long-term/some-slug.md")
        );
    }

    #[test]
    fn identity_routes_creator_by_self_and_character_by_owner() {
        let cb = MemoryBearerRef::Creator("ctr_ownerx");
        let ident = cb.identity();
        assert_eq!(ident.creator_id, "ctr_ownerx");
        assert_eq!(ident.character_id, None);

        let hb = MemoryBearerRef::Character {
            owner_creator_id: "ctr_ownerx",
            character_id: "chr_0123456789abcdef0123456789abcdef",
        };
        let ident = hb.identity();
        assert_eq!(ident.creator_id, "ctr_ownerx", "routed by owner Creator");
        assert_eq!(
            ident.character_id,
            Some("chr_0123456789abcdef0123456789abcdef"),
            "storage identity preserved"
        );
    }
}
