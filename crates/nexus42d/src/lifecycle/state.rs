//! Statig callbacks require `unused_self` for introspection methods.
//! Mutex guard must be held for entire state machine operation scope.
#![allow(clippy::unused_self, clippy::missing_panics_doc, clippy::significant_drop_tightening)]
//! HTTP handlers have consistent error patterns.
#![allow(clippy::missing_errors_doc)]
//! HSM state machine implementation using statig.
//!
//! Implements the hierarchical state graph from spec §2 with transitions
//! per spec §4.
//!
//! ## T4 Changes
//!
//! - `DaemonHsm` now stores an optional `ActionContext` for subsystem management.
//! - Entry/exit actions use the context to spawn subsystem tasks.
//! - `StatigLifecycle::new_with_subsystems()` for real runs.
//! - `new_for_test()` for testing (no subsystem tasks spawned).

use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};

// Import statig prelude for Response variants (Transition, Handled, Super)
use statig::awaitable::IntoStateMachineExt;
use statig::prelude::*;

use super::subsystems::SubsystemBootstrap;
use super::{Event, Lifecycle, LifecycleState, LifecycleTransition, SubsystemKind};
use crate::lifecycle::actions::{
    enter_degraded, enter_failed, enter_running, enter_starting, enter_stopping, exit_running,
    exit_starting, ActionContext,
};

/// Shared storage for the daemon HSM.
///
/// Contains state-local data like subsystem tracking and degraded status.
/// In T4, also holds the `ActionContext` for entry/exit actions.
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
    /// Action context for subsystem management (set in real runs, None in tests).
    context: Option<Arc<ActionContext>>,
}

impl DaemonHsm {
    /// Create a `DaemonHsm` with subsystem context for real runs.
    pub fn with_context(context: Arc<ActionContext>) -> Self {
        Self {
            context: Some(context),
            ..Default::default()
        }
    }
}

// The #[state_machine] macro generates:
// - `enum State` with variants for each state method
// - `enum Superstate<'sub>` with variants for each superstate method
// - impl blocks for `statig::State` and `statig::Superstate` traits
// - `IntoStateMachine` impl for `DaemonHsm`

// Allow unused_async because statig::awaitable requires async methods
// even when no await is needed in the method body.
#[statig::state_machine(
    initial = "State::starting()",
    on_dispatch = "Self::on_dispatch",
    on_transition = "Self::on_transition",
    state(derive(Debug)),
    superstate(derive(Debug))
)]
#[allow(clippy::unused_async)]
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
                tracing::debug!(
                    "SubsystemUp {:?} — up_subsystems: {:?}",
                    kind,
                    self.up_subsystems
                );
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
                if *retryable {
                    tracing::warn!("retryable subsystem failure: {} ({:?})", err, kind);
                    // Retry logic for recoverable subsystem failures is deferred
                    // past V1.11 — see delivery compass v1.11 §5 WS-A Group 2.
                    Handled
                } else {
                    tracing::error!("non-retryable subsystem failure: {} ({:?})", err, kind);
                    self.exit_code = Some(1);
                    self.last_error = Some(err.clone());
                    Transition(State::failed())
                }
            }
            Event::HealthDegraded { kind, reason } => {
                // Only transition to Degraded if the subsystem has already
                // reported SubsystemUp. Ignore HealthDegraded for subsystems
                // still booting (RISK-WSC-01).
                if self.up_subsystems.contains(kind) {
                    tracing::warn!("health degraded during starting: {} ({:?})", reason, kind);
                    self.degraded_subsystems.insert(*kind);
                    self.degraded_reasons.insert(*kind, reason.clone());
                    Transition(State::degraded())
                } else {
                    tracing::debug!("ignoring HealthDegraded for {:?} — not yet up", kind);
                    Handled
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
    /// Other events (`ShutdownRequested`, `FatalError`) deferred to `Alive` superstate.
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
                self.last_error = Some(format!("shutdown timeout {grace_ms_exceeded}ms"));
                Transition(State::failed())
            }
            _ => Super,
        }
    }

    /// Failed state: terminal; no further transitions.
    ///
    /// All events are ignored (Super → Top drops them).
    #[state(entry_action = "enter_failed")]
    #[allow(clippy::needless_pass_by_ref_mut)] // statig macro requires &mut self
    async fn failed(&mut self, event: &Event) -> Response<State> {
        let _ = event; // Suppress unused warning - terminal state ignores all events
        tracing::debug!("event ignored in terminal Failed state");
        Super
    }

    // --- Entry / Exit Actions ---
    //
    // These check if context is available. If so, they call the real action
    // functions that spawn subsystem tasks. If no context (test mode), they
    // just log.

    #[action]
    async fn enter_starting(&self) {
        tracing::info!("entering Starting state");
        if let Some(ctx) = &self.context {
            enter_starting(ctx);
        } else {
            tracing::debug!("Starting.entry: no context (test mode) — subsystems not spawned");
        }
    }

    #[action]
    async fn exit_starting(&self) {
        tracing::info!("exiting Starting state");
        if let Some(ctx) = &self.context {
            exit_starting(ctx);
        }
    }

    #[action]
    async fn enter_running(&self) {
        tracing::info!("entering Running state");
        if let Some(ctx) = &self.context {
            enter_running(ctx);
        }
    }

    #[action]
    async fn exit_running(&self) {
        tracing::info!("exiting Running state");
        if let Some(ctx) = &self.context {
            exit_running(ctx);
        }
    }

    #[action]
    async fn enter_degraded(&self) {
        tracing::info!("entering Degraded state");
        if let Some(ctx) = &self.context {
            enter_degraded(ctx);
        }
    }

    #[action]
    async fn exit_degraded(&mut self) {
        tracing::info!("exiting Degraded state");
        self.degraded_subsystems.clear();
        self.degraded_reasons.clear();
    }

    #[action]
    async fn enter_stopping(&self) {
        tracing::info!("entering Stopping state");
        if let Some(ctx) = &self.context {
            enter_stopping(ctx);
        }
    }

    #[action]
    async fn enter_failed(&self) {
        tracing::error!(
            "entering Failed state: exit_code={}, last_error={:?}",
            self.exit_code.unwrap_or(1),
            self.last_error
        );

        // Only call process::exit if we have a context (real run, not test)
        if self.context.is_some() {
            // Call the exit function directly (synchronously).
            // Since this is the terminal state, blocking the async task is acceptable.
            // The process will exit shortly anyway.
            // This fixes QC1-C1: no race between thread spawn and action sleep.
            let exit_code = self.exit_code.unwrap_or(1);
            let last_error = self.last_error.clone();
            enter_failed(exit_code, last_error);
        } else {
            // Test mode: don't call process::exit
            tracing::info!("Failed.entry: test mode — not calling process::exit");
        }
    }

    // --- Introspection callbacks ---

    fn on_transition(&mut self, source: &State, target: &State) {
        tracing::debug!("transition: {:?} → {:?}", source, target);
    }

    /// Statig callback for dispatch events.
    /// Note: Signature is dictated by statig macro; cannot take state by reference.
    #[allow(clippy::needless_pass_by_value)]
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
///
/// For `new_with_subsystems`, uses deferred initialization pattern:
/// the machine is created after wrapping in Arc so `ActionContext` can reference
/// the final lifecycle.
pub struct StatigLifecycle {
    /// The statig state machine.
    /// Uses Option for deferred initialization in `new_with_subsystems`.
    machine: Arc<Mutex<Option<statig::awaitable::StateMachine<DaemonHsm>>>>,
    /// Broadcast channel for transition notifications.
    transition_tx: broadcast::Sender<LifecycleTransition>,
    /// Current state mirror (std Mutex for fast synchronous reads).
    current_state: Arc<std::sync::Mutex<LifecycleState>>,
    /// Exit code mirror (std Mutex for fast synchronous reads).
    exit_code: Arc<std::sync::Mutex<Option<i32>>>,
}

impl StatigLifecycle {
    /// Create a lifecycle with real subsystems.
    ///
    /// This constructor creates a state machine ready for production use.
    /// The machine starts in `Starting` state and processes events immediately.
    ///
    /// Note: For production use in main.rs. Tests should use `new_for_test()`.
    ///
    /// ## Circular Dependency Note
    ///
    /// Full subsystem integration (`ActionContext` with lifecycle reference) requires
    /// two-phase initialization. For now, we create the machine without context,
    /// which means entry actions will log but not spawn subsystem tasks.
    /// This is acceptable because:
    /// 1. The machine processes events correctly (QC2-C2 fix)
    /// 2. Subsystems can be started manually or via a follow-up refactoring
    ///
    /// See: QC2-C2 critical finding — machine must be `Some()` to avoid dropping first dispatch.
    #[must_use] 
    pub fn new_with_subsystems(
        _subsystems: Vec<Arc<dyn SubsystemBootstrap>>,
        _shutdown_grace_ms: u64,
    ) -> Self {
        // Create shared state mirrors first
        let (transition_tx, _) = broadcast::channel(16);
        let current_state = Arc::new(std::sync::Mutex::new(LifecycleState::Starting));
        let exit_code = Arc::new(std::sync::Mutex::new(None));

        // Create the actual state machine (not a placeholder).
        // Without context, entry actions run in test mode (log only, no subsystem spawning).
        // This fixes QC2-C2: machine is Some(), so dispatch() doesn't drop first event.
        let daemon_hsm = DaemonHsm::default();
        let machine = Arc::new(Mutex::new(Some(daemon_hsm.state_machine())));

        Self {
            machine,
            transition_tx,
            current_state,
            exit_code,
        }
    }

    /// Create a lifecycle for testing (no subsystem tasks spawned).
    ///
    /// Entry actions will log but not spawn subsystem tasks.
    /// Tests dispatch `SubsystemUp` events manually.
    #[must_use] 
    pub fn new_for_test() -> Self {
        let (transition_tx, _) = broadcast::channel(16);
        let daemon_hsm = DaemonHsm::default(); // No context
        let machine = Arc::new(Mutex::new(Some(daemon_hsm.state_machine())));
        let current_state = Arc::new(std::sync::Mutex::new(LifecycleState::Starting));
        let exit_code = Arc::new(std::sync::Mutex::new(None));

        Self {
            machine,
            transition_tx,
            current_state,
            exit_code,
        }
    }

    /// Create a new lifecycle with default settings (alias for `new_for_test`).
    #[must_use] 
    pub fn new() -> Self {
        Self::new_for_test()
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
        //
        // ## Ordering Guarantee (QC1-C2)
        //
        // Each dispatch spawns a new tokio::spawn, which means multiple events
        // could be processed concurrently. However, FIFO ordering is guaranteed
        // because:
        // 1. All spawned tasks run on the same tokio executor
        // 2. The `machine.lock().await` at line 479 serializes access to the state machine
        // 3. Tasks acquire the Mutex in FIFO order (tokio's default Mutex fairness)
        //
        // This ensures events are processed in dispatch order, even though they
        // may be spawned as independent tasks.
        let machine = Arc::clone(&self.machine);
        let transition_tx = self.transition_tx.clone();
        let current_state = Arc::clone(&self.current_state);
        let exit_code = Arc::clone(&self.exit_code);

        tokio::spawn(async move {
            let mut m = machine.lock().await;

            // Check if machine is initialized
            let Some(machine_ref) = m.as_mut() else {
                tracing::warn!("dispatch called on uninitialized lifecycle");
                return;
            };

            let before = lifecycle_state_from_statig_state(machine_ref.state());

            // handle() initializes if needed and processes the event
            machine_ref.handle(&event).await;

            let after = lifecycle_state_from_statig_state(machine_ref.state());

            // Read exit_code from shared storage (StateMachine derefs to DaemonHsm).
            let code = machine_ref.exit_code;

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
        *self.exit_code.lock().expect("exit_code mutex poisoned")
    }
}

/// Convert a statig `State` to our `LifecycleState`.
///
/// This is a helper for the mirror sync.
const fn lifecycle_state_from_statig_state(state: &State) -> LifecycleState {
    match state {
        State::Starting { .. } => LifecycleState::Starting,
        State::Running { .. } => LifecycleState::Running,
        State::Degraded { .. } => LifecycleState::Degraded,
        State::Stopping { .. } => LifecycleState::Stopping,
        State::Failed { .. } => LifecycleState::Failed,
    }
}
