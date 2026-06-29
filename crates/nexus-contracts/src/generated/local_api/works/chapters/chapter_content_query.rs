//! `Nexus` `ChapterContentQuery`
//!
//! `Query` parameters for `GET`/`PUT`/`PATCH` chapter detail, outline, and body routes (`V1`.65 `P0`).
//!
//! `@schema_version` 1
//! `@source` chapter-content-query.schema.json

use serde::{Deserialize, Serialize};

/// `Query` parameters for `GET`/`PUT`/`PATCH` chapter detail, outline, and body routes (`V1`.65 `P0`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ChapterContentQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume: Option<i64>,
}
