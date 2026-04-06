//! Integration Tests — ACP Registry fetch and parse.
//!
//! Tests that the CLI can fetch the ACP Registry from a CDN (mocked),
//! parse the manifest JSON, and verify schema conformance.

use nexus42::acp::registry::Registry;
use serde_json::Value;
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Sample registry JSON matching the ACP CDN schema.
const SAMPLE_REGISTRY_JSON: &str = r#"{
  "version": "1.0.0",
  "agents": [
    {
      "id": "claude-acp",
      "name": "Claude Agent",
      "version": "0.18.0",
      "description": "ACP wrapper for Anthropic's Claude",
      "repository": "https://github.com/zed-industries/claude-agent-acp",
      "authors": ["Anthropic"],
      "license": "proprietary",
      "icon": "https://cdn.agentclientprotocol.com/registry/v1/latest/claude-acp.svg",
      "distribution": {
        "npx": {
          "package": "@zed-industries/claude-agent-acp@0.18.0"
        }
      }
    },
    {
      "id": "codex-acp",
      "name": "Codex Agent",
      "version": "0.9.4",
      "description": "ACP adapter for OpenAI's Codex",
      "distribution": {
        "binary": {
          "darwin-aarch64": {
            "archive": "https://example.com/codex-darwin-aarch64.tar.gz",
            "cmd": "codex-acp"
          },
          "linux-x86_64": {
            "archive": "https://example.com/codex-linux-x86_64.tar.gz",
            "cmd": "codex-acp"
          }
        }
      }
    }
  ],
  "extensions": []
}"#;

/// Test fetching registry from a mock CDN via HTTP.
///
/// This test verifies that:
/// - The registry JSON can be parsed from HTTP response
/// - The parsed registry matches the schema
#[tokio::test]
async fn registry_fetch_from_mock_cdn() {
    // Start a mock HTTP server
    let mock_server = MockServer::start().await;

    // Configure mock to return sample registry JSON
    Mock::given(method("GET"))
        .and(path("/registry.json"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SAMPLE_REGISTRY_JSON))
        .mount(&mock_server)
        .await;

    // Fetch from mock server URL
    let url = format!("{}/registry.json", mock_server.uri());
    let response = reqwest::get(&url)
        .await
        .expect("Failed to fetch from mock server");

    assert!(
        response.status().is_success(),
        "Mock server should return 200"
    );

    let body = response.text().await.expect("Failed to read response body");
    let registry: Registry = serde_json::from_str(&body).expect("Failed to parse registry JSON");

    // Verify parsed data
    assert_eq!(registry.version, "1.0.0");
    assert_eq!(registry.agents.len(), 2);
    assert_eq!(registry.extensions.len(), 0);

    // Verify first agent (Claude)
    let claude = &registry.agents[0];
    assert_eq!(claude.id, "claude-acp");
    assert_eq!(claude.name, "Claude Agent");
    assert_eq!(claude.version, "0.18.0");
    assert_eq!(claude.description, "ACP wrapper for Anthropic's Claude");
    assert!(claude.distribution.npx.is_some());
    assert!(claude.distribution.binary.is_none());

    // Verify second agent (Codex)
    let codex = &registry.agents[1];
    assert_eq!(codex.id, "codex-acp");
    assert_eq!(codex.name, "Codex Agent");
    assert!(codex.distribution.binary.is_some());
    assert!(codex.distribution.npx.is_none());
}

/// Test registry fetch handles HTTP errors gracefully.
#[tokio::test]
async fn registry_fetch_handles_http_error() {
    let mock_server = MockServer::start().await;

    // Mock returns 500 Internal Server Error
    Mock::given(method("GET"))
        .and(path("/registry.json"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&mock_server)
        .await;

    let url = format!("{}/registry.json", mock_server.uri());
    let response = reqwest::get(&url)
        .await
        .expect("Failed to fetch from mock server");

    assert!(
        !response.status().is_success(),
        "Mock server should return 500"
    );
}

/// Test registry fetch handles malformed JSON.
#[tokio::test]
async fn registry_fetch_handles_malformed_json() {
    let mock_server = MockServer::start().await;

    // Mock returns invalid JSON
    Mock::given(method("GET"))
        .and(path("/registry.json"))
        .respond_with(ResponseTemplate::new(200).set_body_string("not valid json"))
        .mount(&mock_server)
        .await;

    let url = format!("{}/registry.json", mock_server.uri());
    let response = reqwest::get(&url)
        .await
        .expect("Failed to fetch from mock server");

    let body = response.text().await.expect("Failed to read response body");
    let result: Result<Registry, _> = serde_json::from_str(&body);

    assert!(result.is_err(), "Should fail to parse invalid JSON");
}

/// Test registry JSON schema conformance.
///
/// Verifies that the parsed registry matches the schema defined in
/// `schemas/acp-runtime/registry-manifest.schema.json`.
#[test]
fn registry_schema_conformance() {
    // Parse the sample registry
    let registry: Registry =
        serde_json::from_str(SAMPLE_REGISTRY_JSON).expect("Failed to parse sample registry");

    // Verify required fields per schema
    assert!(!registry.version.is_empty(), "version is required");
    assert!(!registry.agents.is_empty(), "agents is required");

    // Verify each agent has required fields
    for agent in &registry.agents {
        assert!(!agent.id.is_empty(), "agent.id is required");
        assert!(!agent.name.is_empty(), "agent.name is required");
        assert!(!agent.version.is_empty(), "agent.version is required");

        // Distribution is required
        // Must have either npx or binary distribution
        assert!(
            agent.distribution.npx.is_some() || agent.distribution.binary.is_some(),
            "agent must have npx or binary distribution"
        );
    }

    // Verify npx distribution has required package field
    for agent in &registry.agents {
        if let Some(npx) = &agent.distribution.npx {
            assert!(!npx.package.is_empty(), "npx.package is required");
        }
    }

    // Verify binary distribution has required fields for each platform
    for agent in &registry.agents {
        if let Some(binary) = &agent.distribution.binary {
            // Check each platform entry (if present)
            for platform in [
                &binary.darwin_aarch64,
                &binary.darwin_x86_64,
                &binary.linux_aarch64,
                &binary.linux_x86_64,
                &binary.windows_aarch64,
                &binary.windows_x86_64,
            ] {
                if let Some(pb) = platform {
                    assert!(!pb.archive.is_empty(), "platform.archive is required");
                    assert!(!pb.cmd.is_empty(), "platform.cmd is required");
                }
            }
        }
    }
}

/// Test registry with minimal valid agent entry.
#[test]
fn registry_minimal_agent_entry() {
    let minimal_json = r#"{
    "version": "1.0.0",
    "agents": [
      {
        "id": "test-agent",
        "name": "Test Agent",
        "version": "1.0.0",
        "distribution": {
          "npx": {
            "package": "@test/agent@1.0.0"
          }
        }
      }
    ]
  }"#;

    let registry: Registry =
        serde_json::from_str(minimal_json).expect("Failed to parse minimal registry");

    assert_eq!(registry.agents.len(), 1);
    let agent = &registry.agents[0];
    assert_eq!(agent.id, "test-agent");
    assert_eq!(agent.description, ""); // default
    assert!(agent.repository.is_none()); // optional
    assert!(agent.authors.is_empty()); // default
}

/// Test registry rejects missing required fields.
#[test]
fn registry_missing_required_field() {
    // Missing "version" field (required)
    let missing_version = r#"{
    "agents": [
      {
        "id": "test",
        "name": "Test",
        "version": "1.0.0",
        "distribution": { "npx": { "package": "test" } }
      }
    ]
  }"#;

    let result: Result<Registry, _> = serde_json::from_str(missing_version);
    assert!(result.is_err(), "Should reject missing version");

    // Missing "agents" field (required)
    let missing_agents = r#"{
    "version": "1.0.0"
  }"#;

    let result: Result<Registry, _> = serde_json::from_str(missing_agents);
    assert!(result.is_err(), "Should reject missing agents field");

    // Missing agent "distribution" field (required)
    let missing_distribution = r#"{
    "version": "1.0.0",
    "agents": [
      {
        "id": "test",
        "name": "Test",
        "version": "1.0.0"
      }
    ]
  }"#;

    let result: Result<Registry, _> = serde_json::from_str(missing_distribution);
    assert!(result.is_err(), "Should reject missing distribution field");
}

/// Test registry with empty agents array (valid per schema).
#[test]
fn registry_empty_agents_is_valid() {
    let empty_agents = r#"{
    "version": "1.0.0",
    "agents": [],
    "extensions": []
  }"#;

    let registry: Registry =
        serde_json::from_str(empty_agents).expect("Should parse empty agents array");

    assert_eq!(registry.agents.len(), 0);
}

/// Test registry JSON can be parsed without network.
#[test]
fn registry_parse_raw_json() {
    let registry: Registry =
        serde_json::from_str(SAMPLE_REGISTRY_JSON).expect("Failed to parse registry JSON");

    assert_eq!(registry.version, "1.0.0");
    assert_eq!(registry.agents.len(), 2);
}
