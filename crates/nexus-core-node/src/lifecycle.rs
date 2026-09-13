use std::path::PathBuf;

use std::time::{Duration, Instant};

const CLOSE_BUDGET: Duration = Duration::from_secs(5);
const CLOSE_CANCEL_PHASE: Duration = Duration::from_secs(2);

use std::sync::Arc;

use nexus_agent_host::capability::model::HostStartConfig;
use nexus_agent_host::config::AgentHostConfig;
use nexus_agent_host::{HostFacade, HostManager};
use nexus_contracts::native_open_options::NativeOpenOptionsAccess;
use nexus_contracts::{CoreCloseReport, CoreCloseReportState, NativeOpenOptions};
use nexus_core::{CoreAccess, CoreOpenOptions, CoreService};
use nexus_provider_ports::ProviderPort;

use super::admitting_provider_port::AdmittingProviderPort;

use super::env_state::EnvState;

/// Test-only forcing of an unconfirmed cleanup (debug builds only).
#[cfg(debug_assertions)]
mod forcing {
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

    static FORCE_UNCONFIRMED: AtomicBool = AtomicBool::new(false);
    static CLEANUP_DELAY_MS: AtomicU64 = AtomicU64::new(0);

    pub fn set(enable: bool) {
        FORCE_UNCONFIRMED.store(enable, Ordering::SeqCst);
    }

    pub fn get() -> bool {
        FORCE_UNCONFIRMED.load(Ordering::SeqCst)
    }

    pub fn set_cleanup_delay_ms(ms: u64) {
        CLEANUP_DELAY_MS.store(ms, Ordering::SeqCst);
    }

    pub fn cleanup_delay_ms() -> u64 {
        CLEANUP_DELAY_MS.load(Ordering::SeqCst)
    }
}

/// Enable/disable forced unconfirmed cleanup. Debug builds only.
#[cfg(debug_assertions)]
pub fn set_force_unconfirmed(enable: bool) {
    forcing::set(enable);
}

/// Debug-only: inject async delay at the start of `cleanup_owners`.
#[cfg(debug_assertions)]
pub fn set_force_cleanup_delay_ms(ms: u64) {
    forcing::set_cleanup_delay_ms(ms);
}

#[cfg(debug_assertions)]
fn forced_unconfirmed() -> bool {
    forcing::get()
}

#[cfg(not(debug_assertions))]
const fn forced_unconfirmed() -> bool {
    false
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
            let mut guard = self.state.provider_port.lock().expect("core mutex poisoned");
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

    #[cfg(debug_assertions)]
    {
        let delay_ms = forcing::cleanup_delay_ms();
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
            let pending = manager.pending_lifecycle_snapshot().await;
            state.record_pending_operations(pending).await;
            let shutdown_ok = manager.shutdown().await.is_ok();
            let bridge_evidence = manager.localset_bridge().shutdown_before(deadline).await;
            let bridge_settled = bridge_evidence.joined_cleanly
                && bridge_evidence.active_tasks == 0
                && bridge_evidence.pending_requests == 0;
            if !forced && shutdown_ok && bridge_settled {
                host_guard.disarm();
                true
            } else {
                let mut pending = state.pending_operations_snapshot().await;
                for task_id in bridge_evidence.aborted_task_ids {
                    pending.push(format!("localset-task:{task_id}"));
                }
                if bridge_evidence.active_tasks > 0 {
                    pending.push(format!("localset-active:{}", bridge_evidence.active_tasks));
                }
                if bridge_evidence.pending_requests > 0 {
                    pending.push(format!("localset-pending-req:{}", bridge_evidence.pending_requests));
                }
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
        None => state.provider_port.lock().expect("port mutex poisoned").take(),
    };
    let mut port_guard = RestorePort {
        value: port_taken,
        state: state.clone(),
        disarmed: false,
    };

    let released = core_released && host_released;
    if released {
        port_guard.disarm();
        state.clear_interrupted();
        state.closing.store(0, std::sync::atomic::Ordering::SeqCst);
        state
            .generation
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        state.publish_closed().await;
        (true, core_report)
    } else {
        state.mark_interrupted();
        let pending = state.pending_operations_snapshot().await;
        (false, interrupted_report(pending))
    }
}

/// Settlement of whatever a previous failed rollback or interrupted close retained.
async fn settle_retained(state: Arc<EnvState>) -> (bool, CoreCloseReport) {
    cleanup_owners(state, None, None, None, Instant::now() + CLOSE_BUDGET).await
}

/// Open the native core.
pub async fn open_core(
    state: Arc<EnvState>,
    options: NativeOpenOptions,
    js_port: Option<Arc<dyn ProviderPort>>,
) -> Result<Arc<CoreService>, String> {
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
    if options.allow_uninitialized {
        return Err(
            "allow_uninitialized is service-only and denies effects in native core".to_string(),
        );
    }
    let access = match options.access {
        NativeOpenOptionsAccess::ReadOnly => CoreAccess::ReadOnly,
        NativeOpenOptionsAccess::DirectWriter => CoreAccess::DirectWriter,
        NativeOpenOptionsAccess::EngineOwner => CoreAccess::EngineOwner,
    };
    let user_home = PathBuf::from(options.user_home);

    let core = CoreService::open(CoreOpenOptions {
        user_home: user_home.clone(),
        access,
    })
    .await
    .map_err(|e| e.to_string())?;

    let host = Arc::new(HostManager::new());
    let host_defaults = AgentHostConfig::default();
    let start_config = HostStartConfig {
        config_path: user_home.join("config/agent-host.toml"),
        workspace_root: user_home.clone(),
        max_sessions: host_defaults.max_sessions,
        max_ops_per_session: host_defaults.max_ops_per_session,
        timeouts: host_defaults.timeouts.clone(),
        host_config: None,
        probe_owner: None,
    };
    if let Err(err) = host.start(start_config).await {
        let (released, _) = cleanup_owners(state.clone(), Some(Arc::new(core)), Some(host), js_port, Instant::now() + CLOSE_BUDGET).await;
        if released {
            state.publish_closed().await;
        } else {
            state.mark_interrupted();
        }
        return Err(if released {
            err.to_string()
        } else {
            format!("interrupted: host start failed ({err}); cleanup unconfirmed")
        });
    }

    let provider_port: Arc<dyn ProviderPort> = if let Some(port) = js_port {
        Arc::new(AdmittingProviderPort::new(host.clone(), port))
    } else {
        Arc::new(host.build_provider_port().await)
    };

    let core = Arc::new(core);
    state.host.lock().expect("host mutex poisoned").replace(host);
    state.provider_port.lock().expect("port mutex poisoned").replace(provider_port);
    state.core.lock().expect("core mutex poisoned").replace(core.clone());
    state.publish_open().await;
    Ok(core)
}

/// Close the environment with a 5s phased budget and one concurrent settlement.
pub async fn close_core(state: Arc<EnvState>) -> CoreCloseReport {
    if let Some(report) = state.settled_close.lock().await.clone() {
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

    state.begin_close();
    state.begin_closing_phase().await;
    let started = Instant::now();
    let deadline = started + CLOSE_BUDGET;

    let host_snapshot = state.host.lock().expect("host mutex poisoned").clone();
    if let Some(host) = host_snapshot {
        state
            .record_pending_operations(host.pending_lifecycle_snapshot().await)
            .await;
    }

    let report = match tokio::time::timeout_at(
        tokio::time::Instant::from_std(deadline),
        async {
            let drain_budget = CLOSE_CANCEL_PHASE.min(CLOSE_BUDGET.saturating_sub(started.elapsed()));
            if !drain_budget.is_zero() {
                tokio::time::sleep(Duration::from_millis(25).min(drain_budget)).await;
            }
            let (_, report) = cleanup_owners(state.clone(), None, None, None, deadline).await;
            report
        },
    )
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

    #[cfg(debug_assertions)]
    #[tokio::test]
    async fn close_timeout_retains_owners_settle_then_reopen() {
        use nexus_contracts::native_open_options::NativeOpenOptionsAccess;
        use nexus_contracts::NativeOpenOptions;
        use tempfile::tempdir;
        use crate::wire_fixture::seed_wire_home;

        let dir = tempdir().expect("tempdir");
        seed_wire_home(dir.path()).await;
        let state = Arc::new(EnvState::new());
        set_force_cleanup_delay_ms(10_000);
        let _core = open_core(
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

}
