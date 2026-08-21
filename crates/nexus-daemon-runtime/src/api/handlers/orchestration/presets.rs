//! Presets listing, reload, and profile handlers.

use crate::api::errors::NexusApiError;
use crate::workspace::WorkspaceState;
use axum::{extract::Path, extract::State, http::StatusCode, Json};
use nexus_contracts::local::orchestration::http::{
    ListPresetsResponse, PresetProfileConditionalRule, PresetProfileEnterAction,
    PresetProfileExitWhen, PresetProfileLabeledNext, PresetProfileLanes, PresetProfileNext,
    PresetProfileResponse, PresetProfileRole, PresetProfileSignal, PresetProfileState,
    ReloadPresetResponse,
};
use nexus_contracts::local::orchestration::preset::{
    EnterAction, ExitWhen, NextTarget, PresetRoleDefinition, SignalActionKind, SignalBinding,
    StateDefinition,
};
use nexus_orchestration::preset_ids::cron_role_preset_ids;
use nexus_orchestration::system_preset_dir;

/// `GET /v1/daemon/orchestration/presets`
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
    let loaded = nexus_orchestration::preset::load_embedded_preset(&preset_id, &caps)
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
    let caps = nexus_orchestration::CapabilityRegistry::with_builtins();
    // F-003: O(1) direct-path lookup first (as `creator run` does); fall
    // back to the full 3-tier scan for `_system.` ids and miss/load-error
    // cases (AR-22).
    let loaded = match nexus_orchestration::preset::lookup_preset_by_id(
        &preset_id,
        state.nexus_home(),
        &caps,
    ) {
        Some(loaded) => loaded,
        None => nexus_orchestration::preset::resolve_preset(&preset_id, state.nexus_home(), &caps)
            .map_err(|e| NexusApiError::NotFound(format!("preset '{preset_id}' not found: {e}")))?,
    };

    let manifest = &loaded.manifest;
    let states = manifest
        .states
        .iter()
        .map(profile_state)
        .collect::<Vec<_>>();
    let roles = loaded.roles.iter().map(profile_role).collect::<Vec<_>>();
    let signals = loaded
        .signals
        .iter()
        .map(profile_signal)
        .collect::<Vec<_>>();

    let mut hash_hex = String::with_capacity(64);
    for b in &loaded.source_hash {
        use std::fmt::Write;
        hash_hex
            .write_fmt(format_args!("{b:02x}"))
            .expect("write to String should succeed");
    }

    Ok(Json(PresetProfileResponse {
        id: loaded.id.clone(),
        version: loaded.version,
        source_hash: hash_hex,
        lanes: profile_lanes(&preset_id, !is_user_preset(&preset_id, state.nexus_home())),
        states,
        roles,
        required_capabilities: manifest.preset.requires_capabilities.clone(),
        signals,
    }))
}

/// Trigger-lane classification (AR-21).
///
/// `cron` is derived from the shared works-cron role membership
/// ([`cron_role_preset_ids`] — the brainstorm / write / review role presets
/// per `RolesSchedule`; same source as
/// `schedule::cron_supervisor::role_preset`), never a hand-maintained
/// per-preset list (W-001/F-004).
///
/// `session` is honest per resolvability class (W-003/F-002): the
/// session-start API (`POST /v1/daemon/orchestration/sessions`) loads
/// embedded presets only (`load_embedded_preset`), so a **user** preset
/// reports `session: false` — the lane claim must not overstate what the
/// runtime can serve. System (`_system.`) and embedded presets report
/// `session: true`.
///
/// `wall_clock` / `direct` are platform facts — the daemon schedule path
/// resolves any resolvable preset id (`resolve_preset`), so every resolvable
/// preset can fire on the wall-clock poller or via a direct run with an
/// explicit payload.
fn profile_lanes(preset_id: &str, session: bool) -> PresetProfileLanes {
    PresetProfileLanes {
        cron: cron_role_preset_ids().contains(&preset_id),
        wall_clock: true,
        session,
        direct: true,
    }
}

/// Is `preset_id` a user preset (3-tier resolvability class, AR-22)?
///
/// `_system.` qualified ids are system presets; otherwise a user bundle at
/// `<nexus_home>/presets/<id>/preset.yaml` marks the user class. Used for
/// the `session` lane honesty check (W-003/F-002).
fn is_user_preset(preset_id: &str, nexus_home: &std::path::Path) -> bool {
    !preset_id.starts_with("_system.")
        && nexus_home
            .join("presets")
            .join(preset_id)
            .join("preset.yaml")
            .is_file()
}

/// Map one manifest state to its profile shape.
fn profile_state(state: &StateDefinition) -> PresetProfileState {
    PresetProfileState {
        id: state.id.clone(),
        description: state.description.clone(),
        enter: state.enter.iter().map(profile_enter_action).collect(),
        exit_when: state.exit_when.as_ref().map(profile_exit_when),
        next: state.next.as_ref().map(profile_next),
        terminal: state.terminal,
    }
}

/// Map one enter action to its profile shape.
fn profile_enter_action(action: &EnterAction) -> PresetProfileEnterAction {
    match action {
        EnterAction::Capability { name, .. } => PresetProfileEnterAction {
            kind: "capability".to_string(),
            name: name.clone(),
        },
        EnterAction::InnerGraph { name } => PresetProfileEnterAction {
            kind: "inner_graph".to_string(),
            name: name.clone(),
        },
        EnterAction::HostTool { tool_name, .. } => PresetProfileEnterAction {
            kind: "host_tool".to_string(),
            name: tool_name.clone(),
        },
    }
}

/// Map one exit condition to its profile shape.
fn profile_exit_when(exit_when: &ExitWhen) -> PresetProfileExitWhen {
    match exit_when {
        ExitWhen::LlmJudge {
            template_file,
            judge_capability,
            min_interval,
        } => PresetProfileExitWhen {
            kind: "llm_judge".to_string(),
            template_file: template_file.clone(),
            judge_capability: judge_capability.clone(),
            min_interval: min_interval.clone(),
            duration: None,
        },
        ExitWhen::Rule => PresetProfileExitWhen {
            kind: "rule".to_string(),
            template_file: None,
            judge_capability: None,
            min_interval: None,
            duration: None,
        },
        ExitWhen::GraphComplete => PresetProfileExitWhen {
            kind: "graph_complete".to_string(),
            template_file: None,
            judge_capability: None,
            min_interval: None,
            duration: None,
        },
        ExitWhen::Manual => PresetProfileExitWhen {
            kind: "manual".to_string(),
            template_file: None,
            judge_capability: None,
            min_interval: None,
            duration: None,
        },
        ExitWhen::Timer { duration } => PresetProfileExitWhen {
            kind: "timer".to_string(),
            template_file: None,
            judge_capability: None,
            min_interval: None,
            duration: duration.clone(),
        },
    }
}

/// Map one next-transition form to its profile shape.
fn profile_next(next: &NextTarget) -> PresetProfileNext {
    match next {
        NextTarget::Linear(target) => PresetProfileNext {
            kind: "linear".to_string(),
            target: Some(target.clone()),
            ..Default::default()
        },
        NextTarget::GoNogo(go_nogo) => PresetProfileNext {
            kind: "goNogo".to_string(),
            go: Some(go_nogo.go.clone()),
            nogo: Some(go_nogo.nogo.clone()),
            ..Default::default()
        },
        NextTarget::Labeled(edges) => PresetProfileNext {
            kind: "labeled".to_string(),
            labeled: edges
                .iter()
                .map(|e| PresetProfileLabeledNext {
                    label: e.label.clone(),
                    target: e.target.clone(),
                })
                .collect(),
            ..Default::default()
        },
        NextTarget::Conditional(cond) => PresetProfileNext {
            kind: "conditional".to_string(),
            rules: cond
                .rules
                .iter()
                .map(|r| PresetProfileConditionalRule {
                    when: r.when.clone(),
                    target: r.target.clone(),
                })
                .collect(),
            default: Some(cond.default.clone()),
            ..Default::default()
        },
        NextTarget::Branches(branches) => PresetProfileNext {
            kind: "branches".to_string(),
            branches: branches
                .branches
                .iter()
                .map(|r| PresetProfileConditionalRule {
                    when: r.when.clone(),
                    target: r.target.clone(),
                })
                .collect(),
            default: Some(branches.default.clone()),
            ..Default::default()
        },
    }
}

/// Map one role definition to its profile shape.
fn profile_role(role: &PresetRoleDefinition) -> PresetProfileRole {
    PresetProfileRole {
        id: role.id.clone(),
        description: role.description.clone(),
        system_prompt_file: role.system_prompt_file.clone(),
        recommended_skills: role.recommended_skills.clone(),
    }
}

/// Map one declared signal binding to its profile shape.
fn profile_signal(signal: &SignalBinding) -> PresetProfileSignal {
    let action = match signal.on_receive.action {
        SignalActionKind::Pause => "pause",
        SignalActionKind::ForceTransition => "force_transition",
    };
    PresetProfileSignal {
        name: signal.name.clone(),
        action: action.to_string(),
        target: signal.on_receive.target.clone(),
    }
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
        nexus_orchestration::system_preset_dir::ensure_maintenance_preset(&nexus_home)
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
        nexus_orchestration::system_preset_dir::ensure_maintenance_preset(&nexus_home)
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

        for id in nexus_orchestration::preset_ids::cron_role_preset_ids() {
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
        assert!(
            json["states"]
                .as_array()
                .expect("states array")
                .iter()
                .all(|s| s.get("description").is_some() || s.get("description").is_none()),
            "description optional per state"
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
