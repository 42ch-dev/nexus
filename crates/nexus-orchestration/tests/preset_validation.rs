use nexus_orchestration::preset::load_preset_from_str;

fn test_capability_registry() -> nexus_orchestration::capability::CapabilityRegistry {
    nexus_orchestration::capability::CapabilityRegistry::with_builtins()
}

#[test]
fn reject_unknown_next_state() {
    let yaml = r#"
preset:
  id: bad-next
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  initial: a
  terminal: b
states:
  - id: a
    enter: []
    exit_when: { kind: manual }
    next: does-not-exist
  - id: b
    terminal: true
"#;
    let err = load_preset_from_str(yaml, &test_capability_registry()).unwrap_err();
    assert!(
        err.problems()
            .iter()
            .any(|p| p.path.contains("next") && p.error.contains("unknown state")),
        "expected 'unknown state' problem on next: {:?}",
        err.problems()
    );
}

#[test]
fn reject_missing_capability() {
    let yaml = r#"
preset:
  id: bad-cap
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  initial: a
  terminal: b
states:
  - id: a
    enter:
      - kind: capability
        name: nope.does.not.exist
    exit_when: { kind: manual }
    next: b
  - id: b
    terminal: true
"#;
    let err = load_preset_from_str(yaml, &test_capability_registry()).unwrap_err();
    assert!(
        err.problems()
            .iter()
            .any(|p| p.error.contains("unknown capability")),
        "expected 'unknown capability' problem: {:?}",
        err.problems()
    );
}

#[test]
fn reject_inner_graph_cycle() {
    let yaml = r#"
preset:
  id: cycle-test
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  initial: a
  terminal: b
states:
  - id: a
    enter:
      - kind: inner_graph
        name: cyc
    exit_when: { kind: graph_complete }
    next: b
  - id: b
    terminal: true
inner_graphs:
  cyc:
    nodes:
      - id: diverge
        kind: acp_prompt
        depends_on: [cluster]
      - id: cluster
        kind: acp_prompt
        depends_on: [diverge]
    output_binding: diverge.text
"#;
    let err = load_preset_from_str(yaml, &test_capability_registry()).unwrap_err();
    assert!(
        err.problems()
            .iter()
            .any(|p| p.error.contains("cycle")),
        "expected 'cycle' problem in inner_graphs: {:?}",
        err.problems()
    );
}

#[test]
fn reject_conditional_next_not_yet_supported() {
    let yaml = r#"
preset:
  id: cond-test
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  initial: a
  terminal: c
states:
  - id: a
    enter: []
    exit_when: { kind: rule }
    next:
      kind: conditional
      rules:
        - when: "true"
          to: c
      default: b
  - id: b
    enter: []
    exit_when: { kind: manual }
    next: c
  - id: c
    terminal: true
"#;
    let err = load_preset_from_str(yaml, &test_capability_registry()).unwrap_err();
    assert!(
        err.problems()
            .iter()
            .any(|p| p.error.contains("ConditionalNotYetSupported")),
        "expected 'ConditionalNotYetSupported' problem: {:?}",
        err.problems()
    );
}

#[test]
fn reject_unknown_inner_graph_reference() {
    let yaml = r#"
preset:
  id: bad-ig
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  initial: a
  terminal: b
states:
  - id: a
    enter:
      - kind: inner_graph
        name: nonexistent_graph
    exit_when: { kind: graph_complete }
    next: b
  - id: b
    terminal: true
"#;
    let err = load_preset_from_str(yaml, &test_capability_registry()).unwrap_err();
    assert!(
        err.problems()
            .iter()
            .any(|p| p.error.contains("unknown inner_graph")),
        "expected 'unknown inner_graph' problem: {:?}",
        err.problems()
    );
}

#[test]
fn reject_unknown_judge_capability() {
    let yaml = r#"
preset:
  id: bad-judge
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  initial: a
  terminal: b
states:
  - id: a
    enter: []
    exit_when:
      kind: llm_judge
      judge_capability: judge.nonexistent
    next: b
  - id: b
    terminal: true
"#;
    let err = load_preset_from_str(yaml, &test_capability_registry()).unwrap_err();
    assert!(
        err.problems()
            .iter()
            .any(|p| p.error.contains("unknown capability")),
        "expected 'unknown capability' for judge: {:?}",
        err.problems()
    );
}

#[test]
fn reject_terminal_state_with_next() {
    let yaml = r#"
preset:
  id: bad-terminal
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  initial: a
  terminal: b
states:
  - id: a
    enter: []
    exit_when: { kind: manual }
    next: b
  - id: b
    terminal: true
    next: a
"#;
    let err = load_preset_from_str(yaml, &test_capability_registry()).unwrap_err();
    assert!(
        err.problems()
            .iter()
            .any(|p| p.path.contains("terminal") && p.error.contains("next")),
        "expected terminal state 'next' problem: {:?}",
        err.problems()
    );
}

#[test]
fn reject_invalid_initial_state() {
    let yaml = r#"
preset:
  id: bad-init
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  initial: nonexistent
  terminal: b
states:
  - id: a
    enter: []
    exit_when: { kind: manual }
    next: b
  - id: b
    terminal: true
"#;
    let err = load_preset_from_str(yaml, &test_capability_registry()).unwrap_err();
    assert!(
        err.problems()
            .iter()
            .any(|p| p.path.contains("initial") && p.error.contains("unknown state")),
        "expected 'unknown state' on initial: {:?}",
        err.problems()
    );
}

#[test]
fn reject_invalid_terminal_state_ref() {
    let yaml = r#"
preset:
  id: bad-term
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  initial: a
  terminal: nonexistent
states:
  - id: a
    enter: []
    exit_when: { kind: manual }
    next: b
  - id: b
    terminal: true
"#;
    let err = load_preset_from_str(yaml, &test_capability_registry()).unwrap_err();
    assert!(
        err.problems()
            .iter()
            .any(|p| p.path.contains("terminal") && p.error.contains("unknown state")),
        "expected 'unknown state' on terminal: {:?}",
        err.problems()
    );
}

#[test]
fn reject_unknown_depends_on_in_inner_graph() {
    let yaml = r#"
preset:
  id: bad-dep
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  initial: a
  terminal: b
states:
  - id: a
    enter:
      - kind: inner_graph
        name: my_graph
    exit_when: { kind: graph_complete }
    next: b
  - id: b
    terminal: true
inner_graphs:
  my_graph:
    nodes:
      - id: n1
        kind: acp_prompt
        depends_on: [nonexistent_node]
"#;
    let err = load_preset_from_str(yaml, &test_capability_registry()).unwrap_err();
    assert!(
        err.problems()
            .iter()
            .any(|p| p.error.contains("unknown node")),
        "expected 'unknown node' problem: {:?}",
        err.problems()
    );
}

#[test]
fn valid_preset_loads_with_all_sections() {
    let yaml = r#"
preset:
  id: full-valid
  version: 1
  kind: creator
  description: "A valid preset with all sections"
  requires_capabilities:
    - workspace.open
  initial: a
  terminal: c
states:
  - id: a
    enter:
      - kind: capability
        name: workspace.open
    exit_when: { kind: manual }
    next: b
  - id: b
    enter: []
    exit_when: { kind: rule }
    next: c
  - id: c
    terminal: true
inner_graphs:
  my_graph:
    nodes:
      - id: n1
        kind: acp_prompt
      - id: n2
        kind: acp_prompt
        depends_on: [n1]
    output_binding: n2.text
signals:
  - name: user_paused
    on_receive:
      action: pause
"#;
    let loaded = load_preset_from_str(yaml, &test_capability_registry()).unwrap();
    assert_eq!(loaded.id, "full-valid");
    assert_eq!(loaded.version, 1);
    assert_eq!(loaded.outer_graph.id, "full-valid");
    assert!(loaded.inner_graphs.contains_key("my_graph"));
    assert_eq!(loaded.signals.len(), 1);
}

#[test]
fn loaded_preset_has_correct_structure() {
    let yaml = r#"
preset:
  id: struct-test
  version: 2
  kind: creator
  description: test
  requires_capabilities: []
  initial: a
  terminal: b
states:
  - id: a
    enter: []
    exit_when: { kind: manual }
    next: b
  - id: b
    terminal: true
"#;
    let loaded = load_preset_from_str(yaml, &test_capability_registry()).unwrap();
    assert_eq!(loaded.id, "struct-test");
    assert_eq!(loaded.version, 2);
    assert!(loaded.outer_graph.get_task("a").is_some());
    assert!(loaded.outer_graph.get_task("b").is_some());
    assert!(loaded.inner_graphs.is_empty());
    assert!(loaded.signals.is_empty());
    assert!(!loaded.source_hash.is_empty());
}

#[test]
fn source_hash_is_stable() {
    let yaml = r#"
preset:
  id: hash-test
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  initial: a
  terminal: b
states:
  - id: a
    enter: []
    exit_when: { kind: manual }
    next: b
  - id: b
    terminal: true
"#;
    let caps = test_capability_registry();
    let h1 = load_preset_from_str(yaml, &caps).unwrap().source_hash;
    let h2 = load_preset_from_str(yaml, &caps).unwrap().source_hash;
    assert_eq!(h1, h2);
}
