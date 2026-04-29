//! `AgentSlot` — state machine for a single ACP agent subprocess within a worker.
//!
//! In V1.7, the actual ACP connection is stubbed. T3 will wire up the real
//! `AcpSdkAdapter`. This module provides the state machine and channel-based
//! communication pattern.
//!
//! Design: `orchestration-engine-v1.md` §6.3.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Runtime config for an agent slot.
#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// Unique session identifier for this agent.
    pub session_id: String,
    /// ACP registry agent ID (e.g., "claude-sonnet-4-20250514").
    pub acp_agent_id: String,
    /// Optional role assignment (e.g., "writer", "editor").
    pub role: Option<String>,
    /// Optional model override for the agent.
    pub model: Option<String>,
    /// Optional system prompt content (T8).
    /// When present, injected as the first message in the ACP session.
    /// Daemon reads from role's `system_prompt_file` and sends via IPC.
    pub system_prompt: Option<String>,
}

impl AgentConfig {
    /// Create a minimal config with required fields.
    #[must_use] 
    pub const fn new(session_id: String, acp_agent_id: String) -> Self {
        Self {
            session_id,
            acp_agent_id,
            role: None,
            model: None,
            system_prompt: None,
        }
    }

    /// Builder-style role assignment.
    #[must_use] 
    pub fn with_role(mut self, role: String) -> Self {
        self.role = Some(role);
        self
    }

    /// Builder-style model override.
    #[must_use] 
    pub fn with_model(mut self, model: String) -> Self {
        self.model = Some(model);
        self
    }

    /// Builder-style system prompt (T8).
    #[must_use] 
    pub fn with_system_prompt(mut self, system_prompt: String) -> Self {
        self.system_prompt = Some(system_prompt);
        self
    }
}

/// State machine for an agent slot.
///
/// Represents the lifecycle of an ACP agent subprocess managed by the worker.
/// Transitions are driven by IPC commands from the daemon and internal events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentSlotState {
    /// Slot created, awaiting initialization.
    Initializing,
    /// Agent subprocess ready to receive prompts.
    Ready,
    /// Agent is actively processing a prompt.
    Prompting,
    /// Slot encountered an error (stores last error message).
    Error(String),
    /// Graceful shutdown requested, agent is stopping.
    Stopping,
    /// Slot fully stopped, no longer usable.
    Stopped,
}

impl AgentSlotState {
    /// Check if the slot is in an error state.
    #[must_use] 
    pub const fn is_error(&self) -> bool {
        matches!(self, Self::Error(_))
    }

    /// Check if the slot is ready to accept prompts.
    #[must_use] 
    pub const fn is_ready(&self) -> bool {
        matches!(self, Self::Ready)
    }

    /// Check if the slot is currently prompting.
    #[must_use] 
    pub const fn is_prompting(&self) -> bool {
        matches!(self, Self::Prompting)
    }

    /// Check if the slot is stopped or stopping.
    #[must_use] 
    pub const fn is_terminal(&self) -> bool {
        matches!(self, Self::Stopping | Self::Stopped)
    }
}

/// Health info for a slot, exposed via `worker/health` IPC method.
#[derive(Debug, Clone)]
pub struct SlotHealth {
    /// Current state machine state.
    pub state: AgentSlotState,
    /// Milliseconds since slot creation.
    pub uptime_ms: u64,
    /// Last error message if in Error state.
    pub last_error: Option<String>,
}

impl SlotHealth {
    /// Create health info from a slot.
    #[must_use] 
    pub const fn new(state: AgentSlotState, uptime_ms: u64, last_error: Option<String>) -> Self {
        Self {
            state,
            uptime_ms,
            last_error,
        }
    }

    /// Check if the slot is healthy (Ready or Prompting).
    #[must_use] 
    pub const fn is_healthy(&self) -> bool {
        self.state.is_ready() || self.state.is_prompting()
    }
}

/// `AgentSlot` manages one ACP agent subprocess within a worker.
///
/// In V1.7, the actual ACP connection is stubbed. T3 will wire up
/// the real `AcpSdkAdapter`. This abstraction provides the state machine
/// and channel-based communication pattern.
///
/// # State Transitions
///
/// ```text
/// Initializing → Ready → Prompting → Ready → Stopping → Stopped
///                    ↘ Error ↗
/// ```
///
/// The slot can enter Error state from any non-terminal state.
/// From Error, it can transition to Ready (recovery) or Stopping.
///
/// # System Prompt (T8)
///
/// When `config.system_prompt` is set, the content is injected as the first
/// system message when the ACP session is created. This allows role-based
/// persona customization without modifying preset prompt templates.
pub struct AgentSlot {
    /// Runtime configuration for this agent.
    config: AgentConfig,
    /// Current state machine state (shared for IPC access).
    state: Arc<Mutex<AgentSlotState>>,
    /// Timestamp when the slot was created.
    start_time: Instant,
    /// Last error message (if any).
    last_error: Arc<Mutex<Option<String>>>,
    /// Shutdown flag set by `request_shutdown` or Drop.
    shutdown_requested: AtomicBool,
}

impl AgentSlot {
    /// Create a new agent slot in Initializing state.
    ///
    /// The slot starts in `Initializing` state. Call `mark_ready()` after
    /// the agent subprocess is confirmed ready.
    #[must_use] 
    pub fn new(config: AgentConfig) -> Self {
        Self {
            config,
            state: Arc::new(Mutex::new(AgentSlotState::Initializing)),
            start_time: Instant::now(),
            last_error: Arc::new(Mutex::new(None)),
            shutdown_requested: AtomicBool::new(false),
        }
    }

    /// Get the current state machine state.
    ///
    /// Returns a clone of the state. For atomic operations, use
    /// `mark_*` methods.
    pub fn state(&self) -> AgentSlotState {
        self.state
            .lock().map_or_else(|_| AgentSlotState::Error("state lock poisoned".to_string()), |s| s.clone())
    }

    /// Get health info for this slot.
    ///
    /// Combines state, uptime, and last error for `worker/health` response.
    pub fn health(&self) -> SlotHealth {
        let state = self.state();
        // Uptime in milliseconds — cast is safe since uptime from start_time
        // is bounded by process lifetime (max ~584 years for u64 milliseconds)
        #[allow(clippy::cast_possible_truncation)]
        let uptime_ms = self.start_time.elapsed().as_millis() as u64;
        let last_error = self.last_error.lock().map_or(None, |e| e.clone());
        SlotHealth::new(state, uptime_ms, last_error)
    }

    /// Get the session ID for this agent.
    pub fn session_id(&self) -> &str {
        &self.config.session_id
    }

    /// Get the ACP agent ID for this agent.
    pub fn acp_agent_id(&self) -> &str {
        &self.config.acp_agent_id
    }

    /// Get the role assignment (if set).
    pub fn role(&self) -> Option<&str> {
        self.config.role.as_deref()
    }

    /// Get the model override (if set).
    pub fn model(&self) -> Option<&str> {
        self.config.model.as_deref()
    }

    /// Get the system prompt content (if set) — T8.
    /// This is injected as the first message when creating the ACP session.
    pub fn system_prompt(&self) -> Option<&str> {
        self.config.system_prompt.as_deref()
    }

    /// Transition to Ready state.
    ///
    /// Called after agent subprocess is confirmed ready (e.g., after
    /// successful ACP handshake).
    ///
    /// # Errors
    ///
    /// If the state lock is poisoned, silently returns without change.
    pub fn mark_ready(&self) {
        if let Ok(mut state) = self.state.lock() {
            *state = AgentSlotState::Ready;
        }
    }

    /// Transition to Error state with a message.
    ///
    /// Also stores the error message in `last_error` for health reporting.
    ///
    /// # Errors
    ///
    /// If either lock is poisoned, silently returns without change.
    pub fn mark_error(&self, msg: String) {
        if let Ok(mut state) = self.state.lock() {
            *state = AgentSlotState::Error(msg.clone());
        }
        if let Ok(mut last_error) = self.last_error.lock() {
            *last_error = Some(msg);
        }
    }

    /// Transition to Error state due to a crash.
    ///
    /// A dedicated method for crash-induced errors (as opposed to general
    /// `mark_error`). Prepends "[crash]" to the message for traceability.
    /// Used by the worker's subprocess supervisor and `simulate_crash` tests.
    ///
    /// # Errors
    ///
    /// If either lock is poisoned, silently returns without change.
    pub fn mark_crashed(&self, error_msg: &str) {
        let msg = format!("[crash] {error_msg}");
        if let Ok(mut state) = self.state.lock() {
            *state = AgentSlotState::Error(msg.clone());
        }
        if let Ok(mut last_error) = self.last_error.lock() {
            *last_error = Some(msg);
        }
    }

    /// Transition to Prompting state.
    ///
    /// Called when agent starts processing a prompt via `worker/acp_prompt`.
    ///
    /// # Errors
    ///
    /// If the state lock is poisoned, silently returns without change.
    pub fn mark_prompting(&self) {
        if let Ok(mut state) = self.state.lock() {
            *state = AgentSlotState::Prompting;
        }
    }

    /// Transition from Prompting back to Ready.
    ///
    /// Called after prompt processing completes successfully.
    ///
    /// # Errors
    ///
    /// If the state lock is poisoned, silently returns without change.
    pub fn mark_ready_from_prompt(&self) {
        if let Ok(mut state) = self.state.lock() {
            // Only transition if currently in Prompting; avoid overriding Error.
            if matches!(*state, AgentSlotState::Prompting) {
                *state = AgentSlotState::Ready;
            }
        }
    }

    /// Request graceful shutdown.
    ///
    /// Sets the shutdown flag and transitions to Stopping state.
    /// The actual shutdown logic is handled by the worker loop.
    ///
    /// # Errors
    ///
    /// If the state lock is poisoned, silently returns without change.
    pub fn request_shutdown(&self) {
        self.shutdown_requested.store(true, Ordering::Release);
        if let Ok(mut state) = self.state.lock() {
            // Don't override Stopped state.
            if !matches!(*state, AgentSlotState::Stopped) {
                *state = AgentSlotState::Stopping;
            }
        }
    }

    /// Check if shutdown has been requested.
    pub fn is_shutdown_requested(&self) -> bool {
        self.shutdown_requested.load(Ordering::Acquire)
    }

    /// Transition to Stopped state.
    ///
    /// Called after the agent subprocess has terminated.
    ///
    /// # Errors
    ///
    /// If the state lock is poisoned, silently returns without change.
    pub fn mark_stopped(&self) {
        if let Ok(mut state) = self.state.lock() {
            *state = AgentSlotState::Stopped;
        }
    }

    /// Test helper: simulate a subprocess crash.
    ///
    /// Calls [`mark_crashed`] with the given message, transitioning the slot
    /// to the `Error` state. This allows tests to exercise crash detection
    /// without spawning real child processes.
    #[cfg(test)]
    pub fn simulate_crash(&self, error_msg: &str) {
        self.mark_crashed(error_msg);
    }
}

impl Drop for AgentSlot {
    fn drop(&mut self) {
        // Ensure shutdown flag is set on drop.
        self.request_shutdown();
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> AgentConfig {
        AgentConfig {
            session_id: "sess_123".to_string(),
            acp_agent_id: "agent_456".to_string(),
            role: Some("writer".to_string()),
            model: Some("claude-3".to_string()),
            system_prompt: Some("You are a creative writer.".to_string()),
        }
    }

    #[test]
    fn new_slot_starts_initializing() {
        let slot = AgentSlot::new(test_config());
        assert_eq!(slot.state(), AgentSlotState::Initializing);
        assert!(!slot.is_shutdown_requested());
    }

    #[test]
    fn mark_ready_transitions() {
        let slot = AgentSlot::new(test_config());
        slot.mark_ready();
        assert_eq!(slot.state(), AgentSlotState::Ready);
        assert!(slot.state().is_ready());
    }

    #[test]
    fn mark_error_transitions() {
        let slot = AgentSlot::new(test_config());
        slot.mark_ready();
        slot.mark_error("test error".to_string());
        assert!(slot.state().is_error());
        let health = slot.health();
        assert_eq!(health.last_error, Some("test error".to_string()));
    }

    #[test]
    fn mark_prompting_transitions() {
        let slot = AgentSlot::new(test_config());
        slot.mark_ready();
        slot.mark_prompting();
        assert_eq!(slot.state(), AgentSlotState::Prompting);
        assert!(slot.state().is_prompting());
    }

    #[test]
    fn mark_ready_from_prompt_returns_to_ready() {
        let slot = AgentSlot::new(test_config());
        slot.mark_ready();
        slot.mark_prompting();
        slot.mark_ready_from_prompt();
        assert_eq!(slot.state(), AgentSlotState::Ready);
    }

    #[test]
    fn mark_ready_from_prompt_does_not_override_error() {
        let slot = AgentSlot::new(test_config());
        slot.mark_ready();
        slot.mark_error("test error".to_string());
        slot.mark_ready_from_prompt();
        // Should still be in Error state.
        assert!(slot.state().is_error());
    }

    #[test]
    fn shutdown_requested() {
        let slot = AgentSlot::new(test_config());
        assert!(!slot.is_shutdown_requested());
        slot.request_shutdown();
        assert!(slot.is_shutdown_requested());
        assert_eq!(slot.state(), AgentSlotState::Stopping);
    }

    #[test]
    fn health_reflects_state() {
        let slot = AgentSlot::new(test_config());
        let health = slot.health();
        assert_eq!(health.state, AgentSlotState::Initializing);
        assert!(health.uptime_ms < u64::MAX);
        assert!(health.last_error.is_none());

        slot.mark_ready();
        let health = slot.health();
        assert!(health.is_healthy());

        slot.mark_error("oops".to_string());
        let health = slot.health();
        assert!(!health.is_healthy());
        assert_eq!(health.last_error, Some("oops".to_string()));
    }

    #[test]
    fn drop_requests_shutdown() {
        let slot = AgentSlot::new(test_config());
        let state_arc = Arc::clone(&slot.state);
        drop(slot);
        // After drop, shutdown should have been requested.
        // We can't check is_shutdown_requested() directly since slot is gone,
        // but we can check the state was set to Stopping via the arc.
        let state = state_arc
            .lock()
            .map_or(AgentSlotState::Stopped, |s| s.clone());
        assert!(state.is_terminal());
    }

    #[test]
    fn config_builder() {
        let config = AgentConfig::new("sess_abc".to_string(), "agent_xyz".to_string())
            .with_role("editor".to_string())
            .with_model("gpt-4".to_string())
            .with_system_prompt("You are an editor.".to_string());

        assert_eq!(config.session_id, "sess_abc");
        assert_eq!(config.acp_agent_id, "agent_xyz");
        assert_eq!(config.role, Some("editor".to_string()));
        assert_eq!(config.model, Some("gpt-4".to_string()));
        assert_eq!(config.system_prompt, Some("You are an editor.".to_string()));
    }

    #[test]
    fn slot_accessors() {
        let slot = AgentSlot::new(test_config());
        assert_eq!(slot.session_id(), "sess_123");
        assert_eq!(slot.acp_agent_id(), "agent_456");
        assert_eq!(slot.role(), Some("writer"));
        assert_eq!(slot.model(), Some("claude-3"));
        assert_eq!(slot.system_prompt(), Some("You are a creative writer."));
    }

    #[test]
    fn slot_accessors_without_system_prompt() {
        let config = AgentConfig::new("sess_no_sp".to_string(), "agent_sp".to_string())
            .with_role("researcher".to_string());
        let slot = AgentSlot::new(config);
        assert_eq!(slot.session_id(), "sess_no_sp");
        assert_eq!(slot.role(), Some("researcher"));
        assert!(slot.system_prompt().is_none());
    }

    #[test]
    fn state_terminal_check() {
        assert!(!AgentSlotState::Initializing.is_terminal());
        assert!(!AgentSlotState::Ready.is_terminal());
        assert!(!AgentSlotState::Prompting.is_terminal());
        assert!(!AgentSlotState::Error("err".to_string()).is_terminal());
        assert!(AgentSlotState::Stopping.is_terminal());
        assert!(AgentSlotState::Stopped.is_terminal());
    }

    #[test]
    fn mark_stopped_final_state() {
        let slot = AgentSlot::new(test_config());
        slot.mark_ready();
        slot.request_shutdown();
        slot.mark_stopped();
        assert_eq!(slot.state(), AgentSlotState::Stopped);
        assert!(slot.state().is_terminal());
    }

    #[test]
    fn request_shutdown_from_error() {
        let slot = AgentSlot::new(test_config());
        slot.mark_error("fatal".to_string());
        slot.request_shutdown();
        assert_eq!(slot.state(), AgentSlotState::Stopping);
        assert!(slot.is_shutdown_requested());
    }

#[test]
fn mark_crashed_transitions_to_error() {
    let slot = AgentSlot::new(test_config());
        slot.mark_crashed("segfault in agent subprocess");
        let state = slot.state();
        assert!(state.is_error());
        // The error message should be prefixed with [crash].
        if let AgentSlotState::Error(msg) = state {
            assert!(msg.starts_with("[crash]"));
            assert!(msg.contains("segfault in agent subprocess"));
        } else {
            panic!("Expected Error state after mark_crashed");
        }
        let health = slot.health();
        assert_eq!(
            health.last_error,
            Some("[crash] segfault in agent subprocess".to_string())
        );
    }

    #[test]
    fn simulate_crash_test_helper() {
        let slot = AgentSlot::new(test_config());
        slot.mark_ready();
        slot.simulate_crash("OOM killed");
        assert!(slot.state().is_error());
        let health = slot.health();
        assert!(health
            .last_error
            .as_ref()
            .expect("error")
            .contains("[crash] OOM killed"));
    }
}
