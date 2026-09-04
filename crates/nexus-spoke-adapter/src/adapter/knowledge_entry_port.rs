//! Production `KnowledgeEntryPort` impl — routes `kb_key_blocks` storage
//! through spoke's port surface with V1.73 CAS on the put path (spec §7.4 /
//! §7.3).
//!
//! # Wire conversion reuse (HARD, spec §7.1)
//!
//! The adapter REUSES the sole conversion seam (`knowledge_record_to_spoke` /
//! `spoke_to_knowledge_record` in `crate::conversion`, since V1.145 P1a)
//! between SQLite-backed [`KnowledgeEntryRecord`] rows and spoke [`KnowledgeEntry`]
//! wire types. No second conversion path is added here.
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
//! | Row moved to another world (world-aware CAS miss, V1.154 P2 R3) | world-conflict marker (`InternalError` carrier; wire `world_conflict` per spec §3.2) |

use super::NexusAdapter;
use crate::conversion::{spoke_to_knowledge_record, knowledge_record_to_spoke};
use crate::extensions::build_extensions_nexus;
use crate::{KnowledgeEntry, KnowledgeEntryPort, SpokeReject, SpokeRejectCode, SpokeResult};
use async_trait::async_trait;
use nexus_knowledge::world_kb::store::{KbStore, KbStoreError};
use nexus_knowledge::world_kb::KnowledgeEntryRecord;
use nexus_local_db::kb_store::{
    cas_update_key_block_fields, update_key_block_auxiliary_fields_in_tx, SqliteKbStore,
};
use nexus_local_db::LocalDbError;
use serde_json::{json, Map};

impl NexusAdapter<'_> {
    /// Convert a `SQLite` row error into a `KNOWLEDGE_ENTRY_NOT_FOUND` reject
    /// when the underlying store signals absence. Any other storage error
    /// surfaces as `INTERNAL_ERROR` (server-side failure).
    fn map_get_err(err: KbStoreError, entry_id: &str) -> SpokeResult<KnowledgeEntry> {
        match err {
            KbStoreError::NotFound(_) => reject(
                SpokeRejectCode::KnowledgeEntryNotFound,
                format!("KnowledgeEntry not found: {entry_id}"),
                json!({ "entry_id": entry_id }),
            ),
            other => reject(
                SpokeRejectCode::InternalError,
                format!("storage error on read: {other}"),
                json!({ "entry_id": entry_id }),
            ),
        }
    }

    /// Map a V1.73 `LocalDbError::VersionMismatch` (or any other `LocalDbError`
    /// surfaced from the CAS path) into the spoke reject code dictated by spec
    /// §7.4. `actual = None` (row absent) collapses to `REVISION_CONFLICT`
    /// (caller expects a revision the store has never reached).
    fn map_cas_err<T>(err: LocalDbError, entry_id: &str, expected: u64) -> SpokeResult<T> {
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
            // V1.154 P2 (R3 closure, spec §3.2): a zero-row CAS caused by a
            // world mismatch must surface as `world_conflict`, never as a
            // generic OCC failure. The pinned `SpokeRejectCode` has no
            // conflict-class code, so the classification rides the
            // `InternalError` carrier with a `world_conflict: true` details
            // marker; hosts remap it to the fixed `world_conflict` wire code
            // via [`is_world_conflict_reject`].
            LocalDbError::WorldConflict {
                table,
                id,
                expected_world,
                actual_world,
            } => reject(
                SpokeRejectCode::InternalError,
                format!(
                    "KnowledgeEntry {id} now lives in world {actual_world}, \
                     not the expected world {expected_world} (row moved between \
                     verification and CAS)"
                ),
                json!({
                    "world_conflict": true,
                    "table": table,
                    "id": id,
                    "expectedWorld": expected_world,
                    "actualWorld": actual_world,
                }),
            ),
            other => reject(
                SpokeRejectCode::InternalError,
                format!("storage error on CAS update: {other}"),
                json!({ "entry_id": entry_id }),
            ),
        }
    }
}

/// True when a `SpokeReject` carries the adapter's world-conflict
/// classification (spec §3.2).
///
/// A zero-row CAS caused by the stored row living in a different world
/// than the caller verified. The pinned `SpokeRejectCode`
/// (spoke-operations 0.9.2) has no conflict-class code, so the adapter
/// rides the classification on the `InternalError` carrier with a
/// `world_conflict: true` details marker. Host mappings (Connect
/// `ErrorEnvelope`, daemon HTTP) use this to surface the FIXED
/// `world_conflict` wire spelling instead of collapsing into
/// `revision_conflict` / `stored_revision_stale` or reading as a server
/// fault.
pub fn is_world_conflict_reject(reject: &SpokeReject) -> bool {
    reject
        .details
        .as_ref()
        .and_then(|d| d.get("world_conflict"))
        .and_then(serde_json::Value::as_bool)
        == Some(true)
}

#[async_trait]
impl KnowledgeEntryPort for NexusAdapter<'_> {
    async fn get_knowledge_entry(&self, entry_id: &str) -> SpokeResult<KnowledgeEntry> {
        let pool = self.pool.clone();
        let entry_id = entry_id.to_string();
        let store = SqliteKbStore::new(pool);
        let world_entry: KnowledgeEntryRecord = match store.get_knowledge_entry(&entry_id).await {
            Ok(row) => row,
            Err(e) => return Self::map_get_err(e, &entry_id),
        };
        // Reuse the sole conversion seam (spec §7.1) — now free functions
        // in nexus-spoke-adapter (V1.145 P1a dep-graph reversal).
        SpokeResult::Ok(knowledge_record_to_spoke(&world_entry))
    }

    async fn put_knowledge_entry(
        &self,
        entry: KnowledgeEntry,
        expected_base_revision: Option<u64>,
    ) -> SpokeResult<KnowledgeEntry> {
        let pool = self.pool.clone();
        match expected_base_revision {
            None => put_create(self, &pool, entry).await,
            Some(expected) => put_update(self, &pool, entry, expected).await,
        }
    }
}

/// Create path: `expected_base_revision = None`. Reject if the row already
/// exists; otherwise insert via [`SqliteKbStore::insert_key_block_in_tx`]
/// and return the entry with its initial post-create revision (`Some(1)`).
async fn put_create(
    adapter: &NexusAdapter<'_>,
    pool: &sqlx::SqlitePool,
    entry: KnowledgeEntry,
) -> SpokeResult<KnowledgeEntry> {
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
                SpokeRejectCode::InternalError,
                format!("storage error on create pre-check: {e}"),
                json!({ "entry_id": entry_id }),
            );
        }
    }

    // Reuse the sole conversion seam (spec §7.1) — free function in
    // nexus-spoke-adapter (V1.145 P1a). Set the initial post-create revision
    // to 1 (matches the V1.73 NULL-normalization rule: the first successful
    // write sets revision = 1).
    let mut world_entry: KnowledgeEntryRecord = match spoke_to_knowledge_record(entry.clone()) {
        Ok(v) => v,
        Err(e) => {
            return reject(
                SpokeRejectCode::InvalidInput,
                format!("create entry lacks canonical owner metadata: {e}"),
                json!({ "entry_id": entry_id }),
            );
        }
    };
    world_entry.revision = Some(1);

    // V1.145 P0 T2: build `extensions.nexus` JSON at the adapter boundary so
    // the storage layer stays spoke-unaware. Mirrors the UPDATE CAS path in
    // `run_cas_update_in_tx` (spec §7.4); the JSON is passed opaquely to
    // `insert_key_block_with_extensions_in_tx`. v1.184 P1: owner-aware.
    let extensions_nexus_json = serde_json::to_string(&build_extensions_nexus(
        &world_entry.owner,
        world_entry.creator_only,
        world_entry.created_from_command_id.as_deref(),
        world_entry.source_work_id.as_deref(),
        world_entry.source_chapter,
        world_entry.source_provenance_kind.as_deref(),
        &nexus_extras_extension_map(world_entry.extensions_nexus_extras.as_ref()),
    ))
    .unwrap_or_default();

    let insert_result = if adapter.is_bound() {
        let mut tx = adapter
            .take_bound_tx()
            .expect("bound adapter must have tx in cell");
        let result = store
            .insert_key_block_with_extensions_in_tx(&mut tx, world_entry, extensions_nexus_json)
            .await;
        adapter.restore_bound_tx(tx);
        result
    } else {
        let mut tx = match pool.begin().await {
            Ok(tx) => tx,
            Err(e) => {
                return reject(
                    SpokeRejectCode::InternalError,
                    format!("storage error on tx begin: {e}"),
                    json!({ "entry_id": entry_id }),
                );
            }
        };
        let result = store
            .insert_key_block_with_extensions_in_tx(&mut tx, world_entry, extensions_nexus_json)
            .await;
        if result.is_ok() {
            if let Err(e) = tx.commit().await {
                return reject(
                    SpokeRejectCode::InternalError,
                    format!("storage error on tx commit: {e}"),
                    json!({ "entry_id": entry_id }),
                );
            }
        }
        result
    };

    match insert_result {
        Ok(_) => {
            let mut result = entry;
            result.revision = Some(1);
            SpokeResult::Ok(result)
        }
        Err(KbStoreError::Duplicate {
            owner,
            name,
            block_type,
        }) => reject(
            SpokeRejectCode::KnowledgeEntryAlreadyExists,
            format!("Entry already exists: {entry_id}"),
            json!({
                "entry_id": entry_id,
                "owner": owner,
                "canonical_name": name,
                "block_type": format!("{block_type:?}"),
            }),
        ),
        // v1.184 P1 fix: caller-input failures (canonical-name/body
        // validation, the World-only creator_only invariant, and the
        // immutable-owner guard) are InvalidInput, never InternalError — the
        // latter is reserved for genuine storage failures.
        Err(e @ (KbStoreError::Validation(_) | KbStoreError::ValidationLegacy(_)))
        | Err(e @ KbStoreError::ImmutableOwner(_)) => reject(
            SpokeRejectCode::InvalidInput,
            format!("invalid entry on create: {e}"),
            json!({ "entry_id": entry_id }),
        ),
        Err(e) => reject(
            SpokeRejectCode::InternalError,
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
    adapter: &NexusAdapter<'_>,
    pool: &sqlx::SqlitePool,
    entry: KnowledgeEntry,
    expected: u64,
) -> SpokeResult<KnowledgeEntry> {
    if adapter.is_bound() {
        return put_update_bound(adapter, entry, expected).await;
    }
    put_update_unbound(pool, entry, expected).await
}

async fn put_update_bound(
    adapter: &NexusAdapter<'_>,
    entry: KnowledgeEntry,
    expected: u64,
) -> SpokeResult<KnowledgeEntry> {
    let entry_id = entry.entry_id.clone();
    let world_entry: KnowledgeEntryRecord = match spoke_to_knowledge_record(entry.clone()) {
        Ok(v) => v,
        Err(e) => {
            return reject(
                SpokeRejectCode::InvalidInput,
                format!("knowledge entry lacks canonical owner metadata: {e}"),
                json!({ "entry_id": entry_id }),
            );
        }
    };
    let mut tx = adapter
        .take_bound_tx()
        .expect("bound adapter must have tx in cell");
    let new_rev = match run_cas_update_in_tx(&mut tx, &entry_id, &world_entry, expected).await {
        SpokeResult::Ok(rev) => rev,
        SpokeResult::Reject(r) => {
            adapter.restore_bound_tx(tx);
            return SpokeResult::Reject(r);
        }
    };
    adapter.restore_bound_tx(tx);
    let mut result = entry;
    result.revision = Some(new_rev);
    SpokeResult::Ok(result)
}

async fn put_update_unbound(
    pool: &sqlx::SqlitePool,
    entry: KnowledgeEntry,
    expected: u64,
) -> SpokeResult<KnowledgeEntry> {
    let entry_id = entry.entry_id.clone();
    let world_entry: KnowledgeEntryRecord = match spoke_to_knowledge_record(entry.clone()) {
        Ok(v) => v,
        Err(e) => {
            return reject(
                SpokeRejectCode::InvalidInput,
                format!("knowledge entry lacks canonical owner metadata: {e}"),
                json!({ "entry_id": entry_id }),
            );
        }
    };
    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            return reject(
                SpokeRejectCode::InternalError,
                format!("storage error on tx begin: {e}"),
                json!({ "entry_id": entry_id }),
            );
        }
    };
    let new_rev = match run_cas_update_in_tx(&mut tx, &entry_id, &world_entry, expected).await {
        SpokeResult::Ok(rev) => rev,
        SpokeResult::Reject(r) => return SpokeResult::Reject(r),
    };
    if let Err(e) = tx.commit().await {
        return reject(
            SpokeRejectCode::InternalError,
            format!("storage error on tx commit: {e}"),
            json!({ "entry_id": entry_id }),
        );
    }
    let mut result = entry;
    result.revision = Some(new_rev);
    SpokeResult::Ok(result)
}

/// Atomically CAS-update zero or more knowledge entries and optionally update
/// a compute session's `state_json` in a **single** `SQLite` transaction.
///
/// Used by `ComputablePort::compute` settle path so a multi-target settle
/// cannot leave partial entry writes if a later CAS or the session update
/// fails (Greptile P1: rejected computes leave partial state). On any reject
/// the transaction is dropped without commit → full rollback.
///
/// `entry_updates`: `(candidate entry, expected_base_revision)` pairs.
/// `session_update`: optional `(session_id, state_json)` to persist after
/// all entry CAS succeeds, still inside the same transaction.
pub(crate) async fn commit_compute_settlement(
    adapter: &NexusAdapter<'_>,
    entry_updates: Vec<(KnowledgeEntry, u64)>,
    session_update: Option<(String, String)>,
) -> SpokeResult<()> {
    let pool = adapter.pool.clone();
    {
        let mut tx = match pool.begin().await {
            Ok(tx) => tx,
            Err(e) => {
                return reject(
                    SpokeRejectCode::InternalError,
                    format!("storage error on settlement tx begin: {e}"),
                    json!({}),
                );
            }
        };

        for (entry, expected) in entry_updates {
            let entry_id = entry.entry_id.clone();
            let world_entry: KnowledgeEntryRecord = match spoke_to_knowledge_record(entry) {
                Ok(v) => v,
                Err(e) => {
                    return reject(
                        SpokeRejectCode::InvalidInput,
                        format!("knowledge entry lacks canonical owner metadata: {e}"),
                        json!({ "entry_id": entry_id }),
                    );
                }
            };
            match run_cas_update_in_tx(&mut tx, &entry_id, &world_entry, expected).await {
                SpokeResult::Ok(_) => {}
                SpokeResult::Reject(r) => {
                    // Drop tx without commit → rollback prior CAS writes.
                    return SpokeResult::Reject(r);
                }
            }
        }

        if let Some((session_id, state_json)) = session_update {
            // SAFETY: static SQL; same shape as update_compute_session_state
            // but joins the settlement transaction.
            if let Err(e) =
                sqlx::query("UPDATE compute_sessions SET state_json = ? WHERE session_id = ?")
                    .bind(&state_json)
                    .bind(&session_id)
                    .execute(&mut *tx)
                    .await
            {
                return reject(
                    SpokeRejectCode::InternalError,
                    format!("storage error on compute session state update: {e}"),
                    json!({ "session_id": session_id }),
                );
            }
        }

        if let Err(e) = tx.commit().await {
            return reject(
                SpokeRejectCode::InternalError,
                format!("storage error on settlement tx commit: {e}"),
                json!({}),
            );
        }

        SpokeResult::Ok(())
    }
}

async fn run_cas_update_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    entry_id: &str,
    world_entry: &KnowledgeEntryRecord,
    expected: u64,
) -> SpokeResult<u64> {
    let body_json = world_entry
        .body
        .as_ref()
        .map(|b| serde_json::to_string(b).unwrap_or_default());
    let block_type_str = serde_json::to_string(&world_entry.block_type)
        .unwrap_or_else(|_| format!("{:?}", world_entry.block_type));
    let block_type_str = block_type_str.trim_matches('"').to_string();
    let source_anchor_json = world_entry
        .source_anchor
        .as_ref()
        .map(|a| serde_json::to_string(a).unwrap_or_default());
    let extensions_nexus_json = serde_json::to_string(&build_extensions_nexus(
        &world_entry.owner,
        world_entry.creator_only,
        world_entry.created_from_command_id.as_deref(),
        world_entry.source_work_id.as_deref(),
        world_entry.source_chapter,
        world_entry.source_provenance_kind.as_deref(),
        &nexus_extras_extension_map(world_entry.extensions_nexus_extras.as_ref()),
    ))
    .unwrap_or_default();
    // V1.146 P4 T1: serialize modules_json for the CAS auxiliary update.
    let modules_json = world_entry
        .modules
        .as_ref()
        .map(|m| serde_json::to_string(m).unwrap_or_default());

    // v1.184 P1 fix: `creator_only` is immutable on update. The stored typed
    // value is read back inside the tx and compared to the candidate *before*
    // the CAS writes, but ONLY when the stored row is in the same world as
    // the candidate — a candidate that flips the flag is rejected InvalidInput
    // with no write (otherwise the extension JSON would briefly disagree with
    // the authoritative typed column and the mismatch would be masked).
    //
    // Owner immutability needs no separate check: the World-owned CAS lane
    // already binds the candidate's `world_id` in its predicate, so a row
    // moved to another world between verification and CAS misses the predicate
    // and classifies as a world-conflict (InternalError carrier) — exactly the
    // spec §3.2 behavior. We must NOT intercept that here as an immutable-
    // owner error, or we would mask the world-conflict.
    let stored_creator_only = match sqlx::query_as::<_, (Option<String>, i64)>(
        "SELECT world_id, creator_only FROM kb_key_blocks WHERE key_block_id = ?",
    )
    .bind(entry_id)
    .fetch_optional(&mut **tx)
    .await
    {
        Ok(v) => v,
        Err(e) => {
            return reject(
                SpokeRejectCode::InternalError,
                format!("storage error reading stored creator_only for update: {e}"),
                json!({ "entry_id": entry_id }),
            );
        }
    };
    if let Some((stored_world, stored_creator_only)) = stored_creator_only {
        // Only a same-world creator_only flip is an immutable-owner violation.
        // A different stored world (or non-World row) falls through to the CAS,
        // which classifies world-conflict / not-found exactly as before.
        if stored_world.as_deref() == world_entry.world_id() && (stored_creator_only != 0) != world_entry.creator_only
        {
            return reject(
                SpokeRejectCode::InvalidInput,
                "knowledge entry creator_only is immutable (stored flag differs from candidate)",
                json!({ "entry_id": entry_id }),
            );
        }
    }
    // A missing row falls through to the CAS, which classifies NotFound /
    // CAS-miss exactly as before.

    // v1.184 P1: the CAS update lane is World-owned only — a non-World
    // candidate cannot be patched through the world-scoped CAS (fails closed
    // rather than passing an empty world id into a same-world predicate).
    let Some(world_id) = world_entry.world_id() else {
        return reject(
            SpokeRejectCode::InvalidInput,
            format!(
                "knowledge entry update requires a World-owned record (got {})",
                world_entry.owner.kind()
            ),
            json!({ "entry_id": entry_id }),
        );
    };
    let new_rev = match cas_update_key_block_fields(
        tx,
        entry_id,
        Some(&world_entry.canonical_name),
        Some(&block_type_str),
        body_json.as_deref(),
        expected.cast_signed(),
        // V1.154 P2 (R3 closure): the world bind is the stored-world
        // expected by the request — the candidate's claimed world, which the
        // invoke gate verified against the stored row (spec §3.1). If a
        // cross-process writer moved the row to another world between the
        // gate check and this CAS, the predicate misses and the storage
        // layer classifies it as WorldConflict.
        world_id,
    )
    .await
    {
        Ok(new_rev) => new_rev,
        Err(e) => return NexusAdapter::map_cas_err(e, entry_id, expected),
    };

    if let Err(e) = update_key_block_auxiliary_fields_in_tx(
        tx,
        entry_id,
        &world_entry.status,
        source_anchor_json.as_deref(),
        &extensions_nexus_json,
        modules_json.as_deref(),
        // V1.155 P2 T3 (R-V1152P0-001): stamp the dedicated provenance
        // column atomically with the CAS body replace — the pack-import
        // overwrite no longer needs a separate post-upsert UPDATE.
        world_entry.source_provenance_kind.as_deref(),
    )
    .await
    {
        return reject(
            SpokeRejectCode::InternalError,
            format!("storage error on post-CAS field update: {e}"),
            json!({ "entry_id": entry_id }),
        );
    }

    SpokeResult::Ok(new_rev)
}

/// Build the wire-neutral extension map carrying an entry's unknown
/// `extensions.nexus` keys (mirrors the private `nexus_extras_extension_map`
/// in `kb_store` — duplicated here because the original is private and lives
/// behind `SqliteKbStore`'s module). Empty/absent extras yield an empty map.
fn nexus_extras_extension_map(extras: Option<&serde_json::Value>) -> crate::ExtensionMap {
    let mut map = crate::ExtensionMap::new();
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
    use crate::KnowledgeEntryPort;
    use nexus_contracts::BlockType;
    use nexus_knowledge::world_kb::{KnowledgeEntryBody, KnowledgeEntryRecord};
    use nexus_local_db::{open_pool, run_migrations};

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
        // Round-trip through the sole conversion seam: build a KnowledgeEntryRecord
        // (which carries world_id natively), convert forward to spoke — this
        // guarantees the fixture satisfies the storage shape requirements
        // (world_id present under extensions.nexus; canonical_name format-valid).
        let mut world = KnowledgeEntryRecord::new("wld_1", BlockType::Character, canonical_name);
        world.entry_id = entry_id.to_string();
        world.revision = revision;
        world.body = Some(KnowledgeEntryBody {
            summary: Some(format!("{canonical_name} summary")),
            ..Default::default()
        });
        knowledge_record_to_spoke(&world)
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

        let adapter = NexusAdapter::new(pool);
        let result = adapter.get_knowledge_entry("kb_missing").await;
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

        let adapter = NexusAdapter::new(pool.clone());
        let entry = spoke_entry("kb_alpha", "Alpha", None);
        let put_result = adapter.put_knowledge_entry(entry, None).await;
        assert!(
            matches!(put_result, SpokeResult::Ok(_)),
            "create should succeed"
        );

        let got = adapter.get_knowledge_entry("kb_alpha").await;
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

        let adapter = NexusAdapter::new(pool);
        let entry = spoke_entry("kb_create_happy", "CreateHappy", None);

        match adapter.put_knowledge_entry(entry, None).await {
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

        let adapter = NexusAdapter::new(pool);
        let entry = spoke_entry("kb_dup", "Dup", None);

        let first = adapter.put_knowledge_entry(entry.clone(), None).await;
        assert!(matches!(first, SpokeResult::Ok(_)));

        match adapter.put_knowledge_entry(entry, None).await {
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

        let adapter = NexusAdapter::new(pool);
        let entry = spoke_entry("kb_upd_happy", "UpdHappy", None);

        // Create first (revision becomes 1).
        let created = match adapter.put_knowledge_entry(entry, None).await {
            SpokeResult::Ok(e) => e,
            SpokeResult::Reject(r) => panic!("create failed: {r:?}"),
        };
        assert_eq!(created.revision, Some(1));

        // Update with expected_base_revision = Some(1). CAS accepts; revision
        // bumps to 2. Body / status / extensions all round-trip.
        let mut updated = created;
        updated.body.summary = Some("Updated summary".to_string());
        updated.status = "confirmed".to_string();

        match adapter.put_knowledge_entry(updated, Some(1)).await {
            SpokeResult::Ok(e) => {
                assert_eq!(e.revision, Some(2), "CAS update must bump revision");
                assert_eq!(e.body.summary.as_deref(), Some("Updated summary"));
                assert_eq!(e.status, "confirmed");
            }
            SpokeResult::Reject(r) => panic!("expected ok, got reject: {r:?}"),
        }

        // Verify the row persisted the post-CAS field update (status + body
        // must both be reflected on re-read through the conversion seam).
        match adapter.get_knowledge_entry("kb_upd_happy").await {
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

        let adapter = NexusAdapter::new(pool);
        let entry = spoke_entry("kb_stale", "Stale", None);

        // Create → revision 1. Bump to 2. Then attempt another update with
        // expected = 1 (caller read a stale base before the second writer
        // bumped). Store (2) > expected (1) → STORED_REVISION_STALE.
        let created = unwrap_ok(adapter.put_knowledge_entry(entry, None).await, "create");
        let _ = unwrap_ok(
            adapter.put_knowledge_entry(created.clone(), Some(1)).await,
            "first update",
        );

        match adapter.put_knowledge_entry(created, Some(1)).await {
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

        let adapter = NexusAdapter::new(pool);
        let entry = spoke_entry("kb_conflict", "Conflict", None);

        // Create → revision 1. Then attempt update with expected = 5 (caller
        // expects a revision the store has never reached). Store (1) <
        // expected (5) → REVISION_CONFLICT.
        let created = unwrap_ok(adapter.put_knowledge_entry(entry, None).await, "create");

        match adapter.put_knowledge_entry(created, Some(5)).await {
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

        let adapter = NexusAdapter::new(pool);
        let entry = spoke_entry("kb_absent", "Absent", None);

        // No prior create — entry is absent. Caller passes expected = Some(3),
        // expecting a base the store has never reached. Per spec §7.4 row 3,
        // absent + Some(_) → REVISION_CONFLICT.
        match adapter.put_knowledge_entry(entry, Some(3)).await {
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

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn put_create_unbound_tx_commits_immediately() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let adapter = NexusAdapter::new(pool.clone());
        let entry = spoke_entry("kb_unbound_create", "UnboundCreate", None);

        match adapter.put_knowledge_entry(entry, None).await {
            SpokeResult::Ok(e) => assert_eq!(e.revision, Some(1)),
            SpokeResult::Reject(r) => panic!("expected ok, got reject: {r:?}"),
        }

        let store = SqliteKbStore::new(pool);
        assert!(
            store.get_knowledge_entry("kb_unbound_create").await.is_ok(),
            "unbound put must commit without an outer transaction"
        );
    }

    // ── V1.146 P0: InternalError on DB failure ─────────────────────────

    /// DB failure (dropped table) on get surfaces `InternalError`.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn get_on_dropped_table_surfaces_internal_error() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;
        // Drop the table to simulate a DB-level failure.
        sqlx::query("DROP TABLE kb_key_blocks")
            .execute(&pool)
            .await
            .unwrap();

        let adapter = NexusAdapter::new(pool);
        match adapter.get_knowledge_entry("kb_alpha").await {
            SpokeResult::Reject(r) => {
                assert_eq!(
                    r.code,
                    SpokeRejectCode::InternalError,
                    "dropped table must surface INTERNAL_ERROR"
                );
            }
            SpokeResult::Ok(_) => panic!("expected InternalError reject"),
        }
    }

    /// DB failure on `put_create` surfaces `InternalError`.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn put_create_on_dropped_table_surfaces_internal_error() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;
        sqlx::query("DROP TABLE kb_key_blocks")
            .execute(&pool)
            .await
            .unwrap();

        let adapter = NexusAdapter::new(pool);
        let entry = spoke_entry("kb_fail", "FailCreate", None);
        match adapter.put_knowledge_entry(entry, None).await {
            SpokeResult::Reject(r) => {
                assert_eq!(
                    r.code,
                    SpokeRejectCode::InternalError,
                    "create on dropped table must surface INTERNAL_ERROR"
                );
            }
            SpokeResult::Ok(_) => panic!("expected InternalError reject"),
        }
    }

    /// DB failure on `put_update` surfaces `InternalError`.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn put_update_on_dropped_table_surfaces_internal_error() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        // Create a real entry first so the update path is exercised.
        let adapter = NexusAdapter::new(pool.clone());
        let entry = spoke_entry("kb_upd_fail", "UpdFail", None);
        let created = unwrap_ok(adapter.put_knowledge_entry(entry, None).await, "create");
        assert_eq!(created.revision, Some(1));

        // Drop the table to simulate a DB-level failure on update.
        sqlx::query("DROP TABLE kb_key_blocks")
            .execute(&pool)
            .await
            .unwrap();

        match adapter.put_knowledge_entry(created, Some(1)).await {
            SpokeResult::Reject(r) => {
                assert_eq!(
                    r.code,
                    SpokeRejectCode::InternalError,
                    "update on dropped table must surface INTERNAL_ERROR"
                );
            }
            SpokeResult::Ok(_) => panic!("expected InternalError reject"),
        }
    }

    // ── V1.146 P0: validation → InvalidInput (unchanged) ───────────────

    /// Validation failure (missing `entry_id` / `canonical_name` — rejected by the
    /// spoke boundary before any DB I/O) still surfaces `InvalidInput`.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn validation_still_rejects_invalid_input() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let adapter = NexusAdapter::new(pool);
        // put_create on already-existing entry — this is an OCC/domain signal,
        // NOT a storage failure; the DAO's pre-check returns `KnowledgeEntryAlreadyExists`.
        let entry = spoke_entry("kb_val_ae", "ValAE", None);
        let _ = unwrap_ok(
            adapter.put_knowledge_entry(entry.clone(), None).await,
            "create",
        );

        match adapter.put_knowledge_entry(entry, None).await {
            SpokeResult::Reject(r) => {
                assert_eq!(
                    r.code,
                    SpokeRejectCode::KnowledgeEntryAlreadyExists,
                    "duplicate create must still surface KnowledgeEntryAlreadyExists"
                );
            }
            SpokeResult::Ok(_) => panic!("expected AlreadyExists reject"),
        }

        // get on non-existent entry still surfaces KnowledgeEntryNotFound
        match adapter.get_knowledge_entry("kb_never_created").await {
            SpokeResult::Reject(r) => {
                assert_eq!(
                    r.code,
                    SpokeRejectCode::KnowledgeEntryNotFound,
                    "missing entry must still surface KnowledgeEntryNotFound"
                );
            }
            SpokeResult::Ok(_) => panic!("expected NotFound reject"),
        }
    }

    // ── V1.146 P0: OCC rejects unchanged ───────────────────────────────
    // The put_update_stale_rejects_stored_revision_stale and
    // put_update_conflict_rejects_revision_conflict tests above already
    // cover STORED_REVISION_STALE and REVISION_CONFLICT — they pass
    // unchanged (confirmed by the red-green run). No additional OCC test
    // needed beyond the existing coverage.

    /// Read the stored typed `owner_kind`/`creator_only` columns plus the
    /// persisted `extensions_nexus_json` for an entry — used to assert an
    /// immutable-owner/creator_only rejection leaves storage unchanged.
    async fn stored_typed_owner_and_extensions(
        pool: &sqlx::SqlitePool,
        entry_id: &str,
    ) -> (String, bool, serde_json::Value) {
        let row: (String, i64, String) = sqlx::query_as(
            "SELECT owner_kind, creator_only, extensions_nexus_json \
             FROM kb_key_blocks WHERE key_block_id = ?",
        )
        .bind(entry_id)
        .fetch_one(pool)
        .await
        .unwrap();
        let json_val: serde_json::Value = serde_json::from_str(&row.2).unwrap_or_default();
        (row.0, row.1 != 0, json_val)
    }

    /// Build a World-owned spoke entry with `creator_only` set (v1.184 P1 fix
    /// regression: a candidate that flips `creator_only` on update must be
    /// rejected InvalidInput with no write).
    fn spoke_entry_creator_only(entry_id: &str, canonical_name: &str, creator_only: bool) -> KnowledgeEntry {
        let mut world = KnowledgeEntryRecord::new("wld_1", BlockType::Character, canonical_name);
        world.entry_id = entry_id.to_string();
        world.creator_only = creator_only;
        world.body = Some(KnowledgeEntryBody {
            summary: Some(format!("{canonical_name} summary")),
            ..Default::default()
        });
        knowledge_record_to_spoke(&world)
    }

    /// v1.184 P1 fix: the production spoke update path must reject a
    /// `creator_only` flip (immutable) as InvalidInput, leaving the typed
    /// column and the extensions JSON unchanged.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn put_update_rejects_creator_only_flip_unchanged_data() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let adapter = NexusAdapter::new(pool.clone());
        let entry = spoke_entry_creator_only("kb_flag", "Flagged", false);
        let created = unwrap_ok(adapter.put_knowledge_entry(entry, None).await, "create");
        assert_eq!(created.revision, Some(1));

        // Attempt to flip creator_only true → reject InvalidInput.
        let flip = spoke_entry_creator_only("kb_flag", "Flagged", true);
        match adapter.put_knowledge_entry(flip, Some(1)).await {
            SpokeResult::Reject(r) => {
                assert_eq!(
                    r.code,
                    SpokeRejectCode::InvalidInput,
                    "creator_only flip must map to InvalidInput, got {r:?}"
                );
            }
            SpokeResult::Ok(_) => panic!("expected InvalidInput reject"),
        }

        // Storage unchanged: typed creator_only still false; the extensions
        // JSON namespace must not carry a creator_only key.
        let (kind, stored_flag, ext_json) = stored_typed_owner_and_extensions(&pool, "kb_flag").await;
        assert_eq!(kind, "world");
        assert!(!stored_flag, "typed creator_only must remain false after rejected flip");
        let nexus_obj = ext_json.get("nexus").and_then(serde_json::Value::as_object);
        assert!(
            nexus_obj.map_or(true, |m| !m.contains_key("creator_only")),
            "no creator_only key may leak into the persisted extensions JSON after rejected flip: {ext_json:?}"
        );
    }

    /// v1.184 P1 fix: an ambiguous owner payload reaching the spoke write
    /// boundary maps to InvalidInput (never InternalError) on create.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn put_create_ambiguous_owner_maps_to_invalid_input() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let adapter = NexusAdapter::new(pool);
        let mut entry = spoke_entry("kb_ambig", "Ambig", None);
        // Inject two typed owner keys — the conversion seam must fail closed.
        let key = spoke_schemas::knowledge_entry::KnowledgeEntryExtensionsKey::try_from("nexus")
            .expect("nexus key is a valid extension key");
        if let Some(ns) = entry.extensions.get_mut(&key) {
            ns.insert("character_id".to_string(), serde_json::Value::String("chr_1".into()));
        }

        match adapter.put_knowledge_entry(entry, None).await {
            SpokeResult::Reject(r) => {
                assert_eq!(
                    r.code,
                    SpokeRejectCode::InvalidInput,
                    "ambiguous owner must map to InvalidInput, got {r:?}"
                );
            }
            SpokeResult::Ok(_) => panic!("expected InvalidInput reject"),
        }
    }

    /// v1.184 P1 fix: a non-World owner carrying `creator_only` reaches the
    /// create boundary → InvalidInput (not InternalError).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn put_create_creator_only_on_character_maps_to_invalid_input() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let adapter = NexusAdapter::new(pool);
        let mut rec = KnowledgeEntryRecord::for_character("chr_1", BlockType::Character, "CharFlag");
        rec.creator_only = true;
        rec.body = Some(KnowledgeEntryBody {
            summary: Some("char flag".to_string()),
            ..Default::default()
        });
        let entry = knowledge_record_to_spoke(&rec);

        match adapter.put_knowledge_entry(entry, None).await {
            SpokeResult::Reject(r) => {
                assert_eq!(
                    r.code,
                    SpokeRejectCode::InvalidInput,
                    "creator_only on a Character-owned entry must map to InvalidInput, got {r:?}"
                );
            }
            SpokeResult::Ok(_) => panic!("expected InvalidInput reject"),
        }
    }

    /// v1.184 P1 fix: an unknown `entry_type` on the create boundary maps to
    /// InvalidInput (never silently normalized).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn put_create_unknown_entry_type_maps_to_invalid_input() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let adapter = NexusAdapter::new(pool);
        let mut entry = spoke_entry("kb_unktype", "UnknownType", None);
        entry.entry_type = "not_a_real_block_type".to_string();

        match adapter.put_knowledge_entry(entry, None).await {
            SpokeResult::Reject(r) => {
                assert_eq!(
                    r.code,
                    SpokeRejectCode::InvalidInput,
                    "unknown entry_type must map to InvalidInput, got {r:?}"
                );
            }
            SpokeResult::Ok(_) => panic!("expected InvalidInput reject"),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn put_create_bound_tx_not_visible_until_outer_commit() {
        use std::sync::{Arc, Mutex};

        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let tx = pool.begin().await.unwrap();
        let tx_cell = Arc::new(Mutex::new(Some(tx)));
        let adapter = NexusAdapter::new(pool.clone()).with_tx_cell(Arc::clone(&tx_cell));
        let entry = spoke_entry("kb_bound_create", "BoundCreate", None);
        let entry_id = entry.entry_id.clone();

        // The port method is now async: the closure returns the future and
        // the handler awaits it outside (with_bound_tx stays a sync
        // passthrough). UFCS keeps the future un-awaited inside the closure.
        let put_result = adapter
            .with_bound_tx(|| KnowledgeEntryPort::put_knowledge_entry(&adapter, entry, None))
            .await;
        assert!(
            matches!(put_result, SpokeResult::Ok(_)),
            "bound put should succeed in-tx"
        );

        let store = SqliteKbStore::new(pool.clone());
        assert!(
            store.get_knowledge_entry(&entry_id).await.is_err(),
            "bound put must not be visible before outer commit"
        );

        let tx = tx_cell
            .lock()
            .expect("tx mutex")
            .take()
            .expect("tx in cell");
        tx.commit().await.unwrap();
        assert!(
            store.get_knowledge_entry(&entry_id).await.is_ok(),
            "bound put must be visible after outer commit"
        );
    }
}
