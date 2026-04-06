//! Reference source handlers

use crate::workspace::WorkspaceState;
use axum::{extract::State, Json};
use serde::Serialize;

#[derive(Serialize)]
pub struct ReferenceInfo {
    pub reference_source_id: String,
    pub source_type: String,
    pub title: String,
    pub scan_status: String,
    pub created_at: String,
}

#[derive(Serialize)]
pub struct ListReferencesResponse {
    pub references: Vec<ReferenceInfo>,
}

/// GET /v1/local/references
pub async fn list(State(state): State<WorkspaceState>) -> Json<ListReferencesResponse> {
    let conn = match state.db().await {
        Some(conn) => conn,
        None => return Json(ListReferencesResponse { references: vec![] }),
    };

    let mut references = Vec::new();
    let mut stmt = match conn.prepare(
        "SELECT reference_source_id, source_type, title, scan_status, created_at
         FROM reference_sources ORDER BY created_at DESC",
    ) {
        Ok(s) => s,
        Err(_) => return Json(ListReferencesResponse { references: vec![] }),
    };

    let rows = match stmt.query_map([], |row| {
        Ok(ReferenceInfo {
            reference_source_id: row.get(0)?,
            source_type: row.get(1)?,
            title: row.get(2)?,
            scan_status: row.get(3)?,
            created_at: row.get(4)?,
        })
    }) {
        Ok(r) => r,
        Err(_) => return Json(ListReferencesResponse { references: vec![] }),
    };

    for row in rows.flatten() {
        references.push(row);
    }

    Json(ListReferencesResponse { references })
}
