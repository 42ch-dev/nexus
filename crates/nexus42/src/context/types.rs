//! Context Assembly — request/response types for POST /v1/local/context/assemble.

use serde::{Deserialize, Serialize};

/// Request for context assembly via the Local API.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContextAssembleRequest {
    /// Caller-generated traceable ID.
    pub request_id: String,
    /// Workspace ID (pattern: `wrk_.*`).
    pub workspace_id: String,
    /// Creator ID (pattern: `ctr_.*`).
    pub creator_id: String,
    /// World ID (pattern: `wld_.*`).
    pub world_id: String,
    /// Include memory items in assembled context.
    #[serde(default = "default_true")]
    pub include_memory: bool,
    /// Include timeline events in assembled context.
    #[serde(default = "default_true")]
    pub include_timeline: bool,
    /// Include story summaries in assembled context.
    #[serde(default = "default_true")]
    pub include_story_summaries: bool,
    /// Filter memory items by kind.
    #[serde(default = "default_memory_kinds")]
    pub memory_kinds: Vec<String>,
    /// Maximum number of recent timeline events (null = platform default).
    pub max_timeline_events: Option<u64>,
    /// Maximum number of story summaries (null = platform default).
    pub max_story_summaries: Option<u64>,
}

fn default_true() -> bool {
    true
}

fn default_memory_kinds() -> Vec<String> {
    vec![
        "story_summary".to_string(),
        "research_material".to_string(),
        "review_note".to_string(),
    ]
}

impl ContextAssembleRequest {
    /// Create a minimal request with required fields and default options.
    ///
    /// NOTE: Not yet wired into CLI commands; will be used by the context
    /// assembly workflow once the daemon context endpoint is integrated.
    #[allow(dead_code)]
    pub fn new(
        request_id: String,
        workspace_id: String,
        creator_id: String,
        world_id: String,
    ) -> Self {
        Self {
            request_id,
            workspace_id,
            creator_id,
            world_id,
            include_memory: true,
            include_timeline: true,
            include_story_summaries: true,
            memory_kinds: default_memory_kinds(),
            max_timeline_events: None,
            max_story_summaries: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

/// A KeyBlock in the assembled context.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KeyBlockSnapshot {
    pub key_block_id: String,
    pub block_type: String,
    pub name: String,
    pub summary: String,
}

/// A timeline event in the assembled context.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TimelineEventSnapshot {
    pub event_id: String,
    pub event_type: String,
    pub description: String,
    pub occurred_at: String,
}

/// A story summary in the assembled context.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StorySummarySnapshot {
    pub story_manifest_id: String,
    pub title: String,
    pub summary_text: String,
    pub manifest_type: String,
}

/// A memory item in the assembled context.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryItemSnapshot {
    pub memory_id: String,
    pub memory_kind: String,
    pub content: String,
}

/// Response from POST /v1/local/context/assemble.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContextAssembleResponse {
    /// Echo of request_id for correlation.
    pub request_id: String,
    /// Whether the assembly succeeded.
    pub success: bool,
    /// Error code if success=false.
    pub error_code: Option<String>,
    /// Human-readable error message if success=false.
    pub error_message: Option<String>,
    /// World ID this context belongs to.
    pub world_id: String,
    /// ISO 8601 timestamp when this snapshot was assembled.
    pub assembled_at: String,
    /// Freshness indicator (e.g., bundle ID) to detect stale data.
    pub data_freshness_hint: Option<String>,
    /// Confirmed KeyBlocks relevant to the world.
    #[serde(default)]
    pub key_blocks: Vec<KeyBlockSnapshot>,
    /// Recent canon timeline events.
    #[serde(default)]
    pub timeline_events: Vec<TimelineEventSnapshot>,
    /// Story summaries from StoryManifest.summary_text.
    #[serde(default)]
    pub story_summaries: Vec<StorySummarySnapshot>,
    /// Memory slices.
    #[serde(default)]
    pub memory_items: Vec<MemoryItemSnapshot>,
}

impl ContextAssembleResponse {
    /// Check whether the response indicates an error.
    pub fn is_error(&self) -> bool {
        !self.success
    }

    /// Get the error code, if any.
    pub fn error_code(&self) -> Option<&str> {
        self.error_code.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_serializes_to_valid_json() {
        let req = ContextAssembleRequest::new(
            "req_test".to_string(),
            "wrk_001".to_string(),
            "ctr_001".to_string(),
            "wld_001".to_string(),
        );
        let json = serde_json::to_string(&req).expect("serialization should succeed");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("json should be valid");
        assert_eq!(parsed["request_id"], "req_test");
        assert_eq!(parsed["workspace_id"], "wrk_001");
        assert_eq!(parsed["creator_id"], "ctr_001");
        assert_eq!(parsed["world_id"], "wld_001");
        assert_eq!(parsed["include_memory"], true);
        assert_eq!(parsed["include_timeline"], true);
        assert_eq!(parsed["include_story_summaries"], true);
    }

    #[test]
    fn request_deserializes_with_defaults() {
        let json = r#"{
            "request_id": "req_1",
            "workspace_id": "wrk_1",
            "creator_id": "ctr_1",
            "world_id": "wld_1"
        }"#;
        let req: ContextAssembleRequest =
            serde_json::from_str(json).expect("deserialization should succeed");
        assert!(req.include_memory);
        assert!(req.include_timeline);
        assert!(req.include_story_summaries);
        assert_eq!(req.memory_kinds.len(), 3);
        assert_eq!(req.max_timeline_events, None);
        assert_eq!(req.max_story_summaries, None);
    }

    #[test]
    fn request_deserializes_with_explicit_options() {
        let json = r#"{
            "request_id": "req_2",
            "workspace_id": "wrk_1",
            "creator_id": "ctr_1",
            "world_id": "wld_1",
            "include_memory": false,
            "max_timeline_events": 10
        }"#;
        let req: ContextAssembleRequest =
            serde_json::from_str(json).expect("deserialization should succeed");
        assert!(!req.include_memory);
        assert_eq!(req.max_timeline_events, Some(10));
    }

    #[test]
    fn response_success_roundtrip() {
        let resp = ContextAssembleResponse {
            request_id: "req_1".to_string(),
            success: true,
            error_code: None,
            error_message: None,
            world_id: "wld_001".to_string(),
            assembled_at: "2025-04-05T12:00:00Z".to_string(),
            data_freshness_hint: Some("bdl_abc123".to_string()),
            key_blocks: vec![],
            timeline_events: vec![],
            story_summaries: vec![],
            memory_items: vec![],
        };
        let json = serde_json::to_string(&resp).expect("serialization should succeed");
        let deserialized: ContextAssembleResponse =
            serde_json::from_str(&json).expect("deserialization should succeed");
        assert_eq!(deserialized, resp);
        assert!(!deserialized.is_error());
    }

    #[test]
    fn response_error_roundtrip() {
        let resp = ContextAssembleResponse {
            request_id: "req_2".to_string(),
            success: false,
            error_code: Some("world_not_found".to_string()),
            error_message: Some("World does not exist".to_string()),
            world_id: "wld_999".to_string(),
            assembled_at: "2025-04-05T12:00:00Z".to_string(),
            data_freshness_hint: None,
            key_blocks: vec![],
            timeline_events: vec![],
            story_summaries: vec![],
            memory_items: vec![],
        };
        assert!(resp.is_error());
        assert_eq!(resp.error_code(), Some("world_not_found"));
    }

    #[test]
    fn response_with_data_roundtrip() {
        let resp = ContextAssembleResponse {
            request_id: "req_3".to_string(),
            success: true,
            error_code: None,
            error_message: None,
            world_id: "wld_001".to_string(),
            assembled_at: "2025-04-05T12:00:00Z".to_string(),
            data_freshness_hint: None,
            key_blocks: vec![KeyBlockSnapshot {
                key_block_id: "kb_001".to_string(),
                block_type: "character".to_string(),
                name: "Hero".to_string(),
                summary: "The protagonist".to_string(),
            }],
            timeline_events: vec![TimelineEventSnapshot {
                event_id: "evt_001".to_string(),
                event_type: "plot_point".to_string(),
                description: "Discovery".to_string(),
                occurred_at: "2025-04-01T00:00:00Z".to_string(),
            }],
            story_summaries: vec![StorySummarySnapshot {
                story_manifest_id: "stm_001".to_string(),
                title: "Chapter 1".to_string(),
                summary_text: "The beginning".to_string(),
                manifest_type: "chapter".to_string(),
            }],
            memory_items: vec![MemoryItemSnapshot {
                memory_id: "mem_001".to_string(),
                memory_kind: "story_summary".to_string(),
                content: "Important detail".to_string(),
            }],
        };
        let json = serde_json::to_string(&resp).expect("serialization should succeed");
        let deserialized: ContextAssembleResponse =
            serde_json::from_str(&json).expect("deserialization should succeed");
        assert_eq!(deserialized.key_blocks.len(), 1);
        assert_eq!(deserialized.timeline_events.len(), 1);
        assert_eq!(deserialized.story_summaries.len(), 1);
        assert_eq!(deserialized.memory_items.len(), 1);
    }
}
