use std::path::{Path, PathBuf};

use std::time::{Duration, Instant};

const CLOSE_BUDGET: Duration = Duration::from_secs(5);
const CLOSE_CANCEL_PHASE: Duration = Duration::from_secs(2);

use std::sync::Arc;

use nexus_agent_host::capability::model::HostStartConfig;
use nexus_agent_host::capability::model::SessionOwner;
use nexus_agent_host::config::{
    agent_host_config_path, load_config_from_path, validate_workspace_path, AgentHostConfig,
};
use nexus_agent_host::core::readiness::discover_provider_catalog;
use nexus_agent_host::{HostError, HostFacade, HostManager, ProviderCatalogEntry};
use nexus_contracts::native_open_options::NativeOpenOptionsAccess;
use nexus_contracts::{CoreCloseReport, CoreCloseReportState, NativeOpenOptions};
use nexus_core::{CoreAccess, CoreError, CoreOpenOptions, CoreService, HostHandle};
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

/// The Host lifetime a close or a rollback claims.
///
/// An adopted open attaches the core Host authority (technical contract §3), so
/// the normal case is the attached authority: its `close_before` is the ONE
/// manager + `LocalSet` settlement and its `quiesce_actor_sessions` owns the
/// Actor drains. The unattached case exists only for a failed open whose manager
/// was never adopted — there is no authority to quiesce, so that rollback
/// settles the manager it started.
enum HostOwner {
    Attached(Arc<HostHandle>),
    Unattached(Arc<HostManager>),
}

/// A `LocalSet` thread a bounded settle could not join hands its handle to
/// tracked cleanup straight away, so a retained owner is never left with a
/// detached thread.
fn retain_unsettled_localset_thread(manager: &HostManager, state: &Arc<EnvState>) {
    if let Some(handle) = manager.localset_bridge().take_retained_runtime_thread() {
        super::cleanup_registry::register_localset_thread(handle, state.clone());
    }
}

async fn abort_opening(
    state: Arc<EnvState>,
    core: Option<Arc<CoreService>>,
    host: Option<HostOwner>,
    js_port: Option<Arc<dyn ProviderPort>>,
) {
    if core.is_some() || host.is_some() || js_port.is_some() {
        let (released, _) = cleanup_owners(
            state.clone(),
            core,
            host,
            js_port,
            Instant::now() + close_budget(),
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

    /// Injected cleanup latency, test builds only. It is injected AFTER the
    /// retained core is claimed, so a close cancelled on its budget is observed
    /// with the core in hand — the shape a held `CoreService::close` has.
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

    /// Test-only close-budget override, so timeout/retry checks stay short and
    /// deterministic (0 = the frozen production budget).
    #[cfg(test)]
    pub mod budget {
        use std::sync::atomic::{AtomicU64, Ordering};

        static CLOSE_BUDGET_MS: AtomicU64 = AtomicU64::new(0);

        pub fn set(ms: u64) {
            CLOSE_BUDGET_MS.store(ms, Ordering::SeqCst);
        }

        pub fn get() -> u64 {
            CLOSE_BUDGET_MS.load(Ordering::SeqCst)
        }
    }

    /// Test-only forcing of an unconfirmed ACTOR quiesce.
    ///
    /// The native fixture has no Actor lane — an unconfirmed verdict needs a
    /// supported cancel that failed, or an expired join, on a real Actor
    /// operation — so it is injected here to drive the ordered close's
    /// composition: the withheld core close, the authority retained as one unit
    /// with it, and the retry that must re-enter the quiesce instead of closing
    /// core. The path around the verdict stays production code.
    #[cfg(test)]
    pub mod quiesce {
        use std::sync::atomic::{AtomicBool, Ordering};

        static FORCE_QUIESCE_UNCONFIRMED: AtomicBool = AtomicBool::new(false);

        pub fn set(enable: bool) {
            FORCE_QUIESCE_UNCONFIRMED.store(enable, Ordering::SeqCst);
        }

        pub fn get() -> bool {
            FORCE_QUIESCE_UNCONFIRMED.load(Ordering::SeqCst)
        }
    }

    /// Test-only forcing of a failed Host attach, so the open rollback owner is
    /// exercised without forging an unadoptable core service.
    #[cfg(test)]
    pub mod attach {
        use std::sync::atomic::{AtomicBool, Ordering};

        static FORCE_ATTACH_FAILURE: AtomicBool = AtomicBool::new(false);

        pub fn set(enable: bool) {
            FORCE_ATTACH_FAILURE.store(enable, Ordering::SeqCst);
        }

        pub fn get() -> bool {
            FORCE_ATTACH_FAILURE.load(Ordering::SeqCst)
        }
    }

    /// The forcing seams above are process-global, so the tests that arm them
    /// must not interleave: this lock is their single serialization point.
    #[cfg(test)]
    pub mod seam_lock {
        use tokio::sync::{Mutex, MutexGuard};

        static LOCK: Mutex<()> = Mutex::const_new(());

        pub async fn acquire() -> MutexGuard<'static, ()> {
            LOCK.lock().await
        }
    }
}

/// Enable/disable forced unconfirmed cleanup.
pub fn set_force_unconfirmed(enable: bool) {
    forcing::set(enable);
}

/// Test-only: inject async delay after `cleanup_owners` claims the core.
#[cfg(test)]
pub fn set_force_cleanup_delay_ms(ms: u64) {
    forcing::delay::set(ms);
}

/// Test-only: override the close budget (0 = the frozen production budget).
#[cfg(test)]
pub fn set_force_close_budget_ms(ms: u64) {
    forcing::budget::set(ms);
}

/// Test-only: force the next Host attach to fail.
#[cfg(test)]
pub fn set_force_attach_failure(enable: bool) {
    forcing::attach::set(enable);
}

/// Test-only: force an unconfirmed Actor quiesce verdict (see
/// [`forcing::quiesce`]).
#[cfg(test)]
pub fn set_force_actor_quiesce_unconfirmed(enable: bool) {
    forcing::quiesce::set(enable);
}

/// The close budget: the frozen production value unless a test build overrides
/// it, so timeout checks do not have to wait out real seconds.
fn close_budget() -> Duration {
    #[cfg(test)]
    {
        let ms = forcing::budget::get();
        if ms > 0 {
            return Duration::from_millis(ms);
        }
    }
    CLOSE_BUDGET
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
    value: Option<HostOwner>,
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
            match v {
                HostOwner::Attached(authority) => {
                    let mut guard = self.state.host.lock().expect("core mutex poisoned");
                    if guard.is_none() {
                        *guard = Some(authority);
                    }
                }
                HostOwner::Unattached(manager) => {
                    // A manager the open never adopted has no Actor side and no
                    // published session, and there is no second slot that could
                    // retry it (that would be a second close owner). Its single
                    // settlement ran here; a thread it could not join is already
                    // in tracked cleanup, and the environment stays interrupted
                    // rather than reading an unconfirmed teardown as clean.
                    retain_unsettled_localset_thread(&manager, &self.state);
                    self.state.mark_interrupted();
                }
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
///
/// The order is technical contract §3: the attached authority quiesces its Actor
/// sessions while the core is still open, the existing core/execution owner
/// closes, and the SAME authority then settles the manager + `LocalSet` once
/// through `close_before` — never a second shutdown of that manager.
async fn cleanup_owners(
    state: Arc<EnvState>,
    inject_core: Option<Arc<CoreService>>,
    inject_host: Option<HostOwner>,
    inject_port: Option<Arc<dyn ProviderPort>>,
    deadline: Instant,
) -> (bool, CoreCloseReport) {
    let forced = forced_unconfirmed();
    let mut pending: Vec<String> = Vec::new();

    // Claim the Host lifetime first: the Actor quiesce runs BEFORE the core
    // owner closes, so the authority is still open while it joins its drains.
    let host_taken = match inject_host {
        Some(host) => Some(host),
        None => state.take_host().map(HostOwner::Attached),
    };
    let mut host_guard = RestoreHost {
        value: host_taken,
        state: state.clone(),
        disarmed: false,
    };

    // An Actor quiesce that did not confirm — an error, or the deadline above —
    // leaves an admitted Actor operation that may still register the drain the
    // quiesce's join bounds, so core storage must not close ahead of it.
    let mut actor_quiesce_confirmed = true;
    if let Some(HostOwner::Attached(authority)) = host_guard.value.as_ref() {
        // Bounded by the close's outer deadline: the join itself is deliberately
        // unbounded (the authority retains its drain ownership), so an expired
        // budget cancels this caller and keeps the whole authority instead of
        // detaching live work.
        match tokio::time::timeout_at(
            tokio::time::Instant::from_std(deadline),
            authority.quiesce_actor_sessions(),
        )
        .await
        {
            Ok(Ok(report)) => {
                actor_quiesce_confirmed = report.cleanup_confirmed;
                pending.extend(report.pending_operations);
            }
            Ok(Err(err)) => {
                actor_quiesce_confirmed = false;
                pending.push(format!("actor-quiesce: {err}"));
            }
            Err(_) => {
                actor_quiesce_confirmed = false;
                pending.push("actor-quiesce-deadline".to_string());
            }
        }
        // Test-only: the node fixture carries no Actor lane (an unconfirmed
        // verdict needs a supported cancel that failed, or an expired join, on a
        // real Actor operation), so the verdict is injected here to drive the
        // composition below. Everything else in this path stays production.
        #[cfg(test)]
        if forcing::quiesce::get() {
            actor_quiesce_confirmed = false;
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

    // The guard keeps its own reference for the WHOLE close, and the async work
    // borrows a clone of it: a cleanup cancelled inside the budget (the only way
    // out of a held close) then still hands its owner back through the guard's
    // `Drop` instead of dropping the retained core on the floor.
    let core_report = match core_guard.value.as_ref().map(Arc::clone) {
        Some(_) if !actor_quiesce_confirmed => {
            // Order from technical contract §3: the Actor side quiesces while
            // the core is still open, so an unconfirmed quiesce withholds the
            // core close entirely. The guard hands the core owner back (it is
            // never disarmed below, because the report is not released), the
            // environment reports the withheld close, and a retry re-runs the
            // ordered quiesce instead of releasing the SQL pool/writer fences
            // under a drain that is still about to be registered.
            pending.push("core-close-withheld: actor-quiesce-unconfirmed".to_string());
            interrupted_report(vec![])
        }
        Some(service) => {
            #[cfg(test)]
            {
                let delay_ms = forcing::delay::get();
                if delay_ms > 0 {
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                }
            }
            match service.close().await {
                Ok(report) => {
                    pending.extend(report.pending_operations.iter().cloned());
                    report
                }
                Err(_) => interrupted_report(vec![]),
            }
        }
        None => closed_report(),
    };
    let core_released = !forced && close_released(&core_report);
    if core_released {
        core_guard.disarm();
    }

    // Contract §3's close order is ONE sequence — Actor quiesce (while the core
    // authority is still open), the existing core/execution owner, then this
    // authority's manager + `LocalSet` settlement — so the authority is released
    // as the LAST step of that sequence, never on its own. A quiesce that could
    // not prove the Actor side (or a core close that did not settle) therefore
    // keeps BOTH owners: the state a withheld attempt leaves is "the authority is
    // still here", which is what makes the retry re-enter the ordered close —
    // quiesce included — instead of reading an absent authority as proof that the
    // Actor side settled and closing core with no Actor proof at all.
    let ordered_close_unconfirmed = !actor_quiesce_confirmed || !core_released;

    let host_released = match host_guard.value.as_ref() {
        Some(HostOwner::Attached(authority)) if !ordered_close_unconfirmed => {
            // ONE settlement of the attached manager + `LocalSet`, inside the
            // close's deadline, reporting its own unsettled work.
            //
            // Not wrapped in an inner timeout: `close_before` honors this same
            // absolute deadline itself, and an inner wrapper could cancel a
            // join that had already taken the `LocalSet` thread handle —
            // detaching the thread instead of handing it to tracked cleanup.
            // The whole cleanup stays bounded by the close's outer budget.
            let confirmed = match authority.close_before(deadline).await {
                Ok(report) => {
                    let confirmed = report.cleanup_confirmed;
                    pending.extend(report.pending_operations);
                    confirmed
                }
                Err(err) => {
                    pending.push(format!("host-close: {err}"));
                    false
                }
            };
            if !forced && confirmed {
                host_guard.disarm();
                true
            } else {
                let manager = authority.manager();
                retain_unsettled_localset_thread(&manager, &state);
                false
            }
        }
        Some(HostOwner::Attached(_)) => {
            // The ordered close has not reached this authority's settlement, so
            // the manager/`LocalSet` is not settled and the authority is not
            // discarded: it owns the Actor drains the retry must re-join, and the
            // guard hands it back when this attempt ends.
            pending.push("host-close-withheld: ordered-close-unconfirmed".to_string());
            false
        }
        Some(HostOwner::Unattached(manager)) => {
            // A failed open's manager: never adopted, so there is no authority
            // to quiesce and no Actor drain to settle. Freeze and settle the
            // `LocalSet` it started, then shut the manager down.
            manager.localset_bridge().begin_drain();
            let at_close = manager.localset_bridge().stats();
            let host_pending = manager.pending_lifecycle_snapshot().await;
            pending.extend(host_pending);
            let shutdown_ok = manager.shutdown().await.is_ok();
            let bridge_evidence = manager.localset_bridge().shutdown_before(deadline).await;
            // Settlement is a clean join with no live work — never `!thread_alive`
            // alone, which a panicked thread also satisfies.
            let bridge_settled = bridge_evidence.is_settled();
            let bridge_entries = manager
                .localset_bridge()
                .close_entries(&at_close, &bridge_evidence);
            if !forced && shutdown_ok && bridge_settled {
                pending.extend(bridge_entries);
                host_guard.disarm();
                true
            } else {
                // A join that could not settle inside the budget hands its
                // handle to tracked cleanup; a failed join has none to retain
                // but is still never released.
                retain_unsettled_localset_thread(manager, &state);
                pending.extend(bridge_entries);
                pending.extend(bridge_evidence.unsettled_pending());
                if !shutdown_ok {
                    pending.push("host-shutdown-unconfirmed".to_string());
                }
                false
            }
        }
        None => true,
    };

    if !pending.is_empty() {
        let mut recorded = state.pending_operations_snapshot().await;
        recorded.extend(pending);
        state.record_pending_operations(recorded).await;
    }

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
///
/// The retry is bounded by the SAME outer budget as the first close (R10): an
/// admitted durable commit may be applying, and `CoreService::close` drains it
/// through a retained task, so a retry that waits for it must not wait without
/// a budget of its own. Expiry cancels only this caller — the retained drain
/// keeps running, the owners stay retained, and the report stays interrupted
/// until a later retry observes the settlement that actually happened.
async fn settle_retained(state: Arc<EnvState>) -> (bool, CoreCloseReport) {
    let started = Instant::now();
    let deadline = started + close_budget();
    if !state.js_session_ids().is_empty() {
        release_js_provider_sessions(&state, started + CLOSE_CANCEL_PHASE).await;
    }
    // Bound the retry with the SAME outer budget as the first close, and bind
    // the result so the cancelled cleanup future (and the owner hand-back its
    // guards perform) is dropped before the retained-owner claim is read.
    let settled = tokio::time::timeout_at(
        tokio::time::Instant::from_std(deadline),
        cleanup_owners(state.clone(), None, None, None, deadline),
    )
    .await;
    match settled {
        Ok(settled) => settled,
        Err(_) => {
            state.mark_interrupted();
            let mut pending = state.pending_operations_snapshot().await;
            if state.owner_slots_present() {
                pending.push("cleanup-owners-retained".to_string());
            }
            state.record_pending_operations(pending.clone()).await;
            (false, interrupted_report(pending))
        }
    }
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
        // A retained Interrupted environment is settled through the SAME close
        // owner a close uses. A rival settle here could read the emptied owner
        // slot of an in-flight close as "already closed" and publish a new
        // owner over a cleanup that is still running.
        let report = close_core(state.clone()).await;
        if !report.cleanup_confirmed {
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

    let principal = match core.active_principal().await {
        Ok(principal) => principal,
        Err(err) => {
            let reason = open_err(err);
            abort_opening(state.clone(), Some(Arc::new(core)), None, js_port.clone()).await;
            return Err(reason);
        }
    };

    let host = Arc::new(HostManager::new());
    // Only the execution-owner profile carries the runtime edges: it binds the
    // ONE Host to the selected Creator's canonical creative workspace and runs
    // the bounded owner-bound readiness probes (the ordinary+sealed no-model
    // recipes) there. A domain-only / read-only open acquires neither that
    // boundary nor any provider probe — it keeps the open boundary it had.
    //
    // The root is the CORE'S OPEN-TIME PIN (`admission_creative_root`), not a
    // second read of the selection: the hosted factory composes its workspace
    // ports from that same pinned value, so a metadata write landing between
    // this probe and the owner's composition cannot bind the Host to one root
    // while the execution/commit authority binds another. An admission with no
    // usable pinned root gets NO probe owner: the Host keeps the open boundary
    // it already had and marks every selected candidate
    // `probe_context_unavailable`, so a selected provider can never be
    // reported ready off a fabricated boundary while the core's own factory
    // refuses the missing root (which would publish a ready lane over a null
    // engine epoch). Probing is bound to the pinned creative root or it does
    // not happen.
    let probe_owner = if access == CoreAccess::EngineOwner {
        core.admission_creative_root().map(|root| SessionOwner {
            creator_id: principal.creator_id().to_string(),
            workspace_root: root.to_path_buf(),
            orchestration_run_id: None,
        })
    } else {
        None
    };
    let workspace_root = probe_owner
        .as_ref()
        .map_or_else(|| user_home.clone(), |owner| owner.workspace_root.clone());
    let start_config = HostStartConfig {
        config_path: admission.config_path,
        workspace_root,
        max_sessions: admission.host_config.max_sessions,
        max_ops_per_session: admission.host_config.max_ops_per_session,
        timeouts: admission.host_config.timeouts.clone(),
        host_config: Some(admission.host_config),
        admitted_catalog: Some(admission.admitted_catalog),
        probe_owner,
    };
    if let Err(err) = host.start(start_config).await {
        let reason = core_error::open_reason_from_host(err);
        abort_opening(
            state.clone(),
            Some(Arc::new(core)),
            Some(HostOwner::Unattached(host)),
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
    // Adopt the started manager as the ONE core Host authority (contract §3
    // Open). The exact manager and port are retained: attach starts no second
    // Host and runs no second readiness probe, so the admitted pinned-root
    // configuration, the recipe admission over that manager, and the
    // provider-only JS tracking all stay the same instances. A failed attach
    // enters the same rollback owner as every other open failure.
    let authority = match attach_adopted_host(&core, host.clone(), provider_port.clone()) {
        Ok(authority) => Arc::new(authority),
        Err(err) => {
            let reason = open_err(err);
            abort_opening(
                state.clone(),
                Some(core),
                Some(HostOwner::Unattached(host)),
                js_port.clone(),
            )
            .await;
            return Err(reason);
        }
    };

    state.clear_service_only_uninitialized();
    // Settle any operation orphaned by the predecessor process's exit as
    // `interrupted` (LIFE-3) through the CoreService-owned journal before the
    // host accepts new work, so a restarted process never re-dispatches a
    // journaled op and the prior active op is queryable immediately. A failed
    // settlement cannot prove that recovery, so the open fails instead of
    // publishing a settled journal it does not have — and the rollback carries
    // the whole attached authority, not just its manager.
    if let Err(err) = core.settle_provider_orphans().await {
        let reason = format!("journal settlement failed: {err}");
        abort_opening(
            state.clone(),
            Some(core),
            Some(HostOwner::Attached(authority)),
            js_port.clone(),
        )
        .await;
        return Err(reason);
    }
    state
        .core
        .lock()
        .expect("core mutex poisoned")
        .replace(core);
    state
        .host
        .lock()
        .expect("host mutex poisoned")
        .replace(authority);
    state
        .provider_port
        .lock()
        .expect("port mutex poisoned")
        .replace(provider_port);
    state.publish_open().await;
    Ok(())
}

/// Adopt the started manager as the core Host authority.
///
/// The attach is the open's one adoption step; a test build can force it to fail
/// so the rollback owner is exercised end to end (the forced path performs no
/// adoption at all, which is exactly the shape the rollback must handle).
fn attach_adopted_host(
    core: &Arc<CoreService>,
    host: Arc<HostManager>,
    port: Arc<dyn ProviderPort>,
) -> Result<HostHandle, CoreError> {
    #[cfg(test)]
    if forcing::attach::get() {
        return Err(CoreError::Internal {
            category: "forced_attach_failure".into(),
        });
    }
    core.attach_host(host, port)
}

/// Wait for the close currently in flight to publish its report.
///
/// The waiter registers BEFORE it re-checks the in-flight boundary, so a
/// settlement that lands in between is still delivered — `notify_waiters`
/// leaves no permit behind for a late waiter, and registering first is what
/// makes this handoff lossless instead of able to strand a caller.
async fn await_settled_close(state: &Arc<EnvState>) -> CoreCloseReport {
    loop {
        let notified = state.close_notify_settled.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        if !*state.close_in_flight.lock().await {
            return state
                .settled_close
                .lock()
                .await
                .clone()
                .unwrap_or_else(|| interrupted_report(vec![]));
        }
        notified.await;
    }
}

/// The close body: release the JS sessions, then run ONE bounded cleanup over
/// the retained owners. The settlement owner publishes the result.
async fn run_close(state: Arc<EnvState>) -> CoreCloseReport {
    let started = Instant::now();
    let deadline = started + close_budget();

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
    if let Some(authority) = host_snapshot {
        state
            .record_pending_operations(authority.manager().pending_lifecycle_snapshot().await)
            .await;
    }

    // The `let` binds the timeout result so the cancelled cleanup future — and
    // with it the guards that hand every claimed owner back — is dropped BEFORE
    // the retained-owner claim below is evaluated.
    let settled = tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), async {
        let drain_budget = CLOSE_CANCEL_PHASE.min(close_budget().saturating_sub(started.elapsed()));
        if !drain_budget.is_zero() {
            tokio::time::sleep(Duration::from_millis(25).min(drain_budget)).await;
        }
        let (_, report) = cleanup_owners(state.clone(), None, None, None, deadline).await;
        report
    })
    .await;
    match settled {
        Ok(report) => report,
        Err(_) => {
            state.mark_interrupted();
            let mut pending = state.pending_operations_snapshot().await;
            if state.owner_slots_present() {
                pending.push("cleanup-owners-retained".to_string());
            }
            interrupted_report(pending)
        }
    }
}

/// The claimed settlement: replay a settled confirmed verdict, retry what a
/// previous close retained, otherwise close for the first time.
async fn settle_claimed(state: Arc<EnvState>) -> CoreCloseReport {
    let retained = state.settled_close.lock().await.clone();
    match retained {
        // A settlement landed while this close acquired ownership: report it
        // rather than re-run cleanup over an already settled environment.
        Some(settled) if settled.cleanup_confirmed => settled,
        // A previously interrupted close retained owners and possibly live JS
        // sessions. Retry the retained cleanup on this later close instead of
        // replaying the cached Interrupted verdict forever.
        Some(retained) => {
            let (released, retried) = settle_retained(state.clone()).await;
            if released {
                retried
            } else {
                retained
            }
        }
        None => run_close(state.clone()).await,
    }
}

/// Publish a settlement: the report first, then the boundary, then the waiters.
///
/// A waiter re-checks `close_in_flight` before it reads `settled_close`, so the
/// report must be visible by the time the boundary is released.
async fn publish_settled_close(state: &Arc<EnvState>, report: CoreCloseReport) {
    *state.settled_close.lock().await = Some(report.clone());
    *state.close_in_flight.lock().await = false;
    if !report.cleanup_confirmed {
        state.mark_interrupted();
    }
    state.close_notify_settled.notify_waiters();
}

/// The retained close owner: the ONE settlement of a claimed boundary.
///
/// It is a task of its own, so the caller that claimed the boundary can be
/// cancelled without cancelling the settlement: the retained owners stay
/// claimed by a live future until the cleanup really runs, and the boundary is
/// always released with a report for the waiters.
///
/// The settlement runs in its own task too, so a panic inside the cleanup (a
/// poisoned owner lock, a provider release that unwinds) is observed here as a
/// failed join — after the runtime has unwound it and its guards have handed
/// every claimed owner back. It publishes an unconfirmed Interrupted close: a
/// failed settlement never fabricates a confirmed one, and it never strands the
/// boundary or the waiters.
async fn own_close(state: Arc<EnvState>) {
    let report = match tokio::spawn(settle_claimed(state.clone())).await {
        Ok(report) => report,
        Err(_) => {
            let mut pending = state.pending_operations_snapshot().await;
            if state.owner_slots_present() {
                pending.push("cleanup-owners-retained".to_string());
            }
            interrupted_report(pending)
        }
    };
    publish_settled_close(&state, report).await;
}

/// Claim the close boundary and hand the settlement to its retained owner.
///
/// The claim and the handoff sit in ONE synchronous region of the boundary
/// lock: nothing is awaited between setting `close_in_flight` and spawning the
/// task that owns publication, so no cancellation can land in between and leave
/// the boundary claimed with nobody left to settle it.
async fn claim_close(state: &Arc<EnvState>) {
    let mut in_flight = state.close_in_flight.lock().await;
    if !*in_flight {
        *in_flight = true;
        tokio::spawn(own_close(state.clone()));
    }
}

/// Close the environment with a 5s phased budget and one concurrent settlement.
///
/// Requires a Tokio runtime context: the claimed settlement is owned by a
/// retained task rather than by this caller.
pub async fn close_core(state: Arc<EnvState>) -> CoreCloseReport {
    // A confirmed close is idempotent: nothing is left to retry.
    let cached = state.settled_close.lock().await.clone();
    if let Some(report) = cached {
        if report.cleanup_confirmed {
            return report;
        }
    }
    // ONE close owner at a time — for a fresh close AND for a retry of a
    // retained Interrupted verdict. A rival retry running concurrently could
    // find the core slot already emptied by the owner and read that absence as
    // "already closed", publishing a confirmed report over a settlement that is
    // still running.
    claim_close(&state).await;
    // Every caller — the one that claimed the settlement and every waiter that
    // arrived while it was in flight — is handed the report its owner published.
    await_settled_close(&state).await
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

        let _seams = forcing::seam_lock::acquire().await;
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

        let _seams = forcing::seam_lock::acquire().await;
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

        let authority = state.host_authority().expect("host authority");
        let catalog = authority
            .manager()
            .provider_catalog()
            .await
            .expect("catalog");
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

        let _seams = forcing::seam_lock::acquire().await;
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

    /// R10: a retried settlement is bounded by the SAME outer budget as the
    /// first close. While the cleanup is still blocked, a retry reports
    /// interrupted inside its own budget and RETAINS the owners/lease; it
    /// confirms only after the block is gone and the real settlement ran.
    #[tokio::test]
    async fn settle_retained_retry_is_bounded_and_honest() {
        use crate::wire_fixture::seed_wire_home;
        use nexus_contracts::native_open_options::NativeOpenOptionsAccess;
        use nexus_contracts::NativeOpenOptions;
        use tempfile::tempdir;

        // The forcing seams are process-global: hold their lock for the whole
        // case so no concurrent close test can reset the block mid-phase.
        let _seams = forcing::seam_lock::acquire().await;
        let dir = tempdir().expect("tempdir");
        seed_wire_home(dir.path()).await;
        let state = Arc::new(EnvState::new());
        // A short deterministic budget (the production value stays 5 s) with a
        // cleanup block far past it keeps this timeout check fast.
        set_force_close_budget_ms(300);
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

        // First close: expiry inside its budget keeps the owners retained.
        let first = close_core(state.clone()).await;
        assert_eq!(first.state, CoreCloseReportState::Interrupted);
        assert!(!first.cleanup_confirmed);
        assert!(
            first
                .pending_operations
                .iter()
                .any(|entry| entry == "cleanup-owners-retained"),
            "the expired close must name its retained owners: {first:?}"
        );
        assert!(state.owner_slots_present(), "expiry must retain the owners");

        // The retried settlement (the cached-interrupted path): still blocked,
        // so it must return interrupted inside ITS OWN budget — not wait out
        // the block — and keep the owners and the lease retained.
        let retried_at = Instant::now();
        let second = close_core(state.clone()).await;
        let retried_in = retried_at.elapsed();
        assert_eq!(second.state, CoreCloseReportState::Interrupted);
        assert!(!second.cleanup_confirmed);
        assert!(
            state.owner_slots_present(),
            "the bounded retry must retain the owners, never release on expiry"
        );
        assert!(
            retried_in < Duration::from_secs(2),
            "the retry must be bounded by its own budget, took {retried_in:?}"
        );

        // With the block gone the retry confirms only the real settlement.
        set_force_cleanup_delay_ms(0);
        set_force_close_budget_ms(0);
        let third = close_core(state.clone()).await;
        assert_eq!(third.state, CoreCloseReportState::Closed, "{third:?}");
        assert!(third.cleanup_confirmed);
        assert!(!state.owner_slots_present());

        // The settled environment admits a real owner again.
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
        .expect("reopen after settlement");
        let final_report = close_core(state.clone()).await;
        assert_eq!(final_report.state, CoreCloseReportState::Closed);
        assert!(final_report.cleanup_confirmed);
    }

    /// C2/C3: concurrent close callers share ONE settlement. A rival retry must
    /// never read an emptied owner slot as "already closed" and publish a
    /// confirmed report over the settlement still running, and a caller that
    /// arrives while a close is in flight must be handed that close's real
    /// report inside its budget instead of sleeping through the notification.
    #[tokio::test]
    async fn concurrent_closes_share_one_settlement() {
        use crate::wire_fixture::seed_wire_home;
        use nexus_contracts::native_open_options::NativeOpenOptionsAccess;
        use nexus_contracts::NativeOpenOptions;
        use tempfile::tempdir;

        let _seams = forcing::seam_lock::acquire().await;
        let dir = tempdir().expect("tempdir");
        seed_wire_home(dir.path()).await;
        let state = Arc::new(EnvState::new());
        // A budget wide enough for a legitimate cleanup to run, and a block far
        // past it: a rival that helped itself to an already-claimed core could
        // then complete the remaining legs and confirm.
        set_force_close_budget_ms(1_200);
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

        // A first close that expires inside its budget retains the owners.
        let first = close_core(state.clone()).await;
        assert!(!first.cleanup_confirmed, "{first:?}");
        assert!(state.owner_slots_present());

        // Three concurrent callers over the retained, STILL BLOCKED cleanup.
        // Exactly one owns the settlement; the others must be handed its real
        // report. None may upgrade an emptied owner slot into a confirmation.
        let joined_at = Instant::now();
        let (a, b, c) = tokio::join!(
            close_core(state.clone()),
            close_core(state.clone()),
            close_core(state.clone())
        );
        let joined_in = joined_at.elapsed();
        for (label, report) in [("a", &a), ("b", &b), ("c", &c)] {
            assert_eq!(
                report.state,
                CoreCloseReportState::Interrupted,
                "{label} must report the blocked cleanup: {report:?}"
            );
            assert!(
                !report.cleanup_confirmed,
                "{label} fabricated a confirmed close over a running settlement: {report:?}"
            );
        }
        assert!(
            state.owner_slots_present(),
            "every owner must stay retained while the cleanup is blocked"
        );
        assert!(
            state.core.lock().expect("core mutex poisoned").is_some(),
            "the retained core must stay in its slot"
        );
        assert!(
            state.host.lock().expect("host mutex poisoned").is_some(),
            "the retained host must stay in its slot"
        );
        assert!(
            joined_in < Duration::from_secs(3),
            "every caller must return inside the close budget, took {joined_in:?}"
        );

        // Only a settlement that really ran may confirm.
        set_force_cleanup_delay_ms(0);
        set_force_close_budget_ms(0);
        let settled = close_core(state.clone()).await;
        assert_eq!(settled.state, CoreCloseReportState::Closed, "{settled:?}");
        assert!(settled.cleanup_confirmed);
        assert!(!state.owner_slots_present());
        assert!(state.core.lock().expect("core mutex poisoned").is_none());
        assert!(state.host.lock().expect("host mutex poisoned").is_none());
    }

    /// The frozen L2 Critical: the boundary belongs to the claimed settlement,
    /// not to the caller that claimed it.
    ///
    /// A close caller DROPPED after the claim (the waiter is cancelled) must not
    /// strand the environment: the settlement still runs to publication, later
    /// closers are handed that real report inside their own budget, and an
    /// interrupted open is released with the honest verdict instead of hanging on
    /// a boundary nobody will release. An unsettled close retains the whole
    /// attached authority, never a bare manager.
    #[tokio::test]
    async fn native_actor_owner_dropped_close_waiter_keeps_the_settlement_owner() {
        use crate::wire_fixture::seed_wire_home;
        use nexus_contracts::native_open_options::NativeOpenOptionsAccess;
        use nexus_contracts::NativeOpenOptions;
        use tempfile::tempdir;

        // The forcing seams are process-global: hold their lock for the whole
        // case so no concurrent close test can reset the block mid-phase.
        let _seams = forcing::seam_lock::acquire().await;
        let dir = tempdir().expect("tempdir");
        seed_wire_home(dir.path()).await;
        let state = Arc::new(EnvState::new());
        let options = || NativeOpenOptions {
            user_home: dir.path().to_string_lossy().to_string(),
            access: NativeOpenOptionsAccess::EngineOwner,
            allow_uninitialized: false,
        };
        // A short deterministic budget (the production value stays 5 s) with a
        // cleanup block far past it: the claimed close cannot finish by itself.
        set_force_close_budget_ms(300);
        set_force_cleanup_delay_ms(10_000);
        open_core(state.clone(), options(), None)
            .await
            .expect("open");
        assert!(state.owner_slots_present());

        // Claim, then cancel the OUTER caller. The rendezvous is the claim
        // itself, so the cancellation lands with the boundary already taken and
        // no report published yet.
        let caller = tokio::spawn(close_core(state.clone()));
        let claim_deadline = Instant::now() + Duration::from_secs(5);
        while !*state.close_in_flight.lock().await {
            assert!(
                Instant::now() < claim_deadline,
                "the close never claimed the boundary"
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        caller.abort();
        assert!(
            caller.await.is_err(),
            "the outer caller must really have been cancelled, not completed"
        );

        // The settlement owner publishes on its own: the boundary is released
        // without the cancelled caller.
        let settle_deadline = Instant::now() + Duration::from_secs(2);
        while *state.close_in_flight.lock().await {
            assert!(
                Instant::now() < settle_deadline,
                "a cancelled caller stranded the close boundary"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let published = state
            .settled_close
            .lock()
            .await
            .clone()
            .expect("the surviving settlement must publish a report");
        assert_eq!(
            published.state,
            CoreCloseReportState::Interrupted,
            "the blocked cleanup must publish the honest verdict: {published:?}"
        );
        assert!(!published.cleanup_confirmed);
        assert!(
            state.owner_slots_present(),
            "an interrupted settlement retains its owners"
        );
        assert!(
            state.host_authority().is_some(),
            "and it retains the ATTACHED authority, never a bare manager"
        );

        // A later closer is handed that report inside its own budget instead of
        // waiting on a boundary nobody will ever release.
        let second = tokio::time::timeout(Duration::from_secs(3), close_core(state.clone()))
            .await
            .expect("a cancelled caller must not strand a later closer");
        assert_eq!(
            second.state,
            CoreCloseReportState::Interrupted,
            "{second:?}"
        );
        assert!(
            !second.cleanup_confirmed,
            "no caller may confirm a cleanup that never ran: {second:?}"
        );
        assert!(state.owner_slots_present());

        // An interrupted open routes through the same settlement owner and is
        // released with the verdict rather than hung on the boundary.
        let interrupted_open = tokio::time::timeout(
            Duration::from_secs(3),
            open_core(state.clone(), options(), None),
        )
        .await
        .expect("an interrupted open must not hang on the close boundary");
        assert!(
            interrupted_open.is_err(),
            "an unconfirmed cleanup must not admit a new owner"
        );

        // Only the settlement that really ran may confirm, and the settled
        // environment admits a new owner again.
        set_force_cleanup_delay_ms(0);
        set_force_close_budget_ms(0);
        let settled = close_core(state.clone()).await;
        assert_eq!(settled.state, CoreCloseReportState::Closed, "{settled:?}");
        assert!(settled.cleanup_confirmed);
        assert!(!state.owner_slots_present());
        open_core(state.clone(), options(), None)
            .await
            .expect("a settled environment admits a new owner");
        let reopened = close_core(state.clone()).await;
        assert_eq!(reopened.state, CoreCloseReportState::Closed, "{reopened:?}");
        assert!(reopened.cleanup_confirmed);
    }

    /// A panic inside the cleanup is not a licence to fabricate a close or to
    /// strand the boundary: the retained owner publishes an unconfirmed verdict
    /// with every owner still retained, and a later close confirms only the
    /// cleanup that really ran.
    #[tokio::test]
    async fn panicking_settlement_owner_publishes_unconfirmed() {
        use crate::wire_fixture::seed_wire_home;
        use async_trait::async_trait;
        use nexus_contracts::native_open_options::NativeOpenOptionsAccess;
        use nexus_contracts::{
            CoreError, CoreErrorCode, NativeOpenOptions, ProviderCall, ProviderEventBatch,
            ProviderReply,
        };
        use nexus_provider_ports::{ProviderPort, ProviderResult};
        use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
        use tempfile::tempdir;

        /// A provider whose release unwinds: the shape of an adapter that panics
        /// inside the close's release window. It stops unwinding once disarmed,
        /// so the retry can settle the session it left behind.
        struct PanickingReleasePort {
            armed: AtomicBool,
        }

        #[async_trait]
        impl ProviderPort for PanickingReleasePort {
            async fn call(&self, request: ProviderCall) -> ProviderResult<ProviderReply> {
                if self.armed.load(AtomicOrdering::SeqCst) {
                    panic!("provider release panics while armed");
                }
                Ok(ProviderReply {
                    request_id: request.request_id.clone(),
                    ok: true,
                    session_id: request.session_id.clone(),
                    operation_id: None,
                    health: None,
                    error: None,
                })
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

        let _seams = forcing::seam_lock::acquire().await;
        let dir = tempdir().expect("tempdir");
        seed_wire_home(dir.path()).await;
        let state = Arc::new(EnvState::new());
        let options = || NativeOpenOptions {
            user_home: dir.path().to_string_lossy().to_string(),
            access: NativeOpenOptionsAccess::EngineOwner,
            allow_uninitialized: false,
        };
        let port = Arc::new(PanickingReleasePort {
            armed: AtomicBool::new(true),
        });
        open_core(state.clone(), options(), Some(port.clone()))
            .await
            .expect("open");
        state.record_js_session("sess-panic".to_string(), "mock-provider".to_string());
        assert!(state.owner_slots_present());

        let report = tokio::time::timeout(Duration::from_secs(5), close_core(state.clone()))
            .await
            .expect("a panicking settlement must still publish");
        assert_eq!(
            report.state,
            CoreCloseReportState::Interrupted,
            "a panic must fail unconfirmed, never closed: {report:?}"
        );
        assert!(!report.cleanup_confirmed);
        assert!(
            !*state.close_in_flight.lock().await,
            "a panicking settlement must still release the boundary"
        );
        assert!(
            state.owner_slots_present(),
            "a panicked cleanup retains the core/host owners"
        );
        assert_eq!(
            state.js_session_ids(),
            vec!["sess-panic".to_string()],
            "the session whose release panicked stays registered"
        );

        // Disarm the port: only the cleanup that really runs may confirm.
        port.armed.store(false, AtomicOrdering::SeqCst);
        let settled = tokio::time::timeout(Duration::from_secs(5), close_core(state.clone()))
            .await
            .expect("the retry must not hang");
        assert_eq!(settled.state, CoreCloseReportState::Closed, "{settled:?}");
        assert!(settled.cleanup_confirmed);
        assert!(state.js_session_ids().is_empty());
        assert!(!state.owner_slots_present());
        open_core(state.clone(), options(), None)
            .await
            .expect("a settled environment admits a new owner");
        let _ = close_core(state.clone()).await;
    }

    #[tokio::test]
    async fn close_timeout_retains_owners_settle_then_reopen() {
        use crate::wire_fixture::seed_wire_home;
        use nexus_contracts::native_open_options::NativeOpenOptionsAccess;
        use nexus_contracts::NativeOpenOptions;
        use tempfile::tempdir;

        // The cleanup forcing seams are process-global: hold their lock so a
        // concurrent close test cannot reset the injected block mid-phase.
        let _seams = forcing::seam_lock::acquire().await;
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

    /// F-001 (plan QC fix round 2): contract §3's close order is ONE sequence,
    /// so a quiesce that could not prove the Actor side must keep the WHOLE
    /// authority — core AND host. Discarding the authority on its own settlement
    /// while the core is retained leaves a retry with nothing to re-enter the
    /// quiesce with: it reads the absent authority as "the Actor side settled"
    /// and closes core storage with no Actor proof at all.
    ///
    /// The native fixture carries no Actor lane (the unconfirmed verdict needs a
    /// supported cancel that failed, or an expired join, on a real Actor
    /// operation), so the verdict is injected through the `forcing::quiesce`
    /// seam. Everything else here is the production path: the real core service,
    /// the real attached authority and manager/`LocalSet` settlement, the
    /// bounded retry through `close_core`'s retained-cleanup owner, and the
    /// final confirmed completion.
    #[tokio::test]
    async fn native_retry_reenters_the_actor_quiesce_instead_of_closing_core() {
        use crate::wire_fixture::seed_wire_home;
        use nexus_contracts::native_open_options::NativeOpenOptionsAccess;
        use nexus_contracts::NativeOpenOptions;
        use tempfile::tempdir;

        // The forcing seams are process-global: hold their lock for the whole
        // case so no concurrent close test can reset the injected verdict.
        let _seams = forcing::seam_lock::acquire().await;
        let dir = tempdir().expect("tempdir");
        seed_wire_home(dir.path()).await;
        let state = Arc::new(EnvState::new());
        let options = || NativeOpenOptions {
            user_home: dir.path().to_string_lossy().to_string(),
            access: NativeOpenOptionsAccess::EngineOwner,
            allow_uninitialized: false,
        };
        open_core(state.clone(), options(), None)
            .await
            .expect("open");
        assert!(state.owner_slots_present());
        set_force_actor_quiesce_unconfirmed(true);

        // The unconfirmed quiesce withholds the core close, keeps the authority
        // for the retry, and reports the interruption.
        let first = close_core(state.clone()).await;
        assert_eq!(first.state, CoreCloseReportState::Interrupted, "{first:?}");
        assert!(!first.cleanup_confirmed);
        assert!(
            state.core.lock().expect("core mutex poisoned").is_some(),
            "the withheld core stays in its slot"
        );

        // The retry MUST re-enter the quiesce: an independent owner discard in
        // the first attempt is exactly what let a retry with no attached
        // authority close core here, confirming a close whose Actor side was
        // never proven.
        let retry_had_authority = state.host_authority().is_some();
        let retry = close_core(state.clone()).await;
        assert!(
            !retry.cleanup_confirmed,
            "a retry may not close core with the Actor side unproven \
             (attached authority at retry: {retry_had_authority}): {retry:?}"
        );
        assert!(
            state.core.lock().expect("core mutex poisoned").is_some(),
            "the core stays retained while the Actor side is unproven"
        );
        assert!(
            state.host_authority().is_some(),
            "the authority stays retained with the core it belongs to"
        );
        assert!(
            first
                .pending_operations
                .iter()
                .any(|entry| entry.starts_with("host-close-withheld:")),
            "the first attempt names the authority it withheld: {first:?}"
        );

        // With the Actor side provable again the SAME retained state completes
        // as one coherent teardown: quiesce, core close, manager settlement.
        set_force_actor_quiesce_unconfirmed(false);
        let settled = close_core(state.clone()).await;
        assert_eq!(settled.state, CoreCloseReportState::Closed, "{settled:?}");
        assert!(settled.cleanup_confirmed, "{settled:?}");
        assert!(
            !state.owner_slots_present(),
            "the settled close released every owner"
        );
        open_core(state.clone(), options(), None)
            .await
            .expect("a settled environment admits a new owner");
        let _ = close_core(state.clone()).await;
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

        let _seams = forcing::seam_lock::acquire().await;
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

    // ── `native_actor_owner_`: native adoption of the attached Host authority.
    //
    // One manager/epoch, rollback on a failed attach, Actor + provider-only
    // close, a timed-out close retained then retried, and the dropped-waiter /
    // environment-finalizer guards — all over the REAL native open/close paths.

    /// A JS-provider fixture that records every adapter call and acknowledges
    /// each one — the shape of a live TS adapter at the native boundary.
    #[derive(Default)]
    struct RecordingJsPort {
        calls: std::sync::Mutex<Vec<nexus_contracts::provider_call::ProviderCallMethod>>,
    }

    impl RecordingJsPort {
        fn calls(&self) -> Vec<nexus_contracts::provider_call::ProviderCallMethod> {
            self.calls.lock().expect("calls").clone()
        }
    }

    #[async_trait::async_trait]
    impl ProviderPort for RecordingJsPort {
        async fn call(
            &self,
            request: nexus_contracts::ProviderCall,
        ) -> nexus_provider_ports::ProviderResult<nexus_contracts::ProviderReply> {
            self.calls.lock().expect("calls").push(request.method);
            Ok(nexus_contracts::ProviderReply {
                request_id: request.request_id.clone(),
                ok: true,
                session_id: request.session_id.clone(),
                operation_id: None,
                health: None,
                error: None,
            })
        }

        async fn next(
            &self,
            _operation_id: String,
            _max_events: u32,
            _max_bytes: u32,
        ) -> nexus_provider_ports::ProviderResult<nexus_contracts::ProviderEventBatch> {
            Err(nexus_contracts::CoreError {
                code: nexus_contracts::CoreErrorCode::Internal,
                message: "not used".into(),
                details: Default::default(),
                http_status: Some(500),
            })
        }
    }

    /// Open `state` as the engine owner over a seeded disposable home.
    async fn open_seeded(
        state: &Arc<EnvState>,
        home: &Path,
        js_port: Option<Arc<dyn ProviderPort>>,
    ) -> Result<(), String> {
        open_core(
            state.clone(),
            NativeOpenOptions {
                user_home: home.to_string_lossy().to_string(),
                access: NativeOpenOptionsAccess::EngineOwner,
                allow_uninitialized: false,
            },
            js_port,
        )
        .await
    }

    #[tokio::test]
    async fn native_actor_owner_open_adopts_one_configured_manager() {
        use crate::wire_fixture::seed_wire_home;
        use nexus_agent_host::config::{agent_host_config_path, load_config_from_path};
        use nexus_agent_host::core::readiness::discover_provider_catalog;
        use tempfile::tempdir;

        let _seams = forcing::seam_lock::acquire().await;
        let dir = tempdir().expect("tempdir");
        seed_wire_home(dir.path()).await;
        // A distinctive ADMITTED document: the manager in the slot must be the
        // one this open started and configured, never a second, defaulted Host.
        let config_path = agent_host_config_path(dir.path());
        std::fs::create_dir_all(config_path.parent().expect("config dir")).expect("agent-host dir");
        std::fs::write(&config_path, "max_sessions = 7\n").expect("write host config");
        let expected =
            discover_provider_catalog(&load_config_from_path(&config_path).expect("host config"))
                .expect("admit catalog");

        let state = Arc::new(EnvState::new());
        open_seeded(&state, dir.path(), None).await.expect("open");

        let authority = state.host_authority().expect("attached authority");
        let manager = authority.manager();
        assert_eq!(
            manager.agent_config().await.max_sessions,
            7,
            "the Host slot holds the manager this open started with the admitted document"
        );
        assert_eq!(
            manager
                .provider_catalog()
                .await
                .expect("catalog")
                .entries
                .len(),
            expected.len(),
            "the admitted pinned-root catalog survives adoption unchanged"
        );
        assert!(
            manager.health().await.expect("health").running,
            "the adopted manager is the started one"
        );

        // ONE owner and ONE settlement: a confirmed close releases the Host slot
        // and stops that manager; a repeated close replays the settled verdict.
        let first = close_core(state.clone()).await;
        assert_eq!(first.state, CoreCloseReportState::Closed, "{first:?}");
        assert!(first.cleanup_confirmed, "{first:?}");
        assert!(
            state.host_authority().is_none(),
            "a confirmed close releases the Host authority slot"
        );
        assert!(
            !manager.health().await.expect("health").running,
            "the ONE adopted manager was settled"
        );
        let second = close_core(state.clone()).await;
        assert_eq!(second.state, CoreCloseReportState::Closed, "{second:?}");
        assert!(second.cleanup_confirmed);
    }

    #[tokio::test]
    async fn native_actor_owner_failed_attach_rolls_back_to_closed() {
        use crate::cleanup_registry;
        use crate::env_state::EnvLifecyclePhase;
        use crate::wire_fixture::seed_wire_home;
        use tempfile::tempdir;

        let _seams = forcing::seam_lock::acquire().await;
        let dir = tempdir().expect("tempdir");
        seed_wire_home(dir.path()).await;
        let state = Arc::new(EnvState::new());

        // The attach is the open's one adoption step; force it to fail so the
        // rollback owner runs over the manager the open already started.
        set_force_attach_failure(true);
        let failed = open_seeded(&state, dir.path(), None).await;
        set_force_attach_failure(false);
        assert!(failed.is_err(), "a failed attach fails the open");
        assert_eq!(state.lifecycle_phase(), EnvLifecyclePhase::Closed);
        assert!(
            !state.owner_slots_present(),
            "the rollback retains no owner: {failed:?}"
        );
        assert!(
            !state.is_interrupted(),
            "a rollback whose settlement confirmed is not interrupted"
        );
        assert_eq!(
            cleanup_registry::registry_snapshot().0,
            0,
            "the rolled-back manager leaves no tracked LocalSet thread"
        );

        // The rolled-back environment admits a real owner again.
        open_seeded(&state, dir.path(), None)
            .await
            .expect("reopen after a rolled-back attach");
        assert!(state.host_authority().is_some());
        let closed = close_core(state.clone()).await;
        assert_eq!(closed.state, CoreCloseReportState::Closed, "{closed:?}");
        assert!(closed.cleanup_confirmed);
    }

    #[tokio::test]
    async fn native_actor_owner_close_settles_actor_and_provider_only_owners() {
        use crate::wire_fixture::seed_wire_home;
        use nexus_contracts::provider_call::ProviderCallMethod;
        use tempfile::tempdir;

        let _seams = forcing::seam_lock::acquire().await;
        let dir = tempdir().expect("tempdir");
        seed_wire_home(dir.path()).await;
        let state = Arc::new(EnvState::new());
        let js = Arc::new(RecordingJsPort::default());
        open_seeded(&state, dir.path(), Some(js.clone()))
            .await
            .expect("open");
        let authority = state.host_authority().expect("authority");
        let manager = authority.manager();
        let principal = state
            .journal_core()
            .expect("core")
            .active_principal()
            .await
            .expect("principal");

        // Provider-only tracking: a launched JS session with a live operation,
        // recorded at the one seam the admitting port writes.
        state.record_js_session("sess-js".to_string(), "mock-acp".to_string());
        state.record_js_session_operation("sess-js", "op-js".to_string());
        assert_eq!(state.js_session_ids(), vec!["sess-js".to_string()]);

        let report = close_core(state.clone()).await;
        assert_eq!(report.state, CoreCloseReportState::Closed, "{report:?}");
        assert!(report.cleanup_confirmed, "{report:?}");

        // Provider-only half: the adapter was driven (cancel for the live
        // operation, then the release) and the session is forgotten.
        let calls = js.calls();
        assert!(
            calls.contains(&ProviderCallMethod::Cancel),
            "the live JS operation was cancelled: {calls:?}"
        );
        assert!(
            calls.contains(&ProviderCallMethod::Shutdown),
            "the JS-owned child was released: {calls:?}"
        );
        assert!(
            state.js_session_ids().is_empty(),
            "a confirmed close forgets the released JS session"
        );
        assert!(
            state.provider_port.lock().expect("port").is_none(),
            "the provider-only owner is released"
        );

        // Actor half: the Host authority slot is released, its manager settled,
        // and no Actor effect is admitted any more.
        assert!(
            state.host_authority().is_none(),
            "the Actor authority is released"
        );
        assert!(
            !manager.health().await.expect("health").running,
            "the Actor manager is settled"
        );
        let refused = authority
            .character_operation(
                &principal,
                "00000000-0000-0000-0000-0000000000aa".to_string(),
            )
            .await;
        assert!(
            matches!(refused, Err(nexus_core::CoreError::Closing)),
            "a closed authority refuses Actor reads: {refused:?}"
        );
    }

    #[tokio::test]
    async fn native_actor_owner_timed_out_close_retains_then_retries() {
        use crate::wire_fixture::seed_wire_home;
        use tempfile::tempdir;

        let _seams = forcing::seam_lock::acquire().await;
        let dir = tempdir().expect("tempdir");
        seed_wire_home(dir.path()).await;
        let state = Arc::new(EnvState::new());
        set_force_close_budget_ms(300);
        set_force_cleanup_delay_ms(10_000);
        open_seeded(&state, dir.path(), None).await.expect("open");
        let manager = state.host_authority().expect("authority").manager();

        let timed_out = close_core(state.clone()).await;
        assert_eq!(
            timed_out.state,
            CoreCloseReportState::Interrupted,
            "{timed_out:?}"
        );
        assert!(!timed_out.cleanup_confirmed);
        assert!(
            state.host_authority().is_some(),
            "an expired close retains the WHOLE authority, never a bare manager"
        );
        assert!(
            state.core.lock().expect("core mutex poisoned").is_some(),
            "and the existing core owner with it"
        );
        assert!(
            manager.health().await.expect("health").running,
            "the retained authority keeps its manager running"
        );

        // Only the settlement that really ran may confirm.
        set_force_cleanup_delay_ms(0);
        set_force_close_budget_ms(0);
        let settled = close_core(state.clone()).await;
        assert_eq!(settled.state, CoreCloseReportState::Closed, "{settled:?}");
        assert!(settled.cleanup_confirmed);
        assert!(!state.owner_slots_present());
        assert!(!manager.health().await.expect("health").running);
    }

    /// The environment-finalizer path retains its guards on the ATTACHED
    /// authority: an unsettled `LocalSet` teardown keeps the authority (and its
    /// manager/Actor drains) instead of reporting a clean close, the unjoinable
    /// thread moves into tracked cleanup rather than staying detached, and — the
    /// dead-environment rule — the finalize never invokes a JS callback.
    #[test]
    fn native_actor_owner_finalizer_retains_guards_without_invoking_js() {
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
        use crate::wire_fixture::seed_wire_home;
        use tempfile::tempdir;

        let dir = tempdir().expect("tempdir");
        seed_wire_home(dir.path()).await;
        let state = Arc::new(EnvState::new());
        let js = Arc::new(RecordingJsPort::default());
        open_seeded(&state, dir.path(), Some(js.clone()))
            .await
            .expect("open");
        // The finalize must carry the ATTACHED authority, so the blocked
        // `LocalSet` is the one the authority's manager owns.
        let authority = state.host_authority().expect("attached authority");
        let bridge = authority.manager().localset_bridge();

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
            state.host_authority().is_some(),
            "the finalize retains the ATTACHED authority, not a detached manager"
        );
        assert!(
            js.calls().is_empty(),
            "dead-environment cleanup must never invoke a JS callback: {:?}",
            js.calls()
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
