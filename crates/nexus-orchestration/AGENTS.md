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

## User-Installed Presets

User-installed presets are fully supported. The composable search order (highest priority first):

1. **User presets** — `~/.nexus42/presets/<id>/` — overrides embedded presets with same ID
2. **System presets** — `~/.nexus42/presets/_system/<id>/` — qualified as `_system.<id>`
3. **Embedded presets** — compiled into the binary at build time

### Preset Resolution

`resolve_preset()` in `mod.rs` implements the composable search order:
- `scan_user_presets()` discovers and loads user presets from `~/.nexus42/presets/`
- `find_user_preset()` provides O(1) lookup via a `HashMap` index built at scan time
- Cache invalidation is based on file modification time (mtime) — `is_scan_cache_fresh()`

### YAML Hardening

`load_preset_from_str_with_limits()` enforces:
- Maximum file size (default 1 MiB, `DEFAULT_MAX_YAML_SIZE`)
- Maximum nesting depth (default 10, `DEFAULT_MAX_YAML_DEPTH`)
- Violations produce `PresetLoadError::YamlSizeExceeded` / `PresetLoadError::YamlDepthExceeded`

### CLI Validation

`nexus42 preset validate <path>` checks a preset YAML file for:
- Valid YAML syntax
- Size and depth limit compliance
- Structural correctness (required fields, valid state references)

## Design Reference

Full design: `.agents/knowledge/specs/orchestration-engine.md` (sections 7–9 cover presets, loader, validation).
