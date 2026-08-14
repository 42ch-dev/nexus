//! Product checkers invoked from the spoke `orchestrate_check` callback
//! (the daemon `POST /v1/daemon/check` surface).
//!
//! The callback receives a `CheckRunInput` (`request` + scoped `entries` +
//! scoped `events` + resolved `rules`) and returns the `Finding`s the
//! orchestrator persists via `FindingPort::put_findings` (V1.148 P2 — the
//! check op became daemon-reachable through `api::handlers::check`).
//! V1.164 P2 T3 replaces the baseline no-op evaluator with the mental-layer
//! checker pair (see [`mental`]).

pub mod mental;
