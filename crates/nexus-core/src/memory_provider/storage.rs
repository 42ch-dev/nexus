//! Provider-private storage and transaction layer for `LocalMemoryProvider`.
//!
//! Every write runs inside the core writer protocol: the pool is opened through
//! [`init_guarded_pool`], so its connections carry the writer scalar functions
//! and the `core_writer_registration` row the `guard_memory_*` triggers demand.
//! A raw pool (no registration, no scalar functions) is fenced with
//! `WRITER_FENCED` at the schema boundary — this layer adds no second admission
//! path, and the provider never opens its own unguarded handle.
//!
//! Two invariants live here rather than in the facade because they must be one
//! transaction with the effect:
//!
//! * **Dedup + receipt** (§7.0.5, MEM-R008/R021). The `memory_receipts` primary
//!   key IS the namespace `(space_id, requester_principal_id, op,
//!   idempotency_key)`; a same-namespace replay returns the stored receipt
//!   without re-executing, and a same-namespace different-`payload_digest`
//!   attempt is `IDEMPOTENCY_CONFLICT`. Reading the namespace only after the
//!   effect committed would make replay re-execute, so every mutating entry
//!   point opens the transaction, probes the namespace first, and inserts the
//!   receipt as the last statement of the same transaction.
//! * **Revision CAS** (MEM-R007). `expected_revision` is compared and `revision`
//!   incremented in the same `UPDATE ... WHERE revision = ?` statement.
//!
//! `memory.subjects.ensure` is the sole idempotency-key exemption (§7.0.5): its
//! receipt carries a NULL key, which the partial unique index excludes from the
//! dedup namespace, so two ensures never collide and never replay each other.

use std::path::Path;

use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{json, Value};
use sqlx::SqlitePool;
use uuid::Uuid;

use nexus_local_db::writer_protocol::{init_guarded_pool, GuardedPool};
use nexus_local_db::LocalDbError;

/// §6.10 receipt ids are provider-assigned opaque strings.
fn mint_receipt_id() -> String {
    format!("rcp_{}", Uuid::new_v4().simple())
}

/// The one canonical timestamp rendering.
///
/// Every timestamp column this module writes and every stored expiry is compared
/// as text, so the rendering must be fixed: RFC 3339 UTC, whole seconds, `Z`.
pub(super) fn timestamp(now: DateTime<Utc>) -> String {
    now.to_rfc3339_opts(SecondsFormat::Secs, true)
}

#[derive(Debug)]
pub(super) enum StoreError {
    /// Something the caller can see and correct (missing row, stale revision…).
    NotFound(String),
    RevisionConflict {
        current: i64,
    },
    IdempotencyConflict,
    /// A forbidden lifecycle transition (§7.7/§7.10).
    StatusTransition(String),
    /// A stored row this provider's own writer could not have produced.
    Corrupt(String),
    Database(LocalDbError),
}

impl From<LocalDbError> for StoreError {
    fn from(value: LocalDbError) -> Self {
        Self::Database(value)
    }
}

impl From<sqlx::Error> for StoreError {
    fn from(value: sqlx::Error) -> Self {
        Self::Database(LocalDbError::Sqlx(value))
    }
}

type StoreResult<T> = Result<T, StoreError>;

#[derive(Debug, sqlx::FromRow)]
pub(super) struct SpaceRow {
    pub space_id: String,
    pub display_name: Option<String>,
    pub authority_host_id: String,
    pub authority_generation: i64,
    pub status: String,
    pub revision: i64,
    pub created_at: String,
    pub pending_receipts: i64,
}

#[derive(Debug, sqlx::FromRow)]
pub(super) struct SubjectRow {
    pub space_id: String,
    pub subject_id: String,
    pub kind_ns: Option<String>,
    pub kind_id: Option<String>,
    pub status: String,
    pub fenced_contexts_json: String,
    pub revision: i64,
    pub created_at: String,
}

impl SubjectRow {
    pub(super) fn fenced_contexts(&self) -> StoreResult<Vec<String>> {
        serde_json::from_str(&self.fenced_contexts_json).map_err(|e| {
            StoreError::Corrupt(format!(
                "memory_subjects.fenced_contexts_json for {}: {e}",
                self.subject_id
            ))
        })
    }
}

#[derive(Debug, sqlx::FromRow)]
pub(super) struct GrantRow {
    pub grant_id: String,
    pub space_id: String,
    pub subject_id: Option<String>,
    pub principal_id: String,
    pub capabilities_json: String,
    pub context_ids_json: String,
    pub shared_access: i64,
    pub expires_at: Option<String>,
    pub revoked_at: Option<String>,
    pub revision: i64,
    pub created_at: String,
}

impl GrantRow {
    pub(super) fn capabilities(&self) -> StoreResult<Vec<String>> {
        serde_json::from_str(&self.capabilities_json).map_err(|e| {
            StoreError::Corrupt(format!(
                "memory_grants.capabilities_json for {}: {e}",
                self.grant_id
            ))
        })
    }

    pub(super) fn context_ids(&self) -> StoreResult<Vec<String>> {
        serde_json::from_str(&self.context_ids_json).map_err(|e| {
            StoreError::Corrupt(format!(
                "memory_grants.context_ids_json for {}: {e}",
                self.grant_id
            ))
        })
    }
}

/// A committed memory receipt as stored (§6.10). `idempotency_key` is `None`
/// only for the `memory.subjects.ensure` exemption.
#[derive(Debug, sqlx::FromRow)]
pub(super) struct ReceiptRow {
    pub space_id: String,
    pub requester_principal_id: String,
    pub op: String,
    pub idempotency_key: Option<String>,
    pub receipt_id: String,
    pub payload_digest: String,
    pub status: String,
    pub result_json: Option<String>,
    pub created_at: String,
    pub completed_at: Option<String>,
}

/// Stable operation identity handed to every mutating entry point.
pub(super) struct OpContext<'a> {
    pub space_id: &'a str,
    pub requester_principal_id: &'a str,
    pub op: &'a str,
    /// `None` for the `memory.subjects.ensure` exemption only (§7.0.5).
    pub idempotency_key: Option<&'a str>,
    pub payload_digest: &'a str,
}

/// A grant row as the caller issues it (§7.9).
pub(super) struct NewGrant<'a> {
    pub grant_id: &'a str,
    pub space_id: &'a str,
    pub subject_id: Option<&'a str>,
    pub principal_id: &'a str,
    pub capabilities: &'a [String],
    pub context_ids: &'a [String],
    pub shared_access: bool,
    pub expires_at: Option<DateTime<Utc>>,
}

/// §7.7 fence target.
pub(super) enum FenceTarget<'a> {
    Subject,
    Context(&'a str),
}

/// §7.7 fence action.
#[derive(Debug, Clone, Copy)]
pub(super) enum FenceAction {
    Fence,
    Unfence,
    Archive,
}

pub(super) struct MemoryStore {
    pool: SqlitePool,
    /// Holds this provider's writer admission for the provider's lifetime.
    _guarded: GuardedPool,
}

enum Dedup {
    Fresh,
    Replay(ReceiptRow),
}

const SPACE_COLUMNS: &str = "s.space_id, s.display_name, s.authority_host_id, \
     s.authority_generation, s.status, s.revision, s.created_at, \
     (SELECT COUNT(*) FROM memory_receipts r WHERE r.space_id = s.space_id \
      AND r.status = 'pending') AS pending_receipts";

const SUBJECT_COLUMNS: &str = "space_id, subject_id, kind_ns, kind_id, status, \
     fenced_contexts_json, revision, created_at";

const GRANT_COLUMNS: &str = "grant_id, space_id, subject_id, principal_id, \
     capabilities_json, context_ids_json, shared_access, expires_at, revoked_at, \
     revision, created_at";

const RECEIPT_COLUMNS: &str = "space_id, requester_principal_id, op, idempotency_key, \
     receipt_id, payload_digest, status, result_json, created_at, completed_at";

impl MemoryStore {
    pub(super) async fn open(db_path: &Path, creator_id: &str) -> StoreResult<Self> {
        let guarded = init_guarded_pool(db_path, creator_id).await?;
        let pool = guarded.clone_pool();
        Ok(Self {
            pool,
            _guarded: guarded,
        })
    }

    /// Operator provisioning (§7.0.6): the local space and the first `admin`
    /// grant exist before any protocol op can be called.
    ///
    /// Both inserts are `INSERT OR IGNORE` keyed on their deterministic ids, so
    /// a re-open neither duplicates nor resets provisioning state.
    pub(super) async fn provision(
        &self,
        space_id: &str,
        authority_host_id: &str,
        creator_id: &str,
        bootstrap_grant_id: &str,
        display_name: Option<&str>,
        now: DateTime<Utc>,
    ) -> StoreResult<()> {
        let at = timestamp(now);
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT OR IGNORE INTO memory_spaces \
             (space_id, display_name, authority_host_id, authority_generation, status, revision, created_at) \
             VALUES (?, ?, ?, 1, 'active', 1, ?)",
        )
        .bind(space_id)
        .bind(display_name)
        .bind(authority_host_id)
        .bind(&at)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT OR IGNORE INTO memory_grants \
             (grant_id, space_id, subject_id, principal_id, capabilities_json, context_ids_json, \
              shared_access, expires_at, revoked_at, revision, created_at) \
             VALUES (?, ?, NULL, ?, '[\"admin\"]', '[]', 0, NULL, NULL, 1, ?)",
        )
        .bind(bootstrap_grant_id)
        .bind(space_id)
        .bind(creator_id)
        .bind(&at)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub(super) async fn find_space(&self, space_id: &str) -> StoreResult<Option<SpaceRow>> {
        let sql = format!("SELECT {SPACE_COLUMNS} FROM memory_spaces s WHERE s.space_id = ?");
        Ok(sqlx::query_as::<_, SpaceRow>(sqlx::AssertSqlSafe(sql))
            .bind(space_id)
            .fetch_optional(&self.pool)
            .await?)
    }

    /// Spaces the principal may see: it holds at least one live grant in them
    /// (§7.2 "any valid grant in the space, or provider-configured visibility";
    /// this provider configures visibility as grant-derived).
    pub(super) async fn visible_spaces(
        &self,
        principal_id: &str,
        now: DateTime<Utc>,
    ) -> StoreResult<Vec<SpaceRow>> {
        let sql = format!(
            "SELECT {SPACE_COLUMNS} FROM memory_spaces s \
             WHERE EXISTS ( \
                 SELECT 1 FROM memory_grants g \
                 WHERE g.space_id = s.space_id AND g.principal_id = ? \
                   AND g.revoked_at IS NULL \
                   AND (g.expires_at IS NULL OR g.expires_at > ?) \
             ) \
             ORDER BY s.space_id"
        );
        Ok(sqlx::query_as::<_, SpaceRow>(sqlx::AssertSqlSafe(sql))
            .bind(principal_id)
            .bind(timestamp(now))
            .fetch_all(&self.pool)
            .await?)
    }

    /// Freeze state change (§7.3/§7.4). `frozen`/`active` no-ops succeed without
    /// bumping the revision (§7.0.4).
    pub(super) async fn set_space_status(
        &self,
        ctx: &OpContext<'_>,
        expected_revision: i64,
        status: &str,
        now: DateTime<Utc>,
    ) -> StoreResult<ReceiptRow> {
        let mut tx = self.pool.begin().await?;
        if let Dedup::Replay(row) = probe(&mut tx, ctx).await? {
            return Ok(row);
        }
        let current = require_space_revision(&mut tx, ctx.space_id, expected_revision).await?;
        let revision = if current.status == status {
            current.revision
        } else {
            let updated = sqlx::query(
                "UPDATE memory_spaces SET status = ?, revision = revision + 1 \
                 WHERE space_id = ? AND revision = ?",
            )
            .bind(status)
            .bind(ctx.space_id)
            .bind(current.revision)
            .execute(&mut *tx)
            .await?;
            if updated.rows_affected() != 1 {
                return Err(StoreError::RevisionConflict {
                    current: current.revision,
                });
            }
            current.revision + 1
        };
        let pending = pending_receipts(&mut tx, ctx.space_id).await?;
        let result = json!({
            "status": status,
            "pending_receipts": pending,
            "revision": revision,
        });
        commit_receipt(tx, ctx, &result, now).await
    }

    /// §7.5 get-or-create. `kind` is creation-only metadata: an existing subject
    /// is returned unchanged whatever kind the caller now names.
    pub(super) async fn ensure_subject(
        &self,
        ctx: &OpContext<'_>,
        subject_id: &str,
        kind: Option<(&str, &str)>,
        now: DateTime<Utc>,
    ) -> StoreResult<ReceiptRow> {
        let mut tx = self.pool.begin().await?;
        if let Dedup::Replay(row) = probe(&mut tx, ctx).await? {
            return Ok(row);
        }
        let existing = find_subject_with(&mut tx, ctx.space_id, subject_id).await?;
        let (created, status, revision) = match existing {
            // Archived is terminal for fences, but get-or-create reports the
            // subject honestly rather than resurrecting or refusing it.
            Some(row) => (false, row.status, row.revision),
            None => {
                sqlx::query(
                    "INSERT INTO memory_subjects \
                     (space_id, subject_id, kind_ns, kind_id, status, fenced_contexts_json, revision, created_at) \
                     VALUES (?, ?, ?, ?, 'active', '[]', 1, ?)",
                )
                .bind(ctx.space_id)
                .bind(subject_id)
                .bind(kind.map(|(ns, _)| ns))
                .bind(kind.map(|(_, id)| id))
                .bind(timestamp(now))
                .execute(&mut *tx)
                .await?;
                (true, "active".to_string(), 1)
            }
        };
        let result = json!({
            "subject_id": subject_id,
            "created": created,
            "status": status,
            "revision": revision,
        });
        commit_receipt(tx, ctx, &result, now).await
    }

    pub(super) async fn find_subject(
        &self,
        space_id: &str,
        subject_id: &str,
    ) -> StoreResult<Option<SubjectRow>> {
        let sql = format!(
            "SELECT {SUBJECT_COLUMNS} FROM memory_subjects WHERE space_id = ? AND subject_id = ?"
        );
        Ok(sqlx::query_as::<_, SubjectRow>(sqlx::AssertSqlSafe(sql))
            .bind(space_id)
            .bind(subject_id)
            .fetch_optional(&self.pool)
            .await?)
    }

    /// §7.7 fence state machine. Only the listed transitions are admitted; the
    /// "already in the requested state" arms are no-ops that keep the revision
    /// (§7.0.4), and `unfence` of a non-fenced target is
    /// `INVALID_STATUS_TRANSITION` as §7.7 names explicitly.
    pub(super) async fn fence_subject(
        &self,
        ctx: &OpContext<'_>,
        subject_id: &str,
        target: &FenceTarget<'_>,
        action: FenceAction,
        expected_revision: i64,
        now: DateTime<Utc>,
    ) -> StoreResult<ReceiptRow> {
        let mut tx = self.pool.begin().await?;
        if let Dedup::Replay(row) = probe(&mut tx, ctx).await? {
            return Ok(row);
        }
        let current = find_subject_with(&mut tx, ctx.space_id, subject_id)
            .await?
            .ok_or_else(|| StoreError::NotFound(format!("subject {subject_id}")))?;
        if current.revision != expected_revision {
            return Err(StoreError::RevisionConflict {
                current: current.revision,
            });
        }
        if current.status == "archived" {
            return Err(StoreError::StatusTransition(format!(
                "subject {subject_id} is archived (terminal)"
            )));
        }
        let mut fenced_contexts = current.fenced_contexts()?;
        let mut status = current.status.clone();
        let mut changed = false;
        match (target, action) {
            (FenceTarget::Subject, FenceAction::Fence) => match current.status.as_str() {
                "active" => {
                    status = "fenced".to_string();
                    changed = true;
                }
                // Already fenced: no-op success.
                _ => {}
            },
            (FenceTarget::Subject, FenceAction::Unfence) => match current.status.as_str() {
                "fenced" => {
                    status = "active".to_string();
                    changed = true;
                }
                _ => {
                    return Err(StoreError::StatusTransition(format!(
                        "subject {subject_id} is not fenced"
                    )))
                }
            },
            (FenceTarget::Subject, FenceAction::Archive) => {
                status = "archived".to_string();
                changed = true;
            }
            (FenceTarget::Context(context_id), FenceAction::Fence) => {
                if !fenced_contexts.iter().any(|known| known == context_id) {
                    fenced_contexts.push((*context_id).to_string());
                    changed = true;
                }
            }
            (FenceTarget::Context(context_id), FenceAction::Unfence) => {
                let before = fenced_contexts.len();
                fenced_contexts.retain(|known| known != context_id);
                if fenced_contexts.len() == before {
                    return Err(StoreError::StatusTransition(format!(
                        "context {context_id} is not fenced on subject {subject_id}"
                    )));
                }
                changed = true;
            }
            // §7.7: archive on a context target is INVALID_INPUT, refused by the
            // request validator before this layer.
            (FenceTarget::Context(_), FenceAction::Archive) => {
                return Err(StoreError::StatusTransition(
                    "archive is not valid on a context target".to_string(),
                ))
            }
        }
        let revision = if changed {
            let contexts_json = serde_json::to_string(&fenced_contexts)
                .map_err(|e| StoreError::Corrupt(format!("fenced contexts: {e}")))?;
            let updated = sqlx::query(
                "UPDATE memory_subjects SET status = ?, fenced_contexts_json = ?, \
                 revision = revision + 1 WHERE space_id = ? AND subject_id = ? AND revision = ?",
            )
            .bind(&status)
            .bind(&contexts_json)
            .bind(ctx.space_id)
            .bind(subject_id)
            .bind(current.revision)
            .execute(&mut *tx)
            .await?;
            if updated.rows_affected() != 1 {
                return Err(StoreError::RevisionConflict {
                    current: current.revision,
                });
            }
            current.revision + 1
        } else {
            current.revision
        };
        let result = json!({
            "subject_id": subject_id,
            "status": status,
            "fenced_contexts": fenced_contexts,
            "revision": revision,
        });
        commit_receipt(tx, ctx, &result, now).await
    }

    pub(super) async fn grants(
        &self,
        space_id: &str,
        subject_id: Option<&str>,
        principal_id: Option<&str>,
    ) -> StoreResult<Vec<GrantRow>> {
        let sql = format!(
            "SELECT {GRANT_COLUMNS} FROM memory_grants \
             WHERE space_id = ? AND (? IS NULL OR subject_id = ?) \
             AND (? IS NULL OR principal_id = ?) \
             ORDER BY grant_id"
        );
        Ok(sqlx::query_as::<_, GrantRow>(sqlx::AssertSqlSafe(sql))
            .bind(space_id)
            .bind(subject_id)
            .bind(subject_id)
            .bind(principal_id)
            .bind(principal_id)
            .fetch_all(&self.pool)
            .await?)
    }

    /// A stable marker for cursor invalidation: any grant write in the space
    /// (`revision` bump or new row) changes the sum, so a cursor minted before
    /// it is `CURSOR_INVALID` (§7.0.7, MEM-R026).
    pub(super) async fn grant_snapshot(&self, space_id: &str) -> StoreResult<i64> {
        let (sum,): (i64,) = sqlx::query_as(
            "SELECT COALESCE(SUM(revision), 0) FROM memory_grants WHERE space_id = ?",
        )
        .bind(space_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(sum)
    }

    /// §7.9 issuance. The non-amplification decision is made by the caller from
    /// the same grant set; this transaction owns the dedup namespace and the
    /// row write.
    pub(super) async fn insert_grant(
        &self,
        ctx: &OpContext<'_>,
        new_grant: &NewGrant<'_>,
        now: DateTime<Utc>,
    ) -> StoreResult<ReceiptRow> {
        let mut tx = self.pool.begin().await?;
        if let Dedup::Replay(row) = probe(&mut tx, ctx).await? {
            return Ok(row);
        }
        let capabilities = serde_json::to_string(new_grant.capabilities)
            .map_err(|e| StoreError::Corrupt(format!("grant capabilities: {e}")))?;
        let context_ids = serde_json::to_string(new_grant.context_ids)
            .map_err(|e| StoreError::Corrupt(format!("grant contexts: {e}")))?;
        sqlx::query(
            "INSERT INTO memory_grants \
             (grant_id, space_id, subject_id, principal_id, capabilities_json, context_ids_json, \
              shared_access, expires_at, revoked_at, revision, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, NULL, 1, ?)",
        )
        .bind(new_grant.grant_id)
        .bind(new_grant.space_id)
        .bind(new_grant.subject_id)
        .bind(new_grant.principal_id)
        .bind(&capabilities)
        .bind(&context_ids)
        .bind(i64::from(new_grant.shared_access))
        .bind(new_grant.expires_at.map(timestamp))
        .bind(timestamp(now))
        .execute(&mut *tx)
        .await?;
        let result = json!({
            "grant_id": new_grant.grant_id,
            "revision": 1,
        });
        commit_receipt(tx, ctx, &result, now).await
    }

    /// §7.10 terminal revoke. A repeated revoke is a no-op success: the grant is
    /// already terminal, and §7.0.4 forbids inventing a revision bump for it.
    pub(super) async fn revoke_grant(
        &self,
        ctx: &OpContext<'_>,
        grant_id: &str,
        expected_revision: i64,
        now: DateTime<Utc>,
    ) -> StoreResult<ReceiptRow> {
        let mut tx = self.pool.begin().await?;
        if let Dedup::Replay(row) = probe(&mut tx, ctx).await? {
            return Ok(row);
        }
        let current: Option<(i64, String, Option<String>)> = sqlx::query_as(
            "SELECT revision, space_id, revoked_at FROM memory_grants WHERE grant_id = ?",
        )
        .bind(grant_id)
        .fetch_optional(&mut *tx)
        .await?;
        let (revision, space_id, revoked_at) =
            current.ok_or_else(|| StoreError::NotFound(format!("grant {grant_id}")))?;
        if space_id != ctx.space_id {
            return Err(StoreError::NotFound(format!("grant {grant_id}")));
        }
        if revision != expected_revision {
            return Err(StoreError::RevisionConflict { current: revision });
        }
        let revision = if revoked_at.is_some() {
            revision
        } else {
            let updated = sqlx::query(
                "UPDATE memory_grants SET revoked_at = ?, revision = revision + 1 \
                 WHERE grant_id = ? AND revision = ?",
            )
            .bind(timestamp(now))
            .bind(grant_id)
            .bind(revision)
            .execute(&mut *tx)
            .await?;
            if updated.rows_affected() != 1 {
                return Err(StoreError::RevisionConflict { current: revision });
            }
            revision + 1
        };
        let result = json!({
            "grant_id": grant_id,
            "revoked": true,
            "revision": revision,
        });
        commit_receipt(tx, ctx, &result, now).await
    }
}

/// Probe the dedup namespace inside the caller's transaction.
///
/// A keyless op (the `memory.subjects.ensure` exemption) binds NULL, which
/// SQLite never matches, so it never replays and always records a fresh
/// receipt.
async fn probe(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    ctx: &OpContext<'_>,
) -> StoreResult<Dedup> {
    let sql = format!(
        "SELECT {RECEIPT_COLUMNS} FROM memory_receipts \
         WHERE space_id = ? AND requester_principal_id = ? AND op = ? AND idempotency_key = ?"
    );
    let existing = sqlx::query_as::<_, ReceiptRow>(sqlx::AssertSqlSafe(sql))
        .bind(ctx.space_id)
        .bind(ctx.requester_principal_id)
        .bind(ctx.op)
        .bind(ctx.idempotency_key)
        .fetch_optional(&mut **tx)
        .await?;
    match existing {
        None => Ok(Dedup::Fresh),
        Some(row) if row.payload_digest == ctx.payload_digest => Ok(Dedup::Replay(row)),
        Some(_) => Err(StoreError::IdempotencyConflict),
    }
}

async fn commit_receipt(
    mut tx: sqlx::Transaction<'_, sqlx::Sqlite>,
    ctx: &OpContext<'_>,
    result: &Value,
    now: DateTime<Utc>,
) -> StoreResult<ReceiptRow> {
    let at = timestamp(now);
    let receipt_id = mint_receipt_id();
    let result_json = serde_json::to_string(result)
        .map_err(|e| StoreError::Corrupt(format!("receipt result: {e}")))?;
    sqlx::query(
        "INSERT INTO memory_receipts \
         (space_id, requester_principal_id, op, idempotency_key, receipt_id, payload_digest, \
          status, result_json, error_json, created_at, completed_at) \
         VALUES (?, ?, ?, ?, ?, ?, 'committed', ?, NULL, ?, ?)",
    )
    .bind(ctx.space_id)
    .bind(ctx.requester_principal_id)
    .bind(ctx.op)
    .bind(ctx.idempotency_key)
    .bind(&receipt_id)
    .bind(ctx.payload_digest)
    .bind(&result_json)
    .bind(&at)
    .bind(&at)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(ReceiptRow {
        space_id: ctx.space_id.to_string(),
        requester_principal_id: ctx.requester_principal_id.to_string(),
        op: ctx.op.to_string(),
        idempotency_key: ctx.idempotency_key.map(str::to_string),
        receipt_id,
        payload_digest: ctx.payload_digest.to_string(),
        status: "committed".to_string(),
        result_json: Some(result_json),
        created_at: at.clone(),
        completed_at: Some(at),
    })
}

async fn find_subject_with(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    space_id: &str,
    subject_id: &str,
) -> StoreResult<Option<SubjectRow>> {
    let sql = format!(
        "SELECT {SUBJECT_COLUMNS} FROM memory_subjects WHERE space_id = ? AND subject_id = ?"
    );
    Ok(sqlx::query_as::<_, SubjectRow>(sqlx::AssertSqlSafe(sql))
        .bind(space_id)
        .bind(subject_id)
        .fetch_optional(&mut **tx)
        .await?)
}

async fn require_space_revision(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    space_id: &str,
    expected_revision: i64,
) -> StoreResult<SpaceRow> {
    let sql = format!("SELECT {SPACE_COLUMNS} FROM memory_spaces s WHERE s.space_id = ?");
    let row = sqlx::query_as::<_, SpaceRow>(sqlx::AssertSqlSafe(sql))
        .bind(space_id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| StoreError::NotFound(format!("space {space_id}")))?;
    if row.revision != expected_revision {
        return Err(StoreError::RevisionConflict {
            current: row.revision,
        });
    }
    Ok(row)
}

async fn pending_receipts(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    space_id: &str,
) -> StoreResult<u64> {
    let (pending,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM memory_receipts WHERE space_id = ? AND status = 'pending'",
    )
    .bind(space_id)
    .fetch_one(&mut **tx)
    .await?;
    Ok(u64::try_from(pending).unwrap_or(0))
}
