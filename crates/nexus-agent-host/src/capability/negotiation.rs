//! Capability negotiation helpers.
//!
//! Merges provider static descriptor with ACP initialize response,
//! applies policy deny/allow toggles, and validates against session request.

use crate::capability::model::{CapabilityDescriptor, CreateSessionRequest};
use crate::error::{HostError, HostResult};
use crate::ids::ProviderId;

/// Negotiated capabilities after merging static descriptor, runtime response,
/// and policy.
#[derive(Debug, Clone)]
pub struct NegotiatedCapabilities {
    /// Final negotiated descriptor.
    pub descriptor: CapabilityDescriptor,
    /// List of capabilities that were denied by policy.
    pub denied: Vec<String>,
    /// List of capabilities that were downgraded (requested but unavailable).
    pub downgraded: Vec<String>,
}

/// Merge a static provider descriptor with ACP initialize response data.
///
/// The static descriptor represents what the host knows about the provider
/// at discovery time. The ACP initialize response provides runtime capability
/// information. This function intersects them: a capability is only `true`
/// if both sources agree.
#[must_use]
pub const fn merge_provider_capabilities(
    static_descriptor: &CapabilityDescriptor,
    runtime_caps: &CapabilityDescriptor,
) -> CapabilityDescriptor {
    CapabilityDescriptor {
        text_prompt: static_descriptor.text_prompt && runtime_caps.text_prompt,
        streaming: static_descriptor.streaming && runtime_caps.streaming,
        cancellation: static_descriptor.cancellation && runtime_caps.cancellation,
        session_restore: static_descriptor.session_restore && runtime_caps.session_restore,
        structured_tool_calls: static_descriptor.structured_tool_calls
            && runtime_caps.structured_tool_calls,
        mcp_http: static_descriptor.mcp_http && runtime_caps.mcp_http,
        mcp_sse: static_descriptor.mcp_sse && runtime_caps.mcp_sse,
        mcp_stdio: static_descriptor.mcp_stdio && runtime_caps.mcp_stdio,
        images: static_descriptor.images && runtime_caps.images,
        audio: static_descriptor.audio && runtime_caps.audio,
        embedded_context: static_descriptor.embedded_context && runtime_caps.embedded_context,
        set_model: static_descriptor.set_model && runtime_caps.set_model,
        set_mode: static_descriptor.set_mode && runtime_caps.set_mode,
        diagnostics: static_descriptor.diagnostics && runtime_caps.diagnostics,
    }
}

/// Apply policy deny toggles to a capability descriptor.
///
/// Capabilities in the deny set are forced to `false` regardless of
/// what the provider reports.
#[must_use]
pub fn apply_policy_deny(
    descriptor: &CapabilityDescriptor,
    denied_capabilities: &[String],
) -> (CapabilityDescriptor, Vec<String>) {
    let denied_set: std::collections::HashSet<&str> =
        denied_capabilities.iter().map(String::as_str).collect();

    let mut result = descriptor.clone();
    let mut actually_denied = Vec::new();

    let deny_field =
        |cap_name: &str, enabled: bool, denied_set: &std::collections::HashSet<&str>| {
            if denied_set.contains(cap_name) && enabled {
                (false, true)
            } else {
                (enabled, false)
            }
        };

    macro_rules! apply_deny {
        ($field:ident, $name:expr) => {
            let (val, was_denied) = deny_field($name, result.$field, &denied_set);
            result.$field = val;
            if was_denied {
                actually_denied.push($name.to_string());
            }
        };
    }

    apply_deny!(text_prompt, "text_prompt");
    apply_deny!(streaming, "streaming");
    apply_deny!(cancellation, "cancellation");
    apply_deny!(session_restore, "session_restore");
    apply_deny!(structured_tool_calls, "structured_tool_calls");
    apply_deny!(mcp_http, "mcp_http");
    apply_deny!(mcp_sse, "mcp_sse");
    apply_deny!(mcp_stdio, "mcp_stdio");
    apply_deny!(images, "images");
    apply_deny!(audio, "audio");
    apply_deny!(embedded_context, "embedded_context");
    apply_deny!(set_model, "set_model");
    apply_deny!(set_mode, "set_mode");
    apply_deny!(diagnostics, "diagnostics");

    (result, actually_denied)
}

/// Validate a session request against negotiated capabilities.
///
/// Returns an error if the request requires capabilities that are not
/// available in the negotiated descriptor.
///
/// # Errors
///
/// Returns `HostError::CapabilityUnsupported` if required capabilities are missing.
pub fn validate_session_request(
    provider_id: &ProviderId,
    descriptor: &CapabilityDescriptor,
    request: &CreateSessionRequest,
) -> HostResult<()> {
    if let Some(ref model) = request.model {
        if !descriptor.set_model {
            return Err(HostError::capability_unsupported(
                provider_id.clone(),
                "set_model",
                format!("provider does not support model switching (requested: {model})"),
            ));
        }
    }

    if let Some(ref mode) = request.mode {
        if !descriptor.set_mode {
            return Err(HostError::capability_unsupported(
                provider_id.clone(),
                "set_mode",
                format!("provider does not support mode switching (requested: {mode})"),
            ));
        }
    }

    // Check MCP server requirements
    for server in &request.mcp_servers {
        match server {
            crate::capability::model::McpServerConfig::Http { .. } => {
                if !descriptor.mcp_http {
                    return Err(HostError::capability_unsupported(
                        provider_id.clone(),
                        "mcp_http",
                        "provider does not support HTTP MCP servers",
                    ));
                }
            }
            crate::capability::model::McpServerConfig::Sse { .. } => {
                if !descriptor.mcp_sse {
                    return Err(HostError::capability_unsupported(
                        provider_id.clone(),
                        "mcp_sse",
                        "provider does not support SSE MCP servers",
                    ));
                }
            }
            crate::capability::model::McpServerConfig::Stdio { .. } => {
                if !descriptor.mcp_stdio {
                    return Err(HostError::capability_unsupported(
                        provider_id.clone(),
                        "mcp_stdio",
                        "provider does not support stdio MCP servers",
                    ));
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_provider_id() -> ProviderId {
        ProviderId::new("test-provider")
    }

    #[test]
    fn merge_acp_caps_with_runtime() {
        let static_caps = CapabilityDescriptor::acp_full();
        let runtime_caps = CapabilityDescriptor::acp_full();
        let merged = merge_provider_capabilities(&static_caps, &runtime_caps);
        assert!(merged.streaming);
        assert!(merged.set_model);
    }

    #[test]
    fn merge_caps_intersect() {
        let mut static_caps = CapabilityDescriptor::acp_full();
        static_caps.set_model = false;
        let runtime_caps = CapabilityDescriptor::acp_full();
        let merged = merge_provider_capabilities(&static_caps, &runtime_caps);
        assert!(!merged.set_model, "static disabled should override");
        assert!(merged.streaming, "both enabled should remain");
    }

    #[test]
    fn merge_native_with_runtime() {
        let static_caps = CapabilityDescriptor::native_cli_limited();
        let mut runtime_caps = CapabilityDescriptor::acp_full();
        runtime_caps.streaming = false; // runtime says no streaming
        let merged = merge_provider_capabilities(&static_caps, &runtime_caps);
        assert!(!merged.streaming);
        assert!(!merged.set_model);
        assert!(!merged.session_restore);
    }

    #[test]
    fn apply_policy_deny_removes_capabilities() {
        let caps = CapabilityDescriptor::acp_full();
        let denied = vec!["streaming".to_string(), "set_model".to_string()];
        let (result, actually_denied) = apply_policy_deny(&caps, &denied);
        assert!(!result.streaming);
        assert!(!result.set_model);
        assert!(result.text_prompt);
        assert_eq!(actually_denied.len(), 2);
    }

    #[test]
    fn apply_policy_deny_no_match() {
        let caps = CapabilityDescriptor::acp_full();
        let denied = vec!["nonexistent".to_string()];
        let (result, actually_denied) = apply_policy_deny(&caps, &denied);
        assert!(result.streaming);
        assert!(actually_denied.is_empty());
    }

    #[test]
    fn validate_session_request_ok() {
        let pid = test_provider_id();
        let caps = CapabilityDescriptor::acp_full();
        let request = CreateSessionRequest {
            provider_id: pid.clone(),
            cwd: std::path::PathBuf::from("/tmp"),
            model: Some("gpt-4".to_string()),
            mode: Some("ask".to_string()),
            mcp_servers: vec![],
            metadata: serde_json::Value::Null,
        };
        assert!(validate_session_request(&pid, &caps, &request).is_ok());
    }

    #[test]
    fn validate_session_request_model_not_supported() {
        let pid = test_provider_id();
        let caps = CapabilityDescriptor::native_cli_limited();
        let request = CreateSessionRequest {
            provider_id: pid.clone(),
            cwd: std::path::PathBuf::from("/tmp"),
            model: Some("gpt-4".to_string()),
            mode: None,
            mcp_servers: vec![],
            metadata: serde_json::Value::Null,
        };
        let result = validate_session_request(&pid, &caps, &request);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.category(), "capability_unsupported");
    }

    #[test]
    fn validate_session_request_mcp_http_not_supported() {
        let pid = test_provider_id();
        let caps = CapabilityDescriptor::native_cli_limited();
        let request = CreateSessionRequest {
            provider_id: pid.clone(),
            cwd: std::path::PathBuf::from("/tmp"),
            model: None,
            mode: None,
            mcp_servers: vec![crate::capability::model::McpServerConfig::Http {
                name: "test".to_string(),
                url: "https://example.com/mcp".to_string(),
            }],
            metadata: serde_json::Value::Null,
        };
        let result = validate_session_request(&pid, &caps, &request);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.category(), "capability_unsupported");
    }
}
