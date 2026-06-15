# Load Chapter — Novel Chapter Review (V1.47 P0)

You are preparing to review a novel chapter for the **{{work_ref}}** Work.

## Context

- **Work**: {{work_ref}} ({{work_id}})
- **Chapter**: {{chapter}} (label: {{chapter_label}})
- **Slug**: {{slug}}

## Files to read

- **Chapter body**: `{{body_path}}`
- **Chapter outline**: `{{outline_path}}`

If either path is missing or empty, note the gap and continue with whatever
context is available — do not block the review pass on a missing file.

## Work-level context

- **Creative brief**: {{creative_brief}}
- **Inspiration log**: {{inspiration_log}}

## Rules layers (best-effort)

{{rules_content}}

## World KB (best-effort, World-bound Works only)

{{world_kb_block}}

## Output

Read the chapter body and outline into your working context. Do **not** write
any review output in this step — that happens in the `review` state. Just
confirm you have loaded the chapter context and are ready to perform the
structured 五問 review.
