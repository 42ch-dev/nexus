//! Architecture dependency assertions for `nexus-daemon-runtime`.
//!
//! These tests enforce the intended dependency graph at **compile time** and
//! **runtime**:
//!
//! - **Required product deps** (compile-time): types from each required crate
//!   are referenced in a const assertion. If a dep is removed from `Cargo.toml`,
//!   the file won't compile.
//!
//! - **Forbidden deps** (runtime): the `Cargo.toml` is parsed at test time to
//!   confirm that `nexus-cloud-sync` and `nexus-cloud-domain` are absent.

// ── Compile-time assertions: required product dependencies ──────────

/// Const function that references types from each required product dependency.
/// If any dependency is removed from `Cargo.toml`, this module won't compile.
const _: () = {
    // nexus-creator-memory
    use nexus_creator_memory::LongTermMemory;
    const _LTM_SIZE: usize = std::mem::size_of::<LongTermMemory>();

    // nexus-narrative
    use nexus_narrative::narrative_context::WorldState;
    const _WS_SIZE: usize = std::mem::size_of::<WorldState>();

    // nexus-kb
    use nexus_kb::key_block::KeyBlock;
    const _KB_SIZE: usize = std::mem::size_of::<KeyBlock>();

    // nexus-knowledge
    use nexus_knowledge::KnowledgeEntry;
    const _KE_SIZE: usize = std::mem::size_of::<KnowledgeEntry>();

    // nexus-moment-context-assembly
    use nexus_moment_context_assembly::MomentRequest;
    const _MR_SIZE: usize = std::mem::size_of::<MomentRequest>();

    // Suppress unused warnings
    let _ = (_LTM_SIZE, _WS_SIZE, _KB_SIZE, _KE_SIZE, _MR_SIZE);
};

// ── Runtime assertions: forbidden dependencies ──────────────────────

/// Forbidden dependency names that `nexus-daemon-runtime` must NOT depend on.
const FORBIDDEN_DEPS: &[&str] = &["nexus-cloud-sync", "nexus-cloud-domain"];

/// Required product dependency names that must be present.
const REQUIRED_DEPS: &[&str] = &[
    "nexus-narrative",
    "nexus-kb",
    "nexus-knowledge",
    "nexus-creator-memory",
    "nexus-moment-context-assembly",
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Daemon runtime must not depend on cloud crates.
    #[test]
    fn forbidden_cloud_dependencies_are_absent() {
        let manifest = include_str!("../Cargo.toml");
        for forbidden in FORBIDDEN_DEPS {
            assert!(
                !manifest.contains(forbidden),
                "nexus-daemon-runtime must NOT depend on `{forbidden}` — \
                 cloud isolation boundary violated. Check Cargo.toml.",
            );
        }
    }

    /// All required product dependencies must appear in Cargo.toml.
    #[test]
    fn required_product_dependencies_are_present() {
        let manifest = include_str!("../Cargo.toml");
        for required in REQUIRED_DEPS {
            assert!(
                manifest.contains(required),
                "nexus-daemon-runtime is missing required dependency `{required}`. \
                 Add it to Cargo.toml [dependencies].",
            );
        }
    }
}
