//! Bounded fixture fault points and the controllable clock (C7 control seam).
//!
//! The conformance suite needs to observe what a client sees when a mutation's
//! response is lost, when the process stops mid-transaction, and when an
//! admission is paused behind a competing writer. Those are operator/test
//! controls, never provider authority: the state below lives only in process
//! memory, is consumed once, and is never read back from durable storage.
//!
//! Every holding point is bounded. A gate that is never released by the test
//! expires after [`FAULT_HOLD_TIMEOUT`] (or the bound the test supplied) and the
//! operation fails without committing anything, so no test can hang a suite.

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use spoke_operations::{SpokeReject, SpokeRejectCode};
use tokio::sync::oneshot;

/// One bounded observation point in the fixture's mutation path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaultPoint {
    /// The effect, receipt, dedup row and target are staged inside the caller's
    /// transaction and not committed: the abort must leave nothing visible.
    BeforeCommit,
    /// The transaction committed and the response has not been returned: the
    /// effect is durable and only replay/`receipts.get` can resolve it.
    AfterCommitBeforeResponse,
    /// Before the writer transaction, so a competing identical admission can
    /// commit while this one is paused; the paused admission then replays it.
    AdmissionPause,
    /// The committed response is held until the caller drops the connection.
    ResponseDrop,
}

/// Default bound for a held fault point. Tests that want a shorter bound arm the
/// gate with [`FixtureFaults::hold_next_bounded`].
pub const FAULT_HOLD_TIMEOUT: Duration = Duration::from_secs(5);

/// The four fault points' test-side state. One point is armed at a time; the
/// channels exist only for holding points.
struct FaultState {
    armed: Option<FaultPoint>,
    /// `true` for a holding point, `false` for a one-shot fail point, so the
    /// same arm slot cannot be consumed by the wrong observation.
    holding: bool,
    reached: Option<oneshot::Sender<()>>,
    release: Option<oneshot::Receiver<()>>,
    bound: Duration,
}

impl Default for FaultState {
    fn default() -> Self {
        Self {
            armed: None,
            holding: false,
            reached: None,
            release: None,
            bound: FAULT_HOLD_TIMEOUT,
        }
    }
}

/// Operator/test-only fault control attached to one provider instance.
///
/// Cloning the handle shares the same state, so a test can arm a point and the
/// provider observes it without any process-global registry. Dropping the
/// provider drops the control with it: nothing survives a restart.
#[derive(Clone)]
pub struct FixtureFaults {
    inner: Arc<Mutex<FaultState>>,
}

impl FixtureFaults {
    /// A fresh, disarmed control attached to one provider.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(FaultState::default())),
        }
    }

    fn state(&self) -> MutexGuard<'_, FaultState> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Arm a one-shot fail point (`BeforeCommit` or
    /// `AfterCommitBeforeResponse`). The next operation that reaches `point`
    /// fails through the protocol-defined uncertainty path; later operations
    /// proceed.
    pub fn fail_next(&self, point: FaultPoint) {
        let mut state = self.state();
        state.armed = Some(point);
        state.holding = false;
        state.reached = None;
        state.release = None;
    }

    /// Arm a holding point (`BeforeCommit`, `AdmissionPause` or `ResponseDrop`)
    /// with the default bound and return the test handle.
    pub fn hold_next(&self, point: FaultPoint) -> FaultHold {
        self.hold_next_bounded(point, FAULT_HOLD_TIMEOUT)
    }

    /// Arm a holding point (`BeforeCommit`, `AdmissionPause` or `ResponseDrop`)
    /// bounded by `bound` and return the test handle.
    pub fn hold_next_bounded(&self, point: FaultPoint, bound: Duration) -> FaultHold {
        let (reached_tx, reached_rx) = oneshot::channel();
        let (release_tx, release_rx) = oneshot::channel();
        let mut state = self.state();
        state.armed = Some(point);
        state.holding = true;
        state.reached = Some(reached_tx);
        state.release = Some(release_rx);
        state.bound = bound;
        FaultHold {
            reached: reached_rx,
            release: Some(release_tx),
        }
    }

    /// Consume a one-shot fail point (`BeforeCommit`,
    /// `AfterCommitBeforeResponse`). Crate-private: only the provider's own
    /// mutation path may trip a point.
    pub(crate) fn trip(&self, point: FaultPoint) -> bool {
        let mut state = self.state();
        if state.armed == Some(point) && !state.holding {
            state.armed = None;
            return true;
        }
        false
    }

    /// Block a holding point (`AdmissionPause`, `ResponseDrop`) until the test
    /// releases it, or fail the operation when the bound elapses. A point that
    /// is not armed is not reached and returns immediately.
    pub(crate) async fn hold(&self, point: FaultPoint) -> Result<(), SpokeReject> {
        let gate = {
            let mut state = self.state();
            if state.armed != Some(point) || !state.holding {
                None
            } else {
                state.armed = None;
                Some((
                    state.reached.take(),
                    state.release.take(),
                    state.bound,
                ))
            }
        };
        let Some((Some(reached), Some(release), bound)) = gate else {
            return Ok(());
        };
        let _ = reached.send(());
        match tokio::time::timeout(bound, release).await {
            Ok(Ok(())) => Ok(()),
            _ => Err(fault_reject(point, bound)),
        }
    }
}

impl Default for FixtureFaults {
    fn default() -> Self {
        Self::new()
    }
}

/// The failure a bounded hold reports when the test never releases it. It is an
/// internal failure with no committed effect, so §14.2's retryable class is
/// honest: nothing this operation staged reached the file.
fn fault_reject(point: FaultPoint, bound: Duration) -> SpokeReject {
    SpokeReject {
        code: SpokeRejectCode::InternalError,
        message: format!(
            "fixture fault hold at {point:?} expired after {}ms without a release",
            bound.as_millis()
        ),
        details: None,
    }
}

/// The test-side handle for one armed holding point.
pub struct FaultHold {
    reached: oneshot::Receiver<()>,
    release: Option<oneshot::Sender<()>>,
}

impl FaultHold {
    /// Wait until the provider reached the armed point. The caller bounds this
    /// wait itself when it is about to tear a connection down instead of
    /// releasing the gate.
    pub async fn wait_reached(&mut self) {
        let _ = (&mut self.reached).await;
    }

    /// Let the held operation continue.
    pub fn release(&mut self) {
        if let Some(release) = self.release.take() {
            let _ = release.send(());
        }
    }
}

/// The operator/test clock: every fixture operation evaluates exactly one
/// caller-supplied instant, so tests move time explicitly instead of waiting.
#[derive(Debug, Clone, Copy)]
pub struct FixtureClock {
    at: DateTime<Utc>,
}

impl FixtureClock {
    /// A clock pinned to `at`.
    #[must_use]
    pub fn at(at: DateTime<Utc>) -> Self {
        Self { at }
    }

    /// The instant the next operation is served at.
    #[must_use]
    pub fn now(self) -> DateTime<Utc> {
        self.at
    }

    /// The same clock advanced by `delta`, for the next sampled operation.
    #[must_use]
    pub fn advanced(self, delta: ChronoDuration) -> Self {
        Self {
            at: self.at + delta,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use serde_json::json;
    use spoke_operations::spoke_schemas::{
        MemoryCaptureRequest, MemoryGrantsGrantRequest, MemoryReceipt, MemorySubjectsEnsureRequest,
    };
    use sqlx::Row;
    use tempfile::TempDir;

    use crate::candidates::{admit_capture, Admitted};
    use crate::FixtureMemoryProvider;

    const OPERATOR: &str = "principal-operator";
    const CLIENT: &str = "principal-client";
    const SUBJECT: &str = "subject-alpha";
    const CONTEXT: &str = "bnd-alpha";

    struct Scenario {
        _home: TempDir,
        path: PathBuf,
        provider: FixtureMemoryProvider,
        space_id: String,
    }

    impl Scenario {
        async fn open() -> Self {
            let home = tempfile::tempdir().expect("scenario home");
            let path = home.path().join("faults.sqlite");
            let provider = FixtureMemoryProvider::open(&path)
                .await
                .expect("fixture opens");
            let space_id = provider.identity().space_id.clone();
            provider
                .bootstrap_operator_admin(OPERATOR)
                .await
                .expect("operator bootstrap");
            Self {
                _home: home,
                path,
                provider,
                space_id,
            }
        }

        /// Connection teardown without a clean shutdown: dropping the provider
        /// drops every pooled connection, then the same file is served by a new
        /// provider instance.
        async fn reopen_after_loss(self) -> Self {
            let Self {
                _home,
                path,
                provider,
                space_id,
            } = self;
            drop(provider);
            let provider = FixtureMemoryProvider::open(&path)
                .await
                .expect("fixture reopens the committed file");
            Self {
                _home,
                path,
                provider,
                space_id,
            }
        }

        async fn subject_and_capture_grant(&self, now: DateTime<Utc>) {
            let ensure: MemorySubjectsEnsureRequest = serde_json::from_value(json!({
                "space_id": self.space_id,
                "subject_id": SUBJECT,
            }))
            .expect("valid subjects.ensure request");
            self.provider
                .subjects_ensure(OPERATOR, &ensure, now)
                .await
                .expect("subject ensured");
            let grant: MemoryGrantsGrantRequest = serde_json::from_value(json!({
                "space_id": self.space_id,
                "principal_id": CLIENT,
                "subject_id": SUBJECT,
                "capabilities": ["capture"],
                "idempotency_key": "grant-client-capture",
            }))
            .expect("valid grants.grant request");
            self.provider
                .grants_grant(OPERATOR, &grant, now)
                .await
                .expect("capture grant issued");
        }
    }

    fn capture_request(
        space: &str,
        text: &str,
        key: &str,
    ) -> Admitted<MemoryCaptureRequest> {
        admit_capture(json!({
            "space_id": space,
            "subject_id": SUBJECT,
            "content": { "text": text },
            "capture_scope": { "visibility": "context", "context_id": CONTEXT },
            "kind_hint": "fact",
            "idempotency_key": key,
        }))
        .expect("valid capture request")
    }

    fn receipt_status(receipt: &MemoryReceipt) -> &'static str {
        match receipt {
            MemoryReceipt::CommittedReceipt(_) => "committed",
            MemoryReceipt::PendingReceipt(_) => "pending",
            MemoryReceipt::FailedReceipt(_) => "failed",
        }
    }

    fn receipt_id_of(receipt: &MemoryReceipt) -> &str {
        match receipt {
            MemoryReceipt::CommittedReceipt(receipt) => receipt.receipt_id.as_str(),
            MemoryReceipt::PendingReceipt(receipt) => receipt.receipt_id.as_str(),
            MemoryReceipt::FailedReceipt(receipt) => receipt.receipt_id.as_str(),
        }
    }

    fn refusal<T: std::fmt::Debug>(result: Result<T, SpokeReject>) -> SpokeReject {
        match result {
            Ok(value) => panic!("expected a protocol reject, observed {value:?}"),
            Err(reject) => reject,
        }
    }

    async fn count_rows(scenario: &Scenario, table: &str) -> i64 {
        let sql = match table {
            "candidates" => "SELECT COUNT(*) AS total FROM candidates",
            "records" => "SELECT COUNT(*) AS total FROM records",
            "receipts" => "SELECT COUNT(*) AS total FROM receipts",
            "receipt_targets" => "SELECT COUNT(*) AS total FROM receipt_targets",
            "idempotency_namespace" => "SELECT COUNT(*) AS total FROM idempotency_namespace",
            other => panic!("unknown table {other}"),
        };
        let row = sqlx::query(sql)
            .fetch_one(scenario.provider.store().pool())
            .await
            .expect("count query");
        row.get("total")
    }

    async fn marker(scenario: &Scenario) -> i64 {
        sqlx::query("SELECT marker FROM provider_snapshot WHERE space_id = ?")
            .bind(&scenario.space_id)
            .fetch_one(scenario.provider.store().pool())
            .await
            .expect("snapshot marker")
            .get("marker")
    }

    #[tokio::test]
    async fn c_023_response_loss_replays_one_effect() {
        let scenario = Scenario::open().await;
        let clock = FixtureClock::at(
            DateTime::parse_from_rfc3339("2026-11-04T00:00:00Z")
                .expect("a valid instant")
                .with_timezone(&Utc),
        );
        let now = clock.now();
        scenario.subject_and_capture_grant(now).await;
        let request = capture_request(
            &scenario.space_id,
            "The caller never sees this capture's answer.",
            "loss-1",
        );

        // The response is held after the commit and the caller tears the
        // connection down instead of waiting for it: no receipt is returned and
        // the operation future is cancelled.
        let mut hold = scenario.provider.faults().hold_next(FaultPoint::ResponseDrop);
        let mut operation = Box::pin(scenario.provider.capture(CLIENT, &request, now));
        tokio::select! {
            () = hold.wait_reached() => {}
            result = operation.as_mut() => {
                panic!("the response was not dropped: {result:?}")
            }
        }
        drop(operation);

        // The commit happened before the hold, so the effect and its receipt are
        // on disk even though the response never reached the caller.
        let stored_receipt: String = sqlx::query("SELECT receipt_id FROM receipts")
            .fetch_one(scenario.provider.store().pool())
            .await
            .expect("the committed receipt is durable")
            .get("receipt_id");

        // Actual connection teardown, then the same file is served again.
        let scenario = scenario.reopen_after_loss().await;
        assert_eq!(count_rows(&scenario, "candidates").await, 1);
        assert_eq!(count_rows(&scenario, "receipts").await, 1);
        assert_eq!(count_rows(&scenario, "receipt_targets").await, 1);

        // Same key, same payload: the original receipt, never a second effect.
        let replayed = scenario
            .provider
            .capture(CLIENT, &request, now)
            .await
            .expect("the lost response resolves through replay");
        assert_eq!(receipt_status(&replayed), "committed");
        assert_eq!(receipt_id_of(&replayed), stored_receipt);
        assert_eq!(count_rows(&scenario, "candidates").await, 1);

        // A second replay answers the same receipt again.
        let again = scenario
            .provider
            .capture(CLIENT, &request, now)
            .await
            .expect("the replay stays stable");
        assert_eq!(receipt_id_of(&again), stored_receipt);
        assert_eq!(count_rows(&scenario, "candidates").await, 1);
    }

    #[tokio::test]
    async fn c_024_midcommit_crash_resolves_uncertain_receipt() {
        let scenario = Scenario::open().await;
        let clock = FixtureClock::at(
            DateTime::parse_from_rfc3339("2026-11-04T00:00:00Z")
                .expect("a valid instant")
                .with_timezone(&Utc),
        );
        let now = clock.now();
        scenario.subject_and_capture_grant(now).await;
        let marker_before = marker(&scenario).await;

        // Phase 0 — cancellation mid-commit. The capture is staged inside an
        // open transaction and the caller drops the future; every connection is
        // then torn down. The same file must hold nothing it staged.
        let mid_commit = capture_request(
            &scenario.space_id,
            "Staged inside an open transaction.",
            "crash-mid-commit",
        );
        let mut hold = scenario.provider.faults().hold_next(FaultPoint::BeforeCommit);
        let mut operation = Box::pin(scenario.provider.capture(CLIENT, &mid_commit, now));
        tokio::select! {
            () = hold.wait_reached() => {}
            result = operation.as_mut() => {
                panic!("the pre-commit point was not reached: {result:?}")
            }
        }
        drop(operation);
        // Let the dropped transaction's rollback run before the pool closes.
        tokio::task::yield_now().await;
        let scenario = scenario.reopen_after_loss().await;
        for table in [
            "candidates",
            "records",
            "receipts",
            "receipt_targets",
            "idempotency_namespace",
        ] {
            assert_eq!(
                count_rows(&scenario, table).await,
                0,
                "{table} survived the mid-commit cancellation"
            );
        }
        assert_eq!(marker(&scenario).await, marker_before);

        // Phase 1 — loss before commit. The staged transaction is rolled back,
        // so not even the snapshot marker moved.
        let before_commit = capture_request(
            &scenario.space_id,
            "Staged but never committed.",
            "crash-before",
        );
        scenario.provider.faults().fail_next(FaultPoint::BeforeCommit);
        let rejected = refusal(
            scenario
                .provider
                .capture(CLIENT, &before_commit, now)
                .await,
        );
        assert_eq!(rejected.code, SpokeRejectCode::OutcomeUncertain);
        for table in [
            "candidates",
            "records",
            "receipts",
            "receipt_targets",
            "idempotency_namespace",
        ] {
            assert_eq!(count_rows(&scenario, table).await, 0, "{table} leaked");
        }
        assert_eq!(marker(&scenario).await, marker_before);

        // The rollback is a real file state, not an in-memory mask.
        let scenario = scenario.reopen_after_loss().await;
        for table in [
            "candidates",
            "records",
            "receipts",
            "receipt_targets",
            "idempotency_namespace",
        ] {
            assert_eq!(
                count_rows(&scenario, table).await,
                0,
                "{table} survived the reopen"
            );
        }
        assert_eq!(marker(&scenario).await, marker_before);

        // Same key resolves the uncertainty with exactly one committed effect.
        let committed = scenario
            .provider
            .capture(CLIENT, &before_commit, now)
            .await
            .expect("the retry after the crash commits once");
        assert_eq!(receipt_status(&committed), "committed");
        assert_eq!(count_rows(&scenario, "candidates").await, 1);
        assert_eq!(count_rows(&scenario, "receipts").await, 1);
        assert_eq!(count_rows(&scenario, "receipt_targets").await, 1);
        assert_eq!(count_rows(&scenario, "idempotency_namespace").await, 1);
        let replayed = scenario
            .provider
            .capture(CLIENT, &before_commit, now)
            .await
            .expect("the committed retry replays");
        assert_eq!(receipt_id_of(&replayed), receipt_id_of(&committed));
        assert_eq!(count_rows(&scenario, "candidates").await, 1);

        // Phase 2 — commit succeeded, response lost. The durable receipt answers
        // the retry after a real connection teardown.
        let after_commit = capture_request(
            &scenario.space_id,
            "Committed but the answer was lost.",
            "crash-after",
        );
        scenario
            .provider
            .faults()
            .fail_next(FaultPoint::AfterCommitBeforeResponse);
        let rejected = refusal(
            scenario
                .provider
                .capture(CLIENT, &after_commit, now)
                .await,
        );
        assert_eq!(rejected.code, SpokeRejectCode::OutcomeUncertain);
        assert_eq!(count_rows(&scenario, "candidates").await, 2);
        assert_eq!(count_rows(&scenario, "receipts").await, 2);
        let stored_receipt: String =
            sqlx::query("SELECT receipt_id FROM receipts WHERE idempotency_key = 'crash-after'")
                .fetch_one(scenario.provider.store().pool())
                .await
                .expect("the committed receipt is durable")
                .get("receipt_id");

        let scenario = scenario.reopen_after_loss().await;
        let replayed = scenario
            .provider
            .capture(CLIENT, &after_commit, now)
            .await
            .expect("the committed receipt resolves the lost answer");
        assert_eq!(receipt_id_of(&replayed), stored_receipt);
        assert_eq!(count_rows(&scenario, "candidates").await, 2);
        assert_eq!(count_rows(&scenario, "receipts").await, 2);

        // Phase 3 — a paused admission replays the effect a competing identical
        // retry committed while it was parked. Both callers observe one receipt.
        let paused_request = capture_request(
            &scenario.space_id,
            "Two admissions, one effect.",
            "pause-1",
        );
        let mut hold = scenario
            .provider
            .faults()
            .hold_next(FaultPoint::AdmissionPause);
        let mut paused = Box::pin(scenario.provider.capture(CLIENT, &paused_request, now));
        tokio::select! {
            () = hold.wait_reached() => {}
            result = paused.as_mut() => panic!("the admission was not paused: {result:?}"),
        }
        let winner = scenario
            .provider
            .capture(CLIENT, &paused_request, now)
            .await
            .expect("the competing retry commits");
        hold.release();
        let resumed = paused
            .await
            .expect("the paused admission replays the committed receipt");
        assert_eq!(receipt_id_of(&resumed), receipt_id_of(&winner));
        assert_eq!(count_rows(&scenario, "candidates").await, 3);
        assert_eq!(count_rows(&scenario, "receipts").await, 3);
    }

    #[tokio::test]
    async fn bounded_fault_holds_fail_without_hanging_or_committing() {
        let scenario = Scenario::open().await;
        let clock = FixtureClock::at(
            DateTime::parse_from_rfc3339("2026-11-04T00:00:00Z")
                .expect("a valid instant")
                .with_timezone(&Utc),
        );
        let now = clock.now();
        scenario.subject_and_capture_grant(now).await;
        let request = capture_request(
            &scenario.space_id,
            "A gate nobody releases.",
            "bounded-1",
        );

        let _hold = scenario
            .provider
            .faults()
            .hold_next_bounded(FaultPoint::AdmissionPause, Duration::from_millis(25));
        let started = std::time::Instant::now();
        let rejected = refusal(scenario.provider.capture(CLIENT, &request, now).await);
        assert_eq!(rejected.code, SpokeRejectCode::InternalError);
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "the bounded hold returned promptly"
        );
        assert_eq!(count_rows(&scenario, "candidates").await, 0);
        assert_eq!(count_rows(&scenario, "receipts").await, 0);

        // The same key proceeds normally once the expired gate is gone.
        let committed = scenario
            .provider
            .capture(CLIENT, &request, now)
            .await
            .expect("the operation proceeds after the bounded hold");
        assert_eq!(receipt_status(&committed), "committed");
        assert_eq!(count_rows(&scenario, "candidates").await, 1);
    }

    /// The clock is the only source of "now": moving it changes the instant the
    /// provider evaluates, and nothing samples the wall clock instead.
    #[tokio::test]
    async fn fixture_clock_moves_the_evaluated_instant_explicitly() {
        let base = FixtureClock::at(
            DateTime::parse_from_rfc3339("2026-11-04T00:00:00Z")
                .expect("a valid instant")
                .with_timezone(&Utc),
        );
        assert_eq!(
            base.advanced(ChronoDuration::days(1)).now(),
            base.now() + ChronoDuration::days(1)
        );
        assert_eq!(base.now(), base.now());
    }
}
