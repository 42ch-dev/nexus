//! Database module — connection pooling, canonical schema, and gateway adapters
//!
//! The concrete gateway types (`SqliteKbStore`, `SqliteNarrativeGateway`)
//! are defined in `nexus-local-db`. This module re-exports them and
//! provides the daemon-specific pool and schema utilities.

pub mod pool;
pub mod schema;

// Re-export store types from nexus-local-db (canonical owner of SQLite concerns).
pub use nexus_local_db::kb_store::SqliteKbStore;
pub use nexus_local_db::narrative_gateway::SqliteNarrativeGateway;

#[cfg(test)]
mod restart_tests {
    //! Integration tests that verify data survives a simulated daemon restart.
    //!
    //! A "restart" is modelled by closing the connection pool, then reopening
    //! it against the same file and re-applying migrations (idempotent).

    use nexus_kb::KbStore;
    use nexus_local_db::{open_pool, run_migrations};
    use nexus_narrative::NarrativeGateway;

    /// Open a temp-backed pool with all migrations applied.
    async fn fresh_pool() -> (sqlx::SqlitePool, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = open_pool(&db_path).await.unwrap();
        run_migrations(&pool).await.unwrap();
        (pool, dir)
    }

    #[tokio::test]
    async fn data_survives_simulated_restart() {
        // ── Phase 1: seed data ─────────────────────────────────────
        let (pool, dir) = fresh_pool().await;
        let db_path = dir.path().join("test.db");

        // Seed a world via the narrative gateway seed helper.
        nexus_local_db::narrative_gateway::seed::world(
            &pool,
            "wld_restart",
            "ctr_test",
            "Restart World",
            "restart-world",
            "private",
            "manual",
        )
        .await;

        // Seed a key block for that world via the KB store seed helper.
        nexus_local_db::kb_store::seed::key_block(
            &pool,
            "kb_char_1",
            "wld_restart",
            "Character",
            "Hero",
            "confirmed",
        )
        .await;

        // Close the first pool (simulates daemon shutdown).
        pool.close().await;

        // ── Phase 2: simulate restart ──────────────────────────────
        let pool2 = open_pool(&db_path).await.unwrap();
        run_migrations(&pool2).await.unwrap();

        let gw = super::SqliteNarrativeGateway::new(pool2.clone());
        let kb = super::SqliteKbStore::new(pool2.clone());

        // ── Phase 3: verify persisted data ─────────────────────────

        // list_worlds returns the seeded world
        let worlds = gw.list_worlds().await.unwrap();
        assert_eq!(worlds.len(), 1, "exactly one world should survive restart");
        assert_eq!(worlds[0].world_id, "wld_restart");
        assert_eq!(worlds[0].title, "Restart World");

        // list_by_world returns the seeded key block
        let blocks = kb.list_by_world("wld_restart").await.unwrap();
        assert_eq!(
            blocks.len(),
            1,
            "exactly one key block should survive restart"
        );
        assert_eq!(blocks[0].canonical_name, "Hero");
        assert_eq!(blocks[0].world_id, "wld_restart");
    }
}
