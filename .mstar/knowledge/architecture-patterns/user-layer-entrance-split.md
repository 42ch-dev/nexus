---
module: apps/web + apps/desktop
date: 2026-08-21
problem_type: architecture_pattern
category: architecture-patterns
severity: high
plan_id: 2026-08-20-v1.170-p1-entrance-split
applies_when: [adding a usage-mode axis that filters shell surfaces, adding a new shell surface that renders nav or settings, adding a third first-party client, wiring a persisted client preference across web and desktop]
tags: [entrance-axis, layout-tree, shell-surface, route-guard, settings-modal, mobile-nav, setup-wizard, tauri-ipc]
---

# User-Layer Entrance Split (Orthogonal Usage Axis)

The pattern for splitting the first-party SPA into usage-mode layout trees (**Create** / **Develop**) on a **user-layer entrance axis** that is orthogonal to the agent-layer identity, and — the hard-won part — making every shell surface consume the axis.

## Context

V1.170 P1 split the single Control Room SPA into two trees: **Create** (a reduced tree hiding operator chrome) and **Develop** (full config + Develop hub v1), selected by an entrance identity (`developer` | `content-creator`). The axis is deliberately **not** the Creator profile: Creator is the agent-layer identity aggregate (memory, world ownership, presets); entrance is the human's product-usage mode. v1 keeps entrance as web/desktop persisted client state only — the daemon never sees it (no User entity: a third first-party surface must not inherit a first-party persona).

## Guidance

### 1. Keep the axes orthogonal — in code and in copy

- Switching entrance must never create, swap, or hide Creator profiles; the profile switcher stays available in both trees.
- `developer` / `content-creator` are the only entrance ids. **Never** `creator` as an entrance id (collides with the Creator entity). Chrome labels are layout names: **Create** / **Develop**.

### 2. One route table + typed registry + guard

- The single `<Routes>` table stays; no second tree, no route removal. Two layout trees = one typed registry driving entrance-filtered nav groups, land routes, guard classification, and hidden settings sections.
- `ENTRANCE_DESCRIPTORS` / `ENTRANCE_ROUTE_RULES` (modeled on the `SETTINGS_SECTION_DESCRIPTORS` pattern: readonly descriptor array + derived index maps) is the classification SSOT: `visibility: 'both' | 'develop-only'`, `allowDeepLink` (support deep-links pass through on the hidden tree), `settingsSection`, `landRoute`.
- The guard sits inside the existing gate stack (`SetupGate > EntranceGuard > RootLayout`); mismatch on Create → `<Navigate replace>` to `landRoute` + one-shot bounce toast; Develop never bounces.
- The registry is a **separate axis from the Creator|Orchestrator tabs** — tab highlighting stays untouched.

### 3. Enforcement-path rule (the lesson this doc exists for)

An axis that changes what is visible must be consumed by **every shell surface that renders visibility** — not just the route guard. In V1.170 P1 QC tri, the axis was wired at the route-guard + desktop-sidebar layer only, and **each** unwired surface became a Warning:

| Surface | Failure mode when unwired | Fix pattern |
| --- | --- | --- |
| Settings modal rail | `hiddenSettingsSections` declared but unconsumed; all sections rendered unconditionally | Host passes the entrance's hidden sections into the frame; rail filtered |
| Titlebar gear default section | Gear opened the develop-only `agent` section on Create → guard bounced the whole app | `firstSettingsSectionFor(entrance)` — open the first visible section |
| Mobile nav (`<lg`) | Static `MOBILE_NAV_KEYS` rendered unconditionally — operator chrome leaked on Create | Derive visibility from the same guard classification (`resolveEntranceBounce`) |
| Setup wizard re-run | `WizardState.entrance` seeded `DEFAULT_ENTRANCE`, not the stored value — an untouched Continue silently overwrote a stored `developer` | Seed from the resolved entrance + `entranceTouched` ref so a deliberate re-pick is never overwritten; `finish()` preserves the stored value on untouched re-run |

**Rule:** route-guard alone is insufficient. When adding a shell surface that renders nav, settings, or re-entry flows, wire it to the axis classification in the same change — a surface that renders unconditionally is a future Warning.

### 4. Persistence seams mirror `setup_completed`

- Browser: `localStorage` key (`nexus-entrance`), key convention from the existing locale/active-creator keys.
- Desktop: Tauri IPC pair (`get_entrance` / `set_entrance`) shaped exactly like the `setup_completed` pair — same `~/.nexus42/config.toml` file and durability class; screens depend on the `DesktopCapabilities` interface, never `window.__TAURI__`.
- Provider semantics: desktop reads via IPC on mount, fail-open → default on command error; browser reads synchronously; **no optimistic write** (a landing-layout switch must not flash the wrong tree); unset resolves default **without writing** storage until the user confirms; a stored-but-unparseable value resolves default.
- The daemon never sees the axis — no `state.db` column, no HTTP header.

### 5. Re-entry semantics

- Wizard re-run re-offers the stored entrance as the pre-highlighted default (§3 fix); returning installs skip the identity page.
- URL override (`?entrance=`) is session-only: precedence URL > stored > default, applied in memory, **not** persisted unless the user confirms on the identity page.
- Index redirect is entrance-aware (`/` → `landRoute`), and `ENTRANCE_DESCRIPTORS[].landRoute` is the single source for guard bounces and the index redirect.

## Why This Matters

- Partial axis wiring produces user-visible bounce bugs (gear → whole-app redirect) and silent persisted-state overwrites (re-run flips a developer to content-creator) — each un-wired surface is a guaranteed QC Warning, and the shared root cause costs one full fix wave.
- The axis is the product's core navigation contract now; the registry SSOT is what makes the two trees maintainable instead of two diverging copies of the route table.
- Orthogonality protects the agent-layer identity: entangling entrance with Creator profiles would corrupt the daemon-side identity model to serve a client-side preference.

## When to Apply

- Adding any new usage-mode axis or a third first-party client (revisit then: is the axis still client-local, or does it need a shared identity?).
- Adding a shell surface that renders nav groups, settings sections, or wizard steps.
- Extending the hidden-surface table (add a rule + nav-group entry + settings-section entry together).

## Examples

See §3 table for the four enforcement-path failures and fixes. Structural anchors: `apps/web/src/components/layout/entrance-registry.ts` (registry SSOT), `apps/web/src/lib/entrance-context.tsx` (provider), `apps/web/src/App.tsx` (guard + index redirect), `apps/desktop/src-tauri/src/lib.rs` (`get_entrance`/`set_entrance` + `is_entrance_value` validation).

## References

- Concept: `CONCEPTS.md` § Entrance (canonical definition — this doc does not redefine it)
- Settings host pattern: [settings-modal-primary-host.md](settings-modal-primary-host.md)
- `setup_completed` asymmetry: [asymmetric-setup-completed-context.md](asymmetric-setup-completed-context.md)
- Shell IA: [workspace-parent-shell-ia.md](workspace-parent-shell-ia.md)
- Specs: `.mstar/specs/web-ui.md`, `.mstar/specs/desktop-shell.md`; iteration spec `.mstar/iterations/v1.170/specs/v1.170-entrance-locks.md` EL-1..EL-8 + AR-15..AR-22
