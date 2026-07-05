//! `Nexus` `ReadingAnnotationListQuery`
//!
//! `Query` parameters for `GET` /v1/daemon/reading/annotations. `Returns` all annotations for the current creator on a given (work, chapter). `Creator` scope is inferred from the active session.
//!
//! `@schema_version` 1
//! `@source` reading-annotation-list-query.schema.json

use serde::{Deserialize, Serialize};

/// `Query` parameters for `GET` /v1/daemon/reading/annotations. `Returns` all annotations for the current creator on a given (work, chapter). `Creator` scope is inferred from the active session.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ReadingAnnotationListQuery {
    pub work_id: String,
    pub chapter: i64,
}
