//! The signed Connect open of a selected remote memory provider
//! (`n3-provider-binding.md` §3–§4, feature `memory-remote`).
//!
//! One open resolves to one established [`MemoryProviderBinding`]. This module
//! owns the remote half of that resolution and nothing else:
//!
//! 1. Load the operator-provisioned credential from the validated selection
//!    (§3.1) — the seed never leaves this call and the dial.
//! 2. Dial the exact configured raw endpoint through the one product WS
//!    [`Transport`], with this dial's own signed hello and the allowlist.
//! 3. Require the negotiated `spoke-memory` family and `memory.describe` v1.
//! 4. Discover the exact configured space in bounded pages; zero or several
//!    matches fail.
//! 5. Require the space's `authority_host_id` to be the authenticated host,
//!    and pin the loaded public peer identity the dial presented (§5.1/D18).
//!
//! Every failure closes the session this call owns and returns one static coded
//! refusal: no peer-supplied text, no second provider, and no local fallback.
//! The same ownership covers cancellation: once the signed session is
//! established, [`UndeliveredSession`] holds it and closes it on drop until a
//! binding takes responsibility, so an open cancelled while `describe` or
//! `spaces.list` is pending releases exactly its own session.
//!
//! Nexus serves no memory family — this is a client only.

use std::num::NonZeroU64;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use serde_json::json;
use spoke_connect::core::{
    derive_peer_id_from_ed25519_pubkey, CapabilityTokenProof, CAPABILITY_SPOKE_MEMORY,
};
use spoke_connect::remote::{
    connect_remote_adapter, RemoteAdapter, RemoteAdapterError, RemoteAdapterOptions, RemoteIdentity,
    Transport,
};
use spoke_operations::spoke_schemas::{
    HostCapabilityManifest, MemoryDescribeRequest, MemoryDescribeResponse, MemorySpacesListRequest,
    MemorySpacesListResponse, Principal,
};
use spoke_operations::{MemoryPort, SpokeResult};

use crate::connect::ws_transport::{ws_config, WsTransport, DEFAULT_MAX_ENVELOPE_BYTES};
use crate::error::{CoreError, CoreResult};
use crate::memory_binding::MemoryProviderBinding;
use crate::memory_credentials::{MemoryCredential, CREDENTIAL_INVALID_CODE};
use crate::memory_selection::RemoteMemoryProviderSelection;
use crate::service::CoreAccess;

/// The signed memory protocol version this client speaks (§4.1 step 2).
const MEMORY_PROTOCOL_VERSION: u64 = 1;
/// Bound on the WebSocket connect and on the owned close after a failed dial.
const CONNECT_BOUND: Duration = Duration::from_secs(10);
/// Bound on the space discovery walk: the exact space must appear within this
/// many pages, or the open fails instead of paging a provider forever.
const MAX_SPACE_PAGES: usize = 8;

/// Static refusal categories (§3.1). Peer-controlled text never reaches them.
const HANDSHAKE_REFUSED: &str = "memory_remote_handshake";
const UNAVAILABLE: &str = "memory_remote_unavailable";
const CAPABILITY_REFUSED: &str = "memory_remote_capability";
const SPACE_REFUSED: &str = "memory_remote_space";
const AUTHORITY_REFUSED: &str = "memory_remote_authority";

/// Open the selected remote provider and pin it as this open's binding.
///
/// # Errors
///
/// Returns a static coded refusal for every failure — `memory_credential_invalid`
/// (the credential file, §3.1), `memory_remote_unavailable` (the endpoint cannot
/// be reached), `memory_remote_handshake` (signed hello / negotiation),
/// `memory_remote_capability` (`describe` v1 / the `spoke-memory` family),
/// `memory_remote_space` (the configured space was not found exactly once) or
/// `memory_remote_authority` (the space authority is not the authenticated
/// host). The partially opened session is closed before returning, and no
/// local provider is constructed or substituted.
pub(crate) async fn open_remote_binding(
    user_home: &Path,
    selection: &RemoteMemoryProviderSelection,
    access: CoreAccess,
) -> CoreResult<MemoryProviderBinding> {
    let credential = MemoryCredential::load(user_home, &selection.credential_id)?;
    let remote_pubkey = decode_pubkey_hex(&selection.remote_pubkey_hex)?;
    let capability_token = match credential.capability_token() {
        None => None,
        Some(token) => Some(
            serde_json::from_value::<CapabilityTokenProof>(token.clone()).map_err(|_| {
                coded(
                    CREDENTIAL_INVALID_CODE,
                    "capability_token is not the published proof shape",
                )
            })?,
        ),
    };
    // The identity this dial presents, and the identity the binding pins: the
    // Ed25519 PUBLIC peer id derived from the same loaded seed the signed hello
    // signs with. It is never the credential id, a path, a seed hash or a
    // payload principal (§5.1/D18).
    let local_seed = *credential.identity_seed();
    let client_identity = client_public_peer_id(&local_seed);

    let (ws, _response) = tokio::time::timeout(
        CONNECT_BOUND,
        tokio_tungstenite::connect_async_with_config(
            selection.connect_endpoint.as_str(),
            // The same inbound cap the product WS transport applies on its
            // accept side (frozen contract §2), so neither end accepts an
            // envelope the other would refuse.
            Some(ws_config(DEFAULT_MAX_ENVELOPE_BYTES)),
            false,
        ),
    )
    .await
    .map_err(|_| coded(UNAVAILABLE, "the remote memory endpoint did not connect in time"))?
    .map_err(|_| coded(UNAVAILABLE, "the remote memory endpoint is unreachable"))?;
    let transport: Arc<dyn Transport> = Arc::new(WsTransport::new(ws));

    let dialed = connect_remote_adapter(RemoteAdapterOptions {
        transport: Arc::clone(&transport),
        local_identity: RemoteIdentity { seed: local_seed },
        local_manifest: client_manifest(&client_identity)?,
        remote_pubkey,
        allowlist: selection.allowlist.clone(),
        invoke_timeout_ms: None,
        capability_token,
    })
    .await;
    let adapter = match dialed {
        Ok(adapter) => adapter,
        Err(error) => {
            // The SDK closes the session on its own handshake failures. Its
            // fail-closed pre-I/O refusal returns before an adapter exists, so
            // this end still owns the transport and closes it here.
            let _ = tokio::time::timeout(CONNECT_BOUND, transport.close()).await;
            return Err(dial_refusal(&error));
        }
    };

    // From here the session is established but not yet pinned: the guard owns
    // its cleanup until a binding takes it (see `UndeliveredSession`).
    let session = UndeliveredSession::new(adapter);

    let admitted = admit(session.adapter(), &client_identity, selection).await;
    match admitted {
        Ok((space_id, authority_host_id, authority_generation)) => {
            Ok(MemoryProviderBinding::pin_remote(
                session.into_adapter(),
                space_id,
                authority_host_id,
                authority_generation,
                client_identity,
                access != CoreAccess::ReadOnly,
            ))
        }
        // The explicit refusal is returned unchanged; the guard closes the
        // session on the way out, and the caller's failed-open path then
        // releases the pool admission.
        Err(error) => Err(error),
    }
}

/// Owns one established, not-yet-pinned session.
///
/// The admission sequence awaits `memory.describe` and `memory.spaces.list`
/// after the signed handshake, and a provider can leave either call pending.
/// If the open future is dropped there, the explicit refusal arm never runs;
/// dropping this call's `Arc` alone closes nothing, because the SDK's receive
/// task keeps its own `Arc` and the authenticated session parked on the
/// transport. The guard therefore runs the adapter's own idempotent `close` on
/// drop — releasing the socket so that task and its session end — and is
/// disarmed only when a valid binding takes ownership, whose normal close
/// semantics then apply.
struct UndeliveredSession(Option<Arc<RemoteAdapter>>);

impl UndeliveredSession {
    fn new(adapter: Arc<RemoteAdapter>) -> Self {
        Self(Some(adapter))
    }

    fn adapter(&self) -> &RemoteAdapter {
        self.0
            .as_deref()
            .expect("the guard holds its adapter until handed over")
    }

    /// Hand the session to the binding; cleanup becomes the binding's job.
    fn into_adapter(mut self) -> Arc<RemoteAdapter> {
        self.0
            .take()
            .expect("the guard holds its adapter until handed over")
    }
}

impl Drop for UndeliveredSession {
    fn drop(&mut self) {
        if let Some(adapter) = self.0.take() {
            adapter.close();
        }
    }
}

/// The typed admission the §4.1 sequence requires, over the established
/// session.
async fn admit(
    adapter: &RemoteAdapter,
    client_identity: &str,
    selection: &RemoteMemoryProviderSelection,
) -> CoreResult<(String, String, NonZeroU64)> {
    let manifest = adapter
        .remote_manifest()
        .ok_or_else(|| coded(HANDSHAKE_REFUSED, "the signed hello carried no remote manifest"))?;
    // The negotiated set is the intersection of the two authenticated hello
    // manifests. This dial's manifest advertises exactly the memory family it
    // is a client of, so the remote side must declare the same family.
    if !manifest
        .capabilities
        .iter()
        .any(|capability| capability == CAPABILITY_SPOKE_MEMORY)
    {
        return Err(coded(
            CAPABILITY_REFUSED,
            "the remote host did not negotiate the spoke-memory family",
        ));
    }
    // The adapter ignores this value: the server maps the authenticated peer id
    // to its own stable principal (§4.1), so this is a label, never authority.
    let principal = client_principal(client_identity)?;
    let described = adapter
        .describe(
            &principal,
            MemoryDescribeRequest {
                extensions: std::collections::HashMap::new(),
            },
        )
        .await;
    let (protocol_version, max_page_size) = match described {
        SpokeResult::Ok(MemoryDescribeResponse::Variant0 {
            memory_protocol_version,
            limits,
            ..
        }) => (memory_protocol_version, limits.max_page_size),
        SpokeResult::Ok(_) => {
            return Err(coded(
                CAPABILITY_REFUSED,
                "the remote provider answered memory.describe with an error branch",
            ))
        }
        SpokeResult::Reject(_) => {
            return Err(coded(
                CAPABILITY_REFUSED,
                "the remote provider refused memory.describe",
            ))
        }
    };
    if protocol_version.get() != MEMORY_PROTOCOL_VERSION {
        return Err(coded(
            CAPABILITY_REFUSED,
            "the remote provider serves another memory protocol version",
        ));
    }

    let (space_id, authority_host_id, authority_generation) =
        discover_space(adapter, &principal, selection, max_page_size).await?;
    if authority_host_id != manifest.host_id.as_str() {
        return Err(coded(
            AUTHORITY_REFUSED,
            "the discovered space is not authoritative on the authenticated host",
        ));
    }
    Ok((space_id, authority_host_id, authority_generation))
}

/// Find the exact configured space in bounded provider pages.
///
/// Zero matches and several matches both fail: the selection names one space,
/// and silently taking the first page's entry would bind another authority.
async fn discover_space(
    adapter: &RemoteAdapter,
    principal: &Principal,
    selection: &RemoteMemoryProviderSelection,
    page_size: NonZeroU64,
) -> CoreResult<(String, String, NonZeroU64)> {
    let mut matched: Option<(String, String, NonZeroU64)> = None;
    let mut cursor: Option<String> = None;
    let mut pages = 0_usize;
    loop {
        if pages == MAX_SPACE_PAGES {
            return Err(coded(
                SPACE_REFUSED,
                "space discovery exceeded its page bound",
            ));
        }
        pages += 1;
        let request = spaces_request(cursor.as_deref(), page_size)?;
        let (spaces, next_cursor) = match adapter.list_spaces(principal, request).await {
            SpokeResult::Ok(MemorySpacesListResponse::Variant0 {
                spaces,
                next_cursor,
                ..
            }) => (spaces, next_cursor),
            SpokeResult::Ok(_) => {
                return Err(coded(
                    SPACE_REFUSED,
                    "the remote provider answered memory.spaces.list with an error branch",
                ))
            }
            SpokeResult::Reject(_) => {
                return Err(coded(
                    SPACE_REFUSED,
                    "the remote provider refused memory.spaces.list",
                ))
            }
        };
        for space in spaces {
            if space.space_id.as_str() == selection.space_id {
                if matched.is_some() {
                    return Err(coded(
                        SPACE_REFUSED,
                        "the configured space appears more than once",
                    ));
                }
                matched = Some((
                    space.space_id.as_str().to_owned(),
                    space.authority_host_id.as_str().to_owned(),
                    space.authority_generation,
                ));
            }
        }
        match next_cursor {
            None => break,
            Some(next) => cursor = Some(next.as_str().to_owned()),
        }
    }
    matched.ok_or_else(|| {
        coded(
            SPACE_REFUSED,
            "the configured space is not visible to this identity",
        )
    })
}

/// One bounded `memory.spaces.list` page over the configured space family.
fn spaces_request(cursor: Option<&str>, limit: NonZeroU64) -> CoreResult<MemorySpacesListRequest> {
    let mut body = json!({ "limit": limit.get() });
    if let Some(cursor) = cursor {
        body["cursor"] = json!(cursor);
    }
    serde_json::from_value(body)
        .map_err(|_| coded(SPACE_REFUSED, "the space discovery request is not protocol-shaped"))
}

/// The local hello manifest: this client's own identity and the one family it
/// dials for. It declares no served op and no tool — Nexus serves no memory
/// responder (§1).
fn client_manifest(client_identity: &str) -> CoreResult<HostCapabilityManifest> {
    serde_json::from_value(json!({
        "schema_version": 1,
        "host_id": client_identity,
        "roles": ["memory-client"],
        "capabilities": [CAPABILITY_SPOKE_MEMORY],
        "namespaces": [],
        "extensions": {},
    }))
    .map_err(|_| coded(HANDSHAKE_REFUSED, "the client hello manifest is not protocol-shaped"))
}

/// The public peer id of the loaded seed: the keypair the signed hello signs
/// with and the identity the binding pins (§5.1/D18).
fn client_public_peer_id(seed: &[u8; 32]) -> String {
    let verifying_key = ed25519_dalek::SigningKey::from_bytes(seed)
        .verifying_key()
        .to_bytes();
    derive_peer_id_from_ed25519_pubkey(&verifying_key)
}

/// The label the adapter's `MemoryPort` methods take (`RemoteAdapter` ignores
/// it; the server's admission maps the authenticated peer id instead).
fn client_principal(client_identity: &str) -> CoreResult<Principal> {
    serde_json::from_value(json!({
        "principal_id": client_identity,
        "extensions": {},
    }))
    .map_err(|_| coded(HANDSHAKE_REFUSED, "the loaded client identity is not a principal label"))
}

/// Decode the selection's already-validated 32-byte peer key.
fn decode_pubkey_hex(text: &str) -> CoreResult<[u8; 32]> {
    if text.len() != 64 {
        return Err(invalid_pubkey());
    }
    let mut pubkey = [0_u8; 32];
    for (slot, pair) in pubkey.iter_mut().zip(text.as_bytes().chunks_exact(2)) {
        let high = hex_value(pair[0]).ok_or_else(invalid_pubkey)?;
        let low = hex_value(pair[1]).ok_or_else(invalid_pubkey)?;
        *slot = (high << 4) | low;
    }
    Ok(pubkey)
}

const fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

fn invalid_pubkey() -> CoreError {
    CoreError::InvalidInput {
        field: "remote_pubkey_hex".to_string(),
        reason: "must be 64 lowercase hex characters".to_string(),
    }
}

/// Classify one dial failure without copying any upstream message: the SDK's
/// error text can carry peer-supplied detail, so only the static class
/// survives (§3.1).
fn dial_refusal(error: &RemoteAdapterError) -> CoreError {
    match error {
        RemoteAdapterError::Config(_) => coded(
            HANDSHAKE_REFUSED,
            "the remote memory dial configuration was refused",
        ),
        RemoteAdapterError::Handshake(_) => {
            coded(HANDSHAKE_REFUSED, "the signed Connect handshake failed")
        }
        RemoteAdapterError::ProtocolVersionMismatch(_) => coded(
            HANDSHAKE_REFUSED,
            "the remote Connect protocol version does not match",
        ),
        RemoteAdapterError::Timeout(_) => coded(
            UNAVAILABLE,
            "the signed Connect handshake did not complete in time",
        ),
    }
}

/// One static coded refusal.
fn coded(code: &str, message: &str) -> CoreError {
    CoreError::Coded {
        code: code.to_string(),
        message: message.to_string(),
    }
}
