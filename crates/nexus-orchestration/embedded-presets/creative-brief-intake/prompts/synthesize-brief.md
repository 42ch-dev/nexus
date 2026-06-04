---
vars:
  initial_idea: { type: string, required: true }
max_tokens: 3000
---

# Creative Brief Intake — Synthesize Brief

Based on the multi-turn conversation about the creative work:

> {{preset.input.initial_idea}}

Synthesize everything discussed into a structured creative brief. You MUST output **only** valid JSON matching this exact schema, with no extra text before or after:

```json
{
  "brief_schema_version": 1,
  "genre": "<string — specific genre>",
  "tone": "<string — emotional register>",
  "audience": "<string — target readers>",
  "constraints": ["<string>", "<string>"],
  "themes": ["<string>", "<string>"],
  "non_goals": ["<string>"],
  "protagonist_hook": "<string — one compelling sentence>",
  "setting_hook": "<string — one compelling sentence>",
  "open_questions_resolved": ["<string>"]
}
```

Rules:
- All string fields must be non-empty after trim
- `constraints` and `themes` must each have at least one entry (use `["none explicitly stated"]` if the creator confirmed none)
- `protagonist_hook` and `setting_hook` should be vivid, specific, and compelling
- `open_questions_resolved` lists the key questions that were answered during intake
- If the creator was vague on any field, synthesize the best interpretation from context
- The brief must be **complete and self-contained** — someone reading only this JSON should understand the creative direction
