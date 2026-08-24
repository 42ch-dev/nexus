//! Capabilities listing handler.

use crate::api::errors::NexusApiError;
use crate::api::pagination::{decode_offset_cursor, encode_offset_cursor};
use crate::api::sort::{compare_by_terms, parse_sort_terms};
use crate::workspace::WorkspaceState;
use axum::{
    extract::{Query, State},
    Json,
};
use nexus_contracts::local::orchestration::http::{
    CapabilityInfo, ListCapabilitiesQuery, ListCapabilitiesResponse,
};
use nexus_contracts::PaginationInfo;

/// `GET /v1/daemon/orchestration/capabilities`
///
/// Provenance mapping (AR-40): each row's `origin` is produced by the
/// closure in the `registry.iter().map(...)` below — `CapabilityOrigin`
/// (the trait marker, `capability/mod.rs`) → wire string, with
/// `Builtin → "builtin"`, `User → "user"`. This is the single handler-side
/// mapping fn for the provenance layer (marker → handler → wire); the CLI
/// and web layers consume the wire string, never the enum (dependency
/// direction, AR-40/AR-42).
///
/// # Errors
/// Returns `NexusApiError::BadRequest` if `sort` contains an unsupported key
/// or invalid syntax.
pub async fn list_capabilities(
    State(state): State<WorkspaceState>,
    Query(query): Query<ListCapabilitiesQuery>,
) -> Result<Json<ListCapabilitiesResponse>, NexusApiError> {
    let sort_terms = parse_sort_terms(query.sort.as_deref(), &["name"], "capability")?;

    let Some(registry) = state.capability_registry() else {
        return Ok(Json(ListCapabilitiesResponse {
            items: Vec::new(),
            pagination: PaginationInfo {
                limit: i64::from(query.limit.unwrap_or(100).min(500)),
                next_cursor: None,
                has_more: false,
            },
        }));
    };

    let mut capabilities: Vec<CapabilityInfo> = registry
        .iter()
        .map(|cap| CapabilityInfo {
            name: cap.name().to_string(),
            input_schema: cap.input_schema().to_string(),
            output_schema: cap.output_schema().to_string(),
            origin: match cap.origin() {
                nexus_orchestration::capability::CapabilityOrigin::Builtin => "builtin",
                nexus_orchestration::capability::CapabilityOrigin::User => "user",
            }
            .to_string(),
        })
        .collect();

    // AR-68 #5: merge PeerToolTable rows as `origin: "peer"` (schemas
    // verbatim) behind connect-client. The orchestration `CapabilityOrigin`
    // enum stays `Builtin|User`; the wire string is produced here.
    #[cfg(feature = "connect-client")]
    for entry in crate::connect::peer_tool_table().entries() {
        capabilities.push(CapabilityInfo {
            name: String::from(entry.descriptor.capability_id.clone()),
            input_schema: serde_json::to_string(&entry.descriptor.input)
                .unwrap_or_else(|_| "{}".to_owned()),
            output_schema: serde_json::to_string(&entry.descriptor.output)
                .unwrap_or_else(|_| "{}".to_owned()),
            origin: "peer".to_owned(),
        });
    }

    capabilities.sort_by(|a, b| {
        compare_by_terms(a, b, &sort_terms, |key, a, b| match key {
            "name" => Some(a.name.cmp(&b.name)),
            _ => None,
        })
    });

    let offset = decode_offset_cursor(&query.cursor)?;
    let limit: u32 = query.limit.unwrap_or(100).min(500);
    let total = capabilities.len();
    let start = usize::try_from(offset).unwrap_or(0).min(total);
    let end = start
        .saturating_add(usize::try_from(limit).unwrap_or(total))
        .min(total);
    let page_items: Vec<CapabilityInfo> = capabilities.drain(start..end).collect();
    let has_more = end < total;
    let next_cursor = if has_more {
        Some(encode_offset_cursor(offset.saturating_add(limit)))
    } else {
        None
    };

    Ok(Json(ListCapabilitiesResponse {
        items: page_items,
        pagination: PaginationInfo {
            limit: i64::from(limit),
            next_cursor,
            has_more,
        },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::sync::Arc;

    /// Write an admitted `<name>/capability.json` trio at
    /// `<root>/capabilities/<name>/` (AR-35 layout): a hash-consistent
    /// `manifest.json` + `<module-id>.wasm` pair so the AR-43 admission gates
    /// pass inside the scan (same fixture shape as
    /// `tests/capability_scan_boot.rs`).
    fn write_capability_dir(root: &Path, name: &str) {
        use sha2::{Digest, Sha256};
        use std::fmt::Write as _;
        let dir = root.join("capabilities").join(name);
        std::fs::create_dir_all(&dir).unwrap();
        let wasm = b"fake module bytes";
        let sha: String = {
            let mut hex = String::with_capacity(64);
            for b in Sha256::digest(wasm) {
                let _ = write!(hex, "{b:02x}");
            }
            hex
        };
        let descriptor = format!(
            r#"{{
                "name": "{name}",
                "inputSchema": "{{\"type\":\"object\"}}",
                "outputSchema": "{{\"type\":\"object\"}}",
                "wasm": {{ "moduleId": "basic-combat", "wasmSha256": "{sha}" }}
            }}"#
        );
        std::fs::write(dir.join("capability.json"), descriptor).unwrap();
        let manifest = format!(
            r#"{{
                "module_id": "basic-combat",
                "name": "Basic Combat",
                "version": "1.0.0",
                "nexus_abi_version": 1,
                "required_key_block_types": [],
                "compute_export": "compute",
                "init_export": "",
                "wasm_sha256": "{sha}"
            }}"#
        );
        std::fs::write(dir.join("manifest.json"), manifest).unwrap();
        std::fs::write(dir.join("basic-combat.wasm"), wasm).unwrap();
    }

    #[tokio::test]
    async fn list_capabilities_marks_builtins_builtin() {
        let (tmp, nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        let mut state =
            crate::workspace::WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
        let registry = Arc::new(nexus_orchestration::CapabilityRegistry::with_builtins());
        state.set_capability_registry(registry);

        let Json(resp) = list_capabilities(State(state), Query(ListCapabilitiesQuery::default()))
            .await
            .expect("list_capabilities should succeed");

        assert!(!resp.items.is_empty(), "builtins must be listed");
        for cap in &resp.items {
            assert_eq!(
                cap.origin, "builtin",
                "builtin capability {} must carry origin=builtin",
                cap.name
            );
        }
        std::mem::forget(tmp);
    }

    #[tokio::test]
    async fn list_capabilities_marks_user_capability_user() {
        let tmp_root = tempfile::TempDir::new().unwrap();
        write_capability_dir(tmp_root.path(), "demo.pull");
        let scan_dir = tmp_root.path().join("capabilities");

        let (tmp, nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        let mut state =
            crate::workspace::WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
        let deps = nexus_orchestration::CapabilityRuntimeDeps {
            pool: None,
            worker_provider: None,
            daemon_tool_dispatch: None,
            cdn_config: None,
        };
        let (registry, outcome) =
            nexus_orchestration::CapabilityRegistry::with_runtime_deps_and_user_caps(
                &deps, &scan_dir,
            );
        assert!(
            outcome.skipped.is_empty(),
            "no skips expected: {:?}",
            outcome.skipped
        );
        state.set_capability_registry(Arc::new(registry));

        let Json(resp) = list_capabilities(State(state), Query(ListCapabilitiesQuery::default()))
            .await
            .expect("list_capabilities should succeed");

        let user = resp
            .items
            .iter()
            .find(|cap| cap.name == "demo.pull")
            .unwrap_or_else(|| panic!("demo.pull not in capabilities list"));
        assert_eq!(user.origin, "user", "user capability carries origin=user");

        // Builtins still report builtin alongside the user entry.
        let builtin = resp
            .items
            .iter()
            .find(|cap| cap.name == "narrative.compute")
            .expect("builtin narrative.compute present");
        assert_eq!(builtin.origin, "builtin");

        std::mem::forget(tmp);
    }

    #[tokio::test]
    async fn list_capabilities_without_registry_returns_empty_and_ok() {
        let (tmp, nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        let state =
            crate::workspace::WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
        // No registry wired — the handler returns an empty page, not an error.
        let result = list_capabilities(State(state), Query(ListCapabilitiesQuery::default())).await;
        assert!(result.is_ok(), "no-registry list must not error");
        std::mem::forget(tmp);
    }
}
