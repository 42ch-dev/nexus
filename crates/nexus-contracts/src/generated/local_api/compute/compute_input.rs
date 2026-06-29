//! `Nexus` `Compute` `Input` `Envelope`
//!
//! `Standard` input envelope passed into a `WASM` compute module (`V1`.61 `ABI`, compass `Q3`/`Q8`). `Bundles` a read-only `KeyBlock` snapshot, the narrative position, and module-declared invocation parameters. `Modules` are stateless pure functions (compass `Q6`): every call receives a fresh envelope and returns a `ComputeOutput`.
//!
//! `@schema_version` 1
//! `@source` compute-input.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::domain::key_block::KeyBlock;

/// `Standard` input envelope passed into a `WASM` compute module (`V1`.61 `ABI`, compass `Q3`/`Q8`). `Bundles` a read-only `KeyBlock` snapshot, the narrative position, and module-declared invocation parameters. `Modules` are stateless pure functions (compass `Q6`): every call receives a fresh envelope and returns a `ComputeOutput`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ComputeInput {
    pub schema_version: u32,
    pub world_ref: serde_json::Value,
    pub key_blocks: Vec<KeyBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub narrative_state: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invocation: Option<serde_json::Value>,
}
