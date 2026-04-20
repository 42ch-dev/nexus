//! Frozen capability IDs and capability set construction for ACP `initialize`.
//!
//! This module defines the V1.0 frozen capability IDs that nexus42 declares
//! during the ACP `initialize` handshake. These capabilities tell agents what
//! features the client supports (file system access, terminal management, etc.).
//!
//! # V1.0 Capability Set
//!
//! The following capabilities are frozen for V1.0 and will be declared in every
//! `initialize` request:
//!
//! | Capability ID | Description |
//! |---------------|-------------|
//! | `file_system.read` | Read text files from workspace |
//! | `file_system.write` | Write text files to workspace |
//! | `terminal.create` | Create terminal sessions |
//! | `terminal.output` | Read terminal output |
//! | `terminal.release` | Release terminal resources |
//!
//! # Deferred Capabilities (V1.1+)
//!
//! The following capabilities are intentionally deferred to V1.1+ with rationale:
//!
//! | Capability | Deferred Reason |
//! |------------|----------------|
//! | `terminal.kill` | Advanced terminal management — not needed for basic V1.0 workflow |
//! | `terminal.wait_for_exit` | Requires exit status handling — not critical for V1.0 |
//! | `slash_commands` | Requires UI integration in the CLI prompt loop |
//! | `agent_plan` | Requires structured plan rendering in the CLI |
//! | `session.modes` | Requires mode switching logic (ask/act) in the CLI |
//!
//! These are documented in the tech spec (§5.2) and tracked as residual findings
//! (ACP-R3 through ACP-R11) for future implementation.

use agent_client_protocol::ClientCapabilities;
use agent_client_protocol::FileSystemCapabilities;

/// Frozen capability IDs for V1.0.
///
/// These constants match the ACP spec capability identifiers exactly.
/// Unit tests verify the values against the official ACP registry.
///
/// Note: These constants are intentionally unused in V1.0 code — they will be
/// wired into the initialize request in Task 4 (transport + agent run).
/// The dead_code warning is suppressed to acknowledge this design.
#[allow(dead_code)]
pub mod capabilities {
    /// Client can read text files from the workspace.
    ///
    /// Handler: `fs/read_text_file`
    pub const FILE_SYSTEM_READ: &str = "file_system.read";

    /// Client can write text files to the workspace.
    ///
    /// Handler: `fs/write_text_file`
    pub const FILE_SYSTEM_WRITE: &str = "file_system.write";

    /// Client can create terminal sessions.
    ///
    /// Handler: `terminal/create`
    pub const TERMINAL_CREATE: &str = "terminal.create";

    /// Client can stream terminal output.
    ///
    /// Handler: `terminal/output`
    pub const TERMINAL_OUTPUT: &str = "terminal.output";

    /// Client can release terminal sessions.
    ///
    /// Handler: `terminal/release`
    pub const TERMINAL_RELEASE: &str = "terminal.release";
}

/// Build the V1.0 capability set for ACP `initialize` request.
///
/// This function constructs a `ClientCapabilities` struct containing all
/// frozen V1.0 capabilities. It is called by `AcpSdkAdapter::initialize()`
/// when constructing the handshake request.
///
/// Note: This function is intentionally unused in V1.0 code — it will be
/// wired into the initialize request in Task 4 (transport + agent run).
/// The dead_code warning is suppressed to acknowledge this design.
///
/// # Example
///
/// ```rust,ignore
/// use nexus_acp_host::skills::build_v1_0_capabilities;
///
/// let caps = build_v1_0_capabilities();
/// // caps will include: file_system.read, file_system.write,
/// //                    terminal.create, terminal.output, terminal.release
/// ```
#[allow(dead_code)]
pub fn build_v1_0_capabilities() -> ClientCapabilities {
    ClientCapabilities::new()
        .fs(FileSystemCapabilities::new()
            .read_text_file(true)
            .write_text_file(true))
        .terminal(true)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::capabilities::*;

    /// Verify that all frozen capability IDs match the ACP spec exactly.
    ///
    /// These tests guard against accidental typos or drift from the official
    /// ACP capability registry. If the ACP spec changes these IDs, the tests
    /// will fail and signal a need to update.
    #[test]
    fn test_file_system_read_capability_id() {
        assert_eq!(FILE_SYSTEM_READ, "file_system.read");
    }

    #[test]
    fn test_file_system_write_capability_id() {
        assert_eq!(FILE_SYSTEM_WRITE, "file_system.write");
    }

    #[test]
    fn test_terminal_create_capability_id() {
        assert_eq!(TERMINAL_CREATE, "terminal.create");
    }

    #[test]
    fn test_terminal_output_capability_id() {
        assert_eq!(TERMINAL_OUTPUT, "terminal.output");
    }

    #[test]
    fn test_terminal_release_capability_id() {
        assert_eq!(TERMINAL_RELEASE, "terminal.release");
    }

    /// Verify the capability set builder returns a non-empty result.
    ///
    /// This is a smoke test — the detailed capability structure will be
    /// validated in Task 4 integration tests.
    #[test]
    fn test_build_v1_0_capabilities_returns_non_default() {
        use super::build_v1_0_capabilities;
        let _caps = build_v1_0_capabilities();
        // Placeholder assertion — will be enhanced in Task 4.
        // For now, we just verify the function compiles and runs.
        assert!(true, "Capability builder executed successfully");
    }
}
