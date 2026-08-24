//! V1.174 P0 T1 (AR-57) — WS `Transport` conformance suite.
//!
//! The conformance bar is parametrized over a transport factory and MUST run
//! green against BOTH the in-repo `loopback_transport_pair()` and the WS pair
//! (client-dial + server-accept over a real `TcpListener` on `127.0.0.1:0`):
//!
//! - envelope integrity: payloads containing newlines / NUL / invalid-UTF-8
//!   bytes round-trip byte-identical (proves message delimiting, not
//!   line-framing);
//! - order preservation both directions;
//! - close semantics: pending `recv` fails fast after peer close; `close()`
//!   idempotent; buffered messages may be lost on close (loopback parity);
//! - envelope cap: >2 MiB inbound message ⇒ `TransportError::Io` (fail-closed,
//!   AR-57 #3);
//! - handshake + one tool-invoke round trip over each transport (golden path
//!   parity with spoke's `remote_loopback` test family semantics).
//!
//! The module compiles to nothing without `--features connect-client` — the
//! default-feature build stays tungstenite-free (AR-61).

#![cfg(feature = "connect-client")]
#![allow(clippy::unwrap_used)]

use std::sync::Arc;

use ed25519_dalek::SigningKey;
use futures_util::future::BoxFuture;
use futures_util::{SinkExt, StreamExt};
use parking_lot::Mutex;
use spoke_connect::core::derive_peer_id_from_ed25519_pubkey;
use spoke_connect::remote::{
    connect_remote_adapter, connect_responder, loopback_transport_pair, ConnectResponderOptions,
    RemoteAdapterOptions, RemoteAdapterState, RemoteIdentity, ToolHandler, Transport,
    TransportError,
};
use spoke_operations::{spoke_ok, SpokeResult};
use spoke_schemas::HostCapabilityManifest;
use std::collections::HashMap;
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::protocol::Message;

use nexus_daemon_runtime::connect::{WsTransport, DEFAULT_MAX_ENVELOPE_BYTES};

// ── Fixtures ──────────────────────────────────────────────────────────────

/// Fixed test seeds (mirror of spoke-connect's loopback oracle).
const fn seed_client() -> [u8; 32] {
    [0x10; 32]
}

const fn seed_host() -> [u8; 32] {
    [0xa0; 32]
}

fn pubkey(seed: [u8; 32]) -> [u8; 32] {
    SigningKey::from_bytes(&seed).verifying_key().to_bytes()
}

/// Tool-carrying manifest: every tool capability ∈ capabilities[] so the
/// negotiated set includes the `tools.*` ops (spoke dispatch gate).
fn tool_manifest(host_id: &str) -> HostCapabilityManifest {
    serde_json::from_value(serde_json::json!({
        "schema_version": 1,
        "host_id": host_id,
        "roles": ["data-store"],
        "capabilities": ["spoke-baseline", "tools.math.add", "tools.echo.echo"],
        "namespaces": ["math", "echo", "toy_world"],
        "extensions": {},
        "tools": [
            {
                "schema_version": 1,
                "capability_id": "tools.math.add",
                "op": "tools.math.add",
                "description": "Add two integers",
                "input": { "type": "object" },
                "output": { "type": "object" },
            },
            {
                "schema_version": 1,
                "capability_id": "tools.echo.echo",
                "op": "tools.echo.echo",
                "description": "Echo the arguments",
                "input": { "type": "object" },
                "output": { "type": "object" },
            },
        ],
    }))
    .expect("valid tool manifest")
}

/// Records the arguments and returns `{ "sum": a + b }`.
fn add_handler(calls: Arc<Mutex<Vec<serde_json::Value>>>) -> ToolHandler {
    Arc::new(
        move |args: serde_json::Value| -> BoxFuture<'static, SpokeResult<serde_json::Value>> {
            let calls = Arc::clone(&calls);
            Box::pin(async move {
                calls.lock().push(args.clone());
                let a = args
                    .get("a")
                    .and_then(serde_json::Value::as_i64)
                    .unwrap_or(0);
                let b = args
                    .get("b")
                    .and_then(serde_json::Value::as_i64)
                    .unwrap_or(0);
                spoke_ok(serde_json::json!({ "sum": a + b }))
            })
        },
    )
}

// ── Transport factories ───────────────────────────────────────────────────

/// Which transport family a conformance vector runs against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TransportKind {
    Loopback,
    Ws,
}

impl TransportKind {
    const ALL: [Self; 2] = [Self::Loopback, Self::Ws];

    const fn label(self) -> &'static str {
        match self {
            Self::Loopback => "loopback",
            Self::Ws => "ws",
        }
    }
}

/// Build a (client, server) transport pair.
async fn transport_pair(kind: TransportKind) -> (Arc<dyn Transport>, Arc<dyn Transport>) {
    match kind {
        TransportKind::Loopback => {
            let pair = loopback_transport_pair();
            (Arc::new(pair.client), Arc::new(pair.server))
        }
        TransportKind::Ws => ws_pair().await,
    }
}

/// WS pair over a real `TcpListener` on `127.0.0.1:0` — the server accepts
/// the client's WebSocket upgrade, both ends get a `WsTransport` with the
/// 2 MiB inbound cap (AR-57 #3).
async fn ws_pair() -> (Arc<dyn Transport>, Arc<dyn Transport>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let config = nexus_daemon_runtime::connect::ws_config(DEFAULT_MAX_ENVELOPE_BYTES);
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let ws = tokio_tungstenite::accept_async_with_config(stream, Some(config))
            .await
            .unwrap();
        Arc::new(WsTransport::new(ws)) as Arc<dyn Transport>
    });
    let url = format!("ws://127.0.0.1:{}/connect", addr.port());
    let (client_ws, _resp) = tokio_tungstenite::client_async_with_config(
        url,
        tokio::net::TcpStream::connect(addr).await.unwrap(),
        Some(config),
    )
    .await
    .unwrap();
    let client = Arc::new(WsTransport::new(client_ws)) as Arc<dyn Transport>;
    (client, server.await.unwrap())
}

// ── Conformance vectors ───────────────────────────────────────────────────

/// Envelope integrity: newline / NUL / invalid-UTF-8 payloads round-trip
/// byte-identical, BOTH directions.
#[tokio::test]
async fn conformance_roundtrip_integrity() {
    for kind in TransportKind::ALL {
        let (client, server) = transport_pair(kind).await;
        let vectors: Vec<Vec<u8>> = vec![
            b"plain-ascii".to_vec(),
            b"line1\nline2\r\nline3".to_vec(),
            b"nul\x00inside\x00".to_vec(),
            vec![0xff, 0xfe, 0x00, 0x01], // invalid UTF-8
            (0..=255u8).collect(),        // full byte range
        ];
        // Client → server.
        for payload in &vectors {
            client.send(payload).await.unwrap();
            assert_eq!(
                server.recv().await.unwrap(),
                *payload,
                "[{}] client→server envelope must round-trip byte-identical",
                kind.label()
            );
        }
        // Server → client.
        for payload in &vectors {
            server.send(payload).await.unwrap();
            assert_eq!(
                client.recv().await.unwrap(),
                *payload,
                "[{}] server→client envelope must round-trip byte-identical",
                kind.label()
            );
        }
        client.close().await.unwrap();
        server.close().await.unwrap();
    }
}

/// Order preservation, both directions, interleaved.
#[tokio::test]
async fn conformance_order_preservation() {
    for kind in TransportKind::ALL {
        let (client, server) = transport_pair(kind).await;
        let n: u8 = 32;
        for i in 0..n {
            client.send(&[i]).await.unwrap();
            server.send(&[0xff - i]).await.unwrap();
        }
        for i in 0..n {
            assert_eq!(
                server.recv().await.unwrap(),
                vec![i],
                "[{}] client→server order broken at {i}",
                kind.label()
            );
            assert_eq!(
                client.recv().await.unwrap(),
                vec![0xff - i],
                "[{}] server→client order broken at {i}",
                kind.label()
            );
        }
        client.close().await.unwrap();
        server.close().await.unwrap();
    }
}

/// Close semantics: pending `recv` fails fast after peer close; `close()` is
/// idempotent; buffered messages may be lost on close (loopback parity).
#[tokio::test]
async fn conformance_close_semantics() {
    for kind in TransportKind::ALL {
        let (client, server) = transport_pair(kind).await;

        // Pending recv fails fast once the peer closes.
        let recv_task = tokio::spawn({
            let client = Arc::clone(&client);
            async move { client.recv().await }
        });
        // Let the recv park.
        tokio::task::yield_now().await;
        server.close().await.unwrap();
        let result = tokio::time::timeout(std::time::Duration::from_secs(5), recv_task)
            .await
            .expect("pending recv must resolve after peer close")
            .expect("recv task must not panic");
        assert_eq!(
            result,
            Err(TransportError::Closed),
            "[{}] pending recv must fail with Closed after peer close",
            kind.label()
        );

        // Recv after close fails fast (buffered messages lost on close).
        assert_eq!(
            client.recv().await,
            Err(TransportError::Closed),
            "[{}] recv after close must fail fast (Closed)",
            kind.label()
        );

        // Idempotent close.
        client.close().await.unwrap();
        client.close().await.unwrap();
        server.close().await.unwrap();
        server.close().await.unwrap();
    }
}

/// Envelope cap (AR-57 #3): an inbound message larger than the 2 MiB cap
/// ⇒ `TransportError::Io` ⇒ session unusable (fail-closed). The cap is a WS
/// framing property (tungstenite `max_message_size` on the read side) — the
/// loopback pair is unbounded, so this vector runs against the WS pair only.
#[tokio::test]
async fn conformance_envelope_cap_exceeds_2mib() {
    let (client, server) = ws_pair().await;
    let oversized = vec![0xab; DEFAULT_MAX_ENVELOPE_BYTES + 1];
    // The sender's write succeeds (write side is unbounded); run it
    // concurrently so the server can drain while the payload is in flight.
    let send_task = tokio::spawn({
        let client = Arc::clone(&client);
        async move { client.send(&oversized).await }
    });
    let result = server.recv().await;
    // The write side is unbounded, so the send may complete — or fail with a
    // transport error if the receiver's fail-closed teardown resets the
    // connection mid-write. The cap is enforced on the receiver only.
    match send_task.await.unwrap() {
        Ok(()) | Err(TransportError::Io(_)) => {}
        Err(other) => panic!("sender must complete or fail with Io, got {other:?}"),
    }
    assert!(
        matches!(result, Err(TransportError::Io(_))),
        "inbound message over the 2 MiB cap must fail with Io (fail-closed), got {result:?}"
    );
    // The session is closed: no further traffic can flow.
    let result = server.send(&[0u8; 16]).await;
    assert!(
        matches!(result, Err(TransportError::Io(_) | TransportError::Closed)),
        "session must be unusable after the cap rejection, got {result:?}"
    );
    client.close().await.unwrap();
    server.close().await.unwrap();
}

/// Non-Binary inbound frame (AR-57 #3, fail-closed): a Text message must be
/// rejected with `TransportError::Io` and the session torn down. Requires a
/// raw tungstenite sink (the `Transport::send` seam only emits Binary).
#[tokio::test]
async fn conformance_non_binary_rejected() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let config = nexus_daemon_runtime::connect::ws_config(DEFAULT_MAX_ENVELOPE_BYTES);
    let server_task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let ws = tokio_tungstenite::accept_async_with_config(stream, Some(config))
            .await
            .unwrap();
        Arc::new(WsTransport::new(ws)) as Arc<dyn Transport>
    });
    let url = format!("ws://127.0.0.1:{}/connect", addr.port());
    let (client_ws, _resp) = tokio_tungstenite::client_async_with_config(
        url,
        tokio::net::TcpStream::connect(addr).await.unwrap(),
        Some(config),
    )
    .await
    .unwrap();
    let (mut sink, _stream) = client_ws.split();
    let server = server_task.await.unwrap();

    // Send a Text frame through the raw sink: the receiver must fail-closed.
    sink.send(Message::Text("not-an-envelope".into()))
        .await
        .unwrap();
    let result = server.recv().await;
    assert!(
        matches!(result, Err(TransportError::Io(_))),
        "non-Binary inbound message must fail with Io (fail-closed), got {result:?}"
    );
    // Session unusable afterwards.
    let result = server.send(&[0u8; 8]).await;
    assert!(
        matches!(result, Err(TransportError::Io(_) | TransportError::Closed)),
        "session must be unusable after non-Binary rejection, got {result:?}"
    );
    server.close().await.unwrap();
}

/// Golden path: full spoke handshake + one tool round trip over each
/// transport (parity with spoke's `remote_loopback` semantics).
#[tokio::test]
async fn conformance_golden_path_handshake_and_tool_invoke() {
    for kind in TransportKind::ALL {
        let (client, server) = transport_pair(kind).await;

        let peer_id_client = derive_peer_id_from_ed25519_pubkey(&pubkey(seed_client()));
        let responder = connect_responder(ConnectResponderOptions {
            transport: server,
            identity: RemoteIdentity { seed: seed_host() },
            manifest: tool_manifest("test-responder"),
            allowlist: vec![peer_id_client.clone()],
            peer_keys: HashMap::from([(peer_id_client, pubkey(seed_client()))]),
            ports: None,
            invoke_timeout_ms: None,
        })
        .await;
        let calls = Arc::new(Mutex::new(Vec::new()));
        responder.register_tool_handler("tools.math.add", add_handler(Arc::clone(&calls)));

        let peer_id_host = derive_peer_id_from_ed25519_pubkey(&pubkey(seed_host()));
        let adapter = connect_remote_adapter(RemoteAdapterOptions {
            transport: client,
            local_identity: RemoteIdentity {
                seed: seed_client(),
            },
            local_manifest: tool_manifest("test-client"),
            remote_pubkey: pubkey(seed_host()),
            allowlist: vec![peer_id_host],
            invoke_timeout_ms: None,
            capability_token: None,
        })
        .await
        .unwrap();

        // Handshake completed on both ends.
        assert_eq!(adapter.state(), RemoteAdapterState::Established);
        assert_eq!(responder.state(), RemoteAdapterState::Established);
        assert_eq!(
            responder
                .remote_manifest()
                .expect("peer manifest")
                .host_id
                .to_string(),
            "test-client"
        );

        // One tool-invoke round trip: adapter (dialer) → responder (serving).
        let result = adapter
            .invoke_tool("tools.math.add", serde_json::json!({ "a": 21, "b": 21 }))
            .await;
        assert_eq!(result, SpokeResult::Ok(serde_json::json!({ "sum": 42 })));
        assert_eq!(calls.lock().len(), 1);

        adapter.close();
        responder.close();
    }
}
