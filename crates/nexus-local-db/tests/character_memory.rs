//! v1.184 P3 Task 1 — Character memory SQLite repository proofs.
//!
//! Covers the four dedicated `character_*` table families (pending review,
//! fragments, SOUL meta, SOUL narratives), Character A/B and binding-scope
//! isolation, restrictive FKs, nullable/per-binding cache uniqueness,
//! revision-checked local→shared promotion, binding-removal dependency
//! precedence, and Creator-table non-mutation.

#![allow(clippy::unwrap_used)]

use nexus_local_db::{
    add_actor_world_binding, create_character_with_initial_binding, create_character_fragment,
    create_character_pending_review, delete_character_fragment, delete_character_pending_review,
    delete_character_soul_meta, get_character_fragment, get_character_pending_review,
    get_character_soul_meta, get_character_soul_narrative, list_character_fragments,
    list_character_pending_reviews, character_soul_narrative_fragment_stats,
    count_character_pending_reviews, promote_character_fragment_to_shared, remove_binding,
    upsert_character_soul_meta, upsert_character_soul_narrative,     ActorContractConflict,
    CharacterPendingReviewRecord, CharacterSoulMeta,
    CharacterSoulNarrativeRecord, CreateBindingParams, CreateCharacterParams, LocalDbError,
    NewCharacterMemoryFragment,
};
use sqlx::SqlitePool;

const OWNER: &str = "ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const OTHER: &str = "ctr_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const WORLD_A: &str = "wld_worldA";
const WORLD_B: &str = "wld_worldB";
const WORLD_FOREIGN: &str = "wld_worldForeign";

struct Seed {
    char_a: String,
    char_b: String,
    binding_a1: String,
    binding_a2: String,
    binding_b1: String,
}

async fn fresh_pool() -> (SqlitePool, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let pool = nexus_local_db::open_pool(&dir.path().join("test.db"))
        .await
        .unwrap();
    nexus_local_db::run_migrations(&pool).await.unwrap();
    (pool, dir)
}

async fn seed_creators_worlds(pool: &SqlitePool) {
    for (id, name) in [(OWNER, "Owner"), (OTHER, "Other")] {
        nexus_local_db::ensure_creator_row(pool, id, name)
            .await
            .unwrap();
    }
    for (world_id, owner) in [
        (WORLD_A, OWNER),
        (WORLD_B, OWNER),
        (WORLD_FOREIGN, OTHER),
    ] {
        sqlx::query(
            "INSERT INTO narrative_worlds \
             (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
              time_policy, metadata_json, created_at) \
             VALUES (?, 'ws', ?, ?, ?, 'active', 'private', 'manual', '{}', datetime('now'))",
        )
        .bind(world_id)
        .bind(owner)
        .bind(world_id)
        .bind(world_id)
        .execute(pool)
        .await
        .unwrap();
    }
}

async fn create_character(pool: &SqlitePool, owner: &str, name: &str, world: &str) -> (String, String) {
    let created = create_character_with_initial_binding(
        pool,
        CreateCharacterParams {
            owner_creator_id: owner,
            display_name: name,
            image_uri: None,
            persona_json: "{}",
            world_id: world,
            world_sheet_entry_id: None,
        },
    )
    .await
    .unwrap();
    (created.character.character_id, created.binding.binding_id)
}

/// Seed two Characters: A bound to WORLD_A + WORLD_B, B bound to WORLD_B.
async fn seed(pool: &SqlitePool) -> Seed {
    seed_creators_worlds(pool).await;
    let (char_a, binding_a1) = create_character(pool, OWNER, "Ava", WORLD_A).await;
    let binding_a2 = add_actor_world_binding(
        pool,
        CreateBindingParams {
            owner_creator_id: OWNER,
            character_id: &char_a,
            world_id: WORLD_B,
            world_sheet_entry_id: None,
        },
    )
    .await
    .unwrap()
    .binding_id;
    let (char_b, binding_b1) = create_character(pool, OWNER, "Bree", WORLD_B).await;
    Seed {
        char_a,
        char_b,
        binding_a1,
        binding_a2,
        binding_b1,
    }
}

fn pending(pending_id: &str, character_id: &str, binding_id: Option<&str>) -> CharacterPendingReviewRecord {
    CharacterPendingReviewRecord {
        pending_id: pending_id.to_string(),
        session_id: format!("sess_{pending_id}"),
        character_id: character_id.to_string(),
        actor_world_binding_id: binding_id.map(str::to_string),
        task_kind: "brainstorm".to_string(),
        raw_digest: format!("digest for {pending_id}"),
        created_at: "2026-09-05T10:00:00Z".to_string(),
    }
}

fn fragment(fragment_id: &str, character_id: &str, binding_id: Option<&str>) -> NewCharacterMemoryFragment {
    NewCharacterMemoryFragment {
        fragment_id: fragment_id.to_string(),
        session_id: format!("sess_{fragment_id}"),
        character_id: character_id.to_string(),
        actor_world_binding_id: binding_id.map(str::to_string),
        keywords: "[\"alpha\", \"beta\"]".to_string(),
        summary: format!("summary for {fragment_id}"),
        created_at: "2026-09-05T10:00:00Z".to_string(),
        ttl: None,
    }
}

fn soul_meta(character_id: &str) -> CharacterSoulMeta {
    CharacterSoulMeta {
        character_id: character_id.to_string(),
        file_path: "/tmp/SOUL.md".to_string(),
        schema_version: 1,
        personality_hash: Some("ph".to_string()),
        experience_hash: None,
        created_at: "2026-09-05T10:00:00Z".to_string(),
        updated_at: "2026-09-05T10:00:00Z".to_string(),
    }
}

fn narrative(character_id: &str, binding_id: Option<&str>, text: &str) -> CharacterSoulNarrativeRecord {
    CharacterSoulNarrativeRecord {
        character_id: character_id.to_string(),
        actor_world_binding_id: binding_id.map(str::to_string),
        narrative: Some(text.to_string()),
        generated_at: Some("2026-09-05T11:00:00Z".to_string()),
        fragment_count_at_generation: 3,
        max_fragment_created_at_at_generation: Some("2026-09-05T10:00:00Z".to_string()),
        distinct_keyword_count_cache: 2,
        stats_fingerprint: Some("fp".to_string()),
        created_at: "2026-09-05T11:00:00Z".to_string(),
        updated_at: "2026-09-05T11:00:00Z".to_string(),
    }
}

async fn table_count(pool: &SqlitePool, table: &str) -> i64 {
    // SAFETY: dynamic SQL — test-only row counting over a fixed set of table
    // names controlled by this file; no external input reaches `table`.
    sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn creator_memory_row_counts(pool: &SqlitePool) -> (i64, i64, i64, i64) {
    (
        table_count(pool, "soul_meta").await,
        table_count(pool, "memory_pending_review").await,
        table_count(pool, "memory_fragments").await,
        table_count(pool, "memory_soul_narratives").await,
    )
}

// ── Pending review family ────────────────────────────────────────────────

#[tokio::test]
async fn pending_review_roundtrip_and_character_isolation() {
    let (pool, _dir) = fresh_pool().await;
    let s = seed(&pool).await;

    create_character_pending_review(&pool, OWNER, &pending("pend_1", &s.char_a, None))
        .await
        .unwrap();
    create_character_pending_review(&pool, OWNER, &pending("pend_2", &s.char_a, Some(&s.binding_a1)))
        .await
        .unwrap();

    let fetched = get_character_pending_review(&pool, OWNER, &s.char_a, "pend_2")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fetched.character_id, s.char_a);
    assert_eq!(fetched.actor_world_binding_id.as_deref(), Some(s.binding_a1.as_str()));

    let list = list_character_pending_reviews(&pool, OWNER, &s.char_a, 10)
        .await
        .unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(count_character_pending_reviews(&pool, OWNER, &s.char_a).await.unwrap(), 2);

    // Character B is fully isolated.
    assert!(list_character_pending_reviews(&pool, OWNER, &s.char_b, 10)
        .await
        .unwrap()
        .is_empty());
    assert_eq!(count_character_pending_reviews(&pool, OWNER, &s.char_b).await.unwrap(), 0);

    // Bounded list honors LIMIT.
    let bounded = list_character_pending_reviews(&pool, OWNER, &s.char_a, 1)
        .await
        .unwrap();
    assert_eq!(bounded.len(), 1);

    // Delete is scoped to the owning Character.
    assert!(!delete_character_pending_review(&pool, OWNER, &s.char_b, "pend_1")
        .await
        .unwrap());
    assert!(delete_character_pending_review(&pool, OWNER, &s.char_a, "pend_1")
        .await
        .unwrap());
    assert!(get_character_pending_review(&pool, OWNER, &s.char_a, "pend_1")
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn pending_review_rejects_foreign_character_and_cross_character_binding() {
    let (pool, _dir) = fresh_pool().await;
    let s = seed(&pool).await;

    // Foreign owner cannot write to OWNER's Character.
    let err = create_character_pending_review(&pool, OTHER, &pending("pend_x", &s.char_a, None))
        .await
        .unwrap_err();
    assert!(matches!(err, LocalDbError::ActorNotFound { .. }));

    // A binding that belongs to Character B must not authorize Character A rows.
    let err = create_character_pending_review(
        &pool,
        OWNER,
        &pending("pend_y", &s.char_a, Some(&s.binding_b1)),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, LocalDbError::ActorNotFound { .. }));

    // Both rejections happen before any row is written.
    assert_eq!(table_count(&pool, "character_memory_pending_review").await, 0);
}

#[tokio::test]
async fn pending_review_rejects_foreign_world_and_inactive_binding() {
    let (pool, _dir) = fresh_pool().await;
    let s = seed(&pool).await;

    // Raw-insert bindings that the public APIs would never create: one whose
    // World belongs to another Creator, one inactive. Repository write
    // validation must still refuse them.
    for (binding_id, world_id, status) in [
        ("awb_ffffffffffffffffffffffffffffffff", WORLD_FOREIGN, "active"),
        ("awb_eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee", WORLD_A, "inactive"),
    ] {
        sqlx::query(
            "INSERT INTO actor_world_bindings \
             (binding_id, character_id, world_id, status, created_at, updated_at) \
             VALUES (?, ?, ?, ?, datetime('now'), datetime('now'))",
        )
        .bind(binding_id)
        .bind(&s.char_a)
        .bind(world_id)
        .bind(status)
        .execute(&pool)
        .await
        .unwrap();
    }

    for binding_id in [
        "awb_ffffffffffffffffffffffffffffffff",
        "awb_eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
    ] {
        let err = create_character_pending_review(
            &pool,
            OWNER,
            &pending("pend_z", &s.char_a, Some(binding_id)),
        )
        .await
        .unwrap_err();
        assert!(
            matches!(err, LocalDbError::ActorNotFound { .. }),
            "binding {binding_id} should be refused, got {err:?}"
        );
    }
    assert_eq!(table_count(&pool, "character_memory_pending_review").await, 0);
}

#[tokio::test]
async fn pending_review_session_unique_per_character() {
    let (pool, _dir) = fresh_pool().await;
    let s = seed(&pool).await;

    create_character_pending_review(&pool, OWNER, &pending("pend_1", &s.char_a, None))
        .await
        .unwrap();
    // Same session for the same Character conflicts; for Character B it is fine.
    let dup = CharacterPendingReviewRecord {
        pending_id: "pend_dup".to_string(),
        session_id: "sess_pend_1".to_string(),
        ..pending("pend_dup", &s.char_a, None)
    };
    assert!(create_character_pending_review(&pool, OWNER, &dup).await.is_err());
    let shared_session = CharacterPendingReviewRecord {
        pending_id: "pend_b".to_string(),
        session_id: "sess_pend_1".to_string(),
        ..pending("pend_b", &s.char_b, None)
    };
    create_character_pending_review(&pool, OWNER, &shared_session)
        .await
        .unwrap();
}

// ── Restrictive foreign keys ─────────────────────────────────────────────

#[tokio::test]
async fn foreign_keys_reject_unknown_character_and_cross_character_binding() {
    let (pool, _dir) = fresh_pool().await;
    let s = seed(&pool).await;

    // Unknown character_id violates the characters FK on every family.
    let err = sqlx::query(
        "INSERT INTO character_memory_pending_review \
         (pending_id, session_id, character_id, task_kind, raw_digest, created_at) \
         VALUES ('pend_fk', 'sess_fk', 'chr_00000000000000000000000000000000', 'unknown', 'x', datetime('now'))",
    )
    .execute(&pool)
    .await
    .unwrap_err();
    assert!(err.to_string().contains("FOREIGN KEY"), "{err}");

    // Composite FK backstop: B's binding with A's character id is rejected
    // even if the caller bypasses repository validation.
    let err = sqlx::query(
        "INSERT INTO character_memory_fragments \
         (fragment_id, session_id, character_id, actor_world_binding_id, created_at) \
         VALUES ('frag_fk', 'sess_fk', ?, ?, datetime('now'))",
    )
    .bind(&s.char_a)
    .bind(&s.binding_b1)
    .execute(&pool)
    .await
    .unwrap_err();
    assert!(err.to_string().contains("FOREIGN KEY"), "{err}");

    let err = sqlx::query(
        "INSERT INTO character_soul_narratives \
         (character_id, actor_world_binding_id, created_at, updated_at) \
         VALUES (?, ?, datetime('now'), datetime('now'))",
    )
    .bind(&s.char_a)
    .bind(&s.binding_b1)
    .execute(&pool)
    .await
    .unwrap_err();
    assert!(err.to_string().contains("FOREIGN KEY"), "{err}");
}

// ── Fragment family ──────────────────────────────────────────────────────

#[tokio::test]
async fn fragment_scope_isolation_between_shared_and_bindings() {
    let (pool, _dir) = fresh_pool().await;
    let s = seed(&pool).await;

    create_character_fragment(&pool, OWNER, &fragment("frag_shared", &s.char_a, None))
        .await
        .unwrap();
    create_character_fragment(&pool, OWNER, &fragment("frag_b1", &s.char_a, Some(&s.binding_a1)))
        .await
        .unwrap();
    create_character_fragment(&pool, OWNER, &fragment("frag_b2", &s.char_a, Some(&s.binding_a2)))
        .await
        .unwrap();
    create_character_fragment(&pool, OWNER, &fragment("frag_b_char", &s.char_b, None))
        .await
        .unwrap();

    // Shared scope holds only provenance-free fragments.
    let shared = list_character_fragments(&pool, OWNER, &s.char_a, None, 10)
        .await
        .unwrap();
    assert_eq!(shared.len(), 1);
    assert_eq!(shared[0].fragment_id, "frag_shared");
    assert_eq!(shared[0].revision, 0);

    // Binding scopes are exact: each returns only its own provenance.
    let b1 = list_character_fragments(&pool, OWNER, &s.char_a, Some(&s.binding_a1), 10)
        .await
        .unwrap();
    assert_eq!(b1.len(), 1);
    assert_eq!(b1[0].fragment_id, "frag_b1");
    let b2 = list_character_fragments(&pool, OWNER, &s.char_a, Some(&s.binding_a2), 10)
        .await
        .unwrap();
    assert_eq!(b2.len(), 1);
    assert_eq!(b2[0].fragment_id, "frag_b2");

    // Character A/B isolation holds per scope.
    let b_shared = list_character_fragments(&pool, OWNER, &s.char_b, None, 10)
        .await
        .unwrap();
    assert_eq!(b_shared.len(), 1);
    assert_eq!(b_shared[0].fragment_id, "frag_b_char");

    // A fragment id owned by Character B is invisible through Character A.
    assert!(get_character_fragment(&pool, OWNER, &s.char_a, "frag_b_char")
        .await
        .unwrap()
        .is_none());

    // Scoped delete: cannot delete through another Character.
    assert!(!delete_character_fragment(&pool, OWNER, &s.char_b, "frag_shared")
        .await
        .unwrap());
    assert!(delete_character_fragment(&pool, OWNER, &s.char_a, "frag_b2")
        .await
        .unwrap());
}

#[tokio::test]
async fn fragment_write_rejects_foreign_character_and_binding() {
    let (pool, _dir) = fresh_pool().await;
    let s = seed(&pool).await;

    let err = create_character_fragment(&pool, OTHER, &fragment("frag_x", &s.char_a, None))
        .await
        .unwrap_err();
    assert!(matches!(err, LocalDbError::ActorNotFound { .. }));

    let err = create_character_fragment(
        &pool,
        OWNER,
        &fragment("frag_y", &s.char_a, Some(&s.binding_b1)),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, LocalDbError::ActorNotFound { .. }));

    assert_eq!(table_count(&pool, "character_memory_fragments").await, 0);
}

// ── SOUL meta family ─────────────────────────────────────────────────────

#[tokio::test]
async fn soul_meta_roundtrip_isolation_and_foreign_hiding() {
    let (pool, _dir) = fresh_pool().await;
    let s = seed(&pool).await;

    upsert_character_soul_meta(&pool, OWNER, &soul_meta(&s.char_a))
        .await
        .unwrap();
    upsert_character_soul_meta(&pool, OWNER, &soul_meta(&s.char_b))
        .await
        .unwrap();

    let fetched = get_character_soul_meta(&pool, OWNER, &s.char_a)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fetched.character_id, s.char_a);
    assert_eq!(fetched.personality_hash.as_deref(), Some("ph"));

    // Upsert updates in place; A/B rows stay independent.
    let mut updated = soul_meta(&s.char_a);
    updated.personality_hash = Some("ph2".to_string());
    upsert_character_soul_meta(&pool, OWNER, &updated)
        .await
        .unwrap();
    assert_eq!(table_count(&pool, "character_soul_meta").await, 2);
    assert_eq!(
        get_character_soul_meta(&pool, OWNER, &s.char_a)
            .await
            .unwrap()
            .unwrap()
            .personality_hash
            .as_deref(),
        Some("ph2")
    );

    // Foreign owner is hidden, not leaked.
    let err = get_character_soul_meta(&pool, OTHER, &s.char_a)
        .await
        .unwrap_err();
    assert!(matches!(err, LocalDbError::ActorNotFound { .. }));
    let err = upsert_character_soul_meta(&pool, OTHER, &soul_meta(&s.char_a))
        .await
        .unwrap_err();
    assert!(matches!(err, LocalDbError::ActorNotFound { .. }));

    assert!(delete_character_soul_meta(&pool, OWNER, &s.char_a)
        .await
        .unwrap());
    assert!(!delete_character_soul_meta(&pool, OWNER, &s.char_a)
        .await
        .unwrap());
    assert_eq!(table_count(&pool, "character_soul_meta").await, 1);
}

// ── SOUL narrative cache family ──────────────────────────────────────────

#[tokio::test]
async fn narrative_cache_null_and_per_binding_key_uniqueness() {
    let (pool, _dir) = fresh_pool().await;
    let s = seed(&pool).await;

    upsert_character_soul_narrative(&pool, OWNER, &narrative(&s.char_a, None, "shared"))
        .await
        .unwrap();
    upsert_character_soul_narrative(&pool, OWNER, &narrative(&s.char_a, Some(&s.binding_a1), "b1"))
        .await
        .unwrap();
    upsert_character_soul_narrative(&pool, OWNER, &narrative(&s.char_a, Some(&s.binding_a2), "b2"))
        .await
        .unwrap();

    // The three scopes coexist as three distinct rows.
    assert_eq!(table_count(&pool, "character_soul_narratives").await, 3);
    assert_eq!(
        get_character_soul_narrative(&pool, OWNER, &s.char_a, None)
            .await
            .unwrap()
            .unwrap()
            .narrative
            .as_deref(),
        Some("shared")
    );
    assert_eq!(
        get_character_soul_narrative(&pool, OWNER, &s.char_a, Some(&s.binding_a1))
            .await
            .unwrap()
            .unwrap()
            .narrative
            .as_deref(),
        Some("b1")
    );

    // Upserting the shared scope again replaces it (partial UNIQUE on NULL).
    upsert_character_soul_narrative(&pool, OWNER, &narrative(&s.char_a, None, "shared-v2"))
        .await
        .unwrap();
    assert_eq!(table_count(&pool, "character_soul_narratives").await, 3);
    assert_eq!(
        get_character_soul_narrative(&pool, OWNER, &s.char_a, None)
            .await
            .unwrap()
            .unwrap()
            .narrative
            .as_deref(),
        Some("shared-v2")
    );

    // A raw second shared-scope insert violates the partial UNIQUE index.
    let err = sqlx::query(
        "INSERT INTO character_soul_narratives \
         (character_id, actor_world_binding_id, created_at, updated_at) \
         VALUES (?, NULL, datetime('now'), datetime('now'))",
    )
    .bind(&s.char_a)
    .execute(&pool)
    .await
    .unwrap_err();
    assert!(err.to_string().contains("UNIQUE"), "{err}");

    // A raw duplicate per-binding key violates the composite PK.
    let err = sqlx::query(
        "INSERT INTO character_soul_narratives \
         (character_id, actor_world_binding_id, created_at, updated_at) \
         VALUES (?, ?, datetime('now'), datetime('now'))",
    )
    .bind(&s.char_a)
    .bind(&s.binding_a1)
    .execute(&pool)
    .await
    .unwrap_err();
    assert!(err.to_string().contains("UNIQUE"), "{err}");

    // Binding-local reads refuse a binding owned by another Character.
    let err = get_character_soul_narrative(&pool, OWNER, &s.char_a, Some(&s.binding_b1))
        .await
        .unwrap_err();
    assert!(matches!(err, LocalDbError::ActorNotFound { .. }));
}

#[tokio::test]
async fn narrative_fragment_stats_are_scope_bounded() {
    let (pool, _dir) = fresh_pool().await;
    let s = seed(&pool).await;

    let mut f1 = fragment("frag_s1", &s.char_a, None);
    f1.keywords = "[\"alpha\"]".to_string();
    let mut f2 = fragment("frag_s2", &s.char_a, None);
    f2.keywords = "[\"alpha\", \"gamma\"]".to_string();
    f2.created_at = "2026-09-05T12:00:00Z".to_string();
    let mut f3 = fragment("frag_l1", &s.char_a, Some(&s.binding_a1));
    f3.keywords = "[\"local-only\"]".to_string();
    for f in [f1, f2, f3] {
        create_character_fragment(&pool, OWNER, &f).await.unwrap();
    }

    let (shared, cached) =
        character_soul_narrative_fragment_stats(&pool, OWNER, &s.char_a, None)
            .await
            .unwrap();
    // First compute persists a stats-only cache row and re-reads it, so the
    // returned cache row is present alongside the freshly computed stats.
    assert!(cached.is_some());
    assert_eq!(shared.fragment_count, 2);
    // Shared scope: {alpha} ∪ {alpha, gamma} → 2 distinct keywords.
    assert_eq!(shared.distinct_keyword_count, 2);
    assert_eq!(
        shared.max_created_at.as_deref(),
        Some("2026-09-05T12:00:00Z")
    );

    // Second call serves the fingerprint-cached distinct count.
    let (shared_again, cache_row) =
        character_soul_narrative_fragment_stats(&pool, OWNER, &s.char_a, None)
            .await
            .unwrap();
    assert_eq!(shared_again.distinct_keyword_count, 2);
    assert!(cache_row.is_some());

    // Binding scope only sees its own provenance.
    let (local, _) =
        character_soul_narrative_fragment_stats(&pool, OWNER, &s.char_a, Some(&s.binding_a1))
            .await
            .unwrap();
    assert_eq!(local.fragment_count, 1);
    assert_eq!(local.distinct_keyword_count, 1);
}

// ── Revision-checked local→shared promotion ──────────────────────────────

#[tokio::test]
async fn promotion_is_revision_checked_atomic_and_cache_scoped() {
    let (pool, _dir) = fresh_pool().await;
    let s = seed(&pool).await;

    create_character_fragment(&pool, OWNER, &fragment("frag_p", &s.char_a, Some(&s.binding_a1)))
        .await
        .unwrap();

    // Seed cache rows in every nearby scope.
    upsert_character_soul_narrative(&pool, OWNER, &narrative(&s.char_a, None, "shared"))
        .await
        .unwrap();
    upsert_character_soul_narrative(&pool, OWNER, &narrative(&s.char_a, Some(&s.binding_a1), "b1"))
        .await
        .unwrap();
    upsert_character_soul_narrative(&pool, OWNER, &narrative(&s.char_a, Some(&s.binding_a2), "b2"))
        .await
        .unwrap();
    upsert_character_soul_narrative(&pool, OWNER, &narrative(&s.char_b, None, "b-char"))
        .await
        .unwrap();

    // Stale revision: zero mutation anywhere.
    let err = promote_character_fragment_to_shared(&pool, OWNER, &s.char_a, "frag_p", 7)
        .await
        .unwrap_err();
    assert!(matches!(err, LocalDbError::VersionMismatch { .. }));
    let still = get_character_fragment(&pool, OWNER, &s.char_a, "frag_p")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(still.actor_world_binding_id.as_deref(), Some(s.binding_a1.as_str()));
    assert_eq!(still.revision, 0);
    assert_eq!(table_count(&pool, "character_soul_narratives").await, 4);

    // Correct revision: same fragment id, provenance cleared, revision bumped.
    let promoted = promote_character_fragment_to_shared(&pool, OWNER, &s.char_a, "frag_p", 0)
        .await
        .unwrap();
    assert_eq!(promoted.fragment_id, "frag_p");
    assert_eq!(promoted.actor_world_binding_id, None);
    assert_eq!(promoted.revision, 1);

    // Only the two affected cache scopes were invalidated.
    assert!(get_character_soul_narrative(&pool, OWNER, &s.char_a, None)
        .await
        .unwrap()
        .is_none());
    // The invalidated binding-local row is gone at the storage level.
    let b1_rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM character_soul_narratives WHERE actor_world_binding_id = ?",
    )
    .bind(&s.binding_a1)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(b1_rows, 0);
    // Unaffected scopes survive.
    assert!(get_character_soul_narrative(&pool, OWNER, &s.char_a, Some(&s.binding_a2))
        .await
        .unwrap()
        .is_some());
    assert!(get_character_soul_narrative(&pool, OWNER, &s.char_b, None)
        .await
        .unwrap()
        .is_some());

    // Re-promoting a shared fragment is a stable conflict, not a silent no-op.
    let err = promote_character_fragment_to_shared(&pool, OWNER, &s.char_a, "frag_p", 1)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LocalDbError::ActorContractConflict {
            code: ActorContractConflict::CharacterFragmentAlreadyShared
        }
    ));

    // Foreign owner cannot promote; nothing changes.
    let err = promote_character_fragment_to_shared(&pool, OTHER, &s.char_a, "frag_p", 1)
        .await
        .unwrap_err();
    assert!(matches!(err, LocalDbError::ActorNotFound { .. }));
    assert_eq!(
        get_character_fragment(&pool, OWNER, &s.char_a, "frag_p")
            .await
            .unwrap()
            .unwrap()
            .revision,
        1
    );
}

// ── Binding removal dependency precedence ────────────────────────────────

async fn seed_binding_owned_ke(pool: &SqlitePool, binding_id: &str) {
    sqlx::query(
        "INSERT INTO kb_key_blocks \
         (key_block_id, owner_kind, actor_world_binding_id, block_type, canonical_name, status, body_json, created_at) \
         VALUES ('kb_owned_mem', 'actor_world_binding', ?, 'character', 'owned', 'confirmed', '{}', datetime('now'))",
    )
    .bind(binding_id)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn binding_removal_precedence_last_binding_then_ke_then_local_memory() {
    let (pool, _dir) = fresh_pool().await;
    let s = seed(&pool).await;

    // Precedence 1: last binding wins over local memory.
    create_character_pending_review(&pool, OWNER, &pending("pend_b", &s.char_b, Some(&s.binding_b1)))
        .await
        .unwrap();
    let err = remove_binding(&pool, OWNER, &s.char_b, &s.binding_b1)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LocalDbError::ActorContractConflict {
            code: ActorContractConflict::LastActiveBinding
        }
    ));

    // Precedence 2: binding-owned KE wins over local memory.
    create_character_pending_review(&pool, OWNER, &pending("pend_a", &s.char_a, Some(&s.binding_a2)))
        .await
        .unwrap();
    seed_binding_owned_ke(&pool, &s.binding_a2).await;
    let err = remove_binding(&pool, OWNER, &s.char_a, &s.binding_a2)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LocalDbError::ActorContractConflict {
            code: ActorContractConflict::BindingHasOwnedKnowledge
        }
    ));
    sqlx::query("DELETE FROM kb_key_blocks WHERE key_block_id = 'kb_owned_mem'")
        .execute(&pool)
        .await
        .unwrap();

    // Precedence 3: pending rows block with binding_has_local_memory, zero mutation.
    let err = remove_binding(&pool, OWNER, &s.char_a, &s.binding_a2)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LocalDbError::ActorContractConflict {
            code: ActorContractConflict::BindingHasLocalMemory
        }
    ));
    assert!(list_bindings_still_present(&pool, &s.binding_a2).await);
    assert_eq!(table_count(&pool, "character_memory_pending_review").await, 2);

    // Fragments and narrative cache rows block identically.
    assert!(delete_character_pending_review(&pool, OWNER, &s.char_a, "pend_a")
        .await
        .unwrap());
    create_character_fragment(&pool, OWNER, &fragment("frag_dep", &s.char_a, Some(&s.binding_a2)))
        .await
        .unwrap();
    let err = remove_binding(&pool, OWNER, &s.char_a, &s.binding_a2)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LocalDbError::ActorContractConflict {
            code: ActorContractConflict::BindingHasLocalMemory
        }
    ));
    assert!(delete_character_fragment(&pool, OWNER, &s.char_a, "frag_dep")
        .await
        .unwrap());

    upsert_character_soul_narrative(&pool, OWNER, &narrative(&s.char_a, Some(&s.binding_a2), "cache"))
        .await
        .unwrap();
    let err = remove_binding(&pool, OWNER, &s.char_a, &s.binding_a2)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LocalDbError::ActorContractConflict {
            code: ActorContractConflict::BindingHasLocalMemory
        }
    ));
    assert!(list_bindings_still_present(&pool, &s.binding_a2).await);
}

async fn list_bindings_still_present(pool: &SqlitePool, binding_id: &str) -> bool {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM actor_world_bindings WHERE binding_id = ?",
    )
    .bind(binding_id)
    .fetch_one(pool)
    .await
    .unwrap();
    count == 1
}

#[tokio::test]
async fn binding_removal_succeeds_once_local_memory_is_gone() {
    let (pool, _dir) = fresh_pool().await;
    let s = seed(&pool).await;

    create_character_fragment(&pool, OWNER, &fragment("frag_dep", &s.char_a, Some(&s.binding_a2)))
        .await
        .unwrap();
    upsert_character_soul_narrative(&pool, OWNER, &narrative(&s.char_a, Some(&s.binding_a2), "cache"))
        .await
        .unwrap();
    let err = remove_binding(&pool, OWNER, &s.char_a, &s.binding_a2)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LocalDbError::ActorContractConflict {
            code: ActorContractConflict::BindingHasLocalMemory
        }
    ));

    // Promotion clears the fragment provenance; cache rows deleted directly.
    promote_character_fragment_to_shared(&pool, OWNER, &s.char_a, "frag_dep", 0)
        .await
        .unwrap();
    remove_binding(&pool, OWNER, &s.char_a, &s.binding_a2)
        .await
        .unwrap();
    assert!(!list_bindings_still_present(&pool, &s.binding_a2).await);

    // The promoted fragment survives as shared Character memory.
    let kept = get_character_fragment(&pool, OWNER, &s.char_a, "frag_dep")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(kept.actor_world_binding_id, None);
}

// ── Creator non-mutation ─────────────────────────────────────────────────

#[tokio::test]
async fn character_memory_never_touches_creator_tables() {
    let (pool, _dir) = fresh_pool().await;
    let s = seed(&pool).await;

    // Exercise every Character family end to end.
    create_character_pending_review(&pool, OWNER, &pending("pend_c", &s.char_a, Some(&s.binding_a1)))
        .await
        .unwrap();
    create_character_fragment(&pool, OWNER, &fragment("frag_c", &s.char_a, Some(&s.binding_a1)))
        .await
        .unwrap();
    upsert_character_soul_meta(&pool, OWNER, &soul_meta(&s.char_a))
        .await
        .unwrap();
    upsert_character_soul_narrative(&pool, OWNER, &narrative(&s.char_a, Some(&s.binding_a1), "n"))
        .await
        .unwrap();
    character_soul_narrative_fragment_stats(&pool, OWNER, &s.char_a, Some(&s.binding_a1))
        .await
        .unwrap();
    promote_character_fragment_to_shared(&pool, OWNER, &s.char_a, "frag_c", 0)
        .await
        .unwrap();

    assert_eq!(creator_memory_row_counts(&pool).await, (0, 0, 0, 0));
}
