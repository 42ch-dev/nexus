//! Nexus Meta Schema — local-only schema metadata type.
//!
//! Meta schema defining schema versioning and structure rules for all Nexus schemas.
//! This is used internally for validation; platform never observes it.

use serde::{Deserialize, Serialize};

/// Meta schema defining schema versioning and structure rules.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct Meta {
    #[serde(rename = "$schema")]
    pub dollar_schema: String,
    #[serde(rename = "$id")]
    pub dollar_id: String,
    pub schema_version: u32,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(rename = "type")]
    pub r#type: MetaType,
}

/// Root type of a schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetaType {
    Object,
    Array,
    String,
    Number,
    Integer,
    Boolean,
    Null,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_meta() {
        let v = Meta {
            dollar_schema: "http://json-schema.org/draft-07/schema#".to_string(),
            dollar_id: "https://nexus42.invalid/schemas/domain/test.schema.json".to_string(),
            schema_version: 1,
            title: "Test Schema".to_string(),
            description: Some("A test schema".to_string()),
            r#type: MetaType::Object,
        };
        let s = serde_json::to_string(&v).unwrap();
        let back: Meta = serde_json::from_str(&s).unwrap();
        assert_eq!(back, v);
    }
}
