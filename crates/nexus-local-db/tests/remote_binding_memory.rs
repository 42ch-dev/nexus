//! v1.229 N3 P4 (D18, binding spec §5.1) — remote binding-memory dependency
//! metadata on a real workspace database.
//!
//! These cases prove the consumer-visible invariants against the actual
//! storage helpers and the authoritative `remove_binding` transaction, with no
//! mocked `MemoryPort`:
//!
//! * every wire admission is independent; one attempt's retirement cannot
//!   release another, and an unknown attempt survives close/reopen and later
//!   successful attempts;
//! * an acknowledgement received before the capture result is published is
//!   durable and a later publication cannot resurrect a consumed/released
//!   object;
//! * review transfers a candidate's binding dependency to the returned record
//!   references, shared promotion releases the record, and a normal complete
//!   disposal allows removal;
//! * a foreign/stale namespace can neither read nor erase another namespace's
//!   facts, and an unknown admission blocks indefinitely (the
//!   supervisor-selected conservative policy; owner confirmation pending);
//! * terminal bookkeeping is cleared with the binding and never blocks a valid
//!   deletion.
#![allow(clippy::unwrap_used)]

use nexus_local_db::{
    add_actor_world_binding, begin_immediate, create_character_with_initial_binding,
    insert_remote_binding_admission_tx, record_remote_binding_object_tx,
    remote_binding_dependency_counts_tx, remove_binding, retire_remote_binding_admission_tx,
    ActorContractConflict, CreateBindingParams, CreateCharacterParams, LocalDbError,
    RemoteBindingNamespace, RemoteBindingObjectKind, RemoteBindingObjectState,
};
use sqlx::SqlitePool;

const OWNER: &str = "ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const WORLD_A: &str = "wld_remoteA";
const WORLD_B: &str = "wld_remoteB";
const HOST: &str = "peer_host_alpha";
const SPACE: &str = "spc_remote";
const CLIENT: &str = "peer_pubkey_alpha";
const OTHER_CLIENT: &str = "peer_pubkey_beta";
const NOW: &str = "2026-11-04T00:00:00Z";

struct Seed {
    character_id: String,
    /// The binding the dependency facts are attached to; a sibling binding
    /// keeps removal away from the last-active refusal.
    binding_id: String,
}

async fn fresh_pool() -> (SqlitePool, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let pool = nexus_local_db::init_engine_pool(&dir.path().join("test.db"))
        .await
        .unwrap()
        .clone_pool();
    (pool, dir)
}

async fn seed(pool: &SqlitePool) -> Seed {
    nexus_local_db::ensure_creator_row(pool, OWNER, "Owner")
        .await
        .unwrap();
    for world in [WORLD_A, WORLD_B] {
        sqlx::query(
            "INSERT INTO narrative_worlds \
             (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
              time_policy, metadata_json, created_at) \
             VALUES (?, 'ws', ?, ?, ?, 'active', 'private', 'manual', '{}', datetime('now'))",
        )
        .bind(world)
        .bind(OWNER)
        .bind(world)
        .bind(world)
        .execute(pool)
        .await
        .unwrap();
    }
    let created = create_character_with_initial_binding(
        pool,
        CreateCharacterParams {
            owner_creator_id: OWNER,
            display_name: "Ada",
            image_uri: None,
            persona_json: "{}",
            world_id: WORLD_A,
            world_sheet_entry_id: None,
        },
    )
    .await
    .unwrap();
    let character_id = created.character.character_id;
    let binding_id = created.binding.binding_id;
    add_actor_world_binding(
        pool,
        CreateBindingParams {
            owner_creator_id: OWNER,
            character_id: &character_id,
            world_id: WORLD_B,
            world_sheet_entry_id: None,
        },
    )
    .await
    .unwrap();
    Seed {
        character_id,
        binding_id,
    }
}

fn namespace<'a>(seed: &'a Seed, client_identity: &'a str) -> RemoteBindingNamespace<'a> {
    namespace_at(seed, client_identity, 3)
}

fn namespace_at<'a>(
    seed: &'a Seed,
    client_identity: &'a str,
    generation: u64,
) -> RemoteBindingNamespace<'a> {
    RemoteBindingNamespace::new(
        OWNER,
        &seed.character_id,
        &seed.binding_id,
        HOST,
        generation,
        SPACE,
        client_identity,
    )
}

async fn admit(pool: &SqlitePool, admission_id: &str, ns: &RemoteBindingNamespace<'_>) {
    let mut tx = begin_immediate(pool).await.unwrap();
    insert_remote_binding_admission_tx(&mut tx, admission_id, ns, NOW)
        .await
        .unwrap();
    tx.commit().await.unwrap();
}

async fn retire(pool: &SqlitePool, admission_id: &str, ns: &RemoteBindingNamespace<'_>) -> bool {
    let mut tx = begin_immediate(pool).await.unwrap();
    let retired = retire_remote_binding_admission_tx(&mut tx, admission_id, ns)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    retired
}

async fn record(
    pool: &SqlitePool,
    ns: &RemoteBindingNamespace<'_>,
    kind: RemoteBindingObjectKind,
    object_id: &str,
    state: RemoteBindingObjectState,
) {
    let mut tx = begin_immediate(pool).await.unwrap();
    record_remote_binding_object_tx(&mut tx, ns, kind, object_id, state, NOW)
        .await
        .unwrap();
    tx.commit().await.unwrap();
}

async fn dependencies(pool: &SqlitePool, binding_id: &str) -> (i64, i64) {
    let mut tx = begin_immediate(pool).await.unwrap();
    let counts = remote_binding_dependency_counts_tx(&mut tx, binding_id)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    (counts.admissions, counts.live_objects)
}

async fn fact_states(pool: &SqlitePool, binding_id: &str) -> Vec<(String, String, String)> {
    sqlx::query_as(
        "SELECT object_kind, object_id, state FROM remote_binding_memory_objects \
         WHERE binding_id = ? ORDER BY object_kind, object_id",
    )
    .bind(binding_id)
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn remove(seed: &Seed, pool: &SqlitePool) -> Result<(), LocalDbError> {
    remove_binding(pool, OWNER, &seed.character_id, &seed.binding_id).await
}

fn assert_remote_conflict(error: LocalDbError) {
    match error {
        LocalDbError::ActorContractConflict {
            code: ActorContractConflict::BindingHasRemoteMemory,
        } => assert_eq!(
            ActorContractConflict::BindingHasRemoteMemory.as_str(),
            "binding_has_remote_memory"
        ),
        other => panic!("expected the remote-memory conflict, got {other:?}"),
    }
}

/// Two independent admissions: retiring one attempt's admission releases
/// nothing else, and the binding stays non-removable until the other settles.
#[tokio::test]
async fn independent_admissions_retire_only_themselves() {
    let (pool, _dir) = fresh_pool().await;
    let seed = seed(&pool).await;
    let ns = namespace(&seed, CLIENT);
    admit(&pool, "adm_one", &ns).await;
    admit(&pool, "adm_two", &ns).await;
    assert_eq!(dependencies(&pool, &seed.binding_id).await, (2, 0));

    assert!(retire(&pool, "adm_one", &ns).await);
    assert_eq!(
        dependencies(&pool, &seed.binding_id).await,
        (1, 0),
        "the other attempt's admission is untouched"
    );
    assert_remote_conflict(remove(&seed, &pool).await.unwrap_err());

    assert!(retire(&pool, "adm_two", &ns).await);
    assert_eq!(dependencies(&pool, &seed.binding_id).await, (0, 0));
    remove(&seed, &pool).await.unwrap();
}

/// An unknown admission survives close/reopen, and a later successful attempt
/// neither settles it nor makes the binding removable.
#[tokio::test]
async fn unknown_admission_survives_reopen_and_later_success() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.db");
    let (pool, seed) = {
        let pool = nexus_local_db::init_engine_pool(&path)
            .await
            .unwrap()
            .clone_pool();
        let seed = seed(&pool).await;
        (pool, seed)
    };
    let ns = namespace(&seed, CLIENT);
    admit(&pool, "adm_unknown", &ns).await;
    pool.close().await;

    let reopened = nexus_local_db::init_engine_pool(&path)
        .await
        .unwrap()
        .clone_pool();
    assert_eq!(
        dependencies(&reopened, &seed.binding_id).await,
        (1, 0),
        "the unknown attempt is durable across reopen"
    );
    admit(&reopened, "adm_later", &ns).await;
    assert!(retire(&reopened, "adm_later", &ns).await);
    assert_eq!(
        dependencies(&reopened, &seed.binding_id).await,
        (1, 0),
        "a later success never settles the older unknown attempt"
    );
    assert_remote_conflict(remove(&seed, &reopened).await.unwrap_err());

    assert!(retire(&reopened, "adm_unknown", &ns).await);
    remove(&seed, &reopened).await.unwrap();
}

/// A disposal acknowledgement that precedes the capture-result publication is
/// durable: the late `live` publication cannot resurrect the candidate.
#[tokio::test]
async fn acknowledgement_before_publication_cannot_resurrect() {
    let (pool, _dir) = fresh_pool().await;
    let seed = seed(&pool).await;
    let ns = namespace(&seed, CLIENT);
    record(
        &pool,
        &ns,
        RemoteBindingObjectKind::Candidate,
        "cand_acked",
        RemoteBindingObjectState::Released,
    )
    .await;
    record(
        &pool,
        &ns,
        RemoteBindingObjectKind::Candidate,
        "cand_acked",
        RemoteBindingObjectState::Live,
    )
    .await;
    assert_eq!(
        fact_states(&pool, &seed.binding_id).await,
        vec![(
            "candidate".to_string(),
            "cand_acked".to_string(),
            "released".to_string()
        )],
        "the late publication does not downgrade the terminal fact"
    );
    assert_eq!(dependencies(&pool, &seed.binding_id).await, (0, 0));
    remove(&seed, &pool).await.unwrap();
}

/// Review consumption and the returned record references are one dependency
/// transfer: the candidate stops blocking, the record does, and a shared
/// promotion release lets the binding go.
#[tokio::test]
async fn review_transfers_dependency_and_promotion_releases_it() {
    let (pool, _dir) = fresh_pool().await;
    let seed = seed(&pool).await;
    let ns = namespace(&seed, CLIENT);
    record(
        &pool,
        &ns,
        RemoteBindingObjectKind::Candidate,
        "cand_reviewed",
        RemoteBindingObjectState::Live,
    )
    .await;
    assert_eq!(dependencies(&pool, &seed.binding_id).await, (0, 1));
    assert_remote_conflict(remove(&seed, &pool).await.unwrap_err());

    // One local transaction: consume the candidate and publish the records the
    // provider receipt returned.
    let mut tx = begin_immediate(&pool).await.unwrap();
    record_remote_binding_object_tx(
        &mut tx,
        &ns,
        RemoteBindingObjectKind::Candidate,
        "cand_reviewed",
        RemoteBindingObjectState::Consumed,
        NOW,
    )
    .await
    .unwrap();
    record_remote_binding_object_tx(
        &mut tx,
        &ns,
        RemoteBindingObjectKind::Record,
        "rec_reviewed",
        RemoteBindingObjectState::Live,
        NOW,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(
        dependencies(&pool, &seed.binding_id).await,
        (0, 1),
        "the dependency moved from the candidate to the record"
    );
    assert_remote_conflict(remove(&seed, &pool).await.unwrap_err());

    record(
        &pool,
        &ns,
        RemoteBindingObjectKind::Record,
        "rec_reviewed",
        RemoteBindingObjectState::Released,
    )
    .await;
    assert_eq!(dependencies(&pool, &seed.binding_id).await, (0, 0));
    remove(&seed, &pool).await.unwrap();
    assert!(
        fact_states(&pool, &seed.binding_id).await.is_empty(),
        "terminal facts are cleared with the binding"
    );
}

/// A stale or foreign namespace can neither retire another namespace's
/// admission nor release another namespace's live fact.
#[tokio::test]
async fn namespace_mismatch_cannot_erase_another_namespaces_facts() {
    let (pool, _dir) = fresh_pool().await;
    let seed = seed(&pool).await;
    let own = namespace(&seed, CLIENT);
    let stale = namespace(&seed, OTHER_CLIENT);
    admit(&pool, "adm_own", &own).await;
    record(
        &pool,
        &own,
        RemoteBindingObjectKind::Candidate,
        "cand_own",
        RemoteBindingObjectState::Live,
    )
    .await;
    assert!(
        !retire(&pool, "adm_own", &stale).await,
        "a foreign namespace cannot retire another admission"
    );
    record(
        &pool,
        &stale,
        RemoteBindingObjectKind::Candidate,
        "cand_own",
        RemoteBindingObjectState::Released,
    )
    .await;
    assert_eq!(
        dependencies(&pool, &seed.binding_id).await,
        (1, 1),
        "the own-namespace admission and live fact are untouched"
    );
    assert_remote_conflict(remove(&seed, &pool).await.unwrap_err());

    assert!(retire(&pool, "adm_own", &own).await);
    record(
        &pool,
        &own,
        RemoteBindingObjectKind::Candidate,
        "cand_own",
        RemoteBindingObjectState::Released,
    )
    .await;
    remove(&seed, &pool).await.unwrap();
}

/// A fact write for a binding that no longer exists is a no-op, so a late
/// acknowledgement can never revive a deleted binding.
#[tokio::test]
async fn late_acknowledgement_cannot_revive_a_deleted_binding() {
    let (pool, _dir) = fresh_pool().await;
    let seed = seed(&pool).await;
    let ns = namespace(&seed, CLIENT);
    record(
        &pool,
        &ns,
        RemoteBindingObjectKind::Candidate,
        "cand_terminal",
        RemoteBindingObjectState::Released,
    )
    .await;
    remove(&seed, &pool).await.unwrap();
    record(
        &pool,
        &ns,
        RemoteBindingObjectKind::Candidate,
        "cand_late",
        RemoteBindingObjectState::Live,
    )
    .await;
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM remote_binding_memory_objects WHERE binding_id = ?",
    )
    .bind(&seed.binding_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 0, "the deleted binding gains no new dependency");
}

/// The full `NonZeroU64` generation range is exact identity: two generations at
/// (and beyond) the signed-integer boundary share no row, so releasing one can
/// never release the other's live dependency.
#[tokio::test]
async fn cross_generation_release_cannot_release_another_generation() {
    const SIGNED_MAX: u64 = i64::MAX as u64;
    let (pool, _dir) = fresh_pool().await;
    let seed = seed(&pool).await;
    let at_signed_max = namespace_at(&seed, CLIENT, SIGNED_MAX);
    let just_past = namespace_at(&seed, CLIENT, SIGNED_MAX + 1);
    let at_unsigned_max = namespace_at(&seed, CLIENT, u64::MAX);
    for ns in [&at_signed_max, &just_past, &at_unsigned_max] {
        record(
            &pool,
            ns,
            RemoteBindingObjectKind::Candidate,
            "cand_cross_generation",
            RemoteBindingObjectState::Live,
        )
        .await;
    }
    assert_eq!(
        dependencies(&pool, &seed.binding_id).await,
        (0, 3),
        "three distinct generations are three distinct live facts"
    );

    record(
        &pool,
        &just_past,
        RemoteBindingObjectKind::Candidate,
        "cand_cross_generation",
        RemoteBindingObjectState::Released,
    )
    .await;
    assert_eq!(
        dependencies(&pool, &seed.binding_id).await,
        (0, 2),
        "releasing one generation leaves the others live"
    );
    assert_remote_conflict(remove(&seed, &pool).await.unwrap_err());

    record(
        &pool,
        &at_unsigned_max,
        RemoteBindingObjectKind::Candidate,
        "cand_cross_generation",
        RemoteBindingObjectState::Released,
    )
    .await;
    assert_eq!(dependencies(&pool, &seed.binding_id).await, (0, 1));

    record(
        &pool,
        &at_signed_max,
        RemoteBindingObjectKind::Candidate,
        "cand_cross_generation",
        RemoteBindingObjectState::Released,
    )
    .await;
    assert_eq!(dependencies(&pool, &seed.binding_id).await, (0, 0));
    remove(&seed, &pool).await.unwrap();
    assert!(
        fact_states(&pool, &seed.binding_id).await.is_empty(),
        "terminal facts are cleared with the binding"
    );
}

/// I-000333: the monotone object-fact surface is exactly the consumer-visible
/// upsert plus the binding cleanup, so the SQL boundary takes no extra trigger
/// (PM decision 2: the acceptance's "if needed" branch is not taken).
///
/// Mapping argument for the legitimate update surface:
/// * [`record_remote_binding_object_tx`] is the only state-mutating writer. Its
///   `ON CONFLICT … DO UPDATE` carries the rank guard (`live` 0 < `consumed`
///   1 < `released` 2), so a lower rank never overwrites a higher one — no
///   consumer, and no other admitted writer, can resurrect a terminal fact
///   through it.
/// * `release_binding_dependencies_tx` is the exact terminal cleanup: a
///   `DELETE … WHERE binding_id = ?` behind `require_binding_removable_tx`
///   (live objects must be zero first). It deletes rows, never rewrites a
///   state, so it cannot conflict with the monotone contract.
/// * The removal-gate counts and `remote_binding_object_is_known` are reads.
///
/// A `BEFORE UPDATE` trigger would duplicate a guard the single mapped writer
/// already enforces, so no migration or writer-admission change is added.
#[tokio::test]
async fn object_facts_stay_monotone_across_the_mapped_writes() {
    let (pool, _dir) = fresh_pool().await;
    let seed = seed(&pool).await;
    let ns = namespace(&seed, CLIENT);
    let state_of = |object_id: &'static str| {
        let pool = pool.clone();
        let binding_id = seed.binding_id.clone();
        async move {
            sqlx::query_scalar::<_, String>(
                "SELECT state FROM remote_binding_memory_objects \
                 WHERE binding_id = ? AND object_id = ?",
            )
            .bind(binding_id)
            .bind(object_id)
            .fetch_optional(&pool)
            .await
            .unwrap()
        }
    };

    // Forward transitions are accepted: live → consumed → released.
    record(
        &pool,
        &ns,
        RemoteBindingObjectKind::Record,
        "rec_monotone",
        RemoteBindingObjectState::Live,
    )
    .await;
    record(
        &pool,
        &ns,
        RemoteBindingObjectKind::Record,
        "rec_monotone",
        RemoteBindingObjectState::Consumed,
    )
    .await;
    assert_eq!(state_of("rec_monotone").await.as_deref(), Some("consumed"));
    record(
        &pool,
        &ns,
        RemoteBindingObjectKind::Record,
        "rec_monotone",
        RemoteBindingObjectState::Released,
    )
    .await;
    assert_eq!(state_of("rec_monotone").await.as_deref(), Some("released"));

    // Backward transitions never rewrite the released fact.
    for stale in [
        RemoteBindingObjectState::Live,
        RemoteBindingObjectState::Consumed,
    ] {
        record(
            &pool,
            &ns,
            RemoteBindingObjectKind::Record,
            "rec_monotone",
            stale,
        )
        .await;
        assert_eq!(
            state_of("rec_monotone").await.as_deref(),
            Some("released"),
            "a {stale:?} rewrite never resurrects a released fact"
        );
    }

    // `consumed` is terminal against `live` as well.
    record(
        &pool,
        &ns,
        RemoteBindingObjectKind::Candidate,
        "cand_monotone",
        RemoteBindingObjectState::Consumed,
    )
    .await;
    record(
        &pool,
        &ns,
        RemoteBindingObjectKind::Candidate,
        "cand_monotone",
        RemoteBindingObjectState::Live,
    )
    .await;
    assert_eq!(
        state_of("cand_monotone").await.as_deref(),
        Some("consumed"),
        "a live rewrite never resurrects a consumed candidate"
    );
    assert_eq!(
        dependencies(&pool, &seed.binding_id).await,
        (0, 0),
        "no terminal fact blocks removal"
    );

    // The cleanup deletes rows instead of rewriting states, and a fact for the
    // removed binding is a no-op (the mapped surface cannot revive it).
    remove(&seed, &pool).await.unwrap();
    assert_eq!(fact_states(&pool, &seed.binding_id).await, Vec::new());
    record(
        &pool,
        &ns,
        RemoteBindingObjectKind::Record,
        "rec_late",
        RemoteBindingObjectState::Live,
    )
    .await;
    assert!(state_of("rec_late").await.is_none());
}
