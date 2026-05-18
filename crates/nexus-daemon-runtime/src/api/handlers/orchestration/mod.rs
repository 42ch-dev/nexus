//! Orchestration API handlers.
//!
//! Endpoints for engine session management, capability listing, preset listing,
//! and schedule management (WS7).
//! Design: `.agents/archived/knowledge/acp-client-tech-spec.md` §4.3.

pub mod capabilities;
pub mod presets;
pub mod schedules;
pub mod sessions;
