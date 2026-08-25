//! Hermetic CLI integration tests — `preset patch state|transition|prompt`
//! (V1.175 P1 Task 2, group 1): CAS-guarded strategy canvas writes over the
//! existing daemon routes, end-to-end against a live daemon fixture with
//! hermetic `HOME` (AR-83 #6 / AR-84 group 1).
//!
//! Each test seeds a user preset bundle at `<HOME>/.nexus42/presets/<id>/`
//! (the canonical layout the strategy patch handlers read/write), then
//! drives the REAL `nexus42` binary. Failure paths: one conflict path per
//! leaf (stale `--base-revision` → 409 `strategy_conflict` rendering
//! current revision + conflicting path + recovery hint), plus 404 and
//! `strategy_invalid` surfaces.

mod common;

use common::LiveDaemon;
use serde_json::Value;
use std::path::Path;
use std::process::Output;

/// Seed a minimal valid user preset bundle at the canonical
/// `<HOME>/.nexus42/presets/<id>/` layout and return the bundle dir.
fn seed_bundle(home: &Path, id: &str) -> std::path::PathBuf {
    let bundle_dir = home.join(".nexus42").join("presets").join(id);
    std::fs::create_dir_all(&bundle_dir).expect("create bundle dir");
    let yaml = r#"
revision: 1
preset:
  id: test-strategy
  version: 1
  kind: creator
  description: "Test strategy for CLI patch leaves"
  run_intents: [work_init]
  initial: start
  terminal: end
states:
  - id: start
    description: "Start state"
    next: end
  - id: draft
    description: "Draft state (no outgoing transition)"
  - id: end
    terminal: true
"#;
    std::fs::write(bundle_dir.join("preset.yaml"), yaml).expect("write preset.yaml");
    bundle_dir
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

// ── State leaf ─────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn patch_state_renames_and_bumps_revision() {
    let d = LiveDaemon::start().await;
    let bundle_dir = seed_bundle(d.home.path(), "test-strategy");

    let out = d
        .cli(&[
            "preset",
            "patch",
            "state",
            "test-strategy",
            "start",
            "--base-revision",
            "1",
            "--label",
            "begin",
            "--description",
            "Renamed start state",
        ])
        .await;
    assert!(out.status.success(), "patch state failed: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("Patched state 'start'"), "{text}");
    assert!(text.contains("new_revision: 2"), "{text}");

    let yaml = std::fs::read_to_string(bundle_dir.join("preset.yaml")).unwrap();
    assert!(yaml.contains("revision: 2"), "{yaml}");
    assert!(yaml.contains("id: begin"), "{yaml}");
    assert!(yaml.contains("Renamed start state"), "{yaml}");
    // The rename rewrites references: initial + next targets.
    assert!(yaml.contains("initial: begin"), "{yaml}");
    assert!(yaml.contains("next: end"), "{yaml}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn patch_state_json_emits_dto_verbatim() {
    let d = LiveDaemon::start().await;
    seed_bundle(d.home.path(), "test-strategy");

    let out = d
        .cli(&[
            "preset",
            "patch",
            "state",
            "test-strategy",
            "start",
            "--base-revision",
            "1",
            "--description",
            "JSON description",
            "--json",
        ])
        .await;
    assert!(
        out.status.success(),
        "patch state --json failed: {}",
        stderr(&out)
    );
    let json: Value = serde_json::from_str(&stdout(&out)).expect("json output");
    assert_eq!(json["new_revision"], 2);
    assert!(json["validation_summary"]["errors"].is_array());
    assert!(json["validation_summary"]["warnings"].is_array());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn patch_state_stale_revision_surfaces_conflict() {
    let d = LiveDaemon::start().await;
    seed_bundle(d.home.path(), "test-strategy");

    // base_revision 0 vs current 1 → 409 strategy_conflict.
    let out = d
        .cli(&[
            "preset",
            "patch",
            "state",
            "test-strategy",
            "start",
            "--base-revision",
            "0",
            "--description",
            "stale",
        ])
        .await;
    assert!(!out.status.success(), "stale revision must fail");
    let err = stderr(&out);
    assert!(err.contains("strategy_conflict"), "stderr: {err}");
    assert!(err.contains("409"), "stderr should carry HTTP 409: {err}");
    // All three conflict fields render: current revision, conflicting path,
    // recovery hint.
    assert!(err.contains("current_revision"), "stderr: {err}");
    assert!(err.contains("conflicting_path"), "stderr: {err}");
    assert!(err.contains("recovery_hint"), "stderr: {err}");
    assert!(err.contains("states"), "stderr should name the path: {err}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn patch_state_unknown_strategy_surfaces_404() {
    let d = LiveDaemon::start().await;

    let out = d
        .cli(&[
            "preset",
            "patch",
            "state",
            "does-not-exist",
            "start",
            "--base-revision",
            "1",
            "--description",
            "x",
        ])
        .await;
    assert!(!out.status.success(), "unknown strategy must fail");
    let err = stderr(&out);
    assert!(
        err.contains("404") || err.to_lowercase().contains("not found"),
        "stderr should surface the daemon 404: {err}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn patch_state_requires_set_flag() {
    let d = LiveDaemon::start().await;
    seed_bundle(d.home.path(), "test-strategy");

    let out = d
        .cli(&[
            "preset",
            "patch",
            "state",
            "test-strategy",
            "start",
            "--base-revision",
            "1",
        ])
        .await;
    assert!(!out.status.success(), "no set flag must fail");
    assert!(
        stderr(&out).contains("--label"),
        "stderr should name --label: {}",
        stderr(&out)
    );
}

// ── Transition leaf ────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn patch_transition_update_rewires_target() {
    let d = LiveDaemon::start().await;
    let bundle_dir = seed_bundle(d.home.path(), "test-strategy");

    // Linear update: start -> end rewired to the same target is a no-op
    // that still bumps the revision (daemon semantics — the update path
    // matches the old target and rewrites it).
    let out = d
        .cli(&[
            "preset",
            "patch",
            "transition",
            "test-strategy",
            "--base-revision",
            "1",
            "--source-state",
            "start",
            "--op",
            "update",
            "--old-target",
            "end",
            "--new-target",
            "end",
        ])
        .await;
    assert!(
        out.status.success(),
        "patch transition update failed: {}",
        stderr(&out)
    );
    let text = stdout(&out);
    assert!(text.contains("Patched transition from 'start'"), "{text}");
    assert!(text.contains("new_revision: 2"), "{text}");

    let yaml = std::fs::read_to_string(bundle_dir.join("preset.yaml")).unwrap();
    assert!(yaml.contains("revision: 2"), "{yaml}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn patch_transition_create_branch() {
    let d = LiveDaemon::start().await;
    let bundle_dir = seed_bundle(d.home.path(), "test-strategy");

    // 'draft' has no outgoing transition (and is not terminal), so a
    // branch create seeds a conditional map with a default target.
    let out = d
        .cli(&[
            "preset",
            "patch",
            "transition",
            "test-strategy",
            "--base-revision",
            "1",
            "--source-state",
            "draft",
            "--op",
            "create",
            "--new-target",
            "start",
            "--transition-kind",
            "branch",
            "--condition",
            "_context._judge_result == true",
        ])
        .await;
    assert!(
        out.status.success(),
        "patch transition create failed: {}",
        stderr(&out)
    );
    let text = stdout(&out);
    assert!(text.contains("new_revision: 2"), "{text}");

    let yaml = std::fs::read_to_string(bundle_dir.join("preset.yaml")).unwrap();
    assert!(yaml.contains("revision: 2"), "{yaml}");
    assert!(yaml.contains("kind: conditional"), "{yaml}");
    assert!(yaml.contains("_context._judge_result == true"), "{yaml}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn patch_transition_stale_revision_surfaces_conflict() {
    let d = LiveDaemon::start().await;
    seed_bundle(d.home.path(), "test-strategy");

    let out = d
        .cli(&[
            "preset",
            "patch",
            "transition",
            "test-strategy",
            "--base-revision",
            "0",
            "--source-state",
            "start",
            "--op",
            "update",
            "--old-target",
            "end",
        ])
        .await;
    assert!(!out.status.success(), "stale revision must fail");
    let err = stderr(&out);
    assert!(err.contains("strategy_conflict"), "stderr: {err}");
    assert!(err.contains("current_revision"), "stderr: {err}");
    assert!(err.contains("conflicting_path"), "stderr: {err}");
    assert!(err.contains("recovery_hint"), "stderr: {err}");
    assert!(
        err.contains("transitions"),
        "stderr should name the path: {err}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn patch_transition_missing_old_target_fails_fast() {
    let d = LiveDaemon::start().await;
    seed_bundle(d.home.path(), "test-strategy");

    let out = d
        .cli(&[
            "preset",
            "patch",
            "transition",
            "test-strategy",
            "--base-revision",
            "1",
            "--source-state",
            "start",
            "--op",
            "update",
        ])
        .await;
    assert!(!out.status.success(), "missing --old-target must fail");
    assert!(
        stderr(&out).contains("--old-target"),
        "stderr should name --old-target: {}",
        stderr(&out)
    );
}

// ── Prompt leaf ────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn patch_prompt_writes_template_and_bumps_revision() {
    let d = LiveDaemon::start().await;
    let bundle_dir = seed_bundle(d.home.path(), "test-strategy");

    let template_path = bundle_dir.join("prompts/start.md");
    std::fs::create_dir_all(template_path.parent().unwrap()).expect("create prompts dir");
    std::fs::write(&template_path, "# Hello\n").expect("write template");

    let out = d
        .cli(&[
            "preset",
            "patch",
            "prompt",
            "test-strategy",
            "start",
            "--base-revision",
            "1",
            "--template-ref",
            "prompts/start.md",
            "--file",
            template_path.to_str().unwrap(),
        ])
        .await;
    assert!(
        out.status.success(),
        "patch prompt failed: {}",
        stderr(&out)
    );
    let text = stdout(&out);
    assert!(text.contains("Patched prompt template"), "{text}");
    assert!(text.contains("new_revision: 2"), "{text}");

    let body = std::fs::read_to_string(&template_path).unwrap();
    assert_eq!(body, "# Hello\n");
    let yaml = std::fs::read_to_string(bundle_dir.join("preset.yaml")).unwrap();
    assert!(yaml.contains("revision: 2"), "{yaml}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn patch_prompt_stale_revision_surfaces_conflict() {
    let d = LiveDaemon::start().await;
    let bundle_dir = seed_bundle(d.home.path(), "test-strategy");
    let template_path = bundle_dir.join("prompts/start.md");
    std::fs::create_dir_all(template_path.parent().unwrap()).expect("create prompts dir");
    std::fs::write(&template_path, "# Hello\n").expect("write template");

    let out = d
        .cli(&[
            "preset",
            "patch",
            "prompt",
            "test-strategy",
            "start",
            "--base-revision",
            "0",
            "--template-ref",
            "prompts/start.md",
            "--file",
            template_path.to_str().unwrap(),
        ])
        .await;
    assert!(!out.status.success(), "stale revision must fail");
    let err = stderr(&out);
    assert!(err.contains("strategy_conflict"), "stderr: {err}");
    assert!(err.contains("current_revision"), "stderr: {err}");
    assert!(err.contains("conflicting_path"), "stderr: {err}");
    assert!(err.contains("recovery_hint"), "stderr: {err}");
    assert!(
        err.contains("prompt:prompts/start.md"),
        "stderr should name the prompt path: {err}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn patch_prompt_missing_file_fails_fast() {
    let d = LiveDaemon::start().await;
    seed_bundle(d.home.path(), "test-strategy");

    let out = d
        .cli(&[
            "preset",
            "patch",
            "prompt",
            "test-strategy",
            "start",
            "--base-revision",
            "1",
            "--template-ref",
            "prompts/start.md",
            "--file",
            "/nonexistent/prompt.md",
        ])
        .await;
    assert!(!out.status.success(), "missing file must fail");
    assert!(
        stderr(&out).contains("--file"),
        "stderr should name --file: {}",
        stderr(&out)
    );
}

// ── Help documents the retry guidance ─────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn patch_help_documents_base_revision_retry() {
    let d = LiveDaemon::start().await;

    let out = d.cli(&["preset", "patch", "--help"]).await;
    assert!(
        out.status.success(),
        "patch --help failed: {}",
        stderr(&out)
    );
    let text = stdout(&out);
    assert!(text.contains("state"), "{text}");
    assert!(text.contains("transition"), "{text}");
    assert!(text.contains("prompt"), "{text}");
    assert!(text.contains("--base-revision"), "{text}");
    assert!(text.contains("strategy_conflict"), "{text}");
    assert!(
        text.to_lowercase().contains("re-read"),
        "help should document the re-read retry guidance: {text}"
    );
}
