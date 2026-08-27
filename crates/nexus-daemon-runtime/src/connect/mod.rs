//! Peer-tools Connect client stack (AR-57..77, behind `connect-client`).
//!
//! - Task 1: WS message-oriented [`Transport`] implementation over
//!   tokio-tungstenite (AR-66).
//! - Task 2: daemon accept loop + `PeerSessionManager` + config snapshot
//!   (AR-67) — the daemon-side listening face for spoke dialers.
//!
//! Everything behind this module compiles only with the `connect-client`
//! feature (the default daemon graph stays libp2p-free and
//! tungstenite-free).

// V1.179 P0 T1 (DF-88): shared MCP bridge core — the rmcp `ServerHandler`
// surface generic over an `McpBackend` (Model A stdio child + Model B
// embedded). Compiles with `connect-client`; the embedded server module
// below compiles only under the nested `embedded-mcp` feature.
pub mod accept;
pub mod config;
pub mod identity;
pub mod mcp_bridge;
#[cfg(feature = "embedded-mcp")]
pub mod mcp_embedded;
pub mod session;
pub mod table;
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
// V1.179 P0 T1 (DF-88): shared MCP bridge core re-exports.
pub use mcp_bridge::{
    is_unroutable, CatalogResponse, CatalogRow, McpBackend, McpBridgeHandler, ToolCallOutcome,
};
// V1.179 P0 T1 (DF-88): embedded MCP server re-exports (feature-gated).
#[cfg(feature = "embedded-mcp")]
pub use mcp_embedded::{
    boot_embedded_mcp_server, start_embedded_mcp_server, EmbeddedMcpError, EmbeddedMcpServer,
    EmbeddedSession, EMBEDDED_MCP_MAX_SESSIONS,
};
pub use session::{PeerSessionManager, SessionRecord, DEFAULT_MAX_SESSIONS};
pub use table::{
    mcp_catalog_admission, mcp_catalog_output_root_object, peer_tool_table, AdmissionOutcome,
    McpCatalogRefusal, PeerSessionTools, PeerToolEntry, PeerToolTable, ToolRefusal,
};
pub use ws_transport::{ws_config, WsTransport, DEFAULT_MAX_ENVELOPE_BYTES};
