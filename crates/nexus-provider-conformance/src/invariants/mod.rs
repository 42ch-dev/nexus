//! Per-invariant conformance checks over a collected `HostEvent` stream.
//!
//! Each module owns one invariant from the plan (v1.180 P0, RN-OGA-1):
//!
//! - [`started`] — exactly one `OpStarted` per operation stream.
//! - [`bounds`] — bounded event count / duration (enforced during collection).
//! - [`ordering`] — `OpStarted` before op-scoped events; terminal last.
//! - [`terminal`] — exactly one terminal (`OpFinished` | `OpFailed`), then the stream ends.
//! - [`stop_reason`] — `FinishReason` / `SessionStopReason` closed-set values.
//! - [`values`] — forbidden-value exclusion (`error_category` closed set,
//!   512-byte `error_message` cap, closed enum fields).

pub mod bounds;
pub mod ordering;
pub mod started;
pub mod stop_reason;
pub mod terminal;
pub mod values;
