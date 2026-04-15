//! RuntimeMode
//!
//! Creator runtime mode controlling platform dependency behavior. See ADR-015, ADR-017. local_only: no platform HTTP dependency. local_first: optional platform structured services, no platform LLM. cloud_enhanced: full platform capabilities.
//!
//! @schema_version 1
//! @source runtime-mode.schema.json

use serde::{Deserialize, Serialize};

/// Creator runtime mode controlling platform dependency behavior. See ADR-015, ADR-017. local_only: no platform HTTP dependency. local_first: optional platform structured services, no platform LLM. cloud_enhanced: full platform capabilities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeMode {
    /// No platform HTTP dependency for Creator's end-to-end loop (ADR-017). Platform unreachable/login failure is not a Creator failure.
    #[serde(rename = "local_only")]
    LocalOnly,
    /// Optional platform structured services allowed; platform must not call generative LLM on behalf of Creator (ADR-015).
    #[serde(rename = "local_first")]
    LocalFirst,
    /// Full platform capabilities including hosted LLM (ADR-015). Default for platform-hosted deployments.
    #[serde(rename = "cloud_enhanced")]
    CloudEnhanced,
}
