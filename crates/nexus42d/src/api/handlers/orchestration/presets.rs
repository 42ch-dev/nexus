//! Presets listing and reload handlers.

use axum::{
    extract::Path,
    http::StatusCode,
    Json,
};
use nexus_contracts::local::orchestration::http::{
    ListPresetsResponse, ReloadPresetResponse,
};

/// `GET /v1/local/orchestration/presets`
///
/// Returns all available embedded preset IDs plus any user-installed presets.
pub async fn list_presets() -> (StatusCode, Json<ListPresetsResponse>) {
    let mut presets = nexus_orchestration::preset::list_embedded_presets();
    // Add _system.maintenance (hardcoded in WS2).
    if !presets.iter().any(|p| p == "_system.maintenance") {
        presets.push("_system.maintenance".to_string());
    }
    (
        StatusCode::OK,
        Json(ListPresetsResponse { presets }),
    )
}

/// `POST /v1/local/orchestration/presets/{id}:reload`
///
/// Invalidate loader cache for the given preset ID and reload from embedded
/// storage. Returns the new source hash.
///
/// Running sessions continue on the previous graph (snapshot semantics);
/// new sessions pick up the new graph.
pub async fn reload_preset(
    Path(preset_id): Path<String>,
) -> Result<(StatusCode, Json<ReloadPresetResponse>), (StatusCode, String)> {
    // Validate the preset exists by attempting to load it.
    let caps = nexus_orchestration::CapabilityRegistry::with_builtins();
    let loaded = nexus_orchestration::preset::load_embedded_preset(&preset_id, &caps)
        .map_err(|e| {
            (
                StatusCode::NOT_FOUND,
                format!("preset '{}' not found: {}", preset_id, e),
            )
        })?;

    // Compute the new source hash (blake3 hex).
    let hash_hex = loaded
        .source_hash
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();

    Ok((
        StatusCode::OK,
        Json(ReloadPresetResponse {
            preset_id: preset_id.clone(),
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

    #[test]
    fn list_presets_includes_novel_writing() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let (status, Json(resp)) = rt.block_on(list_presets());
        assert_eq!(status, StatusCode::OK);
        assert!(
            resp.presets.iter().any(|p| p == "novel-writing"),
            "should include novel-writing: {:?}",
            resp.presets
        );
        assert!(
            resp.presets.iter().any(|p| p == "_system.maintenance"),
            "should include _system.maintenance: {:?}",
            resp.presets
        );
    }

    #[tokio::test]
    async fn reload_novel_writing_returns_200() {
        let path = Path("novel-writing".to_string());
        let result = reload_preset(path).await;
        assert!(result.is_ok());
        let (status, Json(resp)) = result.unwrap();
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
        let (status, msg) = result.unwrap_err();
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(msg.contains("not found"), "msg: {msg}");
    }
}
