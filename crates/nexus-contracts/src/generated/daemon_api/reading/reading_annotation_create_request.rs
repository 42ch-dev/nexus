//! `Nexus` `ReadingAnnotationCreateRequest`
//!
//! `Request` body for `POST` /v1/daemon/reading/annotations. `Creates` a persistent highlight anchored by character offsets into the chapter body plain text. `Creator` scope is inferred from the active session.
//!
//! `@schema_version` 1
//! `@source` reading-annotation-create-request.schema.json

use serde::{Deserialize, Serialize};

/// `Request` body for `POST` /v1/daemon/reading/annotations. `Creates` a persistent highlight anchored by character offsets into the chapter body plain text. `Creator` scope is inferred from the active session.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ReadingAnnotationCreateRequest {
    pub work_id: String,
    pub chapter: i64,
    pub start_offset: u64,
    pub end_offset: u64,
    pub selected_text: String,
    pub color: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}
