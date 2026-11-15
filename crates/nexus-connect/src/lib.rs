//! Peer-tools Connect protocol stack, owned by `nexus-connect` (extracted from
//! `nexus-core`'s `connect` module behind the `connect-host` /
//! `connect-client` / `embedded-mcp` cohort features).
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
//! Capability dispatch lives beside this stack in [`peer_tools`] (the
//! process-level peer tool registry); this stack owns peer/MCP sessions,
//! visibility/authz, bounded transport, and watcher ownership — no second
//! registry.
//!
//! The crate depends on nothing in `nexus-core`: the error shape is local
//! ([`error`]), and the reserved builtin tool ids the admission chain needs
//! are injected by the composing side (`start_peer_tools_lane`) rather than
//! read from a core registry.
//!
//! `memory-remote` compiles this crate for [`ws_transport`] alone: the remote
//! memory client dials through the one product WS `Transport` without
//! enabling any accept/session/peer-control/rmcp cohort below
//! (n3-provider-binding.md §6).

pub mod error;

pub use error::{ConnectError, ConnectResult};

// The protocol-neutral peer tool registry (process-global, one per process).
#[cfg(feature = "peer-tools")]
pub mod peer_tools;

// The message-oriented WS transport is the one surface the remote memory
// client shares with the peer-tools stack. Everything below keeps its
// existing Connect cohort gate.
#[cfg(feature = "transport")]
pub mod ws_transport;

#[cfg(feature = "connect-host")]
pub mod accept;
#[cfg(feature = "connect-host")]
pub mod config;
#[cfg(feature = "connect-host")]
pub mod events;
#[cfg(feature = "connect-host")]
pub mod identity;
// The rmcp bridge is the reverse-invoke/operator surface; it compiles only
// with the rmcp-bearing `connect-client` cohort, never in the Connect-host
// product cohort.
#[cfg(feature = "connect-client")]
pub mod mcp_bridge;
#[cfg(feature = "embedded-mcp")]
pub mod mcp_embedded;
#[cfg(feature = "connect-host")]
pub mod peer_control;
#[cfg(feature = "connect-host")]
pub mod session;
#[cfg(feature = "connect-host")]
pub mod table;
#[cfg(feature = "connect-host")]
pub mod visibility;
#[cfg(feature = "connect-host")]
pub mod watch;

#[cfg(feature = "connect-host")]
pub use accept::{
    daemon_manifest, event_lane_readiness_line, spawn_accept_loop, start_peer_tools_lane,
    PeerResponderOptions, PeerToolsLaneHandle,
};
#[cfg(feature = "connect-host")]
pub use config::{load_peer_keys, CollisionPolicy, PeerToolsConfig, DEFAULT_CONNECT_PORT};
#[cfg(feature = "connect-host")]
pub use identity::load_or_create_identity;
#[cfg(feature = "connect-host")]
pub use identity::load_or_create_identity_at;
#[cfg(feature = "connect-host")]
pub use watch::{
    peer_config_digest, spawn_peer_config_watch, supervise_peer_config_watch, PeerConfigHolder,
    PeerConfigSnapshot,
};
// Shared MCP bridge core re-exports (rmcp-bearing `connect-client` cohort).
#[cfg(feature = "connect-client")]
pub use mcp_bridge::{
    is_unroutable, CatalogResponse, CatalogRow, McpBackend, McpBridgeHandler, ToolCallOutcome,
};
// Embedded MCP server shell re-exports (feature-gated).
#[cfg(feature = "embedded-mcp")]
pub use mcp_embedded::{
    start_embedded_mcp_server, EmbeddedMcpError, EmbeddedMcpServer, EmbeddedSession,
    EmbeddedShutdown, EMBEDDED_MCP_MAX_SESSIONS,
};
#[cfg(feature = "connect-host")]
pub use peer_control::PeerControlLane;
#[cfg(feature = "connect-host")]
pub use session::{PeerSessionManager, SessionRecord, DEFAULT_MAX_SESSIONS};
#[cfg(feature = "connect-host")]
pub use table::{
    mcp_catalog_admission, mcp_catalog_output_root_object, peer_tool_table, AdmissionOutcome,
    McpCatalogRefusal, PeerSessionTools, PeerToolEntry, PeerToolTable, ToolRefusal,
};
#[cfg(feature = "connect-host")]
pub use visibility::VisibilityPolicy;
#[cfg(feature = "transport")]
pub use ws_transport::{ws_config, WsTransport, DEFAULT_MAX_ENVELOPE_BYTES};
