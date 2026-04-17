//! Presets listing handler.

use axum::{http::StatusCode, Json};
use nexus_contracts::local::orchestration::http::ListPresetsResponse;

/// `GET /v1/local/orchestration/presets`
///
/// WS2 returns only `_system.maintenance`. WS3 will add preset file loader.
pub async fn list_presets() -> (StatusCode, Json<ListPresetsResponse>) {
    (
        StatusCode::OK,
        Json(ListPresetsResponse {
            presets: vec!["_system.maintenance".to_string()],
        }),
    )
}
