# P1 Spec — Workspace profile + path

**Status:** Draft (Phase 1 — product §5.1, architect §5.2, writing §5.3 locked)  
**Document class:** Draft overlay  
**Iteration compass:** [delivery-compass.md](../delivery-compass.md)  
**Depends on:** P0 Continue unblock (same wizard step)

## Problem statement

1. Profile display name should default to **`default`** (system default / path slug) — today wizard init is `''`, which **disables Continue** until the author types a name (contributes to F7 confusion).
2. Focused Profile name Input overlaps the Workspace folder label (focus ring / scroll position).
3. When folder is not picked, changing Profile name should update the path’s last segment; after **Change…**, name changes must not alter path.

## Target users

| Persona | Scenario |
| --- | --- |
| New author | Accepts defaults — name `default`, path `~/Documents/nexus/default`, Continue works |
| Author customizing name | Types `alice` — path last segment becomes `alice` without Browse |
| Author who picked a folder | Browse to custom location — renaming Profile does not move folder |

## User stories

1. **As a new author**, I open Workspace and see Profile name **`default`** already filled so Continue is immediately available.
2. **As an author**, when I focus Profile name, the Workspace folder label stays fully visible (not covered by the focus ring).
3. **As an author**, typing a Profile name updates the last folder segment until I use **Change…** to pick a folder.

## Product rules (normative)

| Rule | Behavior |
| --- | --- |
| Default name | [`setup-wizard-page.tsx`](../../../../apps/web/src/pages/setup-wizard-page.tsx) initializes `profileDisplayName: 'default'` |
| Placeholder | Keep i18n placeholder as example copy (`My Profile` / localized equivalent) — **not** the field value |
| Path sync | If `!workspacePicked`, last path segment = filesystem-safe slug of trimmed name |
| Empty name | Trimmed empty name slug → `default`; Continue remains valid with value `default` |
| Path freeze | If `workspacePicked === true`, path immutable on name edits |
| Focus / layout | Focused Profile Input must not obscure “Workspace folder” label — use scroll-margin, field order, or spacing so label + path row remain visible without scrolling on default viewport |
| Browser mode | Path sync applies to displayed path string; Browse disabled (unchanged) |

## Slug rule (locked)

Apply to **path segment only**; **display name stays as typed**.

1. Trim leading/trailing whitespace.
2. Replace internal runs of whitespace with `-`.
3. Remove characters illegal on the host FS for a single path segment: `/`, `\`, `\0`, and platform-reserved names (Windows `CON`, `PRN`, … — architect lists in plan).
4. **Unicode:** preserve letters and numbers from any script (CJK, Latin, etc.) after NFKC normalize; do **not** romanize CJK for the path segment.
5. Collapse repeated `-`; strip leading/trailing `-`.
6. If result is empty → `default`.
7. Replace only the **last** segment of `workspaceRoot`; preserve parent path (`~/Documents/nexus/` or desktop-resolved prefix).

**Examples:**

| Display name | Last segment |
| --- | --- |
| `default` | `default` |
| `Alice` | `Alice` |
| `我的空间` | `我的空间` (after illegal-char strip) |
| `  foo  bar  ` | `foo-bar` |
| `///` | `default` |

## Scope boundary

| In scope | Out of scope |
| --- | --- |
| [`setup-wizard-page.tsx`](../../../../apps/web/src/pages/setup-wizard-page.tsx) init | AgentPicker (P2) |
| [`setup-step-workspace.tsx`](../../../../apps/web/src/pages/setup-step-workspace.tsx) sync + layout | Continue bootstrap root cause (P0) except happy-path with defaults |
| Shared slug helper (unit-tested) | Settings workspace path editor redesign |
| i18n placeholder alignment | Multi-profile rename in Settings |

## Acceptance criteria

| ID | Criterion | Verification |
| --- | --- | --- |
| AC-P1-1 | Name field defaults to `default` | Open Workspace step — no typing required |
| AC-P1-2 | Focus does not cover folder label | Focus Input; label fully visible at 480px card width |
| AC-P1-3 | Unpicked: name → path last segment | Type `alice` → `.../nexus/alice`; CJK name → matching segment |
| AC-P1-4 | Picked: name change leaves path | Browse then rename |
| AC-P1-5 | Continue enabled on first paint (desktop) | Default name + resolved path; no disabled Continue for empty name |

## Architecture contract (normative — architect locked)

### Slug helper

File: `apps/web/src/lib/workspace-profile-slug.ts` — export `slugProfileSegment(displayName: string): string`

| Step | Rule |
| --- | --- |
| 1 | Trim whitespace |
| 2 | NFKC normalize |
| 3 | Replace internal whitespace runs with `-` |
| 4 | Remove illegal segment chars: `/ \ : * ? " < > \|` and `\0` |
| 5 | Collapse repeated `-`; strip leading/trailing `-` |
| 6 | If segment matches Windows reserved name (case-insensitive exact): `CON, PRN, AUX, NUL, COM1–COM9, LPT1–LPT9` → append `-profile` and re-trim |
| 7 | If empty → `default` |

Display name field keeps author typing; slug applies to **path last segment only**.

### Path sync

| Trigger | Behavior |
| --- | --- |
| Wizard init | `profileDisplayName: 'default'` in `setup-wizard-page.tsx` |
| Mount (desktop) | After `getWorkspaceRoot`, if `!workspacePicked` and `basename(workspaceRoot) !== slug(profileDisplayName)`, rewrite **displayed** path last segment once (no IPC persist) |
| Name `onChange` | When `!workspacePicked`, replace last segment with `slug(newName)` |
| After Browse | `workspacePicked: true` — name edits do not alter path |

### Focus / layout

Profile Input receives `scroll-margin-top` (token or `1rem`) so focused ring does not cover “Workspace folder” label at **480px** card width.

### Wire contracts

**`wire_contracts_changed: false`**

## Open questions (architect)

~~All resolved in § Architecture contract.~~

1. ~~Windows reserved names~~ → blocklist + `-profile` suffix.
2. ~~Mount re-slug~~ → one-time reconcile when basename ≠ slug and unpicked.
