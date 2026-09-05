//! v1.184 P3 Task 2 — closed `MemoryBearerRef` path/validation dispatch proofs.
//!
//! Covers: exact Creator/Character path layouts, bearer id validation
//! (format + traversal), and Character-arm SOUL/long-term-memory file I/O
//! isolation from the owning Creator's tree.

#![allow(clippy::unwrap_used)]

use nexus_creator_memory::bearer::MemoryBearerRef;
use nexus_creator_memory::errors::MemoryError;
use nexus_creator_memory::long_term_memory::LongTermMemory;
use nexus_creator_memory::{memory_io, soul_io};
use std::path::PathBuf;

const OWNER: &str = "ctr_ownerx";
const OTHER: &str = "ctr_othery";
const CHR_A: &str = "chr_0123456789abcdef0123456789abcdef";
const CHR_B: &str = "chr_aabbccddee00112233445566778899ff";

fn tmp(tag: &str) -> PathBuf {
    PathBuf::from(format!("/tmp/test_bearer_dispatch_{tag}_{}", std::process::id()))
}

fn creator() -> MemoryBearerRef<'static> {
    MemoryBearerRef::Creator(OWNER)
}

fn character() -> MemoryBearerRef<'static> {
    MemoryBearerRef::Character {
        owner_creator_id: OWNER,
        character_id: CHR_A,
    }
}

// ── Path dispatch ────────────────────────────────────────────────────────

#[test]
fn creator_arm_paths_match_legacy_layout() {
    let home = PathBuf::from("/h");
    let bearer = MemoryBearerRef::Creator("ctr_x");
    assert_eq!(
        bearer.soul_path(&home),
        PathBuf::from("/h/.nexus42/creators/ctr_x/SOUL.md")
    );
    assert_eq!(
        bearer.long_term_memory_dir(&home),
        PathBuf::from("/h/.nexus42/creators/ctr_x/memory/long-term")
    );
    assert_eq!(
        bearer.long_term_memory_path(&home, "some-slug"),
        PathBuf::from("/h/.nexus42/creators/ctr_x/memory/long-term/some-slug.md")
    );
}

#[test]
fn character_arm_paths_match_spec_layout() {
    let home = PathBuf::from("/h");
    let bearer = character();
    assert_eq!(
        bearer.soul_path(&home),
        PathBuf::from(format!(
            "/h/.nexus42/creators/{OWNER}/characters/{CHR_A}/SOUL.md"
        ))
    );
    assert_eq!(
        bearer.long_term_memory_dir(&home),
        PathBuf::from(format!(
            "/h/.nexus42/creators/{OWNER}/characters/{CHR_A}/memory/long-term"
        ))
    );
    assert_eq!(
        bearer.long_term_memory_path(&home, "some-slug"),
        PathBuf::from(format!(
            "/h/.nexus42/creators/{OWNER}/characters/{CHR_A}/memory/long-term/some-slug.md"
        ))
    );
}

#[test]
fn bearer_id_returns_primary_id() {
    assert_eq!(creator().id(), OWNER);
    assert_eq!(character().id(), CHR_A);
}

// ── Validation dispatch ──────────────────────────────────────────────────

#[test]
fn validate_accepts_well_formed_bearers() {
    assert!(creator().validate().is_ok());
    assert!(character().validate().is_ok());
}

#[test]
fn validate_rejects_bad_creator_ids() {
    for bad in ["usr_x", "ctr_", "ctr_../escape", "ctr_a/b", "ctr_a\\b"] {
        let err = MemoryBearerRef::Creator(bad).validate().unwrap_err();
        assert!(
            matches!(err, MemoryError::InvalidIdFormat(_)),
            "creator id {bad:?} must be InvalidIdFormat, got {err:?}"
        );
    }
}

#[test]
fn validate_rejects_bad_character_and_owner_ids() {
    // Bad owner.
    for bad_owner in ["usr_x", "../ctr_x", "ctr_a/b"] {
        let bearer = MemoryBearerRef::Character {
            owner_creator_id: bad_owner,
            character_id: CHR_A,
        };
        assert!(
            matches!(bearer.validate(), Err(MemoryError::InvalidIdFormat(_))),
            "owner {bad_owner:?} must be rejected"
        );
    }
    // Bad character id: wrong prefix, traversal, separators, uppercase hex,
    // wrong length, control characters, empty.
    for bad_chr in [
        "",
        "ctr_0123456789abcdef0123456789abcdef",
        "chr_../escape",
        "chr_a/b",
        "chr_a\\b",
        "chr_0123456789ABCDEF0123456789ABCDEF",
        "chr_0123",
        "chr_\u{0}null456789abcdef0123456789ab",
    ] {
        let bearer = MemoryBearerRef::Character {
            owner_creator_id: OWNER,
            character_id: bad_chr,
        };
        assert!(
            matches!(bearer.validate(), Err(MemoryError::InvalidIdFormat(_))),
            "character id {bad_chr:?} must be rejected"
        );
    }
}

// ── Character-arm file I/O ───────────────────────────────────────────────

#[test]
fn character_soul_roundtrip_isolated_from_creator_tree() {
    let home = tmp("soul");
    let _ = std::fs::remove_dir_all(&home);

    let doc = soul_io::create(&home, character()).unwrap();
    assert_eq!(doc.frontmatter.creator_id.as_deref(), Some(CHR_A));

    // File lives exactly at the Character path …
    let expected = home
        .join(".nexus42")
        .join("creators")
        .join(OWNER)
        .join("characters")
        .join(CHR_A)
        .join("SOUL.md");
    assert!(expected.exists(), "SOUL.md must exist at {expected:?}");

    // … and is invisible through the Creator arm of the same owner.
    assert!(!soul_io::exists(&home, creator()));
    assert!(!home
        .join(".nexus42")
        .join("creators")
        .join(OWNER)
        .join("SOUL.md")
        .exists());

    // Load / save / delete through the Character bearer.
    let mut loaded = soul_io::load(&home, character()).unwrap();
    loaded.set_personality("Cautious and brave.".to_string());
    soul_io::save(&home, character(), &loaded).unwrap();
    let reloaded = soul_io::load(&home, character()).unwrap();
    assert_eq!(
        reloaded.personality.as_deref().unwrap().trim(),
        "Cautious and brave."
    );

    soul_io::delete(&home, character()).unwrap();
    assert!(!expected.exists());
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn character_memory_roundtrip_and_scope_isolation() {
    let home = tmp("memory");
    let _ = std::fs::remove_dir_all(&home);

    let mut mem = LongTermMemory::new("story_summary");
    mem.set_body("The character remembers the bridge crossing.");
    memory_io::save_memory(&home, character(), "bridge-crossing", &mem).unwrap();

    // Listed/loaded only through the same Character bearer.
    assert_eq!(
        memory_io::list_memories(&home, character()).unwrap(),
        vec!["bridge-crossing"]
    );
    let loaded = memory_io::load_memory(&home, character(), "bridge-crossing").unwrap();
    assert!(loaded.body.contains("bridge crossing"));

    // Isolation: the owning Creator, a sibling Character, and a foreign
    // owner all see nothing.
    assert!(memory_io::list_memories(&home, creator()).unwrap().is_empty());
    let sibling = MemoryBearerRef::Character {
        owner_creator_id: OWNER,
        character_id: CHR_B,
    };
    assert!(memory_io::list_memories(&home, sibling).unwrap().is_empty());
    let foreign_owner = MemoryBearerRef::Character {
        owner_creator_id: OTHER,
        character_id: CHR_A,
    };
    assert!(memory_io::list_memories(&home, foreign_owner)
        .unwrap()
        .is_empty());

    memory_io::delete_memory(&home, character(), "bridge-crossing").unwrap();
    assert!(memory_io::list_memories(&home, character())
        .unwrap()
        .is_empty());
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn character_memory_rejects_traversal_slugs() {
    let home = tmp("slug");
    let _ = std::fs::remove_dir_all(&home);
    let mem = LongTermMemory::new("story_summary");
    for bad_slug in ["../escape", "a/b", "a\\b", "..", "with\u{0}null"] {
        assert!(
            memory_io::save_memory(&home, character(), bad_slug, &mem).is_err(),
            "slug {bad_slug:?} must be rejected"
        );
        assert!(memory_io::load_memory(&home, character(), bad_slug).is_err());
        assert!(memory_io::delete_memory(&home, character(), bad_slug).is_err());
    }
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn character_io_rejects_malformed_ids_before_any_fs_write() {
    let home = tmp("traversal");
    let _ = std::fs::remove_dir_all(&home);

    let bad = MemoryBearerRef::Character {
        owner_creator_id: OWNER,
        character_id: "chr_../escape",
    };
    assert!(matches!(
        soul_io::load(&home, bad),
        Err(MemoryError::InvalidIdFormat(_))
    ));
    assert!(matches!(
        soul_io::create(&home, bad),
        Err(MemoryError::InvalidIdFormat(_))
    ));
    assert!(matches!(
        soul_io::delete(&home, bad),
        Err(MemoryError::InvalidIdFormat(_))
    ));
    assert!(!soul_io::exists(&home, bad));

    let mem = LongTermMemory::new("story_summary");
    assert!(matches!(
        memory_io::save_memory(&home, bad, "ok-slug", &mem),
        Err(MemoryError::InvalidIdFormat(_))
    ));
    assert!(matches!(
        memory_io::list_memories(&home, bad),
        Err(MemoryError::InvalidIdFormat(_))
    ));

    // No filesystem side effects happened.
    assert!(!home.join(".nexus42").exists());
    let _ = std::fs::remove_dir_all(&home);
}
