//! Connect Host operator config (`~/.nexus42/connect/config.json`) — V1.155
//! P1 capability-token production surface (iteration spec Design #2).
//!
//! `allowlist.json` stays the peer/scope file (architect lock #1); this file
//! carries the token policy: trusted issuers, whether sessions must complete
//! the capability-token challenge, and the outbound proof provider.

use crate::errors::{CliError, Result};
use nexus_home_layout::connect_config_path;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// On-disk capability-token operator config (`config.json`).
///
/// Missing fields default to the production defaults (empty / false / None —
/// the pre-V1.155 behavior, unchanged when the file is absent). The exact
/// schema (architect lock #1 + iteration spec §Contracts):
/// `{ trusted_issuers: string[], require_capability_token: bool,
/// capability_token_provider: { enabled: bool, issuer_key_path?: string } }`.
///
/// `deny_unknown_fields` turns a typo (e.g. `trustedIssuers`,
/// `require_token`) into a hard boot error instead of silently applying
/// defaults that weaken or misconfigure the token policy (same guard as
/// `allowlist.json`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectTokenConfig {
    /// Peer ids whose signed capability tokens this node accepts (parallel
    /// to `peer_allowlist`). **Empty ⇒ the capability-token method is
    /// disabled** (challenges are not offered; any presented proof is
    /// rejected — fail closed).
    #[serde(default)]
    pub trusted_issuers: Vec<String>,

    /// Whether every session must complete the capability-token challenge
    /// before invokes are accepted. Effective only with a non-empty
    /// `trusted_issuers`; default `false` keeps the `noise-peerid`-only
    /// behavior.
    #[serde(default)]
    pub require_capability_token: bool,

    /// Outbound proof surface: when `enabled`, this node mints tokens from
    /// its issuer key to answer challenges when it **dials** peers that
    /// require tokens (the inbound side is governed by `trusted_issuers` +
    /// `require_capability_token`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability_token_provider: Option<ProviderConfig>,
}

/// The `capability_token_provider` block.
///
/// `issuer_key_path` overrides the default `~/.nexus42/connect/issuer.key`:
/// absolute paths are used as-is; relative paths resolve against
/// `~/.nexus42/connect/` (the connect dir the config lives in).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderConfig {
    /// Whether this node presents capability-token proofs on dial.
    pub enabled: bool,
    /// Issuer key path override (optional — defaults to
    /// `~/.nexus42/connect/issuer.key`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issuer_key_path: Option<String>,
}

impl ConnectTokenConfig {
    /// Fail-closed validation (architect lock #3): requiring tokens while
    /// trusting no issuer would challenge every session but accept no proof
    /// — the host would lock itself out. Refuse to boot instead.
    ///
    /// # Errors
    /// [`CliError::Config`] when `require_capability_token` is `true` with
    /// an empty `trusted_issuers`.
    pub fn validate(&self) -> Result<()> {
        if self.require_capability_token && self.trusted_issuers.is_empty() {
            return Err(CliError::Config(
                "config.json: require_capability_token=true with an empty \
                 trusted_issuers list would reject every session (no issuer \
                 is trusted): add at least one trusted issuer or set \
                 require_capability_token=false"
                    .into(),
            ));
        }
        Ok(())
    }
}

/// Load the effective token config from `~/.nexus42/connect/config.json`.
///
/// An absent file is **not** an error — it yields
/// [`ConnectTokenConfig::default`] (empty / false / None, the pre-V1.155
/// behavior). An existing file that is malformed, contains unknown fields,
/// or violates [`ConnectTokenConfig::validate`] is a hard boot error:
/// fail-closed, no silent defaults (architect lock #1).
///
/// # Parameters
/// `home` is the **raw user home** (`$HOME`); this fn joins `.nexus42`
/// internally via `connect_config_path`, so callers MUST NOT pre-join
/// `~/.nexus42`.
///
/// # Errors
/// [`CliError::Io`] when the file exists but cannot be read, or
/// [`CliError::Config`] when it is malformed, has unknown fields, or is
/// invalid (require-without-issuers).
pub fn load(home: &Path) -> Result<ConnectTokenConfig> {
    let path = connect_config_path(home);
    match std::fs::read_to_string(&path) {
        Ok(content) => {
            let config: ConnectTokenConfig = serde_json::from_str(&content)
                .map_err(|e| CliError::Config(format!("{}: {e}", path.display())))?;
            config.validate()?;
            Ok(config)
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(ConnectTokenConfig::default()),
        Err(err) => Err(CliError::Io(err)),
    }
}

/// Resolve the provider issuer key path.
///
/// The configured `issuer_key_path` (absolute → as-is; relative → against
/// `~/.nexus42/connect/`) is used when present; otherwise the default
/// `~/.nexus42/connect/issuer.key`. Empty/whitespace-only values are
/// treated as absent.
pub fn resolve_issuer_key_path(home: &Path, configured: Option<&str>) -> PathBuf {
    match configured.map(str::trim).filter(|p| !p.is_empty()) {
        Some(path) if Path::new(path).is_absolute() => PathBuf::from(path),
        Some(path) => nexus_home_layout::connect_dir(home).join(path),
        None => nexus_home_layout::connect_issuer_key_path(home),
    }
}

#[cfg(all(test, feature = "connect-host"))]
mod tests {
    use super::*;

    fn temp_home() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    fn write_config(home: &Path, body: &str) {
        let path = connect_config_path(home);
        std::fs::create_dir_all(path.parent().expect("connect dir parent")).expect("create dirs");
        std::fs::write(&path, body).expect("write config.json");
    }

    #[test]
    fn absent_file_yields_defaults() {
        let home = temp_home();
        let config = load(home.path()).expect("absent config.json is not an error");
        assert_eq!(config, ConnectTokenConfig::default());
        assert!(config.trusted_issuers.is_empty());
        assert!(!config.require_capability_token);
        assert!(config.capability_token_provider.is_none());
    }

    #[test]
    fn full_config_round_trips() {
        let home = temp_home();
        write_config(
            home.path(),
            r#"{
                "trusted_issuers": ["12D3KooWIssuerA", "12D3KooWIssuerB"],
                "require_capability_token": true,
                "capability_token_provider": {
                    "enabled": true,
                    "issuer_key_path": "/abs/path/issuer.key"
                }
            }"#,
        );
        let config = load(home.path()).expect("valid config loads");
        assert_eq!(
            config.trusted_issuers,
            vec!["12D3KooWIssuerA".to_string(), "12D3KooWIssuerB".to_string()]
        );
        assert!(config.require_capability_token);
        let provider = config
            .capability_token_provider
            .as_ref()
            .expect("provider block present");
        assert!(provider.enabled);
        assert_eq!(
            provider.issuer_key_path.as_deref(),
            Some("/abs/path/issuer.key")
        );

        // Serialize → reparse: the on-disk shape round-trips exactly.
        let serialized = serde_json::to_string(&config).expect("serialize");
        let reparsed: ConnectTokenConfig = serde_json::from_str(&serialized).expect("reparse");
        assert_eq!(reparsed, config);
    }

    #[test]
    fn malformed_json_fails_boot() {
        let home = temp_home();
        write_config(home.path(), "{ not json");
        let err = load(home.path()).expect_err("malformed config.json must fail boot");
        assert!(
            matches!(err, CliError::Config(_)),
            "malformed config is a boot error: {err:?}"
        );
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let home = temp_home();
        write_config(
            home.path(),
            r#"{ "trustedIssuers": [], "require_capability_token": false }"#,
        );
        let err = load(home.path()).expect_err("unknown field must fail boot");
        assert!(
            matches!(err, CliError::Config(_)),
            "unknown field is a boot error: {err:?}"
        );
    }

    #[test]
    fn require_without_issuers_fails_boot() {
        let home = temp_home();
        write_config(home.path(), r#"{ "require_capability_token": true }"#);
        let err = load(home.path()).expect_err("require-without-issuers must fail boot");
        assert!(
            matches!(err, CliError::Config(_)),
            "require-without-issuers is a boot error: {err:?}"
        );
    }

    #[test]
    fn require_with_issuers_loads() {
        let home = temp_home();
        write_config(
            home.path(),
            r#"{ "require_capability_token": true, "trusted_issuers": ["12D3KooWIssuerA"] }"#,
        );
        let config = load(home.path()).expect("require + issuer is valid");
        assert!(config.require_capability_token);
        assert_eq!(config.trusted_issuers, vec!["12D3KooWIssuerA".to_string()]);
    }

    #[test]
    fn provider_block_without_enabled_ignored_by_defaults() {
        let home = temp_home();
        write_config(
            home.path(),
            r#"{ "capability_token_provider": { "enabled": false, "issuer_key_path": "/nope.key" } }"#,
        );
        let config = load(home.path()).expect("disabled provider loads");
        let provider = config
            .capability_token_provider
            .as_ref()
            .expect("block present");
        assert!(!provider.enabled);
        assert_eq!(provider.issuer_key_path.as_deref(), Some("/nope.key"));
    }

    #[test]
    fn issuer_key_path_resolution() {
        let home = Path::new("/fake/home");
        let default = resolve_issuer_key_path(home, None);
        assert_eq!(default, nexus_home_layout::connect_issuer_key_path(home));

        // Absolute path used as-is.
        assert_eq!(
            resolve_issuer_key_path(home, Some("/etc/issuer.key")),
            PathBuf::from("/etc/issuer.key")
        );
        // Relative path resolves against ~/.nexus42/connect/.
        assert_eq!(
            resolve_issuer_key_path(home, Some("keys/issuer.key")),
            PathBuf::from("/fake/home/.nexus42/connect/keys/issuer.key")
        );
        // Empty / whitespace-only values fall back to the default.
        assert_eq!(resolve_issuer_key_path(home, Some("")), default);
        assert_eq!(resolve_issuer_key_path(home, Some("   ")), default);
    }
}
