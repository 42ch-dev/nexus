//! Manuscript handler

use axum::{extract::State, Json};
use serde::Serialize;
use crate::workspace::WorkspaceState;

#[derive(Serialize)]
pub struct ManuscriptStatusResponse {
    pub phase: Option<String>,
    pub active_manifest_id: Option<String>,
}

/// GET /v1/local/manuscript
pub async fn status(State(state): State<WorkspaceState>) -> Json<ManuscriptStatusResponse> {
    let conn = match state.db().await {
        Some(conn) => conn,
        None => return Json(ManuscriptStatusResponse {
            phase: None,
            active_manifest_id: None,
        }),
    };

    let phase: Option<String> = conn
        .query_row(
            "SELECT value FROM workspace_meta WHERE key = 'manuscript_phase'",
            [],
            |row| row.get(0),
        )
        .ok();

    let active_manifest_id: Option<String> = conn
        .query_row(
            "SELECT value FROM workspace_meta WHERE key = 'active_manifest_id'",
            [],
            |row| row.get(0),
        )
        .ok();

    Json(ManuscriptStatusResponse {
        phase,
        active_manifest_id,
    })
}
