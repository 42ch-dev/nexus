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
    use std::sync::atomic::{AtomicBool, Ordering};

    static FORCE_UNCONFIRMED: AtomicBool = AtomicBool::new(false);

    pub fn set(enable: bool) {
        FORCE_UNCONFIRMED.store(enable, Ordering::SeqCst);
    }

    pub fn get() -> bool {
        FORCE_UNCONFIRMED.load(Ordering::SeqCst)
    }
}

/// Enable/disable forced unconfirmed cleanup. Debug builds only.
#[cfg(debug_assertions)]
pub fn set_force_unconfirmed(enable: bool) {
    forcing::set(enable);
}

#[cfg(debug_assertions)]
fn forced_unconfirmed() -> bool {
    forcing::get()
}

#[cfg(not(debug_assertions))]
const fn forced_unconfirmed() -> bool {
    false
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
    state: &EnvState,
    core: Option<Arc<CoreService>>,
    host: Option<Arc<HostManager>>,
    provider_port: Option<Arc<dyn ProviderPort>>,
) -> (bool, CoreCloseReport) {
    let forced = forced_unconfirmed();

    let core_report = match &core {
        Some(service) => match service.close().await {
            Ok(report) => report,
            Err(_) => interrupted_report(vec![]),
        },
        None => closed_report(),
    };
    let core_released = !forced && close_released(&core_report);
    if !core_released {
        if let Some(service) = core {
            state.core.lock().await.replace(service);
        }
    }

    let host_released = match host {
        Some(manager) => {
            let pending = manager.pending_lifecycle_snapshot().await;
            state.record_pending_operations(pending).await;
            let shutdown_ok = manager.shutdown().await.is_ok();
            let bridge_evidence = manager.localset_bridge().shutdown().await;
            let bridge_settled = bridge_evidence.joined_cleanly
                && bridge_evidence.active_tasks == 0
                && bridge_evidence.pending_requests == 0;
            if !forced && shutdown_ok && bridge_settled {
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
                state.host.lock().await.replace(manager);
                false
            }
        }
        None => true,
    };

    let released = core_released && host_released;
    if released {
        state.provider_port.lock().await.take();
        state.clear_interrupted();
        state.closing.store(0, std::sync::atomic::Ordering::SeqCst);
        state
            .generation
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        (true, core_report)
    } else {
        if let Some(port) = provider_port {
            state.provider_port.lock().await.replace(port);
        }
        state.mark_interrupted();
        let pending = state.pending_operations_snapshot().await;
        (false, interrupted_report(pending))
    }
}

/// Settlement of whatever a previous failed rollback or interrupted close retained.
async fn settle_retained(state: &EnvState) -> (bool, CoreCloseReport) {
    let core = state.core.lock().await.take();
    let host = state.host.lock().await.take();
    let provider_port = state.provider_port.lock().await.take();
    cleanup_owners(state, core, host, provider_port).await
}

/// Open the native core.
pub async fn open_core(
    state: Arc<EnvState>,
    options: NativeOpenOptions,
    js_port: Option<Arc<dyn ProviderPort>>,
) -> Result<Arc<CoreService>, String> {
    if state.is_interrupted() {
        let (released, _) = settle_retained(&state).await;
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
        state.env_dead.store(false, std::sync::atomic::Ordering::SeqCst);
    }
    if state.is_closing() && state.settled_close.lock().await.is_none() {
        return Err("closing".to_string());
    }
    if state.core.lock().await.is_some() {
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
        let (released, _) = cleanup_owners(&state, Some(Arc::new(core)), Some(host), js_port).await;
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
    state.host.lock().await.replace(host);
    state.provider_port.lock().await.replace(provider_port);
    state.core.lock().await.replace(core.clone());
    state.publish_open().await;
    Ok(core)
}

/// Close the environment with a 5s phased budget and one concurrent settlement.
pub async fn close_core(state: &EnvState) -> CoreCloseReport {
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

    let mut taken_core = state.core.lock().await.take();
    let mut taken_host = state.host.lock().await.take();
    let mut taken_port = state.provider_port.lock().await.take();
    if let Some(host) = taken_host.as_ref() {
        state
            .record_pending_operations(host.pending_lifecycle_snapshot().await)
            .await;
    }

    let report = match tokio::time::timeout(CLOSE_BUDGET, async {
        let drain_budget = CLOSE_CANCEL_PHASE.min(CLOSE_BUDGET.saturating_sub(started.elapsed()));
        if !drain_budget.is_zero() {
            tokio::time::sleep(Duration::from_millis(25).min(drain_budget)).await;
        }
        let (_, report) = cleanup_owners(
            state,
            taken_core.take(),
            taken_host.take(),
            taken_port.take(),
        )
        .await;
        report
    })
    .await
    {
        Ok(report) => report,
        Err(_) => {
            if let Some(core) = taken_core {
                state.core.lock().await.replace(core);
            }
            if let Some(host) = taken_host {
                state.host.lock().await.replace(host);
            }
            if let Some(port) = taken_port {
                state.provider_port.lock().await.replace(port);
            }
            state.mark_interrupted();
            interrupted_report(state.pending_operations_snapshot().await)
        }
    };

    *state.settled_close.lock().await = Some(report.clone());
    *state.close_in_flight.lock().await = false;
    if report.cleanup_confirmed {
        state.closing.store(0, std::sync::atomic::Ordering::SeqCst);
        state.publish_closed().await;
        state.generation.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    } else {
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
}
