//! Peer-tools Connect client stack (AR-57..77), owned by `nexus-core`
//! (P4-T3 moved from the daemon transport) behind the `connect-client` /
//! `embedded-mcp` cohort features.
//!
//! - WS message-oriented [`ws_transport::Transport`] implementation over
//!   tokio-tungstenite (AR-66).
//! - Accept loop + [`session::PeerSessionManager`] + config snapshot
//!   (`accept.rs`, AR-67) — the listening face for spoke dialers.
//! - Per-consumer MCP tool visibility (`visibility::VisibilityPolicy`,
//!   V1.180 P1, RN-OGA-2) and the shared rmcp bridge core
//!   (`mcp_bridge`, DF-88); the embedded Model B server shell
//!   (`mcp_embedded`) compiles only under the nested `embedded-mcp`
//!   feature.
//!
//! Capability dispatch stays in P3 (`crate::execution::peer_tools` owns the
//! process-level peer tool registry); this stack owns peer/MCP sessions,
//! visibility/authz, bounded transport, and watcher ownership — no second
//! registry. Child stdio CLI belongs to P6-T2, and the Connect-host product
//! (P6-T3) must not enable this reverse-invoke/operator cohort.

pub mod accept;
pub mod config;
pub mod identity;
pub mod mcp_bridge;
#[cfg(feature = "embedded-mcp")]
pub mod mcp_embedded;
pub mod peer_control;
pub mod session;
pub mod table;
pub mod visibility;
pub mod watch;
pub mod ws_transport;

pub use accept::{
    daemon_manifest, spawn_accept_loop, start_peer_tools_lane, PeerResponderOptions,
    PeerToolsLaneHandle,
};
pub use config::{load_peer_keys, CollisionPolicy, PeerToolsConfig, DEFAULT_CONNECT_PORT};
pub use identity::load_or_create_identity;
pub use watch::{
    peer_config_digest, spawn_peer_config_watch, supervise_peer_config_watch, PeerConfigHolder,
    PeerConfigSnapshot,
};
// Shared MCP bridge core re-exports.
pub use mcp_bridge::{
    is_unroutable, CatalogResponse, CatalogRow, McpBackend, McpBridgeHandler, ToolCallOutcome,
};
// Embedded MCP server shell re-exports (feature-gated).
#[cfg(feature = "embedded-mcp")]
pub use mcp_embedded::{
    start_embedded_mcp_server, EmbeddedMcpError, EmbeddedMcpServer, EmbeddedSession, EmbeddedShutdown,
    EMBEDDED_MCP_MAX_SESSIONS,
};
pub use session::{PeerSessionManager, SessionRecord, DEFAULT_MAX_SESSIONS};
pub use table::{
    mcp_catalog_admission, mcp_catalog_output_root_object, peer_tool_table, AdmissionOutcome,
    McpCatalogRefusal, PeerSessionTools, PeerToolEntry, PeerToolTable, ToolRefusal,
};
pub use visibility::VisibilityPolicy;
pub use peer_control::PeerControlLane;
pub use ws_transport::{ws_config, WsTransport, DEFAULT_MAX_ENVELOPE_BYTES};
