//! `Nexus` `ReadingAnnotationPatchRequest`
//!
//! `Request` body for `PATCH` /v1/daemon/reading/annotations/{`annotation_id`}. `Edits` the highlight color and/or optional note. `Both` fields are optional; at least one must be present. `The` `annotation_id` comes from the `URL` path, not the body.
//!
//! `@schema_version` 1
//! `@source` reading-annotation-patch-request.schema.json

use serde::{Deserialize, Serialize};

/// `Request` body for `PATCH` /v1/daemon/reading/annotations/{`annotation_id`}. `Edits` the highlight color and/or optional note. `Both` fields are optional; at least one must be present. `The` `annotation_id` comes from the `URL` path, not the body.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ReadingAnnotationPatchRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}
