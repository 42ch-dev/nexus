---
vars:
  initial_idea: { type: string, required: true }
max_tokens: 1500
---

# Novel Project Init — Working Title

The creator has shared their initial idea:

> {{preset.input.initial_idea}}

Confirm the **working title** for the novel. If they haven't provided one, suggest one based on their idea and ask them to confirm or modify.

The title will be stored as `{{title}}` and used throughout the project.

Rules:
- Must be non-empty after trim
- Can contain any characters including Chinese, English, symbols
- Length should be reasonable (under 100 characters preferred)
