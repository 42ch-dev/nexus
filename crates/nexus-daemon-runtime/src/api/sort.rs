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

/// Compare two values by a list of parsed sort terms.
///
/// `resolve` is called for each term key and should return the natural
/// `Ordering` of `a` vs `b` for that key, or `None` if the key is unknown
/// (unknown keys are ignored). Ascending/descending direction is applied by
/// this function.
///
/// This closes the repeated `compare_*` closure pattern in list handlers
/// (R-V167P0-QC1-S-COMPARE).
pub fn compare_by_terms<T, F>(
    a: &T,
    b: &T,
    terms: &[(String, bool)],
    mut resolve: F,
) -> std::cmp::Ordering
where
    F: FnMut(&str, &T, &T) -> Option<std::cmp::Ordering>,
{
    for (key, ascending) in terms {
        if let Some(ord) = resolve(key, a, b) {
            let ord = if *ascending { ord } else { ord.reverse() };
            if ord != std::cmp::Ordering::Equal {
                return ord;
            }
        }
    }
    std::cmp::Ordering::Equal
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

    #[test]
    fn compare_by_terms_applies_ascending_and_descending_keys() {
        let terms = vec![("name".to_string(), true), ("age".to_string(), false)];
        let a = ("alice".to_string(), 30);
        let b = ("bob".to_string(), 25);

        // name asc → alice < bob
        assert_eq!(
            compare_by_terms(&a, &b, &terms, |key, x, y| match key {
                "name" => Some(x.0.cmp(&y.0)),
                "age" => Some(x.1.cmp(&y.1)),
                _ => None,
            }),
            std::cmp::Ordering::Less
        );

        // name tied, age desc → 30 before 25, so a < b
        let a2 = ("alice".to_string(), 30);
        let b2 = ("alice".to_string(), 25);
        assert_eq!(
            compare_by_terms(&a2, &b2, &terms, |key, x, y| match key {
                "name" => Some(x.0.cmp(&y.0)),
                "age" => Some(x.1.cmp(&y.1)),
                _ => None,
            }),
            std::cmp::Ordering::Less
        );
    }

    #[test]
    fn compare_by_terms_ignores_unknown_keys() {
        let terms = vec![("unknown".to_string(), true), ("x".to_string(), true)];
        let a = 1;
        let b = 2;
        assert_eq!(
            compare_by_terms(&a, &b, &terms, |_key, x, y| Some(x.cmp(y))),
            std::cmp::Ordering::Less
        );
    }
}
