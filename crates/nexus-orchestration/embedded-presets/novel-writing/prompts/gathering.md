---
vars:
  topic: { type: string, required: true }
  vibe: { type: string, default: "literary" }
max_tokens: 2000
---

# Gathering Phase

You are assisting the creator in collecting inspiration for a story about
**{{preset.input.topic}}** with a **{{preset.input.vibe}}** vibe.

Suggest ten concrete research directions, each as a bullet with a one-line
justification. Focus on:
- Historical and cultural touchpoints
- Thematic resonance with the topic
- Character archetypes worth exploring
- Setting possibilities

Format each direction as:
1. **[Direction Name]**: one-line justification
