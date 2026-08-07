//! Durable sample self-check (L1 evidence — P3 QC fix wave W2).
//!
//! Pins two facts that previously lived only in a temporary test:
//!
//! 1. The bundled `strategy-samples/game-narrative/` strategy stays valid
//!    against the real validator core (`load_preset` + `validate_preset_semantic`
//!    + `validate_assets_in_bundle` + `validate_path_safety`), with zero error
//!    AND zero warning diagnostics.
//! 2. The expression-routing constraint that forced the sample's `llm_judge`
//!    lane selector: a dotted `preset.input.*` key is unreachable in
//!    `_context` expressions (the context is a flat map and the grammar has no
//!    bracket syntax), so lane selection must come from `_context._judge_result`,
//!    written by an `llm_judge` exit before branch evaluation.
//!
//! The bundle lives at the repo root (`strategy-samples/game-narrative`); this
//! crate resolves it via `CARGO_MANIFEST_DIR` so the test passes regardless of
//! the invoking working directory.

use nexus_orchestration::preset::manifest::{EnterAction, ExitWhen, NextTarget};
use nexus_orchestration::preset::{
    expr, load_preset, validate_assets_in_bundle, validate_path_safety, validate_preset_semantic,
};
use nexus_orchestration::CapabilityRegistry;

/// Repo-root-relative path to the bundled game-narrative sample.
fn sample_bundle_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../strategy-samples/game-narrative")
}

#[test]
fn game_narrative_sample_validates_clean() {
    let caps = CapabilityRegistry::with_builtins();
    let bundle_root = sample_bundle_root();
    assert!(
        bundle_root.join("preset.yaml").is_file(),
        "sample bundle missing at {}",
        bundle_root.display()
    );

    // Real validator core: loader (structural + A3 path/assets) first...
    let loaded = load_preset(&bundle_root, &caps)
        .unwrap_or_else(|e| panic!("sample must load cleanly: {e:?}"));
    assert_eq!(loaded.id, "game-narrative");

    // ...then the shared semantic surface + explicit A3 surfaces, asserting
    // zero error AND zero warning diagnostics.
    let semantic = validate_preset_semantic(&loaded.manifest, &caps);
    let assets = validate_assets_in_bundle(&loaded.manifest, &bundle_root);
    let path_safety = validate_path_safety(&loaded.manifest);
    for (label, result) in [
        ("validate_preset_semantic", semantic),
        ("validate_assets_in_bundle", assets),
        ("validate_path_safety", path_safety),
    ] {
        assert!(
            result.diagnostics.is_empty(),
            "sample must validate with zero diagnostics ({label}): {:#?}",
            result.diagnostics
        );
    }

    // Pin the scheduled-lane shape (P3 QC fix wave W1): scan+judge state
    // BEFORE extraction, then a separate extract state — the embedded
    // `research` scanning -> extracting pattern.
    let states = &loaded.manifest.states;

    let routing = states
        .iter()
        .find(|s| s.id == "routing")
        .expect("routing state");
    match &routing.exit_when {
        Some(ExitWhen::LlmJudge { template_file, .. }) => {
            assert_eq!(
                template_file.as_deref(),
                Some("templates/lane-route.md"),
                "routing must use the lane-route judge"
            );
        }
        other => panic!("routing must exit via llm_judge, got: {other:?}"),
    }
    match &routing.next {
        Some(NextTarget::Branches(branches)) => {
            assert_eq!(branches.branches.len(), 1, "exactly one GO branch");
            let rule = &branches.branches[0];
            assert_eq!(rule.when, "_context._judge_result == true");
            assert_eq!(rule.target, "scheduled_sweep");
            assert_eq!(branches.default, "trigger_extract");
        }
        other => panic!("routing must use branches next, got: {other:?}"),
    }

    // Scan state: cheap inventory capability + judge, NO extraction here.
    let scheduled_sweep = states
        .iter()
        .find(|s| s.id == "scheduled_sweep")
        .expect("scheduled_sweep state");
    match &scheduled_sweep.enter[..] {
        [EnterAction::Capability { name, args }] => {
            assert_eq!(name, "creator.inject_prompt");
            let prompt_file = args
                .as_ref()
                .and_then(|a| a.get("prompt_file"))
                .and_then(serde_json::Value::as_str);
            assert_eq!(prompt_file, Some("templates/scheduled-sweep.md"));
        }
        other => panic!("scheduled_sweep must enter via a single capability, got: {other:?}"),
    }
    match &scheduled_sweep.exit_when {
        Some(ExitWhen::LlmJudge {
            template_file,
            judge_capability,
            min_interval,
        }) => {
            assert_eq!(
                template_file.as_deref(),
                Some("templates/scheduled-sweep-exit.md")
            );
            assert_eq!(judge_capability.as_deref(), Some("judge.llm"));
            assert_eq!(min_interval.as_deref(), Some("PT1H"));
        }
        other => panic!("scheduled_sweep must exit via llm_judge, got: {other:?}"),
    }
    assert_eq!(
        scheduled_sweep.next,
        Some(NextTarget::Linear("sweep_extract".to_string())),
        "GO must advance to the separate extract state"
    );

    // Extract state: the inner graph runs extraction ONLY, after the judge.
    let sweep_extract = states
        .iter()
        .find(|s| s.id == "sweep_extract")
        .expect("sweep_extract state");
    match &sweep_extract.enter[..] {
        [EnterAction::InnerGraph { name }] => assert_eq!(name, "sweep_graph"),
        other => panic!("sweep_extract must enter via sweep_graph, got: {other:?}"),
    }
    assert!(
        matches!(sweep_extract.exit_when, Some(ExitWhen::GraphComplete)),
        "sweep_extract must exit via graph_complete"
    );
    assert_eq!(
        sweep_extract.next,
        Some(NextTarget::Linear("done".to_string()))
    );

    // The sweep graph must be extraction-only: no inventory node inside it
    // (the scan/judge lives in the scheduled_sweep state).
    let sweep_graph = loaded
        .inner_graphs
        .get("sweep_graph")
        .expect("sweep_graph inner graph");
    assert!(
        sweep_graph.get_task("extract_worldview").is_some(),
        "sweep_graph must contain extract_worldview"
    );
    assert!(
        sweep_graph.get_task("extract_characters").is_some(),
        "sweep_graph must contain extract_characters"
    );
    assert!(
        sweep_graph.get_task("sweep_inventory").is_none(),
        "the inventory scan must NOT be a node of the extraction graph \
         (it lives in the scheduled_sweep state)"
    );
}

#[test]
fn dotted_preset_input_key_is_unreachable_in_expressions() {
    // The context map is flat (`build_context_json` merges literal keys such
    // as "preset.input.mode"), but the grammar resolves `_context.a.b.c` by
    // walking nested objects — a literal dotted key is unreachable, so the
    // expression evaluates false even when the key is present.
    let flat_context = serde_json::json!({ "preset.input.mode": "scheduled" });
    let dotted = expr::parse("_context.preset.input.mode == 'scheduled'")
        .expect("dotted-key expression must parse");
    assert!(
        !expr::evaluate(&dotted, &flat_context).expect("evaluate must not error"),
        "dotted preset.input.mode key must be unreachable in expressions \
         (this is why the sample routes lanes via _judge_result)"
    );

    // The judge-written discriminator is the supported path: true when the
    // llm_judge exit wrote true, false when absent (fail-closed default).
    let judge_true = serde_json::json!({ "_judge_result": true });
    let judge_absent = serde_json::json!({});
    let judge_expr =
        expr::parse("_context._judge_result == true").expect("judge-result expression must parse");
    assert!(expr::evaluate(&judge_expr, &judge_true).expect("evaluate must not error"));
    assert!(
        !expr::evaluate(&judge_expr, &judge_absent).expect("evaluate must not error"),
        "absent _judge_result must route to the default (trigger lane)"
    );
}
