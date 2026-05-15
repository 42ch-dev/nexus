//! Provider adapters: ACP and native CLI.
//!
//! The ACP provider adapter wraps [`nexus_acp_host::AcpSdkAdapter`] behind the
//! [`ProviderAdapter`] trait. It manages ACP session lifecycle (initialize →
//! create_session → prompt/stream → cancel → shutdown) and translates SDK
//! events into normalized [`HostEvent`] items.
//!
//! The native CLI provider manages a subprocess for tools like Claude Code CLI.

pub mod acp;
pub mod native_cli;
