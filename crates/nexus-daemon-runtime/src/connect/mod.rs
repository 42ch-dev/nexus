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

pub mod accept;
pub mod config;
pub mod identity;
pub mod session;
pub mod table;
pub mod ws_transport;

pub use accept::{
    PeerResponderOptions, PeerToolsLaneHandle, daemon_manifest, spawn_accept_loop,
    start_peer_tools_lane,
};
pub use config::{PeerToolsConfig, load_peer_keys, DEFAULT_CONNECT_PORT};
pub use identity::load_or_create_identity;
pub use session::{PeerSessionManager, SessionRecord, DEFAULT_MAX_SESSIONS};
pub use table::{peer_tool_table, AdmissionOutcome, PeerToolEntry, PeerToolTable, PeerSessionTools, ToolRefusal};
pub use ws_transport::{ws_config, WsTransport, DEFAULT_MAX_ENVELOPE_BYTES};
