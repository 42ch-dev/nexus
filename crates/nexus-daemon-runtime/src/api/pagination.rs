//! Shared cursor-pagination helpers for Local API list endpoints (V1.64).
//!
//! Conventions established by `local-api-surface-conventions.md` §2: list
//! endpoints use opaque cursor-based pagination with a `PaginationInfo`
//! envelope (`{ limit, next_cursor, has_more }`).
//!
//! This module implements an **offset-backed opaque cursor**: the cursor is a
//! versioned token (`v1:<offset>`) that encodes the underlying SQL offset.
//! Clients MUST NOT parse it — the `v1:` prefix lets a future encoding coexist
//! with tokens minted by older daemons. Resources with small in-memory lists
//! (e.g. KB) instead key the cursor on the last item's stable id; both are
//! valid under the "opaque cursor" contract.

use crate::api::errors::NexusApiError;

/// Cursor token prefix. Bumping the version lets a future encoding coexist
/// with tokens minted by older daemons (old clients that send a stale cursor
/// get a `400` and simply re-request from page 1).
const CURSOR_PREFIX: &str = "v1:";

/// Encode a row offset into an opaque cursor token.
#[must_use]
pub fn encode_offset_cursor(offset: u32) -> String {
    format!("{CURSOR_PREFIX}{offset}")
}

/// Decode an opaque cursor token into the underlying row offset.
///
/// Returns `Ok(0)` when `cursor` is `None` (first page). Returns
/// `NexusApiError::BadRequest { code: "invalid_input" }` (HTTP 400) when the
/// token is malformed — callers surface this as the canonical
/// `<resource>_cursor_invalid` / `invalid_input` error per convention §3.2.
///
/// # Errors
/// - `NexusApiError::BadRequest` (`invalid_input`) if the cursor is present
///   but not a `v1:`-prefixed non-negative integer.
pub fn decode_offset_cursor(cursor: &Option<String>) -> Result<u32, NexusApiError> {
    match cursor {
        None => Ok(0),
        Some(raw) => {
            let stripped =
                raw.strip_prefix(CURSOR_PREFIX)
                    .ok_or_else(|| {
                        NexusApiError::BadRequest {
                    code: "invalid_input".to_string(),
                    message:
                        "invalid pagination cursor; pass the `next_cursor` value returned by the \
                         previous response unchanged"
                            .to_string(),
                }
                    })?;
            stripped
                .parse::<u32>()
                .map_err(|_| NexusApiError::BadRequest {
                    code: "invalid_input".to_string(),
                    message:
                        "invalid pagination cursor; pass the `next_cursor` value returned by the \
                     previous response unchanged"
                            .to_string(),
                })
        }
    }
}

/// Compute `(next_cursor, has_more)` for an offset-backed cursor page.
///
/// `fetched` is the number of rows the DAO returned when asked for `limit + 1`
/// (the overflow row is used only to detect `has_more` and is truncated by the
/// caller). When `fetched > limit`, another page exists: `has_more` is `true`
/// and `next_cursor` encodes `offset + limit`.
#[must_use]
pub fn offset_page_meta(fetched: usize, limit: u32, offset: u32) -> (Option<String>, bool) {
    let limit_us = usize::try_from(limit).unwrap_or(usize::MAX);
    if fetched > limit_us {
        let next_offset = offset.saturating_add(limit);
        (Some(encode_offset_cursor(next_offset)), true)
    } else {
        (None, false)
    }
}
