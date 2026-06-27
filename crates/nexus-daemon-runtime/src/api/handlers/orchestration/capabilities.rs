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

/// `GET /v1/local/orchestration/capabilities`
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
        })
        .collect();

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
