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
    let (first, second) = tokio::join!(
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
    let results = [first, second];
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        results
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
