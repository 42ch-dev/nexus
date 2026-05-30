---
vars:
  topic: { type: string, required: true }
max_tokens: 500
---

# Draft Quality Check

Evaluate the quality of the draft produced for the topic:

**{{preset.input.topic}}**

Assess whether the draft:
1. Directly addresses the topic with substantive content
2. Has clear structure and logical flow
3. Is free of major factual or logical errors
4. Demonstrates sufficient depth and specificity

If the draft meets these criteria at a "good enough" level for its purpose, respond with "GO". If it needs significant revision, respond with "WAIT" and briefly explain what needs improvement.
