//! Session registry and state machine.
//!
//! Manages host-level session lifecycle with explicit op substates per the compass
//! §Session state machine. Enforces one active op per session in Wave 1.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::capability::model::{
    CapabilityDescriptor, HostEvent, SessionStopReason, SessionStoppedEvent,
};
use crate::error::{HostError, HostResult};
use crate::ids::{HostOperationId, HostSessionId, ProviderId};

/// Session state in the host state machine.
///
/// Per compass §Session state machine:
/// ```text
/// Created → Starting → Ready → Busy(op_id) → Ready
/// Busy(op_id) → Cancelling(op_id) → Ready | Stopped
/// Starting | Busy | Cancelling | Stopping → ErrorRecoverable | ErrorTerminal
/// ErrorRecoverable → Starting (restart policy permits)
/// ErrorTerminal → Stopped
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    /// Session created, not yet started.
    Created,
    /// Session starting (provider launch in progress).
    Starting,
    /// Session ready for operations.
    Ready,
    /// Session busy with an operation.
    Busy(HostOperationId),
    /// Session cancelling an operation.
    Cancelling(HostOperationId),
    /// Session stopping (shutdown in progress).
    Stopping,
    /// Recoverable error state.
    ErrorRecoverable,
    /// Terminal error state — new session required.
    ErrorTerminal,
    /// Session stopped (terminal).
    Stopped,
}

impl SessionState {
    /// Whether the session is in a terminal state (no further transitions).
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(self, Self::Stopped)
    }

    /// Whether the session can accept a new operation.
    #[must_use]
    pub const fn can_exec(&self) -> bool {
        matches!(self, Self::Ready)
    }

    /// Whether the session has an active operation.
    #[must_use]
    pub const fn is_busy(&self) -> bool {
        matches!(self, Self::Busy(_) | Self::Cancelling(_))
    }

    /// Get the active operation ID, if any.
    #[must_use]
    pub const fn active_op_id(&self) -> Option<&HostOperationId> {
        match self {
            Self::Busy(op_id) | Self::Cancelling(op_id) => Some(op_id),
            _ => None,
        }
    }
}

/// A host-managed session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostSession {
    /// Session ID.
    pub id: HostSessionId,
    /// Provider ID for this session.
    pub provider_id: ProviderId,
    /// Current session state.
    pub state: SessionState,
    /// When the session was created.
    pub created_at: DateTime<Utc>,
    /// Active operation ID (if in Busy/Cancelling state).
    pub active_op_id: Option<HostOperationId>,
    /// Negotiated capabilities for this session.
    pub negotiated_capabilities: CapabilityDescriptor,
}

/// Registry of all active host sessions.
#[derive(Debug, Clone, Default)]
pub struct SessionRegistry {
    /// Sessions indexed by ID.
    sessions: HashMap<HostSessionId, HostSession>,
}

/// Result of a state transition that may produce a terminal event.
#[derive(Debug, Clone)]
pub enum TransitionResult {
    /// Transition succeeded with no terminal event.
    Ok,
    /// Transition succeeded and produced a terminal event (emitted when leaving Busy).
    TerminalEvent(HostEvent),
}

impl SessionRegistry {
    /// Create an empty session registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a new session in Created state.
    pub fn register(
        &mut self,
        provider_id: ProviderId,
        capabilities: CapabilityDescriptor,
    ) -> HostSessionId {
        let id = HostSessionId::new();
        let session = HostSession {
            id: id.clone(),
            provider_id,
            state: SessionState::Created,
            created_at: Utc::now(),
            active_op_id: None,
            negotiated_capabilities: capabilities,
        };
        self.sessions.insert(id.clone(), session);
        id
    }

    /// Transition a session to Starting.
    ///
    /// Valid from: Created, `ErrorRecoverable`.
    ///
    /// # Errors
    ///
    /// Returns `HostError` if the session is not in a valid source state or not found.
    pub fn transition_to_starting(&mut self, session_id: &HostSessionId) -> HostResult<()> {
        let session = self.get_session_mut(session_id)?;
        match &session.state {
            SessionState::Created | SessionState::ErrorRecoverable => {
                session.state = SessionState::Starting;
                Ok(())
            }
            _ => Err(HostError::internal(format!(
                "cannot transition to Starting from {:?} (session {})",
                session.state, session_id
            ))),
        }
    }

    /// Transition a session to Ready.
    ///
    /// Valid from: Starting.
    ///
    /// # Errors
    ///
    /// Returns `HostError` if the session is not in `Starting` state or not found.
    pub fn transition_to_ready(&mut self, session_id: &HostSessionId) -> HostResult<()> {
        let session = self.get_session_mut(session_id)?;
        match &session.state {
            SessionState::Starting => {
                session.state = SessionState::Ready;
                Ok(())
            }
            _ => Err(HostError::internal(format!(
                "cannot transition to Ready from {:?} (session {})",
                session.state, session_id
            ))),
        }
    }

    /// Transition a session to `Busy` with an operation.
    ///
    /// Valid from: Ready. Enforces one active op per session.
    /// Returns the transition result; every path out of `Busy` must emit one terminal event.
    ///
    /// # Errors
    ///
    /// Returns `HostError` if the session is not in `Ready` state or not found.
    pub fn transition_to_busy(
        &mut self,
        session_id: &HostSessionId,
        op_id: HostOperationId,
    ) -> HostResult<()> {
        let session = self.get_session_mut(session_id)?;
        match &session.state {
            SessionState::Ready => {
                session.state = SessionState::Busy(op_id.clone());
                session.active_op_id = Some(op_id);
                Ok(())
            }
            _ => Err(HostError::internal(format!(
                "cannot transition to Busy from {:?} (session {})",
                session.state, session_id
            ))),
        }
    }

    /// Transition from `Busy` to Ready (operation completed).
    ///
    /// Produces a terminal event acknowledgement. The caller is responsible
    /// for emitting the actual `OpFinished` or `OpFailed` event.
    ///
    /// # Errors
    ///
    /// Returns `HostError` if the session is not `Busy(op_id)` or not found.
    pub fn transition_busy_to_ready(
        &mut self,
        session_id: &HostSessionId,
        op_id: &HostOperationId,
    ) -> HostResult<TransitionResult> {
        let session = self.get_session_mut(session_id)?;
        match &session.state {
            SessionState::Busy(ref current_op_id) if current_op_id == op_id => {
                session.state = SessionState::Ready;
                session.active_op_id = None;
                // Caller emits the actual terminal event
                Ok(TransitionResult::Ok)
            }
            _ => Err(HostError::internal(format!(
                "cannot transition Busy→Ready from {:?} (session {}, op {})",
                session.state, session_id, op_id
            ))),
        }
    }

    /// Transition from `Busy` to `Cancelling`.
    ///
    /// Valid from: `Busy(op_id)` where `op_id` matches.
    ///
    /// # Errors
    ///
    /// Returns `HostError` if the session is not `Busy(op_id)` or not found.
    pub fn transition_to_cancelling(
        &mut self,
        session_id: &HostSessionId,
        op_id: &HostOperationId,
    ) -> HostResult<()> {
        let session = self.get_session_mut(session_id)?;
        match &session.state {
            SessionState::Busy(ref current_op_id) if current_op_id == op_id => {
                session.state = SessionState::Cancelling(op_id.clone());
                Ok(())
            }
            _ => Err(HostError::internal(format!(
                "cannot transition to Cancelling from {:?} (session {}, op {})",
                session.state, session_id, op_id
            ))),
        }
    }

    /// Transition from `Cancelling` to Ready (cancel acknowledged).
    ///
    /// Produces a terminal event: `OperationCancelled`.
    ///
    /// # Errors
    ///
    /// Returns `HostError` if the session is not `Cancelling(op_id)` or not found.
    pub fn transition_cancelling_to_ready(
        &mut self,
        session_id: &HostSessionId,
        op_id: &HostOperationId,
    ) -> HostResult<TransitionResult> {
        let session = self.get_session_mut(session_id)?;
        match &session.state {
            SessionState::Cancelling(ref current_op_id) if current_op_id == op_id => {
                session.state = SessionState::Ready;
                session.active_op_id = None;
                // The caller emits OpFinished or OpFailed terminal event
                Ok(TransitionResult::Ok)
            }
            _ => Err(HostError::internal(format!(
                "cannot transition Cancelling→Ready from {:?} (session {})",
                session.state, session_id
            ))),
        }
    }

    /// Transition to Stopping.
    ///
    /// Valid from: Ready, Starting, `Cancelling`.
    ///
    /// # Errors
    ///
    /// Returns `HostError` if the session is not in a valid source state or not found.
    pub fn transition_to_stopping(&mut self, session_id: &HostSessionId) -> HostResult<()> {
        let session = self.get_session_mut(session_id)?;
        match &session.state {
            SessionState::Ready | SessionState::Starting | SessionState::Cancelling(_) => {
                session.state = SessionState::Stopping;
                Ok(())
            }
            _ => Err(HostError::internal(format!(
                "cannot transition to Stopping from {:?} (session {})",
                session.state, session_id
            ))),
        }
    }

    /// Transition to Stopped.
    ///
    /// Valid from: Stopping, `ErrorTerminal`.
    ///
    /// # Errors
    ///
    /// Returns `HostError` if the session is not in a valid source state or not found.
    pub fn transition_to_stopped(
        &mut self,
        session_id: &HostSessionId,
        reason: SessionStopReason,
    ) -> HostResult<HostEvent> {
        let session = self.get_session_mut(session_id)?;
        match &session.state {
            SessionState::Stopping | SessionState::ErrorTerminal => {
                session.state = SessionState::Stopped;
                session.active_op_id = None;
                Ok(HostEvent::SessionStopped(SessionStoppedEvent {
                    session_id: session_id.clone(),
                    reason,
                }))
            }
            _ => Err(HostError::internal(format!(
                "cannot transition to Stopped from {:?} (session {})",
                session.state, session_id
            ))),
        }
    }

    /// Transition to `ErrorRecoverable`.
    ///
    /// Valid from: Starting, `Busy`, `Cancelling`, Stopping.
    ///
    /// # Errors
    ///
    /// Returns `HostError` if the session is not in a valid source state or not found.
    pub fn transition_to_error_recoverable(
        &mut self,
        session_id: &HostSessionId,
    ) -> HostResult<Option<HostOperationId>> {
        let session = self.get_session_mut(session_id)?;
        let op_id = session.active_op_id.clone();
        match &session.state {
            SessionState::Starting
            | SessionState::Busy(_)
            | SessionState::Cancelling(_)
            | SessionState::Stopping => {
                session.state = SessionState::ErrorRecoverable;
                session.active_op_id = None;
                Ok(op_id) // Return op_id so caller can emit terminal event for the op
            }
            _ => Err(HostError::internal(format!(
                "cannot transition to ErrorRecoverable from {:?} (session {})",
                session.state, session_id
            ))),
        }
    }

    /// Transition to `ErrorTerminal`.
    ///
    /// Valid from: Starting, `Busy`, `Cancelling`, Stopping.
    ///
    /// # Errors
    ///
    /// Returns `HostError` if the session is not in a valid source state or not found.
    pub fn transition_to_error_terminal(
        &mut self,
        session_id: &HostSessionId,
    ) -> HostResult<Option<HostOperationId>> {
        let session = self.get_session_mut(session_id)?;
        let op_id = session.active_op_id.clone();
        match &session.state {
            SessionState::Starting
            | SessionState::Busy(_)
            | SessionState::Cancelling(_)
            | SessionState::Stopping => {
                session.state = SessionState::ErrorTerminal;
                session.active_op_id = None;
                Ok(op_id) // Return op_id so caller can emit terminal event for the op
            }
            _ => Err(HostError::internal(format!(
                "cannot transition to ErrorTerminal from {:?} (session {})",
                session.state, session_id
            ))),
        }
    }

    /// Get a session by ID.
    #[must_use]
    pub fn get(&self, session_id: &HostSessionId) -> Option<&HostSession> {
        self.sessions.get(session_id)
    }

    /// Get current session count.
    #[must_use]
    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    /// Whether the registry is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }

    /// Iterate over all sessions.
    pub fn iter(&self) -> impl Iterator<Item = &HostSession> {
        self.sessions.values()
    }

    /// Remove a stopped session.
    ///
    /// # Errors
    ///
    /// Returns `HostError` if the session is not in a terminal state or not found.
    pub fn remove_stopped(&mut self, session_id: &HostSessionId) -> HostResult<HostSession> {
        let session = self.get_session(session_id)?;
        if !session.state.is_terminal() {
            return Err(HostError::internal(format!(
                "cannot remove non-terminal session (state: {:?})",
                session.state
            )));
        }
        self.sessions
            .remove(session_id)
            .ok_or_else(|| HostError::internal(format!("session {session_id} not found")))
    }

    fn get_session(&self, session_id: &HostSessionId) -> HostResult<&HostSession> {
        self.sessions
            .get(session_id)
            .ok_or_else(|| HostError::internal(format!("session {session_id} not found")))
    }

    fn get_session_mut(&mut self, session_id: &HostSessionId) -> HostResult<&mut HostSession> {
        self.sessions
            .get_mut(session_id)
            .ok_or_else(|| HostError::internal(format!("session {session_id} not found")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_caps() -> CapabilityDescriptor {
        CapabilityDescriptor::acp_full()
    }

    fn register_session(registry: &mut SessionRegistry) -> HostSessionId {
        registry.register(ProviderId::new("test-provider"), test_caps())
    }

    #[test]
    fn register_creates_session_in_created_state() {
        let mut registry = SessionRegistry::new();
        let id = register_session(&mut registry);
        let session = registry.get(&id).expect("session should exist");
        assert_eq!(session.state, SessionState::Created);
    }

    #[test]
    fn valid_transition_created_to_starting() {
        let mut registry = SessionRegistry::new();
        let id = register_session(&mut registry);
        registry
            .transition_to_starting(&id)
            .expect("should succeed");
        assert_eq!(registry.get(&id).unwrap().state, SessionState::Starting);
    }

    #[test]
    fn valid_transition_starting_to_ready() {
        let mut registry = SessionRegistry::new();
        let id = register_session(&mut registry);
        registry.transition_to_starting(&id).unwrap();
        registry.transition_to_ready(&id).unwrap();
        assert_eq!(registry.get(&id).unwrap().state, SessionState::Ready);
    }

    #[test]
    fn valid_transition_ready_to_busy() {
        let mut registry = SessionRegistry::new();
        let id = register_session(&mut registry);
        registry.transition_to_starting(&id).unwrap();
        registry.transition_to_ready(&id).unwrap();

        let op_id = HostOperationId::new();
        registry.transition_to_busy(&id, op_id.clone()).unwrap();
        assert!(matches!(
            registry.get(&id).unwrap().state,
            SessionState::Busy(_)
        ));
    }

    #[test]
    fn valid_transition_busy_to_ready() {
        let mut registry = SessionRegistry::new();
        let id = register_session(&mut registry);
        registry.transition_to_starting(&id).unwrap();
        registry.transition_to_ready(&id).unwrap();

        let op_id = HostOperationId::new();
        registry.transition_to_busy(&id, op_id.clone()).unwrap();
        registry.transition_busy_to_ready(&id, &op_id).unwrap();
        assert_eq!(registry.get(&id).unwrap().state, SessionState::Ready);
    }

    #[test]
    fn valid_cancel_transition() {
        let mut registry = SessionRegistry::new();
        let id = register_session(&mut registry);
        registry.transition_to_starting(&id).unwrap();
        registry.transition_to_ready(&id).unwrap();

        let op_id = HostOperationId::new();
        registry.transition_to_busy(&id, op_id.clone()).unwrap();
        registry.transition_to_cancelling(&id, &op_id).unwrap();
        assert!(matches!(
            registry.get(&id).unwrap().state,
            SessionState::Cancelling(_)
        ));

        registry
            .transition_cancelling_to_ready(&id, &op_id)
            .unwrap();
        assert_eq!(registry.get(&id).unwrap().state, SessionState::Ready);
    }

    #[test]
    fn invalid_transition_created_to_ready() {
        let mut registry = SessionRegistry::new();
        let id = register_session(&mut registry);
        assert!(registry.transition_to_ready(&id).is_err());
    }

    #[test]
    fn invalid_transition_ready_to_starting() {
        let mut registry = SessionRegistry::new();
        let id = register_session(&mut registry);
        registry.transition_to_starting(&id).unwrap();
        registry.transition_to_ready(&id).unwrap();
        assert!(registry.transition_to_starting(&id).is_err());
    }

    #[test]
    fn one_op_per_session_enforced() {
        let mut registry = SessionRegistry::new();
        let id = register_session(&mut registry);
        registry.transition_to_starting(&id).unwrap();
        registry.transition_to_ready(&id).unwrap();

        let op1 = HostOperationId::new();
        registry.transition_to_busy(&id, op1).unwrap();

        // Trying to start another op while busy should fail
        let op2 = HostOperationId::new();
        assert!(registry.transition_to_busy(&id, op2).is_err());
    }

    #[test]
    fn error_recoverable_from_starting() {
        let mut registry = SessionRegistry::new();
        let id = register_session(&mut registry);
        registry.transition_to_starting(&id).unwrap();
        let result = registry.transition_to_error_recoverable(&id).unwrap();
        assert!(result.is_none()); // No active op
        assert_eq!(
            registry.get(&id).unwrap().state,
            SessionState::ErrorRecoverable
        );
    }

    #[test]
    fn error_recoverable_from_busy() {
        let mut registry = SessionRegistry::new();
        let id = register_session(&mut registry);
        registry.transition_to_starting(&id).unwrap();
        registry.transition_to_ready(&id).unwrap();

        let op_id = HostOperationId::new();
        registry.transition_to_busy(&id, op_id.clone()).unwrap();
        let returned_op = registry.transition_to_error_recoverable(&id).unwrap();
        assert_eq!(returned_op, Some(op_id));
        assert_eq!(
            registry.get(&id).unwrap().state,
            SessionState::ErrorRecoverable
        );
    }

    #[test]
    fn error_terminal_to_stopped() {
        let mut registry = SessionRegistry::new();
        let id = register_session(&mut registry);
        registry.transition_to_starting(&id).unwrap();

        registry.transition_to_error_terminal(&id).unwrap();
        let event = registry
            .transition_to_stopped(&id, SessionStopReason::Error)
            .unwrap();
        assert!(matches!(event, HostEvent::SessionStopped(_)));
        assert_eq!(registry.get(&id).unwrap().state, SessionState::Stopped);
    }

    #[test]
    fn error_recoverable_to_starting_restart() {
        let mut registry = SessionRegistry::new();
        let id = register_session(&mut registry);
        registry.transition_to_starting(&id).unwrap();
        registry.transition_to_error_recoverable(&id).unwrap();

        // Restart from error recoverable
        registry.transition_to_starting(&id).unwrap();
        assert_eq!(registry.get(&id).unwrap().state, SessionState::Starting);
    }

    #[test]
    fn full_lifecycle() {
        let mut registry = SessionRegistry::new();
        let id = register_session(&mut registry);

        // Created → Starting → Ready
        registry.transition_to_starting(&id).unwrap();
        registry.transition_to_ready(&id).unwrap();

        // Ready → Busy → Ready
        let op_id = HostOperationId::new();
        registry.transition_to_busy(&id, op_id.clone()).unwrap();
        registry.transition_busy_to_ready(&id, &op_id).unwrap();

        // Ready → Stopping → Stopped
        registry.transition_to_stopping(&id).unwrap();
        let event = registry
            .transition_to_stopped(&id, SessionStopReason::GracefulShutdown)
            .unwrap();
        assert!(matches!(event, HostEvent::SessionStopped(_)));
        assert!(registry.get(&id).unwrap().state.is_terminal());
    }

    #[test]
    fn remove_stopped_session() {
        let mut registry = SessionRegistry::new();
        let id = register_session(&mut registry);
        registry.transition_to_starting(&id).unwrap();
        registry.transition_to_stopping(&id).unwrap();
        registry
            .transition_to_stopped(&id, SessionStopReason::GracefulShutdown)
            .unwrap();

        let removed = registry.remove_stopped(&id).unwrap();
        assert_eq!(removed.id, id);
        assert!(registry.get(&id).is_none());
    }

    #[test]
    fn cannot_remove_non_stopped_session() {
        let mut registry = SessionRegistry::new();
        let id = register_session(&mut registry);
        assert!(registry.remove_stopped(&id).is_err());
    }

    #[test]
    fn session_not_found_errors() {
        let mut registry = SessionRegistry::new();
        let fake_id = HostSessionId::new();
        assert!(registry.transition_to_starting(&fake_id).is_err());
    }

    #[test]
    fn wrong_op_id_for_cancel() {
        let mut registry = SessionRegistry::new();
        let id = register_session(&mut registry);
        registry.transition_to_starting(&id).unwrap();
        registry.transition_to_ready(&id).unwrap();

        let op_id = HostOperationId::new();
        registry.transition_to_busy(&id, op_id).unwrap();

        let wrong_op_id = HostOperationId::new();
        assert!(registry
            .transition_to_cancelling(&id, &wrong_op_id)
            .is_err());
    }
}
