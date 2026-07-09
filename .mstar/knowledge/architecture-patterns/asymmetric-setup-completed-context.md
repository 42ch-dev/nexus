# Asymmetric Setup-Completed Context (Optimistic True / Await False)

**Status:** Active  
**Tags:** setup-gate, react-context, tauri-ipc, race-condition, settings-rerun  
**Source:** V1.103 P3 Settings Re-run Setup (QC2/QC3 F-001)

## Problem

A shared React context mirrors a durable desktop marker (`setup_completed`) used by `SetupGate` to redirect unfinished authors to `/setup`. Two call sites need opposite timing:

| Call site | Desired behavior |
|-----------|------------------|
| Wizard **Finish** | Navigate into gated routes immediately; marker should already read `true` |
| Settings **Re-run Setup** | Clear marker, then navigate to `/setup`; gated UI must not see stale `true` |

A single `await IPC → then setState` implementation is **unsafe for Finish**: fire-and-forget `markCompleted()` + navigate can paint gated routes while context is still `false` → SetupGate bounces back to `/setup`.

A single optimistic-always implementation is **unsafe for Re-run**: navigating before IPC clear can leave gated routes believing setup is still complete.

## Pattern

Expose one API `setCompleted(next: boolean)` with **asymmetric** semantics:

1. **`setCompleted(true)` (optimistic):** update React state **synchronously**, then await IPC. On IPC failure, roll back state and surface an error toast.
2. **`setCompleted(false)` (await-then-clear):** await IPC **first**, then clear React state. Callers that navigate (Re-run) must `await setCompleted(false)` before `navigate('/setup')`.
3. Keep a thin `markCompleted()` as `void setCompleted(true)` for wizard Finish (fire-and-forget remains OK because state flips before paint).

## Tests that lock the contract

- Finish reaches a gated route while `setSetupCompleted(true)` IPC is still pending.
- `setCompleted(false)` does not clear React state until IPC resolves.
- Re-run IPC failure: error toast, dialog stays open, no navigate.

## Anti-patterns

- Unifying both directions behind one “always await then setState” helper.
- Navigating on Re-run before the clear IPC succeeds.
- Silent IPC failure on destructive/confirm flows (authors need a toast).

## Related

- Iteration specs: `.mstar/iterations/v1.103/specs/settings-setup-section.md`
- Broader Settings IA: `.mstar/iterations/v1.103/specs/settings-shell-ia.md`
- UI studio-first process: [ui-component-promotion-workflow.md](./ui-component-promotion-workflow.md)
