# Web UI — Design Requirements (input brief for `apps/web/DESIGN.md`)

**Status**: Input brief (Prepare Phase 2b) — **not** the DESIGN.md itself  
**Author**: `@product-manager`  
**Consumer**: `@architect` (authors `apps/web/DESIGN.md`, the design-token SSOT; completeness level **Standard** per compass §5 item #6)  
**Iteration**: V1.64 (V1.65 authoring-surface amendment appended in §5)  
**Drives**: [web-ui.md](web-ui.md) §6 (MVP surface) — the screens whose look/feel this brief constrains

> This document captures the **product and design intent** the design system must serve. It deliberately does **not** specify token values (colors, type scale, spacing units) — those are `@architect`'s job in `apps/web/DESIGN.md`. It states *what the UI must feel like and for whom*, so the token system can be derived rather than guessed.

---

## 1. Information density per screen group

The MVP is two registers with very different density profiles. The token system must serve both without one starved or the other overwhelming.

| Register | Screen groups | Density profile | Design implication |
| --- | --- | --- | --- |
| **Control Room (dashboards)** | Works dashboard, orchestration sessions, schedule/cron, capability registry, findings | **Data-dense** — tables / lists with status, severity, timestamps, pagination (cursor). An author may have many Works and many findings. | Strong tabular primitives: row hierarchy, status badges, severity chips, comfortable scan-ability, pagination controls, empty/loading/error states for TanStack Query. Dense without being cramped. |
| **Setup (forms)** | Work CRUD, preset management CRUD (incl. **validate** dry-run) | **Form-dense** — structured inputs, validation feedback, JSON-ish preset fields, create/patch/delete confirmations. | Strong form primitives: labeled inputs, inline validation, destructive-action confirmation, clear primary/secondary action hierarchy. The **validate** action must read as the trustworthy "is this safe?" button. |

Common to both: every list needs first-class **loading / empty / error** states (errors parsed from the shared `ErrorResponse`, one shape — see [web-ui.md](web-ui.md) §4.2). These are not afterthoughts; they are where non-terminal authors either recover or give up.

## 2. Author persona — calm and focused, not engineer-facing

The primary user is a **writer, not an engineer**. They chose Nexus to write, not to operate infrastructure. The aesthetic must therefore be:

- **Calm and focused** — generous whitespace, low visual noise, no dashboard-anxiety (no aggressive reds/greens competing for attention at rest). Status and severity use *meaningful* color, never decorative color.
- **Legible over clever** — plain language labels that mirror the CLI copy authors already see (brand-voice consistency, §3). Avoid protocol jargon (no "ACP", "orchestration graph", "cursor token" in the UI surface).
- **Trust-building** — destructive actions (delete preset, archive Work) confirm clearly; the **validate** dry-run is presented as a reassurance affordance, not a debug tool.
- **Readable at length** — authors stare at Works/findings lists; type and contrast must hold up over long sessions, not just look good in a screenshot.

## 3. Brand voice and content

The UI copy must be **consistent with the existing CLI voice** (see [cli-spec.md](cli-spec.md) §7.1 UX principles — "本地助手" framing, actionable next-steps over raw error text). Concretely:

- **Tone**: helpful, plain, action-oriented. Errors give a one-line next step, never just a code.
- **Vocabulary**: reuse CLI terms authors already know (Work, preset, stage, finding, capability). Do **not** invent UI-only synonyms.
- **Consistency**: the same concept is labeled identically everywhere (CLI, UI, docs). If the CLI says "archive", the UI says "archive", not "remove".
- **Voice tokens belong in DESIGN.md** as a Voice & Content section; this brief only fixes *which* voice and *what constraints*.

## 4. Accessibility bar

This is a local tool for authors, some of whom rely on assistive tech. The bar is **WCAG 2.1 AA** as the floor, not the ceiling:

- **Keyboard navigation** — every screen group and every CRUD action (including preset **validate** and destructive confirmations) is fully operable from the keyboard; visible focus rings are part of the token system, never removed.
- **Screen-reader semantics** — tables/lists use correct roles; status badges and severity chips expose their meaning textually (not color alone); live regions for async state changes where useful.
- **Contrast** — AA contrast minimums in **both** light and dark themes (dark is first-class, not an afterthought — authors write at night).
- **Motion** — respect `prefers-reduced-motion`; transitions are subtle and purposeful, never decorative.
- **Target sizes** — comfortable tap/click targets; this UI is Tauri-bound (V1.65 desktop, V1.66+ mobile), so do not design for mouse-only.

Dark mode is **first-class**: the token system must define light **and** dark from day one (shared token names, different values — see `mstar-design-md` dual-theme rule), because the author persona writes across both.

---

## 5. V1.65 authoring surface — component design requirements

V1.65 adds the first **authoring-write** surfaces to the UI (see [web-ui.md](web-ui.md) §13). These introduce three new component classes whose look/feel the design system must serve. As in §1–§4, this section fixes *product intent and constraints* — token values remain `@architect`'s job in `apps/web/DESIGN.md` (a **Standard+ increment** this iteration; Production-level polish/animations stay V1.66 per compass §5 item #6/#7).

### 5.1 Rich-text outline editor

The editor is where an author plans a chapter's shape. It must feel like a calm writing surface, not a configuration form.

- **Toolbar scope (markdown subset)**: headings, lists, bold, italic, code, blockquote, link. This is the **boundary** of the supported markdown subset (compass §5 item #1); the design must make unsupported nodes visibly out-of-scope (e.g. preserved as raw blocks) rather than silently mangling them.
- **Save-state indicator**: a persistent, glanceable status — `clean` / `dirty` / `saving` / `saved-error`. The author must always know whether their outline is on disk. `saved-error` must surface the parsed `ErrorResponse` one-liner, not a raw failure.
- **Soft-concurrency warning surface**: when the chapter being edited is in `draft` or `finalized` status, show a persistent banner stating plainly that editing the outline will **not** re-draft the body, and guiding the author to the explicit next step: reverse-transition the chapter status to `outlined` (then advance to `draft`) via the structure table to trigger a re-draft. Suggested copy: *"This chapter already has a draft body. Editing the outline will not re-draft it — the orchestration engine re-drafts only when the chapter transitions to `draft` status. To trigger a re-draft after saving: reverse the chapter status to `outlined` in the structure table, then advance it back to `draft`."*
  - **Product priority: non-blocking but unmissable.** It must not block the save (the model is soft — orchestration takes a fresh snapshot), but it must not be dismissible-to-invisible either. Lean: a persistent banner/strip attached to the editor chrome, not a one-time toast.
- **A11y**: the editor is a writing surface — full keyboard operability, visible focus, and `prefers-reduced-motion`-respectful transitions (§4) apply at full strength.

### 5.2 Chapter structure data table

A data-dense table (same register as the Control Room dashboards in §1) with an inline-edit affordance layered on top.

- **Columns**: chapter #, title (**display-only** — derived from outline frontmatter or slug/chapter# fallback; no `title` DB column in V1.65), slug, planned word count, volume, status, actual word count. Status renders as a **meaningful badge** (not decorative color — exposes its text to assistive tech, §4).
- **Inline edit affordance**: slug / planned word count / volume are editable in place (`title` is display-only — title text is shaped in the outline editor); status progression is an explicit action (not free-form typing) so reverse transitions can be gated.
- **Multi-Work switcher reuse**: the per-Work table reuses the V1.64 Works dashboard entry as the Work selector; the design must make "which Work am I editing" unambiguous at all times.
- **Confirmation-dialog policy (protected chapters)**:
  - Structural edits on `finalized` / `published` chapters → **confirmation dialog** (warn, do not silently apply).
  - **Deletion → hard-block** (refuse, with a plain-language reason). There is no "confirm to delete" path; deletion is not offered for settled chapters.
- **Destructive-action visual language** for the confirmation path should match the V1.64 preset/Work destructive language (§2 trust) so the author recognises it across the app.

### 5.3 Body read-only context menu

- **"Copy path" only.** The body is rendered read-only (frontmatter-aware header strip + rendered prose). The right-click menu offers **Copy path** (browser clipboard write; path sourced from the API).
- **Explicitly out of scope (V1.66 Tauri)**: "Open with…" and "Reveal in file manager" are **native-shell** desktop-integration actions (compass §0 Q5), not browser capabilities. The V1.65 design must not imply them — no greyed-out "Open with…" entries that tease an unavailable action. When the Tauri shell lands in V1.66 these become real entries via `TauriClient`; the V1.65 browser menu simply does not contain them.

### 5.4 Light + dark theme parity (carried from V1.64)

All three V1.65 component classes — editor, table, context menu — must ship with **light and dark** token parity from day one (shared token names, different values — `mstar-design-md` dual-theme rule; §4). The author persona writes across both; the editor especially must hold up over long sessions in dark mode.

---

## What this brief deliberately does NOT decide

- Token values (colors, type scale, spacing, radii, elevation, motion durations) → `apps/web/DESIGN.md` (`@architect`).
- Component inventory beyond "strong tables + strong forms + status/severity primitives + loading/empty/error states" (V1.64) plus the V1.65 "editor + structure table + read-only context menu" increment (§5) → `apps/web/DESIGN.md`.
- Completeness level beyond **Standard** for V1.64 + **Standard+ increment** for V1.65 authoring components (Production-level polish/animations are V1.66, compass §5 item #6/#7).

## Open inputs `@architect` should resolve in DESIGN.md

1. The concrete status / severity color mapping (must stay meaningful, not decorative — §2).
2. Focus-ring and destructive-action visual language (§2 trust, §4 a11y).
3. The Voice & Content token section mirroring CLI copy (§3).
4. Light + dark dual-theme token tables sharing names (§4).
5. The V1.65 authoring component tokens — editor (toolbar, surface, save-state indicator, soft-concurrency banner), data-table (structure rows, status badges, inline edit, confirmation / hard-block dialog), context-menu (copy-path) — appended as a **Standard+ increment**, light + dark (§5).
6. The **non-blocking-but-unmissable** visual treatment for the outline-editor soft-concurrency warning (§5.1) — this is the highest product-priority design decision in the V1.65 increment.

---

*Input brief only. The authoritative design system is `apps/web/DESIGN.md`, owned by `@architect`. This brief exists so the design system is derived from product intent rather than aesthetics-in-a-vacuum.*

---

## 6. V1.66 desktop shell surface — component design requirements

V1.66 wraps the V1.65 SPA in a Tauri v2 desktop shell (see [web-ui.md](web-ui.md) §14; contract: [desktop-shell.md](desktop-shell.md)). The shell introduces surface classes whose look/feel the design system must serve. As in §1–§5, this section fixes *product intent and constraints* — token values remain `@architect`'s in `apps/web/DESIGN.md` (**Standard+ desktop supplement** this iteration; Production polish — custom title bar, animated transitions, system tray — stays V1.67+).

### 6.1 Window chrome

- **Native macOS chrome by default**: standard traffic-light region — do not invent custom window controls in V1.66 (custom title bar is V1.67+ Production polish).
- **Window title**: "Nexus" + active Work title when one is open.
- **Minimum/resizable size**: comfortable for both data-dense Control Room tables AND the authoring editor.
- **Light + dark parity**: chrome follows system appearance; matches SPA theme tokens in both.

### 6.2 App menu structure

Standard macOS app menu bar (not a hamburger). Minimal; aligned with V1.64/V1.65 screen groups. Disable unavailable items rather than hiding roadmap commands.

- **Nexus**: About Nexus (version/build/daemon status), Quit Nexus (also stops the sidecar).
- **File**: New Work… / New Preset… (disabled when not in an owning screen), Close Window.
- **Edit**: standard Cut/Copy/Paste/Select All (routed to focused surface, incl. outline editor).
- **View**: theme toggle (Light/Dark/System), reload, entry points to Control Room + Setup + Authoring groups.
- **Window**: standard macOS Window menu.
- **Help**: Nexus Help (docs link), Copy Diagnostics. No protocol jargon in labels.

### 6.3 Native dialogs (open / save / about)

- Native `NSOpenPanel`/`NSSavePanel` via Tauri's dialog plugin where the OS provides them — restricted by the same workspace-root scope whitelist as `openWith`.
- Native about panel (version + daemon status + port + docs link).
- Reuse V1.64/V1.65 destructive-action visual language so confirmations are recognisable across browser and desktop.

### 6.4 Desktop context menu (right-click actions)

- **Entries**: "Copy Path" (browser + desktop) → **"Open With…"** (desktop only, system MD-editor picker) → **"Reveal in Finder"** (desktop only). Order: Copy Path first, then desktop-only native actions.
- **Browser build does not tease**: when not in desktop mode, "Copy Path" only — **no greyed-out "Open With…"** advertising an unavailable action (carries V1.65 §5.3 rule forward).
- **Path-guard affordance**: if a path is outside the workspace-root scope, desktop entries are disabled with a plain-language reason (not a silent no-op).
- **A11y**: fully keyboard-operable; entries expose meaning textually, not icon-only.

### 6.5 Daemon-status indicator (highest product priority in V1.66)

The make-or-break affordance for the non-terminal author: the desktop app's whole promise is "the daemon comes up transparently." When it *doesn't*, the author must not stare at a frozen window.

- **States** (glanceable, persistent — not a toast): `starting` / `running` / `degraded` (restart-on-crash fired) / `stopped` / `error` (e.g., **port already in use** — the most likely real-world failure).
- **Error copy gives a one-line next step**: e.g., *"Nexus couldn't start its background service — port 8420 is already in use. Quit the other Nexus instance, or set a different port."* Never just a code.
- **Non-blocking but unmissable** — same product-priority class as the V1.65 outline-editor soft-concurrency banner (§5.1): persistent indicator (menu-bar icon + window-footer strip), not a vanishing notification.
- **Manual control**: from the indicator, restart/stop the daemon — mapped to `startDaemon` / `stopDaemon` — with confirmation on stop (interrupts running orchestration).

### 6.6 Light + dark theme parity

All V1.66 surface classes — window chrome, app menu, native dialogs, desktop context menu, daemon-status indicator — ship with **light + dark** token parity from day one (shared token names, different values). The daemon-status indicator especially must hold up in dark mode over long sessions.

---

### Open inputs `@architect` resolves in `apps/web/DESIGN.md` (V1.66 desktop supplement)

1. Window-chrome tokens (title bar, traffic-light safe-area, min/default window size) — light + dark.
2. App-menu typography + daemon-status indicator visual treatment (the non-blocking-but-unmissable error/degraded treatment is the highest product-priority design decision in the V1.66 increment).
3. Native-dialog mapping + destructive-dialog parity with V1.64/V1.65.
4. Desktop context-menu tokens (entry order, disabled/scope-violation state) — light + dark.
5. Daemon-status indicator state tokens (starting/running/degraded/stopped/error) — meaningful color, not decorative; exposing state textually.
