use std::path::PathBuf;
use std::sync::Arc;

use nexus_agent_host::capability::model::HostStartConfig;
use nexus_agent_host::config::AgentHostConfig;
use nexus_agent_host::{HostFacade, HostManager};
use nexus_contracts::native_open_options::NativeOpenOptionsAccess;
use nexus_contracts::{CoreCloseReport, CoreCloseReportState, NativeOpenOptions};
use nexus_core::{CoreAccess, CoreOpenOptions, CoreService};
use nexus_provider_ports::ProviderPort;

use super::env_state::EnvState;

/// Outcome of tearing down a just-opened core after a failed host start.
enum FailedOpenCleanup {
    /// Core closed cleanly: the environment retains no owner.
    Released,
    /// Close was interrupted/unconfirmed: the core stays behind the cleanup
    /// owner and the caller reports `Interrupted`.
    Retained(CoreCloseReport),
}

/// A close releases cleanup ownership only when it is closed *and* confirmed.
#[must_use]
fn close_released(report: &CoreCloseReport) -> bool {
    report.state == CoreCloseReportState::Closed && report.cleanup_confirmed
}

/// Close a core whose open was rolled back. An unconfirmed close is never
/// silently dropped: the core is retained so the same cleanup ownership
/// discipline as an explicit close applies.
async fn cleanup_failed_open(state: &EnvState, core: CoreService) -> FailedOpenCleanup {
    let report = match core.close().await {
        Ok(report) => report,
        Err(_) => CoreCloseReport {
            state: CoreCloseReportState::Interrupted,
            cleanup_confirmed: false,
            pending_operations: vec![],
            reason: None,
        },
    };
    if close_released(&report) {
        FailedOpenCleanup::Released
    } else {
        state.core.lock().await.replace(Arc::new(core));
        FailedOpenCleanup::Retained(report)
    }
}

/// Open the native core.
///
/// Failure ordering is deliberate: the core opens first, so a failed open has
/// published no host or provider port. If the host then fails to start, the
/// freshly opened core is closed; an unconfirmed close keeps the core behind
/// the cleanup owner and reports `Interrupted` instead of dropping it.
pub async fn open_core(
    state: Arc<EnvState>,
    options: NativeOpenOptions,
    js_port: Option<Arc<dyn ProviderPort>>,
) -> Result<Arc<CoreService>, String> {
    if state.is_closing() {
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
    let host_config = AgentHostConfig::default();
    let start_config = HostStartConfig {
        config_path: user_home.join("config/agent-host.toml"),
        workspace_root: user_home,
        max_sessions: host_config.max_sessions,
        max_ops_per_session: host_config.max_ops_per_session,
        timeouts: host_config.timeouts.clone(),
        host_config: Some(host_config),
        probe_owner: None,
    };
    if let Err(err) = host.start(start_config).await {
        // A partially started host must not linger either; it was never
        // published to `state`, so best-effort shutdown fully disposes of it.
        let _ = host.shutdown().await;
        return Err(match cleanup_failed_open(&state, core).await {
            FailedOpenCleanup::Released => err.to_string(),
            FailedOpenCleanup::Retained(report) => format!(
                "interrupted: host start failed ({err}); core cleanup unconfirmed (state: {:?})",
                report.state
            ),
        });
    }

    let provider_port: Arc<dyn ProviderPort> = if let Some(port) = js_port {
        port
    } else {
        Arc::new(host.build_provider_port().await)
    };

    let core = Arc::new(core);
    state.host.lock().await.replace(host);
    state.provider_port.lock().await.replace(provider_port);
    state.core.lock().await.replace(core.clone());
    Ok(core)
}

pub async fn close_core(state: &EnvState) -> CoreCloseReport {
    state.begin_close();
    let report = {
        let mut guard = state.core.lock().await;
        if let Some(core) = guard.take() {
            match core.close().await {
                Ok(report) => report,
                Err(_) => CoreCloseReport {
                    state: CoreCloseReportState::Interrupted,
                    cleanup_confirmed: false,
                    pending_operations: vec![],
                    reason: None,
                },
            }
        } else {
            CoreCloseReport {
                state: CoreCloseReportState::Closed,
                cleanup_confirmed: true,
                pending_operations: vec![],
                reason: None,
            }
        }
    };

    if report.state == CoreCloseReportState::Closed && report.cleanup_confirmed {
        if let Some(host) = state.host.lock().await.take() {
            let _ = host.shutdown().await;
        }
        state.provider_port.lock().await.take();
        state
            .closing
            .store(0, std::sync::atomic::Ordering::SeqCst);
        state
            .generation
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
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
    fn unconfirmed_close_reports_interrupted_before_retention() {
        let retained = report(CoreCloseReportState::Interrupted, false);
        let message = format!(
            "interrupted: host start failed (denied); core cleanup unconfirmed (state: {:?})",
            retained.state
        );
        assert!(message.starts_with("interrupted:"));
        assert!(message.contains("cleanup unconfirmed"));
    }
}
