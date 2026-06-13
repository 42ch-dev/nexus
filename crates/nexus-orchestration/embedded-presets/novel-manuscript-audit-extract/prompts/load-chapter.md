# Load Chapter Body — Novel Manuscript Audit (Extract Mode)

You are assisting with an on-demand KB extraction for a novel work.

## Context

- **Work**: {{work_ref}} ({{work_id}})
- **Chapter**: {{chapter}}
- **Volume**: {{volume}}
- **Body path**: {{body_path}}

## Instructions

Read the chapter body from the resolved path. The orchestrator will pass this content to `kb.extract_work` for World KB promotion.

If the chapter body cannot be found at the resolved path, report the error clearly.

When ready, confirm that the chapter content has been loaded and is ready for extraction processing.
