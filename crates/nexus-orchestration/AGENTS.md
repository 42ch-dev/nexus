# nexus-orchestration AGENTS.md

Guidance for agents (and developers) working in the `nexus-orchestration` crate.

## Embedded Presets

Presets define orchestration strategies — state machines with enter/exit transitions, prompt templates, and capability bindings. They are compiled into the binary at build time via `include_dir!`.

### Directory Structure

```
embedded-presets/
  <preset-id>/                # One directory per preset; <preset-id> = /^[a-z][a-z0-9._-]*$/
    preset.yaml               # Required: manifest file
    prompts/                  # Required if preset references prompt files
      <name>.md               # Prompt templates referenced by template_file fields
```

The `include_dir!` macro scans `embedded-presets/` at compile time. Any subdirectory containing a `preset.yaml` is automatically discovered as an available preset.

### Adding a New Embedded Preset

1. Create `embedded-presets/<preset-id>/preset.yaml` following the manifest format below.
2. Add prompt templates under `embedded-presets/<preset-id>/prompts/` if the preset references them.
3. Build the crate — the new preset is automatically available via `list_embedded_presets()` and loadable via `load_embedded_preset("<preset-id>", &caps)`.

No Rust code changes needed — the `include_dir!` macro picks up new directories automatically.

### `preset.yaml` Manifest Format

Types are defined in `nexus-contracts::local::orchestration::preset` (hand-written, not in `schemas/`).

```yaml
preset:
  id: <string>                          # Must match directory name; /^[a-z][a-z0-9._-]*$/
  version: <int>                        # >= 1; bump on breaking changes
  kind: creator | system                # "creator" = user-facing, "system" = internal
  description: <string>                 # <= 240 chars
  requires_capabilities:                # Loader rejects if any are missing from CapabilityRegistry
    - <capability-name>
  initial: <state-id>                   # Must match a states[].id
  terminal: <state-id>                  # Must match a states[].id
  author: <string>                      # Optional
  homepage: <url>                       # Optional
  license: <string>                     # Optional
  initial_action:                       # Optional (WS7 §7): controls core_context v0 seeding
    kind: seed_direct | seed_expansion

states:
  - id: <string>                        # Unique within this preset
    description: <string>               # Optional
    enter:                              # Actions on state entry
      - kind: capability                # Invoke a registered capability
        name: <capability-name>
        args:
          prompt_file: prompts/<name>.md
          vars:
            <key>: "{{preset.input.<field>}}"
      - kind: inner_graph               # Launch a named sub-graph
        name: <inner-graph-name>
    exit_when:                          # Transition condition (absent for terminal)
      kind: llm_judge | rule | graph_complete | manual | timer
      template_file: prompts/<name>.md  # For llm_judge
      judge_capability: <name>          # For llm_judge
      min_interval: <ISO-8601-duration> # For llm_judge (e.g. "PT6H")
    next: <state-id>                    # Linear transition (conditional rejected in V1.4)
    terminal: <bool>                    # true = no outgoing transitions
    context_update:                     # Optional (WS7 §7): fires on state exit
      op:
        kind: append | struct_merge
      template_file: prompts/<name>.md

inner_graphs:                           # Optional: sub-graphs referenced by enter.kind = inner_graph
  <graph-name>:
    nodes:
      - id: <string>
        kind: acp_prompt
        template_file: prompts/<name>.md
        tool_policy: auto_grant_read_only | ...
        depends_on: [<node-id>, ...]    # Must reference valid node IDs; graph must be cycle-free
    output_binding: <node-id>.<field>   # Which node output to expose

signals:                                # Optional: external event bindings
  - name: <string>
    on_receive:
      action: pause | resume | cancel
```

### Validation Rules (loader.rs)

The loader enforces these constraints at load time:

- All `requires_capabilities` must exist in the `CapabilityRegistry`
- `initial` and `terminal` must reference valid `states[].id` values
- `next` must reference a valid state ID (conditional/`Conditional` form is rejected in V1.4)
- Terminal states cannot have `next`
- Inner graphs must be cycle-free
- Inner graph `depends_on` must reference valid node IDs within the same graph
- Context update hooks only allow `append` and `struct_merge` ops (`replace`/`struct_remove` rejected)
- Source hash is computed via blake3 of the raw YAML content (identity across restarts)

### Existing Presets

| Preset ID | Location | Description |
|-----------|----------|-------------|
| `novel-writing` | `embedded-presets/novel-writing/` | Multi-phase novel workflow: gather → brainstorm → outline → draft. Uses inner graphs for brainstorm and drafting phases. |
| `_system.maintenance` | `src/system_preset.rs` | Hardcoded Rust graph (not a YAML preset). Linear chain: `sync_pull → outbox_flush → registry_refresh → end`. Started at daemon boot. |

### Key Source Files

| File | Role |
|------|------|
| `src/preset/mod.rs` | `include_dir!` embedding, `load_embedded_preset()`, `list_embedded_presets()` |
| `src/preset/loader.rs` | YAML parsing, validation, graph building (`load_preset_from_str()`) |
| `src/preset/manifest.rs` | Re-exports types from `nexus-contracts::local::orchestration::preset` |
| `src/system_preset.rs` | Hardcoded `_system.maintenance` graph builder |
| `nexus-contracts/src/local/orchestration/preset.rs` | Preset manifest type definitions (PresetManifest, StateDefinition, etc.) |

### User-Installed Presets (Not Yet Implemented)

The design spec defines a filesystem search order for user-installed presets:

1. `$XDG_CONFIG_HOME/nexus42/presets/<id>/`
2. `$HOME/.nexus42/presets/<id>/`
3. Embedded presets (current: `include_dir!`)

Steps 1 and 2 are **not yet implemented** — `load_preset()` in `loader.rs` is a stub (WS3 T6). V1.4 ships only embedded presets.

### Design Reference

Full design: `.agents/plans/knowledge/orchestration-engine-v1.md` (sections 7–9 cover presets, loader, and validation).
