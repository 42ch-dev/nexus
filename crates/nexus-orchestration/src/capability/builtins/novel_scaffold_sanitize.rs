//! Input sanitization for the `novel.project_scaffold` capability.
//!
//! V1.36 §5.4 — `work_ref`, slugs, and `total_planned_chapters` originate
//! from grill-me (untrusted LLM/user input) and are spliced into
//! filesystem paths and DB rows. Without validation, values containing
//! `..`, `/`, NUL, control characters, or uppercase mixed casing can
//! escape the workspace or break path conventions.
//!
//! Fixes QC findings C-1, C-4, and W-2.

use crate::capability::CapabilityError;

/// Maximum slug length (matches typical filesystem path budget).
pub const SLUG_MAX_LEN: usize = 64;

/// Minimum `total_planned_chapters` (worldless or world-bound).
pub const MIN_CHAPTERS: i32 = 1;
/// Maximum `total_planned_chapters` (matches `init-chapters.md` prompt range).
pub const MAX_CHAPTERS: i32 = 100;

/// Validate a user-supplied `work_ref` slug.
///
/// Accepts kebab-case alphanumeric slugs of length `1..=64`, lowercase only.
/// Rejects values containing `..`, `/`, `\`, NUL bytes, control characters,
/// uppercase letters, or any non-alphanumeric/hyphen characters. The first
/// character must be `[a-z0-9]` (no leading hyphen).
///
/// # Errors
///
/// Returns `CapabilityError::InputInvalid` with a precise reason on any
/// validation failure.
pub fn validate_work_ref(s: &str) -> Result<String, CapabilityError> {
    validate_slug_inner(s, "work_ref")
}

/// Validate a user-supplied chapter / generic slug (e.g. `ch01-foo`).
///
/// Same shape as `work_ref` — kebab-case `[a-z0-9][a-z0-9-]{0,63}`.
///
/// Exposed alongside `validate_work_ref` for symmetry; used when future
/// fields (chapter slug, genre slug) accept user-supplied tokens. Currently
/// not called by the scaffold capability — chapter slugs are auto-derived
/// from `ch{NN}` — but unit-tested below to lock in semantics.
///
/// # Errors
///
/// Returns `CapabilityError::InputInvalid` on any validation failure.
#[allow(dead_code)] // F1: sibling API for future user-supplied slug fields
pub fn validate_slug(s: &str) -> Result<String, CapabilityError> {
    validate_slug_inner(s, "slug")
}

fn validate_slug_inner(s: &str, label: &str) -> Result<String, CapabilityError> {
    if s.is_empty() {
        return Err(CapabilityError::InputInvalid(format!("{label} is empty")));
    }
    if s.len() > SLUG_MAX_LEN {
        return Err(CapabilityError::InputInvalid(format!(
            "{label} exceeds {SLUG_MAX_LEN} chars (got {})",
            s.len()
        )));
    }
    // Explicit path-traversal / separator rejection (defense-in-depth on top
    // of the character class check below).
    if s.contains("..") || s.contains('/') || s.contains('\\') || s.contains('\0') {
        return Err(CapabilityError::InputInvalid(format!(
            "{label} contains path-traversal or separator characters"
        )));
    }
    let mut chars = s.chars();
    let first = chars.next().expect("non-empty checked above");
    if !(first.is_ascii_lowercase() || first.is_ascii_digit()) {
        return Err(CapabilityError::InputInvalid(format!(
            "{label} must start with [a-z0-9] (got {first:?})"
        )));
    }
    for c in s.chars() {
        if c.is_ascii_control() {
            return Err(CapabilityError::InputInvalid(format!(
                "{label} contains control character"
            )));
        }
        if c.is_ascii_uppercase() {
            return Err(CapabilityError::InputInvalid(format!(
                "{label} must be lowercase (uppercase {c:?} rejected)"
            )));
        }
        let is_valid = c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-';
        if !is_valid {
            return Err(CapabilityError::InputInvalid(format!(
                "{label} contains invalid character {c:?} (allowed: [a-z0-9-])"
            )));
        }
    }
    Ok(s.to_string())
}

/// Validate `total_planned_chapters`.
///
/// Must be in the inclusive range `1..=100` to match the
/// `init-chapters.md` prompt advertised range.
///
/// # Errors
///
/// Returns `CapabilityError::InputInvalid` if the count is out of range.
pub fn validate_total_chapters(n: i32) -> Result<u32, CapabilityError> {
    if !(MIN_CHAPTERS..=MAX_CHAPTERS).contains(&n) {
        return Err(CapabilityError::InputInvalid(format!(
            "total_planned_chapters must be {MIN_CHAPTERS}..={MAX_CHAPTERS} (got {n})"
        )));
    }
    // i32 -> u32 is safe given the bounds above.
    Ok(u32::try_from(n).expect("bounded 1..=100"))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn rejects_dotdot_traversal() {
        assert!(validate_work_ref("..").is_err());
        assert!(validate_work_ref("foo/../bar").is_err());
        assert!(validate_work_ref("a..b").is_err());
    }

    #[test]
    fn rejects_slash_and_backslash() {
        assert!(validate_work_ref("a/b").is_err());
        assert!(validate_work_ref("a\\b").is_err());
    }

    #[test]
    fn rejects_empty() {
        assert!(validate_work_ref("").is_err());
    }

    #[test]
    fn rejects_uppercase() {
        assert!(validate_work_ref("MyNovel").is_err());
        assert!(validate_work_ref("aBc").is_err());
    }

    #[test]
    fn rejects_oversize() {
        let too_long = "a".repeat(65);
        assert!(validate_work_ref(&too_long).is_err());
    }

    #[test]
    fn rejects_leading_hyphen() {
        assert!(validate_work_ref("-foo").is_err());
    }

    #[test]
    fn rejects_control_and_nul() {
        assert!(validate_work_ref("a\0b").is_err());
        assert!(validate_work_ref("a\nb").is_err());
    }

    #[test]
    fn accepts_valid_slugs() {
        assert_eq!(validate_work_ref("my-novel").unwrap(), "my-novel");
        assert_eq!(validate_work_ref("a").unwrap(), "a");
        assert_eq!(validate_work_ref("0").unwrap(), "0");
        assert_eq!(validate_slug("ch01-foo").unwrap(), "ch01-foo");
        let max = "a".repeat(SLUG_MAX_LEN);
        assert_eq!(validate_work_ref(&max).unwrap(), max);
    }

    #[test]
    fn chapters_bounds_inclusive() {
        assert_eq!(validate_total_chapters(1).unwrap(), 1);
        assert_eq!(validate_total_chapters(100).unwrap(), 100);
        assert!(validate_total_chapters(0).is_err());
        assert!(validate_total_chapters(101).is_err());
        assert!(validate_total_chapters(-1).is_err());
    }
}
