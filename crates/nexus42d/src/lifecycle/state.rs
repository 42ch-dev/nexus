//! HSM state machine implementation using statig.
//!
//! Implements the hierarchical state graph from spec §2 with transitions
//! per spec §4.

use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};

// Import statig prelude for Response variants (Transition, Handled, Super)
use statig::awaitable::IntoStateMachineExt;
use statig::prelude::*;

use super::{Event, Lifecycle, LifecycleState, LifecycleTransition, SubsystemKind};
use crate::lifecycle::actions;

/// Shared storage for the daemon HSM.
///
/// Contains state-local data like subsystem tracking and degraded status.
#[derive(Debug, Default)]
pub struct DaemonHsm {
    /// Subsystems that have reported `SubsystemUp`.
    pub up_subsystems: HashSet<SubsystemKind>,
    /// Subsystems currently degraded (for `Degraded` state).
    pub degraded_subsystems: HashSet<SubsystemKind>,
    /// Degraded reasons per subsystem.
    pub degraded_reasons: HashMap<SubsystemKind, String>,
    /// Exit code (set when entering `Failed`).
    pub exit_code: Option<i32>,
    /// Last error message (set when entering `Failed`).
    pub last_error: Option<String>,
}

// The #[state_machine] macro generates:
// - `enum State` with variants for each state method
// - `enum Superstate<'sub>` with variants for each superstate method
// - impl blocks for `statig::State` and `statig::Superstate` traits
// - `IntoStateMachine` impl for `DaemonHsm`

#[statig::state_machine(
    initial = "State::starting()",
    on_dispatch = "Self::on_dispatch",
    on_transition = "Self::on_transition",
    state(derive(Debug)),
    superstate(derive(Debug))
)]
impl DaemonHsm {
    /// Starting state: subsystems booting.
    ///
    /// Collects `SubsystemUp` events; transitions to `Running` when all mandatory
    /// subsystems are up. Handles `SubsystemFailed` → `Failed` if non-retryable.
    /// Handles `ShutdownRequested` → `Stopping` (abort-on-start).
    /// Handles `FatalError` → `Failed`.
    #[state(entry_action = "enter_starting", exit_action = "exit_starting")]
    async fn starting(&mut self, event: &Event) -> Response<State> {
        match event {
            Event::SubsystemUp(kind) => {
                self.up_subsystems.insert(*kind);
                if self.all_mandatory_up() {
                    tracing::info!("all mandatory subsystems up → Running");
                    Transition(State::running())
                } else {
                    Handled
                }
            }
            Event::SubsystemFailed {
                kind,
                err,
                retryable,
            } => {
                if !retryable {
                    tracing::error!("non-retryable subsystem failure: {} ({:?})", err, kind);
                    self.exit_code = Some(1);
                    self.last_error = Some(err.clone());
                    Transition(State::failed())
                } else {
                    tracing::warn!("retryable subsystem failure: {} ({:?})", err, kind);
                    Handled // TODO: retry logic in T4
                }
            }
            Event::ShutdownRequested { source } => {
                tracing::info!("shutdown requested during starting: {}", source);
                Transition(State::stopping())
            }
            Event::FatalError { kind, err } => {
                tracing::error!("fatal error in starting: {} ({:?})", err, kind);
                self.exit_code = Some(1);
                self.last_error = Some(err.clone());
                Transition(State::failed())
            }
            _ => Super,
        }
    }

    /// Running state: fully operational.
    ///
    /// Handles `HealthDegraded` → `Degraded`.
    /// Other events (ShutdownRequested, FatalError) deferred to `Alive` superstate.
    #[state(
        superstate = "alive",
        entry_action = "enter_running",
        exit_action = "exit_running"
    )]
    async fn running(&mut self, event: &Event) -> Response<State> {
        match event {
            Event::HealthDegraded { kind, reason } => {
                tracing::warn!("health degraded: {} ({:?})", reason, kind);
                self.degraded_subsystems.insert(*kind);
                self.degraded_reasons.insert(*kind, reason.clone());
                Transition(State::degraded())
            }
            _ => Super,
        }
    }

    /// Degraded state: running but some subsystems unhealthy.
    ///
    /// Handles `HealthRestored` → `Running` when all degraded subsystems recover.
    /// Handles additional `HealthDegraded` (track more degraded subsystems).
    #[state(
        superstate = "alive",
        entry_action = "enter_degraded",
        exit_action = "exit_degraded"
    )]
    async fn degraded(&mut self, event: &Event) -> Response<State> {
        match event {
            Event::HealthRestored { kind } => {
                self.degraded_subsystems.remove(kind);
                self.degraded_reasons.remove(kind);
                if self.degraded_subsystems.is_empty() {
                    tracing::info!("all subsystems restored → Running");
                    Transition(State::running())
                } else {
                    tracing::info!(
                        "subsystem restored: {:?}, still degraded: {:?}",
                        kind,
                        self.degraded_subsystems
                    );
                    Handled
                }
            }
            Event::HealthDegraded { kind, reason } => {
                self.degraded_subsystems.insert(*kind);
                self.degraded_reasons.insert(*kind, reason.clone());
                tracing::warn!("additional degradation: {} ({:?})", reason, kind);
                Handled
            }
            _ => Super,
        }
    }

    /// Alive superstate: groups Running and Degraded.
    ///
    /// Handles `ShutdownRequested` → `Stopping` and `FatalError` → `Failed`
    /// for both child states.
    #[superstate]
    async fn alive(&mut self, event: &Event) -> Response<State> {
        match event {
            Event::ShutdownRequested { source } => {
                tracing::info!("shutdown requested from alive: {}", source);
                Transition(State::stopping())
            }
            Event::FatalError { kind, err } => {
                tracing::error!("fatal error from alive: {} ({:?})", err, kind);
                self.exit_code = Some(1);
                self.last_error = Some(err.clone());
                Transition(State::failed())
            }
            _ => Super,
        }
    }

    /// Stopping state: graceful shutdown in progress.
    ///
    /// Handles `ShutdownDrained` → `Failed` (exit 0, graceful completion).
    /// Handles `ShutdownTimeout` → `Failed` (exit 1).
    #[state(entry_action = "enter_stopping")]
    async fn stopping(&mut self, event: &Event) -> Response<State> {
        match event {
            Event::ShutdownDrained => {
                tracing::info!("shutdown drained → graceful exit (0)");
                self.exit_code = Some(0);
                self.last_error = None;
                Transition(State::failed())
            }
            Event::ShutdownTimeout { grace_ms_exceeded } => {
                tracing::warn!(
                    "shutdown timeout after {}ms → forced exit (1)",
                    grace_ms_exceeded
                );
                self.exit_code = Some(1);
                self.last_error = Some(format!("shutdown timeout {}ms", grace_ms_exceeded));
                Transition(State::failed())
            }
            _ => Super,
        }
    }

    /// Failed state: terminal; no further transitions.
    ///
    /// All events are ignored (Super → Top drops them).
    #[state(entry_action = "enter_failed")]
    async fn failed(&mut self, event: &Event) -> Response<State> {
        let _ = event; // Suppress unused warning - terminal state ignores all events
        tracing::debug!("event ignored in terminal Failed state");
        Super
    }

    // --- Entry / Exit Actions (stubs for T1-T3, full impl in T4) ---

    #[action]
    async fn enter_starting() {
        tracing::info!("entering Starting state");
        actions::enter_starting_stub();
    }

    #[action]
    async fn exit_starting() {
        tracing::info!("exiting Starting state");
        actions::exit_starting_stub();
    }

    #[action]
    async fn enter_running() {
        tracing::info!("entering Running state");
        actions::enter_running_stub();
    }

    #[action]
    async fn exit_running() {
        tracing::info!("exiting Running state");
        // No action: engine keeps running across Running ↔ Degraded.
    }

    #[action]
    async fn enter_degraded() {
        tracing::info!("entering Degraded state");
        actions::enter_degraded_stub();
    }

    #[action]
    async fn exit_degraded(&mut self) {
        tracing::info!("exiting Degraded state");
        self.degraded_subsystems.clear();
        self.degraded_reasons.clear();
    }

    #[action]
    async fn enter_stopping() {
        tracing::info!("entering Stopping state");
        actions::enter_stopping_stub();
    }

    #[action]
    async fn enter_failed(&mut self) {
        tracing::error!(
            "entering Failed state: exit_code={}, last_error={:?}",
            self.exit_code.unwrap_or(1),
            self.last_error
        );
        actions::enter_failed_stub(self.exit_code.unwrap_or(1));
    }

    // --- Introspection callbacks ---

    fn on_transition(&mut self, source: &State, target: &State) {
        tracing::debug!("transition: {:?} → {:?}", source, target);
    }

    fn on_dispatch(&mut self, state: statig::StateOrSuperstate<Self>, event: &Event) {
        tracing::trace!("dispatch: {:?} → {:?}", event, state);
    }
}

impl DaemonHsm {
    /// Returns true if all mandatory subsystems are up.
    fn all_mandatory_up(&self) -> bool {
        SubsystemKind::mandatory()
            .iter()
            .all(|k| self.up_subsystems.contains(k))
    }
}

/// Wrapper that implements the `Lifecycle` trait.
///
/// Wraps the `statig::awaitable::StateMachine<DaemonHsm>` in an `Arc<Mutex<...>>`
/// for shared access from HTTP handlers, signal handlers, and tests.
///
/// Uses `std::sync::Mutex` for mirrors (simple read/write) and
/// `tokio::sync::Mutex` for the statig state machine (async operations).
pub struct StatigLifecycle {
    /// The statig state machine (lazy-initialized).
    machine: Arc<Mutex<statig::awaitable::StateMachine<DaemonHsm>>>,
    /// Broadcast channel for transition notifications.
    transition_tx: broadcast::Sender<LifecycleTransition>,
    /// Current state mirror (std Mutex for fast synchronous reads).
    current_state: Arc<std::sync::Mutex<LifecycleState>>,
    /// Exit code mirror (std Mutex for fast synchronous reads).
    exit_code: Arc<std::sync::Mutex<Option<i32>>>,
}

impl StatigLifecycle {
    /// Create a new lifecycle with real subsystems (T4 will flesh this out).
    pub fn new() -> Self {
        Self::new_for_test()
    }

    /// Create a lifecycle for testing (all subsystems mocked as no-op).
    pub fn new_for_test() -> Self {
        let (transition_tx, _) = broadcast::channel(16);
        let daemon_hsm = DaemonHsm::default();
        let machine = Arc::new(Mutex::new(daemon_hsm.state_machine()));
        let current_state = Arc::new(std::sync::Mutex::new(LifecycleState::Starting));
        let exit_code = Arc::new(std::sync::Mutex::new(None));

        Self {
            machine,
            transition_tx,
            current_state,
            exit_code,
        }
    }

    /// Force the state machine to a specific state for test setup.
    ///
    /// This bypasses normal transition logic; use only in tests.
    pub fn force_state_for_test(&self, state: LifecycleState) {
        let mut current = self
            .current_state
            .lock()
            .expect("current_state mutex poisoned");
        *current = state;
    }

    /// Wait until the machine reaches a terminal state (`Failed`).
    pub async fn wait_until_terminal(&self) {
        loop {
            let state = self.current_state();
            if state.is_terminal() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }
}

impl Default for StatigLifecycle {
    fn default() -> Self {
        Self::new_for_test()
    }
}

impl Lifecycle for StatigLifecycle {
    fn current_state(&self) -> LifecycleState {
        // Fast read from mirror (std::sync::Mutex - safe in async).
        *self
            .current_state
            .lock()
            .expect("current_state mutex poisoned")
    }

    fn dispatch(&self, event: Event) {
        // Spawn a task to handle async dispatch.
        let machine = Arc::clone(&self.machine);
        let transition_tx = self.transition_tx.clone();
        let current_state = Arc::clone(&self.current_state);
        let exit_code = Arc::clone(&self.exit_code);

        tokio::spawn(async move {
            let mut m = machine.lock().await;
            let before = lifecycle_state_from_statig_state(m.state());

            // handle() initializes if needed and processes the event
            m.handle(&event).await;

            let after = lifecycle_state_from_statig_state(m.state());

            // Read exit_code from shared storage (StateMachine derefs to DaemonHsm).
            let code = m.exit_code;

            // Update mirrors (std::sync::Mutex).
            {
                let mut cur = current_state
                    .lock()
                    .expect("current_state mutex poisoned in dispatch");
                *cur = after;
            }
            {
                let mut ec = exit_code
                    .lock()
                    .expect("exit_code mutex poisoned in dispatch");
                *ec = code;
            }

            // Broadcast transition if state changed.
            if before != after {
                let _ = transition_tx.send(LifecycleTransition {
                    from: before,
                    to: after,
                    event,
                });
            }
        });
    }

    fn subscribe(&self) -> broadcast::Receiver<LifecycleTransition> {
        self.transition_tx.subscribe()
    }

    fn exit_code(&self) -> Option<i32> {
        *self
            .exit_code
            .lock()
            .expect("exit_code mutex poisoned")
    }
}

/// Convert a statig `State` to our `LifecycleState`.
///
/// This is a helper for the mirror sync; in T4 we'll make this cleaner.
fn lifecycle_state_from_statig_state(state: &State) -> LifecycleState {
    match state {
        State::Starting { .. } => LifecycleState::Starting,
        State::Running { .. } => LifecycleState::Running,
        State::Degraded { .. } => LifecycleState::Degraded,
        State::Stopping { .. } => LifecycleState::Stopping,
        State::Failed { .. } => LifecycleState::Failed,
    }
}
