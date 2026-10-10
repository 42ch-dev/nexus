//! Minimal first-party consumer for the v1.207 Connect WebSocket event lane.
//!
//! Drives ONE `tools.nexus.subscribe` → host-pushed
//! `tools.nexus.deliver_events` → cursor-ack round-trip against a running
//! Connect host (`nexus42 connect start` / `nexus-runtime`). It speaks the
//! shipped peer stack — the `spoke_connect::remote` adapter over the
//! `nexus_core::connect::WsTransport` WebSocket transport — and uses a
//! fixed-seed Ed25519 identity so the operator's allowlist entry is stable
//! across runs.
//!
//! ## Operator prerequisites (stock, fail-closed host)
//!
//! The lane is loopback-only and fail-closed. This example never loosens that
//! posture — it only presents the identity the operator chose to admit.
//!
//! 1. Start the host and read its readiness block:
//!
//!    ```text
//!    nexus42 connect start
//!      peer_id: 12D3KooW...
//!      ...
//!      event_lane: ws://127.0.0.1:8425/connect
//!    ```
//!
//!    The `event_lane:` value is this example's `ADDR` argument.
//!
//!    NOTE: the block's `peer_id:` is the N-C1 node lane's libp2p peer id
//!    (from `~/.nexus42/connect/identity.key`) — it is NOT the event lane's
//!    identity. The event lane is its own responder with its own persistent
//!    Ed25519 seed at `~/.nexus42/connect/daemon_identity.key`, so its
//!    trust anchor is supplied to this example rather than read off the
//!    readiness block (see step 2).
//! 2. Give this example the event lane's trust anchor — its Ed25519 public
//!    key:
//!    - `--host-pubkey <64-hex>` when the key is distributed out of band, or
//!    - `--host-identity-key <PATH>` pointing at the lane's identity seed
//!      file (default `~/.nexus42/connect/daemon_identity.key`); the lane is
//!      loopback-only by design, so a local consumer running as the host's
//!      user can read the seed and derive the public key.
//! 3. Admit this consumer on the event lane (print its identity with
//!    `--print-peer-only`):
//!    - add its `consumer_peer_id` to the event lane's dialer allowlist in
//!      `~/.nexus42/connect/daemon.json`
//!      (`{"peer_ids": ["<peer_id>"]}`) — this WS-lane config is distinct
//!      from `connect/allowlist.json` and `connect start --allow-peer`,
//!      which govern the N-C1 node lane (the readiness block's
//!      `allowlisted peers:` line);
//!    - pin its key in `~/.nexus42/connect/peer_keys.json`
//!      (`{"peer_keys": {"<peer_id>": "<64-hex pubkey>"}}`) — the lane's
//!      Layer-0 handshake refuses an allowlisted dialer with no pinned key.
//! 4. No capability-token gate applies to the event lane. Token policy is a
//!    N-C1 node-lane concern (`~/.nexus42/connect/config.json` +
//!    `ConnectConfig.require_capability_token`). The WS lane's responder is
//!    composed without a token/issuer field, and its own
//!    `~/.nexus42/connect/daemon.json` (`PeerToolsConfig`) has no token
//!    setting — it is `deny_unknown_fields`, so a borrowed token switch
//!    makes the file invalid and the lane refuses to boot. This example
//!    presents no token proof and needs none on HEAD.
//!
//! ## Usage
//!
//! ```text
//! cargo build -p nexus42 --features connect-host --example connect_event_consumer
//! ./target/debug/examples/connect_event_consumer --print-peer-only
//! ./target/debug/examples/connect_event_consumer \
//!     ws://127.0.0.1:8425/connect \
//!     --host-pubkey <64-hex event-lane pubkey> \
//!     --stream demo \
//!     [--cursor <epoch>:<seq>]
//! ```
//!
//! Exit codes: `0` a batch was delivered and its ack write completed; `1`
//! transport/handshake failure; `2` usage or subscribe refusal; `3` no
//! delivery within `--timeout-secs`; `4` the delivery arrived but the signed
//! ack response was not confirmed written.
//!
//! ## Ack ordering
//!
//! Returning `Ok` from the reverse handler only queues the acknowledgment:
//! the remote adapter signs the response and hands the write to its own task
//! after the handler returns. The example therefore wraps the WS transport in
//! a local observation decorator (the same seam the lane's own
//! `ObservedTransport` uses): it records the delivery request's
//! `request_id` and prints `ACK_SENT` only once the matching signed response
//! envelope's transport write has completed — the socket accepted the bytes.
//! Without that completion the run fails `exit=4` instead of claiming an
//! acknowledgment.
//!
//! ## Delivery note (v1.207 lane, current HEAD)
//!
//! The host pushes a batch only for frames retained in the stream's ring, and
//! the shipped host has no event *publisher* yet (v1.207 ships the lane and
//! the subscription surface), so a cursorless subscribe to a fresh stream
//! stays silent. Passing `--cursor <stale epoch>:<seq>` exercises the shipped
//! replay path: the host answers the subscribe and pushes the cursorless
//! `gap` reconciliation frame (`requires_transcript_reconciliation: true`) —
//! the RN-OGA-4 replay/gap entry — which this example acks. Once a publisher
//! lands, the same command receives data frames and prints the advanced
//! cursor.

use libp2p::identity::Keypair;
use nexus_core::connect::{ws_config, WsTransport, DEFAULT_MAX_ENVELOPE_BYTES};
use nexus_spoke_adapter::{HostCapabilityManifest, SpokeRejectCode, SpokeResult};
use serde_json::{json, Value};
use spoke_connect::core::derive_peer_id_from_ed25519_pubkey;
use spoke_connect::remote::{
    connect_remote_adapter, RemoteAdapter, RemoteAdapterOptions, RemoteIdentity, ToolHandler,
    Transport, TransportError,
};
use std::future::Future;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::TcpStream;

const SUBSCRIBE_TOOL: &str = "tools.nexus.subscribe";
const DELIVER_EVENTS_TOOL: &str = "tools.nexus.deliver_events";
/// The host registers its per-session `tools.nexus.subscribe` handler on the
/// tick after the handshake is established (~25 ms), so the first invoke can
/// arrive before registration and be refused `op_unsupported`. Retry bounded.
const SUBSCRIBE_ATTEMPTS: u32 = 20;
const SUBSCRIBE_RETRY_DELAY: Duration = Duration::from_millis(50);
/// Fixed Ed25519 seed for this consumer's identity — the deterministic
/// `peer_id` / pubkey the operator allowlists and pins (like `connect_dialer`).
const DEFAULT_SEED: u8 = 7;
const DEFAULT_STREAM: &str = "demo";
const DEFAULT_TIMEOUT_SECS: u64 = 15;

const USAGE: &str = "usage: connect_event_consumer <ADDR> \
[--host-pubkey <64-HEX> | --host-identity-key <PATH>] [--stream <NAME>] \
[--cursor <EPOCH>:<SEQ>] [--seed <N>] [--timeout-secs <N>] [--print-peer-only]";

/// The reverse-invoke handler future: the same boxed shape as
/// `spoke_connect::remote::ToolHandler` (kept local so the example needs no
/// `futures` dependency).
type HandlerFuture = std::pin::Pin<Box<dyn Future<Output = SpokeResult<Value>> + Send>>;

/// A failure with the process exit code it maps to.
struct Failure {
    code: i32,
    message: String,
}

impl Failure {
    fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

/// Example-local transport-observation seam — the same shape as the lane's
/// own `ObservedTransport`: it records the delivery request's `request_id`
/// and then signals only once the matching signed ack response has completed
/// its transport write. This is what distinguishes "the handler returned
/// `Ok`" from "the acknowledgement reached the wire".
#[derive(Default)]
struct AckObservation {
    delivery_request_id: Mutex<Option<String>>,
    ack_response_written: AtomicBool,
    written: tokio::sync::Notify,
}

impl AckObservation {
    /// The delivery request's `request_id`, if a delivery arrived.
    fn delivery_request_id(&self) -> Option<String> {
        self.delivery_request_id
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    fn record_delivery_request(&self, envelope: &[u8]) {
        let Ok(value) = serde_json::from_slice::<Value>(envelope) else {
            return;
        };
        if value.get("op").and_then(Value::as_str) != Some(DELIVER_EVENTS_TOOL) {
            return;
        }
        let Some(request_id) = value.get("request_id").and_then(Value::as_str) else {
            return;
        };
        let mut slot = self
            .delivery_request_id
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if slot.is_none() {
            *slot = Some(request_id.to_owned());
        }
    }

    fn record_ack_response(&self, envelope: &[u8]) {
        let Ok(value) = serde_json::from_slice::<Value>(envelope) else {
            return;
        };
        let Some(request_id) = value.get("request_id").and_then(Value::as_str) else {
            return;
        };
        if self.delivery_request_id().as_deref() != Some(request_id)
            || value.pointer("/payload/result").is_none()
        {
            return;
        }
        println!("ACK_RESPONSE_WRITTEN request_id={request_id}");
        self.ack_response_written.store(true, Ordering::SeqCst);
        self.written.notify_one();
    }
}

/// Transport decorator around the WS transport (see [`AckObservation`]).
struct AckObservedTransport {
    inner: Arc<dyn Transport>,
    observation: Arc<AckObservation>,
}

impl AckObservedTransport {
    fn new(inner: Arc<dyn Transport>, observation: Arc<AckObservation>) -> Arc<Self> {
        Arc::new(Self { inner, observation })
    }
}

#[async_trait::async_trait]
impl Transport for AckObservedTransport {
    async fn send(&self, envelope: &[u8]) -> Result<(), TransportError> {
        // Observe only AFTER the write resolved: `WsTransport::send` resolves
        // once the socket accepted the bytes.
        let result = self.inner.send(envelope).await;
        if result.is_ok() {
            self.observation.record_ack_response(envelope);
        }
        result
    }

    async fn recv(&self) -> Result<Vec<u8>, TransportError> {
        let envelope = self.inner.recv().await?;
        self.observation.record_delivery_request(&envelope);
        Ok(envelope)
    }

    async fn close(&self) -> Result<(), TransportError> {
        self.inner.close().await
    }
}

/// Wait (bounded) for the signed ack response's transport write to complete.
async fn wait_for_ack_write(observation: &AckObservation, timeout: Duration) -> Option<String> {
    if !observation.ack_response_written.load(Ordering::SeqCst) {
        tokio::time::timeout(timeout, observation.written.notified())
            .await
            .ok()?;
    }
    if observation.ack_response_written.load(Ordering::SeqCst) {
        observation.delivery_request_id()
    } else {
        None
    }
}

/// Parsed run arguments.
struct Args {
    /// Lane address, e.g. `ws://127.0.0.1:8425/connect`.
    addr: String,
    /// Event-lane trust anchor: the responder's Ed25519 public key, hex.
    host_pubkey: Option<String>,
    /// Event-lane trust anchor: path to the responder's raw 32-byte seed.
    host_identity_key: Option<PathBuf>,
    /// Event stream name to subscribe to.
    stream: String,
    /// Optional `last_event_id` cursor (`<epoch>:<seq>`) for replay/resume.
    cursor: Option<String>,
    /// Local Ed25519 identity seed.
    seed: u8,
    /// Print the consumer identity and exit (no lane contact).
    print_peer_only: bool,
    /// Delivery wait, in seconds.
    timeout_secs: u64,
}

impl Args {
    fn parse(mut argv: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut addr: Option<String> = None;
        let mut host_pubkey = None;
        let mut host_identity_key = None;
        let mut stream = DEFAULT_STREAM.to_owned();
        let mut cursor: Option<String> = None;
        let mut seed = DEFAULT_SEED;
        let mut print_peer_only = false;
        let mut timeout_secs = DEFAULT_TIMEOUT_SECS;
        while let Some(arg) = argv.next() {
            match arg.as_str() {
                "--addr" => addr = Some(argv.next().ok_or("--addr needs a value")?),
                "--host-pubkey" => {
                    host_pubkey = Some(argv.next().ok_or("--host-pubkey needs a value")?);
                }
                "--host-identity-key" => {
                    host_identity_key = Some(PathBuf::from(
                        argv.next().ok_or("--host-identity-key needs a value")?,
                    ));
                }
                "--stream" => stream = argv.next().ok_or("--stream needs a value")?,
                "--cursor" => cursor = Some(argv.next().ok_or("--cursor needs a value")?),
                "--seed" => {
                    seed = argv
                        .next()
                        .ok_or("--seed needs a value")?
                        .parse()
                        .map_err(|e| format!("--seed must be a u8: {e}"))?;
                }
                "--timeout-secs" => {
                    timeout_secs = argv
                        .next()
                        .ok_or("--timeout-secs needs a value")?
                        .parse()
                        .map_err(|e| format!("--timeout-secs must be a u64: {e}"))?;
                }
                "--print-peer-only" => print_peer_only = true,
                other if other.starts_with("--") => {
                    return Err(format!("unknown flag {other}\n{USAGE}"));
                }
                other => {
                    if addr.replace(other.to_owned()).is_some() {
                        return Err(format!("unexpected positional argument {other}\n{USAGE}"));
                    }
                }
            }
        }
        let addr = addr.unwrap_or_default();
        if !print_peer_only && addr.is_empty() {
            return Err(format!("lane address is required\n{USAGE}"));
        }
        Ok(Self {
            addr,
            host_pubkey,
            host_identity_key,
            stream,
            cursor,
            seed,
            print_peer_only,
            timeout_secs,
        })
    }

    const fn timeout(&self) -> Duration {
        Duration::from_secs(self.timeout_secs)
    }
}

#[tokio::main]
async fn main() {
    if std::env::args().any(|arg| arg == "--help" || arg == "-h") {
        println!("{USAGE}");
        return;
    }
    let args = match Args::parse(std::env::args().skip(1)) {
        Ok(args) => args,
        Err(message) => {
            eprintln!("connect_event_consumer: {message}");
            std::process::exit(2);
        }
    };

    let pubkey = pubkey_from_seed(&[args.seed; 32]);
    let consumer_peer_id = derive_peer_id_from_ed25519_pubkey(&pubkey);
    println!("consumer_peer_id: {consumer_peer_id}");
    println!("consumer_pubkey_hex: {}", hex_lower(&pubkey));
    if args.print_peer_only {
        return;
    }

    if let Err(failure) = run(args).await {
        eprintln!("connect_event_consumer: {}", failure.message);
        std::process::exit(failure.code);
    }
}

/// One subscribe → reverse `deliver_events` → ack round-trip.
async fn run(args: Args) -> Result<(), Failure> {
    let observation = Arc::new(AckObservation::default());
    let adapter = connect_lane(&args, Arc::clone(&observation)).await?;

    let (tx, mut deliveries) = tokio::sync::mpsc::channel::<Value>(4);
    let handler: ToolHandler = Arc::new(move |arguments: Value| -> HandlerFuture {
        let tx = tx.clone();
        Box::pin(async move {
            // Answering `Ok` IS the cursor ack the host waits for before it
            // sends the next batch (v1.207 ack-gated delivery).
            let _ = tx.send(arguments).await;
            SpokeResult::Ok(json!({}))
        })
    });
    adapter.register_tool_handler(DELIVER_EVENTS_TOOL, handler);

    let mut subscribe_args = json!({ "stream": args.stream });
    if let Some(cursor) = &args.cursor {
        subscribe_args["last_event_id"] = json!(cursor);
    }
    let mut attempt = 0_u32;
    let response = loop {
        attempt += 1;
        match adapter
            .invoke_tool(SUBSCRIBE_TOOL, subscribe_args.clone())
            .await
        {
            SpokeResult::Ok(response) => break response,
            SpokeResult::Reject(reject)
                if reject.code == SpokeRejectCode::CapabilityPortMissing
                    && attempt < SUBSCRIBE_ATTEMPTS =>
            {
                // The host registers the per-session subscribe handler on its
                // established-poll tick after the handshake, so the first
                // invoke can land before registration.
                tokio::time::sleep(SUBSCRIBE_RETRY_DELAY).await;
            }
            SpokeResult::Reject(reject) => {
                adapter.close();
                return Err(Failure::new(
                    2,
                    format!(
                        "subscribe refused: code={} message={} details={}",
                        reject.code,
                        reject.message,
                        reject.details.map_or_else(
                            || "null".to_owned(),
                            |details| Value::Object(details).to_string()
                        )
                    ),
                ));
            }
        }
    };
    println!("SUBSCRIBE_OK attempts={attempt} response={response}");

    let delivery = tokio::time::timeout(args.timeout(), deliveries.recv()).await;
    match delivery {
        Ok(Some(delivery)) => report_delivery(&delivery),
        Ok(None) => {
            adapter.close();
            return Err(Failure::new(
                3,
                "delivery channel closed before any batch arrived".to_owned(),
            ));
        }
        Err(_) => {
            adapter.close();
            return Err(Failure::new(
                3,
                format!(
                    "no {DELIVER_EVENTS_TOOL} push within {:?} — the host has no publisher for stream {:?}",
                    args.timeout(),
                    args.stream
                ),
            ));
        }
    }

    // The handler's `Ok` only queues the acknowledgment; the adapter signs
    // the response and writes it in its own task afterwards. Do not claim (or
    // exit on) acknowledgment until that write completed.
    if let Some(request_id) = wait_for_ack_write(&observation, args.timeout()).await {
        println!("ACK_SENT request_id={request_id} (signed ack response written to the lane)");
    } else {
        adapter.close();
        return Err(Failure::new(
            4,
            format!(
                "delivery received but the signed ack response for request {:?} was not \
                 confirmed written within {:?}",
                observation.delivery_request_id(),
                args.timeout()
            ),
        ));
    }

    adapter.close();
    Ok(())
}

/// Resolve the lane trust anchor, upgrade the address to a WebSocket, and
/// finish the signed-hello handshake.
async fn connect_lane(
    args: &Args,
    observation: Arc<AckObservation>,
) -> Result<Arc<RemoteAdapter>, Failure> {
    let host_pubkey = resolve_host_pubkey(args)?;
    println!(
        "host_lane_peer_id: {}",
        derive_peer_id_from_ed25519_pubkey(&host_pubkey)
    );

    let url = url::Url::parse(&args.addr)
        .map_err(|e| Failure::new(2, format!("invalid lane address {:?}: {e}", args.addr)))?;
    let host = url
        .host_str()
        .ok_or_else(|| Failure::new(2, format!("lane address {:?} has no host", args.addr)))?;
    let port = url
        .port()
        .ok_or_else(|| Failure::new(2, format!("lane address {:?} has no port", args.addr)))?;

    let tcp = TcpStream::connect((host, port))
        .await
        .map_err(|e| Failure::new(1, format!("TCP connect to {host}:{port} failed: {e}")))?;
    let (ws, _response) = tokio_tungstenite::client_async_with_config(
        args.addr.clone(),
        tcp,
        Some(ws_config(DEFAULT_MAX_ENVELOPE_BYTES)),
    )
    .await
    .map_err(|e| Failure::new(1, format!("WebSocket upgrade to {} failed: {e}", args.addr)))?;

    let raw: Arc<dyn Transport> = Arc::new(WsTransport::new(ws));
    let transport: Arc<dyn Transport> = AckObservedTransport::new(raw, observation);
    connect_remote_adapter(RemoteAdapterOptions {
        transport,
        local_identity: RemoteIdentity {
            seed: [args.seed; 32],
        },
        local_manifest: consumer_manifest(),
        remote_pubkey: host_pubkey,
        allowlist: vec![derive_peer_id_from_ed25519_pubkey(&host_pubkey)],
        invoke_timeout_ms: Some(args.timeout_secs.saturating_mul(1000)),
        capability_token: None,
    })
    .await
    .map_err(|e| Failure::new(1, format!("Connect handshake failed: {e}")))
}

/// The event-lane Ed25519 public key: the explicit `--host-pubkey`, else the
/// public key derived from the lane identity seed file.
fn resolve_host_pubkey(args: &Args) -> Result<[u8; 32], Failure> {
    if let Some(text) = &args.host_pubkey {
        return hex32(text).ok_or_else(|| {
            Failure::new(
                2,
                format!("--host-pubkey must be 64 hex characters, got {text:?}"),
            )
        });
    }
    let path = match &args.host_identity_key {
        Some(path) => path.clone(),
        None => default_host_identity_key().ok_or_else(|| {
            Failure::new(
                2,
                "no host trust anchor: pass --host-pubkey <64-HEX> or --host-identity-key <PATH>"
                    .to_owned(),
            )
        })?,
    };
    let seed = std::fs::read(&path).map_err(|e| {
        Failure::new(
            2,
            format!(
                "cannot read the event lane identity seed {}: {e} — pass --host-pubkey <64-HEX> \
                 instead",
                path.display()
            ),
        )
    })?;
    if seed.len() != 32 {
        return Err(Failure::new(
            2,
            format!(
                "event lane identity seed {} must be 32 bytes, got {}",
                path.display(),
                seed.len()
            ),
        ));
    }
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(&seed);
    Ok(pubkey_from_seed(&bytes))
}

/// The lane's identity seed at its default location: `$NEXUS42_HOME` else
/// `$HOME`, under `~/.nexus42/connect/daemon_identity.key`.
fn default_host_identity_key() -> Option<PathBuf> {
    let base = std::env::var_os("NEXUS42_HOME").or_else(|| std::env::var_os("HOME"))?;
    Some(PathBuf::from(base).join(".nexus42/connect/daemon_identity.key"))
}

/// The Ed25519 public key for a raw 32-byte secret seed (the same derivation
/// spoke's `RemoteIdentity` uses).
fn pubkey_from_seed(seed: &[u8; 32]) -> [u8; 32] {
    Keypair::ed25519_from_bytes(*seed)
        .expect("a 32-byte seed is a valid ed25519 secret")
        .public()
        .try_into_ed25519()
        .expect("an ed25519 keypair exposes an ed25519 public key")
        .to_bytes()
}

/// Decode exactly 64 lowercase/uppercase hex characters into 32 bytes.
fn hex32(text: &str) -> Option<[u8; 32]> {
    let mut out = [0u8; 32];
    let bytes = text.as_bytes();
    if bytes.len() != 64 {
        return None;
    }
    for (slot, pair) in out.iter_mut().zip(bytes.as_chunks::<2>().0) {
        let high = char::from(pair[0]).to_digit(16)?;
        let low = char::from(pair[1]).to_digit(16)?;
        *slot = u8::try_from(high * 16 + low).ok()?;
    }
    Some(out)
}

/// Print one delivered batch. The acknowledgment outcome is printed by `run`
/// only after the signed response write completes.
fn report_delivery(delivery: &Value) {
    let Some(frames) = delivery.get("frames").and_then(Value::as_array) else {
        println!("DELIVERED_WITHOUT_FRAMES {delivery}");
        return;
    };
    let stream = delivery
        .get("stream")
        .and_then(Value::as_str)
        .unwrap_or("?");
    println!("DELIVERED stream={stream} frames={}", frames.len());
    let mut last_cursor: Option<&str> = None;
    for frame in frames {
        let event = frame.get("event").and_then(Value::as_str).unwrap_or("?");
        let data = frame
            .get("data")
            .map_or_else(|| "null".to_owned(), Value::to_string);
        match frame.get("id").and_then(Value::as_str) {
            Some(id) => {
                println!("FRAME id={id} event={event} data={data}");
                last_cursor = Some(id);
            }
            None => println!("FRAME id=<control> event={event} data={data}"),
        }
    }
    match last_cursor {
        Some(cursor) => println!("CURSOR_ADVANCED {cursor}"),
        None => {
            println!("CURSOR_UNCHANGED (control frame: the gap/reconcile push carries no cursor)");
        }
    }
}

/// The consumer hello manifest: it opts the session into the host-served
/// `tools.nexus.subscribe` (negotiation = capability intersection) and
/// advertises the consumer-served reverse tool `tools.nexus.deliver_events`,
/// which the host pushes event batches through.
fn consumer_manifest() -> HostCapabilityManifest {
    let object = json!({ "type": "object" });
    let mut capabilities = vec!["spoke-baseline".to_owned()];
    let mut descriptors = Vec::new();
    for id in [SUBSCRIBE_TOOL, DELIVER_EVENTS_TOOL] {
        capabilities.push(id.to_owned());
        descriptors.push(json!({
            "schema_version": 1,
            "capability_id": id,
            "op": id,
            "description": format!("Connect event-lane consumer {id}"),
            "input": object,
            "output": object,
        }));
    }
    serde_json::from_value(json!({
        "schema_version": 1,
        "host_id": "connect-event-consumer",
        "roles": ["input-source"],
        "capabilities": capabilities,
        "namespaces": ["nexus"],
        "extensions": {},
        "tools": descriptors,
    }))
    .expect("static consumer manifest is valid")
}

/// Lowercase hex — `peer_keys.json` pins the 32-byte Ed25519 public key in
/// that form.
fn hex_lower(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(out, "{byte:02x}").expect("writing to a String cannot fail");
    }
    out
}
