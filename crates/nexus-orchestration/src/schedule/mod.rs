//! Schedule supervision and core-context derivation (WS7).
//!
//! - [`admission`] — pure-function admission gate per spec §5.1
//! - [`supervisor`] — `ScheduleSupervisor` with `tick()` / `on_session_terminal()`
//! - [`derivation`] — `CoreContextManager` for immutable versioned `core_context`

pub mod admission;
pub mod derivation;
pub mod supervisor;
