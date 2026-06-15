# Novel Chapter Review — FL-E `review` Stage (V1.47 P0)

This preset replaces the generic `reflection-loop` demo (V1.31 agentic-design
pattern) per V1.47 compass §0.1 #6. It is the FL-E `review` stage producer
that writes ≥1 finding row per review pass via the supervisor's terminal hook
(novel-quality-loop.md §8).

## What it does

1. Loads chapter body + outline + World KB (best-effort) for the active
   chapter of a `work_profile: novel` Work.
2. Runs a structured 五问 review (character / plot / setting / theme / language).
3. Emits a human-readable review report under `Works/<work_ref>/Logs/review/`.
4. On schedule terminal, the supervisor's `on_schedule_terminal` hook
   synthesizes ≥1 `findings` row via `create_finding_from_review` (the same
   path used by the on-demand `creator run novel-chapter-review`).

## Inputs (populated by `stage_gates::build_preset_input`)

- `work_id`, `work_ref`, `chapter`, `chapter_label`, `body_path`,
  `outline_path`, `slug` — chapter context.
- `creative_brief`, `inspiration_log` — Work-level creative context.
- `rules_content` — Layer 1 + Layer 2 writing-craft rules (best-effort).
- `world_kb_block` — World KB YAML block (empty for worldless Works).

## CLI

```bash
# Auto-chain (after `produce` completes; driver invariant preserved)
#  — no user action; supervisor auto-advances and persists findings.

# On-demand
nexus42 creator run novel-chapter-review <work_id>
```

## Out of V1.47 P0

- Accepting a `rule_suggestion` and mutating
  `Works/<work_ref>/AGENTS.md` (deferred to V1.48+).
- Findings → draft prompt enrichment (§5.5.2).
- Conditional inner_graph cycle for iterative refinement (FL-D / DF-56).
