//! Durable operation receipts — the receipt store and the frozen Recover
//! handshake (v1.207 P3 / RN-OGA-5).
//!
//! Spec: `.mstar/iterations/v1.207/specs/connect-replay-and-operation-receipts.md`
//! §B.2 (the `operation_receipts` table this module reads and writes) and
//! §B.3 (the frozen recovery order implemented by [`recover`]).
//!
//! A logical operation (a compute Run, a Connect invoke) carries a stable
//! [`operation_id`](nexus_core) and one receipt row. The row is the durable
//! answer to "did this already happen?": recovery asks the receipt store
//! **before** any re-apply, so a cancel/timeout/crash retry is answered from
//! the stored receipt instead of being applied a second time.
//!
//! ## Write protocol
//!
//! - [`begin_operation`] writes the `running` receipt **first** (spec §B.3
//!   step 3), before the effect runs. The primary key is first-writer-wins:
//!   an insert for an id that is already owned by a receipt with the **same**
//!   `request_fingerprint` is a replay and returns the stored row unchanged; a
//!   **different** fingerprint is a typed
//!   [`LocalDbError::OperationIdConflict`] (`operation_id_conflict`), never a
//!   silent dedupe.
//! - [`settle_operation`] settles the row terminally. A receipt that is
//!   already terminal is never downgraded: a late settlement returns the
//!   stored terminal row unchanged (first terminal wins).
//!
//! ## Storage/SQL note
//!
//! The statements here are built at runtime rather than as
//! `sqlx::query!` macros: the workspace shares one root `.sqlx` offline cache
//! across crates, and this change does not regenerate it (the same convention
//! `js_provider_journal.rs` follows). The SQL is otherwise static, so the
//! `.sqlx` cache stays untouched.

use sqlx::SqlitePool;

use crate::LocalDbError;

/// Receipt consumer: a compute Run (`subject_id` = the run id).
pub const CONSUMER_COMPUTE_RUN: &str = "compute_run";
/// Receipt consumer: a Connect invoke (`subject_id` = `<peer_session_id>/<op>`).
pub const CONSUMER_CONNECT_INVOKE: &str = "connect_invoke";

/// Frozen wire code of the first-writer-wins receipt refusal (spec §C).
pub const OPERATION_ID_CONFLICT_CODE: &str = "operation_id_conflict";

/// Receipt status while the effect is in flight.
pub const STATUS_RUNNING: &str = "running";

/// Terminal receipt status: the effect committed a result.
pub const STATUS_FINISHED: &str = "finished";
/// Terminal receipt status: the effect failed.
pub const STATUS_FAILED: &str = "failed";
/// Terminal receipt status: the operation was cancelled.
pub const STATUS_CANCELLED: &str = "cancelled";
/// Terminal receipt status: the operation was orphaned (e.g. the owning
/// process died) and settled without a re-apply.
pub const STATUS_INTERRUPTED: &str = "interrupted";

/// Every terminal status. A terminal receipt is never downgraded back to
/// [`STATUS_RUNNING`] (the retained contract on
/// `core-provider-operation.schema.json`, applied to this store).
pub const TERMINAL_STATUSES: [&str; 4] = [
    STATUS_FINISHED,
    STATUS_FAILED,
    STATUS_CANCELLED,
    STATUS_INTERRUPTED,
];

/// True when `status` settles a receipt (terminal).
#[must_use]
pub fn is_terminal_status(status: &str) -> bool {
    TERMINAL_STATUSES.contains(&status)
}

/// One durable receipt row — the storage shape of `operation_receipts`, and
/// the payload [`crate::get_operation_receipt`] returns. The owned wire DTO is
/// `nexus_contracts::CoreOperationReceipt`.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct OperationReceipt {
    pub operation_id: String,
    pub consumer: String,
    pub subject_id: String,
    pub status: String,
    pub request_fingerprint: String,
    pub result_json: Option<String>,
    pub error_json: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub terminal_at: Option<String>,
    pub sequence: i64,
}

impl OperationReceipt {
    /// True when this receipt is terminal — the recovery answer is the
    /// receipt itself, never a re-apply.
    #[must_use]
    pub fn is_terminal(&self) -> bool {
        is_terminal_status(&self.status)
    }

    /// True when the given request fingerprint is the one this receipt was
    /// first written with (i.e. the caller is replaying the same logical
    /// operation, not colliding with another).
    #[must_use]
    pub fn matches_fingerprint(&self, request_fingerprint: &str) -> bool {
        self.request_fingerprint == request_fingerprint
    }
}

/// The frozen Recover decision (spec §B.3 step 3/4).
///
/// The store decides the class; the consumer owns what it renders (a typed
/// Busy / uncertain refusal, or the effect run for [`Self::ApplyOnce`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryDecision {
    /// A terminal receipt exists: answer the replay from the receipt. The
    /// effect is **not** re-applied.
    AnswerFromReceipt(OperationReceipt),
    /// A `running` receipt whose owner is still live: an in-progress answer
    /// (spec §B.3: typed Busy). The effect is **not** re-applied here.
    InProgress(OperationReceipt),
    /// No receipt at all: safe to apply exactly once — write the `running`
    /// receipt FIRST ([`begin_operation`]) and only run the effect when that
    /// call reports [`BeginOutcome::Acquired`], then [`settle_operation`]
    /// terminally.
    ApplyOnce,
    /// A `running` receipt whose owner is no longer live (crash/restart
    /// ambiguity) on a write with no terminal receipt. Spec §B.3 item 4: the
    /// caller gets a typed uncertain/blocked answer — **never** a blind
    /// retry, and the store never fabricates a terminal for it.
    Uncertain(OperationReceipt),
}

/// The outcome of [`begin_operation`]: who performed the first (`running`)
/// write for this operation id.
///
/// This is the exclusive-acquisition gate of the recover → begin → effect
/// sequence (§B.3 step 3): only the [`Self::Acquired`] caller may run the
/// effect. Every other caller — a same-fingerprint replay, a racing loser, or
/// a retry against an already-terminal receipt — gets [`Self::Existing`] and
/// must answer from the stored receipt instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BeginOutcome {
    /// This call wrote the `running` receipt: the caller owns the effect and
    /// is the only one allowed to apply it.
    Acquired(OperationReceipt),
    /// A receipt for this operation id already existed (this call inserted
    /// nothing). The effect must **not** be run by this caller; the stored
    /// receipt — `running`, or terminal if it was already settled — is the
    /// answer.
    Existing(OperationReceipt),
}

impl BeginOutcome {
    /// The stored receipt, whichever side of the acquisition this call landed
    /// on.
    #[must_use]
    pub const fn receipt(&self) -> &OperationReceipt {
        match self {
            Self::Acquired(receipt) | Self::Existing(receipt) => receipt,
        }
    }

    /// True only for the caller that acquired the effect (and must run it).
    #[must_use]
    pub const fn is_acquired(&self) -> bool {
        matches!(self, Self::Acquired(_))
    }
}

const fn db_err(e: sqlx::Error) -> LocalDbError {
    LocalDbError::Sqlx(e)
}

/// RFC 3339 UTC timestamp, as the `created_at` / `updated_at` / `terminal_at`
/// columns store it (`datetime('now')` is not RFC 3339, so the store stamps
/// itself).
fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Read one receipt by operation id.
///
/// Returns `Ok(None)` for an unknown id — recovery never fabricates a receipt
/// for an operation it has no durable record of.
///
/// # Errors
///
/// Returns [`LocalDbError::Sqlx`] if the database query fails.
pub async fn get_operation_receipt(
    pool: &SqlitePool,
    operation_id: &str,
) -> Result<Option<OperationReceipt>, LocalDbError> {
    sqlx::query_as::<_, OperationReceipt>(
        "SELECT operation_id, consumer, subject_id, status, request_fingerprint, \
                result_json, error_json, created_at, updated_at, terminal_at, sequence \
           FROM operation_receipts WHERE operation_id = ?",
    )
    .bind(operation_id)
    .fetch_optional(pool)
    .await
    .map_err(db_err)
}

/// Read the receipt owned by one consumer subject (`consumer`, `subject_id`).
///
/// The §B.2 consumer subject is the recovery-side handle: compute Run keys
/// `subject_id` on the `run_id`, so a boot pass that finds a run row stuck
/// `running` can reach its receipt without re-deriving the operation id from
/// the original request (which is not stored). The `(consumer, subject_id)`
/// pair is indexed; more than one row for a pair would mean two logical
/// operations shared one subject — the first (lowest `sequence`) is returned,
/// and `None` means no receipt was ever written for the subject.
///
/// # Errors
///
/// Returns [`LocalDbError::Sqlx`] if the database query fails.
pub async fn get_operation_receipt_by_subject(
    pool: &SqlitePool,
    consumer: &str,
    subject_id: &str,
) -> Result<Option<OperationReceipt>, LocalDbError> {
    sqlx::query_as::<_, OperationReceipt>(
        "SELECT operation_id, consumer, subject_id, status, request_fingerprint, \
                result_json, error_json, created_at, updated_at, terminal_at, sequence \
           FROM operation_receipts WHERE consumer = ? AND subject_id = ? \
          ORDER BY sequence ASC LIMIT 1",
    )
    .bind(consumer)
    .bind(subject_id)
    .fetch_optional(pool)
    .await
    .map_err(db_err)
}

/// Write the `running` receipt for an operation **before** its effect runs
/// (spec §B.3 step 3), and report who acquired the effect.
///
/// First-writer-wins on `operation_id`:
///
/// - no receipt exists ⇒ this call inserts `running` and returns
///   [`BeginOutcome::Acquired`] — the caller owns the effect and is the only
///   one allowed to run it;
/// - a receipt exists with the same `request_fingerprint` ⇒ replay/racing
///   loser: [`BeginOutcome::Existing`] with the stored row unchanged
///   (whatever status it holds — a terminal row stays terminal), and the
///   effect must NOT be run;
/// - a receipt exists with a **different** fingerprint ⇒
///   [`LocalDbError::OperationIdConflict`], the typed `operation_id_conflict`
///   refusal. The existing row is left untouched.
///
/// The insert is `ON CONFLICT(operation_id) DO NOTHING` and the outcome comes
/// from the statement's own affected-row count, so exactly one racer is told
/// it acquired the effect and every other racer is told it did not.
///
/// The store reads the id FIRST and never re-inserts an existing row: the schema
/// refuses an insert over a terminal id (a settled answer is never
/// deleted-and-reinserted), so a replay after settlement is answered from the
/// stored row rather than by attempting an insert that a replacement clause
/// could turn into a delete.
///
/// # Errors
///
/// [`LocalDbError::ValidationError`] for an unknown `consumer`,
/// [`LocalDbError::OperationIdConflict`] on a fingerprint collision, and
/// [`LocalDbError::Sqlx`] on database failure.
pub async fn begin_operation(
    pool: &SqlitePool,
    operation_id: &str,
    consumer: &str,
    subject_id: &str,
    request_fingerprint: &str,
) -> Result<BeginOutcome, LocalDbError> {
    if !is_known_consumer(consumer) {
        return Err(LocalDbError::ValidationError(format!(
            "unknown operation receipt consumer '{consumer}'"
        )));
    }

    // Read FIRST (QC1-C001): a receipt row is never re-inserted. The schema
    // refuses an insert over a TERMINAL id, so a replay after settlement must be
    // answered from the stored row here instead of attempting the insert the
    // store used to rely on being a no-op.
    if let Some(stored) = get_operation_receipt(pool, operation_id).await? {
        if !stored.matches_fingerprint(request_fingerprint) {
            return Err(LocalDbError::OperationIdConflict {
                operation_id: operation_id.to_string(),
            });
        }
        return Ok(BeginOutcome::Existing(stored));
    }

    let now = now_rfc3339();
    let inserted = sqlx::query(
        "INSERT INTO operation_receipts \
             (operation_id, consumer, subject_id, status, request_fingerprint, \
              result_json, error_json, created_at, updated_at, terminal_at, sequence) \
         VALUES (?, ?, ?, 'running', ?, NULL, NULL, ?, ?, NULL, \
                 COALESCE((SELECT MAX(sequence) FROM operation_receipts), 0) + 1) \
         ON CONFLICT(operation_id) DO NOTHING",
    )
    .bind(operation_id)
    .bind(consumer)
    .bind(subject_id)
    .bind(request_fingerprint)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await
    .map_err(db_err)?;

    // `changes()` distinguishes the two sides of the first-writer-wins race:
    // exactly one racer inserted the `running` row (and owns the effect), the
    // others inserted nothing and must answer from the stored receipt. Both
    // sides then read the same stored row back.
    let acquired = inserted.rows_affected() == 1;

    let stored = get_operation_receipt(pool, operation_id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    if !stored.matches_fingerprint(request_fingerprint) {
        return Err(LocalDbError::OperationIdConflict {
            operation_id: operation_id.to_string(),
        });
    }
    Ok(if acquired {
        BeginOutcome::Acquired(stored)
    } else {
        BeginOutcome::Existing(stored)
    })
}

/// Settle a receipt terminally, and return the stored row.
///
/// `status` must be terminal and `payload` is required: §B.2's contract is
/// "terminal payload, exactly one set on terminal settlement", so every
/// terminal settlement carries exactly one of `result_json` / `error_json`.
/// The column is this store's rendering — `finished` carries the success
/// result in `result_json`, and `failed` / `cancelled` / `interrupted` carry
/// the non-success reason in `error_json`.
///
/// Only a `running` row is settled. A receipt that is already terminal is
/// immutable — the schema refuses every update of a settled row — so a late
/// or racing settlement returns the stored terminal row unchanged: the first
/// terminal wins, and it can never be rewritten.
///
/// # Errors
///
/// [`LocalDbError::ValidationError`] for a non-terminal `status`,
/// [`LocalDbError::Sqlx`] (with [`sqlx::Error::RowNotFound`]) when no receipt
/// has been begun for `operation_id`, and [`LocalDbError::Sqlx`] on database
/// failure.
pub async fn settle_operation(
    pool: &SqlitePool,
    operation_id: &str,
    status: &str,
    payload: &str,
) -> Result<OperationReceipt, LocalDbError> {
    let (result_json, error_json) = match status {
        STATUS_FINISHED => (Some(payload), None),
        // Non-success termination: the cancellation / interruption reason is
        // the terminal payload, so the contract's "exactly one set" holds for
        // every terminal settlement.
        STATUS_FAILED | STATUS_CANCELLED | STATUS_INTERRUPTED => (None, Some(payload)),
        other => {
            return Err(LocalDbError::ValidationError(format!(
                "operation '{operation_id}' cannot be settled as '{other}': \
                 a settlement is terminal"
            )));
        }
    };

    let now = now_rfc3339();
    sqlx::query(
        "UPDATE operation_receipts \
            SET status = ?, result_json = ?, error_json = ?, terminal_at = ?, updated_at = ? \
          WHERE operation_id = ? AND status = 'running'",
    )
    .bind(status)
    .bind(result_json)
    .bind(error_json)
    .bind(&now)
    .bind(&now)
    .bind(operation_id)
    .execute(pool)
    .await
    .map_err(db_err)?;

    // Whether this statement settled the row or lost to an earlier terminal
    // (or raced another settler), the stored row is the truth. A missing row
    // means `settle` was called without `begin` — a caller bug, surfaced as
    // `RowNotFound` rather than a fabricated receipt.
    get_operation_receipt(pool, operation_id)
        .await?
        .ok_or_else(|| db_err(sqlx::Error::RowNotFound))
}

/// Classify the frozen Recover decision for a receipt (spec §B.3 step 3/4).
///
/// `owner_is_live` is the caller's knowledge of whether the process owning a
/// `running` receipt is still alive; the store cannot infer liveness, so it is
/// an explicit input. It is only consulted for a `running` receipt.
#[must_use]
pub fn classify_recovery(
    receipt: Option<&OperationReceipt>,
    owner_is_live: bool,
) -> RecoveryDecision {
    match receipt {
        Some(receipt) if receipt.is_terminal() => {
            RecoveryDecision::AnswerFromReceipt(receipt.clone())
        }
        Some(receipt) if owner_is_live => RecoveryDecision::InProgress(receipt.clone()),
        Some(receipt) => RecoveryDecision::Uncertain(receipt.clone()),
        None => RecoveryDecision::ApplyOnce,
    }
}

/// Ask the receipt store FIRST and decide (spec §B.3 steps 2–3).
///
/// This is the frozen-order entry point for recovery: it reads the receipt
/// before anything else can re-apply. [`RecoveryDecision::ApplyOnce`] is the
/// caller's cue to write the `running` receipt via [`begin_operation`] *before*
/// running the effect, then [`settle_operation`] terminally — and never to
/// apply twice for an id that already has a receipt.
///
/// # Errors
///
/// Returns [`LocalDbError::Sqlx`] if the receipt read fails.
pub async fn recover(
    pool: &SqlitePool,
    operation_id: &str,
    owner_is_live: bool,
) -> Result<RecoveryDecision, LocalDbError> {
    let receipt = get_operation_receipt(pool, operation_id).await?;
    Ok(classify_recovery(receipt.as_ref(), owner_is_live))
}

/// True for the two consumers the `operation_receipts` CHECK admits.
#[must_use]
pub fn is_known_consumer(consumer: &str) -> bool {
    matches!(consumer, CONSUMER_COMPUTE_RUN | CONSUMER_CONNECT_INVOKE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::writer_protocol::{init_engine_pool, GuardedPoolOptions};

    /// A real migrated + admitted engine pool, exactly the production shape the
    /// native core uses, so the writer-protocol guards on the receipt table are
    /// exercised (a plain migrator pool would bypass admission).
    async fn admitted_pool() -> (SqlitePool, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("state.db");
        let guarded = init_engine_pool(
            &db_path,
            crate::writer_protocol::BOOTSTRAP_CREATOR_ID,
            GuardedPoolOptions::default(),
        )
        .await
        .expect("open admitted engine pool");
        (guarded.clone_pool(), dir)
    }

    fn op_id(suffix: &str) -> String {
        // A wire-shaped id (`op_` + 32 lowercase hex), so the rows mirror
        // production receipts. The suffix must already be lowercase hex: the
        // helper refuses anything else rather than silently aliasing two
        // fixture ids onto one row (e.g. `7g` and `7a`).
        assert!(
            suffix
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
            "fixture id suffix must be lowercase hex: {suffix}"
        );
        format!("op_{suffix:0>32}")
    }

    /// Begin an operation and require that THIS call acquired the effect.
    async fn begin_acquired(
        pool: &SqlitePool,
        operation_id: &str,
        consumer: &str,
        subject_id: &str,
        request_fingerprint: &str,
    ) -> OperationReceipt {
        match begin_operation(
            pool,
            operation_id,
            consumer,
            subject_id,
            request_fingerprint,
        )
        .await
        .expect("begin operation")
        {
            BeginOutcome::Acquired(receipt) => receipt,
            BeginOutcome::Existing(receipt) => {
                panic!("expected to acquire the effect, got existing receipt {receipt:?}")
            }
        }
    }

    /// The receipt write path is **cooperative** (spec §B.2, third amendment):
    /// the DIRECT admission — the one the Connect host opens its workspace DB
    /// with (`apps/nexus42` `Schema::init` → `nexus_local_db::init_pool`) — may
    /// begin and settle receipts, so the Connect write surface can enforce
    /// §B.3 on the pool it already has (no engine ownership, no second pool).
    ///
    /// Receipt integrity is writer-agnostic and therefore unchanged by the
    /// admission class: the first terminal wins and the settled row is
    /// immutable even for the direct writer.
    #[tokio::test]
    async fn direct_admission_can_begin_and_settle_receipts() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("state.db");
        let direct = crate::init_pool(&db_path)
            .await
            .expect("direct-admission pool");

        let id = op_id("dc");
        let begun = begin_acquired(
            &direct,
            &id,
            CONSUMER_CONNECT_INVOKE,
            "peer-session-1/upsert",
            "fp-direct",
        )
        .await;
        assert_eq!(begun.status, STATUS_RUNNING);

        let settled = settle_operation(&direct, &id, STATUS_FINISHED, r#"{"ok":true}"#)
            .await
            .expect("the direct writer settles the receipt");
        assert_eq!(settled.status, STATUS_FINISHED);
        assert!(settled.terminal_at.is_some());
        assert_eq!(settled.result_json.as_deref(), Some(r#"{"ok":true}"#));

        // First terminal wins for the direct writer too.
        let late = settle_operation(&direct, &id, STATUS_FAILED, r#"{"late":true}"#)
            .await
            .expect("a late settlement reads the stored terminal back");
        assert_eq!(late, settled, "the settled receipt is never rewritten");

        // Terminal immutability fires for EVERY admitted writer, not just the
        // engine owner: a direct UPDATE of the settled row aborts.
        let err =
            sqlx::query("UPDATE operation_receipts SET status = 'running' WHERE operation_id = ?")
                .bind(&id)
                .execute(&direct)
                .await
                .expect_err("a terminal receipt must be immutable for the direct writer");
        assert!(
            err.to_string()
                .contains("OPERATION_RECEIPT_TERMINAL_IMMUTABLE"),
            "expected the terminal-immutability abort, got: {err}"
        );
        let stored = get_operation_receipt(&direct, &id).await.unwrap().unwrap();
        assert_eq!(stored, settled);

        direct.close().await;
    }

    /// The subject lookup is the recovery-side handle: the receipt for a
    /// consumer subject is reachable without re-deriving the operation id,
    /// and an unknown subject is `None` (never a fabricated receipt).
    #[tokio::test]
    async fn receipt_is_reachable_by_consumer_subject() {
        let (pool, _dir) = admitted_pool().await;
        let id = op_id("aa");
        let begun = begin_acquired(&pool, &id, CONSUMER_COMPUTE_RUN, "run-subject", "fp-aa").await;
        let settled = settle_operation(&pool, &id, STATUS_FINISHED, r#"{"ok":true}"#)
            .await
            .expect("settle");
        assert_eq!(settled.status, STATUS_FINISHED);

        let found = get_operation_receipt_by_subject(&pool, CONSUMER_COMPUTE_RUN, "run-subject")
            .await
            .unwrap()
            .expect("the subject's receipt is reachable");
        assert_eq!(found, settled, "the subject lookup returns the stored row");
        assert_eq!(found.operation_id, begun.operation_id);

        // The pair is consumer-scoped: the same subject under the other
        // consumer is a different receipt (and here, none).
        assert!(
            get_operation_receipt_by_subject(&pool, CONSUMER_CONNECT_INVOKE, "run-subject")
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            get_operation_receipt_by_subject(&pool, CONSUMER_COMPUTE_RUN, "run-unknown")
                .await
                .unwrap()
                .is_none(),
            "an unknown subject never fabricates a receipt"
        );
    }

    /// Crash: a `running` receipt whose owner died is ambiguous — the recovery
    /// answer is `uncertain`, never `ApplyOnce`, so the effect is not applied
    /// again.
    #[tokio::test]
    async fn crashed_running_receipt_is_uncertain_never_reapplied() {
        let (pool, _dir) = admitted_pool().await;
        let id = op_id("1");
        let begun = begin_acquired(&pool, &id, CONSUMER_COMPUTE_RUN, "run-1", "fp-1").await;
        assert_eq!(begun.status, STATUS_RUNNING);
        assert!(begun.terminal_at.is_none());

        // The owner process is gone: replay must not re-apply.
        let decision = recover(&pool, &id, false).await.unwrap();
        match decision {
            RecoveryDecision::Uncertain(receipt) => assert_eq!(receipt.status, STATUS_RUNNING),
            other => panic!("crash ambiguity must be Uncertain, got {other:?}"),
        }
        // A replayed begin does NOT re-acquire the effect.
        match begin_operation(&pool, &id, CONSUMER_COMPUTE_RUN, "run-1", "fp-1")
            .await
            .unwrap()
        {
            BeginOutcome::Existing(receipt) => assert_eq!(receipt.status, STATUS_RUNNING),
            BeginOutcome::Acquired(receipt) => panic!("a replay must not re-acquire: {receipt:?}"),
        }
        // The store fabricated no terminal for the orphaned receipt.
        assert_eq!(
            get_operation_receipt(&pool, &id)
                .await
                .unwrap()
                .unwrap()
                .status,
            STATUS_RUNNING
        );
    }

    /// Timeout / still in flight: a `running` receipt with a live owner is an
    /// in-progress answer, not a re-apply and not an `uncertain` refusal.
    #[tokio::test]
    async fn live_running_receipt_is_in_progress_not_reapplied() {
        let (pool, _dir) = admitted_pool().await;
        let id = op_id("2");
        begin_acquired(&pool, &id, CONSUMER_COMPUTE_RUN, "run-2", "fp-2").await;
        match recover(&pool, &id, true).await.unwrap() {
            RecoveryDecision::InProgress(receipt) => {
                assert_eq!(receipt.status, STATUS_RUNNING);
                assert_eq!(receipt.subject_id, "run-2");
            }
            other => panic!("a live owner must be InProgress, got {other:?}"),
        }
    }

    /// Cancel: a cancelled receipt is terminal, so the replay is answered from
    /// the receipt and a retry of the same logical call stays on the same
    /// terminal row (no second apply, no resurrection).
    #[tokio::test]
    async fn cancelled_receipt_answers_the_replay_without_reapplying() {
        let (pool, _dir) = admitted_pool().await;
        let id = op_id("3");
        begin_acquired(
            &pool,
            &id,
            CONSUMER_CONNECT_INVOKE,
            "peer-1/tools.nexus.x",
            "fp-3",
        )
        .await;
        let settled = settle_operation(
            &pool,
            &id,
            STATUS_CANCELLED,
            "{\"reason\":\"cancel_requested\"}",
        )
        .await
        .unwrap();
        assert_eq!(settled.status, STATUS_CANCELLED);
        assert!(settled.terminal_at.is_some());
        // §B.2: a terminal settlement carries exactly one payload. This store
        // renders a non-success reason (cancel/interruption) in `error_json`.
        assert_eq!(settled.result_json, None);
        assert_eq!(
            settled.error_json.as_deref(),
            Some("{\"reason\":\"cancel_requested\"}")
        );

        // Even with the owner reported live, a terminal receipt wins.
        match recover(&pool, &id, true).await.unwrap() {
            RecoveryDecision::AnswerFromReceipt(receipt) => {
                assert_eq!(receipt.status, STATUS_CANCELLED);
                assert_eq!(receipt.terminal_at, settled.terminal_at);
            }
            other => panic!("a terminal receipt must answer, got {other:?}"),
        }

        // Replaying the SAME logical call returns the same terminal row: one
        // receipt, sequence and created_at unchanged — no second apply.
        let replayed = match begin_operation(
            &pool,
            &id,
            CONSUMER_CONNECT_INVOKE,
            "peer-1/tools.nexus.x",
            "fp-3",
        )
        .await
        .unwrap()
        {
            BeginOutcome::Existing(receipt) => receipt,
            BeginOutcome::Acquired(receipt) => {
                panic!("a replay must not re-acquire the effect: {receipt:?}")
            }
        };
        assert_eq!(replayed.status, STATUS_CANCELLED);
        assert_eq!(
            replayed.error_json.as_deref(),
            Some("{\"reason\":\"cancel_requested\"}"),
            "the terminal payload survives the replay unchanged"
        );
        assert_eq!(replayed.sequence, settled.sequence);
        assert_eq!(replayed.created_at, settled.created_at);
        let stored = get_operation_receipt(&pool, &id).await.unwrap().unwrap();
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM operation_receipts WHERE operation_id = ?")
                .bind(&id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count, 1, "a replay never inserts a second receipt");
        assert_eq!(stored.sequence, settled.sequence);
    }

    /// No receipt: the caller may apply exactly once; the effect-id receipt is
    /// written `running` FIRST and settles terminal.
    #[tokio::test]
    async fn missing_receipt_applies_exactly_once() {
        let (pool, _dir) = admitted_pool().await;
        let id = op_id("4");
        assert_eq!(
            recover(&pool, &id, false).await.unwrap(),
            RecoveryDecision::ApplyOnce
        );

        let begun = match begin_operation(&pool, &id, CONSUMER_COMPUTE_RUN, "run-4", "fp-4")
            .await
            .unwrap()
        {
            BeginOutcome::Acquired(receipt) => receipt,
            BeginOutcome::Existing(receipt) => {
                panic!("a missing receipt must be acquired, got existing {receipt:?}")
            }
        };
        assert_eq!(
            begun.status, STATUS_RUNNING,
            "running is written before the effect"
        );
        assert_eq!(begun.result_json, None);
        assert_eq!(begun.error_json, None);

        let settled = settle_operation(&pool, &id, STATUS_FINISHED, "{\"ok\":true}")
            .await
            .unwrap();
        assert_eq!(settled.status, STATUS_FINISHED);
        assert_eq!(settled.result_json.as_deref(), Some("{\"ok\":true}"));
        assert_eq!(settled.error_json, None);

        // A second "first" attempt is answered by the receipt, not re-applied.
        assert!(matches!(
            recover(&pool, &id, false).await.unwrap(),
            RecoveryDecision::AnswerFromReceipt(_)
        ));
    }

    /// Terminal is never downgraded and never rewritten: a late settlement and
    /// a late `running` write both return the stored terminal receipt
    /// unchanged.
    #[tokio::test]
    async fn terminal_receipt_is_never_downgraded() {
        let (pool, _dir) = admitted_pool().await;
        let id = op_id("5");
        begin_acquired(&pool, &id, CONSUMER_COMPUTE_RUN, "run-5", "fp-5").await;
        let finished = settle_operation(&pool, &id, STATUS_FINISHED, "{\"v\":1}")
            .await
            .unwrap();
        assert_eq!(finished.status, STATUS_FINISHED);

        // A late, conflicting terminal settlement loses to the first terminal.
        let late = settle_operation(&pool, &id, STATUS_FAILED, "{\"v\":2}")
            .await
            .unwrap();
        assert_eq!(late.status, STATUS_FINISHED, "first terminal wins");
        assert_eq!(late.result_json.as_deref(), Some("{\"v\":1}"));
        assert_eq!(late.error_json, None);
        assert_eq!(late.terminal_at, finished.terminal_at);

        // A late `running` write (the crash-recovery shape) cannot resurrect
        // it, and it does not re-acquire the effect either.
        let late_begin = match begin_operation(&pool, &id, CONSUMER_COMPUTE_RUN, "run-5", "fp-5")
            .await
            .unwrap()
        {
            BeginOutcome::Existing(receipt) => receipt,
            BeginOutcome::Acquired(receipt) => {
                panic!("a settled operation must not be re-acquired: {receipt:?}")
            }
        };
        assert_eq!(late_begin.status, STATUS_FINISHED);
        assert_eq!(late_begin.result_json.as_deref(), Some("{\"v\":1}"));

        let stored = get_operation_receipt(&pool, &id).await.unwrap().unwrap();
        assert_eq!(stored.status, STATUS_FINISHED);
        assert_eq!(stored.result_json.as_deref(), Some("{\"v\":1}"));
        assert!(stored.terminal_at.is_some());
    }

    /// First-writer-wins: a different fingerprint for an owned operation id is
    /// the typed `operation_id_conflict`, and the first writer's row survives.
    #[tokio::test]
    async fn conflicting_fingerprint_is_a_typed_operation_id_conflict() {
        let (pool, _dir) = admitted_pool().await;
        let id = op_id("6");
        let first = begin_acquired(&pool, &id, CONSUMER_COMPUTE_RUN, "run-6", "fp-first").await;
        let err = begin_operation(&pool, &id, CONSUMER_COMPUTE_RUN, "run-6b", "fp-second")
            .await
            .expect_err("a different fingerprint must be refused");
        assert!(
            matches!(&err, LocalDbError::OperationIdConflict { operation_id } if operation_id == &id),
            "got {err:?}"
        );
        // The refusal is about the fingerprint, not a generic storage fault,
        // and it is never a silent dedupe: the first writer's row is intact.
        let stored = get_operation_receipt(&pool, &id).await.unwrap().unwrap();
        assert_eq!(stored.request_fingerprint, "fp-first");
        assert_eq!(stored.subject_id, "run-6");
        assert_eq!(stored.status, first.status);
        assert_eq!(stored.sequence, first.sequence);
    }

    /// §B.2's terminal-payload contract: every terminal settlement carries
    /// exactly one of `result_json` / `error_json` — including `cancelled` and
    /// `interrupted`, whose reason IS the payload. A non-terminal (or unknown)
    /// settlement status is refused and leaves the receipt running.
    #[tokio::test]
    async fn every_terminal_settlement_carries_exactly_one_payload() {
        let (pool, _dir) = admitted_pool().await;
        for (suffix, status) in [
            ("7a", STATUS_FINISHED),
            ("7b", STATUS_FAILED),
            ("7c", STATUS_CANCELLED),
            ("7d", STATUS_INTERRUPTED),
        ] {
            let id = op_id(suffix);
            begin_acquired(&pool, &id, CONSUMER_COMPUTE_RUN, "run-7", "fp-7").await;
            let settled = settle_operation(&pool, &id, status, "{\"x\":1}")
                .await
                .unwrap();
            assert_eq!(settled.status, status);
            assert!(settled.terminal_at.is_some());
            assert!(
                settled.result_json.is_some() ^ settled.error_json.is_some(),
                "exactly one terminal payload, got {settled:?}"
            );
            if status == STATUS_FINISHED {
                assert_eq!(settled.result_json.as_deref(), Some("{\"x\":1}"));
            } else {
                assert_eq!(settled.error_json.as_deref(), Some("{\"x\":1}"));
            }
        }

        for (suffix, status) in [("7e", STATUS_RUNNING), ("7f", "not_a_status")] {
            let id = op_id(suffix);
            begin_acquired(&pool, &id, CONSUMER_COMPUTE_RUN, "run-7", "fp-7").await;
            let err = settle_operation(&pool, &id, status, "{\"x\":1}")
                .await
                .expect_err("a non-terminal settlement status must be refused");
            assert!(
                matches!(err, LocalDbError::ValidationError(_)),
                "got {err:?}"
            );
            assert_eq!(
                get_operation_receipt(&pool, &id)
                    .await
                    .unwrap()
                    .unwrap()
                    .status,
                STATUS_RUNNING,
                "a refused settlement leaves the receipt running"
            );
        }

        // Settling a receipt that was never begun is a caller bug, not a
        // fabricated receipt.
        let err = settle_operation(&pool, &op_id("7f0"), STATUS_CANCELLED, "{\"x\":1}")
            .await
            .expect_err("settle without begin must fail");
        assert!(
            matches!(err, LocalDbError::Sqlx(sqlx::Error::RowNotFound)),
            "got {err:?}"
        );
    }

    /// An unknown consumer is refused before any row is written.
    #[tokio::test]
    async fn unknown_consumer_is_refused() {
        let (pool, _dir) = admitted_pool().await;
        let err = begin_operation(&pool, &op_id("8"), "not_a_consumer", "s", "fp-8")
            .await
            .expect_err("unknown consumer must be refused");
        assert!(
            matches!(err, LocalDbError::ValidationError(_)),
            "got {err:?}"
        );
        assert!(get_operation_receipt(&pool, &op_id("8"))
            .await
            .unwrap()
            .is_none());
    }

    /// The schema is the durable home of §B.2's receipt invariant: the CHECK
    /// requires a terminal stamp plus exactly one terminal payload, and a
    /// settled receipt is IMMUTABLE — no admitted write path can downgrade it,
    /// rewrite `finished` → `failed`, or replace its payload under the same
    /// status, so the answer a replay is served can never be rewritten.
    #[tokio::test]
    async fn schema_pins_terminal_payload_and_terminal_immutability() {
        let (pool, _dir) = admitted_pool().await;

        // Malformed terminal shapes: no payload, no terminal stamp, or two
        // payloads — all refused by the CHECK itself.
        for (suffix, result_json, error_json, terminal_at) in [
            ("ca", None, None, None),
            ("cb", None, None, Some("2026-10-07T00:00:01Z")),
            ("cc", Some("{}"), Some("{}"), Some("2026-10-07T00:00:01Z")),
        ] {
            let err = sqlx::query(
                "INSERT INTO operation_receipts \
                     (operation_id, consumer, subject_id, status, request_fingerprint, \
                      result_json, error_json, created_at, updated_at, terminal_at, sequence) \
                 VALUES (?, 'compute_run', 'run-check', 'finished', 'fp-check', \
                         ?, ?, '2026-10-07T00:00:00Z', '2026-10-07T00:00:00Z', ?, 1)",
            )
            .bind(op_id(suffix))
            .bind(result_json)
            .bind(error_json)
            .bind(terminal_at)
            .execute(&pool)
            .await
            .expect_err("a malformed terminal shape must be refused");
            assert!(
                err.as_database_error()
                    .is_some_and(|db| db.to_string().contains("CHECK")),
                "expected a CHECK refusal, got: {err}"
            );
        }

        // Every mutation of a settled receipt is refused.
        let id = op_id("cd");
        begin_acquired(&pool, &id, CONSUMER_COMPUTE_RUN, "run-check", "fp-cd").await;
        let settled = settle_operation(&pool, &id, STATUS_FINISHED, "{\"v\":1}")
            .await
            .unwrap();
        for sql in [
            "UPDATE operation_receipts SET status = 'running', result_json = NULL, \
             terminal_at = NULL WHERE operation_id = ?",
            "UPDATE operation_receipts SET status = 'failed', result_json = NULL, \
             error_json = '{\"v\":2}' WHERE operation_id = ?",
            "UPDATE operation_receipts SET result_json = '{\"v\":3}' WHERE operation_id = ?",
            "UPDATE operation_receipts SET updated_at = '2030-01-01T00:00:00Z' \
             WHERE operation_id = ?",
            "DELETE FROM operation_receipts WHERE operation_id = ?",
        ] {
            let err = sqlx::query(sql)
                .bind(&id)
                .execute(&pool)
                .await
                .expect_err("a settled receipt must be immutable");
            assert!(
                err.to_string()
                    .contains("OPERATION_RECEIPT_TERMINAL_IMMUTABLE"),
                "expected the terminal-immutability abort, got: {err}"
            );
        }

        // The stored row is byte-identical to the settlement.
        let stored = get_operation_receipt(&pool, &id).await.unwrap().unwrap();
        assert_eq!(stored, settled);
    }

    /// QC1-C001: a settled answer cannot be deleted-and-reinserted. An
    /// `INSERT OR REPLACE` over a terminal id resolves its conflict by DELETING
    /// the victim — whose DELETE trigger does not fire while
    /// `recursive_triggers` is OFF (the SQLite default) — so the DELETE-side
    /// immutability triggers alone cannot see it. The schema therefore refuses
    /// the replacement insert itself.
    #[tokio::test]
    async fn replacement_insert_cannot_overwrite_a_terminal_receipt() {
        let (pool, _dir) = admitted_pool().await;
        let id = op_id("e1");
        begin_acquired(&pool, &id, CONSUMER_COMPUTE_RUN, "run-e1", "fp-e1").await;
        let settled = settle_operation(&pool, &id, STATUS_FINISHED, r#"{"ok":true}"#)
            .await
            .expect("settle the receipt terminally");

        let err = sqlx::query(
            "INSERT OR REPLACE INTO operation_receipts \
                 (operation_id, consumer, subject_id, status, request_fingerprint, \
                  result_json, error_json, created_at, updated_at, terminal_at, sequence) \
             VALUES (?, 'compute_run', 'run-replaced', 'running', 'fp-replaced', \
                     NULL, NULL, '2026-10-07T00:00:00Z', '2026-10-07T00:00:00Z', NULL, 99)",
        )
        .bind(&id)
        .execute(&pool)
        .await
        .expect_err("a replacement insert over a terminal receipt must be refused");
        assert!(
            err.to_string()
                .contains("OPERATION_RECEIPT_TERMINAL_IMMUTABLE"),
            "expected the terminal-immutability abort, got: {err}"
        );

        // The settled row survived byte-identically (the victim was not
        // deleted, and nothing was reinserted).
        let stored = get_operation_receipt(&pool, &id).await.unwrap().unwrap();
        assert_eq!(stored, settled, "the settled row must be untouched");
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM operation_receipts")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 1, "no replacement row was created");
    }

    /// QC1-C001, the second half: the receipt KEY is immutable, so a `running`
    /// source cannot MOVE its row onto a settled id — an `UPDATE OR REPLACE`
    /// would otherwise delete the terminal victim without firing its DELETE
    /// trigger, and a plain `UPDATE … SET operation_id` would hijack the
    /// settled id's answer.
    #[tokio::test]
    async fn update_replace_cannot_hijack_a_terminal_receipt() {
        let (pool, _dir) = admitted_pool().await;
        let terminal_id = op_id("e2");
        begin_acquired(&pool, &terminal_id, CONSUMER_COMPUTE_RUN, "run-e2", "fp-e2").await;
        let settled = settle_operation(&pool, &terminal_id, STATUS_FINISHED, r#"{"ok":true}"#)
            .await
            .expect("settle the receipt terminally");
        let running_id = op_id("e3");
        let running =
            begin_acquired(&pool, &running_id, CONSUMER_COMPUTE_RUN, "run-e3", "fp-e3").await;
        assert_eq!(running.status, STATUS_RUNNING);

        // `UPDATE OR REPLACE` from the running source onto the settled id.
        let err = sqlx::query(
            "UPDATE OR REPLACE operation_receipts SET operation_id = ? WHERE operation_id = ?",
        )
        .bind(&terminal_id)
        .bind(&running_id)
        .execute(&pool)
        .await
        .expect_err("UPDATE OR REPLACE must not move a running row onto a settled id");
        assert!(
            err.to_string().contains("OPERATION_RECEIPT_KEY_IMMUTABLE"),
            "expected the key-immutability abort, got: {err}"
        );

        // A plain key mutation is refused the same way.
        let err =
            sqlx::query("UPDATE operation_receipts SET operation_id = ? WHERE operation_id = ?")
                .bind(&terminal_id)
                .bind(&running_id)
                .execute(&pool)
                .await
                .expect_err("a plain receipt key mutation must be refused");
        assert!(
            err.to_string().contains("OPERATION_RECEIPT_KEY_IMMUTABLE"),
            "expected the key-immutability abort, got: {err}"
        );

        // Both rows are byte-identical to their pre-attack state.
        assert_eq!(
            get_operation_receipt(&pool, &terminal_id)
                .await
                .unwrap()
                .unwrap(),
            settled
        );
        assert_eq!(
            get_operation_receipt(&pool, &running_id)
                .await
                .unwrap()
                .unwrap(),
            running
        );
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM operation_receipts")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 2, "the terminal victim was not deleted");
    }

    /// The legitimate replay path is untouched by the replacement guards: after
    /// settlement, `begin_operation` answers from the stored terminal row (the
    /// store reads first and never re-inserts), and a conflicting request is
    /// still the typed refusal.
    #[tokio::test]
    async fn replaying_a_terminal_receipt_never_reinserts_it() {
        let (pool, _dir) = admitted_pool().await;
        let id = op_id("e4");
        begin_acquired(&pool, &id, CONSUMER_COMPUTE_RUN, "run-e4", "fp-e4").await;
        let settled = settle_operation(&pool, &id, STATUS_FINISHED, r#"{"ok":true}"#)
            .await
            .expect("settle the receipt terminally");

        match begin_operation(&pool, &id, CONSUMER_COMPUTE_RUN, "run-e4", "fp-e4")
            .await
            .expect("a terminal replay is answered, never refused")
        {
            BeginOutcome::Existing(stored) => assert_eq!(stored, settled),
            BeginOutcome::Acquired(other) => {
                panic!("a terminal receipt must never be re-acquired: {other:?}")
            }
        }
        assert!(
            matches!(
                begin_operation(&pool, &id, CONSUMER_COMPUTE_RUN, "run-e4", "fp-other").await,
                Err(LocalDbError::OperationIdConflict { .. })
            ),
            "a different fingerprint for the settled id stays the typed conflict"
        );
        let stored = get_operation_receipt(&pool, &id).await.unwrap().unwrap();
        assert_eq!(stored, settled);
    }

    /// The receipt write path runs under the admitted writer protocol: an
    /// admitted writer (engine owner or cooperative direct) writes, while a raw
    /// handle that never received the protocol scalars is fenced by SQLite
    /// itself.
    #[tokio::test]
    async fn receipts_are_guarded_by_the_writer_protocol() {
        let (pool, dir) = admitted_pool().await;
        let id = op_id("9");
        begin_acquired(&pool, &id, CONSUMER_COMPUTE_RUN, "run-9", "fp-9").await;

        let db_path = dir.path().join("state.db");
        let raw = SqlitePool::connect(&format!("sqlite://{}?mode=rwc", db_path.display()))
            .await
            .expect("raw pool");
        let err = sqlx::query(
            "INSERT INTO operation_receipts \
                 (operation_id, consumer, subject_id, status, request_fingerprint, \
                  result_json, error_json, created_at, updated_at, terminal_at, sequence) \
             VALUES ('op_raw', 'compute_run', 'run-raw', 'running', 'fp-raw', \
                     NULL, NULL, '2026-10-07T00:00:00Z', '2026-10-07T00:00:00Z', NULL, 1)",
        )
        .execute(&raw)
        .await
        .expect_err("an unregistered writer must be fenced");
        let message = err.to_string();
        assert!(
            message.contains("WRITER_FENCED") || message.contains("no such function: nexus_writer"),
            "expected a writer fence, got: {message}"
        );
        raw.close().await;
    }

    /// The begin outcome is the exclusive-acquisition gate of the
    /// recover → begin → effect sequence: of two concurrent same-fingerprint
    /// callers exactly ONE is told it acquired the effect, and the loser is
    /// told to answer from the stored receipt instead of running it.
    #[tokio::test]
    async fn racing_begin_distinguishes_acquirer_from_loser() {
        let (pool, _dir) = admitted_pool().await;
        let id = op_id("ff");
        let (first, second) = tokio::join!(
            begin_operation(&pool, &id, CONSUMER_COMPUTE_RUN, "run-race", "fp-race"),
            begin_operation(&pool, &id, CONSUMER_COMPUTE_RUN, "run-race", "fp-race"),
        );
        let outcomes = [first.unwrap(), second.unwrap()];
        assert_eq!(
            outcomes
                .iter()
                .filter(|outcome| outcome.is_acquired())
                .count(),
            1,
            "exactly one racer acquires the effect: {outcomes:?}"
        );
        let acquired = outcomes
            .iter()
            .find(|outcome| outcome.is_acquired())
            .unwrap()
            .receipt()
            .clone();
        let existing = outcomes
            .iter()
            .find(|outcome| !outcome.is_acquired())
            .unwrap()
            .receipt()
            .clone();
        assert_eq!(acquired.operation_id, existing.operation_id);
        assert_eq!(acquired.sequence, existing.sequence);
        assert_eq!(acquired.created_at, existing.created_at);
        assert_eq!(acquired.status, STATUS_RUNNING);

        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM operation_receipts WHERE operation_id = ?")
                .bind(&id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count, 1, "the race converges on exactly one receipt");

        // A later replay is never an acquirer either.
        assert!(
            !begin_operation(&pool, &id, CONSUMER_COMPUTE_RUN, "run-race", "fp-race")
                .await
                .unwrap()
                .is_acquired()
        );
    }
}
