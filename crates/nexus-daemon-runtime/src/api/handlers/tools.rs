//! Spine catalog route — `GET /v1/daemon/tools` (V1.174 P0, AR-68 #7).
//!
//! One read face over the single dispatch spine: rows
//! `{ id, description, input_schema, output_schema?, origin: builtin|user|peer }`
//! for static `nexus.*` rows ∪ admitted user capabilities ∪ `PeerToolTable`
//! entries (peer merge compiled under `connect-client`; without the feature
//! the route lists builtin + user rows). This is a NEW daemon route (own
//! wire contract) — NOT a merge into
//! `GET /v1/daemon/orchestration/capabilities` (different registry,
//! different vocabulary; AR-68 #7 / PL-9).
//!
//! Honesty contract (AR-68 #7 + AR-70 §3): every listed id is dispatchable
//! through the spine, and the spine only contains ids that passed admission
//! (AR-68 #2/#6). Builtin rows emit the documented permissive
//! `{"type":"object"}` input placeholder and no output schema (AcpWire refs
//! are pseudo-schemas, not draft-2020-12).

use crate::api::errors::NexusApiError;
use crate::capability_registry::{host_tool_registry, user_cap_catalog_admission};
use crate::workspace::WorkspaceState;
use axum::extract::State;
use axum::Json;
use serde::Serialize;

/// One catalog row (wire shape mirrors `catalog-tool.schema.json`).
#[derive(Debug, Clone, Serialize)]
pub struct CatalogTool {
    pub id: String,
    pub description: String,
    pub input_schema: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<String>,
    pub origin: String,
}

/// `GET /v1/daemon/tools`
///
/// # Errors
/// Returns `NexusApiError` only on internal failures (the catalog is a
/// read-only projection of already-admitted state).
pub async fn list_tools(
    State(state): State<WorkspaceState>,
) -> Result<Json<serde_json::Value>, NexusApiError> {
    let mut items: Vec<CatalogTool> = Vec::new();

    // Static `nexus.*` rows: permissive input placeholder, no output schema
    // (AR-70 §3 — AcpWire refs are pseudo-schemas; the placeholder is
    // uniform and documented, never per-tool guessing).
    for id in host_tool_registry().ids() {
        if let Some(row) = host_tool_registry().lookup(id) {
            items.push(CatalogTool {
                id: id.to_owned(),
                description: row.handler_test_vector.description.to_owned(),
                input_schema: "{\"type\":\"object\"}".to_owned(),
                output_schema: None,
                origin: "builtin".to_owned(),
            });
        }
    }

    // Admitted user capabilities (AR-68 #6 catalog admission).
    if let Some(reg) = state.capability_registry() {
        for cap in reg.iter() {
            if let Ok(cap) = user_cap_catalog_admission(Some(cap)) {
                items.push(CatalogTool {
                    id: cap.name().to_owned(),
                    description: cap.name().to_owned(),
                    input_schema: cap.input_schema().to_owned(),
                    output_schema: Some(cap.output_schema().to_owned()),
                    origin: "user".to_owned(),
                });
            }
        }
    }

    // PeerToolTable entries (AR-68 #7; compiled under connect-client).
    #[cfg(feature = "connect-client")]
    for entry in crate::connect::peer_tool_table().entries() {
        let id = String::from(entry.descriptor.capability_id.clone());
        items.push(CatalogTool {
            id,
            description: String::from(entry.descriptor.description.clone()),
            input_schema: serde_json::to_string(&entry.descriptor.input)
                .unwrap_or_else(|_| "{}".to_owned()),
            output_schema: Some(
                serde_json::to_string(&entry.descriptor.output)
                    .unwrap_or_else(|_| "{}".to_owned()),
            ),
            origin: "peer".to_owned(),
        });
    }

    items.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(Json(serde_json::json!({ "items": items })))
}
