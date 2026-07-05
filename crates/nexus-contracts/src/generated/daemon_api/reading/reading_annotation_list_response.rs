//! `Nexus` `ReadingAnnotationListResponse`
//!
//! `Response` for `GET` /v1/daemon/reading/annotations. `Returns` all annotations for the current creator on the requested (work, chapter) as a flat list. `No` pagination — per-chapter annotation count is expected to stay bounded (dozens, not hundreds).
//!
//! `@schema_version` 1
//! `@source` reading-annotation-list-response.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::daemon_api::reading::reading_annotation::ReadingAnnotation;

/// `Response` for `GET` /v1/daemon/reading/annotations. `Returns` all annotations for the current creator on the requested (work, chapter) as a flat list. `No` pagination — per-chapter annotation count is expected to stay bounded (dozens, not hundreds).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ReadingAnnotationListResponse {
    pub items: Vec<ReadingAnnotation>,
}
