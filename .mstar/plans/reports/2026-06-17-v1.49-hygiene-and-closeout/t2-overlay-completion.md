## Completion Report v2 — P-last T2 (overlay promotion)

- **plan_id**: 2026-06-17-v1.49-hygiene-and-closeout
- **owner**: @writing-specialist
- **Task**: T2 — Overlay promotion into Masters
- **Working branch used**: docs/v1.49-overlay-promotion
- **Worktree path**: .worktrees/v1.49-overlay-promotion
- **Base**: iteration/v1.49 @ `33f93f7b`
- **Commits**:
  - `a92306bf` docs(specs/v1.49): T2 fold findings-lifecycle.md into quality-loop.md §2
  - `d2961540` docs(specs/v1.49): T2 fold narrative-indexes.md into workflow-profile.md §4.6 (5-col reconciliation per R-V149P1-01)
  - `5628171f` docs(specs/v1.49): T2 verify and update author-experience.md §8 header (Shipped V1.49 P2)
  - `e62aabcf` docs(specs/v1.49): T2 supersede workflow-profile.md §5.5.1 three-state paragraph (→ quality-loop.md §2)
  - `8b253375` docs(specs/v1.49): T2 update novel-writing/README.md index for overlay promotion
- **Folds**:
  - `findings-lifecycle.md` (Draft V1.49) → `quality-loop.md` §2.3–§2.7: 6-state lifecycle, transition state machine, actionable set, migration, error classification (INVALID_TRANSITION vs INVALID_INPUT)
  - `narrative-indexes.md` (Draft V1.49) → `workflow-profile.md` §4.6: 5-col F### schema (ID | Description | Planted | Paid off | Status), allocation, inline format, promotion path, read-for-prompt, E### stub
  - `author-experience.md` §8 (Draft V1.49 P2): header updated to Shipped (V1.49 P2 — author desk UX integrated) + cross-refs added to quality-loop.md §2 and workflow-profile.md §4.6
- **Acceptance criteria**:
  1. ✅ All 3 overlays updated with "Superseded" headers and supersession notes (findings-lifecycle.md, narrative-indexes.md; author-experience.md §8 marked Shipped)
  2. ✅ Master documents updated with new content + header (quality-loop.md §2.1 schema updated to 6-state + §2.3–§2.7 added; workflow-profile.md §4.6 created; author-experience.md headers updated)
  3. ✅ §5.5.1 three-state paragraph superseded — now points to quality-loop.md §2
  4. ✅ novel-writing/README.md index updated (statuses, authority matrix, overlay table)
  5. ✅ No behavior change — documentation-only; all edits are in .mstar/knowledge/specs/
  6. ✅ 5 well-scoped commits with `docs(specs/v1.49):` prefix
- **Risks / follow-ups**: none
- **Ready for P-last T4/T5 (PM-driven)**: yes
