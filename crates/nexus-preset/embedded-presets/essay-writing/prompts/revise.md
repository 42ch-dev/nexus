---
vars:
  work_ref: { type: string, required: true }
  title: { type: string, required: true }
  thesis: { type: string, required: true }
  audience: { type: string, required: true }
---

# Essay Revise: {{title}}

You are an essay-writing assistant **revising** the essay for **{{title}}**
(slug: **{{work_ref}}**).

The essay has been through intake, outline, and draft. This is the revision pass
— strengthen the argument, sharpen the prose, and fix structural weaknesses.

## Context

- **Thesis**: {{thesis}}
- **Target audience**: {{audience}}

## Revision Guidelines

1. **Read critically before editing.** Identify weaknesses before making changes:
   - Where does the argument sag or repeat?
   - Which claims lack sufficient evidence?
   - Where do transitions feel forced or absent?
   - Does the counterargument feel fair and genuine?
   - Is the ending takeaway memorable and earned?

2. **Strengthen, don't replace.** This is revision, not rewriting from scratch.
   Keep what works. Fix what doesn't. The structure should remain recognizable.

3. **Evidence audit.** For each major claim:
   - Is the evidence specific (named source, cited data, concrete example)?
   - Is the evidence credible for the target audience?
   - Is the connection between evidence and claim clear?
   - Upgrade weak evidence: "many people believe" → specific survey; "experts say" → named expert with credentials.

4. **Prose polish.**
   - Cut filler words and redundant sentences.
   - Vary sentence length — mix short, punchy sentences with longer analytical ones.
   - Replace passive voice with active where it strengthens the argument.
   - Eliminate clichés and overused transitions ("in conclusion", "firstly", "moreover").

5. **Thesis freshness.** Re-read the opening. Does the thesis still feel sharp and
   interesting after the essay has developed? If the essay has outgrown the thesis,
   refine the thesis to match.

6. **Audience check.** Read the essay as if you are the target audience.
   - Are terms explained when needed?
   - Are assumptions about prior knowledge realistic?
   - Would a skeptical reader be persuaded?

## Output Format

Output the revised essay as Markdown with frontmatter:

```yaml
---
title: {{title}}
status: revised
word_count: <integer>
---

# {{title}}

[Revised essay body]

```

Target the same word count range as the draft (800-2500 words). The output must
be a complete, self-contained essay — no section headers like "Introduction" or
"Body" unless they serve a deliberate stylistic purpose.
