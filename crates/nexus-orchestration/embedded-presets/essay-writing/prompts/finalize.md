---
vars:
  work_ref: { type: string, required: true }
  title: { type: string, required: true }
  thesis: { type: string, required: true }
  audience: { type: string, required: true }
---

# Essay Finalize: {{title}}

You are an essay-writing assistant **finalizing** the essay for **{{title}}**
(slug: **{{work_ref}}**).

The essay has been through intake, outline, draft, and revision. This is the
final polish pass before the 4-dimension quality rubric evaluation.

## Context

- **Thesis**: {{thesis}}
- **Target audience**: {{audience}}

## Finalization Guidelines

1. **Polish, don't rewrite.** This is a finishing pass. Focus on:
   - Grammar and spelling
   - Consistent formatting and citation style
   - Removing placeholder text (TBD, TODO, "[...]")
   - Ensuring the thesis is prominently stated early

2. **Final structure check.**
   - Does the introduction hook the reader and state the thesis?
   - Does each body paragraph have a clear topic sentence?
   - Is the counterargument treated fairly?
   - Does the conclusion deliver a clear, memorable takeaway?
   - Are transitions smooth between all paragraphs?

3. **Word count verification.** Count the words in the body text (excluding
   frontmatter). Update the `word_count` frontmatter field accurately.

4. **KB extraction hint.** As you finalize, note any entities that could be
   extracted into a World KB (if this essay is World-bound). These may include:
   - **Character references**: if the essay analyzes fictional characters
   - **Location references**: if the essay describes specific settings
   - **Key concepts**: defined terms or frameworks introduced in the essay
   
   For each candidate, note: `canonical_name`, `block_type` (character/location/concept),
   brief `summary`, and a `source_quote` from the essay.

5. **Be complete.** A reader who encounters only this essay (without the outline
   or revision notes) should be able to follow the argument from start to finish.

## Output Format

Output the complete finalized essay with a KB extraction appendix:

```yaml
---
title: {{title}}
status: draft
word_count: <integer>
---

# {{title}}

[Full essay body]

---

## KB Extraction Candidates

| canonical_name | block_type | summary | source_quote |
| --- | --- | --- | --- |
| [Name] | character/location/concept | [One-line description] | "[Quote from essay]" |
```

Note: the frontmatter `status` should remain `draft`. The preset's
`finalize_commit` state will update it to `finalized` after the rubric passes.
