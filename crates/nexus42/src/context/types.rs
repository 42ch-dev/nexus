//! Context Assembly — request/response types for POST /v1/local/context/assemble.
//!
//! Types are generated from `schemas/platform/context-assembly-v1.schema.json`
//! via `pnpm run codegen` into `nexus-contracts`. This module re-exports them
//! for use by CLI crates.

// Re-export generated types from nexus-contracts
pub use nexus_contracts::generated::ContextAssembleRequestV1;
pub use nexus_contracts::generated::ContextAssembleResponseV1;

// Re-export MemoryKind from domain for CLI use
pub use nexus_domain::memory_item::MemoryKind;

/// Backward-compatible type alias.
#[allow(dead_code)]
pub type ContextAssembleRequest = ContextAssembleRequestV1;

/// Backward-compatible type alias.
#[allow(dead_code)]
pub type ContextAssembleResponse = ContextAssembleResponseV1;

/// Helper: check whether a context assembly response indicates an error.
#[inline]
pub fn is_error(resp: &ContextAssembleResponse) -> bool {
    !resp.success
}

/// Helper: get the error code from a context assembly response, if any.
#[inline]
pub fn error_code(resp: &ContextAssembleResponse) -> Option<&str> {
    resp.error_code.as_deref()
}

/// Helper: get the error message from a context assembly response, if any.
#[inline]
pub fn error_message(resp: &ContextAssembleResponse) -> Option<&str> {
    resp.error_message.as_deref()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_serializes_to_valid_json() {
        let req = ContextAssembleRequestV1 {
            request_id: "req_test".to_string(),
            workspace_id: "wrk_001".to_string(),
            creator_id: "ctr_001".to_string(),
            world_id: "wld_001".to_string(),
            include_memory: Some(true),
            include_timeline: Some(true),
            include_story_summaries: Some(true),
            branch_id: None,
            memory_query: None,
            timeline_limit: None,
            key_block_limit: None,
            memory_kinds: None,
            max_timeline_events: None,
            max_story_summaries: None,
            as_of: None,
        };
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
        let req: ContextAssembleRequestV1 =
            serde_json::from_str(json).expect("deserialization should succeed");
        // Optional fields that were omitted
        assert_eq!(req.include_memory, None);
        assert_eq!(req.include_timeline, None);
        assert_eq!(req.include_story_summaries, None);
        assert_eq!(req.memory_kinds, None);
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
        let req: ContextAssembleRequestV1 =
            serde_json::from_str(json).expect("deserialization should succeed");
        assert_eq!(req.include_memory, Some(false));
        assert_eq!(req.max_timeline_events, Some(10));
    }

    #[test]
    fn response_success_roundtrip() {
        let resp = ContextAssembleResponseV1 {
            request_id: "req_1".to_string(),
            success: true,
            error_code: None,
            error_message: None,
            world_id: "wld_001".to_string(),
            assembled_at: "2025-04-05T12:00:00Z".to_string(),
            data_freshness_hint: Some("bdl_abc123".to_string()),
            key_blocks: Some(vec![]),
            timeline_events: Some(vec![]),
            story_summaries: Some(vec![]),
            memory_items: Some(vec![]),
        };
        let json = serde_json::to_string(&resp).expect("serialization should succeed");
        let deserialized: ContextAssembleResponseV1 =
            serde_json::from_str(&json).expect("deserialization should succeed");
        assert_eq!(deserialized, resp);
        assert!(!is_error(&deserialized));
    }

    #[test]
    fn response_error_roundtrip() {
        let resp = ContextAssembleResponseV1 {
            request_id: "req_2".to_string(),
            success: false,
            error_code: Some("world_not_found".to_string()),
            error_message: Some("World does not exist".to_string()),
            world_id: "wld_999".to_string(),
            assembled_at: "2025-04-05T12:00:00Z".to_string(),
            data_freshness_hint: None,
            key_blocks: Some(vec![]),
            timeline_events: Some(vec![]),
            story_summaries: Some(vec![]),
            memory_items: Some(vec![]),
        };
        assert!(is_error(&resp));
        assert_eq!(error_code(&resp), Some("world_not_found"));
    }

    #[test]
    fn response_with_data_roundtrip() {
        let resp = ContextAssembleResponseV1 {
            request_id: "req_3".to_string(),
            success: true,
            error_code: None,
            error_message: None,
            world_id: "wld_001".to_string(),
            assembled_at: "2025-04-05T12:00:00Z".to_string(),
            data_freshness_hint: None,
            key_blocks: Some(vec![
                nexus_contracts::generated::ContextAssembleResponseV1KeyBlock {
                    key_block_id: "kb_001".to_string(),
                    block_type: "character".to_string(),
                    name: "Hero".to_string(),
                    summary: "The protagonist".to_string(),
                },
            ]),
            timeline_events: Some(vec![
                nexus_contracts::generated::ContextAssembleResponseV1TimelineEvent {
                    event_id: "evt_001".to_string(),
                    event_type: "plot_point".to_string(),
                    description: "Discovery".to_string(),
                    occurred_at: "2025-04-01T00:00:00Z".to_string(),
                },
            ]),
            story_summaries: Some(vec![
                nexus_contracts::generated::ContextAssembleResponseV1StorySummary {
                    story_manifest_id: "stm_001".to_string(),
                    title: "Chapter 1".to_string(),
                    summary_text: "The beginning".to_string(),
                    manifest_type: "chapter".to_string(),
                },
            ]),
            memory_items: Some(vec![
                nexus_contracts::generated::ContextAssembleResponseV1MemoryItem {
                    memory_id: "mem_001".to_string(),
                    memory_kind: MemoryKind::StorySummary.to_string(),
                    content: "Important detail".to_string(),
                },
            ]),
        };
        let json = serde_json::to_string(&resp).expect("serialization should succeed");
        let deserialized: ContextAssembleResponseV1 =
            serde_json::from_str(&json).expect("deserialization should succeed");
        assert_eq!(deserialized.key_blocks.as_ref().unwrap().len(), 1);
        assert_eq!(deserialized.timeline_events.as_ref().unwrap().len(), 1);
        assert_eq!(deserialized.story_summaries.as_ref().unwrap().len(), 1);
        assert_eq!(deserialized.memory_items.as_ref().unwrap().len(), 1);
    }
}
