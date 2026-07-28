//! Production `KnowledgeEntryPort` impl — routes `kb_key_blocks` storage
//! through spoke's port surface with V1.73 CAS on the put path (spec §7.4 /
//! §7.3).
//!
//! # Wire conversion reuse (HARD, spec §7.1)
//!
//! The adapter REUSES the existing two `From` impls in
//! `nexus_knowledge::world_kb::knowledge_entry` (`impl From<WorldKbEntry> for
//! SpokeKnowledgeEntry` + reverse) as the **sole** conversion seam between
//! SQLite-backed [`WorldKbEntry`] rows and spoke [`KnowledgeEntry`] wire
//! types. No second conversion path is added here.
//!
//! # CAS contract (spec §7.4)
//!
//! `put_knowledge_entry` routes the update path through the existing V1.73
//! [`cas_update_key_block_fields`] CAS guard inside a caller-managed `SQLite`
//! transaction. The CAS outcomes map to spoke reject codes per the spec
//! table:
//!
//! | CAS outcome (actual vs expected_revision)        | Spoke reject code            |
//! |--------------------------------------------------|------------------------------|
//! | `actual > expected` (stored revision is newer)   | `STORED_REVISION_STALE`      |
//! | `actual < expected` (caller expects future rev)  | `REVISION_CONFLICT`          |
//! | Entry absent + `expected_revision = Some(_)`     | `REVISION_CONFLICT`          |
//! | Entry present + `expected_revision = None`       | `KNOWLEDGE_ENTRY_ALREADY_EXISTS` |

use super::NexusBaselineAdapter;
use crate::kb_store::{cas_update_key_block_fields, SqliteKbStore};
use crate::LocalDbError;
use nexus_knowledge::world_kb::store::{KbStore, KbStoreError};
use nexus_knowledge::world_kb::WorldKbEntry;
use nexus_spoke_adapter::extensions::build_extensions_nexus;
use nexus_spoke_adapter::{
    KnowledgeEntry, KnowledgeEntryPort, SpokeReject, SpokeRejectCode, SpokeResult,
};
use serde_json::{json, Map};

impl NexusBaselineAdapter {
    /// Convert a `SQLite` row error into a `KNOWLEDGE_ENTRY_NOT_FOUND` reject
    /// when the underlying store signals absence. Any other storage error
    /// surfaces as `INVALID_INPUT`.
    fn map_get_err(err: KbStoreError, entry_id: &str) -> SpokeResult<KnowledgeEntry> {
        match err {
            KbStoreError::NotFound(_) => reject(
                SpokeRejectCode::KnowledgeEntryNotFound,
                format!("KnowledgeEntry not found: {entry_id}"),
                json!({ "entry_id": entry_id }),
            ),
            other => reject(
                SpokeRejectCode::InvalidInput,
                format!("storage error on read: {other}"),
                json!({ "entry_id": entry_id }),
            ),
        }
    }

    /// Map a V1.73 `LocalDbError::VersionMismatch` (or any other `LocalDbError`
    /// surfaced from the CAS path) into the spoke reject code dictated by spec
    /// §7.4. `actual = None` (row absent) collapses to `REVISION_CONFLICT`
    /// (caller expects a revision the store has never reached).
    fn map_cas_err(
        err: LocalDbError,
        entry_id: &str,
        expected: u64,
    ) -> SpokeResult<KnowledgeEntry> {
        match err {
            LocalDbError::VersionMismatch {
                actual: Some(stored),
                ..
            } => {
                let stored_u = u64::try_from(stored).unwrap_or(0);
                if stored_u > expected {
                    reject(
                        SpokeRejectCode::StoredRevisionStale,
                        format!("Store revision {stored_u} is ahead of expected base {expected}"),
                        json!({
                            "entry_id": entry_id,
                            "expectedBaseRevision": expected,
                            "storeRevision": stored_u,
                        }),
                    )
                } else {
                    // stored_u < expected (== is impossible: CAS would have
                    // succeeded) — caller expects a revision the store has
                    // never reached.
                    reject(
                        SpokeRejectCode::RevisionConflict,
                        format!(
                            "Expected base revision {expected} is ahead of store revision {stored_u}"
                        ),
                        json!({
                            "entry_id": entry_id,
                            "expectedBaseRevision": expected,
                            "storeRevision": stored_u,
                        }),
                    )
                }
            }
            LocalDbError::VersionMismatch { actual: None, .. } => {
                // Entry absent + `Some(expected)` — spec §7.4 row 3. The store
                // has no revision at all; caller is ahead.
                reject(
                    SpokeRejectCode::RevisionConflict,
                    format!(
                        "KnowledgeEntry not found for update: {entry_id} (expected base {expected})"
                    ),
                    json!({
                        "entry_id": entry_id,
                        "expectedBaseRevision": expected,
                        "storeRevision": null,
                    }),
                )
            }
            other => reject(
                SpokeRejectCode::InvalidInput,
                format!("storage error on CAS update: {other}"),
                json!({ "entry_id": entry_id }),
            ),
        }
    }
}

impl KnowledgeEntryPort for NexusBaselineAdapter {
    fn get_knowledge_entry(&self, entry_id: &str) -> SpokeResult<KnowledgeEntry> {
        let pool = self.pool.clone();
        let entry_id = entry_id.to_string();
        self.block_on(async move {
            let store = SqliteKbStore::new(pool);
            let world_entry: WorldKbEntry = match store.get_knowledge_entry(&entry_id).await {
                Ok(row) => row,
                Err(e) => return Self::map_get_err(e, &entry_id),
            };
            // Reuse the existing `From<WorldKbEntry> for SpokeKnowledgeEntry`
            // impl — sole conversion seam (spec §7.1).
            SpokeResult::Ok(world_entry.into())
        })
    }

    fn put_knowledge_entry(
        &self,
        entry: KnowledgeEntry,
        expected_base_revision: Option<u64>,
    ) -> SpokeResult<KnowledgeEntry> {
        let pool = self.pool.clone();
        self.block_on(async move {
            match expected_base_revision {
                None => put_create(&pool, entry).await,
                Some(expected) => put_update(&pool, entry, expected).await,
            }
        })
    }
}

/// Create path: `expected_base_revision = None`. Reject if the row already
/// exists; otherwise insert via [`SqliteKbStore::insert_knowledge_entry`] and
/// return the entry with its initial post-create revision (`Some(1)`, matching
/// the V1.73 NULL-normalization rule).
async fn put_create(pool: &sqlx::SqlitePool, entry: KnowledgeEntry) -> SpokeResult<KnowledgeEntry> {
    let store = SqliteKbStore::new(pool.clone());
    let entry_id = entry.entry_id.clone();

    // Pre-check existence. (The underlying `kb_key_blocks_active_unique`
    // constraint is the true race guard; if a concurrent writer beats us the
    // Duplicate error from insert is also mapped to AlreadyExists below.)
    match store.get_knowledge_entry(&entry_id).await {
        Ok(_) => {
            return reject(
                SpokeRejectCode::KnowledgeEntryAlreadyExists,
                format!("Entry already exists: {entry_id}"),
                json!({ "entry_id": entry_id }),
            );
        }
        Err(KbStoreError::NotFound(_)) => {} // proceed to insert
        Err(e) => {
            return reject(
                SpokeRejectCode::InvalidInput,
                format!("storage error on create pre-check: {e}"),
                json!({ "entry_id": entry_id }),
            );
        }
    }

    // Reuse the existing `From<SpokeKnowledgeEntry> for WorldKbEntry` impl —
    // sole conversion seam (spec §7.1). Set the initial post-create revision
    // to 1 (matches the V1.73 NULL-normalization rule: the first successful
    // write sets revision = 1).
    let mut world_entry: WorldKbEntry = entry.clone().into();
    world_entry.revision = Some(1);

    match store.insert_knowledge_entry(world_entry).await {
        Ok(_) => {
            let mut result = entry;
            result.revision = Some(1);
            SpokeResult::Ok(result)
        }
        Err(KbStoreError::Duplicate {
            world_id,
            name,
            block_type,
        }) => reject(
            SpokeRejectCode::KnowledgeEntryAlreadyExists,
            format!("Entry already exists: {entry_id}"),
            json!({
                "entry_id": entry_id,
                "world_id": world_id,
                "canonical_name": name,
                "block_type": format!("{block_type:?}"),
            }),
        ),
        Err(e) => reject(
            SpokeRejectCode::InvalidInput,
            format!("storage error on create: {e}"),
            json!({ "entry_id": entry_id }),
        ),
    }
}

/// Update path: `expected_base_revision = Some(rev)`. Routes the CAS guard
/// through the existing V1.73 [`cas_update_key_block_fields`] function inside
/// a caller-managed transaction; on success, writes the remaining fields
/// (`status` / `source_anchor_json` / `extensions_nexus_json`) in the same tx
/// via a sibling UPDATE so the full row is replaced atomically with the CAS
/// guard.
async fn put_update(
    pool: &sqlx::SqlitePool,
    entry: KnowledgeEntry,
    expected: u64,
) -> SpokeResult<KnowledgeEntry> {
    let entry_id = entry.entry_id.clone();

    // Reuse the existing `From<SpokeKnowledgeEntry> for WorldKbEntry` impl —
    // sole conversion seam (spec §7.1).
    let world_entry: WorldKbEntry = entry.clone().into();

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            return reject(
                SpokeRejectCode::InvalidInput,
                format!("storage error on tx begin: {e}"),
                json!({ "entry_id": entry_id }),
            );
        }
    };

    // Serialize the fields `cas_update_key_block_fields` consumes.
    let body_json = world_entry
        .body
        .as_ref()
        .map(|b| serde_json::to_string(b).unwrap_or_default());
    // Stable snake_case serialization matching wire format (mirrors
    // SqliteKbStore::insert_knowledge_entry).
    let block_type_str = serde_json::to_string(&world_entry.block_type)
        .unwrap_or_else(|_| format!("{:?}", world_entry.block_type));
    let block_type_str = block_type_str.trim_matches('"').to_string();

    let expected_i64 = expected.cast_signed();

    let new_rev = match cas_update_key_block_fields(
        &mut tx,
        &entry_id,
        Some(&world_entry.canonical_name),
        Some(&block_type_str),
        body_json.as_deref(),
        expected_i64,
    )
    .await
    {
        Ok(new_rev) => new_rev,
        Err(e) => return NexusBaselineAdapter::map_cas_err(e, &entry_id, expected),
    };

    // CAS succeeded; revision was bumped to `new_rev` and name/type/body were
    // written. Now persist the remaining fields the CAS helper does not touch
    // (status / source_anchor_json / extensions_nexus_json) inside the same
    // transaction so the full row is replaced atomically with the guard. No
    // second CAS layer is added (spec §7.4).
    let source_anchor_json = world_entry
        .source_anchor
        .as_ref()
        .map(|a| serde_json::to_string(a).unwrap_or_default());
    // V1.139 P1 T4: re-serialize the full `extensions.nexus` namespace on the
    // update path so unknown keys survive the read-modify-write cycle (spec
    // §2.3 write path; mirrors SqliteKbStore::update_knowledge_entry).
    let extensions_nexus_json = serde_json::to_string(&build_extensions_nexus(
        &world_entry.world_id,
        world_entry.created_from_command_id.as_deref(),
        world_entry.source_work_id.as_deref(),
        world_entry.source_chapter,
        world_entry.source_provenance_kind.as_deref(),
        &nexus_extras_extension_map(world_entry.extensions_nexus_extras.as_ref()),
    ))
    .unwrap_or_default();

    // SAFETY: static SQL with vetted column names from migration
    // 202606190003_kb_key_blocks_provenance.sql. Runtime query used because
    // `extensions_nexus_json` column is unknown to sqlx offline mode (mirrors
    // the existing SqliteKbStore::update_knowledge_entry path).
    if let Err(e) = sqlx::query(
        r"UPDATE kb_key_blocks SET
             status = ?,
             source_anchor_json = ?,
             extensions_nexus_json = ?
           WHERE key_block_id = ?",
    )
    .bind(&world_entry.status)
    .bind(&source_anchor_json)
    .bind(&extensions_nexus_json)
    .bind(&entry_id)
    .execute(&mut *tx)
    .await
    {
        return reject(
            SpokeRejectCode::InvalidInput,
            format!("storage error on post-CAS field update: {e}"),
            json!({ "entry_id": entry_id }),
        );
    }

    if let Err(e) = tx.commit().await {
        return reject(
            SpokeRejectCode::InvalidInput,
            format!("storage error on tx commit: {e}"),
            json!({ "entry_id": entry_id }),
        );
    }

    let mut result = entry;
    result.revision = Some(new_rev);
    SpokeResult::Ok(result)
}

/// Build the wire-neutral extension map carrying an entry's unknown
/// `extensions.nexus` keys (mirrors the private `nexus_extras_extension_map`
/// in `kb_store` — duplicated here because the original is private and lives
/// behind `SqliteKbStore`'s module). Empty/absent extras yield an empty map.
fn nexus_extras_extension_map(
    extras: Option<&serde_json::Value>,
) -> nexus_spoke_adapter::ExtensionMap {
    let mut map = nexus_spoke_adapter::ExtensionMap::new();
    if let Some(serde_json::Value::Object(obj)) = extras {
        if !obj.is_empty() {
            map.insert("nexus".to_string(), obj.clone());
        }
    }
    map
}

/// Construct a `SpokeResult::Reject` from `code`, `message`, and a `serde_json::Value`
/// details payload (typically a small JSON object). The value is normalized into the
/// `Map<String, Value>` shape that `SpokeReject::details` expects; non-object payloads
/// are wrapped under a `"detail"` key.
fn reject<T>(
    code: SpokeRejectCode,
    message: impl Into<String>,
    details: serde_json::Value,
) -> SpokeResult<T> {
    let details_map = match details {
        serde_json::Value::Object(map) => Some(map),
        other => {
            let mut map = Map::new();
            map.insert("detail".to_string(), other);
            Some(map)
        }
    };
    SpokeResult::Reject(SpokeReject {
        code,
        message: message.into(),
        details: details_map,
    })
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{open_pool, run_migrations};
    use nexus_contracts::BlockType;
    use nexus_knowledge::world_kb::{WorldKbBody, WorldKbEntry};
    use nexus_spoke_adapter::KnowledgeEntryPort;

    async fn fresh_pool() -> (sqlx::SqlitePool, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = open_pool(&db_path).await.unwrap();
        run_migrations(&pool).await.unwrap();
        (pool, dir)
    }

    async fn seed_world(pool: &sqlx::SqlitePool) {
        // SAFETY: test-only static INSERTs with bind params.
        sqlx::query(
            "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) \
             VALUES ('ctr_test', 'Test', 'active', datetime('now'), '{}')",
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO narrative_worlds \
             (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, time_policy, metadata_json) \
             VALUES ('wld_1', 'wrk_test', 'ctr_test', 'Test World', 'test-world', 'active', 'private', 'manual', '{}')",
        )
        .execute(pool)
        .await
        .unwrap();
    }

    /// Build a spoke `KnowledgeEntry` fixture with a populated `extensions.nexus`
    /// (so it round-trips into the `kb_key_blocks` row that requires `world_id`).
    fn spoke_entry(entry_id: &str, canonical_name: &str, revision: Option<u64>) -> KnowledgeEntry {
        // Round-trip through the From impls: build a WorldKbEntry (which carries
        // world_id natively), convert forward to spoke — this guarantees the
        // fixture satisfies the storage shape requirements (world_id present
        // under extensions.nexus; canonical_name format-valid).
        let mut world = WorldKbEntry::new("wld_1", BlockType::Character, canonical_name);
        world.entry_id = entry_id.to_string();
        world.revision = revision;
        world.body = Some(WorldKbBody {
            summary: Some(format!("{canonical_name} summary")),
            ..Default::default()
        });
        world.into()
    }

    /// Test helper: unwrap a `SpokeResult::Ok` or panic with the reject payload.
    fn unwrap_ok<T>(result: SpokeResult<T>, label: &str) -> T {
        match result {
            SpokeResult::Ok(v) => v,
            SpokeResult::Reject(r) => panic!("{label}: expected ok, got reject {r:?}"),
        }
    }

    // ── get_knowledge_entry ───────────────────────────────────────────

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn get_returns_not_found_for_missing_entry() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let adapter = NexusBaselineAdapter::new(pool);
        let result = adapter.get_knowledge_entry("kb_missing");
        match result {
            SpokeResult::Reject(r) => {
                assert_eq!(r.code, SpokeRejectCode::KnowledgeEntryNotFound);
                assert_eq!(
                    r.details.as_ref().and_then(|d| d.get("entry_id")),
                    Some(&serde_json::json!("kb_missing"))
                );
            }
            SpokeResult::Ok(_) => panic!("expected reject, got ok"),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn get_round_trips_inserted_entry() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let adapter = NexusBaselineAdapter::new(pool.clone());
        let entry = spoke_entry("kb_alpha", "Alpha", None);
        let put_result = adapter.put_knowledge_entry(entry, None);
        assert!(
            matches!(put_result, SpokeResult::Ok(_)),
            "create should succeed"
        );

        let got = adapter.get_knowledge_entry("kb_alpha");
        match got {
            SpokeResult::Ok(e) => {
                assert_eq!(e.entry_id, "kb_alpha");
                assert_eq!(e.canonical_name.to_string(), "Alpha");
                assert_eq!(e.revision, Some(1), "post-create revision must be 1");
            }
            SpokeResult::Reject(_) => panic!("expected ok"),
        }
    }

    // ── put_knowledge_entry create path ───────────────────────────────

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn put_create_happy_path_bumps_revision_to_one() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let adapter = NexusBaselineAdapter::new(pool);
        let entry = spoke_entry("kb_create_happy", "CreateHappy", None);

        match adapter.put_knowledge_entry(entry, None) {
            SpokeResult::Ok(e) => {
                assert_eq!(e.entry_id, "kb_create_happy");
                assert_eq!(e.revision, Some(1));
            }
            SpokeResult::Reject(r) => panic!("expected ok, got reject: {r:?}"),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn put_create_on_existing_rejects_already_exists() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let adapter = NexusBaselineAdapter::new(pool);
        let entry = spoke_entry("kb_dup", "Dup", None);

        let first = adapter.put_knowledge_entry(entry.clone(), None);
        assert!(matches!(first, SpokeResult::Ok(_)));

        match adapter.put_knowledge_entry(entry, None) {
            SpokeResult::Reject(r) => {
                assert_eq!(r.code, SpokeRejectCode::KnowledgeEntryAlreadyExists);
            }
            SpokeResult::Ok(_) => panic!("expected AlreadyExists reject"),
        }
    }

    // ── put_knowledge_entry update path (CAS) ─────────────────────────

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn put_update_happy_path_bumps_revision() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let adapter = NexusBaselineAdapter::new(pool);
        let entry = spoke_entry("kb_upd_happy", "UpdHappy", None);

        // Create first (revision becomes 1).
        let created = match adapter.put_knowledge_entry(entry, None) {
            SpokeResult::Ok(e) => e,
            SpokeResult::Reject(r) => panic!("create failed: {r:?}"),
        };
        assert_eq!(created.revision, Some(1));

        // Update with expected_base_revision = Some(1). CAS accepts; revision
        // bumps to 2. Body / status / extensions all round-trip.
        let mut updated = created;
        updated.body.summary = Some("Updated summary".to_string());
        updated.status = "confirmed".to_string();

        match adapter.put_knowledge_entry(updated, Some(1)) {
            SpokeResult::Ok(e) => {
                assert_eq!(e.revision, Some(2), "CAS update must bump revision");
                assert_eq!(e.body.summary.as_deref(), Some("Updated summary"));
                assert_eq!(e.status, "confirmed");
            }
            SpokeResult::Reject(r) => panic!("expected ok, got reject: {r:?}"),
        }

        // Verify the row persisted the post-CAS field update (status + body
        // must both be reflected on re-read through the conversion seam).
        match adapter.get_knowledge_entry("kb_upd_happy") {
            SpokeResult::Ok(e) => {
                assert_eq!(e.revision, Some(2));
                assert_eq!(e.status, "confirmed");
                assert_eq!(e.body.summary.as_deref(), Some("Updated summary"));
            }
            SpokeResult::Reject(r) => panic!("re-read failed: {r:?}"),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn put_update_stale_rejects_stored_revision_stale() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let adapter = NexusBaselineAdapter::new(pool);
        let entry = spoke_entry("kb_stale", "Stale", None);

        // Create → revision 1. Bump to 2. Then attempt another update with
        // expected = 1 (caller read a stale base before the second writer
        // bumped). Store (2) > expected (1) → STORED_REVISION_STALE.
        let created = unwrap_ok(adapter.put_knowledge_entry(entry, None), "create");
        let _ = unwrap_ok(
            adapter.put_knowledge_entry(created.clone(), Some(1)),
            "first update",
        );

        match adapter.put_knowledge_entry(created, Some(1)) {
            SpokeResult::Reject(r) => {
                assert_eq!(
                    r.code,
                    SpokeRejectCode::StoredRevisionStale,
                    "stored > expected must map to STORED_REVISION_STALE (spec §7.4)"
                );
                let details = r.details.expect("details present");
                assert_eq!(details["expectedBaseRevision"], serde_json::json!(1));
                assert_eq!(details["storeRevision"], serde_json::json!(2));
            }
            SpokeResult::Ok(_) => panic!("expected STORED_REVISION_STALE reject"),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn put_update_conflict_rejects_revision_conflict() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let adapter = NexusBaselineAdapter::new(pool);
        let entry = spoke_entry("kb_conflict", "Conflict", None);

        // Create → revision 1. Then attempt update with expected = 5 (caller
        // expects a revision the store has never reached). Store (1) <
        // expected (5) → REVISION_CONFLICT.
        let created = unwrap_ok(adapter.put_knowledge_entry(entry, None), "create");

        match adapter.put_knowledge_entry(created, Some(5)) {
            SpokeResult::Reject(r) => {
                assert_eq!(
                    r.code,
                    SpokeRejectCode::RevisionConflict,
                    "stored < expected must map to REVISION_CONFLICT (spec §7.4)"
                );
                let details = r.details.expect("details present");
                assert_eq!(details["expectedBaseRevision"], serde_json::json!(5));
                assert_eq!(details["storeRevision"], serde_json::json!(1));
            }
            SpokeResult::Ok(_) => panic!("expected REVISION_CONFLICT reject"),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn put_update_on_absent_rejects_revision_conflict() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let adapter = NexusBaselineAdapter::new(pool);
        let entry = spoke_entry("kb_absent", "Absent", None);

        // No prior create — entry is absent. Caller passes expected = Some(3),
        // expecting a base the store has never reached. Per spec §7.4 row 3,
        // absent + Some(_) → REVISION_CONFLICT.
        match adapter.put_knowledge_entry(entry, Some(3)) {
            SpokeResult::Reject(r) => {
                assert_eq!(
                    r.code,
                    SpokeRejectCode::RevisionConflict,
                    "absent + Some(expected) must map to REVISION_CONFLICT (spec §7.4 row 3)"
                );
                let details = r.details.expect("details present");
                assert_eq!(details["expectedBaseRevision"], serde_json::json!(3));
                assert!(
                    details.get("storeRevision").is_some(),
                    "storeRevision key present"
                );
                assert_eq!(details["storeRevision"], serde_json::Value::Null);
            }
            SpokeResult::Ok(_) => panic!("expected REVISION_CONFLICT reject"),
        }
    }
}
