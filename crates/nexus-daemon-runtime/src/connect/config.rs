//! Peer-tools accept-loop operator config (V1.174 P0, AR-67 §3.4 + AR-69).
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
//! - AR-69 outbound authz: `tool_allowlist` entries are validated at load
//!   (umbrella / reserved-ns / malformed ⇒ named `InvalidAllowlist` error,
//!   never silently dropped); `peer_ids` is the dialer handshake allowlist;
//!   `peer_keys.json` supplies the preconfigured dialer Ed25519 keys.
//!   Allowlist edits (tool ids, peer ids, keys) apply on daemon restart —
//!   never mid-session (restart-scoped snapshot, AR-69).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

use crate::connect::session::DEFAULT_MAX_SESSIONS;
use crate::connect::ws_transport::DEFAULT_MAX_ENVELOPE_BYTES;
use spoke_operations::{parse_tool_capability_id, SpokeResult};

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
    /// Validated at load (AR-69 derivation lock): umbrella / malformed /
    /// reserved-ns entries fail config load with a named error — never
    /// silently dropped. Serde default keeps existing `daemon.json` files
    /// valid.
    #[serde(default)]
    pub tool_allowlist: Vec<String>,
    /// Dialer peer ids allowed at the handshake (AR-69 Layer 0). Missing/
    /// empty = fail-closed (every dial rejected). A dialer must be in this
    /// list AND have a preconfigured key in `peer_keys.json`.
    #[serde(default)]
    pub peer_ids: Vec<String>,
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
            peer_ids: Vec::new(),
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
    /// Returns an I/O error when the file exists but cannot be read, a
    /// serde error when it is malformed or has unknown fields, or a named
    /// `InvalidAllowlist` error when an operator allowlist entry fails the
    /// AR-69 derivation-lock validation (fail-closed — never silently
    /// dropped).
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
                for entry in &parsed.tool_allowlist {
                    validate_allowlist_entry(entry)?;
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

/// Validate one operator allowlist entry (AR-69 derivation lock).
///
/// Order: umbrella → reserved-ns → grammar. Every rejection is a named
/// `InvalidAllowlist` error; an invalid entry fails the whole config load
/// (never silently dropped).
fn validate_allowlist_entry(entry: &str) -> Result<(), ConnectConfigError> {
    // Umbrella: `tools`, `tools.*`, `tools.<ns>` (namespace-level), or any
    // wildcard — an allowlist entry must name ONE exact tool id.
    if entry.contains('*') || entry.split('.').count() < 3 {
        return Err(ConnectConfigError::InvalidAllowlist {
            entry: entry.to_owned(),
            reason: "umbrella entry — allowlist must name exact tool ids (tools.<ns>.<id>)"
                .to_owned(),
        });
    }
    // Reserved namespace: `tools.nexus.*` can never be operator-allowlisted
    // (the nexus namespace is daemon-owned, AR-68 #2(iii)).
    if entry.starts_with("tools.nexus.") {
        return Err(ConnectConfigError::InvalidAllowlist {
            entry: entry.to_owned(),
            reason: "reserved namespace (tools.nexus.* is daemon-owned)".to_owned(),
        });
    }
    // Grammar: `^tools\.[a-z][a-z0-9_-]*\.[a-z0-9][a-z0-9_-]*$`.
    if !matches!(parse_tool_capability_id(entry), SpokeResult::Ok(_)) {
        return Err(ConnectConfigError::InvalidAllowlist {
            entry: entry.to_owned(),
            reason:
                "malformed tool id (expected tools.<ns>.<id> with ns ^[a-z][a-z0-9_-]*$ and id ^[a-z0-9][a-z0-9_-]*$)"
                    .to_owned(),
        });
    }
    Ok(())
}

/// Read the preconfigured dialer Ed25519 public keys from
/// `~/.nexus42/connect/peer_keys.json` (AR-69 Layer 0).
///
/// Format: `{ "peer_keys": { "<peer-id>": "<64-hex-chars>" } }`. A missing
/// file yields an empty map (fail-closed — no dialer passes the responder
/// handshake without a preconfigured key). An existing-but-invalid file
/// (malformed JSON, unknown fields, non-hex / wrong-length key) is a hard
/// error — never silently dropped. Read once at boot; key edits apply on
/// daemon restart (AR-69 restart-scoped snapshot).
///
/// # Errors
/// Returns an I/O error when the file exists but cannot be read, or a
/// named `InvalidPeerKeys` error when it is malformed / has unknown fields
/// / a key is not 64 hex chars (fail-closed — never silently dropped).
pub fn load_peer_keys(home: &Path) -> Result<HashMap<String, [u8; 32]>, ConnectConfigError> {
    let path = nexus_home_layout::connect_peer_keys_path(home);
    match std::fs::read_to_string(&path) {
        Ok(raw) => {
            let parsed: PeerKeysFile = serde_json::from_str(&raw).map_err(|e| {
                ConnectConfigError::InvalidPeerKeys(format!(
                    "invalid {}: {e}",
                    path.display()
                ))
            })?;
            let mut keys = HashMap::with_capacity(parsed.peer_keys.len());
            for (peer_id, hex_key) in parsed.peer_keys {
                let key = decode_hex_32(&hex_key).map_err(|reason| {
                    ConnectConfigError::InvalidPeerKeys(format!("peer {peer_id:?}: {reason}"))
                })?;
                keys.insert(peer_id, key);
            }
            Ok(keys)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(HashMap::new()),
        Err(e) => Err(ConnectConfigError::Io(format!(
            "cannot read {}: {e}",
            path.display()
        ))),
    }
}

/// On-disk `peer_keys.json` shape (`deny_unknown_fields` — typos are hard
/// errors, same guard as `daemon.json`).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PeerKeysFile {
    /// `peer_id → 64-hex-char Ed25519 public key`. Default keeps an
    /// explicit `{}` valid (an operator declaring no keys).
    #[serde(default)]
    peer_keys: HashMap<String, String>,
}

/// Decode a 64-char hex string into 32 bytes (lowercase or uppercase).
fn decode_hex_32(s: &str) -> Result<[u8; 32], String> {
    if s.len() != 64 {
        return Err(format!("expected 64 hex chars, found {}", s.len()));
    }
    let mut out = [0u8; 32];
    for (i, byte) in out.iter_mut().enumerate() {
        let hi = hex_val(s.as_bytes()[i * 2])?;
        let lo = hex_val(s.as_bytes()[i * 2 + 1])?;
        *byte = (hi << 4) | lo;
    }
    Ok(out)
}

/// One hex nibble.
fn hex_val(b: u8) -> Result<u8, String> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        b'A'..=b'F' => Ok(b - b'A' + 10),
        _ => Err(format!("invalid hex char {b:#04x}")),
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
    /// An operator allowlist entry failed the AR-69 derivation-lock
    /// validation (umbrella / reserved-ns / malformed). The whole config
    /// load fails — invalid entries are never silently dropped.
    #[error("invalid tool_allowlist entry {entry:?}: {reason}")]
    InvalidAllowlist { entry: String, reason: String },
    /// `peer_keys.json` exists but is malformed / has unknown fields / a
    /// key is not 64 hex chars.
    #[error("invalid peer_keys.json: {0}")]
    InvalidPeerKeys(String),
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
        assert!(config.tool_allowlist.is_empty());
        assert!(config.peer_ids.is_empty());
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

    // ── AR-69 derivation lock: allowlist negatives ────────────────────────

    #[test]
    fn authz_hello_allowlist_umbrella_rejected() {
        for entry in ["tools", "tools.*", "tools.t4", "tools.t4.*"] {
            let home = isolated_home();
            fs::create_dir_all(nexus_home_layout::connect_dir(home.path())).expect("mkdir");
            fs::write(
                nexus_home_layout::connect_daemon_config_path(home.path()),
                format!(r#"{{"tool_allowlist":["{entry}"]}}"#),
            )
            .expect("write");
            match PeerToolsConfig::load(home.path()) {
                Err(ConnectConfigError::InvalidAllowlist { entry: e, reason }) => {
                    assert_eq!(e, entry, "named entry preserved");
                    assert!(
                        reason.contains("umbrella"),
                        "umbrella reason for {entry:?}: {reason}"
                    );
                }
                other => panic!(
                    "umbrella {entry:?} must fail load with InvalidAllowlist, got {other:?}"
                ),
            }
        }
    }

    #[test]
    fn authz_hello_allowlist_malformed_rejected() {
        for entry in ["tools.1abc.x", "tools.t4.", "tools..x", "tools.t4.x y"] {
            let home = isolated_home();
            fs::create_dir_all(nexus_home_layout::connect_dir(home.path())).expect("mkdir");
            fs::write(
                nexus_home_layout::connect_daemon_config_path(home.path()),
                format!(r#"{{"tool_allowlist":["{entry}"]}}"#),
            )
            .expect("write");
            match PeerToolsConfig::load(home.path()) {
                Err(ConnectConfigError::InvalidAllowlist { entry: e, reason }) => {
                    assert_eq!(e, entry, "named entry preserved");
                    assert!(
                        reason.contains("malformed"),
                        "malformed reason for {entry:?}: {reason}"
                    );
                }
                other => panic!(
                    "malformed {entry:?} must fail load with InvalidAllowlist, got {other:?}"
                ),
            }
        }
    }

    #[test]
    fn authz_hello_allowlist_reserved_ns_rejected() {
        let home = isolated_home();
        fs::create_dir_all(nexus_home_layout::connect_dir(home.path())).expect("mkdir");
        fs::write(
            nexus_home_layout::connect_daemon_config_path(home.path()),
            r#"{"tool_allowlist":["tools.nexus.evil"]}"#,
        )
        .expect("write");
        match PeerToolsConfig::load(home.path()) {
            Err(ConnectConfigError::InvalidAllowlist { entry, reason }) => {
                assert_eq!(entry, "tools.nexus.evil");
                assert!(reason.contains("reserved"), "reserved reason: {reason}");
            }
            other => panic!("reserved-ns must fail load with InvalidAllowlist, got {other:?}"),
        }
    }

    #[test]
    fn authz_hello_allowlist_valid_entries_load() {
        let home = isolated_home();
        fs::create_dir_all(nexus_home_layout::connect_dir(home.path())).expect("mkdir");
        fs::write(
            nexus_home_layout::connect_daemon_config_path(home.path()),
            r#"{"tool_allowlist":["tools.t4.echo","tools.acme.ping"],"peer_ids":["peer-1"]}"#,
        )
        .expect("write");
        let config = PeerToolsConfig::load(home.path()).expect("valid allowlist loads");
        assert_eq!(
            config.tool_allowlist,
            vec!["tools.t4.echo".to_owned(), "tools.acme.ping".to_owned()]
        );
        assert_eq!(config.peer_ids, vec!["peer-1".to_owned()]);
    }

    // ── AR-69 Layer 0: peer_keys.json reader ─────────────────────────────

    #[test]
    fn authz_hello_peer_keys_missing_file_is_empty() {
        let home = isolated_home();
        let keys = load_peer_keys(home.path()).expect("missing file ⇒ empty map");
        assert!(keys.is_empty(), "fail-closed: no keys ⇒ no dialer passes");
    }

    #[test]
    fn authz_hello_peer_keys_parses_valid_file() {
        let home = isolated_home();
        fs::create_dir_all(nexus_home_layout::connect_dir(home.path())).expect("mkdir");
        let key = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        fs::write(
            nexus_home_layout::connect_peer_keys_path(home.path()),
            format!(r#"{{"peer_keys":{{"peer-1":"{key}"}}}}"#),
        )
        .expect("write");
        let keys = load_peer_keys(home.path()).expect("load");
        assert_eq!(keys.len(), 1);
        let decoded = keys.get("peer-1").expect("key present");
        assert_eq!(decoded[0], 0x01, "first byte decoded");
        assert_eq!(decoded[31], 0xef, "last byte decoded");
    }

    #[test]
    fn authz_hello_peer_keys_bad_hex_rejected() {
        let home = isolated_home();
        fs::create_dir_all(nexus_home_layout::connect_dir(home.path())).expect("mkdir");
        fs::write(
            nexus_home_layout::connect_peer_keys_path(home.path()),
            r#"{"peer_keys":{"peer-1":"zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz"}}"#,
        )
        .expect("write");
        assert!(matches!(
            load_peer_keys(home.path()),
            Err(ConnectConfigError::InvalidPeerKeys(_))
        ));
    }

    #[test]
    fn authz_hello_peer_keys_wrong_length_rejected() {
        let home = isolated_home();
        fs::create_dir_all(nexus_home_layout::connect_dir(home.path())).expect("mkdir");
        fs::write(
            nexus_home_layout::connect_peer_keys_path(home.path()),
            r#"{"peer_keys":{"peer-1":"abcd"}}"#,
        )
        .expect("write");
        assert!(matches!(
            load_peer_keys(home.path()),
            Err(ConnectConfigError::InvalidPeerKeys(_))
        ));
    }

    #[test]
    fn authz_hello_peer_keys_unknown_field_rejected() {
        let home = isolated_home();
        fs::create_dir_all(nexus_home_layout::connect_dir(home.path())).expect("mkdir");
        fs::write(
            nexus_home_layout::connect_peer_keys_path(home.path()),
            r#"{"peer_keys":{},"bogus":true}"#,
        )
        .expect("write");
        assert!(matches!(
            load_peer_keys(home.path()),
            Err(ConnectConfigError::InvalidPeerKeys(_))
        ));
    }
}
