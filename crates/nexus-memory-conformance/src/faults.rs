//! Bounded fixture fault points (C7 control seam).
//!
//! The conformance suite needs to observe what a client sees when a mutation's
//! response is lost, when the process stops mid-transaction, and when an
//! admission is paused behind a competing writer. Those are operator/test
//! controls, never provider authority: the state below lives only in process
//! memory, is consumed once, and is never read back from durable storage.
//!
//! Every wait is bounded: both the operation's hold and the test's observation
//! of it. An observation that never happens can only end as an explicit
//! [`FaultHoldLost`] error — never as a silent success — and dropping or
//! releasing the handle before the point is reached disarms the arm so a later
//! operation cannot inherit a stale fault.
//!
//! The clock is the `now` parameter every provider operation already takes;
//! there is no wall-clock sampling and no wrapper clock object.

use std::future::Future;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

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

/// Why a bounded fault observation ended without a real reach.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaultHoldLost {
    /// No operation reached the point before the observation bound elapsed.
    Unreached,
    /// The arm was released, replaced or dropped before the point was reached.
    Disarmed,
}

impl std::fmt::Display for FaultHoldLost {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unreached => formatter.write_str("the fault point was never reached in time"),
            Self::Disarmed => formatter.write_str("the fault arm was disarmed before it was reached"),
        }
    }
}

/// Why a held operation stopped waiting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaultHoldFailure {
    /// The bounded hold expired without a release.
    Expired,
    /// The test-side handle was dropped or released without resuming this hold.
    ControlLost,
}

/// One armed observation. The token lets the test-side handle disarm exactly its
/// own arm without touching a replacement.
struct Armed {
    token: u64,
    point: FaultPoint,
    /// `true` for a holding point, `false` for a one-shot fail point.
    holding: bool,
    reached: Option<oneshot::Sender<()>>,
    release: Option<oneshot::Receiver<()>>,
    bound: Duration,
}

struct FaultState {
    next_token: u64,
    armed: Option<Armed>,
    /// The op-targeted admission arm (distinct from the point-based `armed`
    /// slot, so a capture's legacy `AdmissionPause` arm and a target arm never
    /// consume each other).
    target: Option<TargetArm>,
}

impl Default for FaultState {
    fn default() -> Self {
        Self {
            next_token: 0,
            armed: None,
            target: None,
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

    fn next_token(state: &mut FaultState) -> u64 {
        state.next_token = state.next_token.wrapping_add(1);
        state.next_token
    }

    /// Arm a one-shot fail point (`BeforeCommit` or
    /// `AfterCommitBeforeResponse`), replacing any previous arm. The next
    /// operation that reaches `point` fails through the protocol-defined path;
    /// later operations proceed.
    pub fn fail_next(&self, point: FaultPoint) {
        let mut state = self.state();
        let token = Self::next_token(&mut state);
        state.armed = Some(Armed {
            token,
            point,
            holding: false,
            reached: None,
            release: None,
            bound: FAULT_HOLD_TIMEOUT,
        });
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
        let token = Self::next_token(&mut state);
        state.armed = Some(Armed {
            token,
            point,
            holding: true,
            reached: Some(reached_tx),
            release: Some(release_rx),
            bound,
        });
        FaultHold {
            control: Arc::clone(&self.inner),
            token,
            reached: reached_rx,
            release: Some(release_tx),
            bound,
        }
    }

    /// Consume a one-shot fail point. Crate-private: only the provider's own
    /// mutation path may trip a point.
    pub(crate) fn trip(&self, point: FaultPoint) -> bool {
        let mut state = self.state();
        if state
            .armed
            .as_ref()
            .is_some_and(|armed| armed.point == point && !armed.holding)
        {
            state.armed = None;
            return true;
        }
        false
    }

    /// Block a holding point until the test releases it, or end the wait when
    /// the bound elapses or the test-side handle disappears. A point that is
    /// not armed is not reached and returns immediately.
    pub(crate) async fn hold(&self, point: FaultPoint) -> Result<(), FaultHoldFailure> {
        let gate = {
            let mut state = self.state();
            match state.armed.as_ref() {
                Some(armed) if armed.point == point && armed.holding => state.armed.take(),
                _ => None,
            }
        };
        let Some(Armed {
            reached,
            release,
            bound,
            ..
        }) = gate
        else {
            return Ok(());
        };
        if let Some(reached) = reached {
            // The observation may already be gone; a missing receiver is not a
            // success signal for either side.
            let _ = reached.send(());
        }
        let Some(release) = release else {
            return Err(FaultHoldFailure::ControlLost);
        };
        match tokio::time::timeout(bound, release).await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(_)) => Err(FaultHoldFailure::ControlLost),
            Err(_) => Err(FaultHoldFailure::Expired),
        }
    }
}

impl Default for FixtureFaults {
    fn default() -> Self {
        Self::new()
    }
}

/// The test-side handle for one armed holding point.
///
/// `wait_reached` is itself bounded, and every non-observation outcome is an
/// explicit error. Releasing or dropping the handle before the point is reached
/// disarms the arm, so no later operation can inherit it.
pub struct FaultHold {
    control: Arc<Mutex<FaultState>>,
    token: u64,
    reached: oneshot::Receiver<()>,
    release: Option<oneshot::Sender<()>>,
    bound: Duration,
}

impl FaultHold {
    /// Wait — bounded by this hold's own duration — until the provider reached
    /// the armed point.
    ///
    /// Cancellation is owned from the call itself: a wait that ends without a
    /// reach — timeout, a cancelled future, or a future created and dropped
    /// before its first poll — retires the handle's own still-armed entry at
    /// once, so keeping the handle alive cannot make a later operation inherit
    /// the fault.
    ///
    /// # Errors
    ///
    /// [`FaultHoldLost::Unreached`] when no operation reached the point in time,
    /// [`FaultHoldLost::Disarmed`] when the arm was released, replaced or
    /// dropped first.
    pub fn wait_reached(
        &mut self,
    ) -> impl Future<Output = Result<(), FaultHoldLost>> + '_ {
        // Cancellation ownership is established at the CALL, before the future
        // can be polled: an observation future that is created and dropped
        // without a first poll still retires this handle's own still-armed
        // entry, so a retained handle cannot make a later operation inherit the
        // fault. A replacement (different token) or an already-consumed arm is
        // untouched. Native RAII only — no adapter type, allocation or flag.
        let guard = ReachGuard {
            control: Arc::clone(&self.control),
            token: self.token,
        };
        let bound = self.bound;
        let reached = &mut self.reached;
        async move {
            let _guard = guard;
            match tokio::time::timeout(bound, reached).await {
                Ok(Ok(())) => Ok(()),
                Ok(Err(_)) => Err(FaultHoldLost::Disarmed),
                Err(_) => Err(FaultHoldLost::Unreached),
            }
        }
    }

    /// Resume the held operation. If the point has not been reached yet, this
    /// disarms it instead so a later operation is unaffected.
    pub fn release(&mut self) {
        let mut state = self
            .control
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state
            .armed
            .as_ref()
            .is_some_and(|armed| armed.token == self.token)
        {
            state.armed = None;
            return;
        }
        drop(state);
        if let Some(release) = self.release.take() {
            let _ = release.send(());
        }
    }
}

/// Disarms one still-armed token when its observation ends without a reach.
struct ReachGuard {
    control: Arc<Mutex<FaultState>>,
    token: u64,
}

impl Drop for ReachGuard {
    fn drop(&mut self) {
        let mut state = self
            .control
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state
            .armed
            .as_ref()
            .is_some_and(|armed| armed.token == self.token)
        {
            state.armed = None;
        }
    }
}

impl Drop for FaultHold {
    fn drop(&mut self) {
        let mut state = self
            .control
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state
            .armed
            .as_ref()
            .is_some_and(|armed| armed.token == self.token)
        {
            // Never reached: disarm so a later operation cannot inherit it.
            state.armed = None;
        }
        // A taken arm means the operation is waiting: dropping the release
        // sender closes its channel, which is an explicit fault failure.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use chrono::{DateTime, Utc};
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

        /// Await the provider's own bounded async close (which drains every
        /// pooled connection, including the rollback of a cancelled
        /// transaction), then serve the same file from a new provider instance.
        async fn close_and_reopen(self) -> Self {
            let Self {
                _home,
                path,
                provider,
                space_id,
            } = self;
            let closed = tokio::time::timeout(Duration::from_secs(5), provider.close()).await;
            assert!(closed.is_ok(), "the fixture pool closed within the bound");
            let provider = FixtureMemoryProvider::open(&path)
                .await
                .expect("fixture reopens after the awaited close");
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

    fn now_instant() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-11-04T00:00:00Z")
            .expect("a valid instant")
            .with_timezone(&Utc)
    }

    fn capture_request(space: &str, text: &str, key: &str) -> Admitted<MemoryCaptureRequest> {
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

    fn refusal<T: std::fmt::Debug>(result: Result<T, spoke_operations::SpokeReject>) -> spoke_operations::SpokeReject {
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
        sqlx::query(sql)
            .fetch_one(scenario.provider.store().pool())
            .await
            .expect("count query")
            .get("total")
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
        let now = now_instant();
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
            reached = hold.wait_reached() => {
                reached.expect("the response drop point was reached")
            }
            result = operation.as_mut() => {
                panic!("the response was not dropped: {result:?}")
            }
        }
        drop(operation);
        let scenario = scenario.close_and_reopen().await;

        // The commit happened before the hold, so the effect and its receipt are
        // on disk even though the response never reached the caller.
        let stored_receipt: String = sqlx::query("SELECT receipt_id FROM receipts")
            .fetch_one(scenario.provider.store().pool())
            .await
            .expect("the committed receipt is durable")
            .get("receipt_id");
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
        let now = now_instant();
        scenario.subject_and_capture_grant(now).await;
        let marker_before = marker(&scenario).await;

        // Phase 0 — cancellation mid-commit. The capture is staged inside an
        // open transaction and the caller drops the future; the provider's own
        // async close then drains the connection (and its rollback) before the
        // same file is reopened.
        let mid_commit = capture_request(
            &scenario.space_id,
            "Staged inside an open transaction.",
            "crash-mid-commit",
        );
        let mut hold = scenario.provider.faults().hold_next(FaultPoint::BeforeCommit);
        let mut operation = Box::pin(scenario.provider.capture(CLIENT, &mid_commit, now));
        tokio::select! {
            reached = hold.wait_reached() => {
                reached.expect("the pre-commit point was reached")
            }
            result = operation.as_mut() => {
                panic!("the pre-commit point was not reached: {result:?}")
            }
        }
        drop(operation);
        let scenario = scenario.close_and_reopen().await;
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

        // Phase 1 — a clean abort before commit. Same no-effect boundary, and
        // the same-key retry commits exactly once.
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
        assert_eq!(rejected.code, spoke_operations::SpokeRejectCode::OutcomeUncertain);
        assert_eq!(count_rows(&scenario, "candidates").await, 0);
        assert_eq!(marker(&scenario).await, marker_before);
        let committed = scenario
            .provider
            .capture(CLIENT, &before_commit, now)
            .await
            .expect("the retry after the abort commits once");
        assert_eq!(count_rows(&scenario, "candidates").await, 1);
        let replayed = scenario
            .provider
            .capture(CLIENT, &before_commit, now)
            .await
            .expect("the committed retry replays");
        assert_eq!(receipt_id_of(&replayed), receipt_id_of(&committed));
        assert_eq!(count_rows(&scenario, "candidates").await, 1);

        // Phase 2 — commit succeeded, response lost. The durable receipt answers
        // the retry after the awaited close and reopen.
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
        assert_eq!(rejected.code, spoke_operations::SpokeRejectCode::OutcomeUncertain);
        assert_eq!(count_rows(&scenario, "candidates").await, 2);
        let stored_receipt: String =
            sqlx::query("SELECT receipt_id FROM receipts WHERE idempotency_key = 'crash-after'")
                .fetch_one(scenario.provider.store().pool())
                .await
                .expect("the committed receipt is durable")
                .get("receipt_id");
        let scenario = scenario.close_and_reopen().await;
        let replayed = scenario
            .provider
            .capture(CLIENT, &after_commit, now)
            .await
            .expect("the committed receipt resolves the lost answer");
        assert_eq!(receipt_id_of(&replayed), stored_receipt);
        assert_eq!(count_rows(&scenario, "candidates").await, 2);

        // Phase 3 — a paused admission replays the effect a competing identical
        // retry committed while it was parked.
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
            reached = hold.wait_reached() => {
                reached.expect("the admission pause was reached")
            }
            result = paused.as_mut() => {
                panic!("the admission was not paused: {result:?}")
            }
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
    }

    /// §14.2/§14.3: a fault after the commit must never look like a clean
    /// no-effect failure. Expiry of a post-commit hold and a dropped control
    /// both report `OUTCOME_UNCERTAIN` on a durable effect; a pre-commit hold
    /// expiry is a clean retryable failure with no effect.
    #[tokio::test]
    async fn postcommit_faults_report_uncertain_and_precommit_faults_stay_clean() {
        let scenario = Scenario::open().await;
        let now = now_instant();
        scenario.subject_and_capture_grant(now).await;

        // Post-commit hold expiry: the committed receipt stays durable and the
        // caller must resolve it with the same key.
        let timeout_request = capture_request(
            &scenario.space_id,
            "Committed before a bounded hold expires.",
            "post-timeout",
        );
        let _hold = scenario
            .provider
            .faults()
            .hold_next_bounded(FaultPoint::ResponseDrop, Duration::from_millis(25));
        let rejected = refusal(
            scenario
                .provider
                .capture(CLIENT, &timeout_request, now)
                .await,
        );
        assert_eq!(rejected.code, spoke_operations::SpokeRejectCode::OutcomeUncertain);
        assert_eq!(count_rows(&scenario, "candidates").await, 1);
        assert_eq!(count_rows(&scenario, "receipts").await, 1);
        let replayed = scenario
            .provider
            .capture(CLIENT, &timeout_request, now)
            .await
            .expect("the same key resolves the uncertain outcome");
        assert_eq!(receipt_status(&replayed), "committed");

        // Dropped control: the test-side handle disappears while the committed
        // response is held, which is still an uncertain outcome on a durable
        // receipt, never a clean no-effect failure.
        let dropped_request = capture_request(
            &scenario.space_id,
            "Committed before the control was dropped.",
            "post-control-lost",
        );
        let mut hold = scenario.provider.faults().hold_next(FaultPoint::ResponseDrop);
        let mut operation = Box::pin(scenario.provider.capture(CLIENT, &dropped_request, now));
        tokio::select! {
            reached = hold.wait_reached() => {
                reached.expect("the response drop point was reached")
            }
            result = operation.as_mut() => {
                panic!("the committed response was not held: {result:?}")
            }
        }
        drop(hold);
        match operation.await {
            Err(rejected) => assert_eq!(
                rejected.code,
                spoke_operations::SpokeRejectCode::OutcomeUncertain,
                "a lost post-commit control is uncertain"
            ),
            Ok(receipt) => panic!("a dropped post-commit control must not return success: {receipt:?}"),
        }
        assert_eq!(count_rows(&scenario, "candidates").await, 2);
        assert_eq!(count_rows(&scenario, "receipts").await, 2);
        let replayed = scenario
            .provider
            .capture(CLIENT, &dropped_request, now)
            .await
            .expect("the dropped control still resolves by replay");
        assert_eq!(receipt_status(&replayed), "committed");
        assert_eq!(count_rows(&scenario, "candidates").await, 2);

        // Pre-commit hold expiry: nothing staged is durable, so the clean
        // retryable internal failure is honest.
        let precommit_request = capture_request(
            &scenario.space_id,
            "Staged, then the pre-commit hold expired.",
            "pre-timeout",
        );
        let _hold = scenario
            .provider
            .faults()
            .hold_next_bounded(FaultPoint::BeforeCommit, Duration::from_millis(25));
        let rejected = refusal(
            scenario
                .provider
                .capture(CLIENT, &precommit_request, now)
                .await,
        );
        assert_eq!(rejected.code, spoke_operations::SpokeRejectCode::InternalError);
        assert_eq!(count_rows(&scenario, "candidates").await, 2);
        let committed = scenario
            .provider
            .capture(CLIENT, &precommit_request, now)
            .await
            .expect("the retry after the pre-commit fault commits");
        assert_eq!(count_rows(&scenario, "candidates").await, 3);
        assert_eq!(receipt_status(&committed), "committed");
    }

    /// F5: the observation is bounded, and disarmed/replaced/dropped arms are
    /// explicit errors, never a fake reach or a stale fault for a later request.
    #[tokio::test]
    async fn fault_control_bounds_observation_and_never_fakes_a_reach() {
        let scenario = Scenario::open().await;
        let now = now_instant();
        scenario.subject_and_capture_grant(now).await;

        // Nothing polls the point: the observation itself times out, and the
        // dropped handle disarms it so a later capture is unaffected.
        let mut never = scenario
            .provider
            .faults()
            .hold_next_bounded(FaultPoint::AdmissionPause, Duration::from_millis(20));
        assert_eq!(never.wait_reached().await, Err(FaultHoldLost::Unreached));
        drop(never);
        let request = capture_request(&scenario.space_id, "No stale fault.", "control-1");
        scenario
            .provider
            .capture(CLIENT, &request, now)
            .await
            .expect("no stale fault remains after the dropped observation");
        assert_eq!(count_rows(&scenario, "candidates").await, 1);

        // A replacement arm closes the old observation channel: the old handle
        // reports an explicit disarmed result instead of a reach.
        let mut replaced = scenario
            .provider
            .faults()
            .hold_next(FaultPoint::AdmissionPause);
        scenario
            .provider
            .faults()
            .fail_next(FaultPoint::BeforeCommit);
        assert_eq!(
            replaced.wait_reached().await,
            Err(FaultHoldLost::Disarmed),
            "a replaced arm is not a reach"
        );
        drop(replaced);

        // The replacement fail point is still armed and trips on the next
        // operation; it never leaks to a later one.
        let second = capture_request(&scenario.space_id, "Replacement trips.", "control-2");
        assert_eq!(
            refusal(scenario.provider.capture(CLIENT, &second, now).await).code,
            spoke_operations::SpokeRejectCode::OutcomeUncertain
        );
        assert_eq!(count_rows(&scenario, "candidates").await, 1);
        let third = capture_request(&scenario.space_id, "Later call is clean.", "control-3");
        scenario
            .provider
            .capture(CLIENT, &third, now)
            .await
            .expect("the consumed fail point does not reach a later call");
        assert_eq!(count_rows(&scenario, "candidates").await, 2);

        // Releasing before the point is reached disarms it, so the next
        // operation proceeds without a fault.
        let mut early = scenario
            .provider
            .faults()
            .hold_next(FaultPoint::AdmissionPause);
        early.release();
        assert_eq!(early.wait_reached().await, Err(FaultHoldLost::Disarmed));
        let fourth = capture_request(&scenario.space_id, "Released early.", "control-4");
        scenario
            .provider
            .capture(CLIENT, &fourth, now)
            .await
            .expect("an early release does not arm a later fault");
        assert_eq!(count_rows(&scenario, "candidates").await, 3);
    }

    /// F3: a post-commit hold that expires without any release still reports an
    /// uncertain outcome (the operation-side bound, not the observation bound).
    #[tokio::test]
    async fn bounded_fault_hold_reports_uncertain_without_committing_a_clean_failure() {
        let scenario = Scenario::open().await;
        let now = now_instant();
        scenario.subject_and_capture_grant(now).await;
        let request = capture_request(
            &scenario.space_id,
            "Held past its bound after commit.",
            "bounded-1",
        );
        let _hold = scenario
            .provider
            .faults()
            .hold_next_bounded(FaultPoint::ResponseDrop, Duration::from_millis(25));
        let started = std::time::Instant::now();
        let rejected = refusal(scenario.provider.capture(CLIENT, &request, now).await);
        assert_eq!(
            rejected.code,
            spoke_operations::SpokeRejectCode::OutcomeUncertain
        );
        assert!(started.elapsed() < Duration::from_secs(2));
        assert_eq!(count_rows(&scenario, "candidates").await, 1);
        let replayed = scenario
            .provider
            .capture(CLIENT, &request, now)
            .await
            .expect("the bounded post-commit hold resolves by replay");
        assert_eq!(receipt_status(&replayed), "committed");
        assert_eq!(count_rows(&scenario, "candidates").await, 1);
    }

    /// F5/I185: an observation timeout or a cancelled observation retires its
    /// own still-unreached arm at once, so retaining the handle cannot make a
    /// later operation inherit the fault.
    #[tokio::test]
    async fn observation_timeout_or_cancellation_disarms_its_own_arm() {
        let scenario = Scenario::open().await;
        let now = now_instant();
        scenario.subject_and_capture_grant(now).await;

        // Timeout with the handle retained: a later capture proceeds normally.
        let mut timed_out = scenario
            .provider
            .faults()
            .hold_next_bounded(FaultPoint::AdmissionPause, Duration::from_millis(20));
        assert_eq!(timed_out.wait_reached().await, Err(FaultHoldLost::Unreached));
        let first = capture_request(&scenario.space_id, "After an unreached timeout.", "disarm-timeout");
        scenario
            .provider
            .capture(CLIENT, &first, now)
            .await
            .expect("a timed-out observation does not affect a later capture");
        assert_eq!(count_rows(&scenario, "candidates").await, 1);
        drop(timed_out);

        // The same for a post-commit point: a later capture must commit, not
        // surprise the caller with an uncertain outcome.
        let mut timed_out = scenario
            .provider
            .faults()
            .hold_next_bounded(FaultPoint::ResponseDrop, Duration::from_millis(20));
        assert_eq!(timed_out.wait_reached().await, Err(FaultHoldLost::Unreached));
        let second = capture_request(&scenario.space_id, "Committed after a stale drop point.", "disarm-drop");
        assert_eq!(
            receipt_status(
                &scenario
                    .provider
                    .capture(CLIENT, &second, now)
                    .await
                    .expect("a timed-out post-commit observation does not affect a later capture")
            ),
            "committed"
        );
        assert_eq!(count_rows(&scenario, "candidates").await, 2);
        drop(timed_out);

        // Cancelling the observation future (without dropping the handle) also
        // retires its own arm.
        let mut cancelled = scenario
            .provider
            .faults()
            .hold_next_bounded(FaultPoint::AdmissionPause, Duration::from_millis(50));
        {
            let mut observation = Box::pin(cancelled.wait_reached());
            let raced = tokio::time::timeout(Duration::from_millis(5), &mut observation).await;
            assert!(raced.is_err(), "the observation is still waiting");
            // Dropping `observation` cancels the observation future here.
        }
        let third = capture_request(&scenario.space_id, "After a cancelled observation.", "disarm-cancel");
        scenario
            .provider
            .capture(CLIENT, &third, now)
            .await
            .expect("a cancelled observation does not affect a later capture");
        assert_eq!(count_rows(&scenario, "candidates").await, 3);
        drop(cancelled);

        // A reached, still-held operation is not disturbed by another point's
        // observation timing out: the guard retires only its own token.
        let mut held = scenario
            .provider
            .faults()
            .hold_next_bounded(FaultPoint::AdmissionPause, Duration::from_millis(300));
        let fourth = capture_request(&scenario.space_id, "Held once.", "disarm-reached");
        let mut operation = Box::pin(scenario.provider.capture(CLIENT, &fourth, now));
        tokio::select! {
            observation = held.wait_reached() => {
                observation.expect("the point was reached")
            }
            result = operation.as_mut() => {
                panic!("the admission was not paused: {result:?}")
            }
        }
        let mut unrelated = scenario
            .provider
            .faults()
            .hold_next_bounded(FaultPoint::ResponseDrop, Duration::from_millis(20));
        assert_eq!(unrelated.wait_reached().await, Err(FaultHoldLost::Unreached));
        drop(unrelated);
        held.release();
        operation
            .await
            .expect("the reached operation continues after the unrelated timeout");
        assert_eq!(count_rows(&scenario, "candidates").await, 4);
    }

    /// A bounded admission hold that expires before any effect is a clean,
    /// retryable failure; the same key then commits exactly once.
    #[tokio::test]
    async fn admission_hold_expiry_is_clean_and_effect_free() {
        let scenario = Scenario::open().await;
        let now = now_instant();
        scenario.subject_and_capture_grant(now).await;
        let request = capture_request(
            &scenario.space_id,
            "An admission hold that expires.",
            "admission-bounded",
        );
        let _hold = scenario
            .provider
            .faults()
            .hold_next_bounded(FaultPoint::AdmissionPause, Duration::from_millis(20));
        let rejected = refusal(
            scenario
                .provider
                .capture(CLIENT, &request, now)
                .await,
        );
        assert_eq!(
            rejected.code,
            spoke_operations::SpokeRejectCode::InternalError
        );
        assert_eq!(count_rows(&scenario, "candidates").await, 0);
        let committed = scenario
            .provider
            .capture(CLIENT, &request, now)
            .await
            .expect("the retry after the admission expiry commits once");
        assert_eq!(receipt_status(&committed), "committed");
        assert_eq!(count_rows(&scenario, "candidates").await, 1);
    }

    /// I185: a retained handle plus an observation future that is created and
    /// dropped WITHOUT a first poll must retire its own arm, so a later real
    /// capture is unaffected at both fault phases.
    #[tokio::test]
    async fn zero_poll_dropped_observation_disarms_its_own_arm() {
        let scenario = Scenario::open().await;
        let now = now_instant();
        scenario.subject_and_capture_grant(now).await;

        // Pre-commit point: the retained handle's unpolled future disarms.
        let mut retained = scenario
            .provider
            .faults()
            .hold_next_bounded(FaultPoint::AdmissionPause, Duration::from_millis(20));
        {
            let observation = retained.wait_reached();
            drop(observation);
        }
        let first = capture_request(
            &scenario.space_id,
            "Unpolled observation before commit.",
            "zero-poll-pre",
        );
        scenario
            .provider
            .capture(CLIENT, &first, now)
            .await
            .expect("an unpolled dropped observation leaves no pre-commit fault");
        assert_eq!(count_rows(&scenario, "candidates").await, 1);
        drop(retained);

        // Post-commit point: the same, without an unexpected uncertain outcome.
        let mut retained = scenario
            .provider
            .faults()
            .hold_next_bounded(FaultPoint::ResponseDrop, Duration::from_millis(20));
        {
            let observation = retained.wait_reached();
            drop(observation);
        }
        let second = capture_request(
            &scenario.space_id,
            "Unpolled observation after commit.",
            "zero-poll-post",
        );
        assert_eq!(
            receipt_status(
                &scenario
                    .provider
                    .capture(CLIENT, &second, now)
                    .await
                    .expect("an unpolled dropped observation leaves no post-commit fault")
            ),
            "committed"
        );
        assert_eq!(count_rows(&scenario, "candidates").await, 2);
        drop(retained);
    }
}

/// One op-targeted admission arm: a bounded pause at the exact mutating
/// handler entry named by `op`. It is test control only and outcome-neutral:
/// releasing, expiring or losing control always lets the operation proceed.
struct TargetArm {
    token: u64,
    op: String,
    entered: Option<oneshot::Sender<()>>,
    release: Option<oneshot::Receiver<()>>,
    bound: Duration,
}

/// The test-side handle for one op-targeted admission arm.
///
/// Bounded and token-owned: dropping or cancelling the handle can only disarm
/// its own arm, never a replacement; once the provider has taken the arm the
/// handle's `release` resumes it, and a vanished handle only costs the provider
/// its bounded wait.
pub struct TargetHold {
    control: Arc<Mutex<FaultState>>,
    token: u64,
    reached: oneshot::Receiver<()>,
    release: Option<oneshot::Sender<()>>,
    bound: Duration,
}

impl FixtureFaults {
    /// Arm one op-targeted admission pause at the exact mutating handler entry
    /// named by `op` (replacing any previous target arm). Only that op consumes
    /// it; reads, invalid payloads and every other operation are unaffected.
    pub fn hold_next_for(&self, op: impl Into<String>) -> TargetHold {
        self.hold_next_for_bounded(op, FAULT_HOLD_TIMEOUT)
    }

    /// [`Self::hold_next_for`] with an explicit bound.
    pub fn hold_next_for_bounded(&self, op: impl Into<String>, bound: Duration) -> TargetHold {
        let (reached_tx, reached_rx) = oneshot::channel();
        let (release_tx, release_rx) = oneshot::channel();
        let mut state = self.state();
        let token = Self::next_token(&mut state);
        state.target = Some(TargetArm {
            token,
            op: op.into(),
            entered: Some(reached_tx),
            release: Some(release_rx),
            bound,
        });
        TargetHold {
            control: Arc::clone(&self.inner),
            token,
            reached: reached_rx,
            release: Some(release_tx),
            bound,
        }
    }

    /// The provider-side admission wait for one exact mutating operation.
    ///
    /// Takes the armed target only when it names this op, reports the arrival,
    /// and waits for the release no longer than the arm's own bound. Every end
    /// of that wait — release, expiry, a vanished handle — lets the operation
    /// proceed, so the pause never changes an operation's outcome.
    pub(crate) async fn wait_target(&self, op: &str) {
        let arm = {
            let mut state = self.state();
            match state.target.take() {
                Some(arm) if arm.op == op => Some(arm),
                other => {
                    state.target = other;
                    None
                }
            }
        };
        let Some(mut arm) = arm else {
            return;
        };
        if let Some(entered) = arm.entered.take() {
            let _ = entered.send(());
        }
        if let Some(release) = arm.release.take() {
            let _ = tokio::time::timeout(arm.bound, release).await;
        }
    }
}

impl TargetHold {
    /// Wait, bounded, until the provider reached this exact op's admission.
    pub async fn wait_reached(&mut self) -> Result<(), FaultHoldLost> {
        let guard = TargetReachGuard {
            control: Arc::clone(&self.control),
            token: self.token,
        };
        let bound = self.bound;
        let reached = &mut self.reached;
        let outcome = async move {
            let _guard = guard;
            match tokio::time::timeout(bound, reached).await {
                Ok(Ok(())) => Ok(()),
                Ok(Err(_)) => Err(FaultHoldLost::Disarmed),
                Err(_) => Err(FaultHoldLost::Unreached),
            }
        };
        outcome.await
    }

    /// Resume the held operation; before it is taken this disarms only its own
    /// arm.
    pub fn release(&mut self) {
        let mut state = self
            .control
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state
            .target
            .as_ref()
            .is_some_and(|arm| arm.token == self.token)
        {
            state.target = None;
            return;
        }
        drop(state);
        if let Some(release) = self.release.take() {
            let _ = release.send(());
        }
    }
}

/// Disarms one still-armed target arm when its observation ends without a take.
struct TargetReachGuard {
    control: Arc<Mutex<FaultState>>,
    token: u64,
}

impl Drop for TargetReachGuard {
    fn drop(&mut self) {
        let mut state = self
            .control
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state
            .target
            .as_ref()
            .is_some_and(|arm| arm.token == self.token)
        {
            state.target = None;
        }
    }
}
