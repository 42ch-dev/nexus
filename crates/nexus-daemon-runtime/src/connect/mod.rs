//! Peer-tools Connect client stack (AR-57..61, behind `connect-client`).
//!
//! This module holds the WS message-oriented [`Transport`] implementation
//! (Task 1); later tasks add the accept loop + session manager (AR-58),
//! manifest ingestion (AR-59), outbound authz (AR-60) and the honesty
//! lockstep suite (AR-62) behind the same feature gate.

pub mod ws_transport;

pub use ws_transport::{ws_config, WsTransport, DEFAULT_MAX_ENVELOPE_BYTES};
