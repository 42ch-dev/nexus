//! Engine-dependent registry contract cases moved out of the capability
//! crate's unit tests (v249 C2): these drive the orchestration
//! `GraphFlowEngine`, so they live beside the engine — no
//! capability→orchestration dependency is created.

use nexus_capability::{Capability, CapabilityError, CapabilityRegistry, CapabilityRegistryHolder};
use std::sync::Arc;

//
// The premise under test: a run can be driven by a registry the caller
// owns (a fresh holder per run), so a simulation can observe every
// dispatch without swapping the shared production holder. The registry
// holds ONLY the probe row — a path that resolves a real builtin instead
// would fail with "not found", so a green run is also proof that nothing
// real executed.
struct RecordingCap {
    hits: Arc<std::sync::atomic::AtomicUsize>,
}

#[async_trait::async_trait]
impl Capability for RecordingCap {
    fn name(&self) -> &'static str {
        "record.probe"
    }

    fn input_schema(&self) -> &'static str {
        r#"{"type":"object"}"#
    }

    fn output_schema(&self) -> &'static str {
        r#"{"type":"object"}"#
    }

    async fn run(&self, _input: serde_json::Value) -> Result<serde_json::Value, CapabilityError> {
        self.hits.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(serde_json::json!({ "recorded": true }))
    }
}

fn recording_registry(hits: &Arc<std::sync::atomic::AtomicUsize>) -> Arc<CapabilityRegistry> {
    let reg =
        CapabilityRegistry::with_capabilities(vec![Box::new(RecordingCap { hits: hits.clone() })]);
    Arc::new(reg)
}

const T1_PROBE_PRESET: &str = r#"
preset:
  id: preflight-t1-probe
  version: 1
  kind: creator
  description: "preflight T1: one reachable capability dispatch"
  requires_capabilities:
    - record.probe
  initial: probe
  terminal: done
states:
  - id: probe
    enter:
      - kind: capability
        name: record.probe
    exit_when: { kind: rule }
    next: done
  - id: done
    terminal: true
"#;

const T1_QUIET_PRESET: &str = r#"
preset:
  id: preflight-t1-quiet
  version: 1
  kind: creator
  description: "preflight T1: a graph that reaches no capability"
  requires_capabilities: []
  initial: quiet
  terminal: done
states:
  - id: quiet
    exit_when: { kind: rule }
    next: done
  - id: done
    terminal: true
"#;

async fn run_to_completion(
    engine: &nexus_orchestration::GraphFlowEngine,
    loaded: &nexus_preset::LoadedPreset,
) {
    use nexus_orchestration::engine::{OrchestrationEngine, StepOutcome};

    let sid = engine
        .start_session_with_preset(loaded)
        .await
        .expect("start preset run");
    for _ in 0..16 {
        match engine.run_step(&sid).await.expect("run step") {
            StepOutcome::Completed { .. } => return,
            StepOutcome::WaitingForInput { .. } | StepOutcome::Paused { .. } => {
                engine
                    .signal(&sid, nexus_orchestration::engine::EngineSignal::Resume)
                    .await
                    .expect("resume");
            }
        }
    }
    panic!("run did not complete in 16 steps");
}

#[tokio::test]
async fn preflight_t1_recording_registries_are_per_run_and_isolate_dispatch_between_engines() {
    use nexus_orchestration::GraphFlowEngine;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // resolve is the probe (the real builtins are absent from this
    // registry, so any bypass fails the run instead of passing silently).
    let first_hits = Arc::new(AtomicUsize::new(0));
    let first_caps = recording_registry(&first_hits);
    assert!(
        first_caps.get("judge.rule").is_none(),
        "no builtins in the recording registry"
    );
    let probe_loaded = nexus_preset::load_preset_from_str(T1_PROBE_PRESET, first_caps.as_ref())
        .expect("probe preset loads against the recording registry");
    let probe_engine = GraphFlowEngine::new_with_storage(
        Arc::new(graph_flow::InMemorySessionStorage::new()),
        CapabilityRegistryHolder::with_registry(first_caps.clone()),
    );
    run_to_completion(&probe_engine, &probe_loaded).await;
    assert_eq!(
        first_hits.load(Ordering::SeqCst),
        1,
        "one reachable capability, one dispatch"
    );

    // engine resolved the first engine's registry instead, the dispatch
    // below would land in `first_hits` and leave `second_hits` at zero.
    let second_hits = Arc::new(AtomicUsize::new(0));
    let second_caps = recording_registry(&second_hits);
    assert!(
        second_caps.get("judge.rule").is_none(),
        "no builtins in the second recording registry"
    );
    let second_loaded = nexus_preset::load_preset_from_str(T1_PROBE_PRESET, second_caps.as_ref())
        .expect("preset loads against the second registry");
    let second_engine = GraphFlowEngine::new_with_storage(
        Arc::new(graph_flow::InMemorySessionStorage::new()),
        CapabilityRegistryHolder::with_registry(second_caps.clone()),
    );
    run_to_completion(&second_engine, &second_loaded).await;
    assert_eq!(
        second_hits.load(Ordering::SeqCst),
        1,
        "the second engine dispatched through its own registry"
    );
    assert_eq!(
        first_hits.load(Ordering::SeqCst),
        1,
        "the second run did not touch the first engine's counter"
    );

    let quiet_hits = Arc::new(AtomicUsize::new(0));
    let quiet_caps = recording_registry(&quiet_hits);
    let quiet_loaded = nexus_preset::load_preset_from_str(T1_QUIET_PRESET, quiet_caps.as_ref())
        .expect("quiet preset loads");
    let quiet_engine = GraphFlowEngine::new_with_storage(
        Arc::new(graph_flow::InMemorySessionStorage::new()),
        CapabilityRegistryHolder::with_registry(quiet_caps.clone()),
    );
    run_to_completion(&quiet_engine, &quiet_loaded).await;
    assert_eq!(
        quiet_hits.load(Ordering::SeqCst),
        0,
        "no dispatch without a capability"
    );

    assert!(!Arc::ptr_eq(&first_caps, &second_caps));
    assert!(!Arc::ptr_eq(&first_caps, &quiet_caps));
    assert!(!Arc::ptr_eq(&second_caps, &quiet_caps));
    assert!(
        CapabilityRegistryHolder::new().get().is_none(),
        "a fresh holder is empty, never the production one"
    );
}
