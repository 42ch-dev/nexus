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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_and_missing_input_returns_empty_terms() {
        assert!(parse_sort_terms(None, &["a", "b"], "work")
            .unwrap()
            .is_empty());
        assert!(parse_sort_terms(Some(""), &["a", "b"], "work")
            .unwrap()
            .is_empty());
        assert!(parse_sort_terms(Some("   "), &["a", "b"], "work")
            .unwrap()
            .is_empty());
    }

    #[test]
    fn trailing_and_leading_commas_are_ignored() {
        let terms = parse_sort_terms(Some(",a,b,"), &["a", "b"], "work").unwrap();
        assert_eq!(
            terms,
            vec![("a".to_string(), true), ("b".to_string(), true)]
        );
    }

    #[test]
    fn consecutive_commas_are_ignored() {
        let terms = parse_sort_terms(Some("a,,b"), &["a", "b"], "work").unwrap();
        assert_eq!(
            terms,
            vec![("a".to_string(), true), ("b".to_string(), true)]
        );
    }

    #[test]
    fn lone_minus_is_rejected() {
        let err = parse_sort_terms(Some("-"), &["a", "b"], "work").unwrap_err();
        assert_eq!(err.error_code(), "work_sort_invalid");
    }

    #[test]
    fn unknown_key_returns_resource_specific_code() {
        let err = parse_sort_terms(Some("unknown"), &["a", "b"], "schedule").unwrap_err();
        assert_eq!(err.error_code(), "schedule_sort_invalid");
    }

    #[test]
    fn descending_prefix_is_honored() {
        let terms = parse_sort_terms(Some("-a"), &["a", "b"], "work").unwrap();
        assert_eq!(terms, vec![("a".to_string(), false)]);
    }

    #[test]
    fn multi_key_precedence_is_preserved() {
        let terms = parse_sort_terms(Some("a,-b"), &["a", "b"], "work").unwrap();
        assert_eq!(
            terms,
            vec![("a".to_string(), true), ("b".to_string(), false)]
        );
    }

    #[test]
    fn unknown_key_in_multi_key_list_returns_resource_specific_code() {
        let err = parse_sort_terms(Some("a,unknown"), &["a", "b"], "capability").unwrap_err();
        assert_eq!(err.error_code(), "capability_sort_invalid");
    }
}
