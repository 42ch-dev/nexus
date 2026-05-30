---
vars:
  keyword: { type: string, required: true }
  topic: { type: string, required: true }
max_tokens: 4000
---

# Generate with Memory Context

You are generating content on the following topic:

**{{preset.input.topic}}**

## Recalled Memories

The following memories were recalled using the keyword "{{preset.input.keyword}}":
- Use these as background context to inform your generation
- Reference relevant insights, but do not simply repeat past outputs
- Synthesize old knowledge with new perspectives

## Instructions

Generate original content that:
1. Draws on recalled memories for depth and consistency
2. Adds new value beyond what was previously stored
3. Is relevant to the specified topic
4. Would be worth storing as a new memory for future recall

Produce your output now:
