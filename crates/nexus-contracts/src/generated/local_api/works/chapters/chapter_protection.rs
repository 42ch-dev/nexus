//! `Nexus` `ChapterProtection`
//!
//! `Protection` level describing what `UI` actions are allowed for a chapter (`V1`.65 `P0`).
//!
//! `@schema_version` 1
//! `@source` chapter-protection.schema.json

use serde::{Deserialize, Serialize};

/// `Protection` level describing what `UI` actions are allowed for a chapter (`V1`.65 `P0`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ChapterProtection {
    pub level: String,
    pub reason: String,
}
