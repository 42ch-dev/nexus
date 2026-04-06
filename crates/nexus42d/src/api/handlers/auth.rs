//! Auth status handler

use axum::{extract::State, Json};
use serde::Serialize;
use crate::workspace::WorkspaceState;

#[derive(Serialize)]
pub struct AuthStatusResponse {
    pub user_authenticated: bool,
    pub user_id: Option<String>,
    pub creator_tokens: usize,
}

/// GET /v1/local/auth/status
pub async fn status(State(state): State<WorkspaceState>) -> Json<AuthStatusResponse> {
    // Read auth store from disk
    let auth_path = state.nexus_home().join("auth.json");
    let mut user_authenticated = false;
    let mut user_id = None;
    let mut creator_tokens = 0;

    if let Ok(content) = std::fs::read_to_string(&auth_path) {
        if let Ok(store) = serde_json::from_str::<serde_json::Value>(&content) {
            user_authenticated = store.get("user").is_some();
            user_id = store
                .get("user")
                .and_then(|u| u.get("user_id"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            creator_tokens = store
                .get("creators")
                .and_then(|c| c.as_object())
                .map(|m| m.len())
                .unwrap_or(0);
        }
    }

    Json(AuthStatusResponse {
        user_authenticated,
        user_id,
        creator_tokens,
    })
}
