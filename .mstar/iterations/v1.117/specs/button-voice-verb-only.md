# Button voice Verb-only (V1.117 P4)

> Iteration-scoped product brief for V1.117 P4. Normative voice change.
> Architect locked (§5.2); spec frozen after writing (§5.3).

| Attribute | Value |
| --- | --- |
| **plan_id** | `2026-07-14-v1.117-button-voice-verb-only` |
| **Tier** | Must (P4) |
| **Status** | Spec frozen (§5.3) |
| **Audience** | All authors (every button/CTA surface) |
| **primary plan** | `.mstar/plans/2026-07-14-v1.117-button-voice-verb-only.md` |

## Problem framing

`DESIGN.md` §Voice & Content currently requires **Verb + Noun** for actions
(`Create Work`, `Validate Preset`, `保存智能体`). Authors report button copy
feels **bloated** — the noun is often obvious from page context. Grill-me #8
locks **Verb-only** for buttons/CTAs while leaving titles, nav, helpers, and
toasts unchanged.

## User value

| Who | Why they care |
| --- | --- |
| **Authors** | Shorter, calmer buttons (`Save`, `Create`, `保存`, `创建`) — less visual noise. |
| **Maintainers** | Single normative rule in DESIGN.md + AGENTS.md + design-studio Voice page. |
| **Translators** | Clear boundary: sweep **button labels** only, not every string. |

## Goals

1. **Normative change** — Buttons and CTAs use **Verb-only** in en + zh-CN.
2. Update SSOT docs:
   - Root `DESIGN.md` §Voice & Content
   - `apps/web/AGENTS.md` Voice & Content bullet
   - `apps/design-studio/src/pages/voice.tsx` (and mirrored examples)
3. **Locale sweep** — keys classified per AD-P4-1; en + zh-CN catalogs updated
   for **button labels** only (not page titles, nav, dialog titles, helpers, toasts).
4. **Destructive/ambiguous actions** — Verb-only on the visible button; use
   surrounding copy or `aria-label` when screen readers need the object
   (`Delete` + dialog title names the object).
5. Spot-fix **hardcoded** button strings in components if sweep misses them.

## Non-goals

- Page titles, nav labels, dialog titles, helpers, toasts
- Title Case rule changes for titles/nav
- Mass rewrite of historical iteration specs mentioning Verb+Noun
- Changing non-button link text

## Voice boundary (locked)

| Surface | Rule | Example |
| --- | --- | --- |
| **Button / CTA** | Verb-only | `Save`, `Create`, `验证` |
| **Page title** | Title Case, may include noun | `Create Work` page title OK |
| **Nav item** | Title Case | `Works`, `Settings` |
| **Dialog title** | Names the action + object | `Delete Work` |
| **Helper / toast** | Sentence case, may name object | `Work saved.` |

## Before / after (product examples)

| Context | Before (Verb+Noun) | After (Verb-only) |
| --- | --- | --- |
| Agent settings | `保存智能体` / `Save Agent` | `保存` / `Save` |
| Setup verify | Already `验证` / `Verify` (V1.116) | No change |
| Create dialog submit | `Create Profile` (button) | `Create` |
| Destructive | `Delete Work` (button) | `Delete` (dialog title keeps object) |

## Target state

- DESIGN.md and AGENTS.md agree: buttons = Verb-only.
- design-studio Voice page demonstrates the rule for authors and implementers.
- en + zh-CN button locales swept; no systematic Verb+Noun on `<Button>` labels.

## Acceptance criteria (author-observable)

| ID | Criterion | How to verify |
| --- | --- | --- |
| **AC-P4-1** | DESIGN.md §Voice says Verb-only for buttons | Read DESIGN.md — buttons/CTAs rule updated |
| **AC-P4-2** | `apps/web/AGENTS.md` matches | AGENTS Voice bullet says Verb-only for buttons |
| **AC-P4-3** | design-studio Voice page updated | Open Voice gallery → shows Verb-only button examples |
| **AC-P4-4** | Locale button keys swept (en + zh-CN) | Spot-check Setup, Settings, common dialogs — buttons are single verbs |
| **AC-P4-5** | Titles/nav/helpers unchanged | Page titles may still use Verb+Noun where appropriate |
| **AC-P4-6** | Destructive buttons have context | Delete button is `Delete`; dialog title names target |
| **AC-P4-7** | Tests/fixtures updated if assertions break | `pnpm --filter web test` passes |

## Architect decisions (§5.2 — locked)

### AD-P4-1: Locale key classification

**In scope for verb-only sweep** when the string renders inside `<Button>`,
`@42ch/nexus-ui` `Button`, or dialog primary/secondary actions:

| Pattern | Example | Action |
| --- | --- | --- |
| Suffix `.button`, `.submit`, `.cta`, `.confirm` | `settings.agent.save` → split to `settings.agent.save.button` | Shorten value to verb only |
| `common.actions.*` | `common.actions.save` | Already verb-only — verify usage is button-only |
| Destructive | `common.actions.delete` | Verb on button; object in dialog title |

**Out of scope** (leave unchanged even if Verb+Noun):

| Pattern | Example |
| --- | --- |
| `*.title`, `*.heading`, `*.label` (non-button) | `reading.bodyTitle`, page headers |
| `shell.nav.*`, `settings.nav.*` | Nav items |
| `*.description`, `*.helper`, `*.toast` | Helpers and toasts |
| `aria.*` when redundant with visible verb button | Keep full phrase when aria is sole object carrier |

**Shared key split rule:** when one key feeds both dialog title and submit button,
**split** into `.title` + `.button` in the same namespace (do not overload one key).
Example: `settings.agent.save` (button) stays separate from any page title key.

Known sweep targets (non-exhaustive): `settings.json` `workspace.save` /
`agent.save` → verb only; grep for `Save Agent`, `保存智能体`, `Create Profile`.

### AD-P4-2: `@42ch/nexus-ui` boundary

Package primitives remain **caller-owned copy** — no string changes inside
`packages/nexus-ui` in V1.117. Host passes shortened `t()` strings.

### AD-P4-3: Residual / grep policy

| Finding | Action |
| --- | --- |
| Hardcoded button string in touched component | Fix in-plan (T3) |
| Straggler in low-traffic surface found by grep | Register iteration residual (`R-V117P4-VERB-*`) if ≤5 instances and not author-critical |
| >5 stragglers or Setup/Settings/Create flows | Must fix in-plan before QC |

Suggested grep: `(Save|Create|Delete|Add|Remove) [A-Z]` in `apps/web/src` excluding `*.test.*`.

## Key files (expected)

- `DESIGN.md`
- `apps/web/AGENTS.md`
- `apps/design-studio/src/pages/voice.tsx`
- `apps/web/src/locales/{en,zh-CN}/*.json` (button keys only)
