//! Real `hostQuery` dispatch over the ONE attached core Host authority, merged
//! with the structured JS-provider state so a JS-provider session/operation is
//! queryable with exactly the generated response shape.
//!
//! The native rows are **core-authorized**: they come from `HostHandle::query`,
//! so they carry the Actor pair (live or tombstoned) and the authority's owner
//! scoping. Native keeps no second registry of Actor authority — its own state
//! holds the provider-only JS view, which is merged after the native rows and
//! paged after the merge.

use nexus_agent_host::ids::HostOperationId;
use nexus_agent_host::providers::port::operation_status_wire;
use nexus_agent_host::HostFacade;
use nexus_contracts::core_host_query::{CoreHostQueryFormat, CoreHostQueryQuery};
use nexus_contracts::core_host_query_response::{
    CoreHostQueryResponse, CoreHostQueryResponseHealth, NexusAgentHostOperationResponse,
    NexusAgentHostSessionListResponse, NexusAgentHostSessionResponse, NexusPaginationInfo,
};
use nexus_contracts::CoreHostQuery;
use nexus_core::{HostHandle, Principal};
use uuid::Uuid;

use super::core_error;
use super::env_state::{EnvState, JsSessionRecord};

/// The page the authority is asked for while draining its native rows. The
/// merged page is what the caller's own `limit` applies to, so the drain walks
/// the authority in full rather than truncating the native side.
const NATIVE_ROW_PAGE: u64 = 250;

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

/// A host rejection that means "not found" is what get_session/get_operation
/// report after both branches miss. Every other error propagates.
fn not_found(reason: &str) -> String {
    core_error::open_reason_not_found(reason)
}

/// Deterministic merged session list: native (authority) entries share one sort
/// key with the JS entries and keep precedence on a raw session_id collision, so
/// pagination/order/not-found semantics are identical for both.
fn merge_session_wires(
    native: Vec<NexusAgentHostSessionResponse>,
    js: &[JsSessionRecord],
) -> Vec<NexusAgentHostSessionResponse> {
    let mut items = native;
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

/// The attached authority and the active principal every native query is scoped
/// to.
///
/// `hostQuery` carries no principal handle (its signature is unchanged), and the
/// native surface has exactly one open selection — so the principal is the one
/// that open verified. The authority re-verifies it on every query, which is
/// what keeps a cached TS value from bypassing the check.
async fn query_scope(state: &EnvState) -> Result<(std::sync::Arc<HostHandle>, Principal), String> {
    let authority = state
        .host_authority()
        .ok_or_else(|| core_error::open_reason_invalid_input("host not started"))?;
    let core = state
        .journal_core()
        .ok_or_else(|| core_error::open_reason_invalid_input("host not started"))?;
    let principal = core
        .active_principal()
        .await
        .map_err(core_error::open_reason_from_domain)?;
    Ok((authority, principal))
}

fn list_request(cursor: Option<String>) -> CoreHostQuery {
    CoreHostQuery {
        cursor,
        format: None,
        limit: std::num::NonZeroU64::new(NATIVE_ROW_PAGE),
        operation_id: None,
        query: CoreHostQueryQuery::ListSessions,
        session_id: None,
    }
}

fn single_request(query: CoreHostQueryQuery) -> CoreHostQuery {
    CoreHostQuery {
        cursor: None,
        format: None,
        limit: None,
        operation_id: None,
        query,
        session_id: None,
    }
}

/// Owner-scoped native session rows from the ONE core authority — Actor pair and
/// tombstone included — drained past `cursor`.
///
/// The authority pages its rows; the merge pages the union, so every page the
/// authority reports is drained here before the JS view joins it.
async fn native_session_rows(
    authority: &HostHandle,
    principal: &Principal,
    cursor: Option<String>,
) -> Result<Vec<NexusAgentHostSessionResponse>, String> {
    let mut rows: Vec<NexusAgentHostSessionResponse> = Vec::new();
    let mut next = cursor;
    loop {
        let response = authority
            .query(principal, list_request(next.clone()))
            .await
            .map_err(core_error::open_reason_from_domain)?;
        let list = response
            .sessions
            .ok_or_else(core_error::open_reason_internal)?;
        rows.extend(list.items);
        if !list.pagination.has_more {
            return Ok(rows);
        }
        match list.pagination.next_cursor {
            Some(cursor) => next = Some(cursor),
            // A full page without a cursor cannot be followed; the rows already
            // drained are the complete authority view.
            None => return Ok(rows),
        }
    }
}

pub async fn dispatch_host_query(
    state: &EnvState,
    request: CoreHostQuery,
) -> Result<CoreHostQueryResponse, String> {
    let (authority, principal) = query_scope(state).await?;
    match request.query {
        CoreHostQueryQuery::Health => {
            let response = authority
                .query(&principal, single_request(CoreHostQueryQuery::Health))
                .await
                .map_err(core_error::open_reason_from_domain)?;
            let health = response
                .health
                .ok_or_else(core_error::open_reason_internal)?;
            // Native precedence: only count JS sessions that are not already
            // represented in the core-authorized native registry, so a collision
            // cannot double-count a session or its operation.
            let native = native_session_rows(&authority, &principal, None).await?;
            let js = js_sessions_snapshot(state);
            let unique_js: Vec<&JsSessionRecord> = js
                .iter()
                .filter(|s| !native.iter().any(|row| row.session_id == s.session_id))
                .collect();
            let js_sessions =
                u64::try_from(unique_js.len()).map_err(|_| core_error::open_reason_internal())?;
            let js_active_ops = u64::try_from(
                unique_js
                    .iter()
                    .filter(|s| s.active_operation_id.is_some())
                    .count(),
            )
            .map_err(|_| core_error::open_reason_internal())?;
            Ok(CoreHostQueryResponse {
                health: Some(CoreHostQueryResponseHealth {
                    running: health.running || !js.is_empty(),
                    active_sessions: health.active_sessions + js_sessions,
                    active_operations: health.active_operations + js_active_ops,
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
            let mut native_request = single_request(CoreHostQueryQuery::Catalog);
            native_request.format = Some(format);
            let response = authority
                .query(&principal, native_request)
                .await
                .map_err(core_error::open_reason_from_domain)?;
            Ok(CoreHostQueryResponse {
                catalog: response.catalog,
                scan: response.scan,
                health: None,
                sessions: None,
                session: None,
                operation: None,
            })
        }
        CoreHostQueryQuery::ListSessions => {
            let native =
                native_session_rows(&authority, &principal, request.cursor.clone()).await?;
            let js = js_sessions_snapshot(state);
            let items_all = merge_session_wires(native, &js);
            let limit = request
                .limit
                .map_or(50, std::num::NonZero::get)
                .clamp(1, NATIVE_ROW_PAGE);
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
            // A raw comparison is safe and never parses a JS id: every native
            // session id is a UUID, so a JS-shaped id can never collide.
            let native = native_session_rows(&authority, &principal, None).await?;
            if let Some(row) = native.into_iter().find(|row| row.session_id == raw) {
                return Ok(CoreHostQueryResponse {
                    session: Some(row),
                    health: None,
                    catalog: None,
                    sessions: None,
                    operation: None,
                    scan: None,
                });
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
            // Native branch first for a UUID, matching GetSession precedence. The
            // row set is the authority's owner-scoped view; the wire status is
            // the manager session-state projection, which the row does not carry.
            // The durable journal fallback stays AFTER the JS branch below, so a
            // provider-only operation id is never preempted by its own journal
            // row.
            if let Ok(uuid) = Uuid::parse_str(raw) {
                let op_id = HostOperationId(uuid);
                let native = native_session_rows(&authority, &principal, None).await?;
                if native
                    .iter()
                    .any(|row| row.active_op_id.as_deref() == Some(raw))
                {
                    let sessions = authority
                        .manager()
                        .list_sessions()
                        .await
                        .map_err(core_error::open_reason_from_host)?;
                    if let Some(session) = sessions
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
