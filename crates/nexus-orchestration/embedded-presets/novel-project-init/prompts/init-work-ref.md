---
vars:
  initial_idea: { type: string, required: true }
max_tokens: 1500
---

# Novel Project Init — Work Reference Slug

Ask the creator for a **work reference slug** — a short kebab-case identifier that will become the directory name under `Works/`.

For example:
- "the-last-algorithm" → `Works/the-last-algorithm/`
- "cozy-mystery" → `Works/cozy-mystery/`

The initial idea:

> {{preset.input.initial_idea}}

Guidelines:
- **Auto-suggest** a slug derived from the working title (convert to lowercase, replace spaces/special chars with hyphens, strip non-alphanumeric)
- The creator can accept the suggestion or provide their own
- Rules: lowercase alphanumeric and hyphens only, 2-50 characters, must start with a letter
- This slug is stable for the life of the Work (renaming without DB update is unsupported pre-1.0)
