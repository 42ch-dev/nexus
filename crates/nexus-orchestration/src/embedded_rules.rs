//! Embedded rules module.
//!
//! Provides compile-time embedded access to rules documents that ship with the
//! nexus binary.  Rules are markdown-based guidance consumed by orchestration
//! stages (e.g., novel-writing craft rules).  Unlike presets, rules are not
//! state machines — they are pure content layers read by preset templates.
//!
//! **Layer 1 is a compile-time constant with no runtime filesystem dependency.**
//! All rule content is embedded via `include_str!` at compile time, making it
//! always available without requiring any files on disk at runtime.
//!
//! Layout on disk (relative to `crates/nexus-orchestration/`):
//!
//! ```text
//! embedded-rules/
//! └── writing-craft.md
//! ```
//!
//! Each file is embedded via `include_str!` — no runtime filesystem reads
//! are performed.

/// Layer 1 writing-craft rules, compiled into the binary at build time.
///
/// Location: `crates/nexus-orchestration/embedded-rules/writing-craft.md`
pub const WRITING_CRAFT: &str = include_str!("../embedded-rules/writing-craft.md");

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writing_craft_is_not_empty() {
        assert!(
            !WRITING_CRAFT.is_empty(),
            "embedded writing-craft.md must not be empty"
        );
    }

    #[test]
    fn writing_craft_contains_expected_heading() {
        assert!(
            WRITING_CRAFT.contains("Writing Craft Rules"),
            "writing-craft.md should contain its title heading"
        );
    }

    #[test]
    fn writing_craft_contains_five_question_gate() {
        assert!(
            WRITING_CRAFT.contains("Five-Question Gate"),
            "writing-craft.md should contain the Five-Question Gate section"
        );
    }
}
