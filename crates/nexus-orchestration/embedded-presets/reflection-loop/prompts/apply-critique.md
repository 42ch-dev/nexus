---
vars:
  topic: { type: string, required: true }
max_tokens: 4000
---

# Apply Critique

You are revising a draft based on critical feedback. The topic is:

**{{preset.input.topic}}

**

## Instructions

Review the existing draft and any critique from the quality evaluation. Then produce a revised version that:
1. Addresses each point of feedback directly
2. Strengthens weak arguments or underdeveloped sections
3. Improves clarity, flow, and coherence
4. Maintains the strengths of the original draft

Produce the complete revised text — do not show diffs or partial edits.
