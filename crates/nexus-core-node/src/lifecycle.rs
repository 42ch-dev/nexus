use std::path::PathBuf;
use std::sync::Arc;

use nexus_contracts::{CoreCloseReport, CoreCloseReportState, NativeOpenOptions};
use nexus_contracts::native_open_options::NativeOpenOptionsAccess;
use nexus_core::{CoreAccess, CoreOpenOptions, CoreService};
use nexus_provider_ports::ProviderPort;

use super::env_state::EnvState;
use super::provider_stub::StubProviderPort;

pub async fn open_core(
    state: &EnvState,
    options: NativeOpenOptions,
) -> Result<Arc<CoreService>, String> {
    if state.core.lock().await.is_none() {
        state
            .closing
            .store(0, std::sync::atomic::Ordering::SeqCst);
    }
    if state.is_closing() {
        return Err("closing".to_string());
    }
    if options.allow_uninitialized {
        return Err("allow_uninitialized is service-only and denies effects in native core".to_string());
    }
    let access = match options.access {
        NativeOpenOptionsAccess::ReadOnly => CoreAccess::ReadOnly,
        NativeOpenOptionsAccess::DirectWriter => CoreAccess::DirectWriter,
        NativeOpenOptionsAccess::EngineOwner => CoreAccess::EngineOwner,
    };
    let core = CoreService::open(CoreOpenOptions {
        user_home: PathBuf::from(options.user_home),
        access,
    })
    .await
    .map_err(|e| e.to_string())?;
    let arc = Arc::new(core);
    state.core.lock().await.replace(arc.clone());
    let pending_port = state.pending_provider.lock().unwrap().take();
    if let Some(port) = pending_port {
        state.provider_port.lock().await.replace(port);
    } else if state.provider_port.lock().await.is_none() {
        state
            .provider_port
            .lock()
            .await
            .replace(Arc::new(StubProviderPort) as Arc<dyn ProviderPort>);
    }
    Ok(arc)
}

pub async fn install_provider_port(state: &EnvState, port: Arc<dyn ProviderPort>) {
    state.provider_port.lock().await.replace(port);
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
    state.provider_port.lock().await.take();
    state.pending_provider.lock().unwrap().take();
    state
        .closing
        .store(0, std::sync::atomic::Ordering::SeqCst);
    state
        .generation
        .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    report
}
