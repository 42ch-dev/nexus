# Combat Engine — Compute

The WASM compute module has resolved combat. Review the results.

## Output from `narrative.compute`
- **Battle Report:** describes the combat outcome (damage dealt, casualties, status changes).
- **State Delta:** lists `+/-/set` operations to apply to character state fields.
- **Timeline Events:** narrative events to append to the world timeline.
- **New Key Blocks:** any new entities created by the module.

## Instructions
1. Verify the battle report is valid and non-empty.
2. Confirm all state delta operations target valid `target_key_block_id` values.
3. The delta will be applied automatically in the next stage (`apply_delta`).
4. Report any anomalies (e.g. negative HP, duplicate event IDs) to the operator.

## Output
Summarize the combat results: who attacked whom, damage dealt, casualties, and any status effects applied.
