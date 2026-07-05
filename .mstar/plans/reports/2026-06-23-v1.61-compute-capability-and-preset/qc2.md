---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-23-v1.61-compute-capability-and-preset"
verdict: "Approve"
generated_at: "2026-06-23"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: grok-build-0.1
- Review Perspective: security and correctness risk (untrusted WASM output applied to KB state; state delta merge semantics; compute_error handling; battle_report cap; timeline event creation)
- Report Timestamp: 2026-06-23

## Scope
- plan_id: 2026-06-23-v1.61-compute-capability-and-preset
- Review range / Diff basis: 6e0bb90b..feature/v1.61-compute-capability-and-preset
- Working branch (verified): iteration/v1.61
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 14 (primary: crates/nexus-orchestration/src/capability/builtins/narrative_compute.rs; also preset YAML/prompts, capability registry test bump, Cargo.toml, plan doc)
- Commit range: equivalent to git diff 6e0bb90b..feature/v1.61-compute-capability-and-preset (new capability implementation + embedded combat-engine preset)
- Tools run: git branch/rev-parse/log/diff verification, cargo test -p nexus-orchestration (full suite, all passing), manual source review of apply_state_delta / apply_json_delta / apply_op_to_field / handle_compute_error / run(), mental adversarial construction per assignment (path traversal, unknown ops, type mismatches, missing keys, oversized report, WASM traps)

## Findings

### 🔴 Critical
None.

### 🟡 Warning
- **W-1 (correctness — partial state application)**: `apply_state_delta` processes the list sequentially with independent `update_key_block` calls per delta. An error on delta N (e.g., type mismatch on a later entry, missing intermediate for add/sub) leaves deltas 0..N-1 already persisted. No enclosing transaction and no pre-validation pass + apply-all-or-nothing. The `applied` count is lost on error path. While individual deltas are validated before mutation, batch semantics under malformed untrusted output are not atomic.
- **W-2 (untrusted output handling — new_key_blocks scoping)**: `create_new_key_blocks` constructs `KeyBlock` directly from the module-emitted `ComputeOutput.new_key_blocks` and inserts via `kb_store.insert_key_block` using whatever `world_id` the emitted block declares. No re-assertion that the block's world_id matches the capability's admitted `parsed.world_id`. Timeline appends correctly use the context `world_id`/`branch_id`; state_delta is indirectly scoped because targets must be looked up from the world-filtered snapshot. A malicious module (even if only given this world's snapshot) that fabricates a foreign world_id in a new KeyBlock can cause cross-world KB pollution on insert. Practical risk is low (embedded modules trusted; user modules see only their world's data and world_ids are not enumerable via host fns), but defense-in-depth is absent.

### 🟢 Suggestion
- S-1: After `kb_store.get_key_block(target_id)` in `apply_state_delta` and after `KeyBlock::from(...)` in `create_new_key_blocks`, add an explicit `if kb.world_id != parsed.world_id { return Err(CapabilityError::InputInvalid(...)) }` (or equivalent) for belt-and-suspenders against crafted output.
- S-2: Consider wrapping the delta application loop in a single sqlx transaction (or document "best-effort prefix apply; errors stop further deltas but prior ones are durable").
- S-3: Add an explicit upper bound on `state_delta.len()` (or serialized size of the whole output envelope) before processing, or rely on/document the WASM fuel+memory limits as the bounding mechanism. Currently only `battle_report` has a hard 64 KiB serialized cap.
- S-4: The battle_report size check is `if let Ok(report_bytes) = serde_json::to_vec(...)`; on serialization failure an oversized report could still be surfaced in the success path. Consider treating unmeasurable report as `InputInvalid`.
- S-5: `apply_json_delta` requires the top-level state namespace (`state.character`, etc.) to pre-exist even for `set`; it only auto-creates intermediate objects for deeper segments under `set`. Add a unit test (or doc comment) clarifying whether computable KeyBlocks are required to carry a pre-seeded `state.<block_type>` skeleton (current seeding in tests does this; module authors should too).
- S-6: The `_world_id` parameter to `apply_state_delta` is unused. Either use it for the re-check in S-1 or remove the underscore and assert in tests.
- S-7: `handle_compute_error` records the event best-effort; if the append itself fails the original WASM error is still returned. This is acceptable graceful degradation, but consider surfacing a combined diagnostic in the returned error when the event write also failed.

## Source Trace
- Finding W-1: `narrative_compute.rs:323` (`apply_state_delta` loop) → `390` (`update_key_block` inside loop, no tx) → `264` (called from `run` before new_key_blocks/timeline) + error return at `334`/`344`/`373` etc. that short-circuits.
- Finding W-2: `narrative_compute.rs:526` (`create_new_key_blocks`) → `534` (for each emitted `KeyBlock`) → `537` (`insert_key_block` using `kb_contract` world_id) + contrast with `272` (timeline append uses `parsed.world_id`).
- Battle report cap (R-V161P0-LOW-003): `65` (const), `253` (`to_vec` check) → `254` (early `InputInvalid` return) before line `264` apply.
- compute_error path (graceful, no crash): `238` (match on `engine.compute` Err) → `241` (`handle_compute_error`) → `587` (append "state_update"/"compute_error" with detail) → `620` (return TransientExternal); no panic.
- Op / target / numeric validation (untrusted output): `333` (VALID_OPS contains), `341` (target_id non-empty), `349` (get_key_block must succeed), `370` (first path segment matches block_type_state_key), `488` (numeric extraction for add/sub) → `505` (InputInvalid on mismatch).
- Adversarial cases (mental):
  - Path `../../../etc/passwd` or `..` segments: treated as literal JSON keys; first segment fails `block_type_state_key` match (e.g. must be "character") or later "not found" → InputInvalid. No filesystem effect.
  - Op `delete` / `multiply` / unknown: rejected at `333` before any KB read.
  - String value for `add`/`sub`: `current_num`/`delta_num` None → `505` InputInvalid.
  - Missing state key / missing intermediate for add/sub: `416`, `443` InputInvalid.
  - Oversized battle_report: caught at `254` before side-effects.
  - WASM trap/timeout/fuel: routed to handle_compute_error → timeline event + error return (full_cycle test exercises the error branch conceptually).

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 7 |

**Verdict**: Approve

## Additional Notes (security/correctness focus)
- State delta merge implements the documented semantics (add/sub numeric only; set any; dot-path → nested state.<block_type>.*; first segment validated). All specified error cases from the assignment are explicitly handled and tested.
- Untrusted WASM output is validated at the application layer (op whitelist, target existence + block_type key match, numeric checks for mutating ops, battle_report size) before any KB or timeline mutation. The WASM sandbox itself (fuel/memory/wall-time, per-invocation fresh instance) is provided by the P2 WasmEngine and assumed here per compass Q6.
- compute_error produces a proper TimelineEvent (event_type=state_update, title=compute_error) with the error string preserved; daemon does not crash.
- 64 KiB battle_report cap (R-V161P0-LOW-003) is enforced in the hot path before side effects.
- No evidence of path traversal, injection into state, or missing validation on the three ops.
- Minor residual risks (partial apply, new_key_blocks world scoping) are real but low-severity in the current threat model (embedded module + same-creator worlds + UUID key_block_ids). They are noted as Warnings/Suggestions rather than blockers.
- All `cargo test -p nexus-orchestration` (lib + integration binaries) passed in the reviewed range. Clippy and fmt were reported clean by the implementer.

(End of QC report)
