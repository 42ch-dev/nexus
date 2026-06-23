# Novels-System Reference Distill — V1.36 Baseline

**Archived**: 2026-06-23 (from deferred-features-cross-version-tracker.md §3.6 clean-up)
**Source**: `~/workspace/organizations/42ch/internal-sharing/novels-system/`
**Distilled by**: `@project-manager`, 2026-06-07 V1.36 prepare wave
**Consumed by**: V1.36–V1.42 novel-writing feature line

---

## Source system overview

Obsidian + Redis + InStreet literary API; multi-novel, multi-role, multi-chapter serial production system.

Key files: `shared-rules/novel-system-rules.md` (790 lines), `cron-prompts/{novel-brainstorm,novel-write,novel-review,novel-publish}.md`, `schemas/{novel-active,novel-state,novel-review-iteration}.schema.json`, `templates/novel/*.md` (20 templates).

## Capability matrix (novels-system × OSS disposition)

| Capability | novels-system | OSS disposition | Tracker |
|---|---|---|---|
| Layout root | `{作品目录}/` (per-work, 7 subdirs) | `Works/<work_ref>/` + 4 subdirs | DF-57 / DF-63 |
| Chapter state SSOT | `作品状态.md` file | `work_chapters` DB table | V1.36 |
| Worldbuilding | Per-work `世界设定/` (7 sub-types) | World KB (entity-scope-model.md §5.4) | DF-63 |
| Quality loop | review cron + 五问 + findings + 96h escalation | `llm_judge` gate (V1.36); full loop (V1.39) | DF-64 / DF-67 |
| State storage | Redis | Local SQLite | PD-05 |
| Three-layer rules | writing-craft / novel_rules / history | Embedded rules (V1.40) | DF-65 |
| Multi-volume | Per-volume outline + chapter range | Volume-aware auto-chain (V1.42) | DF-62 |
| Selection pool | Obsidian 选题库 + 灵感池 | DB SSOT + `Pool/Ideas/` (V1.41) | DF-61 |
| Auto-chain | 3-cron staggering (brainstorm/write/review) | FL-E stage chain (V1.39) | DF-53 |

## Re-open instructions

When future iterations revisit novel-writing features:
1. Read `novels-system/shared-rules/novel-system-rules.md` as SSOT
2. Map Redis → local DB, Obsidian → `Works/<work_ref>/` subdirs, InStreet API → CLI or sync boundary
3. Update the relevant spec per mapping
