//! Schedule supervision and core-context derivation (WS7).
//!
//! - [`admission`] — pure-function admission gate per spec §5.1
//! - [`supervisor`] — `ScheduleSupervisor` with `tick()` / `on_session_terminal()`
//! - [`cron_supervisor`] — daemon-side cron evaluator for novel-writing staggering (V1.50 T-A P1)
//! - [`derivation`] — `CoreContextManager` for immutable versioned `core_context`

pub mod admission;
pub mod cron_supervisor;
pub mod derivation;
pub mod supervisor;
