//! P3-T4: stale editors cannot overwrite source; authoring stays engine-free.

use nexus_contracts::{
    StrategyPatchPromptTemplateRequest, StrategyPatchStateRequest, StrategyPatchTransitionRequest,
};
use nexus_core::{CoreAccess, CoreError, CoreOpenOptions, CoreService, PresetError, Principal};
use nexus_preset::capability_catalog::BuiltinCapabilityCatalog;
use std::path::PathBuf;
use tempfile::TempDir;

struct Fixture {
    tmp: TempDir,
    core: CoreService,
    principal: Principal,
    bundle: PathBuf,
}

async fn setup() -> Fixture {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path().join(".nexus42");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(nexus_home_layout::operational_workspace_dir(
        tmp.path(),
        "test_creator",
        "default",
    ))
    .unwrap();
    std::fs::write(home.join("config.toml"), "active_creator_id = \"test_creator\"\n[active_workspace_slug_by_creator]\n\"test_creator\" = \"default\"\n").unwrap();
    let core = CoreService::open(CoreOpenOptions {
        user_home: tmp.path().to_path_buf(),
        access: CoreAccess::DirectWriter,
    })
    .await
    .unwrap();
    let principal = core.active_principal().await.unwrap();
    let bundle = home.join("presets/test-strategy");
    std::fs::create_dir_all(&bundle).unwrap();
    std::fs::write(
        bundle.join("preset.yaml"),
        r#"revision: 1
preset:
  id: test-strategy
  version: 1
  kind: creator
  description: "Strategy test"
  run_intents: [work_init]
  initial: start
  terminal: end
states:
  - id: start
    description: "Original"
    next: end
  - id: end
    terminal: true
"#,
    )
    .unwrap();
    Fixture {
        tmp,
        core,
        principal,
        bundle,
    }
}

fn state_request(revision: u64, description: &str) -> StrategyPatchStateRequest {
    serde_json::from_value(serde_json::json!({
        "strategy_id": "test-strategy", "state_id": "start",
        "base_revision": revision, "set": {"description": description}
    }))
    .unwrap()
}

#[tokio::test]
async fn stale_source_hash_cannot_overwrite_strategy() {
    let fixture = setup().await;
    fixture
        .core
        .patch_strategy_state(
            &fixture.principal,
            "test-strategy".into(),
            "start".into(),
            state_request(1, "New source"),
        )
        .await
        .unwrap();
    let committed = nexus_preset::load_preset(&fixture.bundle, &BuiltinCapabilityCatalog)
        .unwrap()
        .source_identity
        .unwrap();
    let bytes = std::fs::read(fixture.bundle.join("preset.yaml")).unwrap();
    let error = fixture
        .core
        .patch_strategy_state(
            &fixture.principal,
            "test-strategy".into(),
            "start".into(),
            state_request(1, "Stale source"),
        )
        .await
        .unwrap_err();
    assert!(
        matches!(&error, CoreError::Preset(PresetError::StrategyConflict(conflict)) if conflict.current_revision == 2)
    );
    assert_eq!(
        std::fs::read(fixture.bundle.join("preset.yaml")).unwrap(),
        bytes
    );
    assert_eq!(
        nexus_preset::load_preset(&fixture.bundle, &BuiltinCapabilityCatalog)
            .unwrap()
            .source_identity,
        Some(committed)
    );
    fixture.core.close().await.unwrap();
}

#[tokio::test]
async fn concurrent_editors_have_one_commit_and_one_conflict() {
    let fixture = setup().await;
    // `tokio::join!` already yields a tuple; collect it directly rather than
    // re-spelling the two results as an array.
    let results = tokio::join!(
        fixture.core.patch_strategy_state(
            &fixture.principal,
            "test-strategy".into(),
            "start".into(),
            state_request(1, "First")
        ),
        fixture.core.patch_strategy_state(
            &fixture.principal,
            "test-strategy".into(),
            "start".into(),
            state_request(1, "Second")
        ),
    );
    assert_eq!(
        [&results.0, &results.1]
            .iter()
            .filter(|r| r.is_ok())
            .count(),
        1
    );
    assert_eq!(
        [&results.0, &results.1]
            .iter()
            .filter(|result| matches!(
                result,
                Err(CoreError::Preset(PresetError::StrategyConflict(_)))
            ))
            .count(),
        1
    );
    let yaml: serde_yaml::Value =
        serde_yaml::from_str(&std::fs::read_to_string(fixture.bundle.join("preset.yaml")).unwrap())
            .unwrap();
    assert_eq!(yaml["revision"].as_u64(), Some(2));
    fixture.core.close().await.unwrap();
}

#[tokio::test]
async fn embedded_sources_reject_edits_without_creating_user_bundle() {
    let fixture = setup().await;
    let request = serde_json::from_value(serde_json::json!({
        "strategy_id": "novel-writing", "state_id": "done", "base_revision": 0,
        "set": {"description": "Forbidden"}
    }))
    .unwrap();
    let error = fixture
        .core
        .patch_strategy_state(
            &fixture.principal,
            "novel-writing".into(),
            "done".into(),
            request,
        )
        .await
        .unwrap_err();
    assert!(
        matches!(&error, CoreError::Preset(PresetError::Rejected {code, ..}) if code == "strategy_update_forbidden")
    );
    assert!(!fixture
        .tmp
        .path()
        .join(".nexus42/presets/novel-writing")
        .exists());
    fixture.core.close().await.unwrap();
}

#[tokio::test]
async fn read_only_and_stale_principals_cannot_edit_sources() {
    let fixture = setup().await;
    let bytes = std::fs::read(fixture.bundle.join("preset.yaml")).unwrap();
    let reader = CoreService::open(CoreOpenOptions {
        user_home: fixture.tmp.path().to_path_buf(),
        access: CoreAccess::ReadOnly,
    })
    .await
    .unwrap();
    let principal = reader.active_principal().await.unwrap();
    assert!(matches!(
        reader
            .patch_strategy_state(
                &principal,
                "test-strategy".into(),
                "start".into(),
                state_request(1, "Forbidden")
            )
            .await,
        Err(CoreError::Forbidden { .. })
    ));
    reader.close().await.unwrap();
    std::fs::write(
        fixture.tmp.path().join(".nexus42/config.toml"),
        "active_creator_id = \"other_creator\"\n",
    )
    .unwrap();
    assert!(matches!(
        fixture
            .core
            .patch_strategy_state(
                &fixture.principal,
                "test-strategy".into(),
                "start".into(),
                state_request(1, "Stale principal")
            )
            .await,
        Err(CoreError::AuthRequired)
    ));
    assert_eq!(
        std::fs::read(fixture.bundle.join("preset.yaml")).unwrap(),
        bytes
    );
    fixture.core.close().await.unwrap();
}

#[tokio::test]
async fn rename_updates_references_and_invalid_transition_preserves_source() {
    let fixture = setup().await;
    let request = serde_json::from_value(serde_json::json!({
        "strategy_id": "test-strategy", "state_id": "start", "base_revision": 1,
        "set": {"label": "begin", "description": "Renamed"}
    }))
    .unwrap();
    fixture
        .core
        .patch_strategy_state(
            &fixture.principal,
            "test-strategy".into(),
            "start".into(),
            request,
        )
        .await
        .unwrap();
    let bytes = std::fs::read(fixture.bundle.join("preset.yaml")).unwrap();
    let yaml: serde_yaml::Value = serde_yaml::from_slice(&bytes).unwrap();
    assert_eq!(yaml["preset"]["initial"].as_str(), Some("begin"));
    assert_eq!(yaml["states"][0]["id"].as_str(), Some("begin"));
    assert_eq!(yaml["states"][0]["description"].as_str(), Some("Renamed"));
    let request: StrategyPatchTransitionRequest = serde_json::from_value(serde_json::json!({
        "strategy_id": "test-strategy", "source_state_id": "begin", "base_revision": 2,
        "old_target": "end", "condition": "not an expression @#$", "op": "update"
    }))
    .unwrap();
    let error = fixture
        .core
        .patch_strategy_transition(&fixture.principal, "test-strategy".into(), request)
        .await
        .unwrap_err();
    assert!(
        matches!(&error, CoreError::Preset(PresetError::Rejected {code, ..}) if code == "strategy_transition_condition_invalid")
    );
    assert_eq!(
        std::fs::read(fixture.bundle.join("preset.yaml")).unwrap(),
        bytes
    );
    fixture.core.close().await.unwrap();
}

#[tokio::test]
async fn prompt_validation_failure_restores_bytes_and_revision() {
    let fixture = setup().await;
    std::fs::create_dir(fixture.bundle.join("prompts")).unwrap();
    let path = fixture.bundle.join("prompts/other.md");
    std::fs::write(&path, "Original template").unwrap();
    let yaml_path = fixture.bundle.join("preset.yaml");
    let mut yaml: serde_yaml::Value =
        serde_yaml::from_slice(&std::fs::read(&yaml_path).unwrap()).unwrap();
    yaml["states"][0]["context_update"] =
        serde_yaml::from_str("op: { kind: append, body: '' }\ntemplate_file: prompts/missing.md")
            .unwrap();
    std::fs::write(&yaml_path, serde_yaml::to_string(&yaml).unwrap()).unwrap();
    let before = std::fs::read(&yaml_path).unwrap();
    let request: StrategyPatchPromptTemplateRequest = serde_json::from_value(serde_json::json!({
        "strategy_id": "test-strategy", "state_id": "start", "base_revision": 1,
        "template_ref": "prompts/other.md", "set": {"body": "Replacement"}
    }))
    .unwrap();
    assert!(matches!(
        fixture
            .core
            .patch_strategy_prompt_template(
                &fixture.principal,
                "test-strategy".into(),
                "start".into(),
                request
            )
            .await,
        Err(CoreError::Preset(PresetError::StrategyValidation(_)))
    ));
    assert_eq!(std::fs::read_to_string(path).unwrap(), "Original template");
    assert_eq!(std::fs::read(yaml_path).unwrap(), before);
    fixture.core.close().await.unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn new_template_cannot_escape_through_symlink_parent() {
    let fixture = setup().await;
    let outside = TempDir::new().unwrap();
    std::os::unix::fs::symlink(outside.path(), fixture.bundle.join("prompts")).unwrap();
    let request: StrategyPatchPromptTemplateRequest = serde_json::from_value(serde_json::json!({
        "strategy_id": "test-strategy", "state_id": "start", "base_revision": 1,
        "template_ref": "prompts/new.md", "set": {"body": "Forbidden"}
    }))
    .unwrap();
    assert!(matches!(
        fixture
            .core
            .patch_strategy_prompt_template(
                &fixture.principal,
                "test-strategy".into(),
                "start".into(),
                request
            )
            .await,
        Err(CoreError::Preset(PresetError::Forbidden { .. }))
    ));
    assert!(!outside.path().join("new.md").exists());
    fixture.core.close().await.unwrap();
}

// ─────────────────────────────────────────────────────────────────────────────
// Migrated from the retired daemon HTTP fixtures (P2-T4): `strategy_patch.rs`
// (the route handlers wrapped this core CAS seam directly) and
// `presets_route_api.rs` (router registration and framework-404 expectations
// retire; the catalog/scaffold behavior stays core-owned). State rename/stale
// source/concurrent editor/prompt-template rollback cases above already cover
// the remaining `strategy_patch.rs` assertions.
// ─────────────────────────────────────────────────────────────────────────────

/// A bundle whose `start` state has no outgoing transition.
const LINEAR_BUNDLE: &str = r#"revision: 1
preset:
  id: test-strategy
  version: 1
  kind: creator
  description: "Strategy test"
  run_intents: [work_init]
  initial: start
  terminal: end
states:
  - id: start
    description: "Start"
  - id: end
    terminal: true
"#;

/// A bundle whose `start` state already carries a conditional next map.
const CONDITIONAL_BUNDLE: &str = r#"revision: 1
preset:
  id: test-strategy
  version: 1
  kind: creator
  description: "Strategy test"
  run_intents: [work_init]
  initial: start
  terminal: end
states:
  - id: start
    description: "Start"
    next:
      kind: conditional
      rules:
        - to: end
          when: "_context.ready"
      default: end
  - id: end
    terminal: true
  - id: alt
    terminal: true
"#;

/// A linear bundle that already declares the rewire target state.
const REWIRE_BUNDLE: &str = r#"revision: 1
preset:
  id: test-strategy
  version: 1
  kind: creator
  description: "Strategy test"
  run_intents: [work_init]
  initial: start
  terminal: end
states:
  - id: start
    description: "Start"
    next: end
  - id: end
    terminal: true
  - id: alt
    terminal: true
"#;

fn write_bundle(fixture: &Fixture, yaml: &str) {
    std::fs::write(fixture.bundle.join("preset.yaml"), yaml).unwrap();
}

fn bundle_yaml(fixture: &Fixture) -> serde_yaml::Value {
    serde_yaml::from_slice(&std::fs::read(fixture.bundle.join("preset.yaml")).unwrap()).unwrap()
}

fn transition_request(
    source_state_id: &str,
    old_target: Option<&str>,
    new_target: Option<&str>,
    condition: Option<&str>,
    transition_kind: Option<&str>,
    op: &str,
) -> StrategyPatchTransitionRequest {
    serde_json::from_value(serde_json::json!({
        "strategy_id": "test-strategy",
        "base_revision": 1,
        "source_state_id": source_state_id,
        "old_target": old_target,
        "new_target": new_target,
        "condition": condition,
        "transition_kind": transition_kind,
        "op": op,
    }))
    .unwrap()
}

/// Transition create/update semantics on the real bundle bytes
/// (`strategy_patch.rs::patch_transition_create_sets_linear_next_when_absent`,
/// `patch_transition_create_appends_rule_when_next_is_map`,
/// `patch_transition_create_honors_transition_kind_default`,
/// `patch_transition_create_branch_seeds_conditional_map`,
/// `patch_transition_default_op_preserves_update_semantics`).
#[tokio::test]
async fn retained_strategy_transition_create_and_rewire_semantics() {
    let fixture = setup().await;

    // (a) Absent `next` → the create writes the linear edge.
    write_bundle(&fixture, LINEAR_BUNDLE);
    let created = fixture
        .core
        .patch_strategy_transition(
            &fixture.principal,
            "test-strategy".into(),
            transition_request("start", None, Some("end"), None, None, "create"),
        )
        .await
        .unwrap();
    assert_eq!(created.new_revision.get(), 2);
    assert_eq!(bundle_yaml(&fixture)["states"][0]["next"], "end");

    // (b) An existing conditional map gains a rule and keeps its default.
    write_bundle(&fixture, CONDITIONAL_BUNDLE);
    fixture
        .core
        .patch_strategy_transition(
            &fixture.principal,
            "test-strategy".into(),
            transition_request(
                "start",
                None,
                Some("alt"),
                Some("_context.branch_b"),
                None,
                "create",
            ),
        )
        .await
        .unwrap();
    let next = bundle_yaml(&fixture)["states"][0]["next"].clone();
    let rules = next["rules"].as_sequence().unwrap();
    assert_eq!(rules.len(), 2, "the new rule is appended: {rules:?}");
    assert_eq!(rules[1]["to"], "alt");
    assert_eq!(rules[1]["when"], "_context.branch_b");
    assert_eq!(next["default"], "end");

    // (c) `transition_kind: default` rewrites the map default instead of a rule.
    write_bundle(&fixture, CONDITIONAL_BUNDLE);
    fixture
        .core
        .patch_strategy_transition(
            &fixture.principal,
            "test-strategy".into(),
            transition_request("start", None, Some("alt"), None, Some("default"), "create"),
        )
        .await
        .unwrap();
    let next = bundle_yaml(&fixture)["states"][0]["next"].clone();
    assert_eq!(next["default"], "alt");
    assert_eq!(next["rules"].as_sequence().unwrap().len(), 1);

    // (d) `transition_kind: branch` seeds a conditional map on a linear state.
    write_bundle(&fixture, LINEAR_BUNDLE);
    fixture
        .core
        .patch_strategy_transition(
            &fixture.principal,
            "test-strategy".into(),
            transition_request(
                "start",
                None,
                Some("end"),
                Some("_context.ready"),
                Some("branch"),
                "create",
            ),
        )
        .await
        .unwrap();
    let next = bundle_yaml(&fixture)["states"][0]["next"].clone();
    assert_eq!(next["kind"], "conditional");
    assert_eq!(next["rules"][0]["to"], "end");
    assert_eq!(next["rules"][0]["when"], "_context.ready");

    // (e) The default `update` op rewires an existing edge.
    write_bundle(&fixture, REWIRE_BUNDLE);
    fixture
        .core
        .patch_strategy_transition(
            &fixture.principal,
            "test-strategy".into(),
            transition_request("start", Some("end"), Some("alt"), None, None, "update"),
        )
        .await
        .unwrap();
    assert_eq!(bundle_yaml(&fixture)["states"][0]["next"], "alt");
    fixture.core.close().await.unwrap();
}

/// Transition validation rejections leave the bundle bytes untouched
/// (`strategy_patch.rs::patch_transition_create_rejects_duplicate_rule`,
/// `patch_transition_create_rejects_self_loop`,
/// `patch_transition_create_rejects_explicit_next_on_conditional_map`,
/// `patch_transition_rejects_create_without_new_target`,
/// `patch_transition_rejects_update_without_old_target`).
#[tokio::test]
async fn retained_strategy_transition_validation_rejections() {
    let fixture = setup().await;
    let cases: [(&str, &str, StrategyPatchTransitionRequest, &str); 5] = [
        (
            "duplicate rule",
            CONDITIONAL_BUNDLE,
            transition_request(
                "start",
                None,
                Some("end"),
                Some("_context.ready"),
                None,
                "create",
            ),
            "strategy_transition_duplicate",
        ),
        (
            "self loop",
            LINEAR_BUNDLE,
            transition_request("start", None, Some("start"), None, None, "create"),
            "strategy_self_loop",
        ),
        (
            "explicit next on conditional map",
            CONDITIONAL_BUNDLE,
            transition_request("start", None, Some("alt"), None, Some("next"), "create"),
            "strategy_transition_already_conditional",
        ),
        (
            "create without new target",
            LINEAR_BUNDLE,
            transition_request("start", None, None, None, None, "create"),
            "strategy_transition_missing_new_target",
        ),
        (
            "update without old target",
            REWIRE_BUNDLE,
            transition_request("start", None, Some("alt"), None, None, "update"),
            "strategy_transition_missing_old_target",
        ),
    ];

    for (label, bundle, request, expected_code) in cases {
        write_bundle(&fixture, bundle);
        let before = std::fs::read(fixture.bundle.join("preset.yaml")).unwrap();
        let error = fixture
            .core
            .patch_strategy_transition(&fixture.principal, "test-strategy".into(), request)
            .await
            .unwrap_err();
        assert!(
            matches!(&error, CoreError::Preset(PresetError::Rejected { code, .. }) if code == expected_code),
            "{label}: expected {expected_code}, got {error:?}"
        );
        assert_eq!(
            std::fs::read(fixture.bundle.join("preset.yaml")).unwrap(),
            before,
            "{label}: a rejected transition must not rewrite the bundle"
        );
    }
    fixture.core.close().await.unwrap();
}

/// Catalog reads, unknown lookup and the scaffold round trip
/// (`presets_route_api.rs::get_preset_by_id_hits_handler_not_framework_404`,
/// `get_preset_unknown_returns_handler_json_404_not_empty_body`,
/// `scaffold_then_get_user_preset_round_trip`).
#[tokio::test]
async fn retained_preset_catalog_scaffold_and_unknown_lookup() {
    let fixture = setup().await;

    let listed = fixture.core.list_presets(&fixture.principal).await.unwrap();
    assert!(
        listed.embedded.iter().any(|preset| preset.id == "novel-writing"),
        "embedded catalog lists builtins"
    );

    let embedded = fixture
        .core
        .get_preset(&fixture.principal, "novel-writing".into())
        .await
        .unwrap();
    assert_eq!(
        embedded.source,
        nexus_contracts::GetPresetResponseSource::Embedded
    );
    assert!(embedded.yaml.contains("preset"), "raw preset.yaml is read back");
    assert!(embedded.path.is_none(), "embedded presets have no path");

    assert!(matches!(
        fixture
            .core
            .get_preset(&fixture.principal, "definitely-missing-preset-id".into())
            .await,
        Err(CoreError::Preset(PresetError::NotFound(_)))
    ));

    let scaffolded = fixture
        .core
        .scaffold_preset(
            &fixture.principal,
            nexus_contracts::ScaffoldPresetRequest {
                name: "route-test-preset".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(scaffolded.id, "route-test-preset");
    assert!(
        std::path::Path::new(&scaffolded.path)
            .join("preset.yaml")
            .is_file(),
        "scaffold writes a readable bundle at {}",
        scaffolded.path
    );
    let user = fixture
        .core
        .get_preset(&fixture.principal, "route-test-preset".into())
        .await
        .unwrap();
    assert_eq!(user.source, nexus_contracts::GetPresetResponseSource::User);
    assert!(user.yaml.contains("route-test-preset"));
    fixture.core.close().await.unwrap();
}
