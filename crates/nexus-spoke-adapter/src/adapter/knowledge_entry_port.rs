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
use crate::conversion::{knowledge_record_to_spoke, spoke_to_knowledge_record};
use crate::extensions::build_extensions_nexus;
use crate::{KnowledgeEntry, KnowledgeEntryPort, SpokeReject, SpokeRejectCode, SpokeResult};
use async_trait::async_trait;
use nexus_knowledge::world_kb::store::{KbStore, KbStoreError};
use nexus_knowledge::world_kb::{KnowledgeEntryRecord, KnowledgeReadScope};
use nexus_local_db::kb_store::{
    cas_update_key_block_fields, CasKeyBlockFieldUpdate, SqliteKbStore,
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
        // Request-bound scope (durable §4.1, v1.191 P1 T8): a KE load on a
        // host-only adapter fails closed instead of reading the world.
        let _selection = match self.require_read_scope("get_knowledge_entry") {
            Ok(selection) => selection,
            Err(reject) => return SpokeResult::Reject(reject),
        };
        let world_entry = match self.load_admitted_entry(entry_id).await {
            Ok(Some(record)) => record,
            // Hidden and absent are indistinguishable (durable §4.2): the same
            // not-found reject covers both.
            Ok(None) => {
                return reject(
                    SpokeRejectCode::KnowledgeEntryNotFound,
                    format!("KnowledgeEntry not found: {entry_id}"),
                    json!({ "entry_id": entry_id }),
                );
            }
            Err(err) => return Self::map_get_err(err, entry_id),
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
        // Request-bound scope (durable §4.1, v1.191 P1 T8): no KE write
        // without an admitted selection — a host-only adapter rejects here
        // instead of mutating the world.
        if let Err(reject) = self.require_read_scope("put_knowledge_entry") {
            return SpokeResult::Reject(reject);
        }
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
#[allow(clippy::too_many_lines)] // single validated create transaction
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
    // L2 F1: the pre-check runs through the bound selection, so a row the
    // caller cannot see is not an existence oracle — it reads as absent here.
    //
    // Precedence (L2 ruling, v1.191 P1 T8): for an id that already exists AND
    // is visible to this caller, the conflict answer wins — `ALREADY_EXISTS` is
    // returned before any content-level refusal, so the legacy-key refusal
    // below (`legacy_creator_only_unsupported`, applied on the conversion path)
    // only ever answers requests for rows that would actually be created. The
    // stored row is neither read out (no key swallowed, nothing echoed) nor
    // written by this branch.
    match adapter.load_admitted_entry(&entry_id).await {
        Ok(Some(_)) => {
            return reject(
                SpokeRejectCode::KnowledgeEntryAlreadyExists,
                format!("Entry already exists: {entry_id}"),
                json!({ "entry_id": entry_id }),
            );
        }
        Ok(None) => {} // absent (or hidden) — the unique constraint is the race guard
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

    // Durable §9 (upsert): a write may only land in a container the bound
    // selection authorizes. The container comes from the candidate's narrative
    // owner axis (never from a client governance value), so an operation whose
    // grant points at another World/Character fails closed here.
    if !adapter.admits_container(&world_entry.owner) {
        return reject(
            SpokeRejectCode::InvalidInput,
            format!(
                "knowledge entry container ({} owner) is outside the bound read selection",
                world_entry.owner.kind()
            ),
            json!({ "entry_id": entry_id }),
        );
    }

    // V1.145 P0 T2: build `extensions.nexus` JSON at the adapter boundary so
    // the storage layer stays spoke-unaware. Mirrors the UPDATE CAS path in
    // `run_cas_update_in_tx` (spec §7.4); the JSON is passed opaquely to
    // `insert_key_block_with_extensions_in_tx`. v1.184 P1: owner-aware;
    // v1.191 P1 T8: the retired legacy key is dropped, never re-emitted.
    let extensions_nexus_json = serde_json::to_string(&build_extensions_nexus(
        &world_entry.owner,
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
        Err(
            e @ (KbStoreError::Validation(_)
            | KbStoreError::ValidationLegacy(_)
            | KbStoreError::ImmutableOwner(_)),
        ) => reject(
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
    put_update_unbound(adapter, pool, entry, expected).await
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
    let new_rev =
        match run_cas_update_in_tx(&mut tx, adapter, &entry_id, &world_entry, expected).await {
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
    adapter: &NexusAdapter<'_>,
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
    let new_rev =
        match run_cas_update_in_tx(&mut tx, adapter, &entry_id, &world_entry, expected).await {
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
#[cfg(feature = "compute")]
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
            match run_cas_update_in_tx(&mut tx, adapter, &entry_id, &world_entry, expected).await {
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
    adapter: &NexusAdapter<'_>,
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

    // Native governance is immutable on the ordinary (port) update path
    // (durable §3, v1.191 P1 T8 — the replacement for the retired
    // `creator_only` flag check). The stored pair is read back inside the tx
    // and compared to the candidate *before* the CAS writes, but ONLY when the
    // stored row is in the same world as the candidate — a candidate that
    // changes the pair is rejected InvalidInput with no write, so an ordinary
    // update (upsert/promote/compute settle) retains stored governance and can
    // never transfer it.
    //
    // This is also the guard that keeps a *hidden* row out of reach: a
    // same-world `owner-private` row admits an update only from a candidate
    // that already carries that exact pair, and no filtered read hands one out
    // (durable §4.2). Owner immutability needs no separate check: the
    // World-owned CAS lane already binds the candidate's `world_id` in its
    // predicate, so a row moved to another world between verification and CAS
    // misses the predicate and classifies as a world-conflict (`InternalError`
    // carrier) — exactly the spec §3.2 behavior. We must NOT intercept that
    // here as an immutable-governance error, or we would mask the world
    // conflict.
    // L2 F1 (HARD): admission comes from the *read path*, not from the
    // constructor marker. The stored row is loaded through the bound selection
    // — the same scoped read the port exposes — so an unscoped-CAS hole cannot
    // exist for any caller that reaches this function. A row the selection does
    // not admit is indistinguishable from an absent one, and its stored
    // governance (or its world) is never observed.
    let stored = match adapter.load_admitted_entry(entry_id).await {
        Ok(Some(stored)) => stored,
        Ok(None) => {
            // Same shape the CAS produces for an absent row: the caller is
            // ahead of the store, or the row is not theirs to see.
            return reject(
                SpokeRejectCode::RevisionConflict,
                format!(
                    "KnowledgeEntry not found for update: {entry_id} (expected base {expected})"
                ),
                json!({
                    "entry_id": entry_id,
                    "expectedBaseRevision": expected,
                    "storeRevision": null,
                }),
            );
        }
        Err(e) => {
            return reject(
                SpokeRejectCode::InternalError,
                format!("storage error reading stored governance for update: {e}"),
                json!({ "entry_id": entry_id }),
            );
        }
    };
    // Only a same-world governance change is an immutable-governance violation.
    // A different stored world falls through to the CAS, which classifies
    // world-conflict / not-found exactly as before.
    if stored.world_id() == world_entry.world_id()
        && (stored.holder_entry_id.as_deref() != world_entry.holder_entry_id.as_deref()
            || stored.disclosure.as_deref() != world_entry.disclosure.as_deref())
    {
        return reject(
            SpokeRejectCode::InvalidInput,
            "knowledge entry governance is immutable through the ordinary update path \
             (stored holder/disclosure differ from the candidate)",
            json!({ "entry_id": entry_id }),
        );
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
    let fields = CasKeyBlockFieldUpdate {
        canonical_name: Some(world_entry.canonical_name.as_str()),
        block_type: Some(block_type_str.as_str()),
        body_json: body_json.as_deref(),
        status: Some(world_entry.status.as_str()),
        source_anchor_json: Some(source_anchor_json.as_deref()),
        extensions_nexus_json: Some(extensions_nexus_json.as_str()),
        modules_json: Some(modules_json.as_deref()),
        source_provenance_kind: world_entry.source_provenance_kind.as_deref(),
    };
    let new_rev = match cas_update_key_block_fields(
        tx,
        entry_id,
        expected.cast_signed(),
        // V1.154 P2 (R3 closure): the world bind is the stored-world
        // expected by the request — the candidate's claimed world, which the
        // invoke gate verified against the stored row (spec §3.1). If a
        // cross-process writer moved the row to another world between the
        // gate check and this CAS, the predicate misses and the storage
        // layer classifies it as WorldConflict.
        world_id,
        &fields,
    )
    .await
    {
        Ok(new_rev) => new_rev,
        Err(e) => return NexusAdapter::map_cas_err(e, entry_id, expected),
    };

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

/// Caller-owned `BEGIN IMMEDIATE` transaction entry point for knowledge-entry
/// writes. Avoids the `with_tx_cell` lifetime coupling used by daemon promote.
///
/// v1.191 P1 T8: like every other KE-capable entry point, this requires the
/// caller's validated request-bound [`KnowledgeReadScope`] — the create path
/// refuses a container outside the selection, and the update path keeps
/// stored governance (durable §3/§4.1).
pub async fn put_knowledge_entry_in_tx(
    pool: &sqlx::SqlitePool,
    read_scope: &KnowledgeReadScope,
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    entry: KnowledgeEntry,
    expected_base_revision: Option<u64>,
) -> SpokeResult<KnowledgeEntry> {
    let adapter = NexusAdapter::new(pool.clone(), read_scope.clone());
    match expected_base_revision {
        None => put_create_in_tx(&adapter, pool, tx, entry).await,
        Some(expected) => put_update_in_tx(&adapter, tx, entry, expected).await,
    }
}

#[allow(clippy::too_many_lines)]
async fn put_create_in_tx(
    adapter: &NexusAdapter<'_>,
    pool: &sqlx::SqlitePool,
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    entry: KnowledgeEntry,
) -> SpokeResult<KnowledgeEntry> {
    let store = SqliteKbStore::new(pool.clone());
    let entry_id = entry.entry_id.clone();

    // L2 F1: same scoped pre-check as the port create path.
    match adapter.load_admitted_entry(&entry_id).await {
        Ok(Some(_)) => {
            return reject(
                SpokeRejectCode::KnowledgeEntryAlreadyExists,
                format!("Entry already exists: {entry_id}"),
                json!({ "entry_id": entry_id }),
            );
        }
        Ok(None) => {}
        Err(e) => {
            return reject(
                SpokeRejectCode::InternalError,
                format!("storage error on create pre-check: {e}"),
                json!({ "entry_id": entry_id }),
            );
        }
    }

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

    // Same container admission as the port create path (durable §9): the
    // caller-owned-tx entry point is a KE-capable mutation, so it is bound to
    // the same request scope.
    if !adapter.admits_container(&world_entry.owner) {
        return reject(
            SpokeRejectCode::InvalidInput,
            format!(
                "knowledge entry container ({} owner) is outside the bound read selection",
                world_entry.owner.kind()
            ),
            json!({ "entry_id": entry_id }),
        );
    }

    let extensions_nexus_json = serde_json::to_string(&build_extensions_nexus(
        &world_entry.owner,
        world_entry.created_from_command_id.as_deref(),
        world_entry.source_work_id.as_deref(),
        world_entry.source_chapter,
        world_entry.source_provenance_kind.as_deref(),
        &nexus_extras_extension_map(world_entry.extensions_nexus_extras.as_ref()),
    ))
    .unwrap_or_default();

    let insert_result = store
        .insert_key_block_with_extensions_in_tx(tx, world_entry, extensions_nexus_json)
        .await;

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
        Err(
            e @ (KbStoreError::Validation(_)
            | KbStoreError::ValidationLegacy(_)
            | KbStoreError::ImmutableOwner(_)),
        ) => reject(
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

async fn put_update_in_tx(
    adapter: &NexusAdapter<'_>,
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
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
    let new_rev =
        match run_cas_update_in_tx(tx, adapter, &entry_id, &world_entry, expected).await {
            SpokeResult::Ok(rev) => rev,
            SpokeResult::Reject(r) => return SpokeResult::Reject(r),
        };
    let mut result = entry;
    result.revision = Some(new_rev);
    SpokeResult::Ok(result)
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::KnowledgeEntryPort;
    use nexus_contracts::BlockType;
    use nexus_knowledge::world_kb::knowledge_entry::{
        KnowledgeOwnerRef, DISCLOSURE_OWNER_PRIVATE,
    };
    use nexus_knowledge::world_kb::{KnowledgeEntryBody, KnowledgeEntryRecord};
    use nexus_local_db::{open_pool, run_migrations};

    /// The admitted selection every port test operates inside (v1.191 P1 T8):
    /// Creator management review over `wld_1`, no known-governance holders —
    /// so a private row is invisible to these port tests unless a test says
    /// otherwise.
    fn world_scope() -> KnowledgeReadScope {
        KnowledgeReadScope::creator_management(vec![KnowledgeOwnerRef::world("wld_1")], Vec::new())
    }

    /// Register the fixture creator's holder so a governed row can be stored
    /// (the native `holder_entry_id` column is FK-bound to the registry, and
    /// these selections authorize no holder — so the row is private to them).
    async fn register_creator_holder(pool: &sqlx::SqlitePool) -> String {
        let mut tx = pool.begin().await.unwrap();
        let holder = nexus_local_db::holders::ensure_creator_holder_in_tx(&mut tx, "ctr_test")
            .await
            .unwrap();
        tx.commit().await.unwrap();
        holder
    }

    /// A KE-capable adapter bound to [`world_scope`].
    fn scoped(pool: sqlx::SqlitePool) -> NexusAdapter<'static> {
        NexusAdapter::new(pool, world_scope())
    }

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

        let adapter = scoped(pool);
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

        let adapter = scoped(pool.clone());
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

        let adapter = scoped(pool);
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

        let adapter = scoped(pool);
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

        let adapter = scoped(pool);
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

        let adapter = scoped(pool);
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

        let adapter = scoped(pool);
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

        let adapter = scoped(pool);
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

        let adapter = scoped(pool.clone());
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

        let adapter = scoped(pool);
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

        let adapter = scoped(pool);
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
        let adapter = scoped(pool.clone());
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

        let adapter = scoped(pool);
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

    /// Byte-level fingerprint of one stored `kb_key_blocks` row (governance +
    /// revision + the persisted extensions document) for unchanged-storage
    /// assertions.
    async fn stored_row_fingerprint(
        pool: &sqlx::SqlitePool,
        entry_id: &str,
    ) -> (String, Option<String>, Option<String>, Option<i64>, String) {
        sqlx::query_as(
            "SELECT owner_kind, holder_entry_id, disclosure, revision, extensions_nexus_json \
             FROM kb_key_blocks WHERE key_block_id = ?",
        )
        .bind(entry_id)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    /// Read the stored container kind + native governance columns plus the
    /// persisted `extensions_nexus_json` for an entry — used to assert a
    /// rejected write leaves storage unchanged.
    async fn stored_owner_and_extensions(
        pool: &sqlx::SqlitePool,
        entry_id: &str,
    ) -> (String, Option<String>, Option<String>, serde_json::Value) {
        let row: (String, Option<String>, Option<String>, String) = sqlx::query_as(
            "SELECT owner_kind, holder_entry_id, disclosure, extensions_nexus_json \
             FROM kb_key_blocks WHERE key_block_id = ?",
        )
        .bind(entry_id)
        .fetch_one(pool)
        .await
        .unwrap();
        let json_val: serde_json::Value = serde_json::from_str(&row.3).unwrap_or_default();
        (row.0, row.1, row.2, json_val)
    }

    /// Build a World-owned spoke entry carrying a native governance pair
    /// (an `owner-private` row under the supplied holder).
    fn spoke_entry_private(entry_id: &str, canonical_name: &str, holder: &str) -> KnowledgeEntry {
        let mut world = KnowledgeEntryRecord::new("wld_1", BlockType::Character, canonical_name);
        world.entry_id = entry_id.to_string();
        world.holder_entry_id = Some(holder.to_string());
        world.disclosure = Some(DISCLOSURE_OWNER_PRIVATE.to_string());
        world.body = Some(KnowledgeEntryBody {
            summary: Some(format!("{canonical_name} summary")),
            ..Default::default()
        });
        knowledge_record_to_spoke(&world)
    }

    /// v1.191 P1 T8 (replaces the retired `creator_only` flip regression):
    /// the ordinary port update path cannot transfer governance. A candidate
    /// that adds an `owner-private` pair to a stored shared row is rejected
    /// `InvalidInput` with zero write — neither the governance columns nor the
    /// extensions document move.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn put_update_rejects_governance_transfer_unchanged_data() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let adapter = scoped(pool.clone());
        let entry = spoke_entry("kb_flag", "Flagged", None);
        let created = unwrap_ok(adapter.put_knowledge_entry(entry, None).await, "create");
        assert_eq!(created.revision, Some(1));

        // Attempt to author governance through the ordinary update → reject.
        let transfer = spoke_entry_private("kb_flag", "Flagged", "hld_test");
        match adapter.put_knowledge_entry(transfer, Some(1)).await {
            SpokeResult::Reject(r) => {
                assert_eq!(
                    r.code,
                    SpokeRejectCode::InvalidInput,
                    "governance transfer must map to InvalidInput, got {r:?}"
                );
            }
            SpokeResult::Ok(_) => panic!("expected InvalidInput reject"),
        }

        // Storage unchanged: still a shared World row, revision still 1.
        let (kind, holder, disclosure, ext_json) =
            stored_owner_and_extensions(&pool, "kb_flag").await;
        assert_eq!(kind, "world");
        assert_eq!(holder, None, "holder_entry_id must stay absent");
        assert_eq!(disclosure, None, "disclosure must stay absent");
        let revision: Option<i64> =
            sqlx::query_scalar("SELECT revision FROM kb_key_blocks WHERE key_block_id = ?")
                .bind("kb_flag")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(revision, Some(1), "a refused update must not bump revision");
        let nexus_obj = ext_json.get("nexus").and_then(serde_json::Value::as_object);
        assert!(
            nexus_obj.is_none_or(|m| !m.contains_key("creator_only")),
            "no creator_only key may leak into the persisted extensions JSON: {ext_json:?}"
        );
    }

    /// v1.191 P1 T8: a wire entry that still carries the retired
    /// `extensions.nexus.creator_only` key is refused at the create boundary
    /// with the stable legacy reason — `false` included, never ignored.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn put_create_legacy_creator_only_key_maps_to_invalid_input() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let adapter = scoped(pool);
        for legacy in [serde_json::Value::Bool(false), serde_json::Value::Bool(true)] {
            let mut entry = spoke_entry("kb_legacy", "Legacy", None);
            let key =
                spoke_schemas::knowledge_entry::KnowledgeEntryExtensionsKey::try_from("nexus")
                    .expect("nexus key is a valid extension key");
            entry
                .extensions
                .entry(key)
                .or_default()
                .insert("creator_only".to_string(), legacy);

            match adapter.put_knowledge_entry(entry, None).await {
                SpokeResult::Reject(r) => {
                    assert_eq!(
                        r.code,
                        SpokeRejectCode::InvalidInput,
                        "legacy creator_only must map to InvalidInput, got {r:?}"
                    );
                    assert!(
                        r.message.contains("legacy_creator_only_unsupported"),
                        "the stable reason must ride the reject: {r:?}"
                    );
                }
                SpokeResult::Ok(_) => panic!("expected InvalidInput reject"),
            }
        }
    }

    /// v1.191 P1 T8: a hidden row is indistinguishable from an absent one on
    /// the by-id read — the private row of another holder, and a row outside
    /// the bound container, both answer `KnowledgeEntryNotFound`.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn get_hidden_row_is_indistinguishable_from_absent() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        // Seed a foreign-holder private row directly (the authoring path is
        // T7's; this test only needs the stored shape).
        let mut private =
            KnowledgeEntryRecord::new("wld_1", BlockType::Character, "HiddenNote");
        private.entry_id = "kb_hidden".to_string();
        let holder = register_creator_holder(&pool).await;
        private.holder_entry_id = Some(holder);
        private.disclosure = Some(DISCLOSURE_OWNER_PRIVATE.to_string());
        SqliteKbStore::new(pool.clone())
            .insert_knowledge_entry(private)
            .await
            .unwrap();

        let adapter = scoped(pool);
        for entry_id in ["kb_hidden", "kb_absent"] {
            match adapter.get_knowledge_entry(entry_id).await {
                SpokeResult::Reject(r) => assert_eq!(
                    r.code,
                    SpokeRejectCode::KnowledgeEntryNotFound,
                    "{entry_id} must read as not found, got {r:?}"
                ),
                SpokeResult::Ok(_) => panic!("{entry_id} must not be readable"),
            }
        }
    }

    /// v1.191 P1 T8: the host metadata/tools construction cannot read or
    /// write knowledge — every KE port fails closed with the
    /// `read_scope_missing` marker instead of widening to an unscoped read.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn host_only_adapter_refuses_every_ke_entry_point() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let adapter = NexusAdapter::new_host(pool);
        assert!(adapter.read_scope().is_none());

        let results = [
            adapter.get_knowledge_entry("kb_any").await,
            adapter
                .put_knowledge_entry(spoke_entry("kb_any", "Any", None), None)
                .await,
            adapter
                .put_knowledge_entry(spoke_entry("kb_any", "Any", None), Some(1))
                .await,
        ];
        for result in results {
            match result {
                SpokeResult::Reject(r) => {
                    assert_eq!(r.code, SpokeRejectCode::InternalError, "got {r:?}");
                    assert_eq!(
                        r.details.as_ref().and_then(|d| d.get("read_scope_missing")),
                        Some(&serde_json::Value::Bool(true)),
                        "the missing-scope reject must carry its marker: {r:?}"
                    );
                }
                SpokeResult::Ok(_) => panic!("host-only adapter must not serve KE reads/writes"),
            }
        }
    }

    /// v1.184 P1 fix: an ambiguous owner payload reaching the spoke write
    /// boundary maps to `InvalidInput` (never `InternalError`) on create.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn put_create_ambiguous_owner_maps_to_invalid_input() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let adapter = scoped(pool);
        let mut entry = spoke_entry("kb_ambig", "Ambig", None);
        // Inject two typed owner keys — the conversion seam must fail closed.
        let key = spoke_schemas::knowledge_entry::KnowledgeEntryExtensionsKey::try_from("nexus")
            .expect("nexus key is a valid extension key");
        if let Some(ns) = entry.extensions.get_mut(&key) {
            ns.insert(
                "character_id".to_string(),
                serde_json::Value::String("chr_1".into()),
            );
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

    /// L2 F1 (HARD): admission lives in the read/write path, so a scoped
    /// adapter whose selection excludes the row's container cannot update or
    /// read it — the refusal is the absent shape and the row keeps its revision.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn put_update_outside_bound_containers_is_refused_without_mutation() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let writer = scoped(pool.clone());
        let created = unwrap_ok(
            writer
                .put_knowledge_entry(spoke_entry("kb_out_of_scope", "Out", None), None)
                .await,
            "create",
        );
        assert_eq!(created.revision, Some(1));

        // A different selection: same database, no authority over `wld_1`.
        let outsider = NexusAdapter::new(
            pool.clone(),
            KnowledgeReadScope::creator_management(
                vec![KnowledgeOwnerRef::world("wld_other")],
                Vec::new(),
            ),
        );
        match outsider
            .put_knowledge_entry(spoke_entry("kb_out_of_scope", "Out", None), Some(1))
            .await
        {
            SpokeResult::Reject(r) => assert_eq!(
                r.code,
                SpokeRejectCode::RevisionConflict,
                "an out-of-selection update must read as absent, got {r:?}"
            ),
            SpokeResult::Ok(_) => panic!("an out-of-selection update must be refused"),
        }
        match outsider.get_knowledge_entry("kb_out_of_scope").await {
            SpokeResult::Reject(r) => assert_eq!(r.code, SpokeRejectCode::KnowledgeEntryNotFound),
            SpokeResult::Ok(_) => panic!("an out-of-selection row must not be readable"),
        }

        let revision: Option<i64> =
            sqlx::query_scalar("SELECT revision FROM kb_key_blocks WHERE key_block_id = ?")
                .bind("kb_out_of_scope")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(revision, Some(1), "a refused update must not mutate the row");
    }

    /// L2 ruling: an existing **visible** id answers `ALREADY_EXISTS` before the
    /// legacy-key refusal — the refusal is for rows that would really be
    /// created. The stored row (revision, extensions document, governance) must
    /// stay byte-identical: no key is swallowed, nothing is written.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn put_create_on_existing_visible_id_wins_over_the_legacy_refusal() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let adapter = scoped(pool.clone());
        let created = unwrap_ok(
            adapter
                .put_knowledge_entry(spoke_entry("kb_prio", "Prio", None), None)
                .await,
            "create",
        );
        assert_eq!(created.revision, Some(1));

        // Simulate a stored pre-cutover row: the cutover migration strips the
        // retired key, so a surviving row that still carries it is reproduced
        // directly in the extensions document.
        sqlx::query("UPDATE kb_key_blocks SET extensions_nexus_json = ? WHERE key_block_id = ?")
            .bind(r#"{"nexus":{"world_id":"wld_1","creator_only":false}}"#)
            .bind("kb_prio")
            .execute(&pool)
            .await
            .unwrap();
        let before = stored_row_fingerprint(&pool, "kb_prio").await;

        // Same visible id, and the request itself carries the legacy key.
        let mut attempt = spoke_entry("kb_prio", "Prio", None);
        let key = spoke_schemas::knowledge_entry::KnowledgeEntryExtensionsKey::try_from("nexus")
            .expect("nexus key is a valid extension key");
        attempt
            .extensions
            .entry(key)
            .or_default()
            .insert("creator_only".to_string(), serde_json::Value::Bool(false));

        match adapter.put_knowledge_entry(attempt, None).await {
            SpokeResult::Reject(r) => {
                assert_eq!(
                    r.code,
                    SpokeRejectCode::KnowledgeEntryAlreadyExists,
                    "the visible-id conflict must win over the legacy refusal, got {r:?}"
                );
                assert_eq!(
                    r.message.matches("legacy_creator_only_unsupported").count(),
                    0,
                    "the conflict, not the legacy reason, is the answer here: {r:?}"
                );
            }
            SpokeResult::Ok(_) => panic!("expected AlreadyExists for an existing visible id"),
        }

        assert_eq!(
            stored_row_fingerprint(&pool, "kb_prio").await,
            before,
            "the stored row must be byte-identical (revision, governance, extensions)"
        );
    }

    /// v1.191 P1 T8: a create whose container is outside the bound selection
    /// is refused `InvalidInput` before any insert (durable §9). The bound
    /// selection here authorizes only `wld_1`, so a Character-owned candidate
    /// has no home in it.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn put_create_outside_bound_container_maps_to_invalid_input() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let adapter = scoped(pool.clone());
        let mut rec =
            KnowledgeEntryRecord::for_character("chr_1", BlockType::Character, "CharNote");
        rec.entry_id = "kb_char_note".to_string();
        rec.body = Some(KnowledgeEntryBody {
            summary: Some("char note".to_string()),
            ..Default::default()
        });
        let entry = knowledge_record_to_spoke(&rec);

        match adapter.put_knowledge_entry(entry, None).await {
            SpokeResult::Reject(r) => {
                assert_eq!(
                    r.code,
                    SpokeRejectCode::InvalidInput,
                    "a foreign container must map to InvalidInput, got {r:?}"
                );
            }
            SpokeResult::Ok(_) => panic!("expected InvalidInput reject"),
        }

        let rows: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM kb_key_blocks WHERE key_block_id = ?")
                .bind("kb_char_note")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(rows, 0, "a refused create must not insert a row");
    }

    /// v1.184 P1 fix: an unknown `entry_type` on the create boundary maps to
    /// `InvalidInput` (never silently normalized).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn put_create_unknown_entry_type_maps_to_invalid_input() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let adapter = scoped(pool);
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
        let adapter = scoped(pool.clone()).with_tx_cell(Arc::clone(&tx_cell));
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
