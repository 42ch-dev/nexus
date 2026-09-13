use std::path::PathBuf;
use std::sync::Arc;

use nexus_agent_host::capability::model::HostStartConfig;
use nexus_agent_host::config::AgentHostConfig;
use nexus_agent_host::{HostFacade, HostManager};
use nexus_contracts::{CoreCloseReport, CoreCloseReportState, NativeOpenOptions};
use nexus_contracts::native_open_options::NativeOpenOptionsAccess;
use nexus_core::{CoreAccess, CoreOpenOptions, CoreService};
use nexus_provider_ports::ProviderPort;

use super::env_state::EnvState;

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
    let host = Arc::new(HostManager::new());
    let host_config = AgentHostConfig::default();
    host
        .start(HostStartConfig {
            config_path: user_home.join("config/agent-host.toml"),
            workspace_root: user_home.clone(),
            max_sessions: host_config.max_sessions,
            max_ops_per_session: host_config.max_ops_per_session,
            timeouts: host_config.timeouts.clone(),
            host_config: Some(host_config),
            probe_owner: None,
        })
        .await
        .map_err(|e| e.to_string())?;
    state.host.lock().await.replace(host.clone());

    let provider_port: Arc<dyn ProviderPort> = if let Some(port) = js_port {
        port
    } else {
        Arc::new(host.build_provider_port().await)
    };
    state.provider_port.lock().await.replace(provider_port);

    let core = CoreService::open(CoreOpenOptions {
        user_home,
        access,
    })
    .await
    .map_err(|e| e.to_string())?;
    let arc = Arc::new(core);
    state.core.lock().await.replace(arc.clone());
    Ok(arc)
}

pub async fn close_core(state: &EnvState) -> CoreCloseReport {
    state.begin_close();
    let mut guard = state.core.lock().await;
    let report = if let Some(core) = guard.take() {
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
