use std::path::PathBuf;

use std::time::{Duration, Instant};

const CLOSE_BUDGET: Duration = Duration::from_secs(5);
const CLOSE_CANCEL_PHASE: Duration = Duration::from_secs(2);
const CLOSE_ABORT_PHASE: Duration = Duration::from_secs(4);

use std::sync::Arc;

use nexus_agent_host::capability::model::{HostStartConfig, SessionOwner};
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

fn interrupted_report() -> CoreCloseReport {
    CoreCloseReport {
        state: CoreCloseReportState::Interrupted,
        cleanup_confirmed: false,
        pending_operations: vec![],
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
///
/// Owners whose cleanup is not confirmed are put back into `EnvState` and the
/// environment is marked interrupted, so nothing is dropped silently and no
/// later open can replace them. Nothing held here is ignored: the
/// `host.shutdown()` result decides whether the host owner is released.
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
            Err(_) => interrupted_report(),
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
            let shutdown_ok = manager.shutdown().await.is_ok();
            if !forced && shutdown_ok {
                true
            } else {
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
        (false, interrupted_report())
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
///
/// Ordering is deliberate: the core opens first, so a failed open has published
/// no host or provider port. If the host then fails to start, the owners are
/// rolled back through the same cleanup discipline as close; an unconfirmed
/// rollback retains them and fences further opens until a confirmed settlement.
pub async fn open_core(
    state: Arc<EnvState>,
    options: NativeOpenOptions,
    js_port: Option<Arc<dyn ProviderPort>>,
) -> Result<Arc<CoreService>, String> {
    if state.is_interrupted() {
        // Admission fence: only a fully confirmed settlement reopens the
        // environment. Otherwise the retained owners survive untouched.
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
        // Load catalog providers from agent-host.toml when present (Rust admission).
        host_config: None,
        probe_owner: Some(SessionOwner {
            creator_id: "nexus-native".to_string(),
            workspace_root: user_home,
            orchestration_run_id: None,
        }),
    };
    if let Err(err) = host.start(start_config).await {
        let (released, _) = cleanup_owners(&state, Some(Arc::new(core)), Some(host), js_port).await;
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
                .unwrap_or_else(interrupted_report);
        }
        *in_flight = true;
    }

    state.begin_close();
    let started = Instant::now();
    let report = tokio::time::timeout(CLOSE_BUDGET, async {
        // Phase 0–2s: cooperative cancel/drain window for in-flight callbacks.
        let drain_budget = CLOSE_CANCEL_PHASE.min(CLOSE_BUDGET.saturating_sub(started.elapsed()));
        if !drain_budget.is_zero() {
            tokio::time::sleep(Duration::from_millis(25).min(drain_budget)).await;
        }
        let core = state.core.lock().await.take();
        let host = state.host.lock().await.take();
        let provider_port = state.provider_port.lock().await.take();
        let (_, report) = cleanup_owners(state, core, host, provider_port).await;
        if report.cleanup_confirmed {
            return report;
        }
        // Unconfirmed cleanup: hold until abort phase, then spend remaining budget.
        let elapsed = started.elapsed();
        if elapsed < CLOSE_ABORT_PHASE {
            tokio::time::sleep(CLOSE_ABORT_PHASE - elapsed).await;
        }
        let remaining = CLOSE_BUDGET.saturating_sub(started.elapsed());
        if !remaining.is_zero() {
            tokio::time::sleep(remaining).await;
        }
        report
    })
    .await
    .unwrap_or_else(|_| interrupted_report());

    *state.settled_close.lock().await = Some(report.clone());
    *state.close_in_flight.lock().await = false;
    if report.cleanup_confirmed {
        state.closing.store(0, std::sync::atomic::Ordering::SeqCst);
        // Revoke principal handles from the closed generation so foreign handles fail.
        state.generation.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
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
        assert!(!close_released(&interrupted_report()));
        assert!(close_released(&closed_report()));
    }

    #[test]
    fn forced_unconfirmed_is_off_in_a_fresh_process() {
        assert!(!forced_unconfirmed());
    }
}
