//! One rule set for a slug that becomes a path segment.
//!
//! Workspace creation (`home`), knowledge entry addressing (`knowledge`) and
//! the CLI creator/workspace selection all accept a caller-supplied slug and
//! used to carry three drifting copies of the rule — reserved Windows device
//! names and trailing dots were enforced in only one of them. This module is
//! the single authority; callers only translate the reason into their own
//! error type.

/// Longest accepted slug, in characters (not bytes).
pub const MAX_SLUG_LEN: usize = 80;

/// Windows reserved device names (case-insensitive): a slug that would create
/// an unopenable directory on Windows is refused on every platform.
const RESERVED_NAMES: [&str; 22] = [
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// Validate a single-segment slug.
///
/// Covered: emptiness, length, path separators (`.`/`..`/`/`/`\`), control
/// characters, a trailing dot, trailing whitespace and Windows reserved device
/// names. Windows silently strips trailing dots and spaces and reserves the
/// device names, so a slug that passes here names the same directory on every
/// platform.
///
/// # Errors
///
/// Returns the refusal reason when `slug` is not a safe single path segment.
pub fn validate_slug_name(slug: &str) -> Result<(), &'static str> {
    if slug.is_empty() {
        return Err("must not be empty");
    }
    if slug.chars().count() > MAX_SLUG_LEN {
        return Err("must be at most 80 characters");
    }
    if slug.contains('/') || slug.contains('\\') || slug == "." || slug == ".." {
        return Err("must be a single path segment");
    }
    if slug.chars().any(char::is_control) {
        return Err("must not contain control characters");
    }
    if slug.ends_with('.') {
        return Err("must not end with '.'");
    }
    if slug.ends_with(char::is_whitespace) {
        return Err("must not end with whitespace");
    }
    // Windows reserves the STEM with any extension (`CON.txt`, `nul.backup`)
    // and ignores everything after the first dot when resolving the device, so
    // the comparison is on the part before it. A dot in the tail is otherwise
    // legal (`my.con`).
    let stem = slug.split_once('.').map_or(slug, |(stem, _extension)| stem);
    if RESERVED_NAMES
        .iter()
        .any(|reserved| stem.eq_ignore_ascii_case(reserved))
    {
        return Err("must not be a reserved device name");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_slug_name;

    #[test]
    fn accepts_ordinary_segments() {
        for slug in [
            "novel",
            "my-novel_2",
            "my.novel",
            "novel.v2",
            "my.con",
            "我的小说",
        ] {
            assert!(validate_slug_name(slug).is_ok(), "slug {slug:?}");
        }
    }

    #[test]
    fn refuses_every_unportable_segment() {
        let cases = [
            ("", "must not be empty"),
            ("a/b", "must be a single path segment"),
            ("a\\b", "must be a single path segment"),
            (".", "must be a single path segment"),
            ("..", "must be a single path segment"),
            ("a\nb", "must not contain control characters"),
            ("novel.", "must not end with '.'"),
            ("novel ", "must not end with whitespace"),
            // A tab is a control character first; a plain trailing space is the
            // whitespace rule Windows silently strips.
            ("novel\t", "must not contain control characters"),
            ("con", "must not be a reserved device name"),
            ("CON", "must not be a reserved device name"),
            ("lpt9", "must not be a reserved device name"),
            // Windows reserves the stem regardless of extension.
            ("CON.txt", "must not be a reserved device name"),
            ("nul.backup", "must not be a reserved device name"),
            ("Aux.v2.yaml", "must not be a reserved device name"),
        ];
        for (slug, reason) in cases {
            assert_eq!(validate_slug_name(slug), Err(reason), "slug {slug:?}");
        }
        let long = "a".repeat(super::MAX_SLUG_LEN + 1);
        assert_eq!(
            validate_slug_name(&long),
            Err("must be at most 80 characters")
        );
    }
}
