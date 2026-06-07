---
vars:
  initial_idea: { type: string, required: true }
max_tokens: 1500
---

# Novel Project Init — Planned Chapter Count

Ask the creator for the **total number of chapters** they plan.

Default: **10**. Range: 1–100.

The initial idea:

> {{preset.input.initial_idea}}

Guidelines:
- If they are unsure, suggest 10 as a reasonable starting point
- For short stories, suggest 1–3 chapters
- For novellas, suggest 5–15
- For full novels, suggest 15–40
- Reassure them this can be adjusted later
- They must provide a specific number (not "undecided")
