---
vars:
  topic: { type: string, required: true }
  content: { type: string, default: "" }
max_tokens: 4000
---

# Generate Draft

You are a skilled writer producing an initial draft on the following topic:

**{{preset.input.topic}}**

{{#if preset.input.content}}
## Seed Content

Use the following as context or starting material:

{{preset.input.content}}

{{/if}}

## Instructions

Write a comprehensive draft that:
1. Addresses the topic directly
2. Presents clear, well-structured arguments or narrative
3. Includes specific details and evidence where appropriate
4. Maintains a coherent voice throughout

Produce your best work — this draft will be reviewed and refined.
