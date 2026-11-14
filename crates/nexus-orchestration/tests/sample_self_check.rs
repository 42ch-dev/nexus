//! Durable sample self-check (L1 evidence — P3 QC fix wave W2).
//!
//! Pins these facts:
//!
//! 1. The bundled `strategy-samples/game-narrative/` strategy stays valid
//!    against the real validator core (`load_preset` + `validate_preset_semantic`
//!    + `validate_assets_in_bundle` + `validate_path_safety`), with zero error
//!      AND zero warning diagnostics.
//! 2. The expression-routing constraint that forced the sample's `llm_judge`
//!    lane selector: a dotted `preset.input.*` key is unreachable in
//!    `_context` expressions (the context is a flat map and the grammar has no
//!    bracket syntax), so lane selection must come from `_context._judge_result`,
//!    written by an `llm_judge` exit before branch evaluation.
//! 3. Every committed `preset.yaml` this repository ships as evidence
//!    (`strategy-samples/game-narrative`, `strategy-samples/react-trpg-turn`,
//!    `examples/first-workflow`) survives `import_preset_bundle` ->
//!    `export_preset` with **no data difference at all**, so the IR can become
//!    the authoring source without rewriting a single manifest field. The
//!    comparison is canonical `serde_yaml::Value` equality with no normalisation
//!    applied, so a divergence fails with its YAML path. [`BYTE_NORMALISATIONS`]
//!    documents what such a comparison cannot see; [`VALUE_REWRITES`] documents
//!    the spellings that would make it fail.
//! 4. Those rewrites still happen exactly where the lists say — in particular the
//!    one shape that changes the value (a judged state's zero-rule branch form,
//!    I-000476), so a reader never has to trust a comment.
//!
//! The bundles live at the repo root; this crate resolves them via
//! `CARGO_MANIFEST_DIR` so the test passes regardless of the invoking working
//! directory.

use nexus_orchestration::CapabilityRegistry;
use nexus_preset::manifest::{EnterAction, ExitWhen, NextTarget, PresetManifest};
use nexus_preset::{
    export_preset, expr, import_preset_bundle, load_preset, validate_assets_in_bundle,
    validate_path_safety, validate_preset_semantic,
};
use serde_yaml::Value;

/// Repo-root-relative path to the bundled game-narrative sample.
fn sample_bundle_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../strategy-samples/game-narrative")
}

fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
// Long integration test; splitting would obscure the end-to-end scenario
#[allow(clippy::too_many_lines)]
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
    let sweep_graph = &loaded.manifest.inner_graphs.as_ref().unwrap()["sweep_graph"];
    assert!(
        sweep_graph
            .nodes
            .iter()
            .any(|node| node.id == "extract_worldview"),
        "sweep_graph must contain extract_worldview"
    );
    assert!(
        sweep_graph
            .nodes
            .iter()
            .any(|node| node.id == "extract_characters"),
        "sweep_graph must contain extract_characters"
    );
    assert!(
        !sweep_graph
            .nodes
            .iter()
            .any(|node| node.id == "sweep_inventory"),
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

// ---------------------------------------------------------------------------
// `preset.yaml` <-> `WorkflowDocument` round trip (IR-primary authoring gate)
// ---------------------------------------------------------------------------

/// One difference between a committed `preset.yaml` and its re-export. Both lists are the
/// exporter's contract and are published for sample authors in `strategy-samples/README.md` §6.
#[derive(Debug)]
struct Normalisation {
    /// The shape a hand-written `preset.yaml` may use.
    input: &'static str,
    /// The shape `export_preset` writes for that input.
    output: &'static str,
    /// `true` when the two spellings mean different manifests: that is a blocking finding, never
    /// a normalisation. Every entry in both lists is `false`.
    semantic: bool,
    /// Where the rewrite comes from, so the entry can be re-derived from the code.
    source: &'static str,
}

/// Differences that live only in the bytes: the round-trip compares `serde_yaml::Value`s, so they
/// cannot appear in its verdict.
const BYTE_NORMALISATIONS: &[Normalisation] = &[
    Normalisation {
        input: "`#` comment lines anywhere in the file",
        output: "dropped",
        semantic: false,
        source: "`serde_yaml` serialises values; comments are not data",
    },
    Normalisation {
        input: "`description: \"text\"` (double-quoted or block scalar)",
        output: "`description: 'text'` (single-quoted)",
        semantic: false,
        source: "`serde_yaml` picks the scalar style; the string value is unchanged",
    },
    Normalisation {
        input: "flow style (`{kind: llm_judge}`) and indented sequences (`  - x`)",
        output: "block style, sequence dashes at the parent indent",
        semantic: false,
        source: "`serde_yaml` emitter defaults",
    },
    Normalisation {
        input: "`preset:` keys and `inner_graphs:` keys in author order",
        output: "`preset:` in schema order, `inner_graphs:` alphabetical",
        semantic: false,
        source: "`PresetHeader` field order; `BTreeMap` for the graph map",
    },
];

/// Differences the export makes to the value: a manifest using the `input` spelling does not
/// round-trip, so the committed YAML must use the `output` spelling. None of the three bundles
/// below uses one — the test would report its YAML path.
const VALUE_REWRITES: &[Normalisation] = &[
    Normalisation {
        input: "an empty value written out (`run_intents: []`, `enter: []`, `terminal: false`)",
        output: "key omitted",
        semantic: false,
        source: "`skip_serializing_if` on those manifest fields",
    },
    Normalisation {
        input: "a defaulted field left out (`preset.requires_capabilities`, `cli_args[].required`, a `previous_preset` gate's `scope`, `context_update.op.body`)",
        output: "the default written out (`requires_capabilities: []`, `required: false`, `scope: work`, `body: ''`)",
        semantic: false,
        source: "those fields have `#[serde(default)]` and no `skip_serializing_if`",
    },
    Normalisation {
        input: "legacy `next: {kind: conditional, rules: [...]}`",
        output: "`next: {branches: [...], default: ...}`",
        semantic: false,
        source: "documented `shortcut:` in `nexus-preset/src/workflow_ir.rs`",
    },
    Normalisation {
        input: "judged state (`exit_when: llm_judge`) with `next: {branches: [], default: B}`",
        output: "`next: B`",
        semantic: false,
        source: "I-000476: the IR cannot distinguish a zero-rule branch from a bare `next` on a \
                 judged node, so the export collapses it",
    },
];

/// Join one YAML path segment onto `path`.
fn field_path(path: &str, segment: &str) -> String {
    if path.is_empty() {
        segment.to_string()
    } else {
        format!("{path}.{segment}")
    }
}

/// First YAML path at which the two values disagree (e.g. `states[2].next.default`).
fn first_divergence(path: &str, committed: &Value, exported: &Value) -> Option<String> {
    match (committed, exported) {
        (Value::Mapping(left), Value::Mapping(right)) => {
            for (key, value) in left {
                let path = field_path(path, key.as_str().unwrap_or_default());
                match right.get(key) {
                    Some(other) => {
                        if let Some(found) = first_divergence(&path, value, other) {
                            return Some(found);
                        }
                    }
                    None => return Some(format!("{path} (absent from the export)")),
                }
            }
            right
                .keys()
                .find(|key| !left.contains_key(*key))
                .map(|key| {
                    format!(
                        "{} (only in the export)",
                        field_path(path, key.as_str().unwrap_or_default())
                    )
                })
        }
        (Value::Sequence(left), Value::Sequence(right)) => {
            for (index, (a, b)) in left.iter().zip(right).enumerate() {
                if let Some(found) = first_divergence(&format!("{path}[{index}]"), a, b) {
                    return Some(found);
                }
            }
            (left.len() != right.len()).then(|| {
                format!(
                    "{path} has {} entries in one spelling, {} in the other",
                    left.len(),
                    right.len()
                )
            })
        }
        _ => (committed != exported).then(|| path.to_string()),
    }
}

/// Round-trip one committed bundle: `preset.yaml` -> `WorkflowDocument` -> `preset.yaml`.
fn assert_bundle_round_trips(bundle: &str) {
    let root = repo_root().join(bundle);
    let yaml = std::fs::read_to_string(root.join("preset.yaml"))
        .unwrap_or_else(|error| panic!("read {bundle}/preset.yaml: {error}"));
    let committed: Value = serde_yaml::from_str(&yaml)
        .unwrap_or_else(|error| panic!("parse {bundle}/preset.yaml as YAML: {error}"));
    let manifest: PresetManifest = serde_yaml::from_str(&yaml)
        .unwrap_or_else(|error| panic!("parse {bundle}/preset.yaml as a manifest: {error}"));

    let imported = import_preset_bundle(&manifest, &root)
        .unwrap_or_else(|error| panic!("import {bundle} into the IR: {error}"));
    let exported = export_preset(&imported.document)
        .unwrap_or_else(|error| panic!("export {bundle} back to a manifest: {error}"));
    let exported: Value = serde_yaml::to_value(&exported)
        .unwrap_or_else(|error| panic!("serialize the {bundle} export: {error}"));

    if let Some(path) = first_divergence("", &committed, &exported) {
        panic!(
            "`{bundle}` does not round-trip: the export differs from the committed YAML at \
             `{path}`. This comparison tolerates no difference: either the bundle uses one of \
             the {} value-level rewrites documented in strategy-samples/README.md §6 (write the \
             canonical spelling instead), or the exporter is wrong for this manifest",
            VALUE_REWRITES.len()
        );
    }
}

#[test]
fn game_narrative_committed_yaml_round_trips() {
    assert_bundle_round_trips("strategy-samples/game-narrative");
}

#[test]
fn react_trpg_turn_committed_yaml_round_trips() {
    assert_bundle_round_trips("strategy-samples/react-trpg-turn");
}

#[test]
fn first_workflow_committed_yaml_round_trips() {
    assert_bundle_round_trips("examples/first-workflow");
}

/// A manifest whose `a` state is spelled by `a_state` and whose graph is otherwise fixed.
fn probe_manifest(a_state: &str) -> String {
    format!(
        "preset:\n  id: probe\n  version: 1\n  kind: creator\n  description: probe\n  \
         requires_capabilities: [acp.prompt]\n  initial: a\n  terminal: done\nstates:\n  \
         - id: a\n{a_state}  - id: b\n    next: done\n  - id: done\n    terminal: true\n"
    )
}

/// Export a probe manifest through the IR. Probes reference no assets, so the bundle root is
/// never read; the repo root only has to exist.
fn probe_export(a_state: &str) -> Value {
    let yaml = probe_manifest(a_state);
    let manifest: PresetManifest = serde_yaml::from_str(&yaml).expect("probe parses");
    let imported = import_preset_bundle(&manifest, &repo_root()).expect("probe imports");
    serde_yaml::to_value(export_preset(&imported.document).expect("probe exports"))
        .expect("probe export serialises")
}

/// Both lists are data, so their own invariant is checked: every entry is cosmetic, and every
/// entry names two different shapes with a source. A `semantic: true` entry means this plan must
/// stop.
#[test]
fn every_documented_normalisation_is_cosmetic() {
    for normalisation in BYTE_NORMALISATIONS.iter().chain(VALUE_REWRITES) {
        assert!(
            !normalisation.semantic,
            "`{}` -> `{}` changes meaning; that is the plan's blocking finding, not a normalisation",
            normalisation.input, normalisation.output
        );
        assert_ne!(normalisation.input, normalisation.output);
        assert!(!normalisation.source.is_empty(), "{normalisation:?}");
    }
}

/// The shape rewrites on the list still happen exactly where the list says — including the one
/// that does rewrite the bytes (I-000476) and the spellings the three bundles use.
#[test]
fn documented_shape_rewrites_and_stable_shapes_hold() {
    // Both samples' spelling: a judged state's bare `next` survives unchanged.
    let bare = probe_export("    exit_when: {kind: llm_judge}\n    next: b\n");
    assert_eq!(bare["states"][0]["next"], Value::String("b".to_string()));

    // I-000476: the judged zero-rule branch form is rewritten to that same bare `next`.
    let collapsed =
        probe_export("    exit_when: {kind: llm_judge}\n    next: {branches: [], default: b}\n");
    assert_eq!(
        collapsed["states"][0]["next"],
        Value::String("b".to_string()),
        "the exporter is expected to collapse this form until I-000476 is fixed; if it now \
         round-trips, drop this case and the VALUE_REWRITES entry"
    );

    // `examples/first-workflow` relies on this: a non-judged zero-rule branch keeps its shape.
    let kept = probe_export("    next: {branches: [], default: b}\n");
    assert_eq!(
        kept["states"][0]["next"]["branches"],
        Value::Sequence(Vec::new())
    );
    assert_eq!(
        kept["states"][0]["next"]["default"],
        Value::String("b".to_string())
    );

    // An explicit default is dropped from the export (`enter: []`, `terminal: false`).
    let explicit = probe_export("    enter: []\n    terminal: false\n    next: b\n");
    assert_eq!(explicit["states"][0].get("enter"), None);
    assert_eq!(explicit["states"][0].get("terminal"), None);

    // The legacy `kind: conditional` spelling is rewritten to the `branches` form.
    let legacy = probe_export("    next: {kind: conditional, rules: [], default: b}\n");
    assert_eq!(
        legacy["states"][0]["next"]["branches"],
        Value::Sequence(Vec::new())
    );
    assert_eq!(
        legacy["states"][0]["next"]["default"],
        Value::String("b".to_string())
    );
    assert_eq!(legacy["states"][0]["next"].get("kind"), None);
}
