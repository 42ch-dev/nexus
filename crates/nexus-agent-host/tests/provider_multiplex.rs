//! Real maintained adapters and a Node ProviderCallbacks peer; no model calls.
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use nexus_agent_host::capability::model::{
    HostContentBlock, HostOperation, HostStartConfig, LaunchSpec, ProbeRequest,
    PromptPermissionScope, ProtocolKind, SessionOwner,
};
use nexus_agent_host::config::{AgentHostConfig, ProviderConfig, TimeoutConfig};
use nexus_agent_host::providers::multiplex::compose_provider_port;
use nexus_agent_host::providers::adapter_from_catalog_entry;
use nexus_agent_host::{HostFacade, HostManager, HostOperationId, HostPermissionResolver, ProviderCatalog};
use nexus_contracts::provider_call::ProviderCallMethod;
use nexus_contracts::{CoreError, CoreErrorCode, ProviderCall, ProviderEventBatch, ProviderReply};
use nexus_provider_ports::{ProviderPort, ProviderResult};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;

struct CallbackPeer {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
}

struct CallbackPort(Mutex<CallbackPeer>);

impl CallbackPort {
    fn spawn() -> Self {
        let module = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../packages/nexus-provider-acp/dist/index.js");
        // This peer is a test transport for the actual callback, not an ACP
        // implementation. Build the package in the serialized validation window.
        let script = format!(r#"
import {{ createAcpProvider }} from {};
import {{ createInterface }} from 'node:readline';
const provider = createAcpProvider();
for await (const line of createInterface({{ input: process.stdin }})) {{
  const input = JSON.parse(line);
  try {{
    const value = input.call ? await provider.call(input.call)
      : await provider.next(input.operation_id, input.max_events, input.max_bytes);
    process.stdout.write(JSON.stringify({{ value }}) + '\n');
  }} catch (error) {{
    process.stdout.write(JSON.stringify({{ error: String(error) }}) + '\n');
  }}
}}
"#, serde_json::to_string(&module.to_string_lossy()).expect("module URL"));
        let mut child = Command::new("node").args(["--input-type=module", "-e", &script])
            .stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit()).kill_on_drop(true).spawn().expect("Node callback peer");
        let stdin = child.stdin.take().expect("stdin");
        let stdout = BufReader::new(child.stdout.take().expect("stdout"));
        Self(Mutex::new(CallbackPeer { child, stdin: Some(stdin), stdout }))
    }

    async fn exchange(&self, value: Value) -> ProviderResult<Value> {
        let mut peer = self.0.lock().await;
        let mut bytes = serde_json::to_vec(&value).expect("peer request");
        bytes.push(b'\n');
        peer.stdin.as_mut().expect("open peer stdin").write_all(&bytes).await.expect("peer write");
        let mut line = String::new();
        let count = tokio::time::timeout(Duration::from_secs(30), peer.stdout.read_line(&mut line))
            .await.expect("bounded callback response").expect("peer read");
        assert_ne!(count, 0, "callback peer must not exit before replying");
        let response: Value = serde_json::from_str(&line).expect("callback response");
        if let Some(error) = response.get("error") {
            return Err(CoreError { code: CoreErrorCode::Internal, message: error.to_string(),
                details: Default::default(), http_status: Some(500) });
        }
        Ok(response["value"].clone())
    }

    async fn close(&self) {
        let mut peer = self.0.lock().await;
        // Drop the pipe writer to deliver EOF; shutdown alone retains the handle.
        drop(peer.stdin.take());
        let status = tokio::time::timeout(Duration::from_secs(5), peer.child.wait())
            .await.expect("callback peer exits after sessions close").expect("peer exit");
        assert!(status.success());
    }
}

#[async_trait]
impl ProviderPort for CallbackPort {
    async fn call(&self, request: ProviderCall) -> ProviderResult<ProviderReply> {
        Ok(serde_json::from_value(self.exchange(json!({"call": request})).await?).expect("reply DTO"))
    }
    async fn next(&self, operation_id: String, max_events: u32, max_bytes: u32) -> ProviderResult<ProviderEventBatch> {
        Ok(serde_json::from_value(self.exchange(json!({"operation_id": operation_id,
            "max_events": max_events, "max_bytes": max_bytes})).await?).expect("batch DTO"))
    }
}

fn owner(cwd: &Path) -> SessionOwner {
    SessionOwner { creator_id: "ctr_multiplex".into(), workspace_root: cwd.to_path_buf(), orchestration_run_id: None }
}

fn shim(root: &Path, name: &str, fixture: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let python = std::process::Command::new("python3").args(["-c", "import sys; print(sys.executable)"])
        .output().expect("Python interpreter");
    assert!(python.status.success());
    let python = String::from_utf8(python.stdout).expect("interpreter path");
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(fixture);
    let path = root.join(name);
    std::fs::write(&path, format!("#!/bin/sh\nexec '{}' '{}' \"$@\"\n", python.trim(), fixture.display())).expect("fixture shim");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("executable shim");
    path
}

async fn host(root: &Path) -> Arc<HostManager> {
    let families = [
        ("claude-native", "native_cli", "native_protocol/mock_claude_cli.py"),
        ("codex-native", "native_cli", "native_protocol/mock_codex_app_server.py"),
        ("dsh-native", "native_cli", "native_protocol/mock_dsh_agent.py"),
        ("configured-acp", "acp", "mock_acp_workflow.py"),
    ];
    let config = AgentHostConfig {
        max_sessions: 8,
        providers: families.iter().map(|(id, protocol, fixture)| ProviderConfig {
            id: (*id).into(), protocol: (*protocol).into(), enabled: true,
            command: Some(shim(root, id, fixture).to_string_lossy().into_owned()), args: vec![],
            env: HashMap::from([
                ("REQ_LOG".into(), root.join(format!("{id}.jsonl")).to_string_lossy().into_owned()),
                ("ACP_FIXTURE_LOG".into(), root.join("acp.jsonl").to_string_lossy().into_owned()),
                ("DSH_HOME".into(), root.join("dsh-home").to_string_lossy().into_owned()),
                ("SCENARIO".into(), "two_messages".into()),
            ]),
        }).collect(),
        ..AgentHostConfig::default()
    };
    let catalog = ProviderCatalog::build_from_sources(&config, vec![], vec![]).expect("catalog");
    let host = Arc::new(HostManager::new());
    // Explicit catalog only: never discover or call an installed real provider.
    for entry in &catalog.entries {
        let adapter = adapter_from_catalog_entry(entry, TimeoutConfig::default(),
            HostPermissionResolver::new_native_only(&config.policy), host.localset_bridge()).expect("factory");
        host.register_provider(adapter, entry.launch.clone()).await;
    }
    host.start(HostStartConfig {
        config_path: root.join("absent.toml"), workspace_root: root.to_path_buf(),
        max_sessions: 8, max_ops_per_session: 1, timeouts: TimeoutConfig::default(),
        host_config: Some(config), admitted_catalog: Some(catalog.entries), probe_owner: Some(owner(root)),
    }).await.expect("start host");
    host
}

fn call(method: ProviderCallMethod, session: Option<&str>, operation: Option<&str>, payload: Value) -> ProviderCall {
    ProviderCall { method, request_id: uuid::Uuid::new_v4().to_string(), session_id: session.map(str::to_string),
        operation_id: operation.map(str::to_string), deadline_ms: 30_000,
        payload: payload.as_object().expect("object payload").clone() }
}

async fn launch(port: &dyn ProviderPort, root: &Path, provider: &str) -> String {
    let mut payload = serde_json::to_value(LaunchSpec {
        cwd: root.to_path_buf(), model: None, mode: None, mcp_servers: vec![], owner: owner(root),
    }).expect("launch spec");
    payload["provider_id"] = provider.into();
    let reply = port.call(call(ProviderCallMethod::Launch, None, None, payload)).await.expect("launch");
    assert!(reply.ok, "{reply:?}");
    reply.session_id.expect("session ID")
}

async fn drain(port: &dyn ProviderPort, operation: &str) -> Vec<String> {
    let mut messages = Vec::new();
    let mut finished = false;
    for _ in 0..200 {
        let batch = tokio::time::timeout(Duration::from_secs(30), port.next(operation.into(), 16, 256 * 1024))
            .await.expect("bounded pull").expect("events");
        assert!(batch.gap.is_none(), "{batch:?}");
        assert!(serde_json::to_vec(&batch.events).expect("events serialize").len() <= 256 * 1024);
        for event in batch.events {
            let value = serde_json::to_value(event).expect("event JSON");
            if let Some(text) = value["MessageDelta"]["text"].as_str() { messages.push(text.to_string()); }
            assert!(value.get("OpFailed").is_none(), "{value}");
            if value.get("OpFinished").is_some() { finished = true; }
        }
        if !batch.has_more { break; }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(finished, "must observe a real terminal");
    messages
}

#[tokio::test]
async fn session_keeps_selected_provider_and_truthful_cancel() {
    let tmp = tempfile::tempdir().expect("workspace");
    let root = tmp.path().canonicalize().expect("canonical workspace");
    let host = host(&root).await;
    let callback = Arc::new(CallbackPort::spawn());
    let port = compose_provider_port(host.clone(), Arc::new(host.build_provider_port().await), Some(callback.clone()));
    let catalog = host.provider_catalog().await.expect("catalog");
    assert!(!catalog.entries.iter().find(|entry| entry.provider_id.0 == "dsh-native").expect("DSH").capabilities.cancellation);
    for (provider, expected) in [
        ("claude-native", vec!["hello from mock claude"]),
        ("codex-native", vec!["hello from mock codex"]),
        ("dsh-native", vec!["A", "B"]),
        ("configured-acp", vec!["transformed:hello"]),
    ] {
        let session = launch(port.as_ref(), &root, provider).await;
        let mut op = HostOperation::Prompt {
            op_id: HostOperationId::new(), content: vec![HostContentBlock::Text { text: "hello".into() }],
            permission_scope: Some(PromptPermissionScope { allow_read: false, allow_write: false, allow_destructive: false }),
        };
        if matches!(provider, "claude-native" | "codex-native") {
            // These native adapters cannot enforce workflow scopes. Refuse the
            // constrained request before testing their ordinary prompt path.
            let error = port.call(call(ProviderCallMethod::Execute, Some(&session), None, serde_json::to_value(&op).expect("scoped operation")))
                .await.expect_err("native workflow scope must remain unsupported");
            assert_eq!(error.code, CoreErrorCode::NotSupported, "{provider}: {error:?}");
            if let HostOperation::Prompt { permission_scope, .. } = &mut op {
                *permission_scope = None;
            }
        }
        let reply = port.call(call(ProviderCallMethod::Execute, Some(&session), None, serde_json::to_value(op).expect("operation")))
            .await.expect("execute");
        assert!(reply.ok, "{provider}: {reply:?}");
        let op_id = reply.operation_id.expect("operation ID");
        if provider == "dsh-native" {
            let cancel = port.call(call(ProviderCallMethod::Cancel, Some(&session), Some(&op_id), json!({"provider_id": "configured-acp"})))
                .await.expect_err("DSH cannot acknowledge cancellation");
            assert_eq!(cancel.code, CoreErrorCode::NotSupported);
            let sessions = host.list_sessions().await.expect("sessions");
            assert!(matches!(sessions.iter().find(|s| s.id.to_string() == session).expect("DSH session").state,
                nexus_agent_host::SessionState::Busy(_)), "refusal must not transition to Cancelling");
        }
        assert_eq!(drain(port.as_ref(), &op_id).await, expected);
        let shutdown = port.call(call(ProviderCallMethod::Shutdown, Some(&session), None, json!({"provider_id": "configured-acp"})))
            .await.expect("shutdown selected owner");
        assert!(shutdown.ok, "{shutdown:?}");
    }
    let dsh_log = std::fs::read_to_string(root.join("dsh-native.jsonl")).expect("DSH protocol log");
    for line in dsh_log.lines() {
        let record: Value = serde_json::from_str(line).expect("DSH record");
        assert_ne!(record["method"], "session/cancel");
    }
    // No callback available means configured ACP remains the Rust LocalSet path.
    let rust_port = compose_provider_port(host.clone(), Arc::new(host.build_provider_port().await), None);
    let session = launch(rust_port.as_ref(), &root, "configured-acp").await;
    assert!(host.list_sessions().await.expect("native registry").iter().any(|s| s.id.to_string() == session));
    rust_port.call(call(ProviderCallMethod::Shutdown, Some(&session), None, json!({}))).await.expect("Rust ACP shutdown");
    callback.close().await;
    host.shutdown().await.expect("host shutdown");
}

struct FailingPort(std::sync::atomic::AtomicUsize);
#[async_trait]
impl ProviderPort for FailingPort {
    async fn call(&self, _: ProviderCall) -> ProviderResult<ProviderReply> {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Err(CoreError { code: CoreErrorCode::Interrupted, message: "admitted effect failed".into(), details: Default::default(), http_status: Some(503) })
    }
    async fn next(&self, _: String, _: u32, _: u32) -> ProviderResult<ProviderEventBatch> {
        Err(CoreError { code: CoreErrorCode::NotFound, message: "no operation".into(), details: Default::default(), http_status: Some(404) })
    }
}

#[tokio::test]
async fn admission_rejects_forged_recipe_and_never_falls_back_after_effect() {
    use std::sync::atomic::Ordering;
    let tmp = tempfile::tempdir().expect("workspace");
    let root = tmp.path().canonicalize().expect("workspace");
    let host = host(&root).await;
    let native = Arc::new(FailingPort(0.into()));
    let acp = Arc::new(FailingPort(0.into()));
    let port = compose_provider_port(host.clone(), native.clone(), Some(acp.clone()));
    let mut payload = serde_json::to_value(ProbeRequest { cwd: root.clone(), owner: owner(&root), timeout_ms: 30_000 }).expect("probe");
    payload["provider_id"] = "configured-acp".into();
    payload["recipe"] = json!({});
    let rejected = port.call(call(ProviderCallMethod::Probe, None, None, payload.clone())).await.expect_err("caller recipe");
    assert_eq!(rejected.code, CoreErrorCode::Forbidden);
    assert_eq!(acp.0.load(Ordering::SeqCst), 0);
    payload.as_object_mut().expect("payload").remove("recipe");
    let failure = port.call(call(ProviderCallMethod::Probe, None, None, payload)).await.expect_err("selected effect fails");
    assert_eq!(failure.code, CoreErrorCode::Interrupted);
    assert_eq!(acp.0.load(Ordering::SeqCst), 1);
    assert_eq!(native.0.load(Ordering::SeqCst), 0, "no native fallback after ACP effect");
    assert!(host.provider_catalog().await.expect("catalog").entries.iter().any(|e| e.protocol_kind == ProtocolKind::NativeCli));
    host.shutdown().await.expect("host shutdown");
}

struct PausedExecutePort {
    inner: Arc<dyn ProviderPort>,
    pause_first: std::sync::atomic::AtomicBool,
    entered: tokio::sync::Notify,
    release: tokio::sync::Notify,
}

#[async_trait]
impl ProviderPort for PausedExecutePort {
    async fn call(&self, request: ProviderCall) -> ProviderResult<ProviderReply> {
        if request.method == ProviderCallMethod::Execute
            && self.pause_first.swap(false, std::sync::atomic::Ordering::SeqCst)
        {
            self.entered.notify_one();
            self.release.notified().await;
        }
        self.inner.call(request).await
    }

    async fn next(&self, id: String, events: u32, bytes: u32) -> ProviderResult<ProviderEventBatch> {
        self.inner.next(id, events, bytes).await
    }
}

#[tokio::test]
async fn concurrent_sessions_cannot_claim_the_same_operation() {
    let tmp = tempfile::tempdir().expect("workspace");
    let root = tmp.path().canonicalize().expect("workspace");
    let host = host(&root).await;
    let native = Arc::new(PausedExecutePort {
        inner: Arc::new(host.build_provider_port().await),
        pause_first: true.into(), entered: tokio::sync::Notify::new(), release: tokio::sync::Notify::new(),
    });
    let port = compose_provider_port(host.clone(), native.clone(), None);
    let first = launch(port.as_ref(), &root, "claude-native").await;
    let second = launch(port.as_ref(), &root, "codex-native").await;
    let id = HostOperationId::new();
    let operation = serde_json::to_value(HostOperation::Prompt {
        op_id: id.clone(), content: vec![HostContentBlock::Text { text: "hello".into() }], permission_scope: None,
    }).expect("prompt");
    let first_request = call(ProviderCallMethod::Execute, Some(&first), None, operation.clone());
    let first_port = port.clone();
    let executing = tokio::spawn(async move { first_port.call(first_request).await });
    tokio::time::timeout(Duration::from_secs(5), native.entered.notified()).await.expect("first effect admitted");
    let duplicate = port.call(call(ProviderCallMethod::Execute, Some(&second), None, operation))
        .await.expect_err("duplicate rejected before the second provider effect");
    assert_eq!(duplicate.code, CoreErrorCode::InvalidInput);
    native.release.notify_one();
    let reply = executing.await.expect("execute task").expect("first execute");
    assert!(reply.ok);
    let wrong_owner = port.call(call(ProviderCallMethod::Cancel, Some(&second), Some(&id.to_string()), json!({})))
        .await.expect_err("foreign session cannot cancel this operation");
    assert_eq!(wrong_owner.code, CoreErrorCode::InvalidInput);
    assert_eq!(drain(port.as_ref(), &id.to_string()).await, ["hello from mock claude"]);
    for session in [first, second] {
        port.call(call(ProviderCallMethod::Shutdown, Some(&session), None, json!({}))).await.expect("shutdown");
    }
    host.shutdown().await.expect("host shutdown");
}
