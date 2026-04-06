//! Creator handlers

use crate::workspace::WorkspaceState;
use axum::{extract::State, Json};
use serde::Serialize;

#[derive(Serialize)]
pub struct CreatorInfo {
    pub creator_id: String,
    pub display_name: String,
    pub status: String,
    pub cached_at: Option<String>,
}

#[derive(Serialize)]
pub struct ListCreatorsResponse {
    pub creators: Vec<CreatorInfo>,
}

/// GET /v1/local/creators
pub async fn list(State(state): State<WorkspaceState>) -> Json<ListCreatorsResponse> {
    let conn = match state.db().await {
        Some(conn) => conn,
        None => return Json(ListCreatorsResponse { creators: vec![] }),
    };

    let mut creators = Vec::new();
    let mut stmt = match conn.prepare(
        "SELECT creator_id, display_name, status, cached_at FROM creators ORDER BY cached_at DESC",
    ) {
        Ok(s) => s,
        Err(_) => return Json(ListCreatorsResponse { creators: vec![] }),
    };

    let rows = match stmt.query_map([], |row| {
        Ok(CreatorInfo {
            creator_id: row.get(0)?,
            display_name: row.get(1)?,
            status: row.get(2)?,
            cached_at: row.get(3)?,
        })
    }) {
        Ok(r) => r,
        Err(_) => return Json(ListCreatorsResponse { creators: vec![] }),
    };

    for row in rows.flatten() {
        creators.push(row);
    }

    Json(ListCreatorsResponse { creators })
}
