//! Raw Nexus invoke bridge for the Trpg-legend standalone context probe.
//!
//! The bridge dials a real `nexus-runtime` through `spoke-connect` and keeps
//! the JavaScript boundary deliberately small: one NDJSON request on stdin,
//! one correlated NDJSON response on stdout. Diagnostics stay on stderr so
//! stdout remains machine-parseable.

use anyhow::{anyhow, Context};
use libp2p::identity::Keypair;
use serde::Deserialize;
use serde_json::{json, Value};
use spoke_connect::{parse_multiaddr, ConnectConfig, InvokeError, PeerSession, SpokeConnectNode};
use spoke_schemas::connect::connect_hello::HostCapabilityManifest;
use std::collections::HashMap;
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

struct Args {
    identity_seed: PathBuf,
    addr: Option<String>,
    host_peer: Option<String>,
    print_peer_only: bool,
}

#[derive(Deserialize)]
struct BridgeCommand {
    id: String,
    op: String,
    payload: Value,
}

fn parse_args() -> anyhow::Result<Args> {
    let mut identity_seed = None;
    let mut addr = None;
    let mut host_peer = None;
    let mut print_peer_only = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--identity-seed" => identity_seed = args.next().map(PathBuf::from),
            "--addr" => addr = args.next(),
            "--host-peer" => host_peer = args.next(),
            "--print-peer-only" => print_peer_only = true,
            other => return Err(anyhow!("unknown argument: {other}")),
        }
    }
    Ok(Args {
        identity_seed: identity_seed.ok_or_else(|| anyhow!("--identity-seed is required"))?,
        addr,
        host_peer,
        print_peer_only,
    })
}

fn load_identity(path: &Path) -> anyhow::Result<Keypair> {
    let bytes =
        std::fs::read(path).with_context(|| format!("read identity seed at {}", path.display()))?;
    let seed: [u8; 32] = bytes
        .try_into()
        .map_err(|bytes: Vec<u8>| anyhow!("identity seed must be 32 bytes, got {}", bytes.len()))?;
    Keypair::ed25519_from_bytes(seed).map_err(|error| anyhow!("invalid Ed25519 seed: {error}"))
}

fn bridge_manifest() -> HostCapabilityManifest {
    HostCapabilityManifest {
        authority: None,
        capabilities: vec!["spoke-baseline".into()],
        extensions: HashMap::default(),
        host_id: "trpg-context-probe"
            .parse()
            .expect("static host id is valid"),
        namespaces: Vec::new(),
        roles: vec!["input-source".into()],
        schema_version: NonZeroU64::new(1).expect("one is non-zero"),
        // V1.169 (0.11.1): honest empty tools declaration (no tool ABI served).
        tools: Vec::new(),
    }
}

fn served_ops(manifest: &HostCapabilityManifest) -> Vec<String> {
    serde_json::to_value(manifest)
        .ok()
        .and_then(|value| value.pointer("/extensions/nexus/served_ops").cloned())
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|value| value.as_str().map(ToOwned::to_owned))
        .collect()
}

#[expect(clippy::missing_const_for_fn)] // match-only mapping kept non-const; const conversion is an off-path refactor (AR-100 off-path set)
fn transport_code(error: &InvokeError) -> &'static str {
    match error {
        InvokeError::SessionClosed | InvokeError::SequenceExhausted => "SESSION_CLOSED",
        InvokeError::Wire(_) => "REMOTE_OPERATION_ERROR",
        InvokeError::Transport(_) | InvokeError::CorrelationMismatch => "TRANSPORT_UNAVAILABLE",
    }
}

async fn emit(stdout: &mut tokio::io::Stdout, value: &Value) -> anyhow::Result<()> {
    let mut bytes = serde_json::to_vec(value)?;
    bytes.push(b'\n');
    stdout.write_all(&bytes).await?;
    stdout.flush().await?;
    Ok(())
}

async fn connect_host(
    node: &SpokeConnectNode,
    addr: &libp2p::Multiaddr,
    host_peer: libp2p::PeerId,
) -> Result<PeerSession, String> {
    let session = node
        .connect(addr.clone())
        .await
        .map_err(|error| error.to_string())?;
    if session.remote_peer_id() != host_peer {
        return Err(format!(
            "connected peer {} did not match expected host {host_peer}",
            session.remote_peer_id()
        ));
    }
    Ok(session)
}

#[tokio::main]
#[expect(clippy::too_many_lines)] // AR-102: linear bridge setup + event-loop teardown in one executable; extraction out of scope
async fn main() -> anyhow::Result<()> {
    let args = parse_args()?;
    let identity = load_identity(&args.identity_seed)?;
    let local_peer = identity.public().to_peer_id();
    if args.print_peer_only {
        println!("BRIDGE_PEER_ID={local_peer}");
        return Ok(());
    }
    let host_peer: libp2p::PeerId = args
        .host_peer
        .ok_or_else(|| anyhow!("--host-peer is required"))?
        .parse()
        .context("parse --host-peer as PeerId")?;
    let addr = parse_multiaddr(&args.addr.ok_or_else(|| anyhow!("--addr is required"))?)?;
    let config = ConnectConfig {
        identity,
        peer_allowlist: vec![host_peer],
        listen_addrs: vec![parse_multiaddr("/ip4/127.0.0.1/tcp/0")?],
        local_manifest: bridge_manifest(),
        handshake_timeout: Some(Duration::from_secs(10)),
        invoke_handler: None,
        invoke_handler_v2: None,
        op_capability_requirements: HashMap::new(),
        trusted_issuers: Vec::new(),
        require_capability_token: false,
        capability_token_provider: None,
    };
    let node = SpokeConnectNode::start(config).await?;
    let mut session = connect_host(&node, &addr, host_peer)
        .await
        .map_err(anyhow::Error::msg)?;

    let mut stdout = tokio::io::stdout();
    emit(
        &mut stdout,
        &json!({
            "type": "ready",
            "bridge_version": env!("CARGO_PKG_VERSION"),
            "peer_id": local_peer.to_string(),
            "remote_peer_id": session.remote_peer_id().to_string(),
            "served_ops": served_ops(session.remote_manifest()),
        }),
    )
    .await?;

    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        let command: BridgeCommand = serde_json::from_str(&line).context("parse NDJSON command")?;
        let invoke_result = session
            .invoke(command.op.clone(), command.payload.clone())
            .await;
        let invoke_result = match invoke_result {
            Err(InvokeError::SessionClosed) => {
                session = match connect_host(&node, &addr, host_peer).await {
                    Ok(reconnected_session) => reconnected_session,
                    Err(error) => {
                        emit(
                            &mut stdout,
                            &json!({
                                "id": command.id,
                                "ok": false,
                                "kind": "transport",
                                "error": {
                                    "code": "TRANSPORT_UNAVAILABLE",
                                    "message": error,
                                },
                            }),
                        )
                        .await?;
                        continue;
                    }
                };
                session.invoke(command.op, command.payload).await
            }
            result => result,
        };

        let response = match invoke_result {
            Ok(success) => json!({
                "id": command.id,
                "ok": true,
                "payload": success.payload,
            }),
            Err(InvokeError::Wire(error)) => json!({
                "id": command.id,
                "ok": false,
                "kind": "wire",
                "error": error,
            }),
            Err(error) => json!({
                "id": command.id,
                "ok": false,
                "kind": "transport",
                "error": {
                    "code": transport_code(&error),
                    "message": error.to_string(),
                },
            }),
        };
        emit(&mut stdout, &response).await?;
    }

    node.shutdown().await?;
    Ok(())
}
