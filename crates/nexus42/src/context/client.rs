//! Context Assembly Local API client.
//!
//! Calls POST /v1/local/context/assemble through the DaemonClient (nexus42d loopback).

use crate::api::DaemonClient;
use crate::context::types::ContextAssembleRequestV1;
use crate::context::types::ContextAssembleResponseV1;
use crate::errors::Result;

/// Client for the Context Assembly Local API.
pub struct ContextClient {
    daemon: DaemonClient,
}

impl ContextClient {
    /// Create a new context client from a DaemonClient.
    pub fn new(daemon: DaemonClient) -> Self {
        Self { daemon }
    }

    /// Request assembled context from the platform via the Local API.
    ///
    /// Sends `POST /v1/local/context/assemble` through nexus42d.
    /// The daemon proxies this request to the platform's Context Assembly service.
    pub async fn assemble(
        &self,
        request: &ContextAssembleRequestV1,
    ) -> Result<ContextAssembleResponseV1> {
        let response: ContextAssembleResponseV1 = self
            .daemon
            .post("/v1/local/context/assemble", request)
            .await?;
        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Helper: create a minimal request for testing.
    fn make_request() -> ContextAssembleRequestV1 {
        ContextAssembleRequestV1 {
            request_id: "req_test_001".to_string(),
            workspace_id: "wrk_001".to_string(),
            creator_id: "ctr_001".to_string(),
            world_id: "wld_001".to_string(),
            include_memory: Some(true),
            include_timeline: Some(true),
            include_story_summaries: Some(true),
            ..Default::default()
        }
    }

    #[test]
    fn request_serializes_correctly() {
        let req = make_request();
        let json = serde_json::to_value(&req).expect("serialization should succeed");
        assert_eq!(json["request_id"], "req_test_001");
        assert_eq!(json["workspace_id"], "wrk_001");
        assert_eq!(json["creator_id"], "ctr_001");
        assert_eq!(json["world_id"], "wld_001");
        assert_eq!(json["include_memory"], true);
    }

    #[tokio::test]
    async fn assemble_success_with_mock() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;
        let success_response = json!({
            "request_id": "req_test_001",
            "success": true,
            "error_code": null,
            "error_message": null,
            "world_id": "wld_001",
            "assembled_at": "2025-04-05T12:00:00Z",
            "data_freshness_hint": "bdl_abc123",
            "key_blocks": [],
            "timeline_events": [],
            "story_summaries": [],
            "memory_items": []
        });

        Mock::given(method("POST"))
            .and(path("/v1/local/context/assemble"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&success_response))
            .mount(&mock_server)
            .await;

        let daemon = DaemonClient::new(&mock_server.uri());
        let client = ContextClient::new(daemon);
        let req = make_request();
        let response = client
            .assemble(&req)
            .await
            .expect("assemble should succeed");

        assert!(response.success);
        assert_eq!(response.world_id, "wld_001");
        assert_eq!(response.data_freshness_hint, Some("bdl_abc123".to_string()));
    }

    #[tokio::test]
    async fn assemble_error_response() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;
        let error_response = json!({
            "request_id": "req_test_001",
            "success": false,
            "error_code": "world_not_found",
            "error_message": "World wld_999 does not exist",
            "world_id": "wld_999",
            "assembled_at": "2025-04-05T12:00:00Z",
            "data_freshness_hint": null,
            "key_blocks": [],
            "timeline_events": [],
            "story_summaries": [],
            "memory_items": []
        });

        Mock::given(method("POST"))
            .and(path("/v1/local/context/assemble"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&error_response))
            .mount(&mock_server)
            .await;

        let daemon = DaemonClient::new(&mock_server.uri());
        let client = ContextClient::new(daemon);
        let req = make_request();
        let response = client
            .assemble(&req)
            .await
            .expect("assemble should succeed");

        assert!(!response.success);
        assert_eq!(response.error_code, Some("world_not_found".to_string()));
        assert_eq!(
            response.error_message,
            Some("World wld_999 does not exist".to_string())
        );
    }

    #[tokio::test]
    async fn assemble_http_error() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/local/context/assemble"))
            .respond_with(ResponseTemplate::new(503).set_body_string("Platform unavailable"))
            .mount(&mock_server)
            .await;

        let daemon = DaemonClient::new(&mock_server.uri());
        let client = ContextClient::new(daemon);
        let req = make_request();
        let result = client.assemble(&req).await;

        assert!(result.is_err(), "should return error for HTTP 503");
    }

    #[tokio::test]
    async fn assemble_with_full_data() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;
        let full_response = json!({
            "request_id": "req_test_001",
            "success": true,
            "error_code": null,
            "error_message": null,
            "world_id": "wld_001",
            "assembled_at": "2025-04-05T12:00:00Z",
            "data_freshness_hint": null,
            "key_blocks": [
                {
                    "key_block_id": "kb_001",
                    "block_type": "character",
                    "name": "Hero",
                    "summary": "The protagonist"
                }
            ],
            "timeline_events": [
                {
                    "event_id": "evt_001",
                    "event_type": "plot_point",
                    "description": "Discovery",
                    "occurred_at": "2025-04-01T00:00:00Z"
                }
            ],
            "story_summaries": [
                {
                    "story_manifest_id": "stm_001",
                    "title": "Chapter 1",
                    "summary_text": "The beginning",
                    "manifest_type": "chapter"
                }
            ],
            "memory_items": [
                {
                    "memory_id": "mem_001",
                    "memory_kind": "story_summary",
                    "content": "Important detail"
                }
            ]
        });

        Mock::given(method("POST"))
            .and(path("/v1/local/context/assemble"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&full_response))
            .mount(&mock_server)
            .await;

        let daemon = DaemonClient::new(&mock_server.uri());
        let client = ContextClient::new(daemon);
        let req = make_request();
        let response = client
            .assemble(&req)
            .await
            .expect("assemble should succeed");

        let key_blocks = response.key_blocks.as_ref().unwrap();
        assert_eq!(key_blocks.len(), 1);
        assert_eq!(key_blocks[0].name, "Hero");
        assert_eq!(response.timeline_events.as_ref().unwrap().len(), 1);
        assert_eq!(response.story_summaries.as_ref().unwrap().len(), 1);
        assert_eq!(response.memory_items.as_ref().unwrap().len(), 1);
    }
}
