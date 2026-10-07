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
    /// receipt FIRST ([`begin_operation`]), run the effect, then
    /// [`settle_operation`] terminally.
    ApplyOnce,
    /// A `running` receipt whose owner is no longer live (crash/restart
    /// ambiguity) on a write with no terminal receipt. Spec §B.3 item 4: the
    /// caller gets a typed uncertain/blocked answer — **never** a blind
    /// retry, and the store never fabricates a terminal for it.
    Uncertain(OperationReceipt),
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

/// Write the `running` receipt for an operation **before** its effect runs
/// (spec §B.3 step 3), and return the stored row.
///
/// First-writer-wins on `operation_id`:
///
/// - no receipt exists ⇒ insert `running` and return it;
/// - a receipt exists with the same `request_fingerprint` ⇒ replay: return the
///   stored row unchanged (whatever status it holds — a terminal row stays
///   terminal);
/// - a receipt exists with a **different** fingerprint ⇒
///   [`LocalDbError::OperationIdConflict`], the typed `operation_id_conflict`
///   refusal. The existing row is left untouched.
///
/// The insert is `ON CONFLICT DO NOTHING` followed by a read, so two racers
/// converge on exactly one stored row.
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
) -> Result<OperationReceipt, LocalDbError> {
    if !is_known_consumer(consumer) {
        return Err(LocalDbError::ValidationError(format!(
            "unknown operation receipt consumer '{consumer}'"
        )));
    }
    let now = now_rfc3339();
    sqlx::query(
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

    let stored = get_operation_receipt(pool, operation_id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    if !stored.matches_fingerprint(request_fingerprint) {
        return Err(LocalDbError::OperationIdConflict {
            operation_id: operation_id.to_string(),
        });
    }
    Ok(stored)
}

/// Settle a receipt terminally, and return the stored row.
///
/// `status` must be terminal. The payload shape is enforced here and by the
/// table CHECK: `finished` requires `Some(result)` (the result payload),
/// `failed` requires `Some(error)` (the error payload), and `cancelled` /
/// `interrupted` require `None` (payload-free terminals).
///
/// Only a `running` row is settled. A receipt that is already terminal is
/// **never downgraded** and never re-settled: a late or racing settlement
/// returns the stored terminal row unchanged (first terminal wins).
///
/// # Errors
///
/// [`LocalDbError::ValidationError`] for a non-terminal `status` or a payload
/// that does not match it, [`LocalDbError::Sqlx`] (with
/// [`sqlx::Error::RowNotFound`]) when no receipt has been begun for
/// `operation_id`, and [`LocalDbError::Sqlx`] on database failure.
pub async fn settle_operation(
    pool: &SqlitePool,
    operation_id: &str,
    status: &str,
    payload: Option<&str>,
) -> Result<OperationReceipt, LocalDbError> {
    let (result_json, error_json) = match (status, payload) {
        (STATUS_FINISHED, Some(payload)) => (Some(payload), None),
        (STATUS_FAILED, Some(payload)) => (None, Some(payload)),
        (STATUS_CANCELLED | STATUS_INTERRUPTED, None) => (None, None),
        (STATUS_FINISHED | STATUS_FAILED, None) => {
            return Err(LocalDbError::ValidationError(format!(
                "settling operation '{operation_id}' as '{status}' requires its terminal payload"
            )));
        }
        (STATUS_CANCELLED | STATUS_INTERRUPTED, Some(_)) => {
            return Err(LocalDbError::ValidationError(format!(
                "settling operation '{operation_id}' as '{status}' takes no terminal payload"
            )));
        }
        (other, _) => {
            return Err(LocalDbError::ValidationError(format!(
                "operation '{operation_id}' cannot be settled as '{other}'"
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

/// Map a `compute_sessions` direct-lane run status onto the receipt status it
/// shadows (spec §B.2), or `None` when the transition has no receipt
/// counterpart.
///
/// The shadowed operation transitions are `running → succeeded/failed`.
/// `applied` / `discarded` are review decisions on an already-settled run, so
/// Accept/Discard stay behavior-preserving and never touch a receipt — that is
/// why the `compute_sessions` schema needs no change for receipts.
#[must_use]
pub fn compute_run_receipt_status(run_status: &str) -> Option<&'static str> {
    match run_status {
        "running" => Some(STATUS_RUNNING),
        "succeeded" => Some(STATUS_FINISHED),
        "failed" => Some(STATUS_FAILED),
        _ => None,
    }
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
        // production receipts.
        let hex = format!("{suffix:0>32}")
            .chars()
            .map(|c| {
                if c.is_ascii_hexdigit() && !c.is_ascii_uppercase() {
                    c
                } else {
                    'a'
                }
            })
            .collect::<String>();
        format!("op_{}", &hex[..32])
    }

    /// Crash: a `running` receipt whose owner died is ambiguous — the recovery
    /// answer is `uncertain`, never `ApplyOnce`, so the effect is not applied
    /// again.
    #[tokio::test]
    async fn crashed_running_receipt_is_uncertain_never_reapplied() {
        let (pool, _dir) = admitted_pool().await;
        let id = op_id("1");
        let begun = begin_operation(&pool, &id, CONSUMER_COMPUTE_RUN, "run-1", "fp-1")
            .await
            .unwrap();
        assert_eq!(begun.status, STATUS_RUNNING);
        assert!(begun.terminal_at.is_none());

        // The owner process is gone: replay must not re-apply.
        let decision = recover(&pool, &id, false).await.unwrap();
        match decision {
            RecoveryDecision::Uncertain(receipt) => assert_eq!(receipt.status, STATUS_RUNNING),
            other => panic!("crash ambiguity must be Uncertain, got {other:?}"),
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
        begin_operation(&pool, &id, CONSUMER_COMPUTE_RUN, "run-2", "fp-2")
            .await
            .unwrap();
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
        begin_operation(
            &pool,
            &id,
            CONSUMER_CONNECT_INVOKE,
            "peer-1/tools.nexus.x",
            "fp-3",
        )
        .await
        .unwrap();
        let settled = settle_operation(&pool, &id, STATUS_CANCELLED, None)
            .await
            .unwrap();
        assert_eq!(settled.status, STATUS_CANCELLED);
        assert!(settled.terminal_at.is_some());

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
        let replayed = begin_operation(
            &pool,
            &id,
            CONSUMER_CONNECT_INVOKE,
            "peer-1/tools.nexus.x",
            "fp-3",
        )
        .await
        .unwrap();
        assert_eq!(replayed.status, STATUS_CANCELLED);
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

        let begun = begin_operation(&pool, &id, CONSUMER_COMPUTE_RUN, "run-4", "fp-4")
            .await
            .unwrap();
        assert_eq!(
            begun.status, STATUS_RUNNING,
            "running is written before the effect"
        );
        assert_eq!(begun.result_json, None);
        assert_eq!(begun.error_json, None);

        let settled = settle_operation(&pool, &id, STATUS_FINISHED, Some("{\"ok\":true}"))
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

    /// Terminal is never downgraded: a late settlement and a late `running`
    /// write both return the stored terminal receipt unchanged.
    #[tokio::test]
    async fn terminal_receipt_is_never_downgraded() {
        let (pool, _dir) = admitted_pool().await;
        let id = op_id("5");
        begin_operation(&pool, &id, CONSUMER_COMPUTE_RUN, "run-5", "fp-5")
            .await
            .unwrap();
        let finished = settle_operation(&pool, &id, STATUS_FINISHED, Some("{\"v\":1}"))
            .await
            .unwrap();
        assert_eq!(finished.status, STATUS_FINISHED);

        // A late, conflicting terminal settlement loses to the first terminal.
        let late = settle_operation(&pool, &id, STATUS_FAILED, Some("{\"v\":2}"))
            .await
            .unwrap();
        assert_eq!(late.status, STATUS_FINISHED, "first terminal wins");
        assert_eq!(late.result_json.as_deref(), Some("{\"v\":1}"));
        assert_eq!(late.error_json, None);
        assert_eq!(late.terminal_at, finished.terminal_at);

        // A late `running` write (the crash-recovery shape) cannot resurrect it.
        let late_begin = begin_operation(&pool, &id, CONSUMER_COMPUTE_RUN, "run-5", "fp-5")
            .await
            .unwrap();
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
        let first = begin_operation(&pool, &id, CONSUMER_COMPUTE_RUN, "run-6", "fp-first")
            .await
            .unwrap();
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

    /// The payload invariant spec §B.2 states for terminal settlement.
    #[tokio::test]
    async fn terminal_settlement_payload_shape_is_enforced() {
        let (pool, _dir) = admitted_pool().await;
        for (suffix, status, payload) in [
            ("7a", STATUS_FINISHED, None),
            ("7b", STATUS_FAILED, None),
            ("7c", STATUS_CANCELLED, Some("{\"x\":1}")),
            ("7d", STATUS_INTERRUPTED, Some("{\"x\":1}")),
            ("7e", STATUS_RUNNING, None),
        ] {
            let id = op_id(suffix);
            begin_operation(&pool, &id, CONSUMER_COMPUTE_RUN, "run-7", "fp-7")
                .await
                .unwrap();
            let err = settle_operation(&pool, &id, status, payload)
                .await
                .expect_err("the payload/status combination must be refused");
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
        let err = settle_operation(&pool, &op_id("7f"), STATUS_CANCELLED, None)
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

    /// Compute Run receipts shadow `running → succeeded/failed`; Accept and
    /// Discard have no receipt counterpart, which is what keeps them
    /// behavior-preserving without a `compute_sessions` schema change.
    #[tokio::test]
    async fn compute_run_shadow_mapping_leaves_review_states_alone() {
        assert_eq!(compute_run_receipt_status("running"), Some(STATUS_RUNNING));
        assert_eq!(
            compute_run_receipt_status("succeeded"),
            Some(STATUS_FINISHED)
        );
        assert_eq!(compute_run_receipt_status("failed"), Some(STATUS_FAILED));
        assert_eq!(compute_run_receipt_status("applied"), None);
        assert_eq!(compute_run_receipt_status("discarded"), None);
    }

    /// The schema is the durable home of §B.2's receipt invariant: the table
    /// CHECK refuses a terminal shape without its terminal stamp, and the
    /// no-downgrade trigger refuses a settled receipt being put back to
    /// `running` even by a direct SQL write.
    #[tokio::test]
    async fn schema_refuses_malformed_and_downgraded_receipt_shapes() {
        let (pool, _dir) = admitted_pool().await;

        // `finished` without its result payload or terminal stamp: refused by
        // the schema, independently of the store's own validation.
        let err = sqlx::query(
            "INSERT INTO operation_receipts \
                 (operation_id, consumer, subject_id, status, request_fingerprint, \
                  result_json, error_json, created_at, updated_at, terminal_at, sequence) \
             VALUES (?, 'compute_run', 'run-check', 'finished', 'fp-check', \
                     NULL, NULL, '2026-10-07T00:00:00Z', '2026-10-07T00:00:00Z', NULL, 1)",
        )
        .bind(op_id("ca"))
        .execute(&pool)
        .await
        .expect_err("a payload-less finished receipt must be refused");
        assert!(
            err.as_database_error()
                .is_some_and(|db| db.to_string().contains("CHECK")),
            "expected a CHECK refusal, got: {err}"
        );

        // A settled receipt cannot be downgraded back to `running`.
        let id = op_id("cb");
        begin_operation(&pool, &id, CONSUMER_COMPUTE_RUN, "run-check", "fp-cb")
            .await
            .unwrap();
        settle_operation(&pool, &id, STATUS_FINISHED, Some("{}"))
            .await
            .unwrap();
        let err = sqlx::query(
            "UPDATE operation_receipts SET status = 'running', result_json = NULL, terminal_at = NULL \
              WHERE operation_id = ?",
        )
        .bind(&id)
        .execute(&pool)
        .await
        .expect_err("a terminal receipt must not be downgradable");
        assert!(
            err.to_string()
                .contains("OPERATION_RECEIPT_TERMINAL_DOWNGRADE"),
            "expected the no-downgrade abort, got: {err}"
        );
        let stored = get_operation_receipt(&pool, &id).await.unwrap().unwrap();
        assert_eq!(stored.status, STATUS_FINISHED);
        assert_eq!(stored.result_json.as_deref(), Some("{}"));
        assert!(stored.terminal_at.is_some());
    }

    /// The receipt write path runs under the engine-owned writer protocol: the
    /// engine owner writes, while a raw handle that never received the protocol
    /// scalars is fenced by SQLite itself.
    #[tokio::test]
    async fn receipts_are_guarded_by_the_writer_protocol() {
        let (pool, dir) = admitted_pool().await;
        let id = op_id("9");
        begin_operation(&pool, &id, CONSUMER_COMPUTE_RUN, "run-9", "fp-9")
            .await
            .expect("the engine owner is admitted to the receipt table");

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
}
