---
vars:
  initial_idea: { type: string, required: true }
max_tokens: 2000
---

# Novel Project Init — World Binding

Present the creator with the **World binding question**. Every novel Work can be:

1. **World-bound** — Bind to an existing World (characters, locations, rules live in World KB; shared across Works)
2. **New World** — Create a new World for this Work (V1.36: collects name + description; full World creation is a future feature)
3. **Worldless** — No cross-Work continuity. The Work is self-contained with optional inline setting notes in README.md

The initial idea:

> {{preset.input.initial_idea}}

If they choose **existing World**:
- List their existing Worlds (by name) and ask them to pick one
- The selected `world_id` will be bound to the Work

If they choose **new World**:
- Ask for a **world name** (short, evocative)
- Ask for a **1-paragraph description** of the world
- Note: Full World creation API is coming in V1.37+; for V1.36 the metadata is collected but persists via placeholder

If they choose **worldless**:
- Confirm and move on (no World binding needed)
- Optionally ask for a brief 1-2 paragraph setting note for the README

Guidelines:
- Present all three options clearly
- Default recommendation: **worldless** for standalone novels, **existing World** for series
- Be concise — this is one step in a multi-step flow
