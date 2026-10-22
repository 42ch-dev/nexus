# Web UI Teardown / Reuse Inventory

| Attribute | Value |
| --- | --- |
| **Status** | **Active** — rebuild prep inventory. Batches 1–2 shipped (PR #429 L1 V1.148, PR #430 L2 V1.149); Batch 3 deferred (trigger: rebuild IA consumption need). |
| **Document class** | Survey-derived inventory (prep input for the web rebuild; not a behavioral spec) |
| **Scope** | Disposition of `apps/web` UI assets: components grading, `components/ui/` per-file state, cross-page patterns, user-named composite blocks |
| **Coordinates with** | [web-ui.md](2026-10-22-web-ui-retired-historical-notes.md), root `DESIGN.md` / `DESIGN.dark.md` (v0.5.0), [web-rebuild-cleanup-list.md](2026-10-22-web-rebuild-cleanup-list-historical-notes.md) |
| **Authored** | 2026-10-22 — product-manager, synthesized from five recorded read-only surveys (plan `20261022-ui-library-teardown` follow-up; docs-only round, no code changed) |

## 1. Purpose & scope

This document is the teardown/reuse inventory of `apps/web/src/components` and `apps/web/src/pages`, produced as input to the upcoming web rebuild (IA work). It is an inventory only: it records grading, dispositions, and migration paths; it does not propose visual or design changes. Batches 1–2 of the `@42ch/nexus-ui` promotion have already shipped (PR #429, PR #430); Batch 3 (layout shells, settings patterns, list-detail shells, read-only detail parts) is deferred with trigger "first consumption need during the web rebuild IA work". Sources: WebComponentsSurvey, WebPagesSurvey, UiLibSurvey, CompositeBlocksSurvey, DeadRoutesSurvey (read-only scouts). Where a grading is a directory-level estimate rather than a per-file audit, the row says so.

## 2. Shipped state (Batches 1–2)

| Batch | PR | Promoted to `@42ch/nexus-ui` | Layer | Notes |
|---|---|---|---|---|
| 1 | #429 | `Dialog`, `Sheet`, `Table`, `States`, `EmptyCreateCard` | L1 primitives | V1.148 |
| 2 | #430 | `ListStateContainer`, `ConfirmDialog`, `FormDialog` + `FormSection` | L2 patterns | V1.149; includes dismissal/pending guards from QC follow-ups |
| 3 | — | `ListPageLayout`, `SettingsSection`/`SettingsLayout`, `ListDetailShell`, read-only detail parts | L2 | **Deferred** — trigger: first consumption need during rebuild IA work |

Already re-exported from `@42ch/nexus-ui` before Batch 1 (app-side `components/ui/*` re-export shims): Button, Badge, Card, Input, Label, Select, Tabs, Textarea, plus brand logos via `components/brand/*`.

## 3. `apps/web/src/components` grading

Grading from WebComponentsSurvey is **directory-level** unless stated otherwise; it was produced under directory-enumeration limits (200-entry page cap), not a per-file audit. Column key — Action: `promote` (promote to @42ch/nexus-ui) / `keep` (keep app-side) / `rewrite` (rewrite at rebuild) / `cleanup` (deletion candidate, see cleanup list). Estimated distribution across the directory: ~15 A (generic UI), ~30 B (parameterizable pattern), ~90 C (domain-coupled) — estimates, not audited counts.

| Module / dir | Action | Priority | Migration path |
|---|---|---|---|
| `ui/` (see per-file table below) | promote (already done) / keep | high | done Batch 1–2 |
| `brand/` (`nexus-logo`, `nexus-text-logo`, `nexus-ink-logo`) | keep (thin app wrappers over `@42ch/nexus-ui` logo assets) | low | keep as-is; app-local `cn` wrappers |
| `layout/` (root-layout, sidebar, header, work-shell, settings-modal-host/context) | keep (shell/routing/entry logic) + cleanup candidates | medium | rebuild IA work (Batch 3 deferred for shell patterns) |
| `layout/presentational/` (shell chrome, dual-pane, `hub-card-list-pane`) | keep; `HubEntityCard` evaluated at Batch 3 | medium | Batch 3 deferred (see §5) |
| `canvas/` (outline/strategy/timeline/world-kb canvases, inspectors, projections) | keep app-side (React Flow + domain data + write boundaries) | low | rewrite at rebuild; share only presentational chrome (`node-chrome-shell`, `timeline-node-chrome` are B-candidates) |
| `setup/` (setup gate, daemon launch gate, agent picker, step fields) | keep (entry-flow coupled) | medium | rewrite at rebuild |
| `memory/` (`memory-detail-panel` etc.) | keep (domain) | low | rewrite at rebuild |
| `findings/` (`finding-detail-panel` etc.) | keep (domain) | low | rewrite at rebuild (see §5 finding entries) |
| `compute/` (`run-studio.tsx` etc.) | keep (Run Studio primitives partially already in nexus-ui) | low | keep; consumes nexus-ui Run Studio components |
| `settings/` (connect-daemon-form etc.) | keep (business forms) | medium | rebuild IA work (Batch 3 settings patterns deferred) |
| `soul/` (`world-selector` etc.) | keep | low | rewrite at rebuild |
| `worlds/` (world-findings panel, severity/rule badges) | keep (domain) | low | rewrite at rebuild |
| `pack/`, `inspector/` | keep (domain) | low | rewrite at rebuild |
| `global-timeline/` | keep (domain view) | low | rewrite at rebuild |
| `reading/` (reading body, chapter nav, annotations, progress) | keep (domain) | low | rewrite at rebuild |
| `selection-submenu/` | keep (B-candidate pattern if a second consumer appears) | low | rebuild IA work |
| `command-palette.tsx` | keep (app shell interaction) | medium | rebuild IA work |
| `path-context-menu.tsx` | keep | low | rewrite at rebuild |
| `daemon-health-indicator.tsx` | keep | low | rewrite at rebuild |
| `theme-provider.tsx`, `locale-provider.tsx` | keep (app-level providers) | medium | keep at rebuild |
| `status-badge.tsx` (root) | keep; potential unify with other badge variants (`world-rule-status-badge`, `world-severity-badge`, `fork-lineage-badge`) — survey-level observation only | medium | rebuild IA work |
| `load-more.tsx` | keep (list pagination primitive; B-candidate) | medium | Batch 3 deferred |

### `components/ui/` per-file state (verified on this branch)

19 files: 13 primitives + `index.ts` + 5 test files. Test files are kept.

| File | State | Migration |
|---|---|---|
| `button.tsx`, `badge.tsx`, `card.tsx`, `input.tsx`, `label.tsx`, `select.tsx`, `tabs.tsx`, `textarea.tsx` | re-export/wrapper of `@42ch/nexus-ui` | pre-Batch-1 done |
| `dialog.tsx`, `sheet.tsx`, `table.tsx`, `states.tsx`, `empty-create-card.tsx` | promoted to `@42ch/nexus-ui` (L1, V1.148) | done Batch 1 |
| `button.test.tsx`, `dialog.test.tsx`, `states.test.tsx`, `table.test.tsx`, `tabs.test.tsx` | tests kept local | done |
| `index.ts` | barrel | done |

## 4. `apps/web/src/pages` cross-page pattern inventory

From WebPagesSurvey. Page file count verifiable from the survey's directory listing: 34 non-test page files at `pages/` root plus `settings/` (8 files incl. `settings-section-frame.tsx`), `dialogs/` (5), `__tests__/` (1 test). Pattern dispositions follow the batch plan; Batch 2 items (`ListStateContainer`, `ConfirmDialog`, `FormDialog`/`FormSection`) are already shipped in `@42ch/nexus-ui`.

| Pattern | Evidence pages | Disposition | Migration path |
|---|---|---|---|
| Shell / nav / settings overlay | `App.tsx` RootLayout; `root-layout.tsx`, `sidebar.tsx`, `work-shell-layout.tsx`, `creator-hub-dual-pane.tsx`, `settings-modal-host.tsx` | Batch 3 deferred (shell patterns); menus/routing stay app-side | rebuild IA work |
| List loading/error/empty lifecycle | works, worlds, chapters, sessions, schedule, capabilities, findings, strategies, modules, world-inspector, work-detail pages | Batch 2 done (`ListStateContainer`); distinguish "no data" vs "no selection" empty states at consumption | done Batch 2 |
| List + filter/selection toolbar + refresh/load-more | `works-page`, `findings-page`, `capabilities-page`, `strategies-page`, `chapters-page`, `modules-page`, `memory-page`, `worlds-page` (World list) | Batch 3 deferred (`ListPageLayout`) | rebuild IA work |
| Table list | `works-page`, `chapters-page`, `sessions-page`, `schedule-page`, `findings-page`, `work-detail-page` | L1 `Table` done Batch 1; slotted table-list layout Batch 3 deferred | done Batch 1 + Batch 3 deferred |
| Form + validation/error feedback | setup steps (`setup-wizard-page`, `setup-step-*`), settings sections (agent/workspace/connection/setup/appearance), dialogs (`create-work`, `create-schedule`, `patch-work`, `scaffold-preset`), `work-inspector-page`, `chapters-page` | Batch 2 done (`FormDialog`/`FormSection`); field rules & mutations stay app-side | done Batch 2 |
| Dialog / confirm flows | `chapters-page` (structure confirm), `strategies-page` (scaffold/validate/delete), `work-detail-page` (patch/archive), `settings-setup-section` (reset); `use-delete-entity-dialog`, `use-agent-picker-dialog`; `pages/dialogs/*` | Batch 2 done (`ConfirmDialog`, `FormDialog`) | done Batch 2 |
| Timeline / time activity | `global-timeline-page`, `timeline-page`, `work-timeline-page`, `worlds-page` (overview) | React Flow canvases stay app-side; lightweight semantic-free activity/event list is a possible future pattern — not scheduled | rewrite at rebuild |
| Inspector / detail side panels | `findings-page`, `memory-page`, `modules-page`, `work-inspector-page`, `work-detail-page` | Batch 3 deferred (`ListDetailShell`) | rebuild IA work |
| Settings sections | `settings-shell-layout`, `settings-section-frame`, 7 `settings-*-section.tsx` | Batch 3 deferred (`SettingsSection`/`SettingsLayout`) | rebuild IA work |
| Read-only detail / report panels | `preset-profile-page`, `session-run-view`, `work-inspector-page`, `world-findings-page` sections | Batch 3 deferred (read-only detail parts); no universal JSON renderer | rebuild IA work |
| Canvas navigation / fallback | `outline-page`, `timeline-page`, `work-timeline-page`, `world-kb-page`, `strategy-page`; `components/canvas/`, `layout/canvas-nav.ts` | keep app-side; share presentational chrome only | rewrite at rebuild |

Cross-cutting constraints recorded by the survey: patterns must not import `api/queries.ts` / `NexusClient` / web i18n config into `packages/nexus-ui`; pages own query lifecycle, URL state, filtering semantics, and copy.

## 5. Named composite blocks (user-requested verdicts)

| Block | Verdict | Evidence |
|---|---|---|
| **Session list item** | **rewrite at rebuild** — no library promotion, not Batch 3 | Inline JSX in `apps/web/src/pages/sessions-page.tsx:80-101`; single use; coupled to session fields, `StatusBadge`, `shortId`, i18n, and `/sessions/:sessionId` routing; no selection/bulk actions. No `SessionRow` export exists in `@42ch/nexus-ui`. |
| **Works cards** | **Batch 3 candidate** — evaluate real cross-page reuse demand during rebuild IA before promoting | App-local `HubEntityCard` in `apps/web/src/components/layout/presentational/hub-card-list-pane.tsx:110-152`; near-pure presentational ({id, label}, icon, onSelect) and already extracted; currently serves only Creator Hub Work/World lists (`creator-hub-dual-pane.tsx` supplies queries/navigation). |
| **Finding entries** | **rewrite at rebuild** — three distinct implementations, not one row shape; do NOT merge into a shared library component merely because they are all "findings" | (1) Work findings table row, inline in `apps/web/src/pages/findings-page.tsx:323-374` — selection/checkbox, bulk update, quick-assign mutation; (2) local expandable read-only `WorldFindingRow` in `apps/web/src/components/worlds/world-findings/world-findings-panel.tsx:91-147` — World-specific severity badge, expand-only interaction; (3) separate summary list in `apps/web/src/pages/work-detail-page.tsx:190-229` — read-only fields + pagination. Different fields, interactions, and write boundaries. |

Siblings (per CompositeBlocksSurvey; all keep app-side / rewrite at rebuild, no promotion on current evidence):

| Sibling | Evidence |
|---|---|
| Module rows | `modules-page.tsx:136-171` — selectable item driving detail panel; module business fields + selection state |
| Preset rows | `strategies-page.tsx:263-269` (catalog) and `:397-435` (group rows) — validate/reload/delete business actions |
| Schedule rows | `schedule-page.tsx:81-85` — entity table row, distinct columns/status from sessions |

## 6. Deliberate non-goals

- No visual redesign: no new tokens, colors, spacing, or layout decisions. `DESIGN.md` / `DESIGN.dark.md` (v0.5.0) and the `@nexus/design-tokens` pipeline remain the sole token source; light/dark parity is generator-enforced.
- No adoption work in this round: adoption of library patterns (Batch 3 items and rewrites) happens inside the web rebuild, not here.
- No deletion in this round: cleanup candidates live in [web-rebuild-cleanup-list.md](2026-10-22-web-rebuild-cleanup-list-historical-notes.md) as a list only.
- DESIGN.md's current boundary (no new public primitive families beyond shipped batches) is respected; any Batch 3 promotion revisits that boundary at rebuild time.
