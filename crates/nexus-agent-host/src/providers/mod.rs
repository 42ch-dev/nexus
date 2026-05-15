//! Provider adapters: ACP and native CLI.
//!
//! The ACP provider adapter wraps [`nexus_acp_host::AcpSdkAdapter`] behind the
//! [`ProviderAdapter`] trait. It manages ACP session lifecycle (initialize →
//! create_session → prompt/stream → cancel → shutdown) and translates SDK
//! events into normalized [`HostEvent`] items.
//!
//! Native CLI providers are planned for a future wave.

pub mod acp;
