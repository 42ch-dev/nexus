---
vars:
  work_ref: { type: string, required: true }
  section_title: { type: string, required: true }
  section_comment: { type: string, default: "" }
---

# Design 五问 Review — {{section_title}}

You are reviewing a game design bible section for quality. The section was just
drafted by a design assistant. Your job is to evaluate it against five quality
dimensions and respond with GO (the section passes) or NOGO (the section needs
revision).

## The Five Questions

Answer each with YES or NO, plus a one-sentence justification. If all five are
YES, respond with GO. Otherwise, respond with NOGO.

### 1. Design Pillars
Does every design claim in this section trace back to a stated design pillar or
constraint? If a claim stands alone without a pillar anchor, say NO.

### 2. Concrete Mechanics
Are the mechanics, systems, or elements described **concretely and specifically**?
Avoid vague hand-waving like "fun combat" or "engaging story." If specifics are
missing, say NO.

### 3. Internal Consistency
Is the section internally consistent with other Design sections? If it contradicts
a known design element (e.g., a faction described differently elsewhere), say NO.

### 4. Player Experience
Can a reader visualize how a player would experience the described element? If the
section describes only abstract properties with no player-facing verbs, feedback,
or moment, say NO.

### 5. Clarity & Completeness
Is the section concrete and specific, avoiding placeholder language (TBD, TODO,
"to be determined")? If the section is a stub (under 80 words) or contains
unresolved placeholders, say NO.

## Response Format

Respond with exactly one of:
- `GO` — if all five questions are YES
- `NOGO: <reason>` — if any question is NO, with a one-line reason

Example:
- `GO`
- `NOGO: mechanics too vague — "combat is fun" needs specific verbs and damage model`
