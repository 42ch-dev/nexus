# Settings Setup Re-run (V1.103 P3)

**Status:** architect-locked (§5.2); writing-polished (§5.3)  
**Plan:** `2026-07-09-v1.103-settings-rerun-setup`  
**Shell:** [`settings-shell-ia.md`](settings-shell-ia.md)  
**Compass:** [`v1.103-delivery-compass.md`](../../v1.103-delivery-compass.md)  
**Tier:** Must (P3)  
**Wire:** `wire_contracts_changed: false`

## Goal

Expose **Re-run setup** from Settings — fulfilling V1.94's deferred "Re-run setup" action (clears `setup_completed` marker). Interaction **R1**: confirm → clear `setup_completed` → navigate to `/setup` immediately.

> V1.94 risk register: Settings exposes a "Re-run setup" action that clears the marker. V1.102 deferred the UI; this spec delivers it.

## Author-facing outcome

Settings → **Setup** → select **Re-run Setup** → confirm dialog explains the wizard will restart → **Re-run Setup** opens `/setup`. Workspace path and agent profile files are **not** deleted. **Cancel** returns to Setup with no change.

## Architecture locks (implementer SSOT)

### Route & module

| Lock | Value |
|------|-------|
| Route | `/settings/setup` |
| Module | `apps/web/src/pages/settings/settings-setup-section.tsx` |

### R1 action sequence (Confirm)

Execute in order:

1. `await desktop.setSetupCompleted(false)` → Tauri **`set_setup_completed`** with `{ value: false }`.
2. Update **`SetupCompletedContext`** to `completed: false` **before** navigation (extend provider — see below).
3. `navigate('/setup', { replace: true })`.

**Cancel:** close dialog; **no** IPC; **no** context change; remain on `/settings/setup`.

### SetupCompletedContext extension (required)

Current `SetupCompletedProvider` only exposes `markCompleted()` (sets `true`). Re-run requires syncing React state when clearing the marker:

| Lock | Value |
|------|-------|
| New API | `setCompleted(value: boolean)` on context — updates state and calls `desktop.setSetupCompleted(value)` when desktop is available |
| Re-run uses | `setCompleted(false)` after successful IPC (or combine IPC inside `setCompleted`) |
| Wizard finish | Existing `markCompleted()` may delegate to `setCompleted(true)` |

Without step 2, an author who cancels the wizard and navigates back to gated routes could briefly see main UI with stale `completed: true` in memory while disk reads `false`.

### SetupGate interaction

| Fact | Implication |
|------|-------------|
| `/setup` route is **outside** `SetupGate` in `App.tsx` | Navigation to wizard always succeeds after marker clear |
| Gated routes (`/works`, `/settings`, …) check `useSetupCompleted()` | After re-run confirm, context `false` → `SetupGate` redirects to `/setup` if author hits a gated URL before finishing wizard |
| `SetupGate` does **not** wrap Settings re-run button | Re-run is initiated from inside gated UI while `completed` is still `true`; only Confirm flips marker |

### Data wipe — forbidden

Do **not** invoke as part of Re-run setup:

- `resetLocalDatabase`
- `setWorkspacePath` / workspace file deletion
- `setAgentProfile` clears or agent-host config deletion
- Any daemon DB wipe helpers

R1 clears the **`setup_completed` marker only**.

### Browser

`useDesktopCapabilities()` is `null`: show honest desktop-only copy; disable or explain Re-run CTA — **do not** invent HTTP setup-marker API.

## Author-facing copy (DESIGN Voice)

Destructive-adjacent tone — warn without implying data loss.

| Surface | Copy (locked) |
|---------|---------------|
| Section title (in-body, if shown beyond shell) | **Setup** |
| Section helper | Return to the first-run wizard to walk through setup steps again. Your workspace and agent choices are kept. |
| Primary CTA (opens dialog) | **Re-run Setup** |
| Confirm dialog title | **Re-run Setup?** |
| Confirm dialog body | This restarts the setup wizard from the beginning. Your workspace path and agent profile are not deleted. |
| Confirm dialog primary | **Re-run Setup** |
| Confirm dialog secondary | **Cancel** |
| Browser-only helper | Re-run setup is available on the desktop app only. |
| Browser CTA disabled tooltip (optional) | Open the Nexus desktop app to re-run setup. |

## In scope

1. Setup section explanatory copy + primary **Re-run Setup** CTA.
2. Confirm dialog (destructive-adjacent wording; Title Case primary CTA per DESIGN Voice).
3. `setCompleted(false)` + navigation per sequence above.
4. `SetupCompletedProvider` extension.
5. Vitest: confirm → `setSetupCompleted(false)` + context + navigation; cancel unchanged.

## Out of scope

- Reset local database / migration recovery (existing daemon error paths).
- Changing wizard step order.
- Auto-wiping agent or workspace on re-run.

## Acceptance (author-visible)

- Setup section shows **Re-run Setup** CTA with confirm dialog (destructive-adjacent copy; Title Case primary CTA per DESIGN Voice).
- **Re-run Setup** (confirm) clears `setup_completed` and navigates to `/setup` immediately.
- **Cancel** leaves `setup_completed` unchanged; author stays on Setup section.
- After confirm, workspace path and agent profile remain available (no silent file deletion).
- No `schemas/` change.
