//! The durable run engine (v1.245 extraction).
//!
//! This crate owns everything that can be driven by ports: the bounded run
//! driver and its single-flight coordinator, the durable workspace commit
//! authority with its snapshot/OCC filesystem primitives and sessions, the
//! bounded per-run event rings the SSE transport replays from, and the
//! background scheduler ownership.
//!
//! What it deliberately does NOT own: the `nexus.*` tool dispatch, the
//! capability/compute surface, the ops surface on the handle, the hosted
//! composition, and the execution owner itself. Those read the owner's
//! service state and stay in `nexus-core`, which composes this engine behind
//! its `execution` feature. Transports, HTTP binding, OS signals and the SPA
//! stay in the daemon.
//!
//! Dependency direction: `nexus-core → nexus-execution`, never back.

pub mod admission;
pub mod authority;
pub mod bounds;
pub mod commit_fs;
#[cfg(feature = "provider-host")]
pub mod prompt_executor;
pub mod run_events;
pub mod schedules;
pub mod scope;
pub mod session;
pub mod session_commit;
pub mod soul_narrative_synthesizer;
pub mod state_provider;
#[cfg(any(test, feature = "test-hooks"))]
pub mod test_hooks;
pub mod workflow;
