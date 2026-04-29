//! Presets listing and reload handlers.

use crate::workspace::WorkspaceState;
use axum::{extract::Path, extract::State, http::StatusCode, Json};
use nexus_contracts::local::orchestration::http::{ListPresetsResponse, ReloadPresetResponse};
use nexus_orchestration::system_preset_dir;

/// `GET /v1/local/orchestration/presets`
///
/// Returns all available embedded preset IDs plus system presets discovered
/// from `~/.nexus42/presets/_system/<name>/`.
pub async fn list_presets(
    State(state): State<WorkspaceState>,
) -> (StatusCode, Json<ListPresetsResponse>) {
    let mut presets = nexus_orchestration::preset::list_embedded_presets();

    // Discover system presets from directory (WS-D).
    let caps = nexus_orchestration::CapabilityRegistry::with_builtins();
    let scan_result = system_preset_dir::scan_system_presets(state.nexus_home(), &caps);
    for id in system_preset_dir::list_system_preset_ids(&scan_result) {
        if !presets.iter().any(|p| p == &id) {
            presets.push(id);
        }
    }

    (StatusCode::OK, Json(ListPresetsResponse { presets }))
}

/// `POST /v1/local/orchestration/presets/{id}:reload`
///
/// Invalidate loader cache for the given preset ID and reload from embedded
/// storage. Returns the new source hash.
///
/// Running sessions continue on the previous graph (snapshot semantics);
/// new sessions pick up the new graph.
///
/// # Errors
///
/// Returns `404 NOT_FOUND` if the preset ID does not exist.
///
/// # Panics
///
/// Does not panic; the `write_fmt` call is infallible for `String`.
pub async fn reload_preset(
    Path(preset_id): Path<String>,
) -> Result<(StatusCode, Json<ReloadPresetResponse>), (StatusCode, String)> {
    // Validate the preset exists by attempting to load it.
    let caps = nexus_orchestration::CapabilityRegistry::with_builtins();
    let loaded =
        nexus_orchestration::preset::load_embedded_preset(&preset_id, &caps).map_err(|e| {
            (
                StatusCode::NOT_FOUND,
                format!("preset '{preset_id}' not found: {e}"),
            )
        })?;

    // Compute the new source hash (blake3 hex).
    let mut hash_hex = String::with_capacity(64);
    for b in &loaded.source_hash {
        use std::fmt::Write;
        hash_hex
            .write_fmt(format_args!("{b:02x}"))
            .expect("write to String should succeed");
    }

    Ok((
        StatusCode::OK,
        Json(ReloadPresetResponse {
            preset_id,
            source_hash: hash_hex,
        }),
    ))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn list_presets_includes_novel_writing() {
        // Create a minimal test workspace with nexus_home.
        let (tmp, nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        let state =
            crate::workspace::WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let (status, Json(resp)) = list_presets(State(state)).await;
        assert_eq!(status, StatusCode::OK);
        assert!(
            resp.presets.iter().any(|p| p == "novel-writing"),
            "should include novel-writing: {:?}",
            resp.presets
        );

        // _system.maintenance should be auto-created by ensure_maintenance_preset
        // if the scan runs (depends on test environment), but we don't assert it
        // here because the test workspace may not have the directory set up.

        std::mem::forget(tmp);
    }

    #[tokio::test]
    async fn reload_novel_writing_returns_200() {
        let path = Path("novel-writing".to_string());
        let result = reload_preset(path).await;
        assert!(result.is_ok());
        let (status, Json(resp)) =
            result.expect("reload_preset should succeed for novel-writing preset");
        assert_eq!(status, StatusCode::OK);
        assert_eq!(resp.preset_id, "novel-writing");
        assert!(!resp.source_hash.is_empty());
        // blake3 hex = 64 chars
        assert_eq!(resp.source_hash.len(), 64);
    }

    #[tokio::test]
    async fn reload_unknown_preset_returns_404() {
        let path = Path("nonexistent-preset".to_string());
        let result = reload_preset(path).await;
        assert!(result.is_err());
        let (status, msg) = result.expect_err("reload_preset should fail for nonexistent preset");
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(msg.contains("not found"), "msg: {msg}");
    }
}
