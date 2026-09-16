//! Real `hostQuery` dispatch over the embedded [`HostFacade`] registry, merged
//! with the structured JS-provider state so a JS-provider session/operation is
//! queryable with exactly the generated response shape.

use nexus_agent_host::core::session::HostSession as RegistryHostSession;
use nexus_agent_host::discovery::path_scan;
use nexus_agent_host::ids::{HostOperationId, HostSessionId};
use nexus_agent_host::providers::port::{operation_status_wire, protocol_kind_wire};
use nexus_agent_host::{HostFacade, HostManager, LaunchStrategy};
use nexus_contracts::core_host_query::{CoreHostQueryFormat, CoreHostQueryQuery};
use nexus_contracts::core_host_query_response::{
    CoreHostQueryResponse, CoreHostQueryResponseCatalog, CoreHostQueryResponseCatalogProvidersItem,
    CoreHostQueryResponseHealth, CoreHostQueryResponseScan, NexusAgentHostOperationResponse,
    NexusAgentHostSessionListResponse, NexusAgentHostSessionResponse, NexusAgentScanEntry,
    NexusPaginationInfo,
};
use nexus_contracts::CoreHostQuery;
use uuid::Uuid;

use super::core_error;
use super::env_state::{EnvState, JsSessionRecord};

fn session_wire(session: &RegistryHostSession) -> NexusAgentHostSessionResponse {
    NexusAgentHostSessionResponse {
        session_id: session.id.to_string(),
        provider_id: session.provider_id.to_string(),
        state: format!("{:?}", session.state),
        active_op_id: session
            .active_op_id
            .as_ref()
            .map(std::string::ToString::to_string),
        model: None,
        actor_ref: None,
        viewpoint: None,
    }
}

/// Wire shape for a JS-provider session. JS adapter ids are arbitrary strings,
/// so they are compared raw and never UUID-parsed.
fn js_session_wire(record: &JsSessionRecord) -> NexusAgentHostSessionResponse {
    NexusAgentHostSessionResponse {
        session_id: record.session_id.clone(),
        provider_id: record.provider_id.clone(),
        state: if record.active_operation_id.is_some() {
            "Busy".to_string()
        } else {
            "Ready".to_string()
        },
        active_op_id: record.active_operation_id.clone(),
        model: None,
        actor_ref: None,
        viewpoint: None,
    }
}

fn path_probe_dirs() -> Vec<std::path::PathBuf> {
    std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect())
        .unwrap_or_default()
}

/// Stable registry snapshot: sorted by session id so cursor/limit boundaries are
/// deterministic even though the registry is map-backed.
async fn sorted_sessions(
    host: &std::sync::Arc<HostManager>,
) -> Result<Vec<RegistryHostSession>, String> {
    let mut sessions = host
        .list_sessions()
        .await
        .map_err(core_error::open_reason_from_host)?;
    sessions.sort_by_key(|a| a.id.to_string());
    Ok(sessions)
}

/// Deterministic merged session list: native and JS entries share one sort key,
/// with native HostManager precedence on a raw session_id collision, so
/// pagination/order/not-found semantics are identical for both.
fn merge_session_wires(
    native: &[RegistryHostSession],
    js: &[JsSessionRecord],
) -> Vec<NexusAgentHostSessionResponse> {
    let mut items: Vec<NexusAgentHostSessionResponse> = native.iter().map(session_wire).collect();
    for record in js {
        if items.iter().any(|i| i.session_id == record.session_id) {
            continue;
        }
        items.push(js_session_wire(record));
    }
    items.sort_by(|a, b| a.session_id.cmp(&b.session_id));
    items
}

fn js_sessions_snapshot(state: &EnvState) -> Vec<JsSessionRecord> {
    state
        .with_js_state(|s| s.sessions().cloned().collect::<Vec<_>>())
        .unwrap_or_default()
}

/// A host rejection that means "not found" is what get_session/get_operation
/// report after both branches miss. Every other error propagates.
fn not_found(reason: &str) -> String {
    core_error::open_reason_not_found(reason)
}

pub async fn dispatch_host_query(
    state: &EnvState,
    request: CoreHostQuery,
) -> Result<CoreHostQueryResponse, String> {
    let host = state
        .host
        .lock()
        .map_err(|_| core_error::open_reason_internal())?
        .clone()
        .ok_or_else(|| core_error::open_reason_invalid_input("host not started"))?;
    match request.query {
        CoreHostQueryQuery::Health => {
            let health = host
                .health()
                .await
                .map_err(core_error::open_reason_from_host)?;
            // Native precedence: only count JS sessions that are not already
            // represented in the native registry, so a collision cannot
            // double-count a session or its operation.
            let native_ids: Vec<String> = sorted_sessions(&host)
                .await?
                .into_iter()
                .map(|s| s.id.to_string())
                .collect();
            let js = js_sessions_snapshot(state);
            let unique_js: Vec<&JsSessionRecord> = js
                .iter()
                .filter(|s| !native_ids.contains(&s.session_id))
                .collect();
            let js_active_ops = unique_js
                .iter()
                .filter(|s| s.active_operation_id.is_some())
                .count();
            let active_sessions = health.active_sessions + unique_js.len();
            let active_operations = health.active_operations + js_active_ops;
            Ok(CoreHostQueryResponse {
                health: Some(CoreHostQueryResponseHealth {
                    running: health.running || !js.is_empty(),
                    active_sessions: u64::try_from(active_sessions)
                        .map_err(|_| core_error::open_reason_internal())?,
                    active_operations: u64::try_from(active_operations)
                        .map_err(|_| core_error::open_reason_internal())?,
                }),
                catalog: None,
                sessions: None,
                session: None,
                operation: None,
                scan: None,
            })
        }
        CoreHostQueryQuery::Catalog => {
            let format = request.format.unwrap_or(CoreHostQueryFormat::Catalog);
            match format {
                CoreHostQueryFormat::Catalog => {
                    let mut catalog = host
                        .provider_catalog()
                        .await
                        .map_err(core_error::open_reason_from_host)?;
                    catalog.entries.sort_by_key(|a| a.provider_id.to_string());
                    Ok(CoreHostQueryResponse {
                        catalog: Some(CoreHostQueryResponseCatalog {
                            providers: catalog
                                .entries
                                .into_iter()
                                .map(|entry| CoreHostQueryResponseCatalogProvidersItem {
                                    provider_id: entry.provider_id.to_string(),
                                    display_name: entry.display_name,
                                    protocol_kind: protocol_kind_wire(entry.protocol_kind),
                                })
                                .collect(),
                        }),
                        health: None,
                        sessions: None,
                        session: None,
                        operation: None,
                        scan: None,
                    })
                }
                CoreHostQueryFormat::Scan => {
                    let config = HostManager::agent_config(&host).await;
                    let probe_dirs = path_probe_dirs();
                    let native_entries = path_scan::scan_path_in(&config, &[], &probe_dirs)
                        .map_err(core_error::open_reason_from_host)?;
                    let entries: Vec<NexusAgentScanEntry> = native_entries
                        .into_iter()
                        .map(|entry| {
                            let launch_command = match &entry.launch {
                                LaunchStrategy::Acp { command, .. }
                                | LaunchStrategy::NativeCli { command, .. } => {
                                    Some(command.clone())
                                }
                            };
                            NexusAgentScanEntry {
                                name: entry.display_name,
                                installed: entry.health.available,
                                launch_command,
                                description: entry.health.message.clone(),
                                icon_url: None,
                                registry_agent_id: None,
                                version: None,
                            }
                        })
                        .collect();
                    Ok(CoreHostQueryResponse {
                        scan: Some(CoreHostQueryResponseScan { entries }),
                        health: None,
                        catalog: None,
                        sessions: None,
                        session: None,
                        operation: None,
                    })
                }
            }
        }
        CoreHostQueryQuery::ListSessions => {
            let native = sorted_sessions(&host).await?;
            let js = js_sessions_snapshot(state);
            let items_all = merge_session_wires(&native, &js);
            let limit = request
                .limit
                .map_or(50, std::num::NonZero::get)
                .clamp(1, 250);
            let limit_us = usize::try_from(limit).unwrap_or(250);
            let items: Vec<NexusAgentHostSessionResponse> = items_all
                .into_iter()
                .skip_while(|s| {
                    request
                        .cursor
                        .as_ref()
                        .is_some_and(|cursor| s.session_id.as_str() <= cursor.as_str())
                })
                .take(limit_us)
                .collect();
            let next_cursor = if items.len() == limit_us {
                items.last().map(|i| i.session_id.clone())
            } else {
                None
            };
            Ok(CoreHostQueryResponse {
                sessions: Some(NexusAgentHostSessionListResponse {
                    items,
                    pagination: NexusPaginationInfo {
                        limit: i64::try_from(limit)
                            .map_err(|_| core_error::open_reason_internal())?,
                        has_more: next_cursor.is_some(),
                        next_cursor,
                    },
                }),
                health: None,
                catalog: None,
                session: None,
                operation: None,
                scan: None,
            })
        }
        CoreHostQueryQuery::GetSession => {
            let raw = request.session_id.as_deref().ok_or_else(|| {
                core_error::open_reason_invalid_input("get_session requires session_id")
            })?;
            // Native branch only for UUID-shaped ids; never parse a JS id.
            if let Ok(uuid) = Uuid::parse_str(raw) {
                let native = sorted_sessions(&host).await?;
                if let Some(session) = native.iter().find(|s| s.id == HostSessionId(uuid)) {
                    return Ok(CoreHostQueryResponse {
                        session: Some(session_wire(session)),
                        health: None,
                        catalog: None,
                        sessions: None,
                        operation: None,
                        scan: None,
                    });
                }
            }
            // JS branch: raw-id compare.
            let js = state.with_js_state(|s| s.session(raw).cloned()).flatten();
            if let Some(record) = js {
                return Ok(CoreHostQueryResponse {
                    session: Some(js_session_wire(&record)),
                    health: None,
                    catalog: None,
                    sessions: None,
                    operation: None,
                    scan: None,
                });
            }
            Err(not_found("session not found"))
        }
        CoreHostQueryQuery::GetOperation => {
            let raw = request.operation_id.as_deref().ok_or_else(|| {
                core_error::open_reason_invalid_input("get_operation requires operation_id")
            })?;
            // Native branch first for a UUID, matching GetSession precedence.
            if let Ok(uuid) = Uuid::parse_str(raw) {
                let op_id = HostOperationId(uuid);
                let native = sorted_sessions(&host).await?;
                if let Some(session) = native
                    .iter()
                    .find(|s| s.state.active_op_id() == Some(&op_id))
                {
                    return Ok(CoreHostQueryResponse {
                        operation: Some(NexusAgentHostOperationResponse {
                            operation_id: op_id.to_string(),
                            session_id: session.id.to_string(),
                            status: operation_status_wire(&session.state, &op_id).to_string(),
                            capture: None,
                        }),
                        health: None,
                        catalog: None,
                        sessions: None,
                        session: None,
                        scan: None,
                    });
                }
            }
            // JS branch: raw-id compare (adapter ids are arbitrary strings).
            if let Some(op) = state.with_js_state(|s| s.operation(raw)).flatten() {
                return Ok(CoreHostQueryResponse {
                    operation: Some(NexusAgentHostOperationResponse {
                        operation_id: op.operation_id,
                        session_id: op.session_id,
                        status: op.status.wire().to_string(),
                        capture: None,
                    }),
                    health: None,
                    catalog: None,
                    sessions: None,
                    session: None,
                    scan: None,
                });
            }
            // Durable journal fallback: after a restart the in-memory state is
            // gone, but a previously active op is still queryable as
            // interrupted, read through the CoreService-owned journal.
            if let Some(core) = state.journal_core() {
                if let Ok(Some((operation_id, session_id, _provider_id, status))) =
                    core.provider_operation_row_internal(raw).await
                {
                    return Ok(CoreHostQueryResponse {
                        operation: Some(NexusAgentHostOperationResponse {
                            operation_id,
                            session_id,
                            status,
                            capture: None,
                        }),
                        health: None,
                        catalog: None,
                        sessions: None,
                        session: None,
                        scan: None,
                    });
                }
            }
            Err(not_found("operation not found"))
        }
    }
}
