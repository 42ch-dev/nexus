//! Shared sort-parameter parser for Local API list endpoints (F-F1).
//!
//! Grammar: `sort := term ("," term)*`, `term := ["-"] key`.
//! `-key` means descending; `key` means ascending.

use crate::api::errors::NexusApiError;

/// Parse an optional comma-separated `sort` query string into validated terms.
///
/// Each term is validated against `allowed_keys`. Empty or missing input returns
/// an empty vector so the caller can apply its default ordering.
///
/// # Errors
///
/// Returns `NexusApiError::BadRequest` with code `<resource>_sort_invalid` when a
/// term uses an unknown key or is otherwise malformed.
pub fn parse_sort_terms(
    input: Option<&str>,
    allowed_keys: &[&str],
    resource: &str,
) -> Result<Vec<(String, bool)>, NexusApiError> {
    let Some(input) = input else {
        return Ok(Vec::new());
    };
    if input.is_empty() {
        return Ok(Vec::new());
    }

    let mut terms = Vec::new();
    for raw in input.split(',') {
        let raw = raw.trim();
        if raw.is_empty() {
            continue;
        }
        let (ascending, key) = raw
            .strip_prefix('-')
            .map_or((true, raw), |stripped| (false, stripped));
        if !allowed_keys.contains(&key) {
            return Err(NexusApiError::BadRequest {
                code: format!("{resource}_sort_invalid"),
                message: format!(
                    "unsupported sort key '{key}'; allowed: {}",
                    allowed_keys.join(", ")
                ),
            });
        }
        terms.push((key.to_string(), ascending));
    }
    Ok(terms)
}
