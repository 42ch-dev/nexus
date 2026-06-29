//! `Nexus` `ScheduleConcurrencyRequest`
//!
//! `Concurrency` mode for schedule creation. `Serial` runs alone; `ParallelWith` groups schedules; `ParallelAny` allows any concurrency.
//!
//! `@schema_version` 1
//! `@source` schedule-concurrency-request.schema.json

use serde::{Deserialize, Serialize};

/// `Concurrency` mode for schedule creation. `Serial` runs alone; `ParallelWith` groups schedules; `ParallelAny` allows any concurrency.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ScheduleConcurrencyRequest {
    #[default]
    #[serde(rename = "serial")]
    Serial,
    #[serde(rename = "parallel_with")]
    ParallelWith,
    #[serde(rename = "parallel_any")]
    ParallelAny,
}
