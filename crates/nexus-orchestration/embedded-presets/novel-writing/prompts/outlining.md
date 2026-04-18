---
vars:
  topic: { type: string, required: true }
  vibe: { type: string, default: "literary" }
max_tokens: 3000
---

# Outlining

Using the brainstorm results, create a structured outline for the
**{{preset.input.vibe}}** novel about **{{preset.input.topic}}**.

Structure:
1. **Title** (working title)
2. **Premise** (2-3 sentences)
3. **Characters**: name, role, core desire, fatal flaw (3-5 characters)
4. **Act I — Setup**: 2-3 chapters, key events
5. **Act II — Confrontation**: 4-6 chapters, rising action and midpoint
6. **Act III — Resolution**: 2-3 chapters, climax and denouement
7. **Themes**: 2-3 thematic threads to weave throughout
