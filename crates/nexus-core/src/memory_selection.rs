//! Memory provider selection — the validated in-memory form of the on-disk
//! `[memory_provider]` table (`n3-provider-binding.md` §2–§3.2).
//!
//! The selection names an authority and its non-secret references only. It is
//! validated at every trust boundary that reads it (disk → open, CLI → write)
//! and never carries credential material.

use nexus_home_layout::MemoryProviderConfig;

use crate::error::{CoreError, CoreResult};

/// Credential ids are also file stems, so the grammar is path-safe by
/// construction: `[A-Za-z0-9][A-Za-z0-9_-]{0,63}`.
const MAX_CREDENTIAL_ID_LEN: usize = 64;
const MAX_SPACE_ID_LEN: usize = 128;
const PUBKEY_HEX_LEN: usize = 64;

/// The memory provider authority selected for one open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemoryProviderSelection {
    /// Built-in local provider (the implicit default).
    Local,
    /// Remote Connect provider reference.
    Remote(RemoteMemoryProviderSelection),
}

/// Non-secret remote provider reference: ids, endpoint and trusted peers.
///
/// The fields are public as the §2.1 surface declares, so every reader and
/// writer validates through [`MemoryProviderSelection::from_config`],
/// [`MemoryProviderSelection::validate`] or
/// [`CoreHomeService::set_memory_provider_selection`] — never by trusting a
/// hand-built value.
///
/// [`CoreHomeService::set_memory_provider_selection`]: crate::CoreHomeService::set_memory_provider_selection
#[derive(Clone, PartialEq, Eq)]
pub struct RemoteMemoryProviderSelection {
    /// Provider space the selected authority serves.
    pub space_id: String,
    /// Operator-provisioned credential file id (not the secret itself).
    pub credential_id: String,
    /// `wss://` endpoint, or `ws://` for loopback tests only.
    pub connect_endpoint: String,
    /// Remote Ed25519 public key, 64 lowercase hex characters.
    pub remote_pubkey_hex: String,
    /// Trusted remote peer ids (fail-closed handshake).
    pub allowlist: Vec<String>,
}

// The selection may be logged: show ids and the sanitized endpoint, never
// credential-file contents or token material (§2.1).
impl std::fmt::Debug for RemoteMemoryProviderSelection {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RemoteMemoryProviderSelection")
            .field("space_id", &self.space_id)
            .field("credential_id", &self.credential_id)
            .field("connect_endpoint", &sanitized_endpoint(&self.connect_endpoint))
            .field("remote_pubkey_hex", &self.remote_pubkey_hex)
            .field("allowlist", &self.allowlist)
            .finish()
    }
}

impl MemoryProviderSelection {
    /// Convert the on-disk table into a validated selection.
    ///
    /// A missing table is the implicit local default (§2.2, rule 3), not an
    /// error.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidInput`] when a stored remote reference
    /// fails validation, so a malformed table never degrades to local
    /// silently.
    pub fn from_config(config: Option<MemoryProviderConfig>) -> CoreResult<Self> {
        match config {
            None | Some(MemoryProviderConfig::Local) => Ok(Self::Local),
            Some(MemoryProviderConfig::Remote {
                space_id,
                credential_id,
                connect_endpoint,
                remote_pubkey_hex,
                allowlist,
            }) => {
                let selection = Self::Remote(RemoteMemoryProviderSelection {
                    space_id,
                    credential_id,
                    connect_endpoint,
                    remote_pubkey_hex,
                    allowlist,
                });
                selection.validate()?;
                Ok(selection)
            }
        }
    }

    /// The on-disk table this selection serializes to.
    #[must_use]
    pub fn to_config(&self) -> MemoryProviderConfig {
        match self {
            Self::Local => MemoryProviderConfig::Local,
            Self::Remote(remote) => MemoryProviderConfig::Remote {
                space_id: remote.space_id.clone(),
                credential_id: remote.credential_id.clone(),
                connect_endpoint: remote.connect_endpoint.clone(),
                remote_pubkey_hex: remote.remote_pubkey_hex.clone(),
                allowlist: remote.allowlist.clone(),
            },
        }
    }

    /// Validate the selection before it is persisted or opened.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidInput`] naming the first malformed field.
    pub fn validate(&self) -> CoreResult<()> {
        match self {
            Self::Local => Ok(()),
            Self::Remote(remote) => remote.validate(),
        }
    }
}

impl RemoteMemoryProviderSelection {
    /// Validate the remote reference (§3.1 id grammar, §3.2 endpoint, peer
    /// identity shape).
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidInput`] naming the first malformed field.
    pub fn validate(&self) -> CoreResult<()> {
        if !is_valid_space_id(&self.space_id) {
            return Err(invalid(
                "space_id",
                "must be 1-128 characters without control characters or whitespace",
            ));
        }
        if !is_valid_credential_id(&self.credential_id) {
            return Err(invalid(
                "credential_id",
                "must match [A-Za-z0-9][A-Za-z0-9_-]{0,63}",
            ));
        }
        validate_connect_endpoint(&self.connect_endpoint)?;
        if self.remote_pubkey_hex.len() != PUBKEY_HEX_LEN
            || !self.remote_pubkey_hex.bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err(invalid(
                "remote_pubkey_hex",
                "must be 64 lowercase hex characters",
            ));
        }
        if self.allowlist.is_empty()
            || self
                .allowlist
                .iter()
                .any(|peer| peer.is_empty() || peer.chars().any(char::is_whitespace))
        {
            return Err(invalid(
                "allowlist",
                "must list at least one non-empty trusted peer id",
            ));
        }
        Ok(())
    }
}

/// Whether `id` is a valid credential-file stem (§3.1).
#[must_use]
pub(crate) fn is_valid_credential_id(id: &str) -> bool {
    let mut characters = id.chars();
    match characters.next() {
        Some(first) if first.is_ascii_alphanumeric() => {}
        _ => return false,
    }
    id.len() <= MAX_CREDENTIAL_ID_LEN
        && characters.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn is_valid_space_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_SPACE_ID_LEN
        && !id.chars().any(|c| c.is_control() || c.is_whitespace())
}

fn validate_connect_endpoint(endpoint: &str) -> CoreResult<()> {
    // Validate the exact raw string the dial path hands to
    // `tokio_tungstenite::connect_async`: it parses that text as an
    // `http::Uri` (`IntoClientRequest for &str`), so this is the transport's
    // own grammar rather than a second, normalized parser (§3.2).
    if endpoint.contains('#') {
        return Err(invalid(
            "connect_endpoint",
            "must not carry a fragment component",
        ));
    }
    let uri = endpoint
        .parse::<http::Uri>()
        .map_err(|_| invalid("connect_endpoint", "must be an absolute ws:// or wss:// URL"))?;
    let authority = uri
        .authority()
        .ok_or_else(|| invalid("connect_endpoint", "must name a host"))?;
    // The transport keeps userinfo in the authority it forwards; the raw
    // component must be rejected even when it is empty (`wss://@host`).
    if authority.as_str().contains('@') {
        return Err(invalid("connect_endpoint", "must not carry userinfo"));
    }
    if uri.query().is_some() {
        return Err(invalid(
            "connect_endpoint",
            "must not carry a query component",
        ));
    }
    let host = authority.host();
    if host.is_empty() {
        return Err(invalid("connect_endpoint", "must name a host"));
    }
    // The transport connects to `uri.port_u16()` or the scheme default, so an
    // unreadable or empty port must not silently become 443/80.
    let port_suffix = authority.as_str().strip_prefix(host).unwrap_or_default();
    if !port_suffix.is_empty() {
        let port = port_suffix.strip_prefix(':').unwrap_or_default();
        let valid = !port.is_empty()
            && port.bytes().all(|byte| byte.is_ascii_digit())
            && port.parse::<u16>().is_ok_and(|port| port != 0);
        if !valid {
            return Err(invalid("connect_endpoint", "port must be 1-65535"));
        }
    }
    match uri.scheme_str() {
        Some("wss") => Ok(()),
        Some("ws") if is_locked_loopback_host(host) => Ok(()),
        Some("ws") => Err(invalid(
            "connect_endpoint",
            "ws:// is allowed only for loopback hosts; use wss://",
        )),
        _ => Err(invalid("connect_endpoint", "scheme must be ws or wss")),
    }
}

/// The locked `ws://` host set (§3.2): exactly `127.0.0.1`, `::1` or
/// `localhost`, not the whole 127/8 range.
fn is_locked_loopback_host(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    let address = host
        .strip_prefix('[')
        .and_then(|host| host.strip_suffix(']'))
        .unwrap_or(host);
    if let Ok(address) = address.parse::<std::net::Ipv4Addr>() {
        return address == std::net::Ipv4Addr::LOCALHOST;
    }
    address
        .parse::<std::net::Ipv6Addr>()
        .is_ok_and(|address| address.is_loopback())
}

/// Drop userinfo, query and fragment so a hand-built invalid value can never
/// leak credentials through [`std::fmt::Debug`].
fn sanitized_endpoint(endpoint: &str) -> String {
    let without_fragment = endpoint.split('#').next().unwrap_or_default();
    let without_query = without_fragment.split('?').next().unwrap_or_default();
    match without_query.split_once("://") {
        Some((scheme, rest)) => {
            let host = rest.rsplit_once('@').map_or(rest, |(_, host)| host);
            format!("{scheme}://{host}")
        }
        None => without_query.to_string(),
    }
}

fn invalid(field: &str, reason: &str) -> CoreError {
    CoreError::InvalidInput {
        field: field.to_string(),
        reason: reason.to_string(),
    }
}
