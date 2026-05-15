//! Core host management: session registry, operation registry, lifecycle.

pub mod manager;
pub mod session;

pub use manager::HostManager;
pub use session::{HostSession, SessionRegistry, SessionState, TransitionResult};
