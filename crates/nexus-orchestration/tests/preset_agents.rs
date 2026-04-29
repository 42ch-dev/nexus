//! Tests for multi-agent preset role definitions (WS-E T6).
//!
//! Covers:
//! - Role parsing and `recommended_models` format
//! - Agent reference validation
//! - Backward compatibility (no roles = single-agent mode)
//! - Rejection of invalid configurations

use nexus_contracts::local::orchestration::preset::PresetManifest;
use nexus_orchestration::preset::loader::load_preset_from_str;

fn test_capability_registry() -> nexus_orchestration::capability::CapabilityRegistry {
    nexus_orchestration::capability::CapabilityRegistry::with_builtins()
}

// ── Role parsing tests ──────────────────────────────────────────────────────

#[test]
fn parse_roles_with_recommended_models() {
    let yaml = r#"
preset:
  id: multi-agent-test
  version: 1
  kind: creator
  description: "Multi-agent test preset"
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
roles:
  - id: writer
    description: "Content writer"
    system_prompt_file: prompts/writer.md
    recommended_models:
      - "claude-acp:claude-sonnet-4-20250514"
      - "gemini:gemini-2.5-pro"
  - id: reviewer
    description: "Content reviewer"
    system_prompt_file: prompts/reviewer.md
    recommended_models:
      - "codex-acp:o3"
"#;
    let caps = test_capability_registry();
    let loaded = load_preset_from_str(yaml, &caps).unwrap();
    assert_eq!(loaded.roles.len(), 2);

    let writer = loaded.roles.iter().find(|r| r.id == "writer").unwrap();
    assert_eq!(writer.description, "Content writer");
    assert_eq!(writer.system_prompt_file, "prompts/writer.md");
    assert_eq!(writer.recommended_models.len(), 2);
    assert_eq!(
        writer.recommended_models[0],
        "claude-acp:claude-sonnet-4-20250514"
    );
    assert_eq!(writer.recommended_models[1], "gemini:gemini-2.5-pro");

    let reviewer = loaded.roles.iter().find(|r| r.id == "reviewer").unwrap();
    assert_eq!(reviewer.recommended_models.len(), 1);
    assert_eq!(reviewer.recommended_models[0], "codex-acp:o3");
}

#[test]
#[allow(clippy::too_many_lines)]
fn recommended_models_format_validation() {
    // Valid format: "agent:model"
    let yaml_valid = r#"
preset:
  id: valid-format
  version: 1
  kind: creator
  description: "test"
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
roles:
  - id: agent1
    description: "Agent"
    system_prompt_file: prompts/agent1.md
    recommended_models:
      - "claude-acp:claude-sonnet-4-20250514"
"#;
    let caps = test_capability_registry();
    assert!(load_preset_from_str(yaml_valid, &caps).is_ok());

    // Invalid format: no colon
    let yaml_no_colon = r#"
preset:
  id: no-colon
  version: 1
  kind: creator
  description: "test"
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
roles:
  - id: agent1
    description: "Agent"
    system_prompt_file: prompts/agent1.md
    recommended_models:
      - "claude-acp-without-colon"
"#;
    let err = load_preset_from_str(yaml_no_colon, &caps).unwrap_err();
    assert!(err
        .problems()
        .iter()
        .any(|p| p.error.contains("invalid recommended_models format")
            && p.error.contains("expected 'acp_agent_id:model_name'")));

    // Invalid format: multiple colons
    let yaml_multi_colon = r#"
preset:
  id: multi-colon
  version: 1
  kind: creator
  description: "test"
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
roles:
  - id: agent1
    description: "Agent"
    system_prompt_file: prompts/agent1.md
    recommended_models:
      - "agent:model:extra"
"#;
    let err = load_preset_from_str(yaml_multi_colon, &caps).unwrap_err();
    assert!(err
        .problems()
        .iter()
        .any(|p| p.error.contains("invalid recommended_models format")));

    // Invalid format: empty agent id
    let yaml_empty_agent = r#"
preset:
  id: empty-agent
  version: 1
  kind: creator
  description: "test"
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
roles:
  - id: agent1
    description: "Agent"
    system_prompt_file: prompts/agent1.md
    recommended_models:
      - ":model-name"
"#;
    let err = load_preset_from_str(yaml_empty_agent, &caps).unwrap_err();
    assert!(err
        .problems()
        .iter()
        .any(|p| p.error.contains("invalid recommended_models format")));

    // Invalid format: empty model name
    let yaml_empty_model = r#"
preset:
  id: empty-model
  version: 1
  kind: creator
  description: "test"
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
roles:
  - id: agent1
    description: "Agent"
    system_prompt_file: prompts/agent1.md
    recommended_models:
      - "agent-id:"
"#;
    let err = load_preset_from_str(yaml_empty_model, &caps).unwrap_err();
    assert!(err
        .problems()
        .iter()
        .any(|p| p.error.contains("invalid recommended_models format")));
}

#[test]
fn role_must_have_at_least_one_recommended_model() {
    let yaml = r#"
preset:
  id: empty-recommended
  version: 1
  kind: creator
  description: "test"
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
roles:
  - id: agent1
    description: "Agent"
    system_prompt_file: prompts/agent1.md
"#;
    let caps = test_capability_registry();
    let err = load_preset_from_str(yaml, &caps).unwrap_err();
    assert!(err.problems().iter().any(|p| p
        .error
        .contains("role must have at least one recommended_model")));
}

#[test]
fn duplicate_role_id_rejected() {
    let yaml = r#"
preset:
  id: dup-role
  version: 1
  kind: creator
  description: "test"
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
roles:
  - id: writer
    description: "Writer 1"
    system_prompt_file: prompts/writer1.md
    recommended_models:
      - "claude-acp:model"
  - id: writer
    description: "Writer 2"
    system_prompt_file: prompts/writer2.md
    recommended_models:
      - "gemini:model"
"#;
    let caps = test_capability_registry();
    let err = load_preset_from_str(yaml, &caps).unwrap_err();
    assert!(err
        .problems()
        .iter()
        .any(|p| p.error.contains("duplicate role id: 'writer'")));
}

// ── Agent reference validation tests ────────────────────────────────────────

#[test]
fn agent_reference_to_unknown_role_rejected() {
    let yaml = r#"
preset:
  id: unknown-role-ref
  version: 1
  kind: creator
  description: "test"
  requires_capabilities: []
  initial: a
  terminal: b
states:
  - id: a
    enter:
      - kind: inner_graph
        name: work
    exit_when: { kind: graph_complete }
    next: b
  - id: b
    terminal: true
roles:
  - id: writer
    description: "Writer"
    system_prompt_file: prompts/writer.md
    recommended_models:
      - "claude-acp:model"
inner_graphs:
  work:
    nodes:
      - id: task1
        kind: acp_prompt
        agent: nonexistent_role
        template_file: prompts/task.md
"#;
    let caps = test_capability_registry();
    let err = load_preset_from_str(yaml, &caps).unwrap_err();
    assert!(err.problems().iter().any(|p| p.path.contains("agent")
        && p.error
            .contains("unknown role reference: 'nonexistent_role'")));
}

#[test]
fn agent_field_without_roles_section_rejected() {
    let yaml = r#"
preset:
  id: agent-no-roles
  version: 1
  kind: creator
  description: "test"
  requires_capabilities: []
  initial: a
  terminal: b
states:
  - id: a
    enter:
      - kind: inner_graph
        name: work
    exit_when: { kind: graph_complete }
    next: b
  - id: b
    terminal: true
inner_graphs:
  work:
    nodes:
      - id: task1
        kind: acp_prompt
        agent: writer
        template_file: prompts/task.md
"#;
    let caps = test_capability_registry();
    let err = load_preset_from_str(yaml, &caps).unwrap_err();
    assert!(err
        .problems()
        .iter()
        .any(|p| p.path.contains("agent") && p.error.contains("no roles section defined")));
}

#[test]
fn valid_agent_reference_to_defined_role_passes() {
    let yaml = r#"
preset:
  id: valid-agent-ref
  version: 1
  kind: creator
  description: "test"
  requires_capabilities: []
  initial: a
  terminal: b
states:
  - id: a
    enter:
      - kind: inner_graph
        name: work
    exit_when: { kind: graph_complete }
    next: b
  - id: b
    terminal: true
roles:
  - id: writer
    description: "Writer"
    system_prompt_file: prompts/writer.md
    recommended_models:
      - "claude-acp:model"
inner_graphs:
  work:
    nodes:
      - id: task1
        kind: acp_prompt
        agent: writer
        template_file: prompts/task.md
"#;
    let caps = test_capability_registry();
    assert!(load_preset_from_str(yaml, &caps).is_ok());
}

// ── Backward compatibility tests ────────────────────────────────────────────

#[test]
fn preset_without_roles_loads_successfully() {
    // Single-agent mode: no roles section
    let yaml = r#"
preset:
  id: single-agent
  version: 1
  kind: creator
  description: "Single-agent preset"
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
    let loaded = load_preset_from_str(yaml, &caps).unwrap();
    assert!(loaded.roles.is_empty());
}

#[test]
fn preset_without_roles_inner_graph_without_agent_loads() {
    // Inner graph nodes can omit agent field when no roles defined
    let yaml = r#"
preset:
  id: no-roles-no-agent
  version: 1
  kind: creator
  description: "test"
  requires_capabilities: []
  initial: a
  terminal: b
states:
  - id: a
    enter:
      - kind: inner_graph
        name: work
    exit_when: { kind: graph_complete }
    next: b
  - id: b
    terminal: true
inner_graphs:
  work:
    nodes:
      - id: task1
        kind: acp_prompt
        template_file: prompts/task.md
"#;
    let caps = test_capability_registry();
    assert!(load_preset_from_str(yaml, &caps).is_ok());
}

#[test]
fn node_can_have_optional_agent_field() {
    // Some nodes have agent, others don't
    let yaml = r#"
preset:
  id: mixed-agent
  version: 1
  kind: creator
  description: "test"
  requires_capabilities: []
  initial: a
  terminal: b
states:
  - id: a
    enter:
      - kind: inner_graph
        name: work
    exit_when: { kind: graph_complete }
    next: b
  - id: b
    terminal: true
roles:
  - id: writer
    description: "Writer"
    system_prompt_file: prompts/writer.md
    recommended_models:
      - "claude-acp:model"
inner_graphs:
  work:
    nodes:
      - id: task1
        kind: acp_prompt
        agent: writer
        template_file: prompts/task.md
      - id: task2
        kind: acp_prompt
        template_file: prompts/task2.md
        depends_on: [task1]
"#;
    let caps = test_capability_registry();
    let loaded = load_preset_from_str(yaml, &caps).unwrap();
    let ig = loaded.inner_graphs.get("work").unwrap();
    // Verify nodes exist (agent field validation already passed)
    assert!(ig.get_task("task1").is_some());
    assert!(ig.get_task("task2").is_some());
}

// ── Roundtrip serialization tests ───────────────────────────────────────────

#[test]
fn preset_with_roles_roundtrip_serializes() {
    let yaml = r#"
preset:
  id: roundtrip-roles
  version: 1
  kind: creator
  description: "test"
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
roles:
  - id: writer
    description: "Writer"
    system_prompt_file: prompts/writer.md
    recommended_models:
      - "claude-acp:claude-sonnet-4-20250514"
"#;
    let caps = test_capability_registry();
    let loaded = load_preset_from_str(yaml, &caps).unwrap();

    // Serialize back to YAML and parse again
    let serialized = serde_yaml::to_string(&loaded.manifest).unwrap();
    let parsed: PresetManifest = serde_yaml::from_str(&serialized).unwrap();
    assert_eq!(parsed.roles.len(), 1);
    assert_eq!(parsed.roles[0].id, "writer");
}
