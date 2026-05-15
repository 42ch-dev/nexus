//! Core host management: session registry, operation registry, lifecycle.

pub mod session;

pub use session::{HostSession, SessionRegistry, SessionState, TransitionResult};
