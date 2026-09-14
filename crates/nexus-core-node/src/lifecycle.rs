use std::path::{Path, PathBuf};

use std::time::{Duration, Instant};

const CLOSE_BUDGET: Duration = Duration::from_secs(5);
const CLOSE_CANCEL_PHASE: Duration = Duration::from_secs(2);

use std::sync::Arc;

use nexus_agent_host::capability::model::HostStartConfig;
use nexus_agent_host::config::{
    agent_host_config_path, load_config_from_path, validate_workspace_path, AgentHostConfig,
};
use nexus_agent_host::core::readiness::discover_provider_catalog;
use nexus_agent_host::{HostError, HostFacade, HostManager, ProviderCatalogEntry};
use nexus_contracts::native_open_options::NativeOpenOptionsAccess;
use nexus_contracts::{CoreCloseReport, CoreCloseReportState, NativeOpenOptions};
use nexus_core::{CoreAccess, CoreError, CoreOpenOptions, CoreService};
use nexus_home_layout::active_context::{try_resolve_state_db_path, CliConfigSnapshot};
use nexus_home_layout::nexus_root_from_home;
use nexus_provider_ports::ProviderPort;

use super::admitting_provider_port::AdmittingProviderPort;
use super::core_error;

use super::env_state::EnvState;

fn open_err(err: CoreError) -> String {
    core_error::open_reason_from_domain(err)
}

fn validate_initialized_prerequisites(
    user_home: &Path,
    access: CoreAccess,
) -> Result<(), CoreError> {
    let nexus_home = nexus_root_from_home(user_home);
    let cfg = CliConfigSnapshot::load(&nexus_home).map_err(|e| CoreError::Internal {
        category: format!("config_load: {e}"),
    })?;
    let creator_id = cfg
        .active_creator_id
        .clone()
        .ok_or(CoreError::AuthRequired)?;
    let workspace_slug = cfg.workspace_slug_for_creator(&creator_id);
    if workspace_slug.trim().is_empty() {
        return Err(CoreError::AuthRequired);
    }
    let db_path =
        try_resolve_state_db_path(user_home, &nexus_home).ok_or(CoreError::Uninitialized)?;
    if !db_path.exists() && access == CoreAccess::ReadOnly {
        return Err(CoreError::Uninitialized);
    }
    Ok(())
}

fn is_genuinely_uninitialized(user_home: &Path, access: CoreAccess) -> bool {
    match validate_initialized_prerequisites(user_home, access) {
        Ok(()) => false,
        Err(CoreError::Uninitialized) | Err(CoreError::AuthRequired) => true,
        Err(_) => false,
    }
}

struct ValidatedHostAdmission {
    config_path: PathBuf,
    host_config: AgentHostConfig,
    admitted_catalog: Vec<ProviderCatalogEntry>,
}

fn host_err(err: HostError) -> nexus_contracts::CoreError {
    core_error::wire_core_error_from_host(err)
}

fn validate_host_admission(
    user_home: &Path,
) -> Result<ValidatedHostAdmission, nexus_contracts::CoreError> {
    validate_workspace_path(user_home).map_err(host_err)?;
    let config_path = agent_host_config_path(user_home);
    let host_config = load_config_from_path(&config_path).map_err(host_err)?;
    let admitted_catalog = discover_provider_catalog(&host_config).map_err(host_err)?;
    Ok(ValidatedHostAdmission {
        config_path,
        host_config,
        admitted_catalog,
    })
}

async fn abort_opening(
    state: Arc<EnvState>,
    core: Option<Arc<CoreService>>,
    host: Option<Arc<HostManager>>,
    js_port: Option<Arc<dyn ProviderPort>>,
) {
    if core.is_some() || host.is_some() || js_port.is_some() {
        let (released, _) = cleanup_owners(
            state.clone(),
            core,
            host,
            js_port,
            Instant::now() + CLOSE_BUDGET,
        )
        .await;
        if released {
            state.publish_closed().await;
        } else {
            state.mark_interrupted();
        }
    } else {
        state.publish_closed().await;
    }
}

/// Test-only forcing of an unconfirmed cleanup.
mod forcing {
    use std::sync::atomic::{AtomicBool, Ordering};

    static FORCE_UNCONFIRMED: AtomicBool = AtomicBool::new(false);

    pub fn set(enable: bool) {
        FORCE_UNCONFIRMED.store(enable, Ordering::SeqCst);
    }

    pub fn get() -> bool {
        FORCE_UNCONFIRMED.load(Ordering::SeqCst)
    }

    /// Injected cleanup latency, test builds only.
    #[cfg(test)]
    pub mod delay {
        use std::sync::atomic::{AtomicU64, Ordering};

        static CLEANUP_DELAY_MS: AtomicU64 = AtomicU64::new(0);

        pub fn set(ms: u64) {
            CLEANUP_DELAY_MS.store(ms, Ordering::SeqCst);
        }

        pub fn get() -> u64 {
            CLEANUP_DELAY_MS.load(Ordering::SeqCst)
        }
    }
}

/// Enable/disable forced unconfirmed cleanup.
pub fn set_force_unconfirmed(enable: bool) {
    forcing::set(enable);
}

/// Test-only: inject async delay at the start of `cleanup_owners`.
#[cfg(test)]
pub fn set_force_cleanup_delay_ms(ms: u64) {
    forcing::delay::set(ms);
}

fn forced_unconfirmed() -> bool {
    forcing::get()
}

struct RestoreCore {
    value: Option<Arc<CoreService>>,
    state: Arc<EnvState>,
    disarmed: bool,
}

impl RestoreCore {
    fn disarm(&mut self) {
        self.disarmed = true;
        self.value = None;
    }
}

impl Drop for RestoreCore {
    fn drop(&mut self) {
        if self.disarmed {
            return;
        }
        if let Some(v) = self.value.take() {
            let mut guard = self.state.core.lock().expect("core mutex poisoned");
            if guard.is_none() {
                *guard = Some(v);
            }
        }
    }
}

struct RestoreHost {
    value: Option<Arc<HostManager>>,
    state: Arc<EnvState>,
    disarmed: bool,
}

impl RestoreHost {
    fn disarm(&mut self) {
        self.disarmed = true;
        self.value = None;
    }
}

impl Drop for RestoreHost {
    fn drop(&mut self) {
        if self.disarmed {
            return;
        }
        if let Some(v) = self.value.take() {
            let mut guard = self.state.host.lock().expect("core mutex poisoned");
            if guard.is_none() {
                *guard = Some(v);
            }
        }
    }
}

struct RestorePort {
    value: Option<Arc<dyn ProviderPort>>,
    state: Arc<EnvState>,
    disarmed: bool,
}

impl RestorePort {
    fn disarm(&mut self) {
        self.disarmed = true;
        self.value = None;
    }
}

impl Drop for RestorePort {
    fn drop(&mut self) {
        if self.disarmed {
            return;
        }
        if let Some(v) = self.value.take() {
            let mut guard = self
                .state
                .provider_port
                .lock()
                .expect("core mutex poisoned");
            if guard.is_none() {
                *guard = Some(v);
            }
        }
    }
}

fn interrupted_report(pending: Vec<String>) -> CoreCloseReport {
    CoreCloseReport {
        state: CoreCloseReportState::Interrupted,
        cleanup_confirmed: false,
        pending_operations: pending,
        reason: None,
    }
}

fn closed_report() -> CoreCloseReport {
    CoreCloseReport {
        state: CoreCloseReportState::Closed,
        cleanup_confirmed: true,
        pending_operations: vec![],
        reason: None,
    }
}

/// A close releases cleanup ownership only when it is closed *and* confirmed.
#[must_use]
fn close_released(report: &CoreCloseReport) -> bool {
    report.state == CoreCloseReportState::Closed && report.cleanup_confirmed
}

/// True when a provider reply proves the session's owned process is gone.
///
/// The TS adapter's `cancel` is a full release for a session with a live
/// operation: it cancels the prompt, reaps the owned child and drops the
/// session. Its early-return path (the operation already had a terminal) only
/// reports the cancel, so an ok cancel alone is not proof — the follow-up
/// shutdown is. `session_not_found` from that shutdown is the adapter telling us
/// the session was already released.
fn js_session_released(reply: &nexus_contracts::ProviderReply) -> bool {
    if reply.ok {
        return true;
    }
    reply
        .error
        .as_ref()
        .is_some_and(|error| error.message == "session_not_found")
}

/// Release the JS provider's owned sessions while its callback bridge is alive.
///
/// The TS adapter owns the ACP child it spawns, so native close must ask it to
/// release: there is no native handle to terminate. This runs inside the close's
/// cancel/drain window, before the admission fence rises, because a release call
/// the fence rejects leaks a child. Each session gets a bounded cancel followed
/// by a bounded shutdown, and only an observable release reply counts. Anything
/// unconfirmed stays registered and forces an Interrupted close, so a leaked
/// child can never be reported as a confirmed cleanup.
async fn release_js_provider_sessions(state: &Arc<EnvState>, deadline: Instant) -> Vec<String> {
    let port = state
        .provider_port
        .lock()
        .expect("port mutex poisoned")
        .clone();
    let Some(port) = port else {
        return state.js_session_ids();
    };

    let sessions = state.js_sessions_snapshot();
    let mut unconfirmed = Vec::new();
    let total = sessions.len();
    for (index, (session_id, operation_id)) in sessions.iter().enumerate() {
        // Share the remaining drain budget across the sessions still to release.
        let outstanding = u32::try_from(total - index).unwrap_or(1);
        let slice = deadline.saturating_duration_since(Instant::now()) / outstanding;
        if slice.is_zero() {
            unconfirmed.push(session_id.clone());
            continue;
        }

        // 1. Ask the adapter to cancel a live operation. For a busy session this
        //    is what makes the blocked prompt settle so the reap can proceed; a
        //    failure is not fatal because the shutdown below is the authority.
        if let Some(operation_id) = operation_id {
            let request = nexus_contracts::ProviderCall {
                request_id: format!("close-cancel-{session_id}"),
                method: nexus_contracts::provider_call::ProviderCallMethod::Cancel,
                session_id: Some(session_id.clone()),
                operation_id: Some(operation_id.clone()),
                deadline_ms: u64::try_from((slice / 2).as_millis()).unwrap_or(u64::MAX),
                payload: serde_json::Map::new(),
            };
            let _ = tokio::time::timeout(slice / 2, port.call(request)).await;
        }

        // 2. Release the session and require the adapter to confirm the owned
        //    child is gone.
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            unconfirmed.push(session_id.clone());
            continue;
        }
        let request = nexus_contracts::ProviderCall {
            request_id: format!("close-release-{session_id}"),
            method: nexus_contracts::provider_call::ProviderCallMethod::Shutdown,
            session_id: Some(session_id.clone()),
            operation_id: None,
            deadline_ms: u64::try_from(remaining.as_millis()).unwrap_or(u64::MAX),
            payload: serde_json::Map::new(),
        };
        match tokio::time::timeout(remaining, port.call(request)).await {
            Ok(Ok(reply)) if js_session_released(&reply) => state.forget_js_session(session_id),
            _ => unconfirmed.push(session_id.clone()),
        }
    }
    unconfirmed
}

/// Attempt cleanup over the retained owners, using the same ownership discipline
/// for a rollback, an explicit close, and an admission-fence settlement.
async fn cleanup_owners(
    state: Arc<EnvState>,
    inject_core: Option<Arc<CoreService>>,
    inject_host: Option<Arc<HostManager>>,
    inject_port: Option<Arc<dyn ProviderPort>>,
    deadline: Instant,
) -> (bool, CoreCloseReport) {
    let forced = forced_unconfirmed();

    #[cfg(test)]
    {
        let delay_ms = forcing::delay::get();
        if delay_ms > 0 {
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        }
    }

    let core_taken = match inject_core {
        Some(core) => Some(core),
        None => state.core.lock().expect("core mutex poisoned").take(),
    };
    let mut core_guard = RestoreCore {
        value: core_taken,
        state: state.clone(),
        disarmed: false,
    };

    let core_report = match core_guard.value.as_ref() {
        Some(service) => match service.close().await {
            Ok(report) => report,
            Err(_) => interrupted_report(vec![]),
        },
        None => closed_report(),
    };
    let core_released = !forced && close_released(&core_report);
    if core_released {
        core_guard.disarm();
    }

    let host_taken = match inject_host {
        Some(host) => Some(host),
        None => state.host.lock().expect("host mutex poisoned").take(),
    };
    let mut host_guard = RestoreHost {
        value: host_taken,
        state: state.clone(),
        disarmed: false,
    };

    let host_released = match host_guard.value.take() {
        Some(manager) => {
            // Closing stops new work and freezes the queue immediately: the
            // queue snapshot below is the state the close actually drains.
            manager.localset_bridge().begin_drain();
            let at_close = manager.localset_bridge().stats();
            let pending = manager.pending_lifecycle_snapshot().await;
            state.record_pending_operations(pending).await;
            let shutdown_ok = manager.shutdown().await.is_ok();
            let bridge_evidence = manager.localset_bridge().shutdown_before(deadline).await;
            // Settlement is a clean join with no live work — never `!thread_alive`
            // alone, which a panicked thread also satisfies.
            let bridge_settled = bridge_evidence.is_settled();
            let bridge_entries = manager
                .localset_bridge()
                .close_entries(&at_close, &bridge_evidence);
            if !forced && shutdown_ok && bridge_settled {
                state.record_pending_operations(bridge_entries).await;
                host_guard.disarm();
                true
            } else {
                // A join that could not settle inside the budget hands its
                // handle to tracked cleanup; a failed join has none to retain
                // but is still never released.
                if bridge_evidence.thread_alive {
                    if let Some(handle) = manager.localset_bridge().take_retained_runtime_thread() {
                        super::cleanup_registry::register_localset_thread(handle, state.clone());
                    }
                }
                let mut pending = state.pending_operations_snapshot().await;
                pending.extend(bridge_entries);
                pending.extend(bridge_evidence.unsettled_pending());
                if !shutdown_ok {
                    pending.push("host-shutdown-unconfirmed".to_string());
                }
                state.record_pending_operations(pending).await;
                host_guard.value = Some(manager);
                false
            }
        }
        None => true,
    };

    let port_taken = match inject_port {
        Some(port) => Some(port),
        None => state
            .provider_port
            .lock()
            .expect("port mutex poisoned")
            .take(),
    };
    let mut port_guard = RestorePort {
        value: port_taken,
        state: state.clone(),
        disarmed: false,
    };

    let unreleased_js = state.js_session_ids();
    let released = core_released && host_released && unreleased_js.is_empty();
    if released {
        port_guard.disarm();
        state.clear_interrupted();
        state.closing.store(0, std::sync::atomic::Ordering::SeqCst);
        state
            .generation
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        state.publish_closed().await;
        let drained = state.pending_operations_snapshot().await;
        let report = CoreCloseReport {
            state: CoreCloseReportState::Closed,
            cleanup_confirmed: true,
            pending_operations: drained,
            reason: None,
        };
        (true, report)
    } else {
        state.mark_interrupted();
        let mut pending = state.pending_operations_snapshot().await;
        pending.extend(
            unreleased_js
                .iter()
                .map(|session_id| format!("js-provider-session:{session_id}")),
        );
        state.record_pending_operations(pending.clone()).await;
        (false, interrupted_report(pending))
    }
}

/// Settlement of whatever a previous failed rollback or interrupted close retained.
///
/// This RETRIES rather than replaying the retained verdict: a JS-provider session
/// whose owned child was never released gets a fresh bounded release attempt, and
/// then the retained owners are re-run to settlement. Claiming a confirmed close
/// still requires every JS session to report released, so a retry that fails
/// keeps the environment interrupted instead of upgrading an unconfirmed result.
async fn settle_retained(state: Arc<EnvState>) -> (bool, CoreCloseReport) {
    let started = Instant::now();
    let deadline = started + CLOSE_BUDGET;
    if !state.js_session_ids().is_empty() {
        release_js_provider_sessions(&state, started + CLOSE_CANCEL_PHASE).await;
    }
    cleanup_owners(state, None, None, None, deadline).await
}

/// Open the native core.
pub async fn open_core(
    state: Arc<EnvState>,
    options: NativeOpenOptions,
    js_port: Option<Arc<dyn ProviderPort>>,
) -> Result<(), String> {
    if state.is_env_dead() {
        return Err("dead_env".to_string());
    }
    if state.is_interrupted() {
        let (released, _) = settle_retained(state.clone()).await;
        if !released {
            return Err(
                "interrupted: retained cleanup owner requires a confirmed close".to_string(),
            );
        }
    }
    if state.settled_close.lock().await.is_some() && !state.is_interrupted() {
        state.settled_close.lock().await.take();
        *state.close_in_flight.lock().await = false;
        state.closing.store(0, std::sync::atomic::Ordering::SeqCst);
    }
    if state.is_closing() && state.settled_close.lock().await.is_none() {
        return Err("closing".to_string());
    }
    if state.core.lock().expect("core mutex poisoned").is_some() {
        return Err("already open".to_string());
    }
    state.try_begin_open().await?;
    let access = match options.access {
        NativeOpenOptionsAccess::ReadOnly => CoreAccess::ReadOnly,
        NativeOpenOptionsAccess::DirectWriter => CoreAccess::DirectWriter,
        NativeOpenOptionsAccess::EngineOwner => CoreAccess::EngineOwner,
    };
    let user_home = PathBuf::from(options.user_home);

    if options.allow_uninitialized && is_genuinely_uninitialized(&user_home, access) {
        state.clear_service_only_uninitialized();
        state.mark_service_only_uninitialized();
        state.publish_open().await;
        return Ok(());
    }

    if let Err(err) = validate_initialized_prerequisites(&user_home, access) {
        let reason = open_err(err);
        abort_opening(state.clone(), None, None, js_port.clone()).await;
        return Err(reason);
    }

    let admission = match validate_host_admission(&user_home) {
        Ok(admission) => admission,
        Err(err) => {
            let reason = core_error::open_reason_from_wire(err);
            abort_opening(state.clone(), None, None, js_port.clone()).await;
            return Err(reason);
        }
    };

    let core = match CoreService::open(CoreOpenOptions {
        user_home: user_home.clone(),
        access,
    })
    .await
    {
        Ok(core) => core,
        Err(err) => {
            let reason = open_err(err);
            abort_opening(state.clone(), None, None, js_port.clone()).await;
            return Err(reason);
        }
    };

    if let Err(err) = core.active_principal().await {
        let reason = open_err(err);
        abort_opening(state.clone(), Some(Arc::new(core)), None, js_port.clone()).await;
        return Err(reason);
    }

    let host = Arc::new(HostManager::new());
    let start_config = HostStartConfig {
        config_path: admission.config_path,
        workspace_root: user_home.clone(),
        max_sessions: admission.host_config.max_sessions,
        max_ops_per_session: admission.host_config.max_ops_per_session,
        timeouts: admission.host_config.timeouts.clone(),
        host_config: Some(admission.host_config),
        admitted_catalog: Some(admission.admitted_catalog),
        probe_owner: None,
    };
    if let Err(err) = host.start(start_config).await {
        let reason = core_error::open_reason_from_host(err);
        abort_opening(
            state.clone(),
            Some(Arc::new(core)),
            Some(host),
            js_port.clone(),
        )
        .await;
        return Err(reason);
    }

    let provider_port: Arc<dyn ProviderPort> = if let Some(port) = js_port.clone() {
        Arc::new(AdmittingProviderPort::new(
            host.clone(),
            port,
            state.clone(),
        ))
    } else {
        Arc::new(host.build_provider_port().await)
    };

    let core = Arc::new(core);
    state.clear_service_only_uninitialized();
    // Install the durable journal pool and settle any operation orphaned by the
    // predecessor process's exit as `interrupted` (LIFE-3). This must happen
    // before the host accepts new work so a restarted process never re-dispatches
    // a journaled op and the prior active op is queryable immediately. A failed
    // settlement cannot prove that recovery, so the open fails instead of
    // publishing a settled journal it does not have.
    state.set_journal_pool(core.pool().clone());
    if let Err(err) = state.settle_journal_on_open().await {
        let reason = format!("journal settlement failed: {err}");
        abort_opening(state.clone(), Some(core), Some(host), js_port.clone()).await;
        return Err(reason);
    }
    state
        .host
        .lock()
        .expect("host mutex poisoned")
        .replace(host);
    state
        .provider_port
        .lock()
        .expect("port mutex poisoned")
        .replace(provider_port);
    state
        .core
        .lock()
        .expect("core mutex poisoned")
        .replace(core);
    state.publish_open().await;
    Ok(())
}

/// Close the environment with a 5s phased budget and one concurrent settlement.
pub async fn close_core(state: Arc<EnvState>) -> CoreCloseReport {
    // Clone into a local first: `if let` on a temporary would hold the async
    // mutex guard across the retry below and deadlock against it.
    let cached = state.settled_close.lock().await.clone();
    if let Some(report) = cached {
        if report.cleanup_confirmed {
            // A confirmed close is idempotent: nothing is left to retry.
            return report;
        }
        // A previously interrupted close retained owners and possibly live JS
        // sessions. Retry the retained cleanup on this later close instead of
        // replaying the cached Interrupted verdict forever.
        let (released, retried) = settle_retained(state.clone()).await;
        let report = if released { retried } else { report };
        *state.settled_close.lock().await = Some(report.clone());
        state.close_notify_settled.notify_waiters();
        return report;
    }
    {
        let mut in_flight = state.close_in_flight.lock().await;
        if *in_flight {
            let notify = state.close_notify_settled.clone();
            drop(in_flight);
            notify.notified().await;
            return state
                .settled_close
                .lock()
                .await
                .clone()
                .unwrap_or_else(|| interrupted_report(vec![]));
        }
        *in_flight = true;
    }

    let started = Instant::now();
    let deadline = started + CLOSE_BUDGET;

    // 0-2s: cancel/drain while the JS environment still lives. The JS adapter
    // owns its ACP children, so their release must run before the admission
    // fence rises: a release call the fence rejects leaks a child, exactly like
    // a shutdown a full LocalSet data queue would otherwise block. The registry
    // stays authoritative, so anything admitted during the drain still blocks a
    // confirmed close.
    let unreleased_js = release_js_provider_sessions(&state, started + CLOSE_CANCEL_PHASE).await;
    if !unreleased_js.is_empty() {
        let mut pending = state.pending_operations_snapshot().await;
        pending.extend(
            unreleased_js
                .iter()
                .map(|session_id| format!("js-provider-session:{session_id}")),
        );
        state.record_pending_operations(pending).await;
    }

    state.begin_close();
    state.begin_closing_phase().await;

    let host_snapshot = state.host.lock().expect("host mutex poisoned").clone();
    if let Some(host) = host_snapshot {
        state
            .record_pending_operations(host.pending_lifecycle_snapshot().await)
            .await;
    }

    let report = match tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), async {
        let drain_budget = CLOSE_CANCEL_PHASE.min(CLOSE_BUDGET.saturating_sub(started.elapsed()));
        if !drain_budget.is_zero() {
            tokio::time::sleep(Duration::from_millis(25).min(drain_budget)).await;
        }
        let (_, report) = cleanup_owners(state.clone(), None, None, None, deadline).await;
        report
    })
    .await
    {
        Ok(report) => report,
        Err(_) => {
            state.mark_interrupted();
            let pending = state.pending_operations_snapshot().await;
            let owners_present = state.owner_slots_present();
            let mut pending = pending;
            if owners_present {
                pending.push("cleanup-owners-retained".to_string());
            }
            interrupted_report(pending)
        }
    };

    *state.settled_close.lock().await = Some(report.clone());
    *state.close_in_flight.lock().await = false;
    if !report.cleanup_confirmed {
        state.mark_interrupted();
    }
    state.close_notify_settled.notify_waiters();
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(state: CoreCloseReportState, cleanup_confirmed: bool) -> CoreCloseReport {
        CoreCloseReport {
            state,
            cleanup_confirmed,
            pending_operations: vec![],
            reason: None,
        }
    }

    #[test]
    fn confirmed_close_releases_cleanup_ownership() {
        assert!(close_released(&report(CoreCloseReportState::Closed, true)));
    }

    #[test]
    fn unconfirmed_or_interrupted_close_retains_cleanup_ownership() {
        assert!(!close_released(&report(
            CoreCloseReportState::Closed,
            false
        )));
        assert!(!close_released(&report(
            CoreCloseReportState::Interrupted,
            true
        )));
        assert!(!close_released(&report(
            CoreCloseReportState::Interrupted,
            false
        )));
    }

    #[test]
    fn interrupted_report_is_never_released() {
        assert!(!close_released(&interrupted_report(vec![])));
        assert!(close_released(&closed_report()));
    }

    #[test]
    fn forced_unconfirmed_is_off_in_a_fresh_process() {
        assert!(!forced_unconfirmed());
    }

    #[tokio::test]
    async fn failed_host_config_open_resets_and_reopens() {
        use crate::env_state::EnvLifecyclePhase;
        use crate::wire_fixture::seed_wire_home;
        use nexus_contracts::native_open_options::NativeOpenOptionsAccess;
        use nexus_contracts::NativeOpenOptions;
        use tempfile::tempdir;

        let dir = tempdir().expect("tempdir");
        seed_wire_home(dir.path()).await;
        let agent_host_dir = dir.path().join(".nexus42/agent-host");
        std::fs::create_dir_all(&agent_host_dir).expect("agent-host dir");
        let bad_config = agent_host_dir.join("config.toml");
        std::fs::write(&bad_config, "not = [valid").expect("bad config");

        let state = Arc::new(EnvState::new());
        let failed = open_core(
            state.clone(),
            NativeOpenOptions {
                user_home: dir.path().to_string_lossy().to_string(),
                access: NativeOpenOptionsAccess::EngineOwner,
                allow_uninitialized: false,
            },
            None,
        )
        .await;
        assert!(failed.is_err(), "invalid host config must fail open");
        assert_eq!(state.lifecycle_phase(), EnvLifecyclePhase::Closed);

        std::fs::remove_file(&bad_config).expect("remove bad config");
        open_core(
            state.clone(),
            NativeOpenOptions {
                user_home: dir.path().to_string_lossy().to_string(),
                access: NativeOpenOptionsAccess::EngineOwner,
                allow_uninitialized: false,
            },
            None,
        )
        .await
        .expect("reopen after corrected host config");
        assert!(state.owner_slots_present());
        let _ = close_core(state).await;
    }

    #[tokio::test]
    async fn open_retains_admitted_catalog_snapshot() {
        use crate::wire_fixture::seed_wire_home;
        use nexus_agent_host::config::{agent_host_config_path, load_config_from_path};
        use nexus_agent_host::core::readiness::discover_provider_catalog;
        use nexus_contracts::native_open_options::NativeOpenOptionsAccess;
        use nexus_contracts::NativeOpenOptions;
        use tempfile::tempdir;

        let dir = tempdir().expect("tempdir");
        seed_wire_home(dir.path()).await;
        let config_path = agent_host_config_path(dir.path());
        let host_config = load_config_from_path(&config_path).expect("host config");
        let expected = discover_provider_catalog(&host_config).expect("admit catalog");

        let state = Arc::new(EnvState::new());
        open_core(
            state.clone(),
            NativeOpenOptions {
                user_home: dir.path().to_string_lossy().to_string(),
                access: NativeOpenOptionsAccess::EngineOwner,
                allow_uninitialized: false,
            },
            None,
        )
        .await
        .expect("open");

        let host = state.host.lock().expect("host slot").clone().expect("host");
        let catalog = host.provider_catalog().await.expect("catalog");
        assert_eq!(catalog.entries.len(), expected.len());
        let expected_ids = expected
            .iter()
            .map(|e| e.provider_id.to_string())
            .collect::<std::collections::BTreeSet<_>>();
        let actual_ids = catalog
            .entries
            .iter()
            .map(|e| e.provider_id.to_string())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(expected_ids, actual_ids);
        let _ = close_core(state).await;
    }

    #[tokio::test]
    async fn service_only_uninitialized_open_denies_effects_and_closes() {
        use nexus_contracts::native_open_options::NativeOpenOptionsAccess;
        use nexus_contracts::NativeOpenOptions;
        use tempfile::tempdir;

        let dir = tempdir().expect("tempdir");
        let state = Arc::new(EnvState::new());
        open_core(
            state.clone(),
            NativeOpenOptions {
                user_home: dir.path().to_string_lossy().to_string(),
                access: NativeOpenOptionsAccess::DirectWriter,
                allow_uninitialized: true,
            },
            None,
        )
        .await
        .expect("service-only open");

        assert!(state.is_service_only_uninitialized());
        assert!(!state.owner_slots_present());

        let report = close_core(state.clone()).await;
        assert_eq!(report.state, CoreCloseReportState::Closed);
        assert!(report.cleanup_confirmed);
        assert!(!state.is_service_only_uninitialized());
    }

    #[tokio::test]
    async fn close_timeout_retains_owners_settle_then_reopen() {
        use crate::wire_fixture::seed_wire_home;
        use nexus_contracts::native_open_options::NativeOpenOptionsAccess;
        use nexus_contracts::NativeOpenOptions;
        use tempfile::tempdir;

        let dir = tempdir().expect("tempdir");
        seed_wire_home(dir.path()).await;
        let state = Arc::new(EnvState::new());
        set_force_cleanup_delay_ms(10_000);
        open_core(
            state.clone(),
            NativeOpenOptions {
                user_home: dir.path().to_string_lossy().to_string(),
                access: NativeOpenOptionsAccess::EngineOwner,
                allow_uninitialized: false,
            },
            None,
        )
        .await
        .expect("open");
        assert!(state.owner_slots_present());

        let report = close_core(state.clone()).await;
        assert_eq!(report.state, CoreCloseReportState::Interrupted);
        assert!(!report.cleanup_confirmed);
        assert!(state.owner_slots_present(), "timeout must retain owners");

        set_force_cleanup_delay_ms(0);
        let (released, settled) = settle_retained(state.clone()).await;
        assert!(released, "settlement must confirm cleanup");
        assert_eq!(settled.state, CoreCloseReportState::Closed);
        assert!(!state.owner_slots_present());

        let reopened = open_core(
            state.clone(),
            NativeOpenOptions {
                user_home: dir.path().to_string_lossy().to_string(),
                access: NativeOpenOptionsAccess::EngineOwner,
                allow_uninitialized: false,
            },
            None,
        )
        .await;
        assert!(reopened.is_ok(), "reopen after settlement must succeed");
        let final_report = close_core(state.clone()).await;
        assert_eq!(final_report.state, CoreCloseReportState::Closed);
        assert!(final_report.cleanup_confirmed);
    }

    /// Q1-C2: a JS session whose owned child was never released must be RETRIED
    /// by a later close, instead of the cached Interrupted verdict being replayed
    /// forever. The port here releases only on a request that carries a live
    /// deadline, which is what the retry path supplies.
    #[tokio::test]
    async fn later_close_retries_retained_js_session_release() {
        use crate::wire_fixture::seed_wire_home;
        use async_trait::async_trait;
        use nexus_contracts::native_open_options::NativeOpenOptionsAccess;
        use nexus_contracts::{
            CoreError, CoreErrorCode, NativeOpenOptions, ProviderCall, ProviderEventBatch,
            ProviderReply,
        };
        use nexus_provider_ports::{ProviderPort, ProviderResult};
        use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
        use tempfile::tempdir;

        struct DeferredReleasePort {
            attempts: AtomicUsize,
            released: std::sync::atomic::AtomicBool,
            /// Fail the first `release_attempts_before_success` shutdowns, as an
            /// adapter that hangs/errors on its first release attempt would.
            release_attempts_before_success: usize,
        }

        #[async_trait]
        impl ProviderPort for DeferredReleasePort {
            async fn call(&self, request: ProviderCall) -> ProviderResult<ProviderReply> {
                match request.method {
                    nexus_contracts::provider_call::ProviderCallMethod::Shutdown => {
                        let attempt = self.attempts.fetch_add(1, AtomicOrdering::SeqCst);
                        if attempt < self.release_attempts_before_success {
                            return Err(CoreError {
                                code: CoreErrorCode::Busy,
                                message: "release_unconfirmed".into(),
                                details: Default::default(),
                                http_status: Some(503),
                            });
                        }
                        self.released.store(true, AtomicOrdering::SeqCst);
                        Ok(ProviderReply {
                            request_id: request.request_id.clone(),
                            ok: true,
                            session_id: request.session_id.clone(),
                            operation_id: None,
                            health: None,
                            error: None,
                        })
                    }
                    _ => Err(CoreError {
                        code: CoreErrorCode::Internal,
                        message: "not used".into(),
                        details: Default::default(),
                        http_status: Some(500),
                    }),
                }
            }

            async fn next(
                &self,
                _operation_id: String,
                _max_events: u32,
                _max_bytes: u32,
            ) -> ProviderResult<ProviderEventBatch> {
                Err(CoreError {
                    code: CoreErrorCode::Internal,
                    message: "not used".into(),
                    details: Default::default(),
                    http_status: Some(500),
                })
            }
        }

        let dir = tempdir().expect("tempdir");
        seed_wire_home(dir.path()).await;
        let state = Arc::new(EnvState::new());
        let port = Arc::new(DeferredReleasePort {
            attempts: AtomicUsize::new(0),
            released: std::sync::atomic::AtomicBool::new(false),
            release_attempts_before_success: 1,
        });
        open_core(
            state.clone(),
            NativeOpenOptions {
                user_home: dir.path().to_string_lossy().to_string(),
                access: NativeOpenOptionsAccess::EngineOwner,
                allow_uninitialized: false,
            },
            Some(port.clone()),
        )
        .await
        .expect("open");

        // A live JS session whose operation is still in flight.
        state.record_js_session("sess-retained".to_string(), "mock-provider".to_string());
        state.record_js_session_operation("sess-retained", "op-live".to_string());

        // First close: the release attempt fails, so cleanup cannot be confirmed.
        let first = close_core(state.clone()).await;
        assert!(
            !first.cleanup_confirmed,
            "a failed release must not confirm cleanup: {first:?}"
        );

        // Later close: the retained session is retried and this time released.
        let second = close_core(state.clone()).await;
        assert!(
            second.cleanup_confirmed,
            "a later close must retry the retained JS session, not replay Interrupted: {second:?}"
        );
        assert!(
            state.js_session_ids().is_empty(),
            "the released session must be forgotten, not retained forever"
        );
        assert!(
            port.released.load(AtomicOrdering::SeqCst),
            "the retry must actually drive the adapter release"
        );
        assert!(
            port.attempts.load(AtomicOrdering::SeqCst) >= 2,
            "the retry must be a second, real release attempt"
        );
    }

    #[test]
    fn finalize_release_requires_a_settled_bridge_and_a_confirmed_host() {
        // Release is a conjunction. In particular a LocalSet thread that
        // panicked reports thread_alive == false, so a decision keyed off
        // `!thread_alive` would release an owner whose join actually failed.
        assert!(EnvState::finalize_owner_released(true, true));
        assert!(
            !EnvState::finalize_owner_released(false, true),
            "an unsettled bridge (live OR join-failed) must never release the owner"
        );
        assert!(
            !EnvState::finalize_owner_released(true, false),
            "a confirmed host cannot stand in for a settled bridge alone"
        );
        assert!(!EnvState::finalize_owner_released(false, false));
    }

    #[test]
    fn host_shutdown_error_never_counts_as_finalize_success() {
        use nexus_agent_host::HostError;

        assert!(
            EnvState::host_shutdown_confirmed(&Ok(Ok(()))),
            "a confirmed host shutdown releases the owner"
        );
        assert!(
            !EnvState::host_shutdown_confirmed(&Ok(Err(HostError::cleanup_unconfirmed(
                "session retained"
            )))),
            "a typed host error must retain the owner, never report success"
        );
    }

    #[test]
    fn bounded_native_finalize_retains_live_localset_thread_owner() {
        use crate::cleanup_registry;

        // The registry is process-global; hold the test lock for the whole
        // test. The guard lives on this synchronous frame, so it never spans
        // an await point.
        let _lock = cleanup_registry::registry_test_lock();

        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("test runtime")
            .block_on(bounded_native_finalize_body());
    }

    /// Async body of the bounded-finalize retention test; the registry test
    /// lock is held by its synchronous wrapper.
    async fn bounded_native_finalize_body() {
        use crate::cleanup_registry;

        let state = Arc::new(EnvState::new());
        let host = Arc::new(HostManager::new());
        let bridge = host.localset_bridge();
        *state.host.lock().expect("host slot") = Some(host);

        // Block the LocalSet thread *synchronously* past the finalize budget:
        // the bounded inner join cannot settle inside the deadline, so the
        // owner must be retained and handed to tracked cleanup.
        let blocked = bridge.clone();
        let blocker = tokio::spawn(async move {
            let _ = blocked
                .execute(4, || {
                    Box::pin(async {
                        // Just past FINALIZE_BUDGET (5s): the bounded join cannot
                        // settle inside the deadline, but the thread does exit
                        // afterwards so tracked cleanup can join it.
                        std::thread::sleep(Duration::from_millis(6_500));
                        1
                    })
                })
                .await;
        });
        tokio::time::sleep(Duration::from_millis(250)).await;

        let finalize_state = state.clone();
        tokio::task::spawn_blocking(move || {
            EnvState::run_bounded_native_finalize(finalize_state);
        })
        .await
        .expect("finalize call");

        // The finalize worker is itself a tracked cleanup owner when it cannot
        // finish inside the budget; wait for it to settle so the assertions see
        // the post-finalize state, not a mid-flight one.
        let settle_deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < settle_deadline && !state.is_interrupted() {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        assert!(
            state.is_interrupted(),
            "an unsettled LocalSet teardown must leave the environment interrupted"
        );
        assert!(
            state.owner_slots_present(),
            "the host owner must be retained, never dropped as if cleanup succeeded"
        );
        assert!(
            !bridge.has_retained_runtime_thread(),
            "the retained handle must move into tracked cleanup, not stay detached"
        );
        let (tracked, _) = cleanup_registry::registry_snapshot();
        assert!(
            tracked >= 1,
            "the retained LocalSet thread must be tracked by the cleanup registry"
        );
        // No false Closed: the interrupted environment publishes no confirmed
        // close report, and the actionable reason is retained for the report.
        assert!(
            state.settled_close.lock().await.is_none(),
            "an unsettled finalize must not publish a settled close report"
        );
        let pending = state.pending_operations_snapshot().await;
        assert!(
            pending.iter().any(|entry| entry == "localset-thread-alive"),
            "the report must carry the actionable unsettled reason: {pending:?}"
        );

        blocker.abort();
        let _ = blocker.await;
        // The retained entry is reaped once the thread actually exits.
        let reap_deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < reap_deadline {
            cleanup_registry::reap_completed();
            if cleanup_registry::registry_snapshot().0 == 0 {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let (tracked, _) = cleanup_registry::registry_snapshot();
        assert_eq!(
            tracked, 0,
            "a settled LocalSet thread must be joined and removed by the reaper"
        );
    }

    #[test]
    fn bounded_native_finalize_completes_within_absolute_budget() {
        use crate::env_state::FINALIZE_BUDGET;
        use std::time::Instant;

        let state = Arc::new(EnvState::new());
        let started = Instant::now();
        state.tombstone_env();
        EnvState::run_bounded_native_finalize(state);
        let elapsed = started.elapsed();
        assert!(
            elapsed <= FINALIZE_BUDGET + Duration::from_millis(500),
            "native finalize must respect the absolute 5s budget, got {:?}",
            elapsed
        );
    }
}
