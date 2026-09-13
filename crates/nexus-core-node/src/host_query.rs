//! Real `hostQuery` dispatch over the embedded [`HostFacade`] registry.

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

use super::env_state::EnvState;

fn session_wire(session: &RegistryHostSession) -> NexusAgentHostSessionResponse {
    NexusAgentHostSessionResponse {
        session_id: session.id.to_string(),
        provider_id: session.provider_id.to_string(),
        state: format!("{:?}", session.state),
        active_op_id: session.active_op_id.as_ref().map(|id| id.to_string()),
        model: None,
        actor_ref: None,
        viewpoint: None,
    }
}

fn parse_session_id(raw: &str) -> Result<HostSessionId, String> {
    let uuid = Uuid::parse_str(raw).map_err(|e| format!("session_id: {e}"))?;
    Ok(HostSessionId(uuid))
}

fn parse_operation_id(raw: &str) -> Result<HostOperationId, String> {
    let uuid = Uuid::parse_str(raw).map_err(|e| format!("operation_id: {e}"))?;
    Ok(HostOperationId(uuid))
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
    let mut sessions = host.list_sessions().await.map_err(|e| e.to_string())?;
    sessions.sort_by(|a, b| a.id.to_string().cmp(&b.id.to_string()));
    Ok(sessions)
}

pub async fn dispatch_host_query(
    state: &EnvState,
    request: CoreHostQuery,
) -> Result<CoreHostQueryResponse, String> {
    let host = state
        .host
        .lock()
        .map_err(|_| "host mutex poisoned".to_string())?
        .clone()
        .ok_or_else(|| "host not started".to_string())?;
    match request.query {
        CoreHostQueryQuery::Health => {
            let health = host.health().await.map_err(|e| e.to_string())?;
            Ok(CoreHostQueryResponse {
                health: Some(CoreHostQueryResponseHealth {
                    running: health.running,
                    active_sessions: u64::try_from(health.active_sessions)
                        .map_err(|e| e.to_string())?,
                    active_operations: u64::try_from(health.active_operations)
                        .map_err(|e| e.to_string())?,
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
                    let mut catalog = host.provider_catalog().await.map_err(|e| e.to_string())?;
                    catalog
                        .entries
                        .sort_by(|a, b| a.provider_id.to_string().cmp(&b.provider_id.to_string()));
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
                        .map_err(|e| e.to_string())?;
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
            let sessions = sorted_sessions(&host).await?;
            let limit = request
                .limit
                .map(|n| n.get())
                .unwrap_or(50)
                .clamp(1, 250);
            let limit_us = usize::try_from(limit).unwrap_or(250);
            let items: Vec<NexusAgentHostSessionResponse> = sessions
                .iter()
                .skip_while(|s| {
                    request
                        .cursor
                        .as_ref()
                        .is_some_and(|cursor| s.id.to_string() <= *cursor)
                })
                .take(limit_us)
                .map(session_wire)
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
                        limit: i64::try_from(limit).map_err(|e| e.to_string())?,
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
            let session_id = request
                .session_id
                .as_deref()
                .ok_or_else(|| "get_session requires session_id".to_string())?;
            let sid = parse_session_id(session_id)?;
            let sessions = sorted_sessions(&host).await?;
            let session = sessions
                .iter()
                .find(|s| s.id == sid)
                .ok_or_else(|| format!("session {session_id} not found"))?;
            Ok(CoreHostQueryResponse {
                session: Some(session_wire(session)),
                health: None,
                catalog: None,
                sessions: None,
                operation: None,
                scan: None,
            })
        }
        CoreHostQueryQuery::GetOperation => {
            let operation_id = request
                .operation_id
                .as_deref()
                .ok_or_else(|| "get_operation requires operation_id".to_string())?;
            let op_id = parse_operation_id(operation_id)?;
            let sessions = sorted_sessions(&host).await?;
            // Status comes from the owning session's live state; an operation the
            // registry no longer tracks is reported as not found, never as a
            // fabricated constant.
            let session = sessions
                .iter()
                .find(|s| s.state.active_op_id() == Some(&op_id))
                .ok_or_else(|| format!("operation {operation_id} is not active"))?;
            Ok(CoreHostQueryResponse {
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
            })
        }
    }
}
