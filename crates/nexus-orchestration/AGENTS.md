# nexus-orchestration AGENTS.md

## Embedded Presets

Presets are orchestration strategies — state machines with transitions, prompt templates, and capability bindings — compiled into the binary at build time via `include_dir!`. Any subdirectory under `embedded-presets/` containing a `preset.yaml` is automatically discovered. No Rust code changes needed to add a preset.

## Validation Rules (loader.rs)

The loader enforces these constraints at load time:

- All `requires_capabilities` must exist in the `CapabilityRegistry`
- `initial` and `terminal` must reference valid `states[].id` values
- `next` must reference a valid state ID (conditional/`Conditional` form is rejected in V1.4)
- Terminal states cannot have `next`
- Inner graphs must be cycle-free
- Inner graph `depends_on` must reference valid node IDs within the same graph
- Context update hooks only allow `append` and `struct_merge` ops (`replace`/`struct_remove` rejected)
- Source hash is computed via blake3 of raw YAML content (identity across restarts)

## User-Installed Presets (Not Yet Implemented)

The design spec defines filesystem search (`$XDG_CONFIG_HOME/nexus42/presets/<id>/`, `$HOME/.nexus42/presets/<id>/`) ahead of embedded presets. Steps 1 and 2 are **not yet implemented** — `load_preset()` in `loader.rs` is a stub. V1.4 ships only embedded presets.

## Design Reference

Full design: `.agents/knowledge/orchestration-engine-v1.md` (sections 7–9 cover presets, loader, validation).
