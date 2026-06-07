---
max_tokens: 2000
---

# Chapter Finalize — 五问质量检验 (Five-Question Quality Gate)

You are evaluating whether the chapter draft is ready for finalization. Apply the
**five-question quality check** (五问质量检验) per the novel-workflow-profile spec §5.1.

Read the chapter file from `Works/{{preset.input.work_ref}}/Stories/` and its
corresponding outline from `Works/{{preset.input.work_ref}}/Outlines/chapters/`.

## The Five Questions

Evaluate each question and respond with **GO** (pass) or **NOGO** (fail) with
a brief reason:

### 1. 开场三行 (Opening Three Lines)
Do the first three lines establish **人物** (character), **地点** (location), and **冲突** (conflict)?
- GO: All three elements are present within the opening three lines
- NOGO: Any element missing or too vague

### 2. 冲突回响 (Conflict Resonance)
Is the chapter's central conflict consistent with the **core conflict** described
in the chapter outline?
- GO: Chapter body delivers on the outlined conflict
- NOGO: Conflict drifts from outline or is unresolved without reason

### 3. 伏笔回收 (Twist / Foreshadowing Recall)
Does the chapter honor every **F###** foreshadowing item listed in the outline
(if any)? For each F### item listed in the outline's "Foreshadowing Touched" section,
verify it is addressed in the body.
- GO: All listed F### items are planted or paid off in the body
- NOGO: Any listed F### item is missing from the body without explanation

### 4. 新视角 (New Perspective)
Does the chapter introduce a **new character perspective** or **relationship change**?
- GO: At least one new insight, shift, or revelation occurs
- NOGO: Chapter is purely static — no character development

### 5. 结尾钩子 (Ending Hook)
Does the chapter end with a hook that compels the reader to turn to the next chapter?
- GO: Clear hook (cliffhanger, question, emotional tension, revelation)
- NOGO: Chapter ends flatly with no pull-forward

## Verdict

- If **all five** questions pass → respond with `GO` and a one-line summary
- If **any** question fails → respond with `NOGO` and list which questions failed with brief explanations

## Response Format

```
VERDICT: GO or NOGO

Q1 (Opening Three Lines): PASS/FAIL — <reason>
Q2 (Conflict Resonance): PASS/FAIL — <reason>
Q3 (Foreshadowing Recall): PASS/FAIL — <reason>
Q4 (New Perspective): PASS/FAIL — <reason>
Q5 (Ending Hook): PASS/FAIL — <reason>

SUMMARY: <one-line summary>
```
