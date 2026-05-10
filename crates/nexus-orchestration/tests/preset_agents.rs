//! Tests for multi-agent preset role definitions (WS-E T6).
//!
//! Covers:
//! - Role parsing and `recommended_skills` format
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
fn parse_roles_with_recommended_skills() {
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
    recommended_skills:
      - "novel-writing-assistant"
  - id: reviewer
    description: "Content reviewer"
    system_prompt_file: prompts/reviewer.md
    recommended_skills:
      - "novel-writing-assistant"
"#;
    let caps = test_capability_registry();
    let loaded = load_preset_from_str(yaml, &caps).unwrap();
    assert_eq!(loaded.roles.len(), 2);

    let writer = loaded.roles.iter().find(|r| r.id == "writer").unwrap();
    assert_eq!(writer.description, "Content writer");
    assert_eq!(writer.system_prompt_file, "prompts/writer.md");
    assert_eq!(writer.recommended_skills.len(), 1);
    assert_eq!(writer.recommended_skills[0], "novel-writing-assistant");

    let reviewer = loaded.roles.iter().find(|r| r.id == "reviewer").unwrap();
    assert_eq!(reviewer.recommended_skills.len(), 1);
    assert_eq!(reviewer.recommended_skills[0], "novel-writing-assistant");
}

#[test]
#[allow(clippy::too_many_lines)]
fn recommended_skills_format_validation() {
    // Valid format: skill slug
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
    recommended_skills:
      - "novel-writing-assistant"
"#;
    let caps = test_capability_registry();
    assert!(load_preset_from_str(yaml_valid, &caps).is_ok());

    // Valid: simple single-char slug
    let yaml_single_char = r#"
preset:
  id: single-char
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
    recommended_skills:
      - "a"
"#;
    assert!(load_preset_from_str(yaml_single_char, &caps).is_ok());

    // Valid: hyphenated multi-word
    let yaml_hyphenated = r#"
preset:
  id: hyphenated
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
    recommended_skills:
      - "my-skill-v2"
"#;
    assert!(load_preset_from_str(yaml_hyphenated, &caps).is_ok());

    // Invalid: uppercase
    let yaml_uppercase = r#"
preset:
  id: uppercase
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
    recommended_skills:
      - "UPPERCASE"
"#;
    let err = load_preset_from_str(yaml_uppercase, &caps).unwrap_err();
    assert!(err
        .problems()
        .iter()
        .any(|p| p.error.contains("invalid recommended_skills format")));

    // Invalid: starts with digit
    let yaml_starts_digit = r#"
preset:
  id: starts-digit
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
    recommended_skills:
      - "123invalid"
"#;
    let err = load_preset_from_str(yaml_starts_digit, &caps).unwrap_err();
    assert!(err
        .problems()
        .iter()
        .any(|p| p.error.contains("invalid recommended_skills format")));

    // Invalid: contains spaces
    let yaml_spaces = r#"
preset:
  id: has-spaces
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
    recommended_skills:
      - "has spaces"
"#;
    let err = load_preset_from_str(yaml_spaces, &caps).unwrap_err();
    assert!(err
        .problems()
        .iter()
        .any(|p| p.error.contains("invalid recommended_skills format")));

    // Invalid: starts with hyphen
    let yaml_starts_hyphen = r#"
preset:
  id: starts-hyphen
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
    recommended_skills:
      - "-starts-hyphen"
"#;
    let err = load_preset_from_str(yaml_starts_hyphen, &caps).unwrap_err();
    assert!(err
        .problems()
        .iter()
        .any(|p| p.error.contains("invalid recommended_skills format")));

    // Invalid: ends with hyphen
    let yaml_ends_hyphen = r#"
preset:
  id: ends-hyphen
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
    recommended_skills:
      - "ends-hyphen-"
"#;
    let err = load_preset_from_str(yaml_ends_hyphen, &caps).unwrap_err();
    assert!(err
        .problems()
        .iter()
        .any(|p| p.error.contains("invalid recommended_skills format")));
}

#[test]
fn role_must_have_at_least_one_recommended_skill() {
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
        .contains("role must have at least one recommended_skill")));
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
    recommended_skills:
      - "writer-skill"
  - id: writer
    description: "Writer 2"
    system_prompt_file: prompts/writer2.md
    recommended_skills:
      - "reviewer-skill"
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
    recommended_skills:
      - "writer-skill"
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
    recommended_skills:
      - "writer-skill"
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
    recommended_skills:
      - "writer-skill"
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
    recommended_skills:
      - "novel-writing-assistant"
"#;
    let caps = test_capability_registry();
    let loaded = load_preset_from_str(yaml, &caps).unwrap();

    // Serialize back to YAML and parse again
    let serialized = serde_yaml::to_string(&loaded.manifest).unwrap();
    let parsed: PresetManifest = serde_yaml::from_str(&serialized).unwrap();
    assert_eq!(parsed.roles.len(), 1);
    assert_eq!(parsed.roles[0].id, "writer");
}
