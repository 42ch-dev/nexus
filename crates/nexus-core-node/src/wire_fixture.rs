//! Shared disposable home seeding for wire proofs and lifecycle tests.

#![allow(dead_code)]

use std::path::Path;

use nexus_local_db::writer_protocol::{init_engine_pool, GuardedPoolOptions};
use sqlx::SqlitePool;

/// Creator id must satisfy the wire `CreatorId` pattern
/// (`^ctr_[a-zA-Z0-9]+$`) so family projections that carry
/// `owner_creator_id` stay wire-valid on the seeded home.
const CREATOR: &str = "ctr_testcreator";
const SLUG: &str = "default";
const OWNED_WORLD: &str = "wld_owned";
const FOREIGN_WORLD: &str = "wld_foreign";

async fn seed_world(pool: &SqlitePool, world_id: &str, owner: &str) {
    // v1.191 P1 T4/T9: a workspace identity is usable only once its subject and
    // its holder registry row have committed together, and the admitted
    // ActorView/KB read scope fails closed (`holder_state_invalid`) on a raw
    // subject row. Materialize through the one Creator materialization.
    nexus_local_db::ensure_creator_row(pool, owner, "Test")
        .await
        .expect("seed creator");
    sqlx::query(
        "INSERT INTO narrative_worlds (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, time_policy, metadata_json) VALUES (?, 'wrk', ?, 't', 's', 'active', 'private', 'manual', '{}')",
    )
    .bind(world_id)
    .bind(owner)
    .execute(pool)
    .await
    .expect("seed world");
}

async fn seed_kb(
    pool: &SqlitePool,
    id: &str,
    world_id: &str,
    name: &str,
    status: &str,
    revision: Option<i64>,
) {
    sqlx::query(
        "INSERT INTO kb_key_blocks (key_block_id, world_id, block_type, canonical_name, status, revision, modules_json, created_at, updated_at) VALUES (?, ?, 'character', ?, ?, ?, NULL, datetime('now'), datetime('now'))",
    )
    .bind(id)
    .bind(world_id)
    .bind(name)
    .bind(status)
    .bind(revision)
    .execute(pool)
    .await
    .expect("seed kb");
}

async fn seed_pending(
    pool: &SqlitePool,
    job_id: &str,
    world_id: &str,
    name: &str,
    created_at: &str,
) {
    sqlx::query(
        "INSERT INTO kb_extract_jobs (job_id, creator_id, workspace_id, work_entry_id, world_id, status, promotion_status, proposed_payload, block_type_guess, canonical_name_guess, version, created_at) VALUES (?, ?, 'ws', ?, ?, 'done', 'pending', '{}', 'character', ?, 0, ?)",
    )
    .bind(job_id)
    .bind(CREATOR)
    .bind(format!("work_{job_id}"))
    .bind(world_id)
    .bind(name)
    .bind(created_at)
    .execute(pool)
    .await
    .expect("seed pending");
}

pub async fn seed_wire_home(user_home: &Path) {
    let nexus_home = user_home.join(".nexus42");
    std::fs::create_dir_all(&nexus_home).expect("mkdir nexus home");
    let op = nexus_home_layout::operational_workspace_dir(user_home, CREATOR, SLUG);
    std::fs::create_dir_all(&op).expect("mkdir workspace");
    std::fs::write(
        nexus_home.join("config.toml"),
        format!(
            "active_creator_id = \"{CREATOR}\"\n[active_workspace_slug_by_creator]\n\"{CREATOR}\" = \"{SLUG}\""
        ),
    )
    .expect("write config");
    let db_path = nexus_home_layout::workspace_state_db_path(user_home, CREATOR, SLUG);
    let guarded = init_engine_pool(&db_path, CREATOR, GuardedPoolOptions::default())
        .await
        .expect("init engine pool");
    let pool = guarded.clone_pool();
    seed_world(&pool, OWNED_WORLD, CREATOR).await;
    seed_world(&pool, FOREIGN_WORLD, "other_creator").await;
    seed_kb(&pool, "kb_mod", OWNED_WORLD, "Mod", "confirmed", Some(0)).await;
    seed_kb(&pool, "kb_cas", OWNED_WORLD, "Cas", "confirmed", Some(2)).await;
    seed_pending(
        &pool,
        "xj_job1",
        OWNED_WORLD,
        "Cand1",
        "2020-01-01T00:00:01Z",
    )
    .await;
    seed_pending(
        &pool,
        "xj_job2",
        OWNED_WORLD,
        "Cand2",
        "2020-01-01T00:00:02Z",
    )
    .await;
    pool.close().await;
}
