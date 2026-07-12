---
iteration_id: V1.112
start_date: 2026-07-12
status: completed
end_date: 2026-07-12
iteration_base_branch: main
target_branch: main
spec_integration_branch: iteration/v1.112
plans:
  - 2026-07-12-v1.112-i18n-foundation
  - 2026-07-12-v1.112-i18n-ui-migration
---

# V1.112 Delivery Compass — Frontend i18n

## User value

Authors who prefer **简体中文** can use the local Control Room (setup, canvas,
reading, findings, memory) in their language without waiting for cloud UI.
English remains the default and fallback; system language is honored when
supported. Language preference is discoverable in Settings and applies
immediately — no restart.

## Scope

**Direction lock mode: interactive** (grill-me, 2026-07-12).

Equip `apps/web` with standard i18n (`i18next` + `react-i18next`), fully adapt
**user-facing** UI copy to `en` + `zh-CN`, default from system language with
`en` fallback, Language control in Settings **Appearance**, instant switch.
Desktop is covered via the shared SPA. Future user-facing UI must use i18n.

**User-facing** means product chrome an author sees while using the Control Room:
navigation, settings, setup, canvas labels, command palette, toasts, empty
states, and error helpers. It does **not** include manuscript body text, CLI /
daemon logs, developer-auxiliary surfaces (`apps/design-studio`), or copy baked
into shared `@42ch/nexus-ui` primitives (callers pass localized strings).

1. **P0 — i18n foundation + Appearance:** Dependencies, `LocaleProvider`,
   locale resolve/persist (`localStorage` `nexus-web-locale`), Settings
   Appearance + Language, document product conventions (caller-owned copy;
   user-facing-only catalogs), migrate shell/nav/settings as the sample path.
2. **P1 — Full user-facing UI migration:** Migrate remaining user-facing
   product strings (setup, canvas, reading, findings, memory/soul, command
   palette, toasts/errors); wire `format.ts` / `Intl` to active locale.
   `blocked_by` P0.

### Locked decisions (grill-me)

| Topic | Decision |
|-------|----------|
| Library | `i18next` + `react-i18next` |
| Surfaces | `apps/web` only (desktop via shared SPA) |
| Conventions | (1) presentational copy is caller-owned; (2) only user-facing copy in catalogs — no design-studio / dev-auxiliary inflation |
| Settings | New **Appearance** section (nav + page title use localized “Appearance” / 外观); Language control with three options — **System** (follow OS/browser), **English**, **简体中文**; theme stays in header this iteration |
| Persistence | `localStorage` `nexus-web-locale` = `system` \| `en` \| `zh-CN`; resolve stored → system (`zh*` → `zh-CN`, else `en`) → `en`; no daemon/config.toml; CLI English-only |
| UX | Instant language switch; sync `document.documentElement.lang` |
| Plan split | 2 Must plans (P0 → P1) |

## Plans

| plan_id | Name | Status | Notes |
|---------|------|--------|-------|
| 2026-07-12-v1.112-i18n-foundation | P0 — i18n foundation + Appearance | Done | Must; QC Approve with residuals (6 R#), QA Pass |
| 2026-07-12-v1.112-i18n-ui-migration | P1 — Full user-facing UI migration | Done | Must; QC Approve with residuals (3 R#), QA Pass |

Status values: `Todo` | `InProgress` | `InReview` | `Done` | `Blocked`

## Milestones

| Milestone | Target date | Status |
|-----------|-------------|--------|
| Spec freeze | 2026-07-12 | done |
| Dev complete | 2026-07-12 | done |
| QC complete | 2026-07-12 | done |
| Iteration close | 2026-07-12 | done |

## Acceptance Criteria

**P0 (foundation + Appearance)**

- [ ] Locale resolves in order: stored `nexus-web-locale` → system
  (`navigator.language`; `zh*` → `zh-CN`, else `en` when not an exact supported
  match) → fallback `en`
- [ ] Settings has an **Appearance** section with Language control offering
  System / English / 简体中文; choice persists in `localStorage` and survives reload
- [ ] Changing Language updates visible UI immediately (no full page reload);
  `document.documentElement.lang` matches the active locale
- [ ] Shell, nav, and settings chrome (including Appearance) demonstrate the
  migration pattern via `t()` / catalogs
- [ ] Product conventions documented in `apps/web/AGENTS.md` and P0 spec:
  caller-owned copy; user-facing-only catalogs

**P1 (full migration) — iteration Done gate**

- [ ] All **user-facing** product UI in `apps/web` renders via i18n keys (setup,
  canvas, reading, findings, memory/soul, command palette, toasts/errors)
- [ ] `format.ts` and related `Intl` / relative-time helpers use the **active**
  UI locale (not a stale browser-default when UI is `zh-CN`)
- [ ] Leftover audit (grep / checklist) shows no hardcoded user-facing product
  copy in production UI modules
- [ ] CLI / daemon messages remain English

## Non-Goals

- Daemon / CLI message localization
- Manuscript / author content translation
- `nexus-platform` cloud UI
- Languages beyond `en` / `zh-CN`
- i18n for developer-auxiliary surfaces (including `apps/design-studio`) or other
  non-product tooling
- Baking product copy into `@42ch/nexus-ui` primitives (caller-owned copy)
- Moving theme into Appearance this iteration
- Wire contract / schema changes (`wire_contracts_changed: false`)

## Roadmap Position

- **Current iteration (V1.112): delivered** - Standard frontend i18n foundation + full
  user-facing UI migration for `en` / `zh-CN` with Settings **Appearance**
  Language control.
- **Next iteration:** Resume deferred canvas trajectory items (graph layout
  engine dagre/elk; Strategy onConnect for inner-graph groups) and/or residual
  paydown — trigger: V1.112 Done + author priority; owner: PM at iteration-start.
- **North star:** Local-first Control Room fully usable in the author's preferred
  language without inflating catalogs with non-product copy.

## Delivery Branch Policy

> Mirror of frontmatter; keep in sync with `.mstar/status.json` `metadata`.

| Field | Value |
|-------|-------|
| `iteration_base_branch` | `main` |
| `spec_integration_branch` | `iteration/v1.112` |
| `target_branch` | `main` |

## Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Large string surface misses leftovers | Med | Med | P1 Done gate includes grep/audit for hardcoded user-facing copy; QC checklist |
| Instant switch misses non-React strings | Low | Med | Centralize command-registry / toast helpers on `t()`; sync `html[lang]` |
| Catalog key sprawl / inconsistent namespaces | Med | Low | Locked namespace map + key convention in P0 spec/plan; sample migration before P1 |
| Command registry mount-once semantics vs instant switch | Low | Med | Locked: store `labelKey`/`groupKey`; palette resolves via `t()` at render (P1 spec) |
| `format.ts` uses `Intl` default locale | Med | Med | Locked: `getActiveLocale()` from i18n singleton; P1 wires all call sites |

## Iteration workspace

- Workspace: `v1.112/` | Iteration specs for i18n foundation + UI migration

## Quality Gate Summary

> Filled at iteration-close.

| plan_id | QC decision | QA gate | Residuals | Durable summary |
|---------|-------------|---------|-----------|-----------------|
| 2026-07-12-v1.112-i18n-foundation | Approve with residuals | Pass (mandatory) | 6 R# (all deferred) | QC tri: clean architecture, ThemeProvider mirror pattern; 1 fix (aria-label); localStorage precedent noted |
| 2026-07-12-v1.112-i18n-ui-migration | Approve with residuals | Pass (mandatory) | 3 R# (all deferred) | QC tri: Command labelKey pattern, format.ts wiring; 1 fix (dialog.tsx regression); ~25 secondary page strings deferred |

## Compound Round Summary

> Filled at iteration-close.

- 结晶文档数：1 (architecture-patterns/web-i18n-pattern.md)
- 新增 CONCEPTS.md 条目：0 (i18n terms are standard vocabulary)
- 触发 compound-refresh：否

## Iteration Retrospective (minimal)

> Filled at iteration-close.

- 做得好的：(1) LocaleProvider mirrors ThemeProvider - zero architecture friction; (2) Command labelKey + render-time resolution enables instant locale switch without store churn; (3) SDD per-task loop caught issues early (T2 Critical, T3 Critical); (4) Audit gate (FB-MIG-005) effectively identified remaining strings
- 可改进的：(1) ~25 secondary page strings not explicitly assigned to any T1-T4 task - plan should enumerate all in-scope files; (2) Subagent infrastructure was non-functional for large fix waves - PM had to handle fixes directly; (3) QC2 subagent returned empty - tri-review degraded to 2/3; (4) Implementer forgot to commit twice (T1 P1, T3 P1) - commit discipline needed
- 下迭代建议：(1) Migrate R-P1-001 secondary page strings; (2) Resume deferred canvas trajectory items; (3) Consider bundle-split for locale catalogs (R-P0-003); (4) Add runtime guard to setPreference (R-P0-004)
