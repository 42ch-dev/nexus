//! Context Assembly — request/response types for the platform context assembly contract.
//!
//! Types are generated from `schemas/platform/context-assembly-v1.schema.json`
//! via `pnpm run codegen` into `nexus-contracts`. This module re-exports them
//! for use by CLI crates.
//!
//! Note (KCA-002 B2): These are wire types for the platform API contract.
//! The daemon `POST /v1/daemon/context/assemble` route is retired; context
//! assembly runs CLI in-process via `nexus-moment-context-assembly`.

// Re-export generated types from nexus-contracts
pub use nexus_contracts::generated::platform::http_bff::context_assembly_v1::ContextAssembleRequestV1;
pub use nexus_contracts::generated::platform::http_bff::context_assembly_v1::ContextAssembleResponseV1;

// Re-export MemoryKind from domain for CLI use
#[cfg(test)]
pub use nexus_creator_memory::memory_item::MemoryKind;

/// Backward-compatible type alias.
#[allow(dead_code)]
pub type ContextAssembleRequest = ContextAssembleRequestV1;

/// Backward-compatible type alias.
#[allow(dead_code)]
pub type ContextAssembleResponse = ContextAssembleResponseV1;

/// Helper: check whether a context assembly response indicates an error.
#[cfg(test)]
#[inline]
#[must_use]
pub const fn is_error(resp: &ContextAssembleResponse) -> bool {
    !resp.success
}

/// Helper: get the error code from a context assembly response, if any.
#[cfg(test)]
#[inline]
#[must_use]
pub fn error_code(resp: &ContextAssembleResponse) -> Option<&str> {
    resp.error_code.as_deref()
}

/// Helper: get the error message from a context assembly response, if any.
#[cfg(test)]
#[inline]
#[must_use]
#[allow(dead_code)]
pub fn error_message(resp: &ContextAssembleResponse) -> Option<&str> {
    resp.error_message.as_deref()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_serializes_to_valid_json() {
        let req: ContextAssembleRequestV1 = serde_json::from_value(serde_json::json!({"request_id": "req_test", "workspace_id": "wrk_001", "creator_id": "ctr_001", "world_id": "wld_001", "include_memory": true, "include_timeline": true, "include_story_summaries": true})).unwrap();
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
        // Schema defaults: include_* flags default to true; memory_kinds has a non-empty default.
        assert!(req.include_memory);
        assert!(req.include_timeline);
        assert!(req.include_story_summaries);
        assert_eq!(req.memory_kinds.len(), 3);
        assert_eq!(req.max_timeline_events, None);
        assert_eq!(req.max_story_summaries, None);
        assert_eq!(req.key_block_limit, 100);
        assert_eq!(req.timeline_limit, 50);
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
        assert!(!req.include_memory);
        assert_eq!(req.max_timeline_events, Some(10));
    }

    #[test]
    fn response_success_roundtrip() {
        let resp: ContextAssembleResponseV1 = serde_json::from_value(serde_json::json!({
            "request_id": "req_test",
            "success": true,
            "world_id": "wld_001",
            "assembled_at": "2025-04-05T12:00:00Z",
            "key_blocks": [],
            "timeline_events": [],
            "story_summaries": [],
            "memory_items": []
        }))
        .unwrap();
        let json = serde_json::to_string(&resp).expect("serialization should succeed");
        let deserialized: ContextAssembleResponseV1 =
            serde_json::from_str(&json).expect("deserialization should succeed");
        assert_eq!(
            serde_json::to_value(&deserialized).unwrap(),
            serde_json::to_value(&resp).unwrap()
        );
        assert!(!is_error(&deserialized));
    }

    #[test]
    fn response_error_roundtrip() {
        let resp: ContextAssembleResponseV1 = serde_json::from_value(serde_json::json!({
            "request_id": "req_test",
            "success": false,
            "error_code": "world_not_found",
            "error_message": "World not found",
            "world_id": "wld_999",
            "assembled_at": "2025-04-05T12:00:00Z",
            "key_blocks": [],
            "timeline_events": [],
            "story_summaries": [],
            "memory_items": []
        }))
        .unwrap();
        assert!(is_error(&resp));
        assert_eq!(error_code(&resp), Some("world_not_found"));
    }

    #[test]
    fn response_with_data_roundtrip() {
        let resp: ContextAssembleResponseV1 = serde_json::from_value(serde_json::json!({
            "request_id": "req_test",
            "success": true,
            "world_id": "wld_001",
            "assembled_at": "2025-04-05T12:00:00Z",
            "key_blocks": [{
                "key_block_id": "kb_001",
                "name": "Alice",
                "summary": "Protagonist",
                "block_type": "character"
            }],
            "timeline_events": [{
                "event_id": "evt_001",
                "event_type": "plot_point",
                "description": "Discovery",
                "occurred_at": "2025-04-01T00:00:00Z"
            }],
            "story_summaries": [{
                "story_manifest_id": "stm_001",
                "title": "Chapter 1",
                "summary_text": "The beginning",
                "manifest_type": "chapter"
            }],
            "memory_items": [{
                "memory_id": "mem_001",
                "memory_kind": "story_summary",
                "content": "Important detail"
            }]
        }))
        .unwrap();
        let json = serde_json::to_string(&resp).expect("serialization should succeed");
        let deserialized: ContextAssembleResponseV1 =
            serde_json::from_str(&json).expect("deserialization should succeed");
        assert_eq!(deserialized.key_blocks.len(), 1);
        assert_eq!(deserialized.timeline_events.len(), 1);
        assert_eq!(deserialized.story_summaries.len(), 1);
        assert_eq!(deserialized.memory_items.len(), 1);
    }
}
