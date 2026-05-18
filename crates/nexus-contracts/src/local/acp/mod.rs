//! Nexus-owned ACP DTO types.
//!
//! These types decouple the `NexusAcpClient` trait from the `agent-client-protocol`
//! SDK. SDK types remain internal to `AcpSdkAdapter`; consumers only see these
//! Nexus DTOs through the trait signatures.
//!
//! Placement follows `schemas-boundary.md` §3: local types are hand-written
//! in `crates/nexus-contracts/src/local/` — no JSON Schema, no codegen.
//!
//! Design choices:
//! - Newtype wrappers (`NexusSessionId`, `NexusProtocolVersion`) for simple
//!   identifiers that consumers only pass through or compare.
//! - Deep structural mirrors (`NexusAgentCapabilities`, `NexusAgentInfo`,
//!   `NexusSessionModeState`, `NexusContentBlock`) only where consumers read
//!   nested fields beyond equality checks.
//! - Request/response DTOs carry only the fields consumers actually use.

mod types;

pub use types::*;
