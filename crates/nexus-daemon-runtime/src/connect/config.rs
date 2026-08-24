//! Peer-tools accept-loop operator config (V1.174 P0, AR-67 §3.4).
//!
//! `~/.nexus42/connect/daemon.json` — the daemon-side Connect lane's own
//! file. It is DISTINCT from `connect/config.json` (the connect-host
//! capability-token policy file owned by the `nexus42 connect` CLI) and
//! from `connect/allowlist.json` (the connect-host peer/scope file): the
//! peer-tools lane never touches token policy and never reuses the
//! N-C1/N-C2 world/op scoping vocabulary.
//!
//! Semantics:
//! - Missing file ⇒ the documented defaults (loopback + port 8425 + the
//!   max-session / invoke-timeout / envelope-cap defaults below).
//! - Existing but malformed / unknown fields / validation failure ⇒ hard
//!   boot error (fail-closed, same guard as `allowlist.json` /
//!   `config.json` in `apps/nexus42`).
//! - Read **once** at subsystem boot; changes apply on daemon restart
//!   (runtime reload = Non-Goal, roadmap row in the AR-67 spec).

use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::connect::session::DEFAULT_MAX_SESSIONS;
use crate::connect::ws_transport::DEFAULT_MAX_ENVELOPE_BYTES;

/// Default Connect listen port (the daemon API default is 8420; the
/// peer-tools lane is a separate listener, so it gets its own port).
pub const DEFAULT_CONNECT_PORT: u16 = 8425;

/// Default Connect listen host — loopback only (non-loopback reuses the
/// daemon remote-bind gate posture; see [`crate::boot`]).
pub const DEFAULT_CONNECT_HOST: &str = "127.0.0.1";

/// Default per-invoke bounded wait (ms) — parity with
/// `DEFAULT_INVOKE_TIMEOUT_MS` in spoke-connect.
pub const DEFAULT_INVOKE_TIMEOUT_MS: u64 = 5000;

/// On-disk accept-loop config (`daemon.json`).
///
/// `deny_unknown_fields` turns typos into a hard boot error instead of
/// silently applying defaults that change the trust posture (same guard as
/// the connect-host config files).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields, default)]
pub struct PeerToolsConfig {
    /// Listen host. Non-loopback reuses the daemon remote-bind gate
    /// (`NEXUS42_DAEMON_API_KEY` + `NEXUS_DAEMON_REMOTE_BIND=1`); loopback is
    /// always allowed.
    pub host: String,
    /// Listen port.
    pub port: u16,
    /// Maximum concurrent peer sessions; excess connections are refused at
    /// accept with a logged refusal.
    pub max_sessions: usize,
    /// Bounded-wait deadline for each responder reverse-invoke waiter (ms).
    pub invoke_timeout_ms: u64,
    /// Maximum inbound WS envelope size (bytes).
    pub max_envelope_bytes: usize,
    /// Operator tool allowlist (AR-68 #2(iii)): exact tool ids a dialer
    /// manifest may admit. Missing/empty = default deny (zero admitted).
    /// Serde default keeps existing `daemon.json` files valid.
    #[serde(default)]
    pub tool_allowlist: Vec<String>,
}

impl Default for PeerToolsConfig {
    fn default() -> Self {
        Self {
            host: DEFAULT_CONNECT_HOST.to_owned(),
            port: DEFAULT_CONNECT_PORT,
            max_sessions: DEFAULT_MAX_SESSIONS,
            invoke_timeout_ms: DEFAULT_INVOKE_TIMEOUT_MS,
            max_envelope_bytes: DEFAULT_MAX_ENVELOPE_BYTES,
            tool_allowlist: Vec::new(),
        }
    }
}

impl PeerToolsConfig {
    /// Read the effective accept-loop config from
    /// `~/.nexus42/connect/daemon.json`.
    ///
    /// `home` is the RAW user home (`$HOME`); the path helper joins
    /// `.nexus42` internally. A missing file yields [`Self::default`].
    ///
    /// # Errors
    /// Returns an I/O error when the file exists but cannot be read, or a
    /// serde error when it is malformed or has unknown fields (fail-closed).
    pub fn load(home: &Path) -> Result<Self, ConnectConfigError> {
        let path = nexus_home_layout::connect_daemon_config_path(home);
        match std::fs::read_to_string(&path) {
            Ok(raw) => {
                let parsed: Self = serde_json::from_str(&raw).map_err(|e| {
                    ConnectConfigError::Malformed(format!(
                        "invalid {}: {e}",
                        path.display()
                    ))
                })?;
                if parsed.max_sessions == 0 {
                    return Err(ConnectConfigError::Invalid(
                        "max_sessions must be >= 1 (0 would refuse every peer)".to_owned(),
                    ));
                }
                Ok(parsed)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(ConnectConfigError::Io(format!(
                "cannot read {}: {e}",
                path.display()
            ))),
        }
    }
}

/// Config load failure (fail-closed on any existing-but-invalid file).
#[derive(Debug, thiserror::Error)]
pub enum ConnectConfigError {
    /// The file exists but cannot be read.
    #[error("{0}")]
    Io(String),
    /// The file is malformed / has unknown fields.
    #[error("{0}")]
    Malformed(String),
    /// Valid JSON but semantically invalid.
    #[error("{0}")]
    Invalid(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn isolated_home() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    #[test]
    fn default_when_file_absent() {
        let home = isolated_home();
        let config = PeerToolsConfig::load(home.path()).expect("default load");
        assert_eq!(config.host, DEFAULT_CONNECT_HOST);
        assert_eq!(config.port, DEFAULT_CONNECT_PORT);
        assert_eq!(config.max_sessions, DEFAULT_MAX_SESSIONS);
        assert_eq!(config.invoke_timeout_ms, DEFAULT_INVOKE_TIMEOUT_MS);
        assert_eq!(config.max_envelope_bytes, DEFAULT_MAX_ENVELOPE_BYTES);
    }

    #[test]
    fn parses_explicit_file() {
        let home = isolated_home();
        let dir = nexus_home_layout::connect_dir(home.path());
        fs::create_dir_all(&dir).expect("mkdir");
        fs::write(
            nexus_home_layout::connect_daemon_config_path(home.path()),
            r#"{"host":"127.0.0.1","port":9999,"max_sessions":3,"invoke_timeout_ms":250,"max_envelope_bytes":4096}"#,
        )
        .expect("write");
        let config = PeerToolsConfig::load(home.path()).expect("load");
        assert_eq!(config.port, 9999);
        assert_eq!(config.max_sessions, 3);
        assert_eq!(config.invoke_timeout_ms, 250);
        assert_eq!(config.max_envelope_bytes, 4096);
    }

    #[test]
    fn unknown_fields_are_hard_errors() {
        let home = isolated_home();
        fs::create_dir_all(nexus_home_layout::connect_dir(home.path())).expect("mkdir");
        fs::write(
            nexus_home_layout::connect_daemon_config_path(home.path()),
            r#"{"max_sessions":2,"port":1,"unknown_field":true}"#,
        )
        .expect("write");
        assert!(matches!(
            PeerToolsConfig::load(home.path()),
            Err(ConnectConfigError::Malformed(_))
        ));
    }

    #[test]
    fn zero_max_sessions_rejected() {
        let home = isolated_home();
        fs::create_dir_all(nexus_home_layout::connect_dir(home.path())).expect("mkdir");
        fs::write(
            nexus_home_layout::connect_daemon_config_path(home.path()),
            r#"{"max_sessions":0,"port":1}"#,
        )
        .expect("write");
        assert!(matches!(
            PeerToolsConfig::load(home.path()),
            Err(ConnectConfigError::Invalid(_))
        ));
    }
}
