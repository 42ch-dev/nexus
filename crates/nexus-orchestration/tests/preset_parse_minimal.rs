use nexus_orchestration::preset::manifest::PresetManifest;

#[test]
fn parse_minimal_creator_preset() {
    let yaml = r#"
preset:
  id: tiny
  version: 1
  kind: creator
  description: minimal
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
    let p: PresetManifest = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(p.preset.id, "tiny");
    assert_eq!(p.states.len(), 2);
}

#[test]
fn parse_full_preset_with_inner_graphs() {
    let yaml = r#"
preset:
  id: novel-writing
  version: 1
  kind: creator
  description: "A novel writing workflow"
  requires_capabilities:
    - creator.inject_prompt
    - acp.prompt
    - judge.llm
  initial: gathering
  terminal: done
states:
  - id: gathering
    enter:
      - kind: capability
        name: creator.inject_prompt
        args:
          prompt_file: prompts/gathering.md
    exit_when:
      kind: llm_judge
      template_file: prompts/gathering-exit.md
      judge_capability: judge.llm
    next: brainstorming
  - id: brainstorming
    enter:
      - kind: inner_graph
        name: brainstorm_graph
    exit_when:
      kind: graph_complete
    next: outlining
  - id: outlining
    enter:
      - kind: capability
        name: creator.inject_prompt
        args:
          prompt_file: prompts/outlining.md
    exit_when:
      kind: manual
    next: drafting
  - id: drafting
    enter:
      - kind: inner_graph
        name: drafting_graph
    exit_when:
      kind: graph_complete
    next: done
  - id: done
    terminal: true
inner_graphs:
  brainstorm_graph:
    nodes:
      - id: diverge
        kind: acp_prompt
        template_file: prompts/brainstorm-diverge.md
        tool_policy: auto_grant_read_only
      - id: cluster
        kind: acp_prompt
        depends_on: [diverge]
        template_file: prompts/brainstorm-cluster.md
      - id: select
        kind: acp_prompt
        depends_on: [cluster]
        template_file: prompts/brainstorm-select.md
    output_binding: select.text
  drafting_graph:
    nodes:
      - id: draft_intro
        kind: acp_prompt
        template_file: prompts/draft-intro.md
      - id: draft_body
        kind: acp_prompt
        depends_on: [draft_intro]
        template_file: prompts/draft-body.md
    output_binding: draft_body.text
"#;
    let p: PresetManifest = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(p.preset.id, "novel-writing");
    assert_eq!(p.states.len(), 5);
    assert!(p.inner_graphs.is_some());
    let ig = p.inner_graphs.as_ref().unwrap();
    assert_eq!(ig.len(), 2);
    assert_eq!(ig["brainstorm_graph"].nodes.len(), 3);
    assert_eq!(ig["drafting_graph"].nodes.len(), 2);
}

#[test]
fn unknown_exit_when_kind_fails() {
    let yaml = r#"
preset:
  id: bad
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
      kind: quantum_teleport
    next: b
  - id: b
    terminal: true
"#;
    let err = serde_yaml::from_str::<PresetManifest>(yaml);
    assert!(err.is_err(), "expected error for unknown exit_when.kind");
    let msg = format!("{:#}", err.unwrap_err());
    assert!(
        msg.contains("quantum_teleport"),
        "error should mention the unknown variant: {msg}"
    );
}

#[test]
fn unknown_enter_kind_fails() {
    let yaml = r#"
preset:
  id: bad
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  initial: a
  terminal: b
states:
  - id: a
    enter:
      - kind: launch_rocket
        name: falcon-9
    exit_when: { kind: manual }
    next: b
  - id: b
    terminal: true
"#;
    let err = serde_yaml::from_str::<PresetManifest>(yaml);
    assert!(err.is_err(), "expected error for unknown enter.kind");
    let msg = format!("{:#}", err.unwrap_err());
    assert!(
        msg.contains("launch_rocket"),
        "error should mention the unknown variant: {msg}"
    );
}

#[test]
fn missing_required_field_fails() {
    let yaml = r#"
preset:
  id: missing-fields
  version: 1
  kind: creator
  description: test
states: []
"#;
    let err = serde_yaml::from_str::<PresetManifest>(yaml);
    assert!(err.is_err(), "expected error for missing required fields");
}

#[test]
fn system_preset_parses() {
    let yaml = r#"
preset:
  id: _system.maintenance
  version: 1
  kind: system
  description: internal
  requires_capabilities: []
  initial: sync
  terminal: end
states:
  - id: sync
    enter: []
    exit_when: { kind: rule }
    next: end
  - id: end
    terminal: true
"#;
    let p: PresetManifest = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(p.preset.kind, nexus_orchestration::preset::manifest::PresetKind::System);
}
