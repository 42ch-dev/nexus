//! Cloud-line domain modules (runtime mode, degradation, guard).
//!
//! These modules belong to the cloud line (CLI) per architecture spec §8:
//! "`runtime_mode`, `degradation`, and platform health probing belong to
//! the cloud line (CLI / cloud-stage builds), not the daemon hot path."

// Pedantic clippy lints that are mechanical/stylistic and don't affect correctness.
// These modules were migrated from nexus-domain which had crate-wide allow attributes.
#![allow(clippy::use_self)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::derive_partial_eq_without_eq)]
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::option_if_let_else)]
#![allow(clippy::missing_const_for_fn)]
#![allow(clippy::items_after_statements)]
#![allow(clippy::needless_collect)]
#![allow(clippy::float_cmp)]
#![allow(clippy::single_char_pattern)]
#![allow(clippy::redundant_clone)]
#![allow(clippy::clone_on_copy)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::single_match_else)]
#![allow(clippy::manual_let_else)]
#![allow(clippy::if_not_else)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::similar_names)]
#![allow(clippy::manual_string_new)]

pub mod degradation;
pub mod errors;
pub mod runtime_guard;
pub mod runtime_mode;

// Re-export primary types for convenience
pub use degradation::{
    DegradationGuard, DegradationPolicy, DegradationSnapshot, DegradationState, HealthCheckSnapshot,
};
pub use errors::DomainError;
pub use runtime_guard::{check_operation, classify_operation, require_platform};
pub use runtime_mode::DomainRuntimeMode;
