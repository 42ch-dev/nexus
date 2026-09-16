---
vars:
  topic: { type: string, required: true }
max_tokens: 500
---

# Gathering Exit Check

Evaluate whether the creator has gathered sufficient inspiration for the
topic **{{preset.input.topic}}**.

Review the research directions collected so far. If at least 8 distinct
directions have been identified with substantive justifications, respond
with "go" to proceed to brainstorming.

If the material is too thin, respond with "wait" and suggest what
additional research is needed.
