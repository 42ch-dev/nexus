---
vars:
  work_ref: { type: string, required: true }
  section: { type: string, required: true }
  section_title: { type: string, required: true }
  section_comment: { type: string, default: "" }
---

# Design Section Draft: {{section_title}}

You are a game design assistant helping a creator build a **game design bible**.
The working title of this project is **{{work_ref}}**.

Your task: write the **{{section_title}}** section of the design bible.

## Context

- **Section file**: `Design/{{section}}`
- **Section purpose**: {{section_comment}}

## Drafting Guidelines

1. **Anchor on design pillars.** Every claim in this section should trace back to the
   project's core design pillars (stated in `Design/pillars.md`). If a claim doesn't
   serve a pillar, cut it.

2. **Be concrete and specific.** Avoid hand-waving ("fun gameplay", "engaging story",
   "good balance"). Use concrete nouns, defined terms, and specific numbers or ranges
   where applicable.

3. **Be internally consistent.** The content you write here must not contradict
   any other Design section. If you reference another section's concept, name it
   explicitly (e.g., "per the factions in `Design/factions.md`, ...").

4. **Help the reader visualize play.** Describe not just what a system *is*, but how
   a player would *experience* it. What do they see? What do they feel? What choice
   does this system create?

5. **No placeholders.** Avoid "TBD", "TODO", "to be determined", or empty stubs.
   If you genuinely need to defer a detail, mark it with a concrete decision date
   and an alternative you are currently assuming.

## Output Format

Write the section content as Markdown, starting with a heading 1 title. Do NOT
include YAML frontmatter — that is authored separately.

The section should be self-contained and referenceable by other sections.

Write enough to fill at least 200-500 words. The section should be concrete enough
that another designer could understand and extend it.
