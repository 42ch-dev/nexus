//! `Nexus` `ChapterStatus`
//!
//! `Lifecycle` status of a work chapter (`V1`.65 `P0`).
//!
//! `@schema_version` 1
//! `@source` chapter-status.schema.json

use serde::{Deserialize, Serialize};

/// `Lifecycle` status of a work chapter (`V1`.65 `P0`).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ChapterStatus {
    #[default]
    #[serde(rename = "not_started")]
    NotStarted,
    #[serde(rename = "outlined")]
    Outlined,
    #[serde(rename = "draft")]
    Draft,
    #[serde(rename = "finalized")]
    Finalized,
    #[serde(rename = "published")]
    Published,
}
