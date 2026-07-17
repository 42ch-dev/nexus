---
module: nexus-daemon-runtime
date: 2026-07-17
problem_type: convention
category: conventions
severity: medium
plan_id: 2026-07-17-v1.120-strategies-repair
tags: [presets, system-preset, qualified-id, locate_preset, daemon, path-resolution, sessions-filter]
---

# System preset qualified-id resolution (`_system.*`)

## Context

System presets ship inside the app bundle under `presets/_system/<name>/preset.yaml`. Their ids are **qualified** with the `_system.` prefix (e.g. `_system.maintenance`). The on-disk directory is the **stripped** name (`maintenance`), not the qualified id. Two subsystems rely on this mapping:

- **Preset handlers** (`crates/nexus-daemon-runtime/src/api/handlers/preset_management.rs`): `locate_preset` / `reload_preset` resolve an id to a filesystem bundle.
- **Sessions list** (`handlers/orchestration/sessions.rs`): daemon auto-starts `_system.*` sessions at boot (`boot.rs` WS-D); these are hidden from the author-facing Sessions list via a `preset_id.starts_with("_system.")` filter (daemon primary + `filterVisibleSessions` client defensive).

## Guidance

- **Resolve** a qualified id with the canonical helper: strip the `_system.` prefix, then delegate to `nexus_orchestration::system_preset_dir::system_preset_bundle_dir`. SSOT: `system_preset_dir_for_id` in `preset_management.rs`. Never join the qualified id literally (`presets/_system/_system.maintenance/` does not exist → false `NotFound`).
- **Classify** a session/preset as system with `starts_with("_system.")` — the same predicate on both daemon and client.
- Any **new** id→path lookup must reuse the helper; check `strategy.rs:145`-style literal joins when touching preset paths (known residual R-V1120P0QC2-S002).

## Why This Matters

Joining the qualified id literally is a recurring bug class: it appeared in `locate_preset`, `reload_preset` (both fixed V1.120 P0), and `strategy.rs:145` (residual). Symptoms are misleading — canvas `ErrorState` (`common.error.title`) for a preset that plainly exists, or `404` where `400 read-only` is correct.

## When to Apply

- Adding or modifying any daemon handler that maps a preset id to a filesystem path.
- Filtering system-vs-author rows in list endpoints (Sessions, future preset surfaces).
- Reviewing preset-path code in QC: grep for `.join("_system")` as a smell.

## Examples

```rust
// WRONG — literal join of the qualified id
user_preset_base_dir(nexus_home).join("_system").join(preset_id).join("preset.yaml")

// RIGHT — strip prefix, delegate to canonical resolver
let dir_name = preset_id.strip_prefix("_system.").unwrap_or(preset_id);
system_preset_bundle_dir(nexus_home, dir_name)
```

Regression tests: `get_preset_returns_system_preset`, `reload_preset_accepts_qualified_system_id` (V1.120 P0 T1, TDD red→green).
