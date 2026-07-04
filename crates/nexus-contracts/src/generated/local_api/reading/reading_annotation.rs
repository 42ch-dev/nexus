//! `Nexus` `ReadingAnnotation`
//!
//! `Shared` annotation detail object returned by `POST`, `PATCH`, and as list items in `GET` /v1/local/reading/annotations. `Represents` a single persistent highlight with optional note, anchored by character offsets into the chapter body plain text.
//!
//! `@schema_version` 1
//! `@source` reading-annotation.schema.json

use serde::{Deserialize, Serialize};

/// `Shared` annotation detail object returned by `POST`, `PATCH`, and as list items in `GET` /v1/local/reading/annotations. `Represents` a single persistent highlight with optional note, anchored by character offsets into the chapter body plain text.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ReadingAnnotation {
    pub annotation_id: String,
    pub work_id: String,
    pub chapter: i64,
    pub start_offset: u64,
    pub end_offset: u64,
    pub selected_text: String,
    pub color: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}
