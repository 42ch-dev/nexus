//! Presets listing, reload, and profile handlers.

use crate::api::errors::NexusApiError;
use crate::workspace::WorkspaceState;
use axum::{extract::Path, extract::State, http::StatusCode, Json};
use nexus_contracts::local::orchestration::http::ReloadPresetResponse;
use nexus_contracts::{OrchestrationPresetListResponse, PresetProfileResponse};

///
/// # Errors
///
/// Returns [`NexusApiError`] when the creator/workspace guard rejects the
/// request, the core authority denies it (ownership, admission or validation),
/// or the bounded store read/write fails.
/// `GET /v1/daemon/orchestration/presets`
///
/// Returns all available embedded preset IDs plus system presets discovered
/// from `~/.nexus42/presets/_system/<name>/`.
pub async fn list_presets(
    State(state): State<WorkspaceState>,
) -> Result<(StatusCode, Json<OrchestrationPresetListResponse>), NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    let response = core.list_orchestration_presets(&principal).await?;
    Ok((StatusCode::OK, Json(response)))
}

/// `POST /v1/daemon/orchestration/presets/:id`
///
/// Invalidate loader cache for the given preset ID and reload from embedded
/// storage. Returns the new source hash.
///
/// Routed as `POST /v1/daemon/orchestration/presets/:id` because matchit 0.7
/// cannot register `:id:reload` as a separate pattern. The path segment must
/// end with `:reload`; otherwise this returns 404.
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
    Path(segment): Path<String>,
) -> Result<(StatusCode, Json<ReloadPresetResponse>), NexusApiError> {
    let preset_id = segment
        .strip_suffix(":reload")
        .ok_or_else(|| NexusApiError::NotFound(format!("Preset route '{segment}' not found")))?
        .to_string();

    // Validate the preset exists by attempting to load it.
    let caps = nexus_orchestration::CapabilityRegistry::with_builtins();
    let loaded = nexus_preset::load_embedded_preset(&preset_id, &caps)
        .map_err(|e| NexusApiError::NotFound(format!("preset '{preset_id}' not found: {e}")))?;

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

/// `GET /v1/daemon/orchestration/presets/:id/profile`
///
/// Returns a manifest-derived profile for any resolvable preset (AR-20..23):
/// trigger-lane classification, per-state enter / exit-when / next forms,
/// roles + recommended skills, required capabilities, and declared signals
/// (declared, not delivered).
///
/// Resolution goes through the 3-tier `resolve_preset` (user → system →
/// embedded; `_system.` qualified ids resolve via `find_system_preset`) —
/// NOT `load_embedded_preset`, which would 404 every user preset (AR-22).
///
/// # Errors
///
/// Returns `404 NOT_FOUND` if the preset ID does not resolve in any tier.
///
/// # Panics
///
/// Does not panic; the `write_fmt` call is infallible for `String`.
pub async fn get_preset_profile(
    State(state): State<WorkspaceState>,
    Path(preset_id): Path<String>,
) -> Result<Json<PresetProfileResponse>, NexusApiError> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await?;
    let response = core.get_preset_profile(&principal, preset_id).await?;
    Ok(Json(response))
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

        let (status, Json(resp)) = list_presets(State(state)).await.expect("list presets");
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
        let path = Path("novel-writing:reload".to_string());
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
        let path = Path("nonexistent-preset:reload".to_string());
        let result = reload_preset(path).await;
        assert!(result.is_err());
        let err = result.expect_err("reload_preset should fail for nonexistent preset");
        assert_eq!(err.status_code(), StatusCode::NOT_FOUND);
        assert!(
            err.error_code().contains("not_found"),
            "code: {}",
            err.error_code()
        );
    }

    // ── Profile endpoint (AR-20..23) ──────────────────────────────────────

    /// Seed a minimal valid user preset bundle under `~/.nexus42/presets/<id>/`.
    ///
    /// `nexus_home` is already the `.nexus42` root (per `create_test_workspace`),
    /// so the bundle lives at `nexus_home/presets/<id>/` — the same layout
    /// `resolve_preset` scans (`user_preset_dir::scan_user_presets`).
    fn seed_user_preset(nexus_home: &std::path::Path, id: &str) {
        let bundle_dir = nexus_home.join("presets").join(id);
        std::fs::create_dir_all(&bundle_dir).expect("create user preset dir");
        let yaml = format!(
            r#"
preset:
  id: {id}
  version: 2
  kind: creator
  description: "User override preset for profile tests"
  requires_capabilities:
    - creator.inject_prompt
  run_intents: [work_continue]
  initial: start
  terminal: end
states:
  - id: start
    description: "Start state"
    enter:
      - kind: capability
        name: creator.inject_prompt
    exit_when:
      kind: manual
    next: end
  - id: end
    terminal: true
"#
        );
        std::fs::write(bundle_dir.join("preset.yaml"), yaml).expect("write user preset.yaml");
    }

    #[tokio::test]
    async fn profile_resolves_embedded_preset() {
        let (tmp, nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        let state =
            crate::workspace::WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let result = get_preset_profile(State(state), Path("novel-writing".to_string())).await;
        let Json(profile) = result.expect("embedded preset profile should resolve");
        assert_eq!(profile.id, "novel-writing");
        assert_eq!(profile.version, 9);
        assert_eq!(profile.source_hash.len(), 64);
        // Lanes: novel-writing is not a works-cron role preset; the other
        // lanes are platform facts for any resolvable preset.
        assert!(!profile.lanes.cron);
        assert!(profile.lanes.wall_clock);
        assert!(profile.lanes.session);
        assert!(profile.lanes.direct);
        // Orchestration shape from the manifest.
        assert!(profile.states.iter().any(|s| s.id == "outline_chapter"));
        let outline = profile
            .states
            .iter()
            .find(|s| s.id == "outline_chapter")
            .expect("outline_chapter state");
        assert_eq!(outline.enter[0].kind, "capability");
        assert_eq!(outline.enter[0].name, "creator.inject_prompt");
        assert_eq!(
            outline.exit_when.as_ref().expect("exit_when").kind,
            "manual"
        );
        assert_eq!(outline.next.as_ref().expect("next").kind, "linear");
        assert_eq!(
            outline.next.as_ref().expect("next").target.as_deref(),
            Some("outline_review")
        );
        // Roles + recommended skills from LoadedPreset.roles.
        assert!(profile.roles.iter().any(|r| r.id == "writer"));
        let writer = profile
            .roles
            .iter()
            .find(|r| r.id == "writer")
            .expect("writer role");
        assert_eq!(writer.recommended_skills, vec!["novel-writing-assistant"]);
        // Required capabilities from the manifest.
        assert!(profile
            .required_capabilities
            .contains(&"judge.llm".to_string()));
        // novel-writing declares no signals → absent, not invented.
        assert!(profile.signals.is_empty());

        std::mem::forget(tmp);
    }

    #[tokio::test]
    async fn profile_resolves_user_override_preset() {
        let (tmp, nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        // User preset with the same id as an embedded preset must win (3-tier
        // user → system → embedded, AR-22).
        seed_user_preset(&nexus_home, "novel-writing");
        let state =
            crate::workspace::WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let result = get_preset_profile(State(state), Path("novel-writing".to_string())).await;
        let Json(profile) = result.expect("user override preset profile should resolve");
        assert_eq!(profile.id, "novel-writing");
        assert_eq!(
            profile.version, 2,
            "user override must win over embedded v9"
        );
        assert_eq!(profile.required_capabilities, vec!["creator.inject_prompt"]);
        assert!(profile.roles.is_empty(), "user override declares no roles");
        assert!(profile.signals.is_empty());

        std::mem::forget(tmp);
    }

    #[tokio::test]
    async fn profile_resolves_system_preset() {
        let (tmp, nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        // First-start fallback creates `presets/_system/maintenance/` on disk.
        nexus_preset::system_preset_dir::ensure_maintenance_preset(&nexus_home)
            .expect("ensure maintenance preset");
        let state =
            crate::workspace::WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let result =
            get_preset_profile(State(state), Path("_system.maintenance".to_string())).await;
        let Json(profile) = result.expect("system preset profile should resolve");
        assert_eq!(profile.id, "maintenance");
        assert_eq!(profile.version, 1);
        assert!(profile.states.iter().any(|s| s.id == "sync_pull"));
        assert!(profile
            .required_capabilities
            .contains(&"sync.pull".to_string()));

        std::mem::forget(tmp);
    }

    #[tokio::test]
    async fn profile_unknown_preset_returns_404() {
        let (tmp, nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        let state =
            crate::workspace::WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let result = get_preset_profile(State(state), Path("nonexistent-preset".to_string())).await;
        let err = result.expect_err("unknown preset must 404");
        assert_eq!(err.status_code(), StatusCode::NOT_FOUND);
        assert_eq!(err.error_code(), "not_found");

        std::mem::forget(tmp);
    }

    #[tokio::test]
    async fn profile_user_preset_reports_session_false() {
        // W-003/F-002: the session-start API loads embedded presets only, so
        // a user preset must report `session: false` — never overstate a lane
        // the runtime cannot serve.
        let (tmp, nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        seed_user_preset(&nexus_home, "my-strategy");
        let state =
            crate::workspace::WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let result = get_preset_profile(State(state), Path("my-strategy".to_string())).await;
        let Json(profile) = result.expect("user preset profile should resolve");
        assert_eq!(profile.id, "my-strategy");
        assert!(
            !profile.lanes.session,
            "user preset must report session: false (session-start API is embedded-only)"
        );
        // Other lanes remain platform facts for any resolvable preset.
        assert!(profile.lanes.wall_clock);
        assert!(profile.lanes.direct);
        assert!(!profile.lanes.cron);

        std::mem::forget(tmp);
    }

    #[tokio::test]
    async fn profile_embedded_and_system_presets_report_session_true() {
        // W-003/F-002: embedded and system presets are loadable by the
        // session-start API, so they report `session: true`.
        let (tmp, nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        nexus_preset::system_preset_dir::ensure_maintenance_preset(&nexus_home)
            .expect("ensure maintenance preset");
        let state =
            crate::workspace::WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let result =
            get_preset_profile(State(state.clone()), Path("novel-writing".to_string())).await;
        let Json(embedded) = result.expect("embedded preset profile should resolve");
        assert!(embedded.lanes.session, "embedded preset session: true");

        let result =
            get_preset_profile(State(state), Path("_system.maintenance".to_string())).await;
        let Json(system) = result.expect("system preset profile should resolve");
        assert!(system.lanes.session, "system preset session: true");

        std::mem::forget(tmp);
    }

    #[tokio::test]
    async fn profile_cron_lane_derives_from_cron_role_preset_ids() {
        // W-001/F-004: `cron` must be derived from the shared
        // `cron_role_preset_ids()` source, not a hand-maintained list.
        let (tmp, nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        let state =
            crate::workspace::WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        for id in nexus_preset::preset_ids::cron_role_preset_ids() {
            let result = get_preset_profile(State(state.clone()), Path(id.to_string())).await;
            let Json(profile) = result
                .unwrap_or_else(|e| panic!("cron-role preset '{id}' profile should resolve: {e}"));
            assert!(
                profile.lanes.cron,
                "cron-role preset '{id}' must report cron: true"
            );
        }

        // A non-cron-role preset reports cron: false.
        let result = get_preset_profile(State(state), Path("novel-writing".to_string())).await;
        let Json(profile) = result.expect("novel-writing profile should resolve");
        assert!(!profile.lanes.cron);

        std::mem::forget(tmp);
    }

    #[tokio::test]
    async fn profile_serializes_absent_manifest_fields_as_absent() {
        let (tmp, nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        let state =
            crate::workspace::WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let result = get_preset_profile(State(state), Path("novel-writing".to_string())).await;
        let Json(profile) = result.expect("embedded preset profile should resolve");
        let json = serde_json::to_value(&profile).expect("profile serializes");
        // No invented defaults: absent manifest fields serialize absent.
        assert!(
            json.get("roles").is_some(),
            "roles present (manifest has roles)"
        );
        // novel-writing declares no signals → the empty vec serializes absent
        // (skip_serializing_if), not as an invented empty array.
        assert!(
            json.get("signals").is_none(),
            "empty signals serialize absent"
        );
        // A state without enter/exit_when/next (terminal `done`) serializes
        // those fields absent, not as invented defaults.
        let done = json["states"]
            .as_array()
            .expect("states array")
            .iter()
            .find(|s| s["id"] == "done")
            .expect("done state");
        assert_eq!(done["terminal"], true);
        assert!(done.get("enter").is_none(), "no enter on terminal state");
        assert!(
            done.get("exit_when").is_none(),
            "no exit_when on terminal state"
        );
        assert!(done.get("next").is_none(), "no next on terminal state");

        std::mem::forget(tmp);
    }
}
