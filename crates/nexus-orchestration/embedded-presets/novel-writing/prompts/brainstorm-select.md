---
vars:
  topic: { type: string, required: true }
  vibe: { type: string, default: "literary" }
max_tokens: 2000
---

# Brainstorm: Select

Given the themed clusters for a **{{preset.input.vibe}}** novel about
**{{preset.input.topic}}**, select the single most promising direction.

Provide:
1. **Chosen concept**: the specific concept (or fusion of 2-3) you recommend
2. **Why**: 2-3 sentences on why this direction has the strongest narrative
   potential
3. **Logline**: a one-sentence summary of the resulting story
4. **Opening image**: a vivid visual that could open the novel

Be decisive. Pick one direction, not a menu of options.
