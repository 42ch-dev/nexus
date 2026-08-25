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
//! (AR-68 #2/#6). Builtin rows emit their authored `CatalogDescriptor`
//! schemas (AR-78, DF-89): real draft-2020-12 input schema when authored,
//! the named placeholder (`NAMED_PLACEHOLDER_INPUT`) for ledgered rows, and
//! the authored output schema when pinned.

use crate::api::errors::NexusApiError;
use crate::capability_registry::{
    host_tool_registry, json_schema_has_object_root, user_cap_catalog_admission,
    NAMED_PLACEHOLDER_INPUT,
};
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

    // Static `nexus.*` rows (AR-78, DF-89): authored `CatalogDescriptor`
    // schemas flow registry → catalog verbatim; a ledgered row (input
    // schema not yet authored) emits the named placeholder — never the
    // silent `{"type":"object"}` of V1.174.
    for id in host_tool_registry().ids() {
        if let Some(row) = host_tool_registry().lookup(id) {
            items.push(CatalogTool {
                id: id.to_owned(),
                description: row.catalog.description.to_owned(),
                input_schema: row
                    .catalog
                    .input_schema
                    .unwrap_or(NAMED_PLACEHOLDER_INPUT)
                    .to_owned(),
                output_schema: row.catalog.output_schema.map(ToOwned::to_owned),
                origin: "builtin".to_owned(),
            });
        }
    }

    // Admitted user capabilities (AR-68 #6 catalog admission).
    if let Some(reg) = state.capability_registry() {
        for cap in reg.iter() {
            if let Ok(cap) = user_cap_catalog_admission(Some(cap)) {
                // AR-70 §3 inclusion rule (same as peers): the MCP tools
                // surface only carries JSON-Schema object tools, so the
                // user output schema is carried iff it parses and declares
                // a root `type: "object"`; non-object outputs are omitted,
                // never invented, never wrapped.
                let output_schema = json_schema_has_object_root(cap.output_schema())
                    .then(|| cap.output_schema().to_owned());
                items.push(CatalogTool {
                    id: cap.name().to_owned(),
                    description: cap.name().to_owned(),
                    input_schema: cap.input_schema().to_owned(),
                    output_schema,
                    origin: "user".to_owned(),
                });
            }
        }
    }

    // PeerToolTable entries (AR-68 #7; compiled under connect-client).
    #[cfg(feature = "connect-client")]
    for entry in crate::connect::peer_tool_table().entries() {
        // MCP catalog projection gate (AR-70 §3, lockstep-pinned per
        // AR-74): the MCP tools surface only carries JSON-Schema object
        // tools. A peer row whose `input` is not a root `type: "object"`
        // is refused from the CATALOG with a named refusal — its
        // registration lane (`PeerToolTable`) is untouched and the tool
        // stays dispatchable through the spine.
        if let Err(refusal) = crate::connect::mcp_catalog_admission(&entry.descriptor) {
            tracing::warn!(
                tool_id = %entry.descriptor.capability_id.as_str(),
                peer_id = %entry.peer_id,
                refusal = ?refusal,
                "peer tool refused from MCP catalog (input_schema not root-object)"
            );
            continue;
        }
        let id = String::from(entry.descriptor.capability_id.clone());
        // Output schema carried iff present AND root-object (AR-70 §3
        // inclusion rule; non-object outputs are omitted, never emitted).
        let output_schema =
            crate::connect::mcp_catalog_output_root_object(&entry.descriptor).then(|| {
                serde_json::to_string(&entry.descriptor.output).unwrap_or_else(|_| "{}".to_owned())
            });
        items.push(CatalogTool {
            id,
            description: String::from(entry.descriptor.description.clone()),
            input_schema: serde_json::to_string(&entry.descriptor.input)
                .unwrap_or_else(|_| "{}".to_owned()),
            output_schema,
            origin: "peer".to_owned(),
        });
    }
    items.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(Json(serde_json::json!({ "items": items })))
}
