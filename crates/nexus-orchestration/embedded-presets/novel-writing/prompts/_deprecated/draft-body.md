---
vars:
  topic: { type: string, required: true }
  vibe: { type: string, default: "literary" }
max_tokens: 4000
---

# Draft: Body

Continue the **{{preset.input.vibe}}** novel about **{{preset.input.topic}}**
following the outline and building on the introduction.

**Output path**: Write the body chapters to:
`Stories/{{preset.input.story_ref}}/ch02-body.md`

Each chapter should be a separate file in the `Stories/{{preset.input.story_ref}}/` directory.
Use the naming pattern `ch<nn>-<descriptive-slug>.md`.

Write 1500-3000 words covering the middle of the story:
- Rising action through Acts II
- Character development and relationship dynamics
- Key scenes that escalate the central conflict
- The midpoint reversal or revelation
- Build toward the climactic confrontation

Maintain the voice and tone established in the introduction.
Each scene should advance plot, reveal character, or deepen theme.
