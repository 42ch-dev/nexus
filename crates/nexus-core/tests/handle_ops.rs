//! P3-T2 typed execution-operation contract.
//!
//! Proves the behaviour classes the L2 review required:
//!
//! 1. **Authorization** — a foreign creator is refused before any write.
//! 2. **Bypass audit** — a `force_gates` request must carry a reason, and the
//!    bypass row is written BEFORE every fallible precondition, so a refused
//!    bypass attempt is still recorded.
//! 3. **Field passthrough** — the request's concurrency declaration and its
//!    explicit role bindings reach the durable row and the frozen descriptor.
//! 4. **Ownership** — a signal against a foreign schedule is refused.
//! 5. **Owner fence** — every entry point refuses once the owner is closing.
//!
//! Each case drives a real engine-owner core over an admitted pool; the
//! schedule supervisor is attached exactly as the daemon bundle attaches it,
//! so the tests exercise the production insert path rather than its refusal.

#![cfg(feature = "execution")]
#![allow(clippy::unwrap_used)]

use async_trait::async_trait;
use nexus_contracts::local::schedule::http::{
    AddScheduleRequest, AgentBindingDto, ScheduleConcurrencyRequest, SignalScheduleRequest,
};
use nexus_contracts::{
    CoreError as WireCoreError, CoreErrorCode, ProviderCall, ProviderEventBatch, ProviderReply,
};
use nexus_core::execution::ExecutionHandle;
use nexus_core::{CoreAccess, CoreError, CoreOpenOptions, CoreService, RunnerDeps};
use nexus_orchestration::capability::{
    CapabilityError, PromptExecutor, PromptRequest, PromptResult,
};
use nexus_provider_ports::{ProviderPort, ProviderResult};
use std::sync::Arc;
use tempfile::TempDir;

const CREATOR: &str = "op_creator";
const OTHER_CREATOR: &str = "intruder";
const SLUG: &str = "default";

/// Deterministic provider that never performs an effect.
struct NullProvider;

#[async_trait]
impl ProviderPort for NullProvider {
    async fn call(&self, _request: ProviderCall) -> ProviderResult<ProviderReply> {
        Err(WireCoreError {
            code: CoreErrorCode::Internal,
            message: "null provider: no live model in this test".to_string(),
            details: serde_json::Map::default(),
            http_status: Some(500),
        })
    }

    async fn next(
        &self,
        operation_id: String,
        _max_events: u32,
        _max_bytes: u32,
    ) -> ProviderResult<ProviderEventBatch> {
        Ok(ProviderEventBatch {
            operation_id,
            events: vec![],
            has_more: false,
            gap: None,
        })
    }
}

/// A prompt executor that never runs (the tests never drive a preset).
struct NullPromptExecutor;

#[async_trait]
impl PromptExecutor for NullPromptExecutor {
    async fn execute(&self, _request: PromptRequest) -> Result<PromptResult, CapabilityError> {
        Err(CapabilityError::Internal(
            "no prompt executor in this test".into(),
        ))
    }
}

struct Fixture {
    tmp: TempDir,
    db_path: std::path::PathBuf,
}

/// Build a workspace home and seed the admitted creator, exactly as the daemon
/// does, so a `CoreService` can be opened over it.
async fn fixture() -> Fixture {
    let tmp = TempDir::new().unwrap();
    let user_home = tmp.path().to_path_buf();
    let home = user_home.join(".nexus42");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(nexus_home_layout::operational_workspace_dir(
        &user_home, CREATOR, SLUG,
    ))
    .unwrap();
    std::fs::write(
        home.join("config.toml"),
        format!(
            "active_creator_id = \"{CREATOR}\"\n[active_workspace_slug_by_creator]\n\"{CREATOR}\" = \"{SLUG}\"\n"
        ),
    )
    .unwrap();

    let db_path = nexus_home_layout::workspace_state_db_path(&user_home, CREATOR, SLUG);
    let guarded = nexus_local_db::init_engine_pool(&db_path)
        .await
        .expect("engine pool init");
    sqlx::query(
        "INSERT OR IGNORE INTO creators (creator_id, display_name, status, \
         cached_at, data) VALUES (?, 'Test', 'active', datetime('now'), '{}')",
    )
    .bind(CREATOR)
    .execute(guarded.pool())
    .await
    .expect("seed the admitted creator row");
    guarded.pool().close().await;
    nexus_local_db::writer_protocol::release_retained_writer_guards(&db_path);

    Fixture { tmp, db_path }
}

/// Open an engine-owner core and establish a handle over it.
///
/// A schedule supervisor is attached over the coordinator's own pool, exactly
/// as the daemon wires its runtime bundle. Without it `add_schedule` refuses
/// with `NotFound` (no insertion authority), so every success-path assertion
/// here would be measuring the refusal instead of the behaviour under test.
async fn open_handle(f: &Fixture) -> (CoreService, Arc<ExecutionHandle>) {
    let (core, handle) = open_handle_bare(f).await;
    handle.set_schedule_supervisor(Arc::new(
        nexus_orchestration::schedule::supervisor::ScheduleSupervisor::new(
            handle.coordinator().pool(),
        ),
    ));
    (core, handle)
}

/// Same owner, but with NO schedule supervisor attached — the insertion
/// authority is absent, so `add_schedule` cannot reach the insert.
async fn open_handle_bare(f: &Fixture) -> (CoreService, Arc<ExecutionHandle>) {
    let core = CoreService::open(CoreOpenOptions {
        user_home: f.tmp.path().to_path_buf(),
        access: CoreAccess::EngineOwner,
    })
    .await
    .expect("engine-owner core open");
    let deps = RunnerDeps {
        prompt_executor: Some(Arc::new(NullPromptExecutor) as Arc<dyn PromptExecutor>),
        workspace_root: Some(nexus_home_layout::operational_workspace_dir(
            f.tmp.path(),
            CREATOR,
            SLUG,
        )),
        nexus_home: Some(f.tmp.path().join(".nexus42")),
        ..RunnerDeps::default()
    };
    let handle = core
        .start_execution(Arc::new(NullProvider) as Arc<dyn ProviderPort>, deps)
        .await
        .expect("execution owner starts");
    (core, handle)
}

/// A schedule request naming `creator`.
fn request_for(creator: &str, preset: &str) -> AddScheduleRequest {
    AddScheduleRequest {
        creator_id: creator.to_string(),
        preset_id: preset.to_string(),
        seed: None,
        label: None,
        depends_on: None,
        concurrency: None,
        scheduled_at: None,
        input: None,
        force_gates: false,
        reason: None,
        agent_bindings: None,
    }
}

/// `memory-augmented` declares an `acp_prompt` node, so the freeze requires a
/// binding for the `default` role. Supplying it explicitly keeps the success
/// path reachable without a configured default provider.
fn default_bindings() -> std::collections::HashMap<String, AgentBindingDto> {
    let mut bindings = std::collections::HashMap::new();
    bindings.insert(
        "default".to_string(),
        AgentBindingDto {
            provider_id: "test-provider".to_string(),
            model: None,
        },
    );
    bindings
}

/// 1. A foreign creator is refused before any write.
#[tokio::test]
#[serial_test::serial]
async fn add_schedule_refuses_foreign_creator() {
    let f = fixture().await;
    let (core, handle) = open_handle(&f).await;
    let principal = core.active_principal().await.unwrap();

    let err = handle
        .add_schedule(&principal, request_for(OTHER_CREATOR, "memory-augmented"))
        .await
        .unwrap_err();
    assert!(
        matches!(err, CoreError::Forbidden { .. }),
        "a foreign creator must be refused, got {err:?}"
    );
}

/// 2. A `force_gates` request without a reason is a validation refusal (the
///    bypass is audited, so it can never be silent).
#[tokio::test]
#[serial_test::serial]
async fn force_gates_requires_a_reason() {
    let f = fixture().await;
    let (core, handle) = open_handle(&f).await;
    let principal = core.active_principal().await.unwrap();

    let mut request = request_for(CREATOR, "memory-augmented");
    request.force_gates = true;
    request.reason = None;
    let err = handle.add_schedule(&principal, request).await.unwrap_err();
    assert!(
        matches!(err, CoreError::InvalidInput { .. }),
        "force_gates without a reason must be refused, got {err:?}"
    );
}

/// 3. A `force_gates` bypass leaves an audit row even when the insert never
///    runs.
///
/// This is the ordering invariant: the audit row is written BEFORE every
/// fallible insertion precondition. The daemon writes the audit and schedule
/// rows in one transaction; this seam keeps ONE insertion authority (the
/// supervisor, which owns the row+deps+seed transaction) and accepts an
/// audit-without-schedule row, which records an ATTEMPTED bypass — the
/// conservative side.
///
/// Regression: the audit write used to sit AFTER the insertion preconditions,
/// so a bypass attempt that was refused for any of them (here: no supervisor
/// attached, i.e. no insertion authority) left NO trace at all.
#[tokio::test]
#[serial_test::serial]
async fn force_gates_bypass_is_audited_even_when_the_insert_is_refused() {
    let f = fixture().await;
    let (core, handle) = open_handle_bare(&f).await;
    let principal = core.active_principal().await.unwrap();

    let mut request = request_for(CREATOR, "memory-augmented");
    request.force_gates = true;
    request.reason = Some("test bypass".to_string());
    request.agent_bindings = Some(default_bindings());
    // No supervisor: the insert cannot run and the call is refused.
    let err = handle.add_schedule(&principal, request).await.unwrap_err();
    assert!(
        matches!(err, CoreError::NotFound { .. }),
        "an unattached insertion authority must be refused, got {err:?}"
    );

    let rows = nexus_local_db::list_force_gates_audit(core.pool(), CREATOR)
        .await
        .expect("audit rows");
    assert!(
        rows.iter().any(|r| r.preset_id == "memory-augmented"),
        "a refused bypass must still leave an audit row, got {rows:?}"
    );

    // And the refused attempt published no schedule row.
    let scheduled: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM creator_schedules WHERE creator_id = ?")
            .bind(CREATOR)
            .fetch_one(core.pool())
            .await
            .unwrap();
    assert_eq!(
        scheduled, 0,
        "a refused insert must publish no schedule row"
    );
}

/// 4. The same invariant holds when the refusal comes from the OTHER fallible
///    precondition: an unresolvable preset.
///
/// `resolve_preset` runs after the audit write, so a bypass naming a preset
/// that cannot be loaded is still recorded. This is the second half of the
/// ordering bug — the first fix only moved the supervisor lookup.
#[tokio::test]
#[serial_test::serial]
async fn force_gates_bypass_is_audited_when_the_preset_cannot_resolve() {
    let f = fixture().await;
    let (core, handle) = open_handle(&f).await;
    let principal = core.active_principal().await.unwrap();

    let mut request = request_for(CREATOR, "no-such-preset-anywhere");
    request.force_gates = true;
    request.reason = Some("test bypass".to_string());
    let err = handle.add_schedule(&principal, request).await.unwrap_err();
    assert!(
        matches!(err, CoreError::Internal { .. }),
        "an unresolvable preset must be refused, got {err:?}"
    );

    let rows = nexus_local_db::list_force_gates_audit(core.pool(), CREATOR)
        .await
        .expect("audit rows");
    assert!(
        rows.iter()
            .any(|r| r.preset_id == "no-such-preset-anywhere"),
        "a bypass for an unresolvable preset must still be audited, got {rows:?}"
    );
}

/// 5. The request's concurrency declaration reaches the durable row.
#[tokio::test]
#[serial_test::serial]
async fn add_schedule_preserves_parallel_with_concurrency() {
    let f = fixture().await;
    let (core, handle) = open_handle(&f).await;
    let principal = core.active_principal().await.unwrap();

    let mut request = request_for(CREATOR, "memory-augmented");
    request.concurrency = Some(ScheduleConcurrencyRequest::ParallelWith {
        schedule_ids: vec!["SCH-other".to_string()],
    });
    request.agent_bindings = Some(default_bindings());
    let response = handle
        .add_schedule(&principal, request)
        .await
        .expect("a fully-specified request must be admitted");

    let kind: String =
        sqlx::query_scalar("SELECT concurrency_kind FROM creator_schedules WHERE schedule_id = ?")
            .bind(&response.schedule_id)
            .fetch_one(core.pool())
            .await
            .unwrap();
    assert_eq!(
        kind, "parallel_with",
        "the request's concurrency must be stored, not silently serialized"
    );
    let whitelist: Option<String> = sqlx::query_scalar(
        "SELECT concurrency_whitelist FROM creator_schedules WHERE schedule_id = ?",
    )
    .bind(&response.schedule_id)
    .fetch_one(core.pool())
    .await
    .unwrap();
    assert!(
        whitelist
            .as_deref()
            .unwrap_or_default()
            .contains("SCH-other"),
        "the parallel_with whitelist must be preserved, got {whitelist:?}"
    );
}

/// 6. The caller's explicit bindings are frozen into the descriptor, so
///    admission cannot silently rebind a run to a different provider.
#[tokio::test]
#[serial_test::serial]
async fn add_schedule_freezes_explicit_agent_bindings() {
    let f = fixture().await;
    let (core, handle) = open_handle(&f).await;
    let principal = core.active_principal().await.unwrap();

    let mut request = request_for(CREATOR, "memory-augmented");
    request.agent_bindings = Some(default_bindings());
    let response = handle.add_schedule(&principal, request).await.unwrap();

    let raw: Option<Vec<u8>> = sqlx::query_scalar(
        "SELECT execution_descriptor_json FROM creator_schedules WHERE schedule_id = ?",
    )
    .bind(&response.schedule_id)
    .fetch_one(core.pool())
    .await
    .unwrap();
    let descriptor =
        String::from_utf8(raw.expect("a driven_v1 row must carry a descriptor")).unwrap();
    assert!(
        descriptor.contains("test-provider"),
        "the frozen descriptor must carry the caller's binding, got {descriptor}"
    );
}

/// 7. A signal against a foreign schedule is refused (ownership is enforced
///    before mutation, and a foreign row is indistinguishable from absent).
#[tokio::test]
#[serial_test::serial]
async fn signal_schedule_refuses_foreign_owner() {
    let f = fixture().await;
    let (core, handle) = open_handle(&f).await;
    let principal = core.active_principal().await.unwrap();

    // Seed a schedule owned by ANOTHER creator straight into the durable store.
    sqlx::query(
        "INSERT INTO creator_schedules \
         (schedule_id, creator_id, preset_id, preset_version, status, concurrency_kind, \
          current_core_context_version, created_at, updated_at, execution_policy) \
         VALUES ('SCH-foreign', ?, 'memory-augmented', 1, 'pending', 'serial', \
                 0, 0, 0, 'driven_v1')",
    )
    .bind(OTHER_CREATOR)
    .execute(core.pool())
    .await
    .unwrap();

    let err = handle
        .signal_schedule(
            &principal,
            "SCH-foreign".to_string(),
            SignalScheduleRequest {
                signal: "pause".to_string(),
                wait_id: None,
            },
        )
        .await
        .unwrap_err();
    assert!(
        matches!(err, CoreError::NotFound { .. }),
        "a foreign schedule must be refused, got {err:?}"
    );

    // The foreign row is untouched.
    let status: String = sqlx::query_scalar(
        "SELECT status FROM creator_schedules WHERE schedule_id = 'SCH-foreign'",
    )
    .fetch_one(core.pool())
    .await
    .unwrap();
    assert_eq!(
        status, "pending",
        "a refused signal must not mutate the row"
    );
}

/// 8. Every entry point refuses once the owner has begun closing.
#[tokio::test]
#[serial_test::serial]
async fn closing_owner_fences_every_entry_point() {
    let f = fixture().await;
    let (core, handle) = open_handle(&f).await;
    let principal = core.active_principal().await.unwrap();

    handle.close().await.expect("close");
    assert!(handle.is_draining(), "close must set the draining barrier");

    let add = handle
        .add_schedule(&principal, request_for(CREATOR, "memory-augmented"))
        .await
        .unwrap_err();
    assert!(
        matches!(add, CoreError::Closing),
        "add_schedule must be fenced after close, got {add:?}"
    );

    let signal = handle
        .signal_schedule(
            &principal,
            "SCH-any".to_string(),
            SignalScheduleRequest {
                signal: "pause".to_string(),
                wait_id: None,
            },
        )
        .await
        .unwrap_err();
    assert!(
        matches!(signal, CoreError::Closing),
        "signal_schedule must be fenced after close, got {signal:?}"
    );

    let commit = handle
        .commit_workspace(
            &principal,
            serde_json::from_value(serde_json::json!({ "sessionId": "s", "changes": [] })).unwrap(),
        )
        .await
        .unwrap_err();
    assert!(
        matches!(commit, CoreError::Closing),
        "commit_workspace must be fenced after close, got {commit:?}"
    );
}
