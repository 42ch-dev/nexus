//! `Nexus` `Compute` `Output` `Envelope`
//!
//! `Standard` 4-part output envelope returned by a `WASM` compute module (`V1`.61 `ABI`, compass `Q8`). `Modules` emit state deltas to apply, timeline events to append (aligned with `V1`.60 timeline.event.append), new `KeyBlocks` to create, and a module-declared freeform report. `The` host applies these in order: `state_delta` -> `new_key_blocks` -> `timeline_events`, then surfaces `battle_report`.
//!
//! `@schema_version` 1
//! `@source` compute-output.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::domain::key_block::KeyBlock;
use crate::generated::domain::timeline_event::TimelineEvent;

/// Inline array item type (auto-generated from schema)
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ComputeOutputStateDelta {
    pub op: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_key_block_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>,
}
/// `Standard` 4-part output envelope returned by a `WASM` compute module (`V1`.61 `ABI`, compass `Q8`). `Modules` emit state deltas to apply, timeline events to append (aligned with `V1`.60 timeline.event.append), new `KeyBlocks` to create, and a module-declared freeform report. `The` host applies these in order: `state_delta` -> `new_key_blocks` -> `timeline_events`, then surfaces `battle_report`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ComputeOutput {
    pub schema_version: u32,
    pub state_delta: Vec<ComputeOutputStateDelta>,
    pub timeline_events: Vec<TimelineEvent>,
    pub new_key_blocks: Vec<KeyBlock>,
    pub battle_report: serde_json::Value,
}
