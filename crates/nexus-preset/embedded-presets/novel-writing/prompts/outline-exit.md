---
max_tokens: 2000
---

# Chapter Outline — 五问质量检验 (Five-Question Quality Gate)

You are evaluating whether the chapter outline is strong enough to begin drafting. Apply the outline 五问 quality check per the novel-workflow-profile spec §5.1.1.

Read the outline from `Works/{{preset.input.work_ref}}/Outlines/chapters/`.

## The Five Questions

Evaluate each question and respond with **GO** (pass) or **NOGO** (fail) with a brief reason:

### 1. 结构 (Structure)
Does the outline have clear beat-level structure (sections, bullets, or numbered beats) that covers the chapter from opening to closing?
- GO: The outline is organized into distinct beats or sections with clear progression.
- NOGO: The outline is a single dense paragraph or lacks any structural markers.

### 2. 弧线 (Arc)
Does the outline describe a character or situation arc — conflict, stakes, or a meaningful change?
- GO: At least one character faces a choice, obstacle, or revelation.
- NOGO: The outline is purely descriptive with no tension or change.

### 3. 伏笔 (Foreshadow)
Does the outline plant or reference future story elements (F### items, promises, seeds)?
- GO: At least one future-setup element is present.
- NOGO: The outline stands in isolation with no connective tissue to the larger story.

### 4. 节奏 (Pacing)
Is the outline a concise blueprint rather than a full draft or a bare skeleton?
- GO: The outline is detailed enough to guide drafting but not so long that it becomes prose.
- NOGO: The outline is empty, extremely short, or longer than a reasonable chapter summary.

### 5. 钩子 (Hook)
Does the outline end with a hook — unresolved tension, a question, or a compulsion to read the next chapter?
- GO: The final beat leaves the reader wanting more.
- NOGO: The outline ends flatly with all tension resolved.

## Verdict

- If **all five** questions pass → respond with `GO` and a one-line summary
- If **any** question fails → respond with `NOGO` and list which questions failed with brief explanations

## Response Format

```
VERDICT: GO or NOGO

Q1 (Structure): PASS/FAIL — <reason>
Q2 (Arc): PASS/FAIL — <reason>
Q3 (Foreshadow): PASS/FAIL — <reason>
Q4 (Pacing): PASS/FAIL — <reason>
Q5 (Hook): PASS/FAIL — <reason>

SUMMARY: <one-line summary>
```
